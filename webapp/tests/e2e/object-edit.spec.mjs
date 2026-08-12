// docs/85 Phase A — object editing UI: the already-shipped image crop / alt-text
// / delete WASM ops are wired onto the object-selection chrome. Each is one
// undoable action, applied through the same gated path as the other object edits
// (resize/move/wrap): read-only in Viewing, fail-closed (untracked) in
// Suggesting. The `?fixture=float` document holds one top-level floating image
// (the shipped sample docs contain none); it is selected exactly as in
// object-anchor.spec.mjs.
import { test, expect } from "./fixtures.mjs";

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

async function selectFloat(page) {
  const canvas = page.locator(".page-wrap .page").first();
  const box = await canvas.boundingBox();
  await canvas.click({ position: { x: box.width * FLOAT_POS.fx, y: box.height * FLOAT_POS.fy } });
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
}

const altTextBtn = (page) => page.locator('.object-bar-btn[aria-label="Edit alt text"]');
const cropBtn = (page) => page.locator('.object-bar-btn[aria-label="Crop image"]');
const deleteBtn = (page) => page.locator('.object-bar-btn[aria-label="Delete object"]');

test("setting alt text applies as one undoable action and Undo reverts it", async ({
  page,
  consoleErrors,
}) => {
  await gotoFloat(page);
  await selectFloat(page);

  // The image bar offers alt-text + crop + delete (crop is picture-only).
  await expect(altTextBtn(page)).toBeVisible();
  await expect(cropBtn(page)).toBeVisible();
  await expect(deleteBtn(page)).toBeVisible();
  // A fresh document has nothing to undo yet.
  await expect(page.locator("#undoBtn")).toBeDisabled();

  await altTextBtn(page).click();
  await expect(page.locator("#altTextDialog")).toBeVisible();
  await page.locator("#altTextInput").fill("Quarterly revenue chart");
  await page.locator("#altTextInput").press("Enter");

  // Applied: the dialog closed, the document is dirty, and there is now one
  // undoable action.
  await expect(page.locator("#altTextDialog")).toBeHidden();
  await expect(page.locator("#documentState")).toHaveAttribute("data-state", "edited");
  await expect(page.locator("#undoBtn")).toBeEnabled();

  // One Undo reverts it (the op becomes redoable).
  await page.locator("#undoBtn").click();
  await expect(page.locator("#redoBtn")).toBeEnabled();

  expect(consoleErrors).toEqual([]);
});

test("selected objects expose a nonmodal properties inspector with exact size", async ({ page, consoleErrors }) => {
  await gotoFloat(page);
  await selectFloat(page);
  await page.locator('.object-bar-btn[aria-label="Open object properties"]').click();
  const panel = page.locator(".object-inspector");
  await expect(panel).toBeVisible();
  await expect(panel.locator("[data-object-prop=width]")).toHaveValue(/\d/);
  await expect(panel.locator("[data-object-prop=height]")).toHaveValue(/\d/);
  await expect(panel.locator("[data-object-inspector-wrap-select]")).toBeVisible();
  await expect(panel.locator("[data-object-inspector-alt-input]")).toBeVisible();
  await expect(page.locator("canvas.page").first()).toBeVisible();
  expect(consoleErrors).toEqual([]);
});

test("the alt-text dialog prefills the existing description instead of blank", async ({
  page,
  consoleErrors,
}) => {
  await gotoFloat(page);
  await selectFloat(page);

  // Set an initial description.
  await altTextBtn(page).click();
  await page.locator("#altTextInput").fill("First description");
  await page.locator("#altTextInput").press("Enter");
  await expect(page.locator("#altTextDialog")).toBeHidden();

  // Reopening the dialog prefills the current alt text (not blank) so it can be
  // refined rather than blind-overwritten.
  await selectFloat(page);
  await altTextBtn(page).click();
  await expect(page.locator("#altTextInput")).toHaveValue("First description");

  expect(consoleErrors).toEqual([]);
});

test("delete removes the object and Undo restores it", async ({ page, consoleErrors }) => {
  await gotoFloat(page);
  await selectFloat(page);
  await expect(page.locator(".overlay .object-outline")).toHaveCount(1);

  // Delete (keyboard) while the object — not text — is selected.
  await page.keyboard.press("Delete");

  // The object is gone: its selection cleared and no outline is painted.
  await expect(page.locator("#pages")).not.toHaveAttribute("data-object-mode", /.*/);
  await expect(page.locator(".overlay .object-outline")).toHaveCount(0);
  await expect(page.locator("#undoBtn")).toBeEnabled();

  // One Undo restores the object; it is selectable again at its old position.
  await page.locator("#undoBtn").click();
  await selectFloat(page);
  await expect(page.locator(".overlay .object-outline")).toHaveCount(1);

  expect(consoleErrors).toEqual([]);
});

