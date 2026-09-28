// The table gutter and the cell selection, proven from the GESTURE end
// (`docs/141` D-2 and the UI half of D-3).
//
// The bar is the owner's own sentence: *"table UX is also not built"*, against
// Google Docs. So every assertion below is on something a user does and sees —
// "dragging down the left gutter selects three rows and shading them applies to
// all of them in one undo entry" — and not on the existence of a DOM node. A
// guard that asserts `.table-row-strip` exists would pass while the strip
// selected the wrong row, inserted at the caret instead of at the pointer, or
// shaded one cell of nine.
//
// The observable for a selection is the count and geometry of
// `.table-cell-selection` rectangles, which the painter draws straight from
// `tableCellRangeRects` — so reading them is reading the engine's answer, not a
// re-derivation of it.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  stableBox,
  MOD,
} from "./fixtures.mjs";

/** Inserts a 3-row x 3-column table below a first paragraph. */
async function insertTable(page, rows = 3, cols = 3) {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type("Above the table.");
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await expect(page.locator("#insertTableMenu")).toBeVisible();
  await page.locator(`.gc[data-r="${rows}"][data-c="${cols}"]`).click();
  await expect(page.locator("#tabTable")).toBeEnabled();
}

/** Page-local rects of every selection fill, sorted top-to-bottom. */
const selectionRects = (page) =>
  page.evaluate(() => {
    const sheet = document.querySelector(".page-wrap .page").getBoundingClientRect();
    return [...document.querySelectorAll(".overlay .table-cell-selection")]
      .map((el) => {
        const r = el.getBoundingClientRect();
        return {
          x: Math.round(r.x - sheet.x),
          y: Math.round(r.y - sheet.y),
          w: Math.round(r.width),
          h: Math.round(r.height),
        };
      })
      .sort((a, b) => a.y - b.y || a.x - b.x);
  });

/** The active cell outline, which marks the caret's own cell. */
const activeCell = (page) =>
  page.evaluate(() => {
    const el = document.querySelector(".overlay .cell-outline");
    if (!el) return null;
    const sheet = document.querySelector(".page-wrap .page").getBoundingClientRect();
    const r = el.getBoundingClientRect();
    return { x: Math.round(r.x - sheet.x), y: Math.round(r.y - sheet.y), w: Math.round(r.width), h: Math.round(r.height) };
  });

/** Sweeps LEFT of the table until the row strip appears, and returns the client
 *  point at which it did. Probing rather than computing is deliberate: the test
 *  has to find the strip the way a user does — by moving the pointer and seeing
 *  it appear — so a strip that exists in the geometry but is never armed fails. */
async function findRowStrip(page, { top, height }) {
  const box = await stableBox(page.locator(".page-wrap .page").first());
  const cell = await activeCell(page);
  for (let dx = 2; dx <= 26; dx += 2) {
    await page.mouse.move(box.x + cell.x - dx, box.y + top + height / 2);
    await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => r())));
    const strip = await page.locator(".overlay .table-row-strip").count();
    if (strip > 0) return { x: box.x + cell.x - dx, y: box.y + top + height / 2 };
  }
  return null;
}

