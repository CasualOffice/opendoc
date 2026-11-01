// The table tools (`docs/153` table rows), driven as a person drives them: from
// the ribbon, the keyboard and the palette, against the real engine, and read
// back from the model-derived accessibility mirror rather than from a function
// returning Ok.
//
// The command tree and the dialogs' pure rules are `tests/table_tools.test.mjs`;
// the engine's own rules are `casual-doc-command`'s `table_tools` tests.
import { test, expect, gotoEditor, clickIntoFirstPage, stableBox, runPaletteCommand, MOD } from "./fixtures.mjs";

const MARK = "TTMARK";

async function focusActiveCell(page) {
  const box = await stableBox(page.locator(".cell-outline"));
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  await expect(page.locator(".cell-outline")).toBeVisible();
}

/** A `rows x cols` table from the grid, its first cell marked. */
async function insertTable(page, rows, cols, { open = true } = {}) {
  if (open) {
    await gotoEditor(page);
    await clickIntoFirstPage(page);
  }
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await expect(page.locator("#insertTableMenu")).toBeVisible();
  await page.locator(`.gc[data-r="${rows}"][data-c="${cols}"]`).click();
  await expect(page.locator(".cell-outline")).toBeVisible();
  await focusActiveCell(page);
  await page.keyboard.type(MARK);
  await expect(page.locator("#tabTable")).toBeEnabled();
}

/** The cell texts of the mirror table holding `mark`, row by row, its own rows only. */
async function modelCells(page, mark = MARK) {
  return page.evaluate((needle) => {
    const tables = [...document.querySelectorAll("#a11yDocument table")];
    const table = tables.reverse().find((candidate) => candidate.textContent.includes(needle));
    if (!table) return null;
    return [...table.querySelectorAll("tr")]
      .filter((row) => row.closest("table") === table)
      .map((row) =>
        [...row.children].filter((cell) => cell.closest("table") === table).map((cell) => cell.textContent.trim()),
      );
  }, mark);
}

// Read two ways, neither of which is the nested `<table>` the first version of
// this test looked for: the accessibility projection (`accessibility_tree`) has
// never nested a table — a cell is its plain text, so the `rich` fixture's own
// nested table reads "Outer Nested A Nested B" — and that test could not pass.
//   * the PAINT: the new table's active cell lies inside the outer cell's
//     column and is narrower than it (two columns share that cell's width),
//     where a body-level 2 x 2 table's cell is as wide as the outer one;
//   * the MODEL, through the mirror: one table holds the mark, and its first
//     cell holds both marks as separate blocks (the nested table's cell text),
//     not one run typed twice into one paragraph.
// Mutation: make `table_insertion_point` return the body branch for a caret in
// a cell; the new table lands after the outer one, full width, and this goes red.
test("Insert table from inside a cell nests the new table in that cell", async ({ page }) => {
  await insertTable(page, 2, 2);
  const outerCell = await stableBox(page.locator(".cell-outline"));
  await insertTable(page, 2, 2, { open: false });
  const innerCell = await stableBox(page.locator(".cell-outline"));
  expect(innerCell.x, "the new table starts inside the outer cell").toBeGreaterThanOrEqual(outerCell.x - 1);
  expect(innerCell.x + innerCell.width, "and ends inside it").toBeLessThanOrEqual(outerCell.x + outerCell.width + 1);
  expect(innerCell.width, "and its cell shares the outer cell's width").toBeLessThan(outerCell.width * 0.75);
  const reading = await page.evaluate((needle) => {
    const tables = [...document.querySelectorAll("#a11yDocument table")].filter((table) =>
      table.textContent.includes(needle),
    );
    return { tables: tables.length, firstCell: tables[0]?.querySelector("td")?.textContent ?? "" };
  }, MARK);
  expect(reading, "the second table sits inside the first table's cell").toEqual({
    tables: 1,
    firstCell: `${MARK} ${MARK}`,
  });
});

// Mutation: drop the `insert.tableDialog` row from INSERT_SURFACE; the ribbon
// button is never enabled or wired and the first click times out.
test("Insert table by size inserts the size typed, past the 8 x 10 grid", async ({ page }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableDialogBtn").click();
  const dialog = page.locator("#tableInsertDialog");
  await expect(dialog).toBeVisible();
  await page.locator("#tableInsertColumns").fill("25");
  await dialog.locator("[data-dialog-apply]").click();
  await expect(dialog, "an out-of-range count keeps the dialog open").toBeVisible();
  await page.locator("#tableInsertColumns").fill("12");
  await page.locator("#tableInsertRows").fill("3");
  await dialog.locator("[data-dialog-apply]").click();
  await expect(dialog).toBeHidden();
  await expect(page.locator("#tableContext")).toContainText("12 × 3");

  // The palette reaches the same dialog.
  await focusActiveCell(page);
  await runPaletteCommand(page, "insert.tableDialog", "insert table by size");
  await expect(dialog).toBeVisible();
});

