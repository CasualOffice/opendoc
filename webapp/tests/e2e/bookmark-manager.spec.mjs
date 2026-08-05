// Bookmark manager UX: the Word/Docs-style dialog over the engine's
// create/rename/delete bookmark ops (PR #390) plus the existing
// bookmarkPosition navigation. One flow exercises the full lifecycle — add
// from a selection, Go to, inline Rename, Delete, and Undo — proving each
// mutation is a single undoable action and that navigation lands the caret on
// the bookmark. A second test covers the Insert menu entry and inline
// validation.
import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart, MOD } from "./fixtures.mjs";

// Selects `count` characters forward from the current caret.
async function selectForward(page, count) {
  for (let i = 0; i < count; i += 1) await page.keyboard.press("Shift+ArrowRight");
}

// Opens the bookmark dialog through the command palette (⌘⇧P). The palette
// preserves the current selection, so the manager can bookmark it. Used for the
// lifecycle flow; the Insert-menu entry is covered separately.
async function openBookmarkManager(page) {
  await page.keyboard.press(`${MOD}+Shift+P`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  await page.locator("#cmdInput").fill("bookmark manager");
  await page.locator("#cmdInput").press("Enter");
  await expect(page.locator("#bookmarkDialog")).toBeVisible();
}

// The rounded on-screen caret position, for asserting navigation moved it.
function caretPosition(page) {
  return page.evaluate(() => {
    const caret = document.querySelector(".overlay .caret");
    const wraps = [...document.querySelectorAll(".page-wrap")];
    return caret
      ? { page: wraps.indexOf(caret.closest(".page-wrap")), top: Math.round(Number.parseFloat(caret.style.top)) }
      : null;
  });
}

test("bookmark manager: add from selection, go to, rename, delete, and undo", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await selectForward(page, 8);

  const dialog = page.locator("#bookmarkDialog");
  const list = page.locator("#bookmarkList");

  // Open the manager with the name field focused and no bookmarks yet.
  await openBookmarkManager(page);
  await expect(page.locator("#bookmarkNameInput")).toBeFocused();
  await expect(page.locator("#bookmarkEmpty")).toBeVisible();

  // Add a bookmark named "Intro" over the selection → it appears in the list.
  await page.locator("#bookmarkNameInput").fill("Intro");
  await page.locator("#bookmarkAddBtn").click();
  await expect(list.locator(".bookmark-row")).toHaveCount(1);
  await expect(list.locator(".bookmark-name")).toHaveText("Intro");
  await expect(page.locator("#bookmarkEmpty")).toBeHidden();
  await page.locator("#bookmarkDone").click();
  await expect(dialog).toBeHidden();

  // Move the caret well away from the bookmark, then Go to → the caret returns.
  for (let i = 0; i < 6; i += 1) await page.keyboard.press("ArrowDown");
  const away = await caretPosition(page);
  await openBookmarkManager(page);
  await list.locator(".bookmark-goto").click();
  await expect(dialog).toBeHidden();
  await expect.poll(() => caretPosition(page)).not.toEqual(away);

  // Inline rename "Intro" → "Overview".
  await openBookmarkManager(page);
  await list.locator(".bookmark-action[title='Rename']").click();
  const renameInput = list.locator(".bookmark-rename-input");
  await expect(renameInput).toBeFocused();
  await renameInput.fill("Overview");
  await renameInput.press("Enter");
  await expect(list.locator(".bookmark-name")).toHaveText("Overview");
  await expect(list.locator(".bookmark-row")).toHaveCount(1);

  // Delete → the list is empty again.
  await list.locator(".bookmark-action[title='Delete']").click();
  await expect(list.locator(".bookmark-row")).toHaveCount(0);
  await expect(page.locator("#bookmarkEmpty")).toBeVisible();
  await page.locator("#bookmarkDone").click();

  // Undo restores the deleted bookmark (one undoable "Bookmark change").
  await page.locator("#undoBtn").click();
  await openBookmarkManager(page);
  await expect(list.locator(".bookmark-row")).toHaveCount(1);
  await expect(list.locator(".bookmark-name")).toHaveText("Overview");
  await page.locator("#bookmarkClose").click();
  await expect(dialog).toBeHidden();

  expect(consoleErrors).toEqual([]);
});

test("bookmark manager: Insert menu opens it; name is required; Escape closes it", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  // Insert → Bookmark… opens the manager from the real application menu.
  await page.locator('.app-menu-button[data-menu="insert"]').click();
  const menuItem = page.locator('#appMenuPopover .app-menu-item[data-command="insert.bookmark"]');
  await expect(menuItem).toBeVisible();
  await expect(menuItem).toContainText("Bookmark");
  await menuItem.click();
  await expect(page.locator("#bookmarkDialog")).toBeVisible();

  // No name: the add is rejected inline (error note), nothing is created.
  await page.locator("#bookmarkAddBtn").click();
  await expect(page.locator("#bookmarkAddNote")).toHaveClass(/error/);
  await expect(page.locator("#bookmarkList .bookmark-row")).toHaveCount(0);

  // Esc closes the dialog cleanly.
  await page.keyboard.press("Escape");
  await expect(page.locator("#bookmarkDialog")).toBeHidden();

  expect(consoleErrors).toEqual([]);
});
