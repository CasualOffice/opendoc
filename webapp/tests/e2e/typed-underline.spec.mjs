// P1F-38 editor authoring: underline style and color are model-backed formatting,
// not a paint-only toolbar preference. Covers selection edits, mixed reflection,
// exact undo, armed typing, and the explicit Suggesting-mode boundary.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  setReviewMode,
} from "./fixtures.mjs";

async function selectForward(page, count) {
  for (let i = 0; i < count; i += 1) await page.keyboard.press("Shift+ArrowRight");
}

async function selectBackward(page, count) {
  for (let i = 0; i < count; i += 1) await page.keyboard.press("Shift+ArrowLeft");
}

async function openUnderlineMenu(page) {
  await page.locator("#underlineMenuBtn").click();
  const menu = page.locator("#underlineMenu");
  await expect(menu).toBeVisible();
  return menu;
}

test("typed underline style and color apply to a selection and undo independently", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type("UNDERLINE");
  await selectBackward(page, 9);

  let menu = await openUnderlineMenu(page);
  await menu.locator('[data-underline-style="double"]').click();
  await expect(menu).toBeHidden();
  await expect(page.locator("#underline")).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("#underline")).toHaveAttribute("data-underline-style", "double");

  menu = await openUnderlineMenu(page);
  await menu.locator('[data-color="#ff0000"]').first().click();
  await expect(menu).toBeHidden();
  await expect(page.locator("#underlineMenuBtn")).toHaveAttribute("title", /#FF0000/);

  menu = await openUnderlineMenu(page);
  await expect(menu.locator('[data-underline-style="double"]')).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await expect(menu.locator('[data-color="#ff0000"]').first()).toHaveClass(/is-active/);
  await page.keyboard.press("Escape");

  // Color and style were separate user gestures, therefore separate exact undos.
  await page.locator("#undoBtn").click();
  await expect(page.locator("#underlineMenuBtn")).toHaveAttribute("title", /Automatic color/);
  await page.locator("#undoBtn").click();
  await expect(page.locator("#underline")).toHaveAttribute("aria-pressed", "false");
  expect(consoleErrors).toEqual([]);
});

test("typed underline can be armed at a caret and typed as one undoable action", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  let menu = await openUnderlineMenu(page);
  await menu.locator('[data-underline-style="wavy"]').click();
  menu = await openUnderlineMenu(page);
  await menu.locator('[data-color="#00ff00"]').first().click();

  await page.keyboard.type("WAVE");
  await selectBackward(page, 4);
  await expect(page.locator("#underline")).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("#underline")).toHaveAttribute("data-underline-style", "wavy");
  await expect(page.locator("#underlineMenuBtn")).toHaveAttribute("title", /#00FF00/);

  await page.locator("#undoBtn").click();
  await expect(page.locator("#a11yDocument")).not.toContainText("WAVE");
  expect(consoleErrors).toEqual([]);
});

// HF-255. Suggesting mode used to refuse an underline style or colour with "not
// tracked yet". The tracked-format delta carries both, so the change is now a
// SUGGESTION: it shows, and rejecting it puts the run back.
test("Suggesting mode records an underline style as a suggestion, and Reject undoes it", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await selectForward(page, 4);
  const before = await page.locator("#underline").getAttribute("aria-pressed");
  await setReviewMode(page, "suggesting");

  const menu = await openUnderlineMenu(page);
  await menu.locator('[data-underline-style="thick"]').click();
  await expect(page.locator("#status")).not.toContainText("not tracked yet");
  await expect(page.locator("#underline")).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("#underline")).toHaveAttribute("data-underline-style", "thick");

  // It is a tracked change, not a silent direct edit: rejecting every change
  // returns the selection to what it was.
  await page.locator('[data-tab="review"]').click();
  await page.locator("#reviewRejectAllBtn").click();
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await selectForward(page, 4);
  await expect(page.locator("#underline")).toHaveAttribute("aria-pressed", before);
  expect(consoleErrors).toEqual([]);
});
