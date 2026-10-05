// docs/85 Phase A / Slice 3 (P1G-OBJ-GEOMETRY): the selection handles from
// #269 become functional — dragging a handle resizes the object, committing ONE
// SetExtent op on release (one undo step), with a live preview during the drag
// and fail-closed gating in Suggesting/Viewing mode. Move + wrap are floating-
// object ops (deferred with anchored-float selection); resize is the inline op.
import { MOD, expect, gotoEditor, stableBox, test } from "./fixtures.mjs";

const IMAGE_POS = { fx: 0.32, fy: 0.1 };

async function selectImage(page) {
  const canvas = page.locator(".page-wrap .page").first();
  const box = await stableBox(canvas);
  await canvas.click({ position: { x: box.width * IMAGE_POS.fx, y: box.height * IMAGE_POS.fy } });
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
}

async function outlineSize(page) {
  return page.locator(".overlay .object-outline").first().evaluate((el) => {
    const r = el.getBoundingClientRect();
    return { w: Math.round(r.width), h: Math.round(r.height) };
  });
}

/** The selected object's outline in viewport coordinates — for the grips whose
 *  whole point is WHICH edge moved, not by how much. */
async function outlineBox(page) {
  return page.locator(".overlay .object-outline").first().evaluate((el) => {
    const r = el.getBoundingClientRect();
    return { x: r.x, y: r.y, width: r.width, height: r.height };
  });
}

async function dragHandle(page, handleIndex, dx, dy, { shift = false } = {}) {
  const handle = page.locator(`.overlay .object-handle[data-handle="${handleIndex}"]`).first();
  const b = await stableBox(handle);
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
  await expect(page.locator(".overlay .object-handle:not(.object-rotate-handle)")).toHaveCount(8);
  // The rotation grip rides alongside the eight, so the resize count is taken
  // on the resize grips rather than on every `.object-handle`.
  await expect(page.locator(".overlay .object-rotate-handle")).toHaveCount(1);

  // One undo reverts the resize to the original size.
  await page.keyboard.press(`${MOD}+z`);
  await page.waitForTimeout(150);
  const undone = await outlineSize(page);
  expect(Math.abs(undone.w - before.w)).toBeLessThanOrEqual(3);
  expect(Math.abs(undone.h - before.h)).toBeLessThanOrEqual(3);

  expect(consoleErrors).toEqual([]);
});

test("a picture keeps its aspect ratio on a corner drag, and Shift does not free it", async ({
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

  // Undo, then the SAME drag WITH Shift. This used to FREE the aspect, which was
  // our own invention: ONLYOFFICE's `ResizeTracks.js` ORs Shift into the
  // constraint (`ShiftKey === true || getNoChangeAspect()`), so holding it can
  // only add one, and Word and Docs agree. Distorting a picture is the
  // inspector's width/height fields, not a modifier that means the opposite on
  // the next object you select.
  await page.keyboard.press(`${MOD}+z`);
  await page.waitForTimeout(150);
  await dragHandle(page, 4, 120, 8, { shift: true });
  const held = await outlineSize(page);
  expect(held.h).toBeGreaterThan(before.h + 10);
  expect(Math.abs(held.w / held.h - ratio)).toBeLessThan(ratio * 0.15);

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

test("an inline object offers all eight grips, like every other editor", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await selectImage(page);
  const handles = page.locator(".overlay .object-handle");
  expect(await handles.evaluateAll((nodes) => nodes.map((node) => node.dataset.handle))).toEqual([
    "0",
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    // 8 is the ROTATION grip — ONLYOFFICE's own numbering, where `hitToHandles`
    // returns 0..7 for the resize markers and 8 for the rotation one. An inline
    // picture models `a:xfrm` and the layout paints it, so it is offered here.
    "8",
  ]);
  expect(consoleErrors).toEqual([]);
});

// The gesture the owner reported, end to end. A grip that is painted but throws
// "inline resize cannot move its flow anchor" on release is worse than no grip,
// so this asserts the DOCUMENT changed and in the right direction — not that
// eight elements exist.

test("dragging the WEST midpoint of an inline picture widens it and leaves its height alone", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await selectImage(page);
  const before = await outlineSize(page);
  const node = await page.locator("#pages").getAttribute("data-object-selected");

  await dragHandle(page, 7, -80, 0); // W midpoint, dragged outward to the left

  const after = await outlineSize(page);
  expect(after.w).toBeGreaterThan(before.w + 10);
  expect(Math.abs(after.h - before.h)).toBeLessThanOrEqual(3);
  // No refusal reached the user, and the selection survived the commit.
  await expect(page.locator("#status")).not.toContainText(/flow anchor|cannot/i);
  await expect(page.locator("#pages")).toHaveAttribute("data-object-selected", node);

  // And the DOCUMENT says so: one undo step restores the authored extent, which
  // only exists if a SetExtent was really committed.
  await page.keyboard.press(`${MOD}+z`);
  await page.waitForTimeout(150);
  const undone = await outlineSize(page);
  expect(Math.abs(undone.w - before.w)).toBeLessThanOrEqual(3);

  expect(consoleErrors).toEqual([]);
});

test("the north grip works too, and the preview pins the south edge while it drags", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await selectImage(page);
  const before = await outlineBox(page);

  const handle = page.locator('.overlay .object-handle[data-handle="1"]').first();
  const b = await stableBox(handle);
  await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2);
  await page.mouse.down();
  await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2 - 70, { steps: 6 });
  // Mid-drag: the preview grew UPWARDS, keeping its bottom edge where it was —
  // that is what Word and ONLYOFFICE draw, and it is the half we deliberately
  // do not commit.
  const preview = await stableBox(page.locator(".object-resize-preview"));
  expect(preview.y).toBeLessThan(before.y - 10);
  expect(Math.abs(preview.y + preview.height - (before.y + before.height))).toBeLessThanOrEqual(3);
  await page.mouse.up();
  await page.waitForTimeout(150);

  // On release the picture is TALLER, at the origin the paragraph gives it.
  const after = await outlineBox(page);
  expect(after.height).toBeGreaterThan(before.height + 10);
  expect(Math.abs(after.width - before.width)).toBeLessThanOrEqual(3);
  await expect(page.locator("#status")).not.toContainText(/flow anchor|cannot/i);

  expect(consoleErrors).toEqual([]);
});

