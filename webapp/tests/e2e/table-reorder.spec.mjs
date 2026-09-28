// Reordering a table row or column — the gesture and the commands (`docs/141`
// §4.2.3, TBL-18).
//
// Every assertion here is on what a user does and what the DOCUMENT then says.
// The observable is the accessibility mirror, which is built from the model, so
// "the rows came back in the new order" is read from the document and not from
// the painted overlay. A guard that asserted `.table-move-indicator` exists
// would pass while the wrong row moved, or while nothing moved at all.
//
// The refusals are asserted too, and they are the point of the feature rather
// than its edge case: a torn vertical merge is silent data corruption, and the
// engine refuses it with a sentence naming the row. That sentence has to reach
// the reader — `runEdit` flattens engine errors into one generic line, which is
// right for internal vocabulary and wrong here.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  mirrorBlocks,
  openCommandPalette,
  stableBox,
  MOD,
} from "./fixtures.mjs";

/** Inserts a rows x cols table below a first paragraph, caret in cell (0,0). */
async function insertTable(page, rows = 3, cols = 3) {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await expect(page.locator("#insertTableMenu")).toBeVisible();
  await page.locator(`.gc[data-r="${rows}"][data-c="${cols}"]`).click();
  await expect(page.locator("#tabTable")).toBeEnabled();
}

/** The active cell outline, page-local — the caret's own cell. */
const activeCell = (page) =>
  page.evaluate(() => {
    const el = document.querySelector(".overlay .cell-outline");
    if (!el) return null;
    const sheet = document.querySelector(".page-wrap .page").getBoundingClientRect();
    const r = el.getBoundingClientRect();
    return {
      x: Math.round(r.x - sheet.x),
      y: Math.round(r.y - sheet.y),
      w: Math.round(r.width),
      h: Math.round(r.height),
    };
  });

/** The table's first cell, captured BEFORE anything is typed — the caret is in
 *  cell (0,0) then, so every other band is `first.y + first.h * row`. Read after
 *  typing it would be whichever cell the caret ended in, which is how a row
 *  index quietly becomes an offset from the wrong place. */
async function firstCell(page) {
  const cell = await activeCell(page);
  expect(cell, "no active cell after inserting the table").not.toBeNull();
  return cell;
}

/** Sweeps left of the table until the row strip arms, and returns that point.
 *  Probed the way a user finds it — by moving the pointer and seeing it appear —
 *  so a strip that exists in the geometry but never arms fails here. */
async function findRowStrip(page, cell, row) {
  const box = await stableBox(page.locator(".page-wrap .page").first());
  const y = box.y + cell.y + cell.h * row + cell.h / 2;
  await page.mouse.move(box.x + cell.x + cell.w / 2, y);
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => r())));
  for (let dx = 2; dx <= 26; dx += 2) {
    await page.mouse.move(box.x + cell.x - dx, y);
    await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => r())));
    if ((await page.locator(".overlay .table-row-strip").count()) > 0) {
      return { x: box.x + cell.x - dx, y };
    }
  }
  return null;
}

/** Types one string into each cell, Tab BETWEEN them and not after the last —
 *  Tab in the final cell appends a row, which would quietly change the table
 *  under every geometry calculation here. */
async function fillCells(page, texts) {
  for (const [index, text] of texts.entries()) {
    if (index > 0) await page.keyboard.press("Tab");
    await page.keyboard.type(text);
  }
}

/** `texts` in the order the DOCUMENT holds them, read from the accessibility
 *  mirror. The mirror renders a whole table as one block, so the order is the
 *  position of each string inside the joined text — not the order they were
 *  asked for, which is the mistake that makes this helper always agree with
 *  itself. */
async function orderOf(page, texts) {
  const joined = (await mirrorBlocks(page)).join("\n");
  return texts
    .map((text) => ({ text, at: joined.indexOf(text) }))
    .filter((entry) => entry.at >= 0)
    .sort((a, b) => a.at - b.at)
    .map((entry) => entry.text);
}

/** Runs a registry command by id from the palette. */
async function runPaletteCommand(page, id) {
  await openCommandPalette(page);
  await page.locator("#cmdList .cmd-item").first().waitFor();
  const row = page.locator(`#cmdList .cmd-item[data-command-id="${id}"]`);
  await expect(row, `no palette row for ${id}`).toHaveCount(1);
  await row.click();
  await expect(page.locator("#cmdPalette")).toBeHidden();
}

