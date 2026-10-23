// Every table operation, in the real editor, against tables with merged cells
// (`docs/171`). "For table our current approach fails in case of merged cells"
// — the owner. The engine's matrix (`table_merge_matrix_tests.rs`) drives every
// operation on every cell of every family; these specs drive one
// representative of each operation FAMILY through the surfaces a person uses
// (keyboard, palette, menu, pointer) and read the result where a reader meets
// it: the accessibility mirror's `<td>` grid, with its colspan/rowspan, and the
// SAVED file.
//
// `fixtures/generated/merged-tables.docx` holds one table per family; every
// cell's text names the grid rectangle it covers (`r0-2c1` is rows 0-2 of
// column 1), so a grid read back as text says whether the merge survived.
import { readFile } from "node:fs/promises";
import { test, expect, gotoEditor, stableBox, runPaletteCommand, MOD } from "./fixtures.mjs";
import { opcPart } from "./opc-part.mjs";

const FIXTURE = "../fixtures/generated/merged-tables.docx";
const TABLE = { H: 0, V: 1, COMBINED: 2, EDGES: 3, RAGGED: 4, NESTED: 5 };

async function openFixture(page) {
  await page.setViewportSize({ width: 1280, height: 1000 });
  await gotoEditor(page);
  await page.locator("#file").setInputFiles(FIXTURE);
  await expect(page.locator("#a11yDocument")).toContainText("NESTED", { timeout: 60_000 });
}

/** Puts the caret at the start of the `nth` (0-based, document order) match of
 *  `word`, with a click where Find draws it. Cell texts repeat across the
 *  fixture's tables, so the occurrence picks the table. */
async function caretAt(page, word, nth = 0) {
  await page.keyboard.press(`${MOD}+f`);
  await page.locator("#findInput").fill(word);
  // Find may open on the match nearest the caret, so step until the status names
  // the wanted occurrence rather than counting presses from wherever it opened.
  const status = page.locator("#findStatus");
  await expect(status).toHaveText(/^\d+ (match|of \d+)$/);
  const wanted = new RegExp(`^(${nth + 1} of \\d+|1 match)$`);
  for (let i = 0; i < 12 && !wanted.test((await status.textContent()) ?? ""); i++) {
    await page.locator("#findInput").press("Enter");
  }
  await expect(status).toHaveText(wanted);
  await page.keyboard.press("Escape");
  const box = await stableBox(page.locator(".overlay .highlight").first());
  await page.mouse.click(box.x + 1, box.y + box.height / 2);
}

/** Table `index` of the mirror (top-level tables in order) as rows of
 *  `text:colspan:rowspan`, spans omitted when 1. */
function grid(page, index) {
  return page.evaluate((index) => {
    const tables = [...document.querySelectorAll("#a11yDocument table")].filter(
      (table) => !table.parentElement?.closest("table"),
    );
    const table = tables[index];
    if (!table) return null;
    return [...table.querySelectorAll(":scope > thead > tr, :scope > tbody > tr")].map((tr) =>
      [...tr.children]
        .map((td) => {
          const spans = `${td.colSpan > 1 ? `:c${td.colSpan}` : ""}${td.rowSpan > 1 ? `:r${td.rowSpan}` : ""}`;
          return `${td.textContent.trim()}${spans}`;
        })
        .join(" "),
    );
  }, index);
}

/** Every `w:t` of the saved body, in order, plus each gridSpan/vMerge marker. */
async function savedTableXml(page) {
  const download = page.waitForEvent("download");
  await page.keyboard.press(`${MOD}+s`);
  return opcPart(await readFile(await (await download).path()), "word/document.xml");
}

test("the mirror announces merged cells once, with their spans", async ({ page, consoleErrors }) => {
  await openFixture(page);
  expect(await grid(page, TABLE.V)).toEqual([
    "r0c0 r0-2c1:r3 r0c2 r0c3",
    "r1c0 r1c2 r1-2c3:r2",
    "r2c0 r2c2",
    "r3c0 r3c1 r3c2 r3c3",
  ]);
  expect(await grid(page, TABLE.H)).toEqual([
    "r0c0 r0c1-2:c2 r0c3",
    "r1c0 r1c1 r1c2 r1c3",
    "r2c0-1:c2 r2c2-3:c2",
    "r3c0 r3c1 r3c2 r3c3",
  ]);
  expect(consoleErrors).toEqual([]);
});

