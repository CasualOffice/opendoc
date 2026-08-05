// REVIEW-GAP-023 (docs/81): review sidebar accessibility — real button semantics
// and descriptive aria-labels on card + header controls, a polite live region
// that announces review events, keyboard-activatable controls, and focus return
// on close.
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

test("header controls are labeled real buttons and keyboard-activatable (REVIEW-GAP-023)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await setReviewMode(page, "suggesting");
  await pastePlainText(page, "A11Y_HEADER_INSERT ");

  const sidebar = page.locator("#reviewSidebar");
  await expect(sidebar).toBeVisible();

  // Every header control is a real <button> with an accessible name.
  for (const [id, name] of [
    ["#reviewPrevious", "Previous comment or change"],
    ["#reviewNext", "Next comment or change"],
    ["#reviewClose", "Close comments and suggestions"],
  ]) {
    const btn = page.locator(id);
    await expect(btn).toHaveJSProperty("tagName", "BUTTON");
    await expect(btn).toHaveAttribute("aria-label", name);
  }
  // The Open/Resolved/All filter buttons are labeled toggle buttons.
  const filters = sidebar.locator("[data-review-filter]");
  await expect(filters).toHaveCount(3);
  await expect(sidebar.locator('[data-review-filter="open"]')).toHaveAttribute("aria-pressed", /true|false/);

  // Accept-all is keyboard-activatable: focus it and press Enter clears changes.
  const acceptAll = page.locator("#reviewAcceptAll");
  await expect(acceptAll).toHaveJSProperty("tagName", "BUTTON");
  await acceptAll.focus();
  await expect(acceptAll).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(sidebar.locator(".review-margin-card.review-margin-insertion")).toHaveCount(0);

  expect(consoleErrors).toEqual([]);
});

test("a card and its actions expose button roles and descriptive aria-labels (REVIEW-GAP-023)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await setReviewMode(page, "suggesting");
  await pastePlainText(page, "A11Y_CARD_INSERT ");

  const sidebar = page.locator("#reviewSidebar");
  const card = sidebar.locator(".review-margin-card.review-margin-insertion");
  await expect(card).toBeVisible();

  // The card is a labelled, expandable group (not a nameless article).
  await expect(card).toHaveAttribute("role", "group");
  await expect(card).toHaveAttribute("aria-label", /Suggested .* by/);
  await expect(card).toHaveAttribute("aria-expanded", "false");

  await card.click();
  await expect(card).toHaveAttribute("aria-expanded", "true");

  // Accept/Reject are real buttons with descriptive names (not bare "Accept").
  const accept = card.getByRole("button", { name: /Accept this/ });
  const reject = card.getByRole("button", { name: /Reject this/ });
  await expect(accept).toBeVisible();
  await expect(reject).toBeVisible();

  expect(consoleErrors).toEqual([]);
});

test("the review live region announces accept and reject (REVIEW-GAP-023)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  const live = page.locator("#reviewLiveRegion");
  await expect(live).toHaveAttribute("aria-live", "polite");
  await expect(live).toHaveAttribute("role", "status");

  await setReviewMode(page, "suggesting");
  await pastePlainText(page, "A11Y_ANNOUNCE_INSERT ");

  const card = page.locator("#reviewSidebar .review-margin-card.review-margin-insertion");
  await card.click();
  await card.getByRole("button", { name: /Accept this/ }).click();
  await expect(live).toHaveText(/Accepted/);

  // A second suggestion, rejected, announces the rejection.
  await moveCaretToDocStart(page);
  await pastePlainText(page, "A11Y_ANNOUNCE_TWO ");
  const card2 = page.locator("#reviewSidebar .review-margin-card.review-margin-insertion");
  await card2.click();
  await card2.getByRole("button", { name: /Reject this/ }).click();
  await expect(live).toHaveText(/Rejected/);

  expect(consoleErrors).toEqual([]);
});

test("the live region announces comment added, filter change, and accept-all (REVIEW-GAP-023)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  const live = page.locator("#reviewLiveRegion");
  const sidebar = page.locator("#reviewSidebar");

  // Comment added.
  await page.keyboard.type("ANNOUNCE_COMMENT");
  for (let i = 0; i < "ANNOUNCE_COMMENT".length; i++) await page.keyboard.press("Shift+ArrowLeft");
  await page.locator("#selComment").click();
  await sidebar.locator('[data-testid="review-comment-composer"]').fill("Announce me");
  await sidebar.locator('[data-testid="review-comment-submit"]').click();
  await expect(live).toHaveText(/Comment added/);

  // Filter change is announced.
  await sidebar.locator('[data-review-filter="all"]').click();
  await expect(live).toHaveText(/Filter: All comments/);

  expect(consoleErrors).toEqual([]);
});

test("closing the sidebar returns focus to the rail toggle (REVIEW-GAP-023)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await setReviewMode(page, "suggesting");
  await pastePlainText(page, "A11Y_FOCUS_INSERT ");

  const sidebar = page.locator("#reviewSidebar");
  await expect(sidebar).toBeVisible();

  await page.locator("#reviewClose").click();
  await expect(sidebar).toBeHidden();
  // Focus returned to the rail toggle that owns the sidebar (not stranded).
  await expect(page.locator("#railReview")).toBeFocused();

  expect(consoleErrors).toEqual([]);
});