test("dragging the selected third row above the first reorders the DOCUMENT, in one undo entry", async ({
  page,
  consoleErrors,
}) => {
  await insertTable(page, 3, 1);
  const cell = await firstCell(page);
  await fillCells(page, ["Alpha", "Beta", "Gamma"]);
  await expect.poll(() => orderOf(page, ["Alpha", "Beta", "Gamma"])).toEqual(["Alpha", "Beta", "Gamma"]);

  // Docs' rule: the strip SELECTS on the first press, and only a band that is
  // already the selection is a handle. So a press-and-drag on an unselected
  // strip must still be the select-drag — proven by the row order surviving it.
  const third = await findRowStrip(page, cell, 2);
  expect(third, "no row strip was armed beside the third row").not.toBeNull();
  await page.mouse.move(third.x, third.y);
  await page.mouse.down();
  await page.mouse.move(third.x, third.y - cell.h * 2.5, { steps: 8 });
  await page.mouse.up();
  // Waited on the SELECT drag's own observable first — three rows of one column
  // selected — so "the order did not change" is read after the gesture has
  // finished rather than before it could have.
  await expect(page.locator(".overlay .table-cell-selection")).toHaveCount(3);
  expect(
    await orderOf(page, ["Alpha", "Beta", "Gamma"]),
    "a drag on an UNSELECTED strip must select, not reorder",
  ).toEqual(["Alpha", "Beta", "Gamma"]);

  // Select the third row, on its own.
  await page.mouse.click(third.x, third.y);
  await expect(page.locator(".overlay .table-cell-selection")).toHaveCount(1);

  // Now the band under the pointer is a handle: it says `grab` and says so.
  await page.mouse.move(third.x + 1, third.y);
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => r())));
  await expect(page.locator('.overlay .table-gutter-band[data-move="ready"]')).toHaveCount(1);

  const undoBefore = await page.locator("#undoBtn").getAttribute("aria-label");

  // Carry it above the first row. The indicator tracks the drop while the
  // pointer is still moving, which is the half a drag without one was missing.
  await page.mouse.down();
  await page.mouse.move(third.x, third.y - cell.h, { steps: 4 });
  await page.mouse.move(third.x, third.y - cell.h * 2.4, { steps: 8 });
  await expect(page.locator(".overlay .table-move-indicator")).toHaveCount(1);
  await page.mouse.up();

  // THE DOCUMENT. Not the overlay, not the selection: the rows come back in the
  // new order, read from the accessibility mirror the model builds.
  await expect
    .poll(() => orderOf(page, ["Alpha", "Beta", "Gamma"]))
    .toEqual(["Gamma", "Alpha", "Beta"]);
  await expect(page.locator("#status")).toContainText("moved to position 1");

  // ONE undo entry for the whole drag.
  await page.keyboard.press(`${MOD}+z`);
  await expect
    .poll(() => orderOf(page, ["Alpha", "Beta", "Gamma"]))
    .toEqual(["Alpha", "Beta", "Gamma"]);
  await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", undoBefore ?? "");

  // FORWARDS as well, and this direction is the one that can be written wrong
  // and still commit: the facade's `to` is the index AFTER the move, which is
  // one less than the drop boundary only when the band travels down. A row
  // carried to the bottom must land at the bottom, not one short of it and not
  // outside the table.
  const first = await findRowStrip(page, cell, 0);
  expect(first).not.toBeNull();
  await page.mouse.click(first.x, first.y);
  await expect(page.locator(".overlay .table-cell-selection")).toHaveCount(1);
  await page.mouse.move(first.x, first.y);
  await page.mouse.down();
  await page.mouse.move(first.x, first.y + cell.h * 3, { steps: 10 });
  await page.mouse.up();
  await expect
    .poll(() => orderOf(page, ["Alpha", "Beta", "Gamma"]))
    .toEqual(["Beta", "Gamma", "Alpha"]);
  await expect(page.locator("#status")).toContainText("moved to position 3");

  expect(consoleErrors).toEqual([]);
});

test("a drop where the row started says so and pushes no undo step", async ({
  page,
  consoleErrors,
}) => {
  await insertTable(page, 3, 1);
  const cell = await firstCell(page);
  await fillCells(page, ["Alpha", "Beta", "Gamma"]);

  const second = await findRowStrip(page, cell, 1);
  expect(second).not.toBeNull();
  await page.mouse.click(second.x, second.y);
  await expect(page.locator(".overlay .table-cell-selection")).toHaveCount(1);

  const undoBefore = await page.locator("#undoBtn").getAttribute("aria-label");
  await page.mouse.move(second.x, second.y);
  await page.mouse.down();
  // Wander inside the SAME band and come back — a real hand does not hold still.
  await page.mouse.move(second.x, second.y + 2, { steps: 3 });
  await page.mouse.move(second.x, second.y, { steps: 3 });
  await page.mouse.up();

  // The engine refuses a same-index move BY NAME, and the sentence is shown
  // rather than swallowed into "that edit isn't supported for this selection".
  await expect(page.locator("#status")).toContainText("already at that position");
  await expect(page.locator("#status")).not.toContainText("isn't supported");
  await expect.poll(() => orderOf(page, ["Alpha", "Beta", "Gamma"])).toEqual(["Alpha", "Beta", "Gamma"]);
  // No undo step: a gesture that changed nothing must leave the history alone.
  await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", undoBefore ?? "");

  expect(consoleErrors).toEqual([]);
});

