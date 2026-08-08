import { test, expect, gotoEditor, clickIntoFirstPage, setReviewMode } from "./fixtures.mjs";

// Marker-format picker: the bullet and numbered buttons carry a ▾ split that
// opens a small gallery of bullet glyphs / number formats. Picking one changes
// the caret's list marker through the gated setListFormat path — one undo,
// blocked in Viewing — matching Word/Docs. (Full multilevel authoring is a
// separate follow-up; this covers the current-list marker format only.)

test("pick a different number format from the gallery; it changes and is one undo", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // Make a numbered list on the caret paragraph.
  await page.locator("#numberedList").click();
  await expect(page.locator("#numberedList")).toHaveAttribute("aria-pressed", "true");

  const menu = page.locator("#numberGalleryMenu");
  const decimalCell = menu.locator('[data-spec="decimal"]');
  const lowerLetterCell = menu.locator('[data-spec="lowerLetter"]');

  // Opening the gallery reflects the current format: a fresh numbered list is
  // decimal.
  await page.locator("#numberedListMenuBtn").click();
  await expect(menu).toBeVisible();
  await expect(decimalCell).toHaveAttribute("aria-checked", "true");
  await expect(lowerLetterCell).toHaveAttribute("aria-checked", "false");

  // Pick lowercase letters; reopening the gallery now reflects the new choice.
  await lowerLetterCell.click();
  await expect(menu).toBeHidden();
  await page.locator("#numberedListMenuBtn").click();
  await expect(lowerLetterCell).toHaveAttribute("aria-checked", "true");
  await expect(decimalCell).toHaveAttribute("aria-checked", "false");
  // The button is still a numbered list (format change, not a toggle off).
  await expect(page.locator("#numberedList")).toHaveAttribute("aria-pressed", "true");

  // A single undo restores the original decimal format (one undoable action);
  // the list itself survives (that toggle is a separate, earlier undo step).
  await page.keyboard.press("Escape");
  await page.locator("#undoBtn").click();
  await page.locator("#numberedListMenuBtn").click();
  await expect(decimalCell).toHaveAttribute("aria-checked", "true");
  await expect(lowerLetterCell).toHaveAttribute("aria-checked", "false");
  await expect(page.locator("#numberedList")).toHaveAttribute("aria-pressed", "true");
  await page.keyboard.press("Escape");

  expect(consoleErrors).toEqual([]);
});

test("picking a bullet glyph from the gallery changes the current bullet list", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await page.locator("#bulletList").click();
  await expect(page.locator("#bulletList")).toHaveAttribute("aria-pressed", "true");

  const menu = page.locator("#bulletGalleryMenu");
  const roundCell = menu.locator('[data-spec="bullet:•"]');
  const squareCell = menu.locator('[data-spec="bullet:▪"]');

  // A fresh bullet list starts with the filled round bullet.
  await page.locator("#bulletListMenuBtn").click();
  await expect(roundCell).toHaveAttribute("aria-checked", "true");

  // Switch to the square bullet; the choice is reflected on reopen.
  await squareCell.click();
  await page.locator("#bulletListMenuBtn").click();
  await expect(squareCell).toHaveAttribute("aria-checked", "true");
  await expect(roundCell).toHaveAttribute("aria-checked", "false");
  await expect(page.locator("#bulletList")).toHaveAttribute("aria-pressed", "true");
  await page.keyboard.press("Escape");

  expect(consoleErrors).toEqual([]);
});

test("the marker-format gallery is blocked in Viewing mode", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await page.locator("#numberedList").click();
  await expect(page.locator("#numberedList")).toHaveAttribute("aria-pressed", "true");

  await setReviewMode(page, "viewing");

  const menu = page.locator("#numberGalleryMenu");
  await page.locator("#numberedListMenuBtn").click();
  await menu.locator('[data-spec="lowerLetter"]').click();

  // Read-only: the status reports it and the format is unchanged (still decimal).
  await expect(page.locator("#status")).toContainText("read-only");
  await page.locator("#numberedListMenuBtn").click();
  await expect(menu.locator('[data-spec="decimal"]')).toHaveAttribute("aria-checked", "true");
  await expect(menu.locator('[data-spec="lowerLetter"]')).toHaveAttribute(
    "aria-checked",
    "false",
  );
  await page.keyboard.press("Escape");

  expect(consoleErrors).toEqual([]);
});
