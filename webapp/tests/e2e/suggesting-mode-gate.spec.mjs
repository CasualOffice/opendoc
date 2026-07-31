// REVIEW-GAP-004 (docs/81-COMMENTS-SUGGESTIONS-COMPLETENESS-AUDIT.md):
// Suggesting mode was only enforced by `runToolbarEdit`. Table
// style/width/sort/merge/split/insert, Find/Replace apply, command-palette
// Insert Table/Restart List, document properties, and page setup called
// `runEdit`/`runNodeEdit` directly with no mode check, so those commands
// could silently mutate the document while the UI still read Suggesting.
// P1G-REVIEW-042 adds a single `blockUntrackedInSuggesting()` gate — the
// exact status message `runToolbarEdit` already used — to every one of those
// paths, and makes `runNodeEdit` (table/cell formatting; every call site is
// a table op) fail closed by default. These commands have no tracked-
// revision representation yet (REVIEW-GAP-009), so the fix is to block them
// in Suggesting mode, not to fake tracking.
import { test, expect, gotoEditor, clickIntoFirstPage, setReviewMode } from "./fixtures.mjs";

async function insertTwoByTwoTable(page) {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await expect(page.locator("#insertTableMenu")).toBeVisible();
  await page.locator('.gc[data-r="2"][data-c="2"]').click();
  await expect(page.locator("#tabTable")).toBeEnabled();
  await page.locator("#tabTable").click();
}

// The three-state mode control lives in the Home ribbon panel, hidden while
// the contextual Table (or Insert) panel is showing — `setReviewMode` switches
// to Home first.
async function enterSuggestingMode(page) {
  await setReviewMode(page, "suggesting");
}

test("table style and table structure commands are blocked in Suggesting mode instead of silently mutating the table", async ({
  page,
  consoleErrors,
}) => {
  await insertTwoByTwoTable(page);
  await expect(page.locator("#tableContext")).toContainText("2×2 table");

  await enterSuggestingMode(page);
  await page.locator("#tabTable").click();

  // Table structure (the exact GAP-004 list item "table ... insert"): a
  // row-insert ribbon action used to call `doc.insertRow` via a bare
  // `runEdit` with no mode check.
  const ribbon = page.locator(".table-ribbon");
  await ribbon.locator('[data-table-action="insert-row-below"]').click();
  await expect(page.locator("#status")).toContainText("cannot be tracked");
  await expect(page.locator("#tableContext")).toContainText("2×2 table");

  // Table style (the exact bug named in REVIEW-GAP-004: `applyTableStyle`
  // called `runEdit` instead of `runToolbarEdit`).
  await page.locator("#tableStyleBtn").click();
  await expect(page.locator("#tableStyleMenu")).toBeVisible();
  await page.locator("#tableStyleMenu [data-table-style]").first().click();
  await expect(page.locator("#status")).toContainText("cannot be tracked");

  // Confirm the gate is mode-specific, not a permanent regression: switching
  // back to Editing lets the identical action through.
  await setReviewMode(page, "editing");
  await page.locator("#tabTable").click();
  await ribbon.locator('[data-table-action="insert-row-below"]').click();
  await expect(page.locator("#tableContext")).toContainText("3×2 table");
  expect(consoleErrors).toEqual([]);
});

test("inserting a new table from the ribbon grid is blocked in Suggesting mode", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  // The document is rendered on a canvas, not as DOM tables, so the proof
  // that nothing was inserted is that the caret never lands in a table
  // (`#tabTable` only enables once `doc.inTable()` is true).
  await expect(page.locator("#tabTable")).toBeDisabled();

  await enterSuggestingMode(page);

  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await expect(page.locator("#insertTableMenu")).toBeVisible();
  await page.locator('.gc[data-r="2"][data-c="2"]').click();
  await expect(page.locator("#status")).toContainText("cannot be tracked");
  await expect(page.locator("#tabTable")).toBeDisabled();
  expect(consoleErrors).toEqual([]);
});

test("document properties cannot be silently applied in Suggesting mode", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await enterSuggestingMode(page);

  await page.locator("#propertiesBtn").click();
  await expect(page.locator("#propertiesPanel")).toBeVisible();
  const original = await page.locator("#propTitle").inputValue();
  await page.locator("#propTitle").fill("SNEAKY_UNTRACKED_TITLE");
  await page.locator("#propertiesApply").click();
  await expect(page.locator("#status")).toContainText("cannot be tracked");

  // Reopen and confirm the engine's own copy of the title was never touched
  // (the dialog's transient input value proves nothing by itself).
  await page.locator("#propertiesBtn").click();
  await expect(page.locator("#propertiesPanel")).toBeVisible();
  await expect(page.locator("#propTitle")).toHaveValue(original);
  expect(consoleErrors).toEqual([]);
});