test("Ctrl resizes about the centre instead of pinning the opposite edge", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await selectImage(page);
  const before = await outlineBox(page);

  const handle = page.locator('.overlay .object-handle[data-handle="3"]').first();
  const b = await stableBox(handle);
  await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2);
  await page.mouse.down();
  await page.keyboard.down("Control");
  await page.mouse.move(b.x + b.width / 2 + 60, b.y + b.height / 2, { steps: 6 });
  const preview = await stableBox(page.locator(".object-resize-preview"));
  await page.keyboard.up("Control");
  await page.mouse.up();
  await page.waitForTimeout(150);

  // Both edges moved out by the same amount, so the centre held. Without Ctrl
  // the west edge would not have moved at all.
  expect(preview.width).toBeGreaterThan(before.width + 100);
  expect(preview.x).toBeLessThan(before.x - 40);
  const centreShift = Math.abs(
    preview.x + preview.width / 2 - (before.x + before.width / 2),
  );
  expect(centreShift).toBeLessThanOrEqual(3);

  expect(consoleErrors).toEqual([]);
});

test("Shift on a SHAPE corner constrains it, the same direction it does everywhere", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  // A shape, not a picture: a picture is proportional by DEFAULT, so it cannot
  // tell whether Shift added the constraint or the default did.
  await page.locator("#tabInsert").click();
  await expect(page.locator("#panelInsert")).toBeVisible();
  await page.locator("#insertShapeBtn").click();
  await page.locator('#shapeGalleryMenu [data-shape-geometry="rect"]').click();
  const canvas = page.locator(".page-wrap .page").first();
  const box = await stableBox(canvas);
  await canvas.click({ position: { x: box.width * 0.35, y: box.height * 0.35 } });
  await expect(page.locator("#pages")).toHaveAttribute("data-object-kind", "shape");
  const before = await outlineSize(page);
  const ratio = before.w / before.h;

  // A mostly-horizontal SE drag WITH Shift: the height follows the ratio.
  await dragHandle(page, 4, 140, 6, { shift: true });
  const held = await outlineSize(page);
  expect(held.w).toBeGreaterThan(before.w + 20);
  expect(held.h).toBeGreaterThan(before.h + 10);
  expect(Math.abs(held.w / held.h - ratio)).toBeLessThan(ratio * 0.15);

  // The same drag WITHOUT Shift is free — which is what makes the assertion
  // above about the KEY and not about the fixture's proportions.
  await page.keyboard.press(`${MOD}+z`);
  await page.waitForTimeout(150);
  await dragHandle(page, 4, 140, 6);
  const free = await outlineSize(page);
  expect(free.w).toBeGreaterThan(before.w + 20);
  expect(Math.abs(free.h - before.h)).toBeLessThan(30);

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
  const box = await stableBox(se);
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