test("Tab visits a merged cell once and selects each cell's contents", async ({
  page,
  consoleErrors,
}) => {
  await openFixture(page);
  await caretAt(page, "r1c0", 1);
  // r1c0 -> r1c2: the covered continuation of r0-2c1 between them is skipped.
  await page.keyboard.press("Tab");
  // The destination's contents are SELECTED, so typing replaces them (OL-03).
  await page.keyboard.type("X");
  expect((await grid(page, TABLE.V))[1]).toBe("r1c0 X r1-2c3:r2");
  // Shift+Tab back over the same gap, and typing replaces r1c0.
  await page.keyboard.press("Shift+Tab");
  await page.keyboard.type("Y");
  expect((await grid(page, TABLE.V))[1]).toBe("Y X r1-2c3:r2");
  expect(consoleErrors).toEqual([]);
});

test("a row inserted below the top of a vertical merge extends the merge", async ({
  page,
  consoleErrors,
}) => {
  await openFixture(page);
  await caretAt(page, "r0c0", 1);
  await runPaletteCommand(page, "table.insert.rowBelow", "row below");
  const rows = await grid(page, TABLE.V);
  expect(rows[0]).toBe("r0c0 r0-2c1:r4 r0c2 r0c3");
  expect(rows).toHaveLength(5);
  // Undo restores the three-row merge exactly; redo re-applies it.
  await page.keyboard.press(`${MOD}+z`);
  expect((await grid(page, TABLE.V))[0]).toBe("r0c0 r0-2c1:r3 r0c2 r0c3");
  await page.keyboard.press(`${MOD}+Shift+z`);
  expect((await grid(page, TABLE.V))[0]).toBe("r0c0 r0-2c1:r4 r0c2 r0c3");
  expect(consoleErrors).toEqual([]);
});

test("deleting the top row of a vertical merge keeps the merged cell's text", async ({
  page,
  consoleErrors,
}) => {
  await openFixture(page);
  await caretAt(page, "r0c0", 1);
  await runPaletteCommand(page, "table.delete.row", "delete row");
  expect(await grid(page, TABLE.V)).toEqual([
    "r1c0 r0-2c1:r2 r1c2 r1-2c3:r2",
    "r2c0 r2c2",
    "r3c0 r3c1 r3c2 r3c3",
  ]);
  expect(consoleErrors).toEqual([]);
});

test("a column inserted through a horizontal merge widens it; delete shrinks it back", async ({
  page,
  consoleErrors,
}) => {
  await openFixture(page);
  await caretAt(page, "r1c1");
  await runPaletteCommand(page, "table.insert.columnRight", "column right");
  const rows = await grid(page, TABLE.H);
  // Row 0's r0c1-2 straddled the new line and now covers it; row 2's two merges
  // likewise; row 1 and row 3 gained a cell.
  expect(rows[0]).toBe("r0c0 r0c1-2:c3 r0c3");
  expect(rows[1]).toBe("r1c0 r1c1  r1c2 r1c3");
  // The caret is in the new cell: deleting its column restores the table.
  await runPaletteCommand(page, "table.delete.column", "delete column");
  expect((await grid(page, TABLE.H))[0]).toBe("r0c0 r0c1-2:c2 r0c3");
  expect(consoleErrors).toEqual([]);
});

test("a table that holds merges merges again, and a merged cell unmerges", async ({
  page,
  consoleErrors,
}) => {
  await openFixture(page);
  await caretAt(page, "r3c0", 1);
  await runPaletteCommand(page, "table.select.row", "select row");
  await runPaletteCommand(page, "table.merge", "merge cells");
  expect((await grid(page, TABLE.V))[3]).toBe("r3c0 r3c1 r3c2 r3c3:c4");
  await caretAt(page, "r0-2c1");
  await runPaletteCommand(page, "table.unmerge", "unmerge");
  expect((await grid(page, TABLE.V)).slice(0, 3)).toEqual([
    "r0c0 r0-2c1 r0c2 r0c3",
    "r1c0  r1c2 r1-2c3:r2",
    "r2c0  r2c2",
  ]);
  expect(consoleErrors).toEqual([]);
});