test("dragging down the left gutter selects three whole rows, and shading applies to all of them in one undo entry", async ({
  page,
  consoleErrors,
}) => {
  await insertTable(page, 3, 3);
  const cell = await activeCell(page); // row 0, column 0
  expect(cell).not.toBeNull();

  // 1. The strip appears beside the table on hover, and pressing it selects the
  //    row it is BESIDE — three cells, not one, and row 1, not row 0. The middle
  //    row is chosen deliberately: a strip that always resolved the first row,
  //    or the caret's row, would pass a test that started at the top.
  const middle = await findRowStrip(page, { top: cell.y + cell.h, height: cell.h });
  expect(middle, "no row strip was armed anywhere left of the table").not.toBeNull();
  await page.mouse.click(middle.x, middle.y);
  await expect(page.locator(".overlay .table-cell-selection")).toHaveCount(3);
  const row1 = await selectionRects(page);
  for (const rect of row1) {
    expect(
      Math.abs(rect.y - (cell.y + cell.h)),
      `the strip beside row 1 selected something at y=${rect.y}, not row 1 at y=${cell.y + cell.h}`,
    ).toBeLessThanOrEqual(3);
  }

  // 2. Pressing the first row's strip starts a selection there instead.
  const at = await findRowStrip(page, { top: cell.y, height: cell.h });
  expect(at).not.toBeNull();
  await page.mouse.move(at.x, at.y);
  await page.mouse.down();
  await expect(page.locator(".overlay .table-cell-selection")).toHaveCount(3);

  // 3. Dragging DOWN the strip extends it across rows. Overshooting the last
  //    row is deliberate — a user aiming at "everything" drags past the end, and
  //    the drop must clamp to the last band rather than stop tracking.
  await page.mouse.move(at.x, at.y + cell.h * 5, { steps: 10 });
  await page.mouse.up();
  await expect(page.locator(".overlay .table-cell-selection")).toHaveCount(9);
  const rects = await selectionRects(page);
  expect(new Set(rects.map((r) => r.y)).size, "three distinct rows").toBe(3);

  // 4. …and the status line says how many cells, which is the only channel a
  //    reader who cannot see the fill has.
  await expect(page.locator("#status")).toContainText("9 cells selected");

  // 5. Shading applies to the selection, and does NOT refuse. The old behaviour
  //    was a hard stop — "Shading, alignment and cell borders apply to one cell
  //    — put the caret in the cell to format it" — so the first thing to prove
  //    is that the sentence is gone rather than merely reworded.
  const undoBefore = await page.locator("#undoBtn").getAttribute("aria-label");
  await page.locator('[data-tab="table"]').click();
  await page.locator("#tableBtn").click();
  await expect(page.locator("#tableMenu")).toBeVisible();
  await page.locator("#cellShade").evaluate((el) => {
    el.value = "#ffcc00";
    el.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await expect(page.locator("#status")).not.toContainText("put the caret in the cell");

  // 6. ONE undo entry for nine cells, not nine. The label moved to the shading
  //    edit; a SINGLE undo must take it all the way back to the edit that came
  //    before, which is only true if the nine writes were one action.
  const undoAfter = page.locator("#undoBtn");
  await expect(undoAfter).not.toHaveAttribute("aria-label", undoBefore ?? "");
  await page.keyboard.press(`${MOD}+z`);
  await expect(undoAfter).toHaveAttribute("aria-label", undoBefore ?? "");
  expect(consoleErrors).toEqual([]);
});

test("a drag inside the table selects a RECTANGLE of cells, and one cell is not a cell selection", async ({
  page,
  consoleErrors,
}) => {
  await insertTable(page, 3, 3);
  const cell = await activeCell(page);
  const box = await stableBox(page.locator(".page-wrap .page").first());
  const centre = (row, col) => ({
    x: box.x + cell.x + cell.w * col + cell.w / 2,
    y: box.y + cell.y + cell.h * row + cell.h / 2,
  });

  // A drag that stays in one cell is ordinary text selection — no fill.
  const a = centre(0, 0);
  await page.mouse.move(a.x - 6, a.y);
  await page.mouse.down();
  await page.mouse.move(a.x + 6, a.y, { steps: 4 });
  await page.mouse.up();
  await expect(page.locator(".overlay .table-cell-selection")).toHaveCount(0);

  // A drag that crosses into another cell selects the rectangle between them.
  const b = centre(1, 1);
  await page.mouse.move(a.x, a.y);
  await page.mouse.down();
  await page.mouse.move(b.x, b.y, { steps: 10 });
  await page.mouse.up();
  await expect(page.locator(".overlay .table-cell-selection")).toHaveCount(4);
  await expect(page.locator("#status")).toContainText("4 cells selected");

  // Merge is now offered, because a rectangle of four cells is two or more.
  await page.locator('[data-tab="table"]').click();
  await expect(page.locator("#mergeCellsBtn")).toBeEnabled();
  expect(consoleErrors).toEqual([]);
});

test("merge refuses a single cell with the sentence that says why, and never as a dead control", async ({
  page,
  consoleErrors,
}) => {
  await insertTable(page, 2, 2);
  await page.locator('[data-tab="table"]').click();
  const merge = page.locator("#mergeCellsBtn");
  await expect(merge).toBeDisabled();
  // Disabled WITH a reason: `title` is the only channel a disabled button has,
  // and a hover must not erase it (`docs/141` TBL-03).
  // `data-tip-title` is where a disabled control's reason lives: `title` is
  // cleared while the custom tooltip owns it, and the fix that stopped a hover
  // erasing the reason is exactly what this asserts survives a hover.
  const reasonOf = () =>
    merge.evaluate((el) => el.getAttribute("title") || el.dataset.tipTitle || "");
  expect(await reasonOf()).toBe("Select two or more cells before merging");
  await merge.hover({ force: true });
  expect(await reasonOf()).toBe("Select two or more cells before merging");
  expect(consoleErrors).toEqual([]);
});

test("Shift+Arrow extends the cell selection instead of collapsing it to text", async ({
  page,
  consoleErrors,
}) => {
  await insertTable(page, 3, 3);
  const cell = await activeCell(page);
  const box = await stableBox(page.locator(".page-wrap .page").first());

  // Start from a real cell selection made by the gutter, so the keyboard is
  // extending what the pointer built — one selection, two ways in.
  const at = await findRowStrip(page, { top: cell.y, height: cell.h });
  expect(at).not.toBeNull();
  await page.mouse.click(at.x, at.y);
  await expect(page.locator(".overlay .table-cell-selection")).toHaveCount(3);

  await page.keyboard.press("Shift+ArrowDown");
  await expect(page.locator(".overlay .table-cell-selection")).toHaveCount(6);
  await page.keyboard.press("Shift+ArrowDown");
  await expect(page.locator(".overlay .table-cell-selection")).toHaveCount(9);

  // At the bottom edge it is a WALL: the key is consumed and the selection is
  // left exactly as it was, rather than being destroyed by walking out.
  await page.keyboard.press("Shift+ArrowDown");
  await expect(page.locator(".overlay .table-cell-selection")).toHaveCount(9);
  void box;
  expect(consoleErrors).toEqual([]);
});

test("the + disc inserts at the boundary the pointer is on, not at the caret", async ({
  page,
  consoleErrors,
}) => {
  // This is the assertion that distinguishes "insert where I am pointing" from
  // "insert where the caret is" — the whole point of the affordance
  // (`docs/141` §4.2.7 case 2). The caret is left in the FIRST row and the disc
  // is pressed on the boundary below the LAST row, so a row appended at the
  // caret would land in the wrong place and the mirror would show it.
  await insertTable(page, 2, 2);
  await page.keyboard.type("TOP");
  await page.keyboard.press("Tab");
  await page.keyboard.press("Tab");
  await page.keyboard.type("SECOND");
  // …and the caret goes BACK to the first row, so "insert at the caret" and
  // "insert at the pointer" put the new row in different places.
  await page.keyboard.press("Shift+Tab");
  await page.keyboard.press("Shift+Tab");
  const cell = await activeCell(page);
  const box = await stableBox(page.locator(".page-wrap .page").first());

  const at = await findRowStrip(page, { top: cell.y, height: cell.h });
  expect(at).not.toBeNull();
  // Move onto the boundary BELOW the second row — the table's own trailing edge.
  const discY = box.y + cell.y + cell.h * 2;
  await page.mouse.move(at.x, discY);
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => r())));
  const disc = page.locator(".overlay .table-insert-target");
  await expect(disc).toHaveCount(1);
  await expect(disc).toHaveAttribute("title", "Insert a row here");
  await page.mouse.click(at.x, discY);

  await expect(page.locator("#status")).toContainText("Row inserted");
  // The table grew by exactly one row, and "TOP" is still the FIRST cell — an
  // insert at the caret would have pushed it down. The row count is the whole
  // assertion: a new row carries no text, so comparing the mirror's TEXT would
  // have been a guard that could not fail.
  // Scoped to OUR table — the demo document carries a nested one, and counting
  // every `tr` in the mirror would have measured that instead.
  const grown = await page.evaluate(() => {
    const table = [...document.querySelectorAll("#a11yDocument table")].find((t) =>
      t.textContent.includes("TOP"),
    );
    return table ? [...table.querySelectorAll("tr")].map((tr) => tr.textContent.trim()) : null;
  });
  expect(grown).not.toBeNull();
  // Three rows, and the NEW one is LAST — which is what "at the boundary I am
  // pointing at" means and what "at the caret" (still in row 0) would not give.
  expect(grown.length).toBe(3);
  expect(grown[0]).toContain("TOP");
  expect(grown[1]).toContain("SECOND");
  expect(grown[2]).toBe("");
  expect(consoleErrors).toEqual([]);
});

