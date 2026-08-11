// docs/85 Phase A / Slice 3b (P1G-OBJ-ANCHOR-SELECT + move/wrap): floating
// (anchored) objects are now selectable, movable (SetAnchor), wrappable
// (SetWrap), and resizable (SetExtent). The `?fixture=float` document holds one
// top-level floating image; the shipped sample docs contain none.
import { test, expect, gotoEditor, MOD } from "./fixtures.mjs";

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

async function outlineBox(page) {
  return page.locator(".overlay .object-outline").first().evaluate((el) => {
    const r = el.getBoundingClientRect();
    return { x: Math.round(r.left), y: Math.round(r.top), w: Math.round(r.width), h: Math.round(r.height) };
  });
}

test("a floating image is selectable and exposes move/resize/wrap controls", async ({
  page,
  consoleErrors,
}) => {
  await gotoFloat(page);
  await selectFloat(page);

  await expect(page.locator("#pages")).toHaveAttribute("data-object-kind", "image");
  await expect(page.locator(".overlay .object-outline")).toHaveCount(1);
  await expect(page.locator(".overlay .object-handle")).toHaveCount(8);
  // The context bar offers the live Wrap control (only floats get it).
  await expect(page.locator(".object-wrap-menu")).toBeVisible();
  await expect(page.locator(".object-wrap-btn")).toHaveCount(6);

  expect(consoleErrors).toEqual([]);
});

test("dragging a floating object's body moves it (SetAnchor), and one undo reverts", async ({
  page,
  consoleErrors,
}) => {
  await gotoFloat(page);
  await selectFloat(page);
  const before = await outlineBox(page);

  // Drag the object body (its center) — not a handle — to move it.
  const cx = before.x + before.w / 2;
  const cy = before.y + before.h / 2;
  await page.mouse.move(cx, cy);
  await page.mouse.down();
  await page.mouse.move(cx + 90, cy + 70, { steps: 6 });
  await page.mouse.up();
  await page.waitForTimeout(150);

  const moved = await outlineBox(page);
  expect(moved.x).toBeGreaterThan(before.x + 30);
  expect(moved.y).toBeGreaterThan(before.y + 20);
  // Size unchanged by a move.
  expect(Math.abs(moved.w - before.w)).toBeLessThanOrEqual(3);
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");

  // One undo returns it to its original position.
  await page.keyboard.press(`${MOD}+z`);
  await page.waitForTimeout(150);
  const undone = await outlineBox(page);
  expect(Math.abs(undone.x - before.x)).toBeLessThanOrEqual(3);
  expect(Math.abs(undone.y - before.y)).toBeLessThanOrEqual(3);

  expect(consoleErrors).toEqual([]);
});

test("changing wrap mode re-lays-out and reflects the active mode", async ({
  page,
  consoleErrors,
}) => {
  await gotoFloat(page);
  await selectFloat(page);

  const square = page.locator('.object-wrap-btn[data-wrap="square"]');
  const behind = page.locator('.object-wrap-btn[data-wrap="behind"]');
  await expect(square).toHaveAttribute("aria-pressed", "true");
  await expect(behind).toHaveAttribute("aria-pressed", "false");

  await behind.click();
  await page.waitForTimeout(150);
  // The object stays selected and the active wrap flipped to "behind".
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
  await expect(page.locator('.object-wrap-btn[data-wrap="behind"]')).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(page.locator('.object-wrap-btn[data-wrap="square"]')).toHaveAttribute(
    "aria-pressed",
    "false",
  );

  expect(consoleErrors).toEqual([]);
});

