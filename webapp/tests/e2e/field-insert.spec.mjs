// Insert ▸ Field UI (PR: field insert). A Word/Docs-style field inserter over
// the engine's `insertField` op. The editor renders on canvas, so there is no
// DOM glyph to assert on; instead each test proves the field via two
// render-independent signals: the caret advances past the field's inline width
// (the layout reflowed to include it) and the Undo control reports the single
// undoable "Field change" the op produces. Because the canvas paints only after
// a web-font fetch, caret geometry is read with expect.poll, never a fixed sleep.
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  setReviewMode,
  MOD,
} from "./fixtures.mjs";

// The collapsed caret's rounded on-screen x, from the same overlay the editor
// draws its caret into. A field inserted at the caret advances this value by the
// field's inline width — the render-layer proof it laid out inline.
function caretX(page) {
  return page.evaluate(() => {
    const caret = document.querySelector(".overlay .caret");
    return caret ? Math.round(Number.parseFloat(caret.style.left)) : null;
  });
}

// Inserts a field of `kind` through the real Insert ▸ Field affordance and its
// picker dialog, exactly as a user would reach it.
//
// Through the Insert TAB, not the Insert menu. The menu bar is the compact
// chrome's axis now (docs/122), and driving it here would put the rest of the
// test in a chrome whose band — `#undoBtn` included — is not on screen. The
// button carries `data-command="insert.field"`, so this is still the same
// command, reached the way a user in the default chrome reaches it.
async function insertFieldViaMenu(page, kind) {
  await page.locator("#tabInsert").click();
  const menuItem = page.locator('#panelInsert [data-command="insert.field"]');
  await expect(menuItem).toBeVisible();
  await menuItem.click();
  await expect(page.locator("#fieldDialog")).toBeVisible();
  await page.locator(`.field-choice[data-field-kind="${kind}"]`).click();
  await expect(page.locator("#fieldDialog")).toBeHidden();
}

test("insert field: page and date fields render inline and each undoes as one action", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // Undo lives on the HOME band, and reaching the picker meant switching to the
  // Insert tab. Back to Home first: a hidden button is not a clickable one.
  await page.locator("#tabHome").click();
  const undoBtn = page.locator("#undoBtn");
  // A freshly opened document has no history: nothing to undo yet.
  await expect(undoBtn).toBeDisabled();
  const startX = await caretX(page);

  // A PAGE field (recomputed at pagination, no cached text) renders inline: the
  // caret advances past its width, and the insert is one undoable "Field change".
  await insertFieldViaMenu(page, "page");
  await expect(undoBtn).toHaveAttribute("aria-label", "Undo Field change");
  await expect.poll(() => caretX(page)).toBeGreaterThan(startX);

  // A single Undo removes it: the caret returns to the paragraph start and there
  // is nothing left to undo (the field was the only history entry).
  await page.locator("#tabHome").click();
  await undoBtn.click();
  await expect.poll(() => caretX(page)).toBe(startX);
  await expect(undoBtn).toBeDisabled();

  // A DATE field caches the host-formatted display string and behaves the same:
  // inline render, single "Field change", removed by one Undo.
  await insertFieldViaMenu(page, "date");
  await expect(undoBtn).toHaveAttribute("aria-label", "Undo Field change");
  await expect.poll(() => caretX(page)).toBeGreaterThan(startX);
  await page.locator("#tabHome").click();
  await undoBtn.click();
  await expect(undoBtn).toBeDisabled();

  expect(consoleErrors).toEqual([]);
});

test("insert field: the picker is keyboard accessible and Escape closes it without inserting", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  await page.locator("#tabInsert").click();
  await page.locator('#panelInsert [data-command="insert.field"]').click();
  const dialog = page.locator("#fieldDialog");
  await expect(dialog).toBeVisible();

  // The first field choice is focused for keyboard use; ArrowDown moves through
  // the list.
  const first = page.locator('.field-choice[data-field-kind="page"]');
  await expect(first).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(page.locator('.field-choice[data-field-kind="numpages"]')).toBeFocused();

  // Escape closes the picker and inserts nothing (still nothing to undo).
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
  await page.locator("#tabHome").click();
  await expect(page.locator("#undoBtn")).toBeDisabled();

  expect(consoleErrors).toEqual([]);
});

test("insert field: blocked in Viewing mode (read-only, nothing inserted)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await setReviewMode(page, "viewing");

  // The per-kind palette command stays enabled with a caret, but the mutation
  // fails closed: the read-only banner shows and no "Field change" enters
  // history (the Undo control is still disabled).
  await page.keyboard.press(`${MOD}+Shift+P`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  await page.locator("#cmdInput").fill("Insert field: Page number");
  await page.locator("#cmdInput").press("Enter");

  await expect(page.locator("#status")).toContainText("read-only");
  await page.locator("#tabHome").click();
  await expect(page.locator("#undoBtn")).toBeDisabled();

  expect(consoleErrors).toEqual([]);
});
