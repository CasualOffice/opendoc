// The object toolbar as ONE anchored popover (`docs/172`), and the side grip's
// reach (`docs/109` OL-05).
//
// What a user sees, asserted in the browser: the bar is a named toolbar, it
// sits BELOW the object (Docs' placement) clear of every grip and inside the
// page view, it is narrow enough to be about the object rather than the page,
// it steps aside while the object is dragged, it hides when the object scrolls
// out of view, and the keyboard can reach it and leave it. The geometry rule
// itself is a node test (`tests/object_anchor.test.mjs`).
import { expect, gotoEditor, stableBox, test } from "./fixtures.mjs";

const IMAGE_POS = { fx: 0.32, fy: 0.1 };

async function selectImage(page) {
  const canvas = page.locator(".page-wrap .page").first();
  const box = await stableBox(canvas);
  await canvas.click({ position: { x: box.width * IMAGE_POS.fx, y: box.height * IMAGE_POS.fy } });
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
  await expect(page.locator(".object-context-bar")).toBeVisible();
}

/** Grows the demo's 16px picture to ~130px by its south-east grip, so the side
 *  grips are far enough apart to be told from the corners. */
async function enlargeImage(page) {
  const se = await stableBox(page.locator('.overlay .object-handle[data-handle="4"]'));
  await page.mouse.move(se.x + se.width / 2, se.y + se.height / 2);
  await page.mouse.down();
  await page.mouse.move(se.x + 120, se.y + 120, { steps: 8 });
  await page.mouse.up();
  await expect.poll(async () => (await stableBox(page.locator(".overlay .object-outline").first())).width).toBeGreaterThan(100);
}

test("the bar is a named toolbar below its object, clear of the grips and narrower than the page", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await selectImage(page);
  const bar = page.locator(".object-context-bar");
  await expect(bar).toHaveAttribute("role", "toolbar");
  await expect(bar).toHaveAttribute("aria-label", "Image actions");
  await expect(bar).toHaveAttribute("data-side", "below");

  const barBox = await stableBox(bar);
  const outline = await stableBox(page.locator(".overlay .object-outline").first());
  const sheet = await stableBox(page.locator(".page-wrap .page").first());
  // Below the object AND below every grip's target (the south grips grow 15px).
  const southGrips = await page.locator('.overlay .object-handle:is([data-handle="4"],[data-handle="5"],[data-handle="6"])').all();
  for (const grip of southGrips) {
    const box = await stableBox(grip);
    expect(barBox.y, "the bar must not sit inside a south grip's target").toBeGreaterThanOrEqual(box.y + box.height + 15);
  }
  expect(barBox.y).toBeGreaterThan(outline.y + outline.height);
  // About the object: centred on it, and nowhere near the width of the page.
  const barCentre = barBox.x + barBox.width / 2;
  const objectCentre = outline.x + outline.width / 2;
  expect(Math.abs(barCentre - objectCentre)).toBeLessThanOrEqual(2);
  expect(barBox.width).toBeLessThan(sheet.width * 0.75);
  // Every control has a name.
  for (const control of await bar.locator("button").all()) {
    expect(await control.getAttribute("aria-label")).toBeTruthy();
  }
  expect(consoleErrors).toEqual([]);
});

test("Alt+F10 reaches the toolbar, the arrows walk it, and Escape gives the keyboard back", async ({ page }) => {
  await gotoEditor(page);
  await selectImage(page);
  const bar = page.locator(".object-context-bar");
  const before = await page.evaluate(() => document.activeElement?.className ?? "");

  await page.keyboard.press("Alt+F10");
  const controls = bar.locator("button");
  await expect(controls.first()).toBeFocused();
  // One tab stop: only the focused control is in the tab order.
  await expect(bar.locator('button[tabindex="0"]')).toHaveCount(1);
  await page.keyboard.press("ArrowRight");
  await expect(controls.nth(1)).toBeFocused();
  await page.keyboard.press("End");
  await expect(controls.last()).toBeFocused();
  await page.keyboard.press("Home");
  await expect(controls.first()).toBeFocused();

  await page.keyboard.press("Escape");
  expect(await page.evaluate(() => document.activeElement?.className ?? "")).toBe(before);
  // Leaving the toolbar is not leaving the object.
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
});

test("the bar steps aside while the object is dragged and comes back after", async ({ page }) => {
  await gotoEditor(page);
  await selectImage(page);
  await enlargeImage(page);
  const bar = page.locator(".object-context-bar");
  const outline = await stableBox(page.locator(".overlay .object-outline").first());
  await page.mouse.move(outline.x + outline.width / 2, outline.y + outline.height / 2);
  await page.mouse.down();
  await expect(bar).toHaveClass(/is-suspended/);
  await expect(bar).not.toBeVisible();
  await page.mouse.up();
  await expect(bar).not.toHaveClass(/is-suspended/);
  await expect(bar).toBeVisible();
});

test("the bar hides when its object scrolls out of view and returns with it", async ({ page }) => {
  await gotoEditor(page);
  await selectImage(page);
  const bar = page.locator(".object-context-bar");
  // Room to scroll: the demo document is one short page.
  await page.locator("#viewport").evaluate((el) => {
    el.style.paddingBottom = "3000px";
  });
  await page.locator("#viewport").evaluate((el) => el.scrollBy(0, 1200));
  await expect(bar).toBeHidden();
  await page.locator("#viewport").evaluate((el) => el.scrollTo(0, 0));
  await expect(bar).toBeVisible();
  await expect(bar).toHaveAttribute("data-side", "below");
});

// `docs/109` OL-05. A press 5px off a SIDE grip's centre — along the edge, and
// a few pixels into the picture — used to land on the object body and MOVE the
// picture. It has to resize, as ONLYOFFICE's ~10px grip radius does.
test("a press just off the side grip still resizes rather than moving the picture", async ({ page }) => {
  await gotoEditor(page);
  await selectImage(page);
  await enlargeImage(page);
  const outline = page.locator(".overlay .object-outline").first();
  const before = await stableBox(outline);
  const east = await stableBox(page.locator('.overlay .object-handle[data-handle="3"]'));
  const cx = east.x + east.width / 2;
  const cy = east.y + east.height / 2;
  // 6px below the grip's centre and 4px inside the picture.
  await page.mouse.move(cx - 4, cy + 6);
  await page.mouse.down();
  await page.mouse.move(cx + 36, cy + 6, { steps: 6 });
  await page.mouse.up();
  await expect.poll(async () => (await stableBox(outline)).width).toBeGreaterThan(before.width + 20);
  const after = await stableBox(outline);
  expect(Math.abs(after.x - before.x), "the picture's left edge did not move").toBeLessThanOrEqual(1);
  expect(Math.abs(after.y - before.y), "nor its top").toBeLessThanOrEqual(1);
});
