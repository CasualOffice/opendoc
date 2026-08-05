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

test("the crop dialog applies a crop and Remove crop clears it", async ({
  page,
  consoleErrors,
}) => {
  await gotoFloat(page);
  await selectFloat(page);

  // Apply a crop via the numeric dialog.
  await cropBtn(page).click();
  await expect(page.locator("#cropDialog")).toBeVisible();
  await page.locator("#cropLeft").fill("10");
  await page.locator("#cropTop").fill("12");
  await page.locator("#cropRight").fill("8");
  await page.locator("#cropBottom").fill("6");
  await page.locator("#cropApply").click();

  await expect(page.locator("#cropDialog")).toBeHidden();
  await expect(page.locator("#documentState")).toHaveAttribute("data-state", "edited");
  await expect(page.locator("#undoBtn")).toBeEnabled();

  // Reopen and Remove crop — a second undoable op that clears it.
  await selectFloat(page);
  await cropBtn(page).click();
  await expect(page.locator("#cropDialog")).toBeVisible();
  await page.locator("#cropRemove").click();
  await expect(page.locator("#cropDialog")).toBeHidden();
  await expect(page.locator("#undoBtn")).toBeEnabled();

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

  // Crop: blocked.
  await cropBtn(page).click();
  await page.locator("#cropLeft").fill("10");
  await page.locator("#cropApply").click();
  await expect(page.locator("#cropDialog")).toBeHidden();
  await expect(page.locator("#status")).toContainText("read-only");

  // Nothing mutated: still nothing to undo.
  await expect(page.locator("#undoBtn")).toBeDisabled();

  expect(consoleErrors).toEqual([]);
});