test("dragging a crop handle crops the image as one undoable action (Word/Docs style)", async ({
  page,
  consoleErrors,
}) => {
  await gotoFloat(page);
  await selectFloat(page);

  // Crop is direct manipulation: the button enters a crop MODE with black crop
  // handles + a dimmed cut region — not a numeric dialog.
  await cropBtn(page).click();
  await expect(page.locator(".object-crop-handle")).toHaveCount(8);
  // The button reads Apply (active) while a crop session is live.
  await expect(page.locator('.object-bar-btn[aria-label="Apply crop (Enter)"]')).toBeVisible();
  // Nothing is dimmed yet (crop starts at 0); dragging a handle inward reveals it.
  await expect(page.locator(".object-crop-dim")).toHaveCount(0);

  // Drag the SE handle (index 4) up-and-left to crop off the right + bottom.
  const se = page.locator('.object-crop-handle[data-handle="4"]');
  const b = await se.boundingBox();
  await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2);
  await page.mouse.down();
  await page.mouse.move(b.x - 40, b.y - 30, { steps: 8 });
  await page.mouse.up();
  // The removed margins are now dimmed.
  await expect(page.locator(".object-crop-dim").first()).toBeVisible();

  // Enter commits one SetImageCrop op: crop mode exits, the document is dirty,
  // and there is exactly one undoable action.
  await page.keyboard.press("Enter");
  await expect(page.locator(".object-crop-handle")).toHaveCount(0);
  await expect(page.locator("#documentState")).toHaveAttribute("data-state", "edited");
  await expect(page.locator("#undoBtn")).toBeEnabled();
  await expect(page.locator("#status")).toContainText("cropped");

  // Re-entering crop must preserve the authored source crop instead of
  // silently resetting the session to the full image.
  await cropBtn(page).click();
  await expect(page.locator(".object-crop-dim").first()).toBeVisible();
  await page.keyboard.press("Escape");

  // One Undo reverts the crop (it becomes redoable).
  await page.locator("#undoBtn").click();
  await expect(page.locator("#redoBtn")).toBeEnabled();

  expect(consoleErrors).toEqual([]);
});

test("Escape cancels a crop with no change", async ({ page, consoleErrors }) => {
  await gotoFloat(page);
  await selectFloat(page);
  await cropBtn(page).click();
  await expect(page.locator(".object-crop-handle")).toHaveCount(8);

  // Drag then Escape — the crop is discarded: mode exits and nothing is undoable.
  const se = page.locator('.object-crop-handle[data-handle="4"]');
  const b = await se.boundingBox();
  await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2);
  await page.mouse.down();
  await page.mouse.move(b.x - 30, b.y - 20, { steps: 6 });
  await page.mouse.up();
  await page.keyboard.press("Escape");

  await expect(page.locator(".object-crop-handle")).toHaveCount(0);
  await expect(page.locator("#undoBtn")).toBeDisabled();

  expect(consoleErrors).toEqual([]);
});

test("alt text, delete, and crop are all blocked (fail-closed) in Viewing mode", async ({
  page,
  consoleErrors,
}) => {
  await gotoFloat(page);
  // Enter Viewing (read-only), then select the image and try each edit.
  await page.locator('#reviewModeControl [data-review-mode="viewing"]').click();
  await selectFloat(page);
  await expect(page.locator("#undoBtn")).toBeDisabled();

  // Alt text: submitting is blocked and the read-only reason is surfaced.
  await altTextBtn(page).click();
  await page.locator("#altTextInput").fill("Should not apply");
  await page.locator("#altTextInput").press("Enter");
  await expect(page.locator("#altTextDialog")).toBeHidden();
  await expect(page.locator("#status")).toContainText("read-only");

  // Delete: blocked; the object remains.
  await selectFloat(page);
  await page.keyboard.press("Delete");
  await expect(page.locator(".overlay .object-outline")).toHaveCount(1);
  await expect(page.locator("#status")).toContainText("read-only");

  // Crop: blocked — crop mode never opens (no handles) and the read-only reason
  // is surfaced.
  await cropBtn(page).click();
  await expect(page.locator(".object-crop-handle")).toHaveCount(0);
  await expect(page.locator("#status")).toContainText("read-only");

  // Nothing mutated: still nothing to undo.
  await expect(page.locator("#undoBtn")).toBeDisabled();

  expect(consoleErrors).toEqual([]);
});
