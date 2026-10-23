// The table chrome's TARGETS, measured against what they promise (`docs/171`
// §7, HF-341..HF-346).
//
// Every assertion here compares two things the user can perceive, never a
// constant with itself: the box a `+` disc PAINTS against the box a press
// INSERTS over; the box a strip occupies against the line box of the text above
// it; the strip that is lit against where the pointer is after a scroll; the
// sentence in the live region against the edit that ran. Each was seen red
// against the defect it guards (the commit message carries the mutations).
import {
  clickIntoFirstPage,
  expect,
  gotoEditor,
  moveCaretToDocStart,
  runAppMenuCommand,
  runPaletteCommand,
  stableBox,
  test,
} from "./fixtures.mjs";

/** Inserts a `rows` x `cols` table below a first paragraph. */
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

/** The client rect of the caret's cell, the first cell after an insert. */
const firstCell = (page) =>
  page.evaluate(() => {
    const r = document.querySelector(".overlay .cell-outline").getBoundingClientRect();
    return { x: r.x, y: r.y, w: r.width, h: r.height };
  });

/** Client rects of every overlay child matching `selector`. */
const rects = (page, selector) =>
  page.evaluate(
    (sel) =>
      [...document.querySelectorAll(sel)].map((el) => {
        const r = el.getBoundingClientRect();
        return { x: r.x, y: r.y, w: r.width, h: r.height };
      }),
    selector,
  );

const frame = (page) => page.evaluate(() => new Promise((r) => requestAnimationFrame(() => r())));

async function hover(page, x, y) {
  await page.mouse.move(x, y);
  await frame(page);
}

/** Whether two rects share any area (touching edges do not count). */
const intersects = (a, b) =>
  a.x < b.x + b.w - 0.01 && b.x < a.x + a.w - 0.01 && a.y < b.y + b.h - 0.01 && b.y < a.y + a.h - 0.01;

/** How many rows the mirror shows for the table holding `marker`. */
const mirrorRows = (page, marker) =>
  page.evaluate((text) => {
    const table = [...document.querySelectorAll("#a11yDocument table")].find((t) =>
      t.textContent.includes(text),
    );
    return table ? table.querySelectorAll("tr").length : -1;
  }, marker);

test("the + disc inserts over every pixel it paints, and is a 24px target (HF-341)", async ({
  page,
  consoleErrors,
}) => {
  await insertTable(page, 3, 3);
  await page.keyboard.type("MARK");
  const cell = await firstCell(page);
  // Rows are ~19px: shorter than the disc, so the NEXT boundary is nearer than
  // this disc's own centre over part of its box. That is the case where the
  // painted box and the live box used to disagree.
  const boundaryY = cell.y + cell.h;
  await hover(page, cell.x - 36, boundaryY);
  const [disc] = await rects(page, ".overlay .table-insert-target");
  expect(disc, "a disc must show in the lane left of the strip").toBeTruthy();
  expect(disc.w).toBeGreaterThanOrEqual(24);
  expect(disc.h).toBeGreaterThanOrEqual(24);
  expect(Math.abs(disc.y + disc.h / 2 - boundaryY)).toBeLessThanOrEqual(1);

  // Every point of the painted disc keeps THIS disc armed: same box, and the
  // browser hit-tests the disc element itself.
  for (const fy of [0.1, 0.3, 0.5, 0.7, 0.9]) {
    for (const fx of [0.1, 0.5, 0.9]) {
      const px = disc.x + disc.w * fx;
      const py = disc.y + disc.h * fy;
      await hover(page, px, py);
      const now = await rects(page, ".overlay .table-insert-target");
      expect(now, `the disc moved when the pointer was at ${fx},${fy} of it`).toEqual([disc]);
      // Inside the painted circle the browser hit-tests the disc itself; its
      // unpainted corners still answer for it through the gutter's geometry.
      if (Math.hypot(fx - 0.5, fy - 0.5) * disc.w > disc.w / 2 - 1) continue;
      const owner = await page.evaluate(
        ([x, y]) => document.elementFromPoint(x, y)?.className ?? "",
        [px, py],
      );
      expect(owner).toContain("table-insert-target");
    }
  }

  // The press on the disc's LOWER part, where a row selection used to answer.
  const rowsBefore = await mirrorRows(page, "MARK");
  await page.mouse.click(disc.x + disc.w / 2, disc.y + disc.h * 0.9);
  await expect(page.locator("#status")).toContainText("Row inserted");
  await expect(page.locator(".overlay .table-cell-selection")).toHaveCount(0);
  await expect.poll(() => mirrorRows(page, "MARK")).toBe(rowsBefore + 1);
  expect(consoleErrors).toEqual([]);
});

