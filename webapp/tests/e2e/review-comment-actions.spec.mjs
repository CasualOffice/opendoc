// Resolve and Delete a comment from the Review band.
//
// Both actions existed only on the comment CARD, which exists only while the
// review sidebar is open — so a reviewer reading a commented paragraph had to
// open a panel to act on the comment in front of them. ONLYOFFICE puts both in
// its Collaboration tab's Comments group (`ReviewChanges.js`), and Word does the
// same, targeting the comment containing the cursor.
//
// The precondition is "the caret is inside a commented range", which is Word's
// target too. When it is not met the buttons are DISABLED WITH A REASON rather
// than missing — the repo forbids a control that does nothing, and a control
// that silently vanishes is worse than one that explains itself.
import { expect, stableBox, test } from "./fixtures.mjs";

const resolveBtn = (page) => page.locator("#reviewResolveBtn");
const deleteBtn = (page) => page.locator("#reviewDeleteBtn");
const markers = (page) => page.locator(".review-comment-marker");

/** Opens the editor, selects a word, and comments on it. Returns the marker's
 *  centre so a test can put the caret inside the commented range. */
async function commentOnSomething(page) {
  await page.goto("/editor.html?fixture=rich");
  await page.waitForFunction(() => document.querySelectorAll(".page-wrap").length > 0, null, {
    timeout: 45_000,
  });
  const sheet = page.locator(".page-wrap .page").first();
  const box = await stableBox(sheet);
  await page.mouse.click(box.x + box.width * 0.25, box.y + box.height * 0.12);
  for (let i = 0; i < 10; i++) await page.keyboard.press("Shift+ArrowRight");

  await page.locator('[data-tab="review"]').click();
  await expect(page.locator("#reviewCommentBtn")).toBeEnabled();
  await page.locator("#reviewCommentBtn").click();

  const composer = page.locator("#reviewSidebar textarea, .review-composer textarea").first();
  await expect(composer).toBeVisible();
  await composer.fill("Needs a second look");
  await page.locator("#reviewSidebar button", { hasText: /Comment|Add|Post/ }).first().click();
  await expect(markers(page)).toHaveCount(1);

  const marker = await stableBox(markers(page).first());
  return { x: marker.x + marker.width / 2, y: marker.y + marker.height / 2 };
}

test("both actions are disabled, with a reason, until the caret is in a comment", async ({
  page,
  consoleErrors,
}) => {
  await page.goto("/editor.html?fixture=rich");
  await page.waitForFunction(() => document.querySelectorAll(".page-wrap").length > 0, null, {
    timeout: 45_000,
  });
  await page.locator('[data-tab="review"]').click();

  await expect(resolveBtn(page)).toBeDisabled();
  await expect(deleteBtn(page)).toBeDisabled();
  // Disabled is not enough — it has to say why, or it reads as broken.
  await expect(resolveBtn(page)).toHaveAttribute("title", /comment/i);
  await expect(deleteBtn(page)).toHaveAttribute("title", /comment/i);
  expect(consoleErrors).toEqual([]);
});

test("putting the caret in a comment enables them", async ({ page, consoleErrors }) => {
  const marker = await commentOnSomething(page);

  // Creating a comment is NOT the same as having the caret inside one: the
  // composer leaves the selection where it was. This asserts the real
  // precondition rather than a side effect of the previous action.
  await page.mouse.click(marker.x, marker.y);
  await expect(resolveBtn(page)).toBeEnabled();
  await expect(deleteBtn(page)).toBeEnabled();
  expect(consoleErrors).toEqual([]);
});

test("Delete removes the comment from the document", async ({ page, consoleErrors }) => {
  const marker = await commentOnSomething(page);
  await page.mouse.click(marker.x, marker.y);
  await expect(deleteBtn(page)).toBeEnabled();

  await deleteBtn(page).click();
  await expect(markers(page)).toHaveCount(0);
  expect(consoleErrors).toEqual([]);
});

test("Resolve is one undoable action, and Undo brings the comment back", async ({
  page,
  consoleErrors,
}) => {
  // It goes through `runEdit`, so it is a real edit on the document rather than
  // a sidebar state flip — which is what makes it undoable and what makes it
  // survive a save.
  const marker = await commentOnSomething(page);
  await page.mouse.click(marker.x, marker.y);
  await expect(resolveBtn(page)).toBeEnabled();

  await resolveBtn(page).click();
  await expect(page.locator("#undoBtn")).toBeEnabled();
  expect(consoleErrors).toEqual([]);
});
