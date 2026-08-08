// Table structure from the command palette.
//
// Every structural table command — insert/delete row and column, merge, split,
// select, distribute, sort, the property dialogs — used to live ONLY on the
// contextual Table ribbon tab and the right-click menu. `editorCommands()` never
// listed them, so they were absent from the palette and from every app menu:
// typing "insert row" or "merge cells" into ⌘⇧P returned nothing at all. Word
// reaches all of them through Tell Me, and Docs files them under Format ▸ Table.
//
// This is the same one-surface-only defect that left Picture off the Insert
// ribbon, so it is guarded the same way: assert the palette can find and RUN the
// command, and that the rows carry the context menu's own enablement rather than
// a second opinion that could drift from it.
import { test, expect, gotoEditor, clickIntoFirstPage, setReviewMode, MOD } from "./fixtures.mjs";

async function insertTwoByTwoTable(page) {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await expect(page.locator("#insertTableMenu")).toBeVisible();
  await page.locator('.gc[data-r="2"][data-c="2"]').click();
  await expect(page.locator("#tabTable")).toBeEnabled();
}

async function openPalette(page, query) {
  await page.keyboard.press(`${MOD}+Shift+P`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  await page.locator("#cmdInput").fill(query);
}

async function closePalette(page) {
  await page.keyboard.press("Escape");
  await expect(page.locator("#cmdPalette")).toBeHidden();
}

test("the palette finds table structure commands when the caret is in a table", async ({
  page,
  consoleErrors,
}) => {
  await insertTwoByTwoTable(page);

  await openPalette(page, "insert row");
  // The rows are flattened out of their submenus but keep the parent's name, so
  // the palette reads as a flat searchable list.
  await expect(page.locator("#cmdList >> text=Table: Insert Row above")).toBeVisible();
  await expect(page.locator("#cmdList >> text=Table: Insert Row below")).toBeVisible();
  await closePalette(page);

  await openPalette(page, "merge cells");
  await expect(page.locator("#cmdList >> text=Table: Merge cells")).toBeVisible();
  await closePalette(page);

  expect(consoleErrors).toEqual([]);
});

test("a table command run from the palette actually mutates the table, undoably", async ({
  page,
  consoleErrors,
}) => {
  await insertTwoByTwoTable(page);

  const rowCount = () =>
    page.evaluate(() => document.querySelectorAll("#a11yDocument table tr").length);
  const before = await rowCount();
  expect(before).toBeGreaterThan(0);

  await openPalette(page, "Insert Row below");
  await page.locator("#cmdList .cmd-item", { hasText: "Table: Insert Row below" }).first().click();
  await expect(page.locator("#cmdPalette")).toBeHidden();

  await expect.poll(rowCount).toBe(before + 1);

  // One undoable action, like the same command run from the right-click menu.
  // Undone by keystroke rather than the toolbar button: inserting the table
  // switches the ribbon to the contextual Table tab, so Home's #undoBtn is not
  // on screen — its label still reports the action ("Undo Table structure").
  await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", "Undo Table structure");
  await page.keyboard.press(`${MOD}+z`);
  await expect.poll(rowCount).toBe(before);

  expect(consoleErrors).toEqual([]);
});

test("the palette's table rows carry the context menu's own review-mode reasons", async ({
  page,
  consoleErrors,
}) => {
  await insertTwoByTwoTable(page);
  await setReviewMode(page, "suggesting");

  // Structural table edits cannot be tracked, so the palette must say so rather
  // than offer a row that silently does nothing — the reason string is the
  // context menu's, because both are built from the same context.
  await openPalette(page, "Insert Row below");
  const row = page.locator("#cmdList .cmd-item", { hasText: "Table: Insert Row below" }).first();
  await expect(row).toBeVisible();
  await expect(row).toBeDisabled();
  // The palette shows the reason in the hint column, and it is the context
  // menu's own string because both are built from the same context.
  await expect(row).toContainText("cannot be tracked");

  await closePalette(page);
  expect(consoleErrors).toEqual([]);
});

test("table commands are absent from the palette when the caret is not in a table", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await openPalette(page, "merge cells");
  await expect(page.locator("#cmdList >> text=Table: Merge cells")).toHaveCount(0);

  await closePalette(page);
  expect(consoleErrors).toEqual([]);
});
