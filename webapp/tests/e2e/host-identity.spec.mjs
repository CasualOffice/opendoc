import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart, MOD } from "./fixtures.mjs";

// Clicking into the editor surface is also how tests recover keyboard focus
// after interacting with unrelated chrome (like the Settings popover), so
// every `setIdentity` call site re-focuses the canvas afterward before typing.

// REVIEW-GAP-013 (docs/81): the host application must be able to supply the
// current reviewer's identity through an explicit API (`doc.setActiveAuthor`,
// wired here to the Settings panel's "Reviewer identity" fields) rather than
// the editor reading a hidden legacy DOM input. These specs drive that real
// UI control and assert the resulting attribution on newly created comments,
// end to end through the WASM engine.

async function setIdentity(page, name, initials = "") {
  const settingsPanel = page.locator("#settingsPanel");
  if (await settingsPanel.isHidden()) {
    await page.locator("#settingsBtn").click();
  }
  await expect(settingsPanel).toBeVisible();
  await page.locator("#authorName").fill(name);
  await page.locator("#authorInitials").fill(initials);
  // Close the popover so it does not intercept the next click.
  await page.keyboard.press("Escape");
  await expect(settingsPanel).toBeHidden();
}

async function addComment(page, marker, text) {
  await page.keyboard.type(marker);
  for (let i = 0; i < marker.length; i++) {
    await page.keyboard.press("Shift+ArrowLeft");
  }
  await page.locator("#selComment").click();
  const sidebar = page.locator("#reviewSidebar");
  const composer = sidebar.locator('[data-testid="review-comment-composer"]');
  await expect(composer).toBeVisible();
  await composer.fill(text);
  await sidebar.locator('[data-testid="review-comment-submit"]').click();
  // Wait for the committed card itself (not just the click) before returning,
  // so a caller that immediately changes identity and adds another comment
  // cannot race this one's still-in-flight WASM call + re-render.
  await expect(
    sidebar.locator(".review-margin-card.review-margin-comment").filter({ hasText: text }),
  ).toBeVisible();
}

test("the legacy hidden author input is gone and Settings exposes a real identity control", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await expect(page.locator("#reviewAuthor")).toHaveCount(0);
  await page.locator("#settingsBtn").click();
  await expect(page.locator("#settingsPanel")).toBeVisible();
  await expect(page.locator("#authorName")).toBeVisible();
  await expect(page.locator("#authorInitials")).toBeVisible();
  expect(consoleErrors).toEqual([]);
});

test("setting a host identity attributes a newly created comment to it", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await setIdentity(page, "Ada Lovelace");

  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await addComment(page, "IDENTITY_TARGET_ONE", "First comment");

  const sidebar = page.locator("#reviewSidebar");
  const card = sidebar.locator(".review-margin-card.review-margin-comment").first();
  await expect(card).toContainText("First comment");
  await expect(card.locator(".review-margin-card-head strong")).toHaveText("Ada Lovelace");
  // No initials were supplied, so the engine derives them from the name for
  // the avatar/initial fallback rather than the webapp recomputing them.
  await expect(card.locator(".review-margin-avatar")).toHaveText("A");

  expect(consoleErrors).toEqual([]);
});

test("changing the host identity attributes later comments differently without rewriting earlier ones", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await setIdentity(page, "Ada Lovelace");

  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await addComment(page, "IDENTITY_TARGET_A", "By Ada");

  await setIdentity(page, "Grace Hopper", "GH");

  // Placed at the document end (rather than doc start again) so its marked
  // range cannot overlap the first comment's marker pair still sitting at
  // the very start of the document.
  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+End`);
  await addComment(page, "IDENTITY_TARGET_B", "By Grace");

  const sidebar = page.locator("#reviewSidebar");
  const authors = await sidebar
    .locator(".review-margin-card.review-margin-comment .review-margin-card-head strong")
    .allTextContents();
  expect(authors).toContain("Ada Lovelace");
  expect(authors).toContain("Grace Hopper");

  const graceCard = sidebar
    .locator(".review-margin-card.review-margin-comment")
    .filter({ hasText: "By Grace" });
  await expect(graceCard.locator(".review-margin-avatar")).toHaveText("G");

  expect(consoleErrors).toEqual([]);
});