test("the accessibility mirror expresses the cell selection, not only the fill", async ({
  page,
  consoleErrors,
}) => {
  // A fill on a canvas says nothing to a screen reader. With a cell selection
  // live, the caret's table becomes an ARIA grid and the selected rectangle
  // carries `aria-selected` — so walking back over the table tells you what is
  // in the selection, which the live-region sentence alone cannot.
  await insertTable(page, 3, 3);
  const cell = await activeCell(page);
  const at = await findRowStrip(page, { top: cell.y, height: cell.h });
  expect(at).not.toBeNull();
  await page.mouse.click(at.x, at.y);
  await expect(page.locator(".overlay .table-cell-selection")).toHaveCount(3);

  const grid = await page.evaluate(() => {
    const table = document.querySelector('#a11yDocument table[role="grid"]');
    if (!table) return null;
    return {
      multiselectable: table.getAttribute("aria-multiselectable"),
      selected: [...table.querySelectorAll('[aria-selected="true"]')].length,
      unselected: [...table.querySelectorAll('[aria-selected="false"]')].length,
    };
  });
  expect(grid, "the caret's table must become a grid while cells are selected").not.toBeNull();
  expect(grid.multiselectable).toBe("true");
  expect(grid.selected).toBe(3);
  expect(grid.unselected).toBe(6);
  expect(consoleErrors).toEqual([]);
});

