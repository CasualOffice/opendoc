// The table chrome layer, proven from the gesture end (`docs/141` D-1).
//
// The bar this file exists to hold is the owner's own sentence: put the pointer
// near a row boundary on a table the caret is NOT in, see a resize cursor, drag
// it, and watch the row resize. Every assertion below is on that guarantee rather
// than on a mechanism — "the row is taller in the laid-out document", not "a
// handle element exists" — because the trap for this work specifically is a guard
// that passes because the caret happened to be in the table already.
//
// So each test starts by moving the caret OUT of the table and asserting the
// contextual Table tab is disabled, which is the product's own statement that the
// caret is not in a table. If that precondition ever silently stops holding, the
// tests stop testing the thing they are named for.
//
// The observable for "the cursor says so" is `data-pointer-target` on the page
// canvas: `pointer_hover.mjs` writes both the inline cursor and the target id
// there, so reading it is reading the router's actual answer rather than a
// re-derivation of it.
//
// TBL-37 recorded that no test dragged a table boundary at all before this file.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  stableBox,
  setReviewMode,
} from "./fixtures.mjs";

/** Inserts a 2x2 table below a first paragraph, leaving the caret in the table. */
async function insertTable(page) {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type("Above the table.");
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await expect(page.locator("#insertTableMenu")).toBeVisible();
  await page.locator('.gc[data-r="2"][data-c="3"]').click();
  await expect(page.locator("#tabTable")).toBeEnabled();
}

/** Moves the caret to the paragraph ABOVE the table and proves it left.
 *
 *  `#tabTable` is a contextual tab enabled by `inTable` alone, so its disabled
 *  state is the product's own answer to "is the caret in a table" — which is
 *  exactly the precondition every test here depends on. */
async function leaveTable(page) {
  await moveCaretToDocStart(page);
  await expect(page.locator("#tabTable")).toBeDisabled();
}

/** The page-local rect of `selector`, or null. */
const local = (page, selector) =>
  page.evaluate((sel) => {
    const el = document.querySelector(sel);
    if (!el) return null;
    const r = el.getBoundingClientRect();
    const p = document.querySelector(".page-wrap .page").getBoundingClientRect();
    return { x: +(r.x - p.x).toFixed(1), y: +(r.y - p.y).toFixed(1), w: +r.width.toFixed(1), h: +r.height.toFixed(1) };
  }, selector);

/** What the hover router says is under the pointer right now. */
const pointerTarget = (page) =>
  page.evaluate(() => document.querySelector(".page-wrap .page")?.dataset.pointerTarget ?? "");

/** The computed cursor on the page canvas. */
const pageCursor = (page) =>
  page.evaluate(() => {
    const el = document.querySelector(".page-wrap .page");
    return el ? getComputedStyle(el).cursor : "";
  });

/**
 * Sweeps the pointer down (or across) the table looking for the boundary the
 * router reports as `target`, and returns where it found it in page-local px.
 *
 * Probing rather than computing the boundary is deliberate: the test must find
 * the zone the way a user does — by moving the pointer and watching the cursor —
 * so a zone that exists in the geometry but is never armed cannot pass.
 */
async function findBoundary(page, target, { x0, y0, x1, y1, step }) {
  const box = await stableBox(page.locator(".page-wrap .page").first());
  const horizontal = y1 === y0;
  for (let at = horizontal ? x0 : y0; at <= (horizontal ? x1 : y1); at += step) {
    const px = horizontal ? at : x0;
    const py = horizontal ? y0 : at;
    await page.mouse.move(box.x + px, box.y + py);
    // One animation frame: the router is rAF-throttled on purpose.
    await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => r())));
    if ((await pointerTarget(page)) === target) return { x: px, y: py };
  }
  return null;
}

