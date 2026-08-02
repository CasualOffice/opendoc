// REVIEW markup view (docs/93): the "Show changes" toggle switches the canvas to
// a read-only markup render (struck deletions, colored/underlined insertions,
// highlighted comments) via the engine's markup layout, without touching the
// model or the caret. This drives the real editor path (wasm `setShowChanges` ->
// `paginate_document_view(Markup)`).
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  setReviewMode,
} from "./fixtures.mjs";

async function setIdentity(page, name, initials = "") {
  const settingsPanel = page.locator("#settingsPanel");
  if (await settingsPanel.isHidden()) {
    await page.locator("#settingsBtn").click();
  }
  await expect(settingsPanel).toBeVisible();
  await page.locator("#authorName").fill(name);
  await page.locator("#authorInitials").fill(initials);
  await page.keyboard.press("Escape");
  await expect(settingsPanel).toBeHidden();
}

test("the Show changes toggle renders the read-only markup view and back (REVIEW-GAP markup)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // Author a tracked deletion so there is markup to show.
  await setReviewMode(page, "suggesting");
  await setIdentity(page, "Ada Lovelace");
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.press("Shift+ArrowRight");
  await page.keyboard.press("Shift+ArrowRight");
  await page.keyboard.press("Shift+ArrowRight");
  await page.keyboard.press("Backspace"); // tracked deletion of 3 chars

  const toggle = page.locator("#showChangesToggle");
  await expect(toggle).toBeVisible();
  await expect(toggle).toBeEnabled();
  await expect(toggle).toHaveAttribute("aria-pressed", "false");

  // Turn on the markup preview: it re-renders the canvas (still at least one
  // page) with no engine errors, and reflects the pressed state.
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator(".page-wrap .page").first()).toBeVisible();

  // Turn it back off.
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-pressed", "false");
  await expect(page.locator(".page-wrap .page").first()).toBeVisible();

  expect(consoleErrors).toEqual([]);
});