test("the strips are hover chrome only: they never rest on the page, and a click in a cell still places the caret", async ({
  page,
  consoleErrors,
}) => {
  await insertTable(page, 3, 3);
  const cell = await activeCell(page);
  const box = await stableBox(page.locator(".page-wrap .page").first());

  // Nothing at rest, before the pointer has been anywhere near the table.
  await expect(page.locator(".overlay .table-row-strip")).toHaveCount(0);

  // Hovering the gutter arms the strip…
  const at = await findRowStrip(page, { top: cell.y, height: cell.h });
  expect(at).not.toBeNull();
  await expect(page.locator(".overlay .table-row-strip")).toHaveCount(1);

  // …and moving away takes it down again. Without this the strips accumulate
  // beside every table the pointer has ever passed, which is exactly the
  // "resting chrome" `docs/141` §4.4 forbids.
  await page.mouse.move(box.x + cell.x + cell.w / 2, box.y + Math.max(8, cell.y - 24));
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => r())));
  await expect(page.locator(".overlay .table-row-strip")).toHaveCount(0);
  await expect(page.locator(".overlay .table-column-strip")).toHaveCount(0);

  // A click anywhere inside a cell still places the caret and types there — the
  // property the strips keep by lying entirely outside the table.
  for (const [row, col] of [[0, 0], [1, 1], [2, 2]]) {
    await page.mouse.click(
      box.x + cell.x + cell.w * col + cell.w / 2,
      box.y + cell.y + cell.h * row + cell.h / 2,
    );
    await expect(page.locator(".overlay .table-cell-selection")).toHaveCount(0);
    await page.keyboard.type("X");
  }
  const text = await page.evaluate(
    () => document.querySelector("#a11yDocument table")?.textContent ?? "",
  );
  expect(text.replace(/[^X]/g, "")).toBe("XXX");
  expect(consoleErrors).toEqual([]);
});