test("the row strip, the disc lane and a column border are 24px targets that never overlap (HF-342)", async ({
  page,
  consoleErrors,
}) => {
  await insertTable(page, 3, 3);
  const cell = await firstCell(page);
  await hover(page, cell.x + cell.w / 2, cell.y + cell.h * 1.5);
  const [strip] = await rects(page, ".overlay .table-row-strip");
  expect(strip, "the row strip shows for a pointer in the table").toBeTruthy();
  expect(strip.w).toBeGreaterThanOrEqual(24);

  // A column border between two 226px columns: the span over which the
  // cursor promises a resize, measured by walking across it.
  const borderX = cell.x + cell.w;
  const y = cell.y + cell.h * 1.5;
  const armed = [];
  for (let dx = -20; dx <= 20; dx += 1) {
    await hover(page, borderX + dx, y);
    const cursor = await page.evaluate(
      () => getComputedStyle(document.querySelector(".page-wrap canvas.page")).cursor,
    );
    if (cursor === "col-resize") armed.push(dx);
  }
  expect(armed.length, `the border arms over ${armed.length}px`).toBeGreaterThanOrEqual(24);

  // The resting grip on the caret's table is exactly that zone, not wider.
  const [grip] = await rects(page, ".overlay .table-col-resize-handle");
  expect(Math.round(grip.w)).toBe(armed.length - 1);

  // No painted gutter element overlaps another gutter element.
  await hover(page, cell.x - 36, cell.y + cell.h);
  const all = await rects(
    page,
    ".overlay .table-row-strip, .overlay .table-column-strip, .overlay .table-insert-target",
  );
  for (let i = 0; i < all.length; i++) {
    for (let j = i + 1; j < all.length; j++) {
      expect(intersects(all[i], all[j]), `targets ${i} and ${j} overlap`).toBe(false);
    }
  }
  expect(consoleErrors).toEqual([]);
});

test("the row strip arms on approach from the margin, before the pointer enters the table (HF-343)", async ({
  page,
  consoleErrors,
}) => {
  await insertTable(page, 3, 3);
  const cell = await firstCell(page);
  // The pointer starts well clear of the table and never enters it.
  await hover(page, cell.x - 90, cell.y + cell.h * 1.5);
  await expect(page.locator(".overlay .table-row-strip")).toHaveCount(0);
  let shown = null;
  for (let x = cell.x - 90; x < cell.x - 2; x += 3) {
    await hover(page, x, cell.y + cell.h * 1.5);
    if ((await page.locator(".overlay .table-row-strip").count()) > 0) {
      shown = x;
      break;
    }
  }
  expect(shown, "the row strip never showed on the way in from the margin").not.toBeNull();
  expect(shown).toBeLessThan(cell.x - 20);
  // And a press there selects the row the pointer is level with: a real target.
  await page.mouse.click(cell.x - 10, cell.y + cell.h * 1.5);
  await expect(page.locator(".overlay .table-cell-selection")).toHaveCount(3);
  expect(consoleErrors).toEqual([]);
});

test("the column strip never covers the line of text above the table (HF-344)", async ({
  page,
  consoleErrors,
}) => {
  await insertTable(page, 3, 3);
  const cell = await firstCell(page);
  // The line box of the paragraph above: its selection highlight, which fills
  // the whole line box.
  await moveCaretToDocStart(page);
  await page.keyboard.press("Shift+End");
  const lines = await rects(page, ".overlay .highlight");
  expect(lines.length).toBeGreaterThan(0);
  await page.keyboard.press("ArrowRight");

  const chrome = ".overlay .table-column-strip, .overlay .table-insert-target, .overlay .table-row-strip";
  // In the table, on the top border, and along the gap above it: whatever is
  // shown stays clear of every line box.
  for (const [x, yy] of [
    [cell.x + cell.w / 2, cell.y + cell.h / 2],
    [cell.x + cell.w / 2, cell.y + 1],
    [cell.x + cell.w, cell.y - 4],
    [cell.x + cell.w * 2, cell.y - 8],
  ]) {
    await hover(page, x, yy);
    const shown = await rects(page, chrome);
    expect(shown.length).toBeGreaterThan(0);
    for (const box of shown) {
      for (const line of lines) {
        expect(intersects(box, line), `chrome ${JSON.stringify(box)} covers the line ${JSON.stringify(line)}`).toBe(false);
      }
    }
  }
  // ON the line above, nothing is offered and a press places the caret there.
  const last = lines[lines.length - 1];
  await hover(page, cell.x + cell.w / 2, last.y + last.h - 2);
  await expect(page.locator(".overlay .table-column-strip")).toHaveCount(0);
  await page.mouse.click(cell.x + cell.w / 2, last.y + last.h - 2);
  await expect(page.locator(".overlay .table-cell-selection")).toHaveCount(0);
  await expect(page.locator("#tabTable")).toBeDisabled();
  expect(consoleErrors).toEqual([]);
});

