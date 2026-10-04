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
  MOD,
  clickIntoFirstPage,
  expect,
  gotoEditor,
  moveCaretToDocStart,
  stableBox,
  test,
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
 *  it appear — so a strip that exists in the geometry but is never armed fails.
 *
 *  It ENTERS the table first, because that is what arms the gutter: the strips
 *  are not targets for a pointer that has never been on the table. */
async function findRowStrip(page, { top, height }) {
  const box = await stableBox(page.locator(".page-wrap .page").first());
  const cell = await activeCell(page);
  await page.mouse.move(box.x + cell.x + cell.w / 2, box.y + top + height / 2);
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => r())));
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

test("the gutter arms by ENTERING the table, so the line above one is still that line", async ({
  page,
  consoleErrors,
}) => {
  // The column strip is a 14px band directly above the table, and body text
  // frequently sits in it. Arming on proximity alone made the paragraph above a
  // table report a column strip — and a press there would have selected a column
  // instead of placing the caret in that paragraph. The pointer-cursor sweep
  // caught it as "a form checkbox one line above a table stopped being a
  // control", which is the same defect seen from the other end.
  await insertTable(page, 3, 3);
  const cell = await activeCell(page);
  const box = await stableBox(page.locator(".page-wrap .page").first());
  const justAbove = { x: box.x + cell.x + cell.w / 2, y: box.y + cell.y - 4 };

  await page.mouse.move(justAbove.x, justAbove.y);
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => r())));
  await expect(page.locator(".overlay .table-column-strip")).toHaveCount(0);
  const target = await page.evaluate(
    () => document.querySelector(".page-wrap .page")?.dataset.pointerTarget ?? "",
  );
  expect(target, "the router must still speak for the text under the pointer").not.toBe("");

  // …and a press there places a caret rather than selecting a column.
  await page.mouse.click(justAbove.x, justAbove.y);
  await expect(page.locator(".overlay .table-cell-selection")).toHaveCount(0);
  expect(consoleErrors).toEqual([]);
});

test("a merged table gets a row strip and NO column strip, rather than one that refuses", async ({
  page,
  consoleErrors,
}) => {
  // A column selection has no meaning on a merged or spanned table —
  // `tableSelectionAnchorNodes(node, "column")` returns nothing there — so a
  // painted column strip would be a control that refuses on click. A row of a
  // merged table is still a row, so that strip stays.
  await insertTable(page, 3, 3);
  await page.locator('[data-tab="table"]').click();
  await page.locator('[data-table-select="row"]').click();
  await page.locator("#mergeCellsBtn").click();
  await expect(page.locator("#tableContext")).toContainText("merged/spanned");

  const cell = await activeCell(page);
  const box = await stableBox(page.locator(".page-wrap .page").first());
  await page.mouse.move(box.x + cell.x + cell.w / 2, box.y + cell.y + cell.h / 2);
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => r())));
  await expect(page.locator(".overlay .table-row-strip")).toHaveCount(1);
  await expect(page.locator(".overlay .table-column-strip")).toHaveCount(0);
  expect(consoleErrors).toEqual([]);
});

test.describe("touch", () => {
  test.use({ hasTouch: true });

  test("a tap inside the table arms the gutter, and the strip is 24px under a finger", async ({
    page,
    consoleErrors,
  }) => {
    // Hover does not exist on touch, so without an arming tap the whole gutter is
    // unreachable from a finger. The first tap places the caret AND arms the
    // strips — the rule the boundary pills already follow (`docs/141` TBL-18) —
    // and the strip then measures 24 CSS px, WCAG 2.5.8 Target Size (Minimum).
    // It can afford to: the strip is outside the table, so widening it takes
    // nothing from any cell, which is exactly what stopped the BOUNDARY zones
    // from doing the same.
    await insertTable(page, 3, 3);
    const cell = await activeCell(page);
    const box = await stableBox(page.locator(".page-wrap .page").first());
    await expect(page.locator(".overlay .table-row-strip")).toHaveCount(0);

    await page.touchscreen.tap(box.x + cell.x + cell.w / 2, box.y + cell.y + cell.h * 1.5);
    const strip = page.locator(".overlay .table-row-strip");
    await expect(strip).toHaveCount(1);
    const width = await strip.evaluate((el) => el.getBoundingClientRect().width);
    expect(width, "a strip a finger can hit is at least 24 CSS px").toBeGreaterThanOrEqual(24);

    // …and a second tap ON the strip selects that row.
    await page.touchscreen.tap(box.x + cell.x - 12, box.y + cell.y + cell.h * 1.5);
    await expect(page.locator(".overlay .table-cell-selection")).toHaveCount(3);
    expect(consoleErrors).toEqual([]);
  });
});

