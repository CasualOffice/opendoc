// Insert/Edit link dialog (PR: hyperlink dialog). Replaces the old
// window.prompt hyperlink UI with a Word/Docs-style modal: a "Text to display"
// field, a Web-address / Place-in-this-document target picker (bookmarks are
// listed by name so a user never hand-types "#anchor"), and an optional
// ScreenTip. The editor paints on canvas, so links are proven with two
// render-independent signals: the Undo control's single "Link change" label
// (the one undoable action the setHyperlink op produces) and the hover chip the
// canvas hit-test surfaces over a link. Canvas geometry is read after the caret
// paints, via boundingBox/expect.poll, never a fixed sleep.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  setReviewMode,
  MOD,
} from "./fixtures.mjs";

// Selects `count` characters forward from the current caret.
async function selectForward(page, count) {
  for (let i = 0; i < count; i += 1) await page.keyboard.press("Shift+ArrowRight");
}

// Opens the link dialog on the current selection through the real ⌘K binding.
async function openLinkDialog(page) {
  await page.keyboard.press(`${MOD}+k`);
  await expect(page.locator("#linkDialog")).toBeVisible();
}

// Places the caret two characters into a link at the document start, then
// clicks that exact painted-caret point so the canvas hit-test treats it as a
// bare click on the link (the gesture that surfaces the hover chip).
async function clickTwoCharsIn(page) {
  await moveCaretToDocStart(page);
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("ArrowRight");
  const box = await page.locator(".overlay .caret").first().boundingBox();
  await page.mouse.click(box.x + 2, box.y + box.height / 2);
}

// Opens the bookmark manager via the command palette (which preserves the
// selection), so the place picker later has a bookmark to offer.
async function addBookmark(page, name) {
  await page.keyboard.press(`${MOD}+Shift+P`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  await page.locator("#cmdInput").fill("bookmark manager");
  await page.locator("#cmdInput").press("Enter");
  await expect(page.locator("#bookmarkDialog")).toBeVisible();
  await page.locator("#bookmarkNameInput").fill(name);
  await page.locator("#bookmarkAddBtn").click();
  await expect(page.locator("#bookmarkList .bookmark-row")).toHaveCount(1);
  await page.locator("#bookmarkDone").click();
  await expect(page.locator("#bookmarkDialog")).toBeHidden();
}

test("hyperlink dialog: insert a URL, edit it via the chip, and remove it", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await selectForward(page, 8);

  const dialog = page.locator("#linkDialog");
  const undoBtn = page.locator("#undoBtn");
  await expect(undoBtn).toBeDisabled();

  // ⌘K opens the dialog for a fresh insert: title reads "Insert link", the
  // display-text field is prefilled with the selection, and there is no Remove.
  await openLinkDialog(page);
  await expect(page.locator("#linkDialogTitle")).toHaveText("Insert link");
  await expect(page.locator("#linkTextInput")).not.toHaveValue("");
  await expect(page.locator("#linkRemoveBtn")).toBeHidden();

  // An empty target is rejected inline — no link is created, the dialog stays.
  await page.locator("#linkApplyBtn").click();
  await expect(page.locator("#linkDialogNote")).toHaveClass(/error/);
  await expect(dialog).toBeVisible();
  await expect(undoBtn).toBeDisabled();

  // Enter a URL and apply with Enter → exactly one undoable "Link change".
  await page.locator("#linkUrlInput").fill("https://example.com");
  await page.locator("#linkUrlInput").press("Enter");
  await expect(dialog).toBeHidden();
  await expect(undoBtn).toHaveAttribute("aria-label", "Undo Link change");

  // Clicking inside the linked text surfaces the hover chip with an Edit action.
  await clickTwoCharsIn(page);
  await expect(page.locator("#linkChip")).toBeVisible();
  await expect(page.locator("#linkChipEdit")).toBeVisible();

  // Edit → the dialog reopens as "Edit link", prefilled with the existing URL
  // and offering Remove.
  await page.locator("#linkChipEdit").click();
  await expect(dialog).toBeVisible();
  await expect(page.locator("#linkDialogTitle")).toHaveText("Edit link");
  await expect(page.locator("#linkUrlInput")).toHaveValue("https://example.com");
  await expect(page.locator("#linkRemoveBtn")).toBeVisible();

  // Remove clears the link: the same text no longer surfaces a link chip.
  await page.locator("#linkRemoveBtn").click();
  await expect(dialog).toBeHidden();
  await clickTwoCharsIn(page);
  await expect(page.locator("#linkChip")).toBeHidden();

  expect(consoleErrors).toEqual([]);
});

test("hyperlink dialog: link to a bookmark from the place picker", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await selectForward(page, 8);

  // Seed a bookmark; the palette keeps the selection so the same range is still
  // selected afterwards.
  await addBookmark(page, "Intro");

  const undoBtn = page.locator("#undoBtn");

  // Open the dialog and switch to "Place in this document": the picker offers
  // the bookmark by name (never a hand-typed "#anchor").
  await openLinkDialog(page);
  await page.locator("#linkModePlace").click();
  const placeSelect = page.locator("#linkPlaceSelect");
  await expect(placeSelect.locator("option", { hasText: "Intro" })).toHaveCount(1);
  await placeSelect.selectOption({ label: "Intro" });
  await page.locator("#linkApplyBtn").click();

  // Applied as one undoable "Link change" — the internal (bookmark) link exists.
  await expect(page.locator("#linkDialog")).toBeHidden();
  await expect(undoBtn).toHaveAttribute("aria-label", "Undo Link change");

  expect(consoleErrors).toEqual([]);
});

test("hyperlink dialog: fails closed in Viewing mode", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // Viewing is read-only. Re-focus the canvas (the mode control took focus) and
  // select text there so ⌘K reaches the gate rather than being ignored as a
  // chrome shortcut.
  await setReviewMode(page, "viewing");
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await selectForward(page, 8);

  await page.keyboard.press(`${MOD}+k`);
  await expect(page.locator("#linkDialog")).toBeHidden();
  await expect(page.locator("#status")).toContainText("read-only");

  expect(consoleErrors).toEqual([]);
});
