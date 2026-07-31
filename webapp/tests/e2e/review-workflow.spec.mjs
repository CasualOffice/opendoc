// REVIEW-GAP-018/019 (docs/81): the review sidebar's workflow chrome — a fixed
// header with Open/Resolved/All filter, Next/Previous-change navigation, and
// Accept-all/Reject-all — plus caret-driven card expansion and card→canvas
// scroll-to-anchor for both comments and tracked changes.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  setReviewMode,
} from "./fixtures.mjs";

async function pastePlainText(page, text) {
  await page.evaluate((value) => {
    const data = new DataTransfer();
    data.setData("text/plain", value);
    document.dispatchEvent(
      new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }),
    );
  }, text);
}

/** Add a comment "TEXT" over the currently-at-doc-start typed word `target`. */
async function addComment(page, target, body) {
  await page.keyboard.type(target);
  for (let i = 0; i < target.length; i++) await page.keyboard.press("Shift+ArrowLeft");
  await page.locator("#selComment").click();
  const sidebar = page.locator("#reviewSidebar");
  await sidebar.locator('[data-testid="review-comment-composer"]').fill(body);
  await sidebar.locator('[data-testid="review-comment-submit"]').click();
}

test("Open / Resolved / All filter shows and hides resolved comments (REVIEW-GAP-018/019)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await addComment(page, "FILTER_TARGET", "A comment to resolve");

  const sidebar = page.locator("#reviewSidebar");
  const card = sidebar.locator(".review-margin-card.review-margin-comment");
  await expect(card).toBeVisible();

  // Resolve it. Under the default "Open" filter the card then disappears.
  await card.click();
  await card.locator(":scope > .review-margin-card-head").getByRole("button", { name: "Resolve" }).click();
  await expect(sidebar.locator(".review-margin-card.review-margin-comment")).toHaveCount(0);

  // "Resolved" shows only resolved comments — the card is back.
  await sidebar.locator('[data-review-filter="resolved"]').click();
  await expect(sidebar.locator(".review-margin-card.review-margin-comment")).toHaveCount(1);

  // "All" also shows it.
  await sidebar.locator('[data-review-filter="all"]').click();
  await expect(sidebar.locator(".review-margin-card.review-margin-comment")).toHaveCount(1);

  // Back to "Open" — hidden again.
  await sidebar.locator('[data-review-filter="open"]').click();
  await expect(sidebar.locator(".review-margin-card.review-margin-comment")).toHaveCount(0);

  expect(consoleErrors).toEqual([]);
});

test("Accept all decides every tracked change at once (REVIEW-GAP-018)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await setReviewMode(page, "suggesting");
  await pastePlainText(page, "SUGGEST_ONE ");
  await pastePlainText(page, "SUGGEST_TWO ");

  const sidebar = page.locator("#reviewSidebar");
  const insertions = sidebar.locator(".review-margin-card.review-margin-insertion");
  await expect(insertions).toHaveCount(2);

  // Accept all lives in the fixed header; it decides every change in one action.
  await page.locator("#reviewAcceptAll").click();
  await expect(insertions).toHaveCount(0);

  expect(consoleErrors).toEqual([]);
});

test("Next change navigates between tracked changes and expands the target card (REVIEW-GAP-018)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await setReviewMode(page, "suggesting");
  await pastePlainText(page, "NAV_INSERT ");

  await moveCaretToDocStart(page); // move the caret away from the change

  const sidebar = page.locator("#reviewSidebar");
  const card = sidebar.locator(".review-margin-card.review-margin-insertion");
  await expect(card).toHaveAttribute("aria-expanded", "false");

  await page.locator("#reviewNext").click();
  // Navigating to the change selects its range and surfaces (expands) its card.
  await expect(card).toHaveAttribute("aria-expanded", "true");

  expect(consoleErrors).toEqual([]);
});

test("a caret landing in a commented range expands that comment's card (REVIEW-GAP-019)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await addComment(page, "CARET_TARGET", "Expand me on caret");

  const sidebar = page.locator("#reviewSidebar");
  const card = sidebar.locator(".review-margin-card.review-margin-comment");
  await expect(card).toBeVisible();
  await expect(card).toHaveAttribute("aria-expanded", "false");

  // Clicking on the commented text (its highlight passes clicks through to the
  // hit-test, REVIEW-GAP-041) places the caret inside the range, which expands
  // the card as a non-blocking caret-driven effect.
  await page.locator(".review-comment-marker").first().click();
  await expect(card).toHaveAttribute("aria-expanded", "true");

  expect(consoleErrors).toEqual([]);
});

test("clicking a suggestion chip scrolls the canvas to its anchor (REVIEW-GAP-019)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await setReviewMode(page, "suggesting");
  await pastePlainText(page, "SCROLL_ANCHOR_INSERT ");

  const sidebar = page.locator("#reviewSidebar");
  const card = sidebar.locator(".review-margin-card.review-margin-insertion");
  await expect(card).toBeVisible();

  // Scroll the canvas far away so the change's (near-doc-start) anchor leaves
  // the viewport.
  const before = await page.locator("#viewport").evaluate((v) => {
    v.scrollTop = v.scrollHeight;
    return v.scrollTop;
  });
  expect(before).toBeGreaterThan(100);
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));

  // Clicking the chip scrolls the canvas back toward the anchor (card→canvas
  // sync): the viewport scrolls up substantially from the bottom.
  await card.click();
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  const after = await page.locator("#viewport").evaluate((v) => v.scrollTop);
  expect(after).toBeLessThan(before - 50);

  expect(consoleErrors).toEqual([]);
});

test("stacked chips: clicking one surfaces it as the active/aligned card (REVIEW-GAP-019)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // Two adjacent tracked insertions at doc start collision-stack near one anchor
  // row. (Two comments on one line would hit the separate REVIEW-GAP-010 limit
  // on comments adjacent to existing comment markers, so suggestions are used.)
  await setReviewMode(page, "suggesting");
  await pastePlainText(page, "STACK_FIRST ");
  await pastePlainText(page, "STACK_SECOND ");

  const sidebar = page.locator("#reviewSidebar");
  const cards = sidebar.locator(".review-margin-card.review-margin-insertion");
  await expect(cards).toHaveCount(2);

  const first = cards.filter({ hasText: "STACK_FIRST" });
  const second = cards.filter({ hasText: "STACK_SECOND" });

  // Click the first stacked chip — it becomes the active/expanded card.
  await first.click();
  await expect(first).toHaveAttribute("aria-expanded", "true");
  await expect(second).toHaveAttribute("aria-expanded", "false");

  // Clicking the second one surfaces IT as the active card; the first collapses.
  await second.click();
  await expect(second).toHaveAttribute("aria-expanded", "true");
  await expect(first).toHaveAttribute("aria-expanded", "false");

  expect(consoleErrors).toEqual([]);
});