// Mutation: remove the `⌘⇧⏎` row from KEYMAP; the chord types nothing table-
// shaped and the first table keeps three rows.
test("Ctrl/Cmd+Shift+Enter splits the table at the caret's row", async ({ page }) => {
  await insertTable(page, 3, 2);
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press(`${MOD}+Shift+Enter`);
  await expect(page.locator("#status")).toContainText("Table split");
  expect(await modelCells(page)).toEqual([[MARK, ""]]);
});

// Mutation: make `split_inlines` ignore the comma (return the whole paragraph as
// one piece); the table is one column wide and the cell reading goes red.
test("Convert text to table, then back to text with another separator", async ({ page }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await page.keyboard.type("QX1,QX2,QX3");
  await page.keyboard.press("Enter");
  await page.keyboard.type("QX4,QX5");
  await page.keyboard.press("Shift+Home");
  await page.keyboard.press("Shift+ArrowUp");
  await page.keyboard.press("Shift+Home");
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#textToTableBtn").click();
  const dialog = page.locator("#textToTableDialog");
  await expect(dialog).toBeVisible();
  await dialog.locator('input[name="textToTableSeparator"][value="comma"]').check();
  await expect(page.locator("#textToTableColumns"), "the column count follows the separator").toHaveValue("3");
  await dialog.locator("[data-dialog-apply]").click();
  await expect(dialog).toBeHidden();
  expect(await modelCells(page, "QX1")).toEqual([
    ["QX1", "QX2", "QX3"],
    ["QX4", "QX5", ""],
  ]);

  await page.locator("#tabTable").click();
  await page.locator("#tableToTextBtn").click();
  const back = page.locator("#tableToTextDialog");
  await expect(back).toBeVisible();
  await back.locator('input[name="tableToTextSeparator"][value="semicolon"]').check();
  await back.locator("[data-dialog-apply]").click();
  await expect(back).toBeHidden();
  await expect(page.locator("#a11yDocument")).toContainText("QX1;QX2;QX3");
  expect(await modelCells(page, "QX1")).toBeNull();
});

// Mutation: invert `LookOption::is_on` for HeaderRow in `table_tools.rs`; the
// box reads checked before it is clicked and the first assertion goes red.
test("the style options are checkboxes on the Table band, and a click turns one on", async ({ page }) => {
  await insertTable(page, 2, 2);
  await page.locator("#tabTable").click();
  await page.locator("#tableLookBtn").click();
  const menu = page.locator("#tableLookMenu");
  await expect(menu.locator('[role="menuitemcheckbox"]')).toHaveCount(6);
  const header = menu.locator('[role="menuitemcheckbox"]').first();
  await expect(header).toHaveAttribute("aria-checked", "false");
  await header.click();
  await expect(page.locator("#status")).toContainText("Header row");
  await expect(menu.locator('[role="menuitemcheckbox"]').first()).toHaveAttribute("aria-checked", "true");
});

// Mutation: make `insert_cells` insert at `range.cells.1 + 1`; the new cell lands
// after the marked one and the row reading goes red.
test("Insert cells shifts right and Delete cells closes the row up again", async ({ page }) => {
  await insertTable(page, 2, 2);
  await page.locator("#tabTable").click();
  await page.locator("#tableInsertCellsBtn").click();
  const insert = page.locator("#tableInsertCellsDialog");
  await expect(insert).toBeVisible();
  await insert.locator("[data-dialog-apply]").click();
  await expect(insert).toBeHidden();
  expect((await modelCells(page))[0]).toEqual(["", MARK, ""]);

  await page.locator("#tableDeleteCellsBtn").click();
  const remove = page.locator("#tableDeleteCellsDialog");
  await expect(remove).toBeVisible();
  await remove.locator("[data-dialog-apply]").click();
  await expect(remove).toBeHidden();
  expect((await modelCells(page))[0]).toEqual([MARK, ""]);
});

// Mutation: make `setCellTextDirection` write nothing (return before the edit);
// the status never names the direction and the radio stays on Horizontal.
test("text direction is chosen from the Table band and read back as the checked choice", async ({ page }) => {
  await insertTable(page, 2, 2);
  await page.locator("#tabTable").click();
  await page.locator("#tableTextDirectionBtn").click();
  const menu = page.locator("#tableTextDirectionMenu");
  await expect(menu.locator('[role="menuitemradio"]')).toHaveCount(3);
  await menu.locator('[role="menuitemradio"]').nth(1).click();
  await expect(page.locator("#status")).toContainText("Rotate text down");
  await page.locator("#tableTextDirectionBtn").click();
  await expect(menu.locator('[role="menuitemradio"]').nth(1)).toHaveAttribute("aria-checked", "true");
});