test("Select column works on a merged table and takes each merged cell once", async ({
  page,
  consoleErrors,
}) => {
  await openFixture(page);
  // Column 1 of V-MERGE: the three-row merged cell r0-2c1 and r3c1 — two cells,
  // the merged one once. It used to select nothing on any merged table. Read off
  // the mirror's ARIA grid, which marks the selected cells in GRID columns.
  await caretAt(page, "r3c1", 1);
  await runPaletteCommand(page, "table.select.column", "select column");
  const selected = () =>
    page.evaluate(() =>
      [...document.querySelectorAll('#a11yDocument [aria-selected="true"]')].map((td) =>
        td.textContent.trim(),
      ),
    );
  await expect.poll(selected).toEqual(["r0-2c1", "r3c1"]);
  // A selection is ONE rectangle (`table_range.mjs`, Docs' model), grown to
  // whole merged cells: in H-MERGE the merges chain across every column, so a
  // column there is the whole table, and the status says it grew.
  await caretAt(page, "r1c1");
  await runPaletteCommand(page, "table.select.column", "select column");
  await expect.poll(async () => (await selected()).length).toBe(13);
  expect(consoleErrors).toEqual([]);
});

test("merges survive a DOCX save and reopen", async ({ page, consoleErrors }) => {
  await openFixture(page);
  await caretAt(page, "r0c0", 1);
  await runPaletteCommand(page, "table.insert.rowBelow", "row below");
  const xml = await savedTableXml(page);
  // The V-MERGE table now carries a four-row merge: one restart, three continues.
  const vTable = xml.split("<w:tbl>")[2];
  expect((vTable.match(/<w:vMerge w:val="restart"\/>/g) ?? []).length).toBe(2);
  expect((vTable.match(/<w:vMerge\/>/g) ?? []).length).toBe(4);
  expect(consoleErrors).toEqual([]);
});

/** The active cell's outline, page-local px. */
async function activeCell(page) {
  await expect(page.locator(".overlay .cell-outline")).toHaveCount(1);
  return page.evaluate(() => {
    const r = document.querySelector(".overlay .cell-outline").getBoundingClientRect();
    const p = document.querySelector(".page-wrap .page").getBoundingClientRect();
    return { x: r.x - p.x, y: r.y - p.y, w: r.width, h: r.height };
  });
}

test("a column border a merged cell crosses drags, and the merged cell keeps its width", async ({
  page,
  consoleErrors,
}) => {
  await openFixture(page);
  // H-MERGE: the border between grid columns 1 and 2 exists in rows 1 and 3 and
  // is crossed by r0c1-2. It used to be disarmed for the whole table.
  await caretAt(page, "r0c1-2");
  const merged = await activeCell(page);
  await caretAt(page, "r1c1");
  const cell = await activeCell(page);
  const sheet = () => stableBox(page.locator(".page-wrap .page").first());
  let box = await sheet();
  let found = null;
  for (let x = cell.x + cell.w - 8; x <= cell.x + cell.w + 8 && found === null; x += 1) {
    await page.mouse.move(box.x + x, box.y + cell.y + cell.h / 2);
    await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => r())));
    const target = await page.evaluate(
      () => document.querySelector(".page-wrap .page")?.dataset.pointerTarget ?? "",
    );
    if (target === "table-column-boundary") found = x;
  }
  expect(found, "the border inside the merged table is armed").not.toBeNull();
  box = await sheet();
  await page.mouse.move(box.x + found, box.y + cell.y + cell.h / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + found + 30, box.y + cell.y + cell.h / 2, { steps: 6 });
  await page.mouse.up();
  await caretAt(page, "r1c1");
  const after = await activeCell(page);
  expect(after.w, "the column grew").toBeGreaterThan(cell.w + 15);
  await caretAt(page, "r0c1-2");
  const mergedAfter = await activeCell(page);
  expect(
    Math.abs(mergedAfter.w - merged.w),
    "the merged cell spans both columns, so its total width is unchanged",
  ).toBeLessThan(4);
  expect(consoleErrors).toEqual([]);
});
