// REVIEW-GAP-020 (docs/81-COMMENTS-SUGGESTIONS-COMPLETENESS-AUDIT.md): the
// review sidebar mounted every comment/revision card into the DOM at once, with
// no virtualization or retained item model, so a document with many review
// items allocated and laid out hundreds of card elements regardless of what was
// on screen. This spec loads the tall multi-page sample document, places
// comments far apart vertically, and proves that only the cards inside (or near)
// the viewport are ever mounted: a card off screen is detached, and scrolling it
// back into view remounts it — while the item itself is never lost.
import { test, expect } from "./fixtures.mjs";

const MOD = process.platform === "darwin" ? "Meta" : "Control";

// The default editor route opens the 14-page sample document (~15000px tall),
// which is what makes the off-screen band observable at all.
async function gotoSample(page) {
  await page.goto("/editor.html");
  await page.waitForFunction(
    () => {
      const status = document.getElementById("status");
      return (
        status !== null &&
        status.textContent === "" &&
        !status.classList.contains("error") &&
        document.querySelectorAll(".page-wrap").length > 0
      );
    },
    null,
    { timeout: 45_000 },
  );
}

function scrollViewportTo(page, top) {
  return page.evaluate((y) => {
    const v = document.getElementById("viewport");
    v.scrollTop = y;
    v.dispatchEvent(new Event("scroll"));
  }, top);
}

// Types a marker at the caret, selects it, and attaches a comment through the
// real on-selection affordance (mirrors review-margin.spec.mjs). The card body
// shows `note`, so cards are located by their note text.
async function addCommentAtCaret(page, marker, note) {
  await page.keyboard.type(marker);
  for (let i = 0; i < marker.length; i++) await page.keyboard.press("Shift+ArrowLeft");
  await page.locator("#selComment").click();
  const composer = page.locator('[data-testid="review-comment-composer"]');
  await expect(composer).toBeVisible();
  await composer.fill(note);
  await page.locator('[data-testid="review-comment-submit"]').click();
}

test("only cards inside the viewport band are mounted, and scrolling remounts them", async ({
  page,
  consoleErrors,
}) => {
  await gotoSample(page);
  const sidebar = page.locator("#reviewSidebar");
  const cards = page.locator(".review-margin-card.review-margin-comment");

  // A comment near the very top of the document.
  await page.locator(".page-wrap").first().locator(".page").click({ position: { x: 120, y: 120 } });
  await page.keyboard.press(`${MOD}+Home`);
  await addCommentAtCaret(page, "TOPMARK", "TOPNOTE");
  const topCard = sidebar.locator(".review-margin-card", { hasText: "TOPNOTE" });
  await expect(topCard).toHaveCount(1);

  // A comment far down the document (a lower page), several thousand px away.
  await scrollViewportTo(page, 9000);
  await page.waitForTimeout(150);
  await page.locator(".page-wrap").nth(9).locator(".page").click({ position: { x: 150, y: 150 } });
  await addCommentAtCaret(page, "FARMARK", "FARNOTE");
  const farCard = sidebar.locator(".review-margin-card", { hasText: "FARNOTE" });
  await expect(farCard).toHaveCount(1);

  // Scrolled down here, the far comment is mounted and the top comment — now
  // thousands of px above the viewport band — is detached from the DOM.
  await scrollViewportTo(page, 9000);
  await page.waitForTimeout(120);
  await expect(farCard).toHaveCount(1);
  await expect(topCard).toHaveCount(0);
  // The mounted card count stays small (viewport-bounded), not the full set.
  expect(await cards.count()).toBeLessThan(6);

  // Scroll back to the top: the top comment remounts, and the far comment (now
  // far below) is detached. The items are never dropped — only their DOM is
  // windowed.
  await scrollViewportTo(page, 0);
  await page.waitForTimeout(120);
  await expect(topCard).toHaveCount(1);
  await expect(farCard).toHaveCount(0);

  expect(consoleErrors).toEqual([]);
});
