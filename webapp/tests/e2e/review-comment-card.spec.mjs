// The comment card, against Word / Google Docs / ONLYOFFICE:
//
//   RC-15  after posting, the keyboard is back in the document — typing goes into
//          the text, not into <body>;
//   RC-11  the thread's ROOT comment can be edited, not only its replies;
//   RC-12  once the commented text is deleted the card still shows what it was
//          about, marked deleted, as ONLYOFFICE keeps its quote.
import { clickIntoFirstPage, expect, expectEditorFocused, moveCaretToDocStart, test } from "./fixtures.mjs";

const markers = (page) => page.locator(".review-comment-marker");
const card = (page) => page.locator(".review-margin-comment").first();
/** The text the comment is anchored on: typed, so the test knows it exactly. */
const ANCHOR = "QUOTEMARKER";

/** Types `ANCHOR` at the start of the rich fixture and comments on exactly it. */
async function commentOnTen(page, text = "Needs a second look") {
  await page.goto("/editor.html?fixture=rich");
  await page.waitForFunction(() => document.querySelectorAll(".page-wrap").length > 0, null, {
    timeout: 45_000,
  });
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type(ANCHOR);
  await expect(page.locator("#a11yDocument")).toContainText(ANCHOR);
  for (let i = 0; i < ANCHOR.length; i++) await page.keyboard.press("Shift+ArrowLeft");
  await page.locator('[data-tab="review"]').click();
  await page.locator("#reviewCommentBtn").click();
  await expect(page.getByTestId("review-comment-composer")).toBeFocused();
  await page.keyboard.type(text);
  await page.keyboard.press("Enter");
  await expect(markers(page)).toHaveCount(1);
}

test("after posting a comment the keyboard is back in the document (RC-15)", async ({ page, consoleErrors }) => {
  await commentOnTen(page);
  await expectEditorFocused(page);
  expect(await page.evaluate(() => document.activeElement === document.body)).toBe(false);
  // The guarantee, not the mechanism: what the reader types next lands in the text.
  await page.keyboard.press("End");
  await page.keyboard.type("ZQXTYPED");
  await expect(page.locator("#a11yDocument")).toContainText("ZQXTYPED");
  expect(consoleErrors).toEqual([]);
});

test("the root comment of a thread can be edited (RC-11)", async ({ page, consoleErrors }) => {
  await commentOnTen(page, "Teh first wrod");
  await card(page).click();
  await page.getByTestId("review-comment-edit-open").click();
  const field = page.getByTestId("review-comment-edit");
  await expect(field).toBeFocused();
  await expect(field).toHaveValue("Teh first wrod");
  await field.fill("The first word");
  await page.keyboard.press("Control+Enter");
  await expect(field).toHaveCount(0);
  await expect(card(page).locator(".review-margin-body")).toHaveText("The first word");
  await expectEditorFocused(page);
  expect(consoleErrors).toEqual([]);
});

test("Escape cancels an edit and leaves the comment as it was", async ({ page, consoleErrors }) => {
  await commentOnTen(page, "Unchanged");
  await card(page).click();
  await page.getByTestId("review-comment-edit-open").click();
  await page.keyboard.type(" and more");
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("review-comment-edit")).toHaveCount(0);
  await expect(card(page).locator(".review-margin-body")).toHaveText("Unchanged");
  await expectEditorFocused(page);
  expect(consoleErrors).toEqual([]);
});

test("the card quotes the commented text, and keeps it marked deleted once it is gone (RC-12)", async ({
  page,
  consoleErrors,
}) => {
  await commentOnTen(page);
  const quote = card(page).getByTestId("review-comment-quote");
  await expect(quote).toBeVisible();
  await expect(quote.locator(".review-margin-quote-text")).toHaveText(ANCHOR);
  await expect(quote).not.toHaveClass(/deleted/);
  // Delete exactly the commented text from the keyboard.
  await moveCaretToDocStart(page);
  for (let i = 0; i < ANCHOR.length; i++) await page.keyboard.press("Shift+ArrowRight");
  await page.keyboard.press("Backspace");
  await expect(page.locator("#a11yDocument")).not.toContainText(ANCHOR);
  await expect(quote).toHaveClass(/deleted/);
  await expect(quote.locator(".review-margin-quote-text")).toHaveText(ANCHOR);
  await expect(quote.locator(".review-margin-quote-state")).toHaveText("Text deleted");
  expect(consoleErrors).toEqual([]);
});
