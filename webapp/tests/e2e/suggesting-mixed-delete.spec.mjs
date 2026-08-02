// REVIEW-GAP-007 phase 2b (docs/86 decision 2): a delete that spans the
// author's own pending insertion AND accepted text must remove the
// not-yet-accepted insertion outright while suggesting-deletion of the accepted
// remainder — Word semantics. This used to fail as an unsupported cross-wrapper
// delete. Exercises the real Suggesting-mode path (wasm `suggest_delete` ->
// `strip_authored_insertions_in_range` + `wrap_review_deletion`).
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

test("deleting across own pending insertion and accepted text removes the insert and suggests-deletes the rest (REVIEW-GAP-007)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await setReviewMode(page, "suggesting");
  await setIdentity(page, "Ada Lovelace");
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // Author a pending insertion "XYZ" at the very start, so the paragraph reads
  // «insertion XYZ»«accepted original text…».
  await page.keyboard.type("XYZ");
  const insertionMarkers = page.locator(".overlay .review-insertion-marker");
  const deletionMarkers = page.locator(".overlay .review-deletion-marker");
  await expect(insertionMarkers).not.toHaveCount(0);
  await expect(deletionMarkers).toHaveCount(0);

  // Select [1, 5): "YZ" (inside our own insertion) + the first two accepted
  // characters, then delete.
  await moveCaretToDocStart(page);
  await page.keyboard.press("ArrowRight");
  for (let i = 0; i < 4; i += 1) await page.keyboard.press("Shift+ArrowRight");
  await page.keyboard.press("Backspace");
  await page.waitForTimeout(200);

  // No rejection, and both a surviving insertion (the un-deleted "X") and a new
  // tracked deletion (the two accepted characters) are present.
  expect(await page.locator("#status").textContent()).not.toMatch(
    /isn't supported|not supported|requires/i,
  );
  await expect(insertionMarkers).not.toHaveCount(0);
  await expect(deletionMarkers).not.toHaveCount(0);
  expect(consoleErrors).toEqual([]);
});
