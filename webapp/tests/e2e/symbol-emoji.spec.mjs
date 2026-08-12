// Insert ▸ Symbol and Insert ▸ Emoji pickers (PR: symbol/emoji picker). Word's
// Insert ▸ Symbol and Docs' special-characters / emoji, built to that standard:
// a categorized grid whose glyphs insert at the caret and KEEP the dialog open
// so several can be added. The editor paints on canvas, so — as in
// field-insert.spec — each insertion is proven by two render-independent
// signals: the caret advances past the inserted glyph's inline width (the layout
// reflowed to include it) and the Undo control reports the single undoable
// "Paste" action the shared gated text path produces (one glyph = one Undo). The
// insert reuses `pasteText` → `insertPlainTextAs` with the non-coalescing "paste"
// HistoryKind, so it is an ordinary text edit: read-only in Viewing.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  setReviewMode,
  MOD,
} from "./fixtures.mjs";

// The collapsed caret's rounded on-screen x, from the overlay the editor draws
// its caret into. Inserting a glyph at the caret advances this value.
function caretX(page) {
  return page.evaluate(() => {
    const caret = document.querySelector(".overlay .caret");
    return caret ? Math.round(Number.parseFloat(caret.style.left)) : null;
  });
}

// Opens a picker (`symbol` or `emoji`) through the real Insert menu affordance,
// exactly as a user would reach it.
async function openPickerViaMenu(page, which) {
  await page.locator('.app-menu-button[data-menu="insert"]').click();
  const item = page.locator(`#appMenuPopover .app-menu-item[data-command="insert.${which}"]`);
  await expect(item).toBeVisible();
  await item.click();
  await expect(page.locator(`#${which}Dialog`)).toBeVisible();
}

test("insert symbol: a glyph lands at the caret as one undoable action, the picker stays open, and Esc closes it", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  const undoBtn = page.locator("#undoBtn");
  await expect(undoBtn).toBeDisabled(); // a fresh document has nothing to undo

  await openPickerViaMenu(page, "symbol");
  await expect(page.locator("#symbolDialog .panel-head")).toBeVisible();
  await expect(page.locator("#symbolDialog .panel-body")).toBeVisible();
  await expect(page.locator("#symbolDialog .dialog-card")).toHaveCount(0);
  expect(await page.locator("#viewport").evaluate((viewport) => {
    const panel = document.querySelector("#symbolDialog");
    return !!panel && viewport.compareDocumentPosition(panel) & Node.DOCUMENT_POSITION_FOLLOWING;
  })).toBe(true);
  const startX = await caretX(page);

  // The default (Currency) category leads with the Euro sign; clicking it
  // inserts € at the caret. The caret advances (it laid out inline) and the
  // insert is one undoable "Paste".
  const euro = page.locator('#symbolGrid .glyph-cell[data-glyph="€"]');
  await expect(euro).toBeVisible();
  await euro.click();
  await expect.poll(() => caretX(page)).toBeGreaterThan(startX);
  await expect(undoBtn).toBeEnabled();
  await expect(undoBtn).toHaveAttribute("aria-label", "Undo Paste");

  // KEEP-OPEN (Word/Docs): the dialog is still open after inserting, so a second
  // glyph can be added — and it is a SEPARATE undoable action (paste never
  // coalesces), so the caret advances again.
  await expect(page.locator("#symbolDialog")).toBeVisible();
  const afterFirst = await caretX(page);
  await page.locator('#symbolGrid .glyph-cell[data-glyph="£"]').click();
  await expect.poll(() => caretX(page)).toBeGreaterThan(afterFirst);

  // Search filters the grid by name across every category.
  await page.locator("#symbolSearch").fill("omega");
  await expect(page.locator('#symbolGrid .glyph-cell[data-glyph="Ω"]')).toBeVisible();
  await expect(page.locator('#symbolGrid .glyph-cell[data-glyph="€"]')).toHaveCount(0);

  // Esc closes the picker.
  await page.keyboard.press("Escape");
  await expect(page.locator("#symbolDialog")).toBeHidden();

  // Each insertion is exactly ONE undo: two undos remove both glyphs and return
  // the caret to the paragraph start with nothing left to undo.
  await undoBtn.click();
  await expect.poll(() => caretX(page)).toBe(afterFirst);
  await undoBtn.click();
  await expect.poll(() => caretX(page)).toBe(startX);
  await expect(undoBtn).toBeDisabled();

  expect(consoleErrors).toEqual([]);
});

test("insert emoji: a glyph lands at the caret as one undoable action, the picker stays open, and Esc closes it", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  const undoBtn = page.locator("#undoBtn");
  await expect(undoBtn).toBeDisabled();

  await openPickerViaMenu(page, "emoji");
  const startX = await caretX(page);

  const grin = page.locator('#emojiGrid .glyph-cell[data-glyph="😀"]');
  await expect(grin).toBeVisible();
  await grin.click();
  await expect.poll(() => caretX(page)).toBeGreaterThan(startX);
  await expect(undoBtn).toBeEnabled();
  await expect(undoBtn).toHaveAttribute("aria-label", "Undo Paste");

  // Stays open for a second insertion (keep-open).
  await expect(page.locator("#emojiDialog")).toBeVisible();
  await page.locator("#emojiSearch").fill("fire");
  await expect(page.locator('#emojiGrid .glyph-cell[data-glyph="🔥"]')).toBeVisible();

  await page.keyboard.press("Escape");
  await expect(page.locator("#emojiDialog")).toBeHidden();

  // One undo removes the single inserted emoji.
  await undoBtn.click();
  await expect.poll(() => caretX(page)).toBe(startX);
  await expect(undoBtn).toBeDisabled();

  expect(consoleErrors).toEqual([]);
});

test("insert symbol: blocked (read-only) in Viewing mode, nothing inserted", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await setReviewMode(page, "viewing");

  // Reached via the command palette; the mutation fails closed before the picker
  // opens — the read-only banner shows, the dialog stays hidden, and no action
  // enters history.
  await page.keyboard.press(`${MOD}+Shift+P`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  await page.locator("#cmdInput").fill("Symbol");
  await page.locator("#cmdInput").press("Enter");

  await expect(page.locator("#status")).toContainText("read-only");
  await expect(page.locator("#symbolDialog")).toBeHidden();
  await expect(page.locator("#undoBtn")).toBeDisabled();

  expect(consoleErrors).toEqual([]);
});