test("a row boundary on a table the caret is not in resizes by drag", async ({
  page,
  consoleErrors,
}) => {
  await insertTable(page);

  // The row's laid-out height BEFORE, read from the active-cell outline — which
  // the engine paints from `cellRect` on the layout that is on screen. Then the
  // caret leaves, so nothing about the gesture below can be charged to it.
  await expect(page.locator(".overlay .cell-outline")).toHaveCount(1);
  const before = await local(page, ".overlay .cell-outline");
  expect(before).not.toBeNull();
  await leaveTable(page);

  const box = await stableBox(page.locator(".page-wrap .page").first());
  const found = await findBoundary(page, "table-row-boundary", {
    x0: before.x + before.w / 2,
    y0: before.y - 6,
    x1: before.x + before.w / 2,
    y1: before.y + before.h * 3,
    step: 2,
  });
  expect(
    found,
    "no point along the table's first column reported a row boundary — the zone " +
      "is not armed for a table the caret is not in, which is the whole of D-1",
  ).not.toBeNull();
  expect(await pageCursor(page)).toBe("row-resize");

  // Drag it down. The guide must appear (the gesture is live, not a cursor lie)
  // and the commit lands on release.
  await page.mouse.move(box.x + found.x, box.y + found.y);
  await page.mouse.down();
  await page.mouse.move(box.x + found.x, box.y + found.y + 48, { steps: 6 });
  await expect(page.locator(".overlay .table-row-resize-preview")).toHaveCount(1);
  await page.mouse.up();
  await expect(page.locator(".overlay .table-row-resize-preview")).toHaveCount(0);

  // THE GUARANTEE: the row is taller in the laid-out document. Measured by
  // putting the caret back in the same cell and reading its outline, which comes
  // from the painted layout — not from the chrome query under test.
  await page.mouse.click(box.x + before.x + before.w / 2, box.y + before.y + before.h / 2);
  await expect(page.locator("#tabTable")).toBeEnabled();
  await expect
    .poll(async () => (await local(page, ".overlay .cell-outline"))?.h ?? 0, {
      message: "the first row's laid-out height after the drag",
    })
    .toBeGreaterThan(before.h + 20);

  // …and the MODEL carries it as a minimum height, which is what makes the row
  // grow with content instead of clipping it.
  await page.locator("#tabTable").click();
  await page.locator("#tablePropertiesBtn").click();
  await expect(page.locator("#tableRowHeightRule")).toHaveValue("atLeast");
  expect(Number(await page.locator("#tableRowHeight").inputValue())).toBeGreaterThan(0);

  expect(consoleErrors).toEqual([]);
});

test("a column boundary on a table the caret is not in moves the border, keeping the table width", async ({
  page,
  consoleErrors,
}) => {
  await insertTable(page);
  await expect(page.locator(".overlay .cell-outline")).toHaveCount(1);
  const cell = await local(page, ".overlay .cell-outline");
  await leaveTable(page);

  const found = await findBoundary(page, "table-column-boundary", {
    x0: cell.x + cell.w - 10,
    y0: cell.y + cell.h / 2,
    x1: cell.x + cell.w + 10,
    y1: cell.y + cell.h / 2,
    step: 1,
  });
  expect(found, "no point across the first column's trailing border was armed").not.toBeNull();
  expect(await pageCursor(page)).toBe("col-resize");

  // The table's own painted width, from the engine's table selection rects — an
  // independent geometry path from the one the drag uses.
  //
  // Every viewport coordinate below is taken from a FRESH sheet box, and that is
  // not caution: selecting the contextual Table tab swaps the ribbon band, which
  // is a different height, so the sheet moves under the pointer. Reusing one box
  // across the tab switch put the drag 20-odd px off the border and the gesture
  // never started — which looked exactly like a broken zone.
  const sheet = () => stableBox(page.locator(".page-wrap .page").first());
  const tableWidth = async () => {
    const box = await sheet();
    await page.mouse.click(box.x + cell.x + cell.w / 2, box.y + cell.y + cell.h / 2);
    await expect(page.locator("#tabTable")).toBeEnabled();
    await page.locator("#tabTable").click();
    await page.locator('[data-table-select="table"]').click();
    const rects = page.locator(".overlay .table-cell-selection");
    await expect(rects.first()).toBeVisible();
    return page.evaluate(() => {
      const all = [...document.querySelectorAll(".overlay .table-cell-selection")];
      const left = Math.min(...all.map((el) => el.getBoundingClientRect().x));
      const right = Math.max(...all.map((el) => el.getBoundingClientRect().right));
      return +(right - left).toFixed(1);
    });
  };
  const widthBefore = await tableWidth();
  await leaveTable(page);

  const box = await sheet();
  await page.mouse.move(box.x + found.x, box.y + found.y);
  await expect
    .poll(() => pointerTarget(page), { message: "the column boundary under the pointer" })
    .toBe("table-column-boundary");
  await page.mouse.down();
  await page.mouse.move(box.x + found.x + 40, box.y + found.y, { steps: 6 });
  await expect(page.locator(".overlay .table-col-resize-preview")).toHaveCount(1);
  await page.mouse.up();

  // THE GUARANTEE: the first column grew and the table did NOT. `setTableColumnWidthAt`
  // could not express this — it set one column's absolute width and let the
  // solver decide the rest, so dragging column 1's right border widened the whole
  // table (`docs/141` §1.1 cost 2).
  await expect
    .poll(async () => (await local(page, ".overlay .cell-outline"))?.w ?? 0, {
      message: "the first column's laid-out width after the drag",
    })
    .toBeGreaterThan(cell.w + 15);
  const widthAfter = await tableWidth();
  expect(
    Math.abs(widthAfter - widthBefore),
    `the table width moved from ${widthBefore} to ${widthAfter}: an internal ` +
      'border drag must run in "border" mode, where the neighbour absorbs the delta',
  ).toBeLessThan(6);

  expect(consoleErrors).toEqual([]);
});

