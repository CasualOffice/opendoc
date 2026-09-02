// Pointer-size floors (HF-060, HF-098).
//
// The whole suite runs one Desktop Chrome project with a fine pointer, so
// nothing here had ever been exercised with a finger. These specs are the
// ratchet: the first block runs the shell under real touch emulation, where
// `(pointer: coarse)` matches, and the second measures a hit area that must
// hold for every pointer.
import { test, expect, stableBox, gotoEditor, MOD } from "./fixtures.mjs";

// The iOS Safari floor: anything under 16px zooms the page on focus.
const IOS_NO_ZOOM_PX = 16;
// The HIG / Material floor for a finger target.
const TOUCH_TARGET_PX = 44;

const fontSize = (locator) =>
  locator.evaluate((el) => Number.parseFloat(getComputedStyle(el).fontSize));

test.describe("with a coarse pointer", () => {
  // A phone-sized touch device. `hasTouch` is what makes Chromium report a
  // coarse primary pointer, which is the condition the stylesheet keys on —
  // width alone would not select the block, and that is the point: the same
  // rules have to reach a touchscreen laptop at 1440px.
  test.use({ hasTouch: true, viewport: { width: 390, height: 844 } });

  test("the media query the shell keys on actually matches under touch emulation", async ({
    page,
  }) => {
    await page.goto("/editor.html");
    expect(await page.evaluate(() => matchMedia("(pointer: coarse)").matches)).toBe(true);
  });

  test("no text field is small enough to make iOS Safari zoom on focus", async ({
    page,
    consoleErrors,
  }) => {
    await gotoEditor(page);

    // Four fields, deliberately one per specificity class, because a fix that
    // only sizes a bare `input` selector leaves the class-scoped ones at 13px
    // and the bug survives in most of the places it actually bit:
    //
    //   #docTitle             — no size rule of its own    (inherits 13px)
    //   #findInput            — .find-panel input[type=text]   (0,2,1)
    //   .ctl > input[number]  — its own child-combinator rule  (0,2,1)
    //   #propDescription      — .dialog-field > textarea       (0,1,1)

    expect(await fontSize(page.locator("#docTitle"))).toBeGreaterThanOrEqual(IOS_NO_ZOOM_PX);

    const numberField = page.locator('.ctl > input[type="number"]').first();
    await expect(numberField).toHaveCount(1);
    expect(await fontSize(numberField)).toBeGreaterThanOrEqual(IOS_NO_ZOOM_PX);

    await page.keyboard.press(`${MOD}+f`);
    const findInput = page.locator("#findInput");
    await expect(findInput).toBeVisible();
    expect(await fontSize(findInput)).toBeGreaterThanOrEqual(IOS_NO_ZOOM_PX);
    await page.keyboard.press("Escape");

    await page.locator("#propertiesBtn").click();
    const description = page.locator("#propDescription");
    await expect(description).toBeVisible();
    expect(await fontSize(description)).toBeGreaterThanOrEqual(IOS_NO_ZOOM_PX);

    expect(consoleErrors).toEqual([]);
  });

  test("rows in floating menus reach the 44px finger floor", async ({ page, consoleErrors }) => {
    await gotoEditor(page);

    // An application menu.
    await page.locator('.app-menu-button[data-menu="edit"]').click();
    const menuRow = page.locator("#appMenuPopover .app-menu-item").first();
    await expect(menuRow).toBeVisible();
    expect((await stableBox(menuRow)).height).toBeGreaterThanOrEqual(TOUCH_TARGET_PX);
    await page.keyboard.press("Escape");

    // The command palette — the documented keyboard/AT fallback surface.
    await page.locator("#searchTrigger").click();
    const cmdRow = page.locator(".cmd-item").first();
    await expect(cmdRow).toBeVisible();
    expect((await stableBox(cmdRow)).height).toBeGreaterThanOrEqual(TOUCH_TARGET_PX);
    await page.keyboard.press("Escape");

    // The comment column's icon buttons, which float over the page.
    await page.locator("#railReview").click();
    const navButton = page.locator("#reviewNext");
    await expect(navButton).toBeVisible();
    const nav = await stableBox(navButton);
    expect(nav.width).toBeGreaterThanOrEqual(TOUCH_TARGET_PX);
    expect(nav.height).toBeGreaterThanOrEqual(TOUCH_TARGET_PX);

    // ...and the growth is spent on floating chrome only: the document canvas
    // must not have lost height to it.
    const ribbonHeight = (await stableBox(page.locator(".ribbon"))).height;
    expect(ribbonHeight).toBeLessThan(180);

    expect(consoleErrors).toEqual([]);
  });
});

// ---- HF-098 -----------------------------------------------------------------
// Not touch-specific: a 9px grip is fiddly with a trackpad and unusable with a
// pen, so the expanded target has to hold for every pointer.
test("an image resize grip is hit-testable well outside its 9px visual", async ({
  page,
  consoleErrors,
}) => {
  await page.goto("/editor.html?fixture=float");
  await page.waitForFunction(
    () => {
      const s = document.getElementById("status");
      return s && s.textContent === "" && document.querySelectorAll(".page-wrap").length > 0;
    },
    null,
    { timeout: 45_000 },
  );

  const canvas = page.locator(".page-wrap .page").first();
  const box = await canvas.boundingBox();
  await canvas.click({ position: { x: box.width * 0.14, y: box.height * 0.11 } });
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");

  const hitAt = (x, y) =>
    page.evaluate(
      ([px, py]) => {
        const el = document.elementFromPoint(px, py);
        return el?.classList.contains("object-handle") ? el.dataset.handle : null;
      },
      [x, y],
    );

  // Two grips whose outward directions are opposite, so a mistake in one
  // direction cannot hide behind the other.
  for (const [index, outX, outY] of [
    ["4", +1, +1], // south-east: grows right and down
    ["0", -1, -1], // north-west: grows left and up
  ]) {
    const handle = page.locator(`.overlay .object-handle[data-handle="${index}"]`);
    await expect(handle).toBeVisible();
    const grip = await stableBox(handle);

    // The drawn grip stays small — this is chrome, not a button.
    expect(grip.width).toBeLessThanOrEqual(12);

    const cx = grip.x + grip.width / 2;
    const cy = grip.y + grip.height / 2;

    // The grip still owns its own centre. This is the assertion that catches an
    // over-eager expansion: a neighbouring grip's target must never cover it.
    expect(await hitAt(cx, cy)).toBe(index);

    // ...and the target reaches out to WCAG 2.5.8's 24px on each of the grip's
    // OWN drag axes, 12px past the visual in the outward direction.
    expect(await hitAt(cx + outX * 12, cy)).toBe(index);
    expect(await hitAt(cx, cy + outY * 12)).toBe(index);

    // Inward, it stays put: growing over the object is what stole the corner
    // drag from itself on a small image.
    expect(await hitAt(cx - outX * 12, cy)).not.toBe(index);
    expect(await hitAt(cx, cy - outY * 12)).not.toBe(index);
  }

  expect(consoleErrors).toEqual([]);
});