test("a wheel scroll and a zoom re-place the hover instead of leaving it lit (HF-345)", async ({
  page,
  consoleErrors,
}) => {
  await insertTable(page, 3, 3);
  const cell = await firstCell(page);
  const at = { x: cell.x - 10, y: cell.y + cell.h * 1.5 };
  await hover(page, at.x, at.y);
  await expect(page.locator(".overlay .table-row-strip")).toHaveCount(1);
  await expect(page.locator(".overlay .table-gutter-hover")).toHaveCount(1);

  // The page moves 300px under a pointer that does not: no pointer event fires.
  await page.mouse.wheel(0, 300);
  await expect
    .poll(() => page.evaluate(() => document.getElementById("viewport").scrollTop))
    .toBeGreaterThan(0);
  await frame(page);
  // The table went 300px up; the pointer is beside nothing, so nothing is lit.
  await expect(page.locator(".overlay .table-gutter-hover")).toHaveCount(0);
  await expect(page.locator(".overlay .table-row-strip")).toHaveCount(0);

  // Back, re-arm, then zoom with the pointer still: the hover is re-resolved
  // at the new scale, so a lit band is level with the pointer, or there is none.
  await page.mouse.wheel(0, -300);
  await frame(page);
  const again = await firstCell(page);
  const back = { x: again.x - 10, y: again.y + again.h * 1.5 };
  await hover(page, back.x, back.y);
  await expect(page.locator(".overlay .table-gutter-hover")).toHaveCount(1);
  await page.evaluate(() => document.getElementById("zoomIn").click());
  await expect(page.locator("#zoom")).not.toHaveValue("100%");
  await frame(page);
  for (const box of await rects(page, ".overlay .table-gutter-hover")) {
    // A row band is a vertical line on the left border, a column band a
    // horizontal one on the top border: the pointer is level with the band and
    // within the gutter's two lanes of its border.
    const level =
      box.h > box.w
        ? back.y >= box.y && back.y <= box.y + box.h && Math.abs(back.x - box.x) <= 48
        : back.x >= box.x && back.x <= box.x + box.w && Math.abs(back.y - box.y) <= 48;
    expect(level, `after a zoom the band lit at ${JSON.stringify(box)} is not level with ${JSON.stringify(back)}`).toBe(true);
  }
  expect(consoleErrors).toEqual([]);
});

test("structural edits from the menu and the palette are announced (HF-346)", async ({
  page,
  consoleErrors,
}) => {
  await insertTable(page, 3, 3);
  await page.keyboard.type("MARK");
  // The polite live region is what a screen reader hears; the status line is
  // its visible twin. Both must carry the sentence.
  const said = async (text) => {
    await expect(page.locator("#status")).toHaveText(text);
    await expect(page.locator("#statusLiveRegion")).toContainText(text);
  };
  // The ribbon band, which ran the same edits through its own buttons.
  await page.locator("#tabTable").click();
  await page.locator('.table-ribbon [data-table-action="insert-row-above"]').click();
  await said("Row inserted");
  await expect.poll(() => mirrorRows(page, "MARK")).toBe(4);
  await runAppMenuCommand(page, "table", "table.insert.rowBelow");
  await said("Row inserted");
  await expect.poll(() => mirrorRows(page, "MARK")).toBe(5);
  await runAppMenuCommand(page, "table", "table.insert.columnRight");
  await said("Column inserted");
  await runPaletteCommand(page, "table.delete.column", "delete column");
  await said("Column deleted");
  await runPaletteCommand(page, "table.delete.row", "delete row");
  await said("Row deleted");
  expect(consoleErrors).toEqual([]);
});
