// docs/85 Phase A / Slice 2 (P1G-OBJ-SELECT + the §4 grammar): clicking a
// drawing selects it as an OBJECT (distinct from a text caret), the engine draws
// its outline + eight resize/move handles, and the interaction grammar's Escape
// two-step returns to a text caret. Move/resize mutation is a later slice — the
// handles are visible but not yet draggable.
import { test, expect, gotoEditor, MOD } from "./fixtures.mjs";

// The rich producer fixture places an inline image near the top of page 1;
// this fraction of the first page's box lands on it (found empirically, stable
// across runs — the image is a fixed layout object).
const IMAGE_POS = { fx: 0.32, fy: 0.1 };

async function clickImage(page) {
  const canvas = page.locator(".page-wrap .page").first();
  const box = await canvas.boundingBox();
  await canvas.click({ position: { x: box.width * IMAGE_POS.fx, y: box.height * IMAGE_POS.fy } });
}

async function clickBodyText(page) {
  // A point well below the image, in ordinary body prose.
  const canvas = page.locator(".page-wrap .page").first();
  const box = await canvas.boundingBox();
  await canvas.click({ position: { x: box.width * 0.2, y: box.height * 0.55 } });
}

test("clicking an inline image selects it as an object and draws eight handles", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickImage(page);

  const pages = page.locator("#pages");
  // The object is selected as a unit (not a text caret).
  await expect(pages).toHaveAttribute("data-object-kind", "image");
  await expect(pages).toHaveAttribute("data-object-mode", "selected");
  const node = await pages.getAttribute("data-object-selected");
  expect(node).toBeTruthy();

  // Engine-drawn selection chrome: one outline + eight resize/move handles
  // (corners + edge midpoints), painted on the overlay.
  await expect(page.locator(".overlay .object-outline")).toHaveCount(1);
  await expect(page.locator(".overlay .object-handle")).toHaveCount(8);
  await expect(page.locator(".object-context-bar")).toBeVisible();
  await expect(page.locator(".object-context-bar")).toContainText("Drag handles to resize");
  await expect(page.locator(".object-context-bar")).not.toContainText(/coming soon|later editing slice/i);

  expect(consoleErrors).toEqual([]);
});

test("Escape collapses a selected object to a text caret (two-step exit)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickImage(page);
  const pages = page.locator("#pages");
  await expect(pages).toHaveAttribute("data-object-mode", "selected");

  // A leaf image has no edit mode: Escape from "selected" goes straight to a
  // text caret (the handles disappear, no object is selected).
  await page.keyboard.press("Escape");
  await expect(pages).not.toHaveAttribute("data-object-selected", /.*/);
  await expect(page.locator(".overlay .object-handle")).toHaveCount(0);
  await expect(page.locator(".object-context-bar")).toBeHidden();
  // A text caret is now present.
  await expect(page.locator(".overlay .caret")).toHaveCount(1);

  expect(consoleErrors).toEqual([]);
});

test("clicking body text deselects the object", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickImage(page);
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");

  await clickBodyText(page);
  await expect(page.locator("#pages")).not.toHaveAttribute("data-object-selected", /.*/);
  await expect(page.locator(".overlay .object-handle")).toHaveCount(0);
  // The click placed a text caret instead.
  await expect(page.locator(".overlay .caret")).toHaveCount(1);

  expect(consoleErrors).toEqual([]);
});

test("a selected object swallows text keys, and Delete removes the object (undoable)", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickImage(page);
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");

  // Typing does not edit through a selected object (a stale caret is never
  // mutated); the object stays selected and the typed text is not findable.
  await page.keyboard.type("XYZ");
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
  await page.keyboard.press(`${MOD}+f`);
  await page.locator("#findInput").fill("XYZ");
  await expect(page.locator("#findStatus")).toHaveText("No match");
  await page.keyboard.press("Escape");

  // Delete now removes the still-selected object as one undoable action: the
  // object selection clears and there is something to undo.
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
  await page.keyboard.press("Delete");
  await expect(page.locator("#pages")).not.toHaveAttribute("data-object-mode", /.*/);
  await expect(page.locator("#undoBtn")).toBeEnabled();

  // One Undo restores it; the image is selectable again.
  await page.locator("#undoBtn").click();
  await clickImage(page);
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");

  expect(consoleErrors).toEqual([]);
});

test("image-options feedback is transient and clears when object selection exits", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickImage(page);
  await page.keyboard.press("Enter");
  await expect(page.locator("#status")).toContainText("drag its handles to resize");

  await page.keyboard.press("Escape");
  await expect(page.locator("#status")).toHaveText("");
  await expect(page.locator("#pages")).not.toHaveAttribute("data-object-selected", /.*/);
  expect(consoleErrors).toEqual([]);
});
