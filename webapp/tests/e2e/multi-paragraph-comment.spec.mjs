// A comment anchor may span multiple paragraphs (Word/Docs parity): the
// `CommentRangeStart` lands in the start paragraph and the `CommentRangeEnd`
// plus reference in the end paragraph, both keyed to one comment id. This spec
// proves the multi-paragraph selection is commentable, the single-paragraph
// path still works, and add-comment stays blocked in read-only Viewing mode.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  setReviewMode,
} from "./fixtures.mjs";

const commentCard = (page) =>
  page.locator("#reviewSidebar .review-margin-card.review-margin-comment");

// Submits the sidebar composer with `body` once it is open.
async function submitComposer(page, body) {
  const sidebar = page.locator("#reviewSidebar");
  await sidebar.locator('[data-testid="review-comment-composer"]').fill(body);
  await sidebar.locator('[data-testid="review-comment-submit"]').click();
}

test("a comment can span two paragraphs and renders one anchored card", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // Two fresh paragraphs at the document start, then a selection that starts in
  // the first and ends in the second (a genuine cross-paragraph range).
  await page.keyboard.type("MPARAONE");
  await page.keyboard.press("Enter");
  await page.keyboard.type("MPARATWO");
  await page.keyboard.press("Shift+ArrowUp");
  await page.keyboard.press("Shift+Home");

  await page.locator("#selComment").click();
  await submitComposer(page, "Spans two paragraphs");

  // The comment is created (one anchored card) and its highlight resolves —
  // proof the cross-paragraph markers produced a usable anchor rather than none.
  await expect(commentCard(page)).toHaveCount(1);
  await expect(commentCard(page)).toContainText("Spans two paragraphs");
  await expect(page.locator(".review-comment-marker").first()).toBeVisible();

  expect(consoleErrors).toEqual([]);
});

test("a single-paragraph comment still works", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  const target = "SINGLEPARA";
  await page.keyboard.type(target);
  for (let i = 0; i < target.length; i++) await page.keyboard.press("Shift+ArrowLeft");

  await page.locator("#selComment").click();
  await submitComposer(page, "One paragraph only");

  await expect(commentCard(page)).toHaveCount(1);
  await expect(commentCard(page)).toContainText("One paragraph only");

  expect(consoleErrors).toEqual([]);
});

test("adding a comment is blocked in read-only Viewing mode", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // Two paragraphs authored while still editable, then a cross-paragraph
  // selection over them.
  await page.keyboard.type("VIEWPARAONE");
  await page.keyboard.press("Enter");
  await page.keyboard.type("VIEWPARATWO");

  await setReviewMode(page, "viewing");

  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.press("Shift+ArrowDown");
  await page.keyboard.press("Shift+End");

  // Viewing is fully read-only: the add-comment mutation fails closed with the
  // read-only status and no comment card is created.
  await page.locator("#selComment").click();
  await submitComposer(page, "Should be blocked");
  await expect(page.locator("#status")).toContainText("read-only");
  await expect(commentCard(page)).toHaveCount(0);

  expect(consoleErrors).toEqual([]);
});