test("all floating handles preserve their opposite edge and undo position plus size", async ({
  page,
  consoleErrors,
}) => {
  await gotoFloat(page);
  await selectFloat(page);
  const gestures = [
    { handle: 0, fx: -1, fy: -1, dx: -48, dy: -32 },
    { handle: 1, fx: 0, fy: -1, dx: 0, dy: -32 },
    { handle: 2, fx: 1, fy: -1, dx: 48, dy: -32 },
    { handle: 3, fx: 1, fy: 0, dx: 48, dy: 0 },
    { handle: 4, fx: 1, fy: 1, dx: 48, dy: 32 },
    { handle: 5, fx: 0, fy: 1, dx: 0, dy: 32 },
    { handle: 6, fx: -1, fy: 1, dx: -48, dy: 32 },
    { handle: 7, fx: -1, fy: 0, dx: -48, dy: 0 },
  ];

  for (const gesture of gestures) {
    const before = await outlineBox(page);
    const handle = page
      .locator(`.overlay .object-handle[data-handle="${gesture.handle}"]`)
      .first();
    const b = await handle.boundingBox();
    const cx = b.x + b.width / 2;
    const cy = b.y + b.height / 2;
    await page.mouse.move(cx, cy);
    await page.mouse.down();
    await page.mouse.move(cx + gesture.dx, cy + gesture.dy, { steps: 4 });
    await page.mouse.up();
    await page.waitForTimeout(80);

    const after = await outlineBox(page);
    if (gesture.fx < 0) {
      expect(Math.abs(after.x + after.w - (before.x + before.w))).toBeLessThanOrEqual(3);
    } else {
      expect(Math.abs(after.x - before.x)).toBeLessThanOrEqual(3);
    }
    if (gesture.fx === 0) expect(Math.abs(after.w - before.w)).toBeLessThanOrEqual(3);
    if (gesture.fy < 0) {
      expect(Math.abs(after.y + after.h - (before.y + before.h))).toBeLessThanOrEqual(3);
    } else {
      expect(Math.abs(after.y - before.y)).toBeLessThanOrEqual(3);
    }
    if (gesture.fy === 0) expect(Math.abs(after.h - before.h)).toBeLessThanOrEqual(3);

    await page.keyboard.press(`${MOD}+z`);
    await page.waitForTimeout(80);
    const undone = await outlineBox(page);
    expect(Math.abs(undone.x - before.x)).toBeLessThanOrEqual(3);
    expect(Math.abs(undone.y - before.y)).toBeLessThanOrEqual(3);
    expect(Math.abs(undone.w - before.w)).toBeLessThanOrEqual(3);
    expect(Math.abs(undone.h - before.h)).toBeLessThanOrEqual(3);
  }

  expect(consoleErrors).toEqual([]);
});

test("a handle crossing its opposite edge clamps to the deterministic minimum", async ({
  page,
  consoleErrors,
}) => {
  await gotoFloat(page);
  await selectFloat(page);
  const before = await outlineBox(page);
  const west = page.locator('.overlay .object-handle[data-handle="7"]').first();
  const b = await west.boundingBox();
  const cx = b.x + b.width / 2;
  const cy = b.y + b.height / 2;
  await page.mouse.move(cx, cy);
  await page.mouse.down();
  await page.mouse.move(cx + before.w * 2, cy, { steps: 6 });
  await page.mouse.up();
  await page.waitForTimeout(100);

  const clamped = await outlineBox(page);
  expect(clamped.w).toBeGreaterThanOrEqual(7);
  expect(clamped.w).toBeLessThan(20);
  expect(Math.abs(clamped.x + clamped.w - (before.x + before.w))).toBeLessThanOrEqual(3);

  await page.keyboard.press(`${MOD}+z`);
  await page.waitForTimeout(100);
  const undone = await outlineBox(page);
  expect(Math.abs(undone.x - before.x)).toBeLessThanOrEqual(3);
  expect(Math.abs(undone.w - before.w)).toBeLessThanOrEqual(3);
  expect(consoleErrors).toEqual([]);
});

test("arrow keys nudge a floating object and Shift takes a larger step; one undo reverts", async ({
  page,
  consoleErrors,
}) => {
  await gotoFloat(page);
  await selectFloat(page);
  const before = await outlineBox(page);

  // A plain ArrowRight nudges the object right by a small step.
  await page.keyboard.press("ArrowRight");
  await page.waitForTimeout(120);
  const nudged = await outlineBox(page);
  expect(nudged.x).toBeGreaterThan(before.x);
  const smallStep = nudged.x - before.x;

  // Shift+ArrowRight takes a visibly larger step than the plain nudge.
  await page.keyboard.press("Shift+ArrowRight");
  await page.waitForTimeout(120);
  const big = await outlineBox(page);
  expect(big.x - nudged.x).toBeGreaterThan(smallStep);

  // Each nudge is one undoable action: two undos return to the start.
  await page.keyboard.press(`${MOD}+z`);
  await page.keyboard.press(`${MOD}+z`);
  await page.waitForTimeout(150);
  const undone = await outlineBox(page);
  expect(Math.abs(undone.x - before.x)).toBeLessThanOrEqual(3);

  expect(consoleErrors).toEqual([]);
});
