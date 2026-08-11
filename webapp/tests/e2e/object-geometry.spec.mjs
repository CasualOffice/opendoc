// docs/85 Phase A / Slice 3 (P1G-OBJ-GEOMETRY): the selection handles from
// #269 become functional — dragging a handle resizes the object, committing ONE
// SetExtent op on release (one undo step), with a live preview during the drag
// and fail-closed gating in Suggesting/Viewing mode. Move + wrap are floating-
// object ops (deferred with anchored-float selection); resize is the inline op.
import { test, expect, gotoEditor, MOD } from "./fixtures.mjs";

const IMAGE_POS = { fx: 0.32, fy: 0.1 };

async function selectImage(page) {
  const canvas = page.locator(".page-wrap .page").first();
  const box = await canvas.boundingBox();
  await canvas.click({ position: { x: box.width * IMAGE_POS.fx, y: box.height * IMAGE_POS.fy } });
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
}

async function outlineSize(page) {
  return page.locator(".overlay .object-outline").first().evaluate((el) => {
    const r = el.getBoundingClientRect();
    return { w: Math.round(r.width), h: Math.round(r.height) };
  });
}

async function dragHandle(page, handleIndex, dx, dy, { shift = false } = {}) {
  const handle = page.locator(`.overlay .object-handle[data-handle="${handleIndex}"]`).first();
  const b = await handle.boundingBox();
  await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2);
  await page.mouse.down();
  if (shift) await page.keyboard.down("Shift");
  await page.mouse.move(b.x + b.width / 2 + dx, b.y + b.height / 2 + dy, { steps: 6 });
  await page.mouse.up();
  if (shift) await page.keyboard.up("Shift");
  await page.waitForTimeout(150);
}

test("dragging a corner handle resizes the object, and one undo reverts it", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await selectImage(page);
  const before = await outlineSize(page);

  // Drag the SE (bottom-right, index 4) handle outward.
  await dragHandle(page, 4, 60, 60);

  // The object grew and is still selected with its handles.
  const after = await outlineSize(page);
  expect(after.w).toBeGreaterThan(before.w + 10);
  expect(after.h).toBeGreaterThan(before.h + 10);
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
  await expect(page.locator(".overlay .object-handle")).toHaveCount(3);

  // One undo reverts the resize to the original size.
  await page.keyboard.press(`${MOD}+z`);
  await page.waitForTimeout(150);
  const undone = await outlineSize(page);
  expect(Math.abs(undone.w - before.w)).toBeLessThanOrEqual(3);
  expect(Math.abs(undone.h - before.h)).toBeLessThanOrEqual(3);

  expect(consoleErrors).toEqual([]);
});

test("a picture keeps its aspect ratio on a corner drag by default; Shift frees it", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await selectImage(page);
  const before = await outlineSize(page);
  const ratio = before.w / before.h;

  // Non-proportional corner drag (mostly horizontal) WITHOUT Shift: because a
  // picture aspect-locks by default (Word/Docs), height grows in proportion — so
  // the ratio is preserved even though the pointer moved far more in x.
  await dragHandle(page, 4, 120, 8);
  const locked = await outlineSize(page);
  expect(locked.w).toBeGreaterThan(before.w + 10);
  expect(locked.h).toBeGreaterThan(before.h + 10);
  expect(Math.abs(locked.w / locked.h - ratio)).toBeLessThan(ratio * 0.15);

  // Undo, then the SAME drag WITH Shift frees the aspect: width grows, height
  // barely moves.
  await page.keyboard.press(`${MOD}+z`);
  await page.waitForTimeout(150);
  await dragHandle(page, 4, 120, 8, { shift: true });
  const free = await outlineSize(page);
  expect(free.w).toBeGreaterThan(before.w + 40);
  expect(Math.abs(free.h - before.h)).toBeLessThan(30);

  expect(consoleErrors).toEqual([]);
});

test("an edge handle resizes only its axis", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await selectImage(page);
  const before = await outlineSize(page);

  // Drag the E (right edge, index 3) handle: width grows, height holds.
  await dragHandle(page, 3, 60, 0);
  const after = await outlineSize(page);
  expect(after.w).toBeGreaterThan(before.w + 10);
  expect(Math.abs(after.h - before.h)).toBeLessThanOrEqual(3);

  expect(consoleErrors).toEqual([]);
});

test("an inline object omits handles that would have to move its flow anchor", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await selectImage(page);
  const handles = page.locator(".overlay .object-handle");
  expect(await handles.evaluateAll((nodes) => nodes.map((node) => node.dataset.handle))).toEqual([
    "3",
    "4",
    "5",
  ]);
  await expect(page.locator('.overlay .object-handle[data-handle="7"]')).toHaveCount(0);
  await expect(page.locator('.overlay .object-handle[data-handle="1"]')).toHaveCount(0);
  expect(consoleErrors).toEqual([]);
});

test("pointer cancellation discards the resize preview without creating history", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await selectImage(page);
  const before = await outlineSize(page);
  const undoLabel = await page.locator("#undoBtn").getAttribute("aria-label");
  const se = page.locator('.overlay .object-handle[data-handle="4"]').first();
  const box = await se.boundingBox();
  const cx = box.x + box.width / 2;
  const cy = box.y + box.height / 2;
  await page.mouse.move(cx, cy);
  await page.mouse.down();
  await page.mouse.move(cx + 80, cy + 50, { steps: 4 });
  await page.evaluate(() => window.dispatchEvent(new PointerEvent("pointercancel")));
  await page.mouse.up();
  await page.waitForTimeout(100);

  const after = await outlineSize(page);
  expect(Math.abs(after.w - before.w)).toBeLessThanOrEqual(3);
  expect(Math.abs(after.h - before.h)).toBeLessThanOrEqual(3);
  await expect(page.locator(".object-resize-preview")).toHaveCount(0);
  await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", undoLabel);
  expect(consoleErrors).toEqual([]);
});

test("object resize is blocked (fail-closed) in Suggesting mode", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  // Enter Suggesting mode, then select the image and try to resize it.
  await page.locator('#reviewModeControl [data-review-mode="suggesting"]').click();
  await selectImage(page);
  const before = await outlineSize(page);

  await dragHandle(page, 4, 60, 60);

  // No mutation: the size is unchanged and the block is reported.
  const after = await outlineSize(page);
  expect(Math.abs(after.w - before.w)).toBeLessThanOrEqual(3);
  expect(Math.abs(after.h - before.h)).toBeLessThanOrEqual(3);
  await expect(page.locator("#status")).toContainText("switch to Editing");

  expect(consoleErrors).toEqual([]);
});