test("a row inside a vertical merge refuses with the reason, naming the row", async ({
  page,
  consoleErrors,
}) => {
  await insertTable(page, 3, 2);
  const cell = await firstCell(page);
  await fillCells(page, ["Alpha", "A2", "Beta", "B2", "Gamma", "G2"]);

  const box = await stableBox(page.locator(".page-wrap .page").first());
  const centre = (row, col) => ({
    x: box.x + cell.x + cell.w * col + cell.w / 2,
    y: box.y + cell.y + cell.h * row + cell.h / 2,
  });

  // Merge the first column's first two cells vertically, which is exactly the
  // `vMerge` run a row move would tear.
  const a = centre(0, 0);
  const b = centre(1, 0);
  await page.mouse.move(a.x, a.y);
  await page.mouse.down();
  await page.mouse.move(b.x, b.y, { steps: 10 });
  await page.mouse.up();
  await expect(page.locator(".overlay .table-cell-selection")).toHaveCount(2);
  await page.locator('[data-tab="table"]').click();
  await page.locator("#mergeCellsBtn").click();

  // Put the caret in the merged run's first row and ask to move it down.
  await page.mouse.click(a.x, a.y);
  await runPaletteCommand(page, "table.move.rowDown");

  await expect(page.locator("#status")).toContainText("vertically merged");
  await expect(page.locator("#status")).not.toContainText("isn't supported");
  await expect.poll(() => orderOf(page, ["Alpha", "Beta", "Gamma"])).toEqual(["Alpha", "Beta", "Gamma"]);

  expect(consoleErrors).toEqual([]);
});

test("a column move on a table with a horizontal merge is refused BEFORE it is offered", async ({
  page,
  consoleErrors,
}) => {
  await insertTable(page, 2, 3);
  const cell = await firstCell(page);
  await fillCells(page, ["A1", "B1", "C1", "A2", "B2", "C2"]);

  const box = await stableBox(page.locator(".page-wrap .page").first());
  const centre = (row, col) => ({
    x: box.x + cell.x + cell.w * col + cell.w / 2,
    y: box.y + cell.y + cell.h * row + cell.h / 2,
  });

  // A `gridSpan` in the second row: the grid index a drag reports then names no
  // cell at all, which is why the engine refuses rather than guessing.
  const a = centre(1, 0);
  const b = centre(1, 1);
  await page.mouse.move(a.x, a.y);
  await page.mouse.down();
  await page.mouse.move(b.x, b.y, { steps: 10 });
  await page.mouse.up();
  await expect(page.locator(".overlay .table-cell-selection")).toHaveCount(2);
  await page.locator('[data-tab="table"]').click();
  await page.locator("#mergeCellsBtn").click();

  await page.mouse.click(centre(0, 1).x, centre(0, 1).y);

  // `gridSpan` makes the grid index a drag reports name no cell, so the engine
  // refuses it by row. The UI refuses EARLIER, which is better and is the rule
  // this repository keeps: the command is present and disabled CARRYING ITS
  // REASON, not absent and not silently doing nothing.
  await openCommandPalette(page);
  await page.locator("#cmdList .cmd-item").first().waitFor();
  const move = page.locator('#cmdList .cmd-item[data-command-id="table.move.columnLeft"]');
  await expect(move).toHaveCount(1);
  await expect(move).toBeDisabled();
  await expect(move).toHaveAttribute("title", /merged or spanned/i);
  await page.keyboard.press("Escape");

  // …and the column strip is not painted on such a table either, so the gesture
  // cannot be started from the gutter: the document is unchanged either way.
  await expect.poll(() => orderOf(page, ["A1", "B1", "C1"])).toEqual(["A1", "B1", "C1"]);

  expect(consoleErrors).toEqual([]);
});

test("the command twin moves the caret's row, so reordering is not mouse-only", async ({
  page,
  consoleErrors,
}) => {
  await insertTable(page, 3, 1);
  const cell = await firstCell(page);
  await fillCells(page, ["Alpha", "Beta", "Gamma"]);

  const box = await stableBox(page.locator(".page-wrap .page").first());
  // Caret into the middle row.
  await page.mouse.click(box.x + cell.x + cell.w / 2, box.y + cell.y + cell.h * 1.5);

  await runPaletteCommand(page, "table.move.rowUp");
  await expect
    .poll(() => orderOf(page, ["Alpha", "Beta", "Gamma"]))
    .toEqual(["Beta", "Alpha", "Gamma"]);

  expect(consoleErrors).toEqual([]);
});