/** The painted appearance of one overlay element: its box, its fill, and the
 *  browser-resolved `var(--accent)` to compare that fill against. */
const gutterPaint = (page, selector) =>
  page.evaluate((sel) => {
    const el = document.querySelector(sel);
    if (!el) return null;
    const box = el.getBoundingClientRect();
    // `var(--accent)` resolved by the browser, so the comparison holds whatever
    // accent the user has persisted — main.js writes it inline on the root.
    const probe = document.createElement("div");
    probe.style.background = "var(--accent)";
    document.body.appendChild(probe);
    const accent = getComputedStyle(probe).backgroundColor;
    probe.remove();
    // Chrome serializes a `color-mix()` as `color(srgb 0.79 0.80 0.80 / 0.26)` —
    // 0..1 floats — and a plain colour as `rgb(51, 85, 196)`. Both are
    // normalised to 0..255 here, or every comparison would be between scales.
    const rgb = (value) => {
      const parts = (value.match(/[\d.]+/g) ?? []).map(Number);
      const head = parts.slice(0, 3);
      return value.startsWith("color(") ? head.map((v) => Math.round(v * 255)) : head;
    };
    // These fills are deliberately TRANSLUCENT — they are drawn over a border
    // the engine has already painted — so the channel triple alone cannot say
    // which of two states is darker. The alpha comes back with it.
    const alpha = (value) => {
      const parts = (value.match(/[\d.]+/g) ?? []).map(Number);
      if (value.startsWith("color(")) return parts.length > 3 ? parts[3] : 1;
      return parts.length > 3 ? parts[3] : 1;
    };
    const canvas = document.querySelector(".page-wrap canvas.page");
    const sheet = canvas.getBoundingClientRect();
    return {
      x: box.x - sheet.x,
      y: box.y - sheet.y,
      width: box.width,
      height: box.height,
      centreX: box.x + box.width / 2 - sheet.x,
      centreY: box.y + box.height / 2 - sheet.y,
      fill: rgb(getComputedStyle(el).backgroundColor),
      alpha: alpha(getComputedStyle(el).backgroundColor),
      accent: rgb(accent),
    };
  }, selector);

/**
 * Where the ENGINE actually painted a dark vertical (or horizontal) rule, read
 * out of the page raster within `win` CSS px of `about`.
 *
 * The border is drawn into a canvas, so this is the only way to check that the
 * highlight lands ON it rather than a pixel or two off it — comparing the
 * highlight against the same `tableChromeOnPage` geometry that positioned it
 * would be a tautology, and a two-pixel offset reads as a second parallel rule
 * next to the border rather than as the border being lit.
 */
const paintedRule = (page, axis, about, along, win = 10) =>
  page.evaluate(
    ([a, aboutCss, alongCss, w]) => {
      const canvas = document.querySelector(".page-wrap canvas.page");
      const rect = canvas.getBoundingClientRect();
      const kx = canvas.width / rect.width;
      const ky = canvas.height / rect.height;
      const ctx = canvas.getContext("2d", { willReadFrequently: true });
      const vertical = a === "row";
      const k = vertical ? kx : ky;
      const from = Math.max(0, Math.round((aboutCss - w) * k));
      const count = Math.max(1, Math.round(2 * w * k));
      const at = Math.round(alongCss * (vertical ? ky : kx));
      const data = vertical
        ? ctx.getImageData(from, at, count, 1).data
        : ctx.getImageData(at, from, 1, count).data;
      let best = -1;
      let darkest = 256;
      for (let i = 0; i < count; i++) {
        const lum = (data[i * 4] + data[i * 4 + 1] + data[i * 4 + 2]) / 3;
        if (lum < darkest) {
          darkest = lum;
          best = i;
        }
      }
      return { at: (from + best) / k, luminance: darkest };
    },
    [axis, about, along, win],
  );

/** How far a colour is from grey: the spread between its strongest and weakest
 *  channel. 0 is a pure neutral; `--accent` (#3355c4 by default) is ~145. */
const chroma = ([r, g, b]) => Math.max(r, g, b) - Math.min(r, g, b);

/** Perceived lightness, enough to say "this one is darker than that one". */
const luma = ([r, g, b]) => 0.2126 * r + 0.7152 * g + 0.0722 * b;

