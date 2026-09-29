// Free rotation of an object — the last piece of ONLYOFFICE/Word parity for
// pictures and shapes, and the half that only a browser can prove.
//
// What is asserted here is the GESTURE and the DOCUMENT together, never a node
// count on its own: dragging the grip round turns the object AND the engine
// reports the angle; a rotated object still shows its eight grips and can still
// be resized; Shift lands on 15-degree steps. The pure arithmetic — the snap
// rule, the object-space mapping — is `tests/object_rotate.test.mjs`.
import { test, expect, gotoEditor, MOD } from "./fixtures.mjs";

const IMAGE_POS = { fx: 0.32, fy: 0.1 };

async function selectImage(page) {
  const canvas = page.locator(".page-wrap .page").first();
  const box = await canvas.boundingBox();
  await canvas.click({ position: { x: box.width * IMAGE_POS.fx, y: box.height * IMAGE_POS.fy } });
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
}

/** Opens the object properties panel, whose Rotation field is filled by an
 *  independent `objectFrame` read on every repaint. */
async function openInspector(page) {
  await page.locator('.object-bar-btn[aria-label="Open object properties"]').click();
  await expect(page.locator(".object-inspector")).toBeVisible();
}

/** What the MODEL says the object's angle is, in degrees.
 *
 *  Read out of the properties panel rather than off the chrome: the panel asks
 *  the engine for the object's frame on every repaint, so a rotation that never
 *  reached the document reads 0 here however the outline is drawn. The editor
 *  exposes no engine handle to a test, and this is the closest independent path
 *  to the stored value that a browser has. */
async function modelAngle(page) {
  const value = await page
    .locator(".object-inspector [data-object-prop=rotation]")
    .inputValue();
  return Number(value);
}

/** The object outline's own transform, i.e. what the user can SEE. */
async function paintedAngle(page) {
  return page.locator(".overlay .object-outline").first().evaluate((el) => {
    const matrix = new DOMMatrixReadOnly(getComputedStyle(el).transform);
    return Math.round(((Math.atan2(matrix.b, matrix.a) * 180) / Math.PI + 360) % 360);
  });
}

async function dragRotationGrip(page, degrees, { shift = false } = {}) {
  const grip = page.locator(".overlay .object-rotate-handle").first();
  const outline = page.locator(".overlay .object-outline").first();
  const g = await grip.boundingBox();
  const o = await outline.boundingBox();
  const centre = { x: o.x + o.width / 2, y: o.y + o.height / 2 };
  const radius = Math.hypot(g.x + g.width / 2 - centre.x, g.y + g.height / 2 - centre.y);
  const target = ((degrees - 90) * Math.PI) / 180;
  await page.mouse.move(g.x + g.width / 2, g.y + g.height / 2);
  await page.mouse.down();
  if (shift) await page.keyboard.down("Shift");
  await page.mouse.move(
    centre.x + Math.cos(target) * radius,
    centre.y + Math.sin(target) * radius,
    { steps: 8 },
  );
  await page.mouse.up();
  if (shift) await page.keyboard.up("Shift");
  await page.waitForTimeout(150);
}

test("dragging the rotation grip turns the object, and the document says so", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await selectImage(page);
  await openInspector(page);
  expect(await modelAngle(page)).toBe(0);

  await dragRotationGrip(page, 30);

  // The DOCUMENT, not the chrome: the engine's own frame reports the angle.
  const committed = await modelAngle(page);
  expect(Math.abs(committed - 30)).toBeLessThanOrEqual(3);
  // …and the chrome agrees with it, which is the other half of the guarantee —
  // an engine that turned while the outline stayed upright is a defect too.
  expect(Math.abs((await paintedAngle(page)) - committed)).toBeLessThanOrEqual(2);

  // One undo straightens it: a rotation is ONE action.
  await page.keyboard.press(`${MOD}+z`);
  await page.waitForTimeout(200);
  expect(await modelAngle(page)).toBe(0);

  expect(consoleErrors).toEqual([]);
});

test("Shift snaps the rotation to 15 degrees, as it does in ONLYOFFICE", async ({ page }) => {
  await gotoEditor(page);
  await selectImage(page);
  await openInspector(page);

  // 38 degrees is deliberately not a multiple of 15 and is outside the 4-degree
  // cardinal tolerance, so only the Shift step can explain the answer.
  await dragRotationGrip(page, 38, { shift: true });
  const snapped = await modelAngle(page);
  expect(snapped % 15).toBe(0);
  expect(snapped).toBe(45);
});

test("a rotated object still shows its eight grips and still resizes", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await selectImage(page);
  await openInspector(page);
  await dragRotationGrip(page, 30);
  expect(await modelAngle(page)).toBeGreaterThan(0);

  // THE regression this whole feature turns on: a rotated object used to lose
  // every grip and the size chrome until undo.
  await expect(page.locator(".overlay .object-handle:not(.object-rotate-handle)")).toHaveCount(8);
  await expect(page.locator("#pages")).toHaveAttribute(
    "data-object-capabilities",
    /canResize/,
  );

  // And they are not decoration — a drag on one changes the object's size in
  // the document, while leaving the angle it was given alone. The size comes
  // from the properties panel, which re-reads the engine on every repaint.
  const sizeOf = async () => {
    const panel = page.locator(".object-inspector");
    return (
      Number(await panel.locator("[data-object-prop=width]").inputValue()) +
      Number(await panel.locator("[data-object-prop=height]").inputValue())
    );
  };
  const before = await sizeOf();
  const grip = page.locator('.overlay .object-handle[data-handle="4"]').first();
  const b = await grip.boundingBox();
  await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2);
  await page.mouse.down();
  await page.mouse.move(b.x + b.width / 2 + 70, b.y + b.height / 2 + 70, { steps: 6 });
  await page.mouse.up();
  await page.waitForTimeout(200);

  const after = await sizeOf();
  expect(after).toBeGreaterThan(before + 0.2);
  expect(await modelAngle(page)).toBeGreaterThan(0);

  expect(consoleErrors).toEqual([]);
});

test("the rotation grip is a keyboard control, and it agrees with the menu", async ({ page }) => {
  await gotoEditor(page);
  await selectImage(page);
  await openInspector(page);
  const grip = page.locator(".overlay .object-rotate-handle").first();
  await expect(grip).toHaveAttribute("role", "slider");
  await expect(grip).toHaveAttribute("aria-valuenow", "0");

  await grip.focus();
  // Shift steps by the same 15 the drag snaps to, so the two halves of the one
  // capability cannot drift apart.
  await page.keyboard.press("Shift+ArrowRight");
  await page.waitForTimeout(200);
  expect(await modelAngle(page)).toBe(15);

  await page.locator(".overlay .object-rotate-handle").first().focus();
  await page.keyboard.press("Home");
  await page.waitForTimeout(200);
  expect(await modelAngle(page)).toBe(0);
});

test("the object bar clears the rotation grip instead of sitting on it", async ({ page }) => {
  await gotoEditor(page);
  await selectImage(page);
  const grip = await page.locator(".overlay .object-rotate-handle").first().boundingBox();
  // Whatever answers a press at the grip's centre must be the grip. The bar has
  // been placed over a grip's target before (`docs/104` HF-058 neighbourhood),
  // and the symptom is a control that simply cannot be grabbed.
  const owner = await page.evaluate(
    ({ x, y }) => {
      const el = document.elementFromPoint(x, y);
      return el?.className ?? "";
    },
    { x: grip.x + grip.width / 2, y: grip.y + grip.height / 2 },
  );
  expect(owner).toContain("object-rotate-handle");
});
