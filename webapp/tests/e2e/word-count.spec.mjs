// Word count (HF-051): Word's dialog, from every surface, and the selection's
// "x of y words" in the status bar.
//
// Every test reads a COUNT, not a control: a dialog that opened over the wrong
// figures, or a status bar that kept the document's total while text was
// selected, would pass a spec that only looked for the dialog.
import { test, expect, clickIntoFirstPage, runAppMenuCommand, MOD } from "./fixtures.mjs";

/** A blank document holding exactly `text`, caret at its end — so every figure
 *  the dialog shows is one this test can account for. */
async function blankDocumentWithText(page, text) {
  await page.goto("/editor.html?blank=1");
  await page.waitForFunction(
    () => document.getElementById("status")?.textContent.startsWith("Ready — open"),
    null,
    { timeout: 45_000 },
  );
  await runAppMenuCommand(page, "file", "file.new");
  await expect(page.locator(".page-wrap")).not.toHaveCount(0, { timeout: 45_000 });
  await clickIntoFirstPage(page);
  await page.locator("#pages").pressSequentially(text);
  await expect(page.locator("#statWords")).toHaveText(/\b3 words\b/);
}

const dialog = (page) => page.locator("#wordCountDialog");
const cell = (page, stat, column) =>
  page.locator(`#wordCountDialog [data-stat="${stat}"] [data-word-count-${column}]`);

async function expectDocumentFigures(page) {
  await expect(dialog(page)).toBeVisible();
  await expect(cell(page, "words", "document")).toHaveText("3");
  await expect(cell(page, "characters", "document")).toHaveText("14");
  await expect(cell(page, "charactersWithSpaces", "document")).toHaveText("16");
  await expect(cell(page, "paragraphs", "document")).toHaveText("1");
  await expect(cell(page, "pages", "document")).toHaveText("1");
}

test("Word count opens from the band, the menu, the chord and the status bar", async ({
  page,
  consoleErrors,
}) => {
  await blankDocumentWithText(page, "Alpha beta gamma");

  // 1. The Review band's Proofing group — Word's own home for it.
  await page.locator('[data-tab="review"]').click();
  await expect(page.locator("#reviewWordCountBtn")).toBeEnabled();
  await page.locator("#reviewWordCountBtn").click();
  await expectDocumentFigures(page);
  // A caret is not a selection: no Selection column to misread as zeros.
  await expect(page.locator('#wordCountDialog th[data-word-count-selection]')).toBeHidden();
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toBeHidden();

  // 2. The Review menu.
  await runAppMenuCommand(page, "review", "tools.wordCount");
  await expectDocumentFigures(page);
  await page.locator("#wordCountDone").click();
  await expect(dialog(page)).toBeHidden();

  // 3. Word's chord, which also closes it again.
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+Shift+G`);
  await expectDocumentFigures(page);
  await page.keyboard.press(`${MOD}+Shift+G`);
  await expect(dialog(page)).toBeHidden();

  // 4. The status bar's counts, by keyboard as well as by pointer.
  await page.locator("#stats").click();
  await expectDocumentFigures(page);
  await page.keyboard.press("Escape");
  await page.locator("#stats").focus();
  await page.keyboard.press("Enter");
  await expectDocumentFigures(page);
  await page.keyboard.press("Escape");

  expect(consoleErrors).toEqual([]);
});

test("a selection is counted in the dialog and as x of y in the status bar", async ({
  page,
  consoleErrors,
}) => {
  await blankDocumentWithText(page, "Alpha beta gamma");
  // "beta gamma": ten characters back from the end.
  for (let i = 0; i < 10; i++) await page.keyboard.press("Shift+ArrowLeft");

  await expect(page.locator("#statWords")).toHaveText("2 of 3 words");

  await page.keyboard.press(`${MOD}+Shift+G`);
  await expect(dialog(page)).toBeVisible();
  await expect(page.locator('#wordCountDialog th[data-word-count-selection]')).toBeVisible();
  await expect(cell(page, "words", "selection")).toHaveText("2");
  await expect(cell(page, "characters", "selection")).toHaveText("9");
  await expect(cell(page, "charactersWithSpaces", "selection")).toHaveText("10");
  await expect(cell(page, "paragraphs", "selection")).toHaveText("1");
  // A selection has no page count of its own; it says so rather than print 0.
  await expect(cell(page, "pages", "selection")).toHaveText("—");
  await expect(cell(page, "words", "document")).toHaveText("3");
  await page.keyboard.press("Escape");

  // Collapsing the selection puts the document's total back.
  await page.keyboard.press("End");
  await expect(page.locator("#statWords")).toHaveText("3 words");

  expect(consoleErrors).toEqual([]);
});
