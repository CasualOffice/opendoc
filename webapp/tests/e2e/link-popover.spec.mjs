// ⌘K opens a link POPOVER at the selection (`docs/184` D-13, audit
// DIALOGS-11): Google Docs' link popover and Word for the web's link callout,
// not a centred modal with a scrim. The full dialog is its "More options…".
//
// What this holds, each as the guarantee rather than the mechanism:
//   * the popover hangs from the selection (it starts below or above the
//     painted highlight and overlaps it horizontally), and nothing dims the
//     page: no `.dialog-overlay` is on screen;
//   * Apply is disabled while the field holds nothing linkable, and text that
//     is no address is marked invalid with a reason (DIALOGS-14);
//   * Enter writes ONE undoable link and gives the caret back to the document;
//   * Esc closes it and the document has focus, never <body>;
//   * "More options…" replaces it with the dialog, carrying what was typed.
import { MOD, clickIntoFirstPage, expect, expectEditorFocused, gotoEditor, moveCaretToDocStart, stableBox, test } from "./fixtures.mjs";

async function selectFirstWord(page) {
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  for (let i = 0; i < 6; i += 1) await page.keyboard.press("Shift+ArrowRight");
  return stableBox(page.locator(".overlay .highlight").first());
}

const visibleOverlays = (page) =>
  page.locator(".dialog-overlay").evaluateAll((els) => els.filter((el) => !el.hidden && el.getClientRects().length > 0).map((el) => el.id));

test("⌘K opens the link popover at the selection, with no scrim", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  const selection = await selectFirstWord(page);
  await page.keyboard.press(`${MOD}+k`);
  const popover = page.locator("#linkPopover");
  await expect(popover).toBeVisible();
  await expect(page.locator("#linkDialog")).toBeHidden();
  expect(await visibleOverlays(page)).toEqual([]);
  const box = await stableBox(popover);
  const below = box.y >= selection.y + selection.height - 1;
  const above = box.y + box.height <= selection.y + 1;
  expect(below || above, `popover ${JSON.stringify(box)} is not beside the selection ${JSON.stringify(selection)}`).toBe(true);
  expect(Math.abs(box.y - (selection.y + selection.height)) < 24 || above, "the popover is not at the selection").toBe(true);
  expect(box.x < selection.x + selection.width && box.x + box.width > selection.x, "the popover does not overlap the selection horizontally").toBe(true);
  await expect(page.locator("#linkPopoverUrl")).toBeFocused();
  expect(consoleErrors).toEqual([]);
});

test("the link field refuses what is not an address, and Enter writes one undoable link", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await selectFirstWord(page);
  await page.keyboard.press(`${MOD}+k`);
  const url = page.locator("#linkPopoverUrl");
  const apply = page.locator("#linkPopoverApply");
  await expect(apply).toBeDisabled();
  await url.fill("not a url");
  await expect(url).toHaveAttribute("aria-invalid", "true");
  await expect(page.locator("#linkPopoverError")).toHaveText("That is not a web address, an email address or a #bookmark.");
  await expect(apply).toBeDisabled();
  await url.fill("javascript:alert(1)");
  await expect(page.locator("#linkPopoverError")).toHaveText("A link can open a web page or an email only.");
  await url.fill("example.com/docs");
  await expect(url).toHaveAttribute("aria-invalid", "false");
  await expect(apply).toBeEnabled();
  await url.press("Enter");
  await expect(page.locator("#linkPopover")).toBeHidden();
  await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", "Undo Link change");
  await expectEditorFocused(page);
  expect(consoleErrors).toEqual([]);
});

test("Esc closes the popover and the document keeps the keyboard", async ({ page }) => {
  await gotoEditor(page);
  await selectFirstWord(page);
  await page.keyboard.press(`${MOD}+k`);
  await expect(page.locator("#linkPopover")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.locator("#linkPopover")).toBeHidden();
  await expectEditorFocused(page);
});

test("More options replaces the popover with the dialog and carries what was typed", async ({ page }) => {
  await gotoEditor(page);
  await selectFirstWord(page);
  await page.keyboard.press(`${MOD}+k`);
  await page.locator("#linkPopoverUrl").fill("example.org");
  await page.locator("#linkPopoverMore").click();
  await expect(page.locator("#linkPopover")).toBeHidden();
  await expect(page.locator("#linkDialog")).toBeVisible();
  await expect(page.locator("#linkDialogTitle")).toHaveText("Insert link");
  await expect(page.locator("#linkUrlInput")).toHaveValue("https://example.org");
  await expect(page.locator("#linkRemoveBtn")).toBeHidden();
});
