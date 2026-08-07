// docs/67 audit row 8 (command/shortcut coverage + ⌘K conflict). ⌘K used to
// open the command palette, colliding with the Word / Google Docs / Pages
// convention where ⌘K inserts/edits a hyperlink. The palette now lives on ⌘⇧P
// (VS Code convention) and ⌘K authors a link on the current selection; a batch
// of previously mouse-only commands (review mode, add comment, accept/reject
// all, super/subscript) gained command-palette entries with shortcut hints.
import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart, MOD } from "./fixtures.mjs";

async function typeAndSelect(page, marker) {
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type(marker);
  for (let i = 0; i < marker.length; i++) await page.keyboard.press("Shift+ArrowLeft");
}

test("⌘K authors a link on the selection and no longer opens the command palette", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await typeAndSelect(page, "CMDKLINK");

  // ⌘K opens the link dialog (not the palette); enter a URL and apply.
  await page.keyboard.press(`${MOD}+k`);
  await expect(page.locator("#cmdPalette")).toBeHidden();
  await expect(page.locator("#linkDialog")).toBeVisible();
  await page.locator("#linkUrlInput").fill("https://example.com/cmdk");
  await page.locator("#linkUrlInput").press("Enter");
  await expect(page.locator("#linkDialog")).toBeHidden();

  // The selection is now a hyperlink: right-clicking it offers edit/remove
  // (not add), which is only true when a link exists over the range.
  const box = await page.locator(".overlay .highlight").first().boundingBox();
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2, { button: "right" });
  const menu = page.locator(".editor-context-menu");
  await expect(menu.locator('[data-command-id="link.edit"]')).toBeVisible();
  await expect(menu.locator('[data-command-id="link.add"]')).toHaveCount(0);
  await page.keyboard.press("Escape");

  expect(consoleErrors).toEqual([]);
});

test("⌘K with no selection prompts to select text instead of failing silently", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.press(`${MOD}+k`);
  await expect(page.locator("#cmdPalette")).toBeHidden();
  await expect(page.locator("#status")).toContainText("Select text to add a link");
  expect(consoleErrors).toEqual([]);
});

test("the command palette opens on ⌘⇧P", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await page.keyboard.press(`${MOD}+Shift+p`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  await expect(page.locator("#cmdInput")).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(page.locator("#cmdPalette")).toBeHidden();
  expect(consoleErrors).toEqual([]);
});

test("a previously mouse-only command (Add comment) is reachable and executable from the palette, with its shortcut hint", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await typeAndSelect(page, "PALETTECOMMENT");

  await page.keyboard.press(`${MOD}+Shift+p`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  await page.locator("#cmdInput").fill("Add comment");

  const item = page.locator(".cmd-item", { hasText: "Add comment" }).first();
  await expect(item).toBeVisible();
  // The palette teaches the shortcut in the hint column.
  await expect(item.locator(".cmd-hint")).toHaveText("⌘⌥M");
  await item.click();

  // Running it opens the review composer over the selection — the same action
  // the mouse-only "Add comment" selection button performs.
  await expect(page.locator('[data-testid="review-comment-composer"]')).toBeVisible();
  expect(consoleErrors).toEqual([]);
});

test("⌘⌥M adds a comment on the selection", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await typeAndSelect(page, "ALTMCOMMENT");
  await page.keyboard.press(`${MOD}+Alt+m`);
  const composer = page.locator('[data-testid="review-comment-composer"]');
  await expect(composer).toBeVisible();
  await composer.fill("Shortcut comment");
  await page.locator('[data-testid="review-comment-submit"]').click();
  await expect(
    page.locator("#reviewSidebar .review-margin-card.review-margin-comment").filter({ hasText: "Shortcut comment" }),
  ).toBeVisible();
  expect(consoleErrors).toEqual([]);
});