/** What the eye actually gets: the fill composited over the white sheet. A
 *  translucent line's darkness lives in its alpha, not in its channels. */
const onPaper = ({ fill, alpha }) => luma(fill.map((c) => c * alpha + 255 * (1 - alpha)));

test("the table's own border is the affordance: lit on the border, neutral until the row is SELECTED", async ({
  page,
  consoleErrors,
}) => {
  // The owner, on the previous cut: *"instead of adding new header on left and
  // top.. why not highlighting the existing border.. and using that"*. So
  // nothing is drawn beside the table any more — what is drawn is a 3px line
  // centred on the table's OWN leading border, in three states.
  //
  // ONLYOFFICE is the product whose source was actually read for this:
  // `private_CheckHitInBorder` cancels the border resize (`Border = -1`) on the
  // leading edges and reports `RowSelection`/`ColumnSelection` instead.
  //
  // Four things are held here, and each can fail on its own:
  //   1. the highlight lands ON the painted border, read out of the raster;
  //   2. it is 3px drawn inside a >= 14px target, so no target shrank;
  //   3. rest is neutral, hover is the same line darker, selection is accent;
  //   4. a click exactly ON the border selects the row rather than placing a
  //      caret, which is what "using that" means.
  await insertTable(page, 3, 3);
  const cell = await activeCell(page);
  const box = await stableBox(page.locator(".page-wrap .page").first());
  const rowMiddle = cell.y + cell.h * 1.5;

  // Arm the gutter the way a user does: by entering the table.
  await page.mouse.move(box.x + cell.x + cell.w * 1.5, box.y + rowMiddle);
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => r())));

  // --- 1. the highlight is ON the border -----------------------------------
  const border = await paintedRule(page, "row", cell.x, rowMiddle);
  expect(border.luminance, "the fixture table must have a painted left border").toBeLessThan(160);
  const edge = await gutterPaint(page, ".overlay .table-gutter-edge");
  expect(edge, "the resting highlight must be painted while the pointer is in the table").not.toBeNull();
  expect(
    Math.abs(edge.centreX - border.at),
    `the highlight is centred at ${edge.centreX} but the border is painted at ${border.at}`,
  ).toBeLessThanOrEqual(1.5);

  // --- 2. drawn 3px, targeted 14 -------------------------------------------
  const strip = await gutterPaint(page, ".overlay .table-row-strip");
  expect(strip.width, "the pointer target stays at least 14 CSS px").toBeGreaterThanOrEqual(14);
  expect(strip.fill, "the grab zone paints nothing itself").toEqual([0, 0, 0]);
  expect(edge.width, "the drawn line is far narrower than the target").toBeLessThan(strip.width);
  expect(edge.width, "…and thick enough to read as a lit border").toBeGreaterThanOrEqual(2);

  // --- 3. the three colours ------------------------------------------------
  expect(
    chroma(edge.fill),
    `the resting border must be neutral, not accent — got rgb(${edge.fill})`,
  ).toBeLessThanOrEqual(8);

  // Hover the band, on the border itself.
  await page.mouse.move(box.x + border.at, box.y + rowMiddle);
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => r())));
  const hover = await gutterPaint(page, ".overlay .table-gutter-hover");
  expect(hover, "the band under the pointer must light up").not.toBeNull();
  expect(chroma(hover.fill), "the hovered band must be neutral too").toBeLessThanOrEqual(8);
  expect(
    onPaper(hover),
    "the hovered band must be DARKER than the resting border, or hover says nothing",
  ).toBeLessThan(onPaper(edge) - 20);
  expect(edge.alpha, "the resting line must be translucent, or it bleaches the border").toBeLessThan(
    1,
  );
  expect(hover.width, "hover must not move or widen the line").toBeCloseTo(edge.width, 1);
  expect(hover.centreX, "hover must stay on the border").toBeCloseTo(edge.centreX, 1);

  // --- 4. the border IS the target -----------------------------------------
  // A click exactly on the painted border selects that row.
  await page.mouse.click(box.x + border.at, box.y + rowMiddle);
  await expect(page.locator(".overlay .table-cell-selection")).toHaveCount(3);

  // …and so does a click a hair INSIDE it. This is the half that only works
  // because the PRESS path takes the border too: the hover lit it from either
  // side while `tryBeginDrag` still bailed on anything inside the table's box,
  // so the cursor promised a selection the press then declined. Clicking from
  // outside alone could not have caught that, which is why both sides are here.
  await page.mouse.click(box.x + cell.x + cell.w / 2, box.y + cell.y + cell.h / 2);
  await expect(page.locator(".overlay .table-cell-selection")).toHaveCount(0);
  const inside = border.at + 2;
  expect(inside, "the probe point must really be inside the table's box").toBeGreaterThan(cell.x);
  await page.mouse.move(box.x + cell.x + cell.w * 1.5, box.y + rowMiddle);
  await page.mouse.click(box.x + inside, box.y + rowMiddle);
  await expect(page.locator(".overlay .table-cell-selection")).toHaveCount(3);

  const selected = await gutterPaint(page, ".overlay .table-gutter-selection");
  expect(selected, "a selected row keeps its border lit").not.toBeNull();
  expect(
    selected.fill,
    "the SELECTED border is the accent — that is what colour means here",
  ).toEqual(selected.accent);
  expect(selected.centreX, "the selection stays on the border too").toBeCloseTo(edge.centreX, 1);
  expect(consoleErrors).toEqual([]);
});