test("the zone is not armed in Viewing, and says why in Suggesting", async ({
  page,
  consoleErrors,
}) => {
  await insertTable(page);
  const cell = await local(page, ".overlay .cell-outline");
  await leaveTable(page);

  // Editing first, so the rest of the test is a comparison rather than an
  // assertion about nothing: a mode test that cannot tell "refused" from "there
  // was never a boundary there" proves nothing at all.
  const found = await findBoundary(page, "table-row-boundary", {
    x0: cell.x + cell.w / 2,
    y0: cell.y - 6,
    x1: cell.x + cell.w / 2,
    y1: cell.y + cell.h * 3,
    step: 2,
  });
  expect(found, "the boundary must be armed in Editing for this test to mean anything").not.toBeNull();

  // Each mode re-measures the cell and the sheet, deliberately: the review banner
  // and the review gutter both move AND RESIZE the sheet, so a page-local
  // coordinate taken in Editing is at a different document position in Suggesting.
  // That is exactly the shape of bug that makes a refusal test pass for the wrong
  // reason.
  const boundaryNow = async () => {
    const sheet = await stableBox(page.locator(".page-wrap .page").first());
    const here = await local(page, ".overlay .cell-outline");
    expect(here, "the caret must be in a cell so its box can be measured").not.toBeNull();
    return { x: sheet.x + here.x + here.w / 2, y: sheet.y + here.y + here.h };
  };
  const clickIntoCell = async () => {
    const sheet = await stableBox(page.locator(".page-wrap .page").first());
    await page.mouse.click(sheet.x + cell.x + cell.w / 2, sheet.y + cell.y + cell.h / 2);
    await expect(page.locator(".overlay .cell-outline")).toHaveCount(1);
  };

  // Viewing: nothing is offered, so nothing is said. The cursor stays body text
  // and a press there is an ordinary caret placement.
  await setReviewMode(page, "viewing");
  await clickIntoCell();
  let at = await boundaryNow();
  await page.mouse.move(at.x, at.y);
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => r())));
  expect(await pointerTarget(page)).not.toBe("table-row-boundary");
  expect(await pageCursor(page)).not.toBe("row-resize");

  // Suggesting: also not armed — a structural change has no tracked-change
  // representation — but a PRESS explains the absence rather than leaving the
  // user to guess at a gesture every other mode has.
  await setReviewMode(page, "suggesting");
  await clickIntoCell();
  at = await boundaryNow();
  await page.mouse.move(at.x, at.y);
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => r())));
  expect(await pageCursor(page)).not.toBe("row-resize");
  await page.mouse.click(at.x, at.y);
  await expect(page.locator("#status")).toContainText(/Suggesting/i);

  expect(consoleErrors).toEqual([]);
});

test("Alt+Shift+Arrow sizes the caret's row and column", async ({ page, consoleErrors }) => {
  await insertTable(page);
  const before = await local(page, ".overlay .cell-outline");

  await page.keyboard.press("Alt+Shift+ArrowDown");
  await expect
    .poll(async () => (await local(page, ".overlay .cell-outline"))?.h ?? 0, {
      message: "the row's laid-out height after Alt+Shift+Down",
    })
    .toBeGreaterThan(before.h);

  const taller = await local(page, ".overlay .cell-outline");
  await page.keyboard.press("Alt+Shift+ArrowRight");
  await expect
    .poll(async () => (await local(page, ".overlay .cell-outline"))?.w ?? 0, {
      message: "the column's laid-out width after Alt+Shift+Right",
    })
    .toBeGreaterThan(taller.w);

  expect(consoleErrors).toEqual([]);
});

test("a touch tap arms 24px boundary pills", async ({ page, consoleErrors }) => {
  await insertTable(page);
  const cell = await local(page, ".overlay .cell-outline");
  const box = await stableBox(page.locator(".page-wrap .page").first());

  // No pills from a mouse: hover is the mouse's affordance and pills would be
  // resting chrome nobody asked for.
  await expect(page.locator(".overlay .table-row-touch-pill")).toHaveCount(0);

  // A real touch pointer. Playwright's mouse cannot produce `pointerType:
  // "touch"`, so the tap is dispatched as the pointer events the browser would
  // send — which is also the only shape the product reads (`event.pointerType`).
  const tap = ([x, y]) => {
    const el = document.querySelector(".page-wrap .page");
    const init = {
      pointerType: "touch",
      isPrimary: true,
      pointerId: 1,
      button: 0,
      buttons: 1,
      clientX: x,
      clientY: y,
      bubbles: true,
      cancelable: true,
    };
    el.dispatchEvent(new PointerEvent("pointerdown", init));
    el.dispatchEvent(new PointerEvent("pointerup", { ...init, buttons: 0 }));
  };
  await page.evaluate(tap, [box.x + cell.x + cell.w / 2, box.y + cell.y + cell.h / 2]);

  const pills = page.locator(".overlay .table-row-touch-pill");
  await expect(pills.first()).toBeVisible();
  const size = await pills.first().evaluate((el) => el.getBoundingClientRect().height);
  expect(size, "WCAG 2.5.8 Target Size (Minimum) is 24 CSS px").toBeGreaterThanOrEqual(24);
  await expect(page.locator(".overlay .table-col-touch-pill").first()).toBeVisible();

  expect(consoleErrors).toEqual([]);
});
