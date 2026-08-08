// The status footer must report a character count alongside words and
// paragraphs — parity with Word (status bar "Characters") and Google Docs
// (Tools ▸ Word count "Characters"). The count is read straight from the
// engine's `doc.documentStats()` (charactersWithSpaces), so it is an
// independent proof of document content, and it must update live as the user
// types. A hover tooltip surfaces the with- vs without-spaces breakdown Word
// distinguishes.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
} from "./fixtures.mjs";

// Parses the leading integer out of e.g. "1,234 characters".
async function charCount(page) {
  const text = await page.locator("#statChars").textContent();
  return Number.parseInt(text.replace(/[^0-9]/g, ""), 10);
}

test("status footer shows a live character count that updates as you type", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);

  const stat = page.locator("#statChars");
  await expect(stat).toBeVisible();
  await expect(stat).toContainText(/character/);

  const before = await charCount(page);
  expect(before).toBeGreaterThan(0);

  // The tooltip distinguishes with- vs without-spaces (Word's two figures).
  await expect(stat).toHaveAttribute("title", /characters \(with spaces\)/);
  await expect(stat).toHaveAttribute("title", /characters \(no spaces\)/);

  // Typing three visible characters raises the count by exactly three.
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type("abc");

  await expect
    .poll(async () => charCount(page))
    .toBe(before + 3);

  expect(consoleErrors).toEqual([]);
});
