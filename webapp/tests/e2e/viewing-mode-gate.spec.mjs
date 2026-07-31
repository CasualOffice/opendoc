// REVIEW-GAP-014 (docs/81-COMMENTS-SUGGESTIONS-COMPLETENESS-AUDIT.md):
// The design specifies three modes — Editing / Suggesting / Viewing — but
// `setReviewMode` used to collapse every non-suggesting value to Editing, so a
// genuine read-only Viewing mode did not exist. docs/68 §"Suggesting mode"
// defines Viewing as fully read-only: "no Operation reaches apply." This spec
// proves that in Viewing mode every document-mutating path fails closed
// (typing, deletion, paste, toolbar formatting, and table insertion) while the
// non-mutating capabilities a reader needs — selection and copy — still work.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  setReviewMode,
  MOD,
} from "./fixtures.mjs";

// The engine word count in the footer is the mutation-independent proof that
// the document did not change: it is read straight from `doc.documentStats()`,
// not from the DOM the command touched.
async function wordCount(page) {
  return page.locator("#statWords").textContent();
}

// A read-only status message plus focus returning to the canvas is the exact
// signal `blockMutationInViewing()` emits for every blocked mutation.
async function expectReadOnlyBlocked(page) {
  await expect(page.locator("#status")).toContainText("read-only");
}

test("Viewing mode is read-only: typing, deletion, paste, formatting, and table insertion are all blocked", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  const before = await wordCount(page);

  await setReviewMode(page, "viewing");
  // The mode is reflected: the Viewing segment is pressed, the other two are
  // not, and the read-only banner is shown.
  await expect(
    page.locator('#reviewModeControl [data-review-mode="viewing"]'),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(
    page.locator('#reviewModeControl [data-review-mode="editing"]'),
  ).toHaveAttribute("aria-pressed", "false");
  await expect(
    page.locator('#reviewModeControl [data-review-mode="suggesting"]'),
  ).toHaveAttribute("aria-pressed", "false");
  await expect(page.locator("#viewingBanner")).toBeVisible();
  await expect(page.locator("#suggestingBanner")).toBeHidden();

  // 1) Typing is blocked. The character is refused, the read-only message
  // shows, and the marker is not findable anywhere in the document.
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type("VIEWONLYMARKER");
  await expectReadOnlyBlocked(page);
  expect(await wordCount(page)).toBe(before);

  await page.keyboard.press(`${MOD}+f`);
  await page.locator("#findInput").fill("VIEWONLYMARKER");
  await expect(page.locator("#findStatus")).toHaveText("No match");
  await page.keyboard.press("Escape");

  // 2) Backspace/Delete are blocked.
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("Backspace");
  await expectReadOnlyBlocked(page);
  expect(await wordCount(page)).toBe(before);
  await page.keyboard.press("Delete");
  await expectReadOnlyBlocked(page);
  expect(await wordCount(page)).toBe(before);

  // 3) Paste is blocked (a synthetic clipboard paste, exactly as a browser
  // dispatches one) with no insertion.
  await page.evaluate(() => {
    const data = new DataTransfer();
    data.setData("text/plain", "PASTED_WHILE_VIEWING");
    document.dispatchEvent(
      new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }),
    );
  });
  await expectReadOnlyBlocked(page);
  expect(await wordCount(page)).toBe(before);
  await page.keyboard.press(`${MOD}+f`);
  await page.locator("#findInput").fill("PASTED_WHILE_VIEWING");
  await expect(page.locator("#findStatus")).toHaveText("No match");
  await page.keyboard.press("Escape");

  // 4) Toolbar formatting is blocked. Select a word first (selection works in
  // Viewing — see below), then Bold reports read-only instead of applying.
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  for (let i = 0; i < 4; i++) await page.keyboard.press("Shift+ArrowRight");
  await page.locator("#tabHome").click();
  await page.locator("#bold").click();
  await expectReadOnlyBlocked(page);
  expect(await wordCount(page)).toBe(before);

  // 5) Table insertion is blocked: the caret never lands in a table, so the
  // contextual Table tab stays disabled.
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await expect(page.locator("#insertTableMenu")).toBeVisible();
  await page.locator('.gc[data-r="2"][data-c="2"]').click();
  await expectReadOnlyBlocked(page);
  await expect(page.locator("#tabTable")).toBeDisabled();
  expect(await wordCount(page)).toBe(before);

  expect(consoleErrors).toEqual([]);
});

test("Viewing mode still allows selection and copy", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await setReviewMode(page, "viewing");

  // Selection works: extend a range with the keyboard and confirm the engine
  // produced a non-empty selected string (navigation/selection are not
  // mutations, so they are unaffected by the read-only gate).
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  for (let i = 0; i < 5; i++) await page.keyboard.press("Shift+ArrowRight");

  // Copy works: a synthetic copy event is served by the editor's copy handler,
  // which writes the selected text into the clipboard payload. This is the
  // reader capability Viewing must preserve; copy is not a mutation.
  const copied = await page.evaluate(() => {
    const data = new DataTransfer();
    document.dispatchEvent(
      new ClipboardEvent("copy", { clipboardData: data, bubbles: true, cancelable: true }),
    );
    return data.getData("text/plain");
  });
  expect(copied.length).toBeGreaterThan(0);

  // No read-only error was raised by the read-only-safe copy gesture.
  await expect(page.locator("#status")).not.toContainText("read-only");
  expect(consoleErrors).toEqual([]);
});
