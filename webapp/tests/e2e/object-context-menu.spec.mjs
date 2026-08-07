// Right-clicking a selected object (image / text box) must show an OBJECT
// context menu — Wrap / Alt text / Crop / Delete — not the paragraph-text menu.
// The commands reuse the exact functions the floating object context bar wires
// up (setObjectWrap / openAltTextDialog / enterCropMode / deleteSelectedObject).
// The `?fixture=float` document holds one top-level floating image, selected as
// in object-edit.spec.mjs.
import { test, expect, setReviewMode } from "./fixtures.mjs";

// The floating image sits near the top-left of page 1 in the float fixture.
const FLOAT_POS = { fx: 0.14, fy: 0.11 };

async function gotoFloat(page) {
  await page.goto("/editor.html?fixture=float");
  await page.waitForFunction(
    () => {
      const s = document.getElementById("status");
      return s && s.textContent === "" && document.querySelectorAll(".page-wrap").length > 0;
    },
    null,
    { timeout: 45_000 },
  );
}

// Resolves the client point at the image inside page 1.
async function imagePoint(page) {
  const canvas = page.locator(".page-wrap .page").first();
  const box = await canvas.boundingBox();
  return { x: box.x + box.width * FLOAT_POS.fx, y: box.y + box.height * FLOAT_POS.fy };
}

async function rightClickImage(page) {
  const p = await imagePoint(page);
  await page.mouse.click(p.x, p.y, { button: "right" });
  // Right-click selects the object as a unit before opening its menu.
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
}

const menu = (page) => page.locator(".editor-context-menu");
const item = (page, id) => menu(page).locator(`[data-command-id="${id}"]`);

test("right-clicking an image shows the OBJECT menu, not the text menu", async ({
  page,
  consoleErrors,
}) => {
  await gotoFloat(page);
  await rightClickImage(page);

  await expect(menu(page)).toBeVisible();

  // Object commands are present and enabled (Editing mode).
  await expect(item(page, "object.altText")).toBeEnabled();
  await expect(item(page, "object.crop")).toBeEnabled(); // picture-only
  await expect(item(page, "object.delete")).toBeEnabled();
  // Wrap is a submenu parent (the float image is anchored/floating).
  const wrap = item(page, "object.wrap");
  await expect(wrap).toBeVisible();
  await expect(wrap).toHaveAttribute("aria-haspopup", "menu");

  // Paragraph-text commands must NOT appear on an object menu.
  await expect(item(page, "edit.copy")).toHaveCount(0);
  await expect(item(page, "edit.paste")).toHaveCount(0);
  await expect(item(page, "paragraph.properties")).toHaveCount(0);
  await expect(item(page, "format.menu")).toHaveCount(0);
  await expect(item(page, "link.add")).toHaveCount(0);
  await expect(item(page, "comment.add")).toHaveCount(0);

  // The wrap submenu lists the wrap modes, each an object command.
  await wrap.hover();
  const submenu = page.locator(".editor-submenu").last();
  await expect(submenu.locator('[data-command-id="object.wrap.square"]')).toBeVisible();
  await expect(submenu.locator('[data-command-id="object.wrap.behind"]')).toBeVisible();

  await page.keyboard.press("Escape");
  expect(consoleErrors).toEqual([]);
});

test("invoking Delete from the object menu removes the object (one undo)", async ({
  page,
  consoleErrors,
}) => {
  await gotoFloat(page);
  await rightClickImage(page);
  await expect(page.locator(".overlay .object-outline")).toHaveCount(1);

  await item(page, "object.delete").click();

  // The object is gone: the menu closed, selection cleared, no outline painted.
  await expect(menu(page)).toBeHidden();
  await expect(page.locator("#pages")).not.toHaveAttribute("data-object-mode", /.*/);
  await expect(page.locator(".overlay .object-outline")).toHaveCount(0);
  await expect(page.locator("#undoBtn")).toBeEnabled();

  // One Undo restores the object; it is selectable again.
  await page.locator("#undoBtn").click();
  await expect(page.locator("#redoBtn")).toBeEnabled();
  await rightClickImage(page);
  await expect(page.locator(".overlay .object-outline")).toHaveCount(1);
  await page.keyboard.press("Escape");

  expect(consoleErrors).toEqual([]);
});

test("Shift+F10 opens the object menu when an object is selected", async ({
  page,
  consoleErrors,
}) => {
  await gotoFloat(page);
  // Select the object first (a left click), then invoke the keyboard menu key.
  const p = await imagePoint(page);
  await page.mouse.click(p.x, p.y);
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");

  await page.keyboard.press("Shift+F10");
  await expect(menu(page)).toBeVisible();
  await expect(item(page, "object.delete")).toBeVisible();
  await expect(item(page, "object.altText")).toBeVisible();
  await expect(item(page, "paragraph.properties")).toHaveCount(0);

  await page.keyboard.press("Escape");
  expect(consoleErrors).toEqual([]);
});

test("object menu commands are gated (disabled) in Viewing mode", async ({
  page,
  consoleErrors,
}) => {
  await gotoFloat(page);
  await setReviewMode(page, "viewing");
  await rightClickImage(page);

  await expect(menu(page)).toBeVisible();
  // Every mutating object command is greyed out fail-closed in read-only Viewing.
  await expect(item(page, "object.altText")).toBeDisabled();
  await expect(item(page, "object.crop")).toBeDisabled();
  await expect(item(page, "object.delete")).toBeDisabled();

  await page.keyboard.press("Escape");

  // Nothing mutated and the object is intact.
  await rightClickImage(page);
  await expect(page.locator(".overlay .object-outline")).toHaveCount(1);
  await expect(page.locator("#undoBtn")).toBeDisabled();
  await page.keyboard.press("Escape");

  expect(consoleErrors).toEqual([]);
});
