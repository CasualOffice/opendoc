import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  setReviewMode,
} from "./fixtures.mjs";

// Regression: when a paragraph carries a comment AND a suggestion whose range
// falls inside (or shares a boundary with) the comment's, focusing the
// suggestion must activate the SUGGESTION — not the comment. The old code
// re-derived the active item from the caret point via reviewCommentAtAnchor,
// which short-circuited to the comment, so the suggestion's card drifted from
// its marker (REVIEW-GAP-019 only held for non-overlapping items).

async function pastePlainText(page, text) {
  await page.evaluate(async (t) => {
    const dt = new DataTransfer();
    dt.setData("text/plain", t);
    document
      .querySelector(".editor-surface, #viewport")
      .dispatchEvent(new ClipboardEvent("paste", { clipboardData: dt, bubbles: true }));
  }, text);
}

test("focusing a suggestion nested in a commented range activates the suggestion, not the comment", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // A commented word, then a tracked insertion in its interior so the caret used
  // to resolve the active item lands inside the comment's range.
  await page.keyboard.type("ABCDEFGH");
  for (let i = 0; i < 8; i++) await page.keyboard.press("Shift+ArrowLeft");
  await page.locator("#selComment").click();
  const sidebar = page.locator("#reviewSidebar");
  await sidebar.locator('[data-testid="review-comment-composer"]').fill("a note");
  await sidebar.locator('[data-testid="review-comment-submit"]').click();

  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  for (let i = 0; i < 4; i++) await page.keyboard.press("ArrowRight");
  await setReviewMode(page, "suggesting");
  await pastePlainText(page, "X");

  // Both items exist: one comment marker, one insertion marker.
  await expect(page.locator(".overlay .review-comment-marker").first()).toBeVisible();
  await expect(page.locator(".overlay .review-insertion-marker").first()).toBeVisible();

  // Walk the unified list from the top: Next -> the comment, Next -> the
  // suggestion (further right in the same paragraph). Landing on the suggestion
  // is the focusReviewTarget path the bug lived in.
  await moveCaretToDocStart(page);
  await page.locator("#reviewNext").click();
  await page.locator("#reviewNext").click();

  // The suggestion is the active item: its marker carries the active class and
  // its sidebar card is the expanded one; the comment is NOT active.
  await expect(
    page.locator(".overlay .review-insertion-marker.review-revision-marker-active"),
  ).not.toHaveCount(0);
  await expect(page.locator(".overlay .review-comment-marker-active")).toHaveCount(0);

  // Card ↔ marker alignment: the active suggestion's card top tracks its marker
  // (REVIEW-GAP-019), so the two never drift apart. Poll — the margin layout
  // commits on a requestAnimationFrame after the card is surfaced active.
  await expect
    .poll(
      () =>
        page.evaluate(() => {
          const card = document.querySelector(".review-margin-insertion");
          const marker = document.querySelector(
            ".overlay .review-insertion-marker.review-revision-marker-active",
          );
          if (!card || !marker) return Number.POSITIVE_INFINITY;
          return Math.abs(
            card.getBoundingClientRect().top - marker.getBoundingClientRect().top,
          );
        }),
      { timeout: 4000 },
    )
    .toBeLessThan(48);

  expect(consoleErrors).toEqual([]);
});