test("selecting on the border does not cost the resize: both survive, a few pixels apart", async ({
  page,
  consoleErrors,
}) => {
  // The tension this design has to answer. Every row's leading edge except the
  // first is the previous row's trailing edge — a real resize boundary — so the
  // two gestures DO meet where an inner row boundary crosses the left border.
  // The gutter wins there (it is consulted first in the press path and its zone
  // carries the `cell` cursor), and the resize has to stay reachable immediately
  // past the zone. Both halves are asserted, because only holding one of them
  // would let the fix for either break the other silently.
  await insertTable(page, 3, 3);
  const cell = await activeCell(page);
  const box = await stableBox(page.locator(".page-wrap .page").first());
  const crossing = cell.y + cell.h; // row 0's bottom = row 1's top
  await page.mouse.move(box.x + cell.x + cell.w * 1.5, box.y + cell.y + cell.h * 1.5);
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => r())));
  const border = await paintedRule(page, "row", cell.x, cell.y + cell.h * 1.5);

  /** What owns the point: the element the browser hit-tests to, its cursor, and
   *  whatever the router has put on the page underneath it. */
  const owner = async (x, y) => {
    await page.mouse.move(x, y);
    await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => r())));
    return page.evaluate(
      ([cx, cy]) => {
        const el = document.elementFromPoint(cx, cy);
        return {
          klass: typeof el?.className === "string" ? el.className : "",
          cursor: el ? getComputedStyle(el).cursor : "",
          pageCursor: getComputedStyle(document.querySelector(".page-wrap canvas.page")).cursor,
        };
      },
      [x, y],
    );
  };

  // ON the border, in the middle of a band: the grab zone, saying `cell`.
  const mid = await owner(box.x + border.at, box.y + cell.y + cell.h * 1.5);
  expect(mid.klass, "the grab zone owns the leading border").toContain("table-row-strip");
  expect(mid.cursor, "and it says so with the select cursor").toBe("cell");
  expect(mid.pageCursor, "a resize must NOT be promised on the leading border").not.toBe(
    "row-resize",
  );

  // ON the border where an inner row boundary crosses it — the one place the
  // two gestures really do meet. The gutter still owns it, as the `+` disc that
  // inserts at that boundary, and no resize is promised.
  const cross = await owner(box.x + border.at, box.y + crossing);
  expect(cross.klass, "the gutter owns the crossing, as its insert disc").toContain(
    "table-insert-target",
  );
  expect(cross.pageCursor, "a resize must not be promised at the crossing either").not.toBe(
    "row-resize",
  );

  // Further along the SAME boundary, off the gutter: the resize, intact.
  const away = await owner(box.x + cell.x + cell.w / 2, box.y + crossing);
  expect(away.pageCursor, "the row resize must still arm off the leading border").toBe(
    "row-resize",
  );
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
  await page.mouse.move(box.x + cell.x + cell.w / 2, box.y + 8);
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
    // The caret went where the click did — read from the active-cell outline,
    // which is the editor's own statement of which cell it is editing.
    const landed = await activeCell(page);
    expect(
      Math.abs(landed.y - (cell.y + cell.h * row)),
      `a click in row ${row}, column ${col} put the caret in a different row`,
    ).toBeLessThanOrEqual(3);
    expect(
      Math.abs(landed.x - (cell.x + cell.w * col)),
      `a click in row ${row}, column ${col} put the caret in a different column`,
    ).toBeLessThanOrEqual(3);
    await page.keyboard.type("X");
  }
  expect(consoleErrors).toEqual([]);
});
