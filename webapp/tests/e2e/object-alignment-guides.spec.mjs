// Alignment guides and snapping, and double-click as the crop gesture.
//
// Every assertion here is about the LAID-OUT DOCUMENT after the gesture, not
// about chrome existing: "the image's centre is on the page's centre line",
// not "a guide element was rendered". The guide is checked too, but only as
// the explanation — the placement is the feature.
//
// Each test CREATES its condition: it drags the image somewhere off-centre
// first, so a pass cannot come from an image that happened to start aligned.
import { test, expect, gotoEditor } from "./fixtures.mjs";

const FLOAT_POS = { fx: 0.14, fy: 0.11 };
const IMAGE_POS = { fx: 0.32, fy: 0.1 };

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

async function selectAt(page, pos) {
  const canvas = page.locator(".page-wrap .page").first();
  const box = await canvas.boundingBox();
  await canvas.click({ position: { x: box.width * pos.fx, y: box.height * pos.fy } });
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
}

/** The selected object's outline, in viewport pixels. */
async function outline(page) {
  return page.locator(".overlay .object-outline").first().evaluate((el) => {
    const r = el.getBoundingClientRect();
    return { x: r.left, y: r.top, w: r.width, h: r.height, cx: r.left + r.width / 2 };
  });
}

/** The sheet's own rect, so a page-relative expectation can be stated. */
async function sheet(page) {
  return page.locator(".page-wrap .page").first().evaluate((el) => {
    const r = el.getBoundingClientRect();
    return { x: r.left, w: r.width, cx: r.left + r.width / 2 };
  });
}

/** Drags from one viewport point to another, leaving the button UP. */
async function dragTo(page, from, to, { steps = 12 } = {}) {
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(to.x, to.y, { steps });
  await page.mouse.up();
  await page.waitForTimeout(250);
}

test("an image dropped near the page centre comes to rest ON it", async ({
  page,
  consoleErrors,
}) => {
  await gotoFloat(page);
  await selectAt(page, FLOAT_POS);

  // CREATE THE CONDITION: park the image well away from the centre first, so a
  // pass cannot come from an image that was already centred.
  const sheetBox = await sheet(page);
  const start = await outline(page);
  await dragTo(
    page,
    { x: start.x + start.w / 2, y: start.y + start.h / 2 },
    { x: sheetBox.x + sheetBox.w * 0.25, y: start.y + start.h / 2 },
  );
  const parked = await outline(page);
  expect(Math.abs(parked.cx - sheetBox.cx)).toBeGreaterThan(40);

  // Now drop it with its centre a few pixels off the page's centre line.
  const OFF_BY = 5;
  await dragTo(
    page,
    { x: parked.cx, y: parked.y + parked.h / 2 },
    { x: sheetBox.cx + OFF_BY, y: parked.y + parked.h / 2 },
  );

  // THE GUARANTEE: the object is centred in the laid-out document, not left
  // where the pointer was. Measured before this change: it stayed 5px off.
  const dropped = await outline(page);
  expect(Math.abs(dropped.cx - sheetBox.cx)).toBeLessThanOrEqual(1.5);

  expect(consoleErrors).toEqual([]);
});

test("a drop far from every line is left exactly where the pointer put it", async ({
  page,
  consoleErrors,
}) => {
  await gotoFloat(page);
  await selectAt(page, FLOAT_POS);
  const sheetBox = await sheet(page);
  const start = await outline(page);

  // A point deliberately between the left margin and the centre line.
  const target = sheetBox.x + sheetBox.w * 0.32;
  await dragTo(
    page,
    { x: start.x + start.w / 2, y: start.y + start.h / 2 },
    { x: target, y: start.y + start.h / 2 },
  );

  const dropped = await outline(page);
  // Free placement survives: a snap that swallowed every drop would fail here.
  expect(Math.abs(dropped.cx - target)).toBeLessThanOrEqual(3);
  expect(Math.abs(dropped.cx - sheetBox.cx)).toBeGreaterThan(40);

  expect(consoleErrors).toEqual([]);
});

test("the guide names the line the object was pulled onto, and leaves with the drag", async ({
  page,
  consoleErrors,
}) => {
  await gotoFloat(page);
  await selectAt(page, FLOAT_POS);
  const sheetBox = await sheet(page);
  const start = await outline(page);

  await page.mouse.move(start.x + start.w / 2, start.y + start.h / 2);
  await page.mouse.down();
  await page.mouse.move(sheetBox.cx + 4, start.y + start.h / 2, { steps: 12 });

  const guide = page.locator(".overlay .object-align-guide:not([hidden])");
  await expect(guide).toHaveCount(1);
  await expect(guide).toHaveAttribute("data-guide", "pageCentre");

  await page.mouse.up();
  await page.waitForTimeout(250);
  // A guide that outlives its drag is a line across the page explaining nothing.
  await expect(page.locator(".overlay .object-align-guide")).toHaveCount(0);

  expect(consoleErrors).toEqual([]);
});

test("holding Alt while dropping suppresses the pull", async ({ page, consoleErrors }) => {
  await gotoFloat(page);
  await selectAt(page, FLOAT_POS);
  const sheetBox = await sheet(page);
  const start = await outline(page);

  const OFF_BY = 5;
  await page.mouse.move(start.x + start.w / 2, start.y + start.h / 2);
  await page.mouse.down();
  await page.keyboard.down("Alt");
  await page.mouse.move(sheetBox.cx + OFF_BY, start.y + start.h / 2, { steps: 12 });
  await expect(page.locator(".overlay .object-align-guide:not([hidden])")).toHaveCount(0);
  await page.mouse.up();
  await page.keyboard.up("Alt");
  await page.waitForTimeout(250);

  // The deliberate off-centre placement survived.
  const dropped = await outline(page);
  expect(Math.abs(dropped.cx - sheetBox.cx)).toBeGreaterThan(2);

  expect(consoleErrors).toEqual([]);
});

test("dragging an edge grip snaps that edge to the page margin", async ({
  page,
  consoleErrors,
}) => {
  await gotoFloat(page);
  await selectAt(page, FLOAT_POS);
  const sheetBox = await sheet(page);

  // The right margin of the float fixture's page, in viewport pixels: the page
  // is 8.5in wide with 1in margins, so the margin sits at 1 - 1/8.5 of the way
  // across from the right edge.
  const rightMargin = sheetBox.x + sheetBox.w * (1 - 1 / 8.5);

  // CREATE THE CONDITION: the east edge starts nowhere near the right margin.
  const before = await outline(page);
  expect(Math.abs(before.x + before.w - rightMargin)).toBeGreaterThan(40);

  // Drag the east grip (kind 3) to just short of the right margin.
  const grip = page.locator('.overlay .object-handle[data-handle="3"]');
  const gb = await grip.boundingBox();
  await dragTo(
    page,
    { x: gb.x + gb.width / 2, y: gb.y + gb.height / 2 },
    { x: rightMargin - 4, y: gb.y + gb.height / 2 },
  );

  // THE GUARANTEE: the image is wider in the laid-out document AND its right
  // edge is on the margin, not 4px short of it.
  const after = await outline(page);
  expect(after.w).toBeGreaterThan(before.w + 10);
  expect(Math.abs(after.x + after.w - rightMargin)).toBeLessThanOrEqual(2);

  expect(consoleErrors).toEqual([]);
});

test("double-clicking a picture enters crop, the way Docs does", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await selectAt(page, IMAGE_POS);

  // CREATE THE CONDITION: not cropping yet, and the bar offers Crop rather than
  // Apply — so a pass cannot come from a session that was already open.
  await expect(page.locator(".overlay .object-crop-handle")).toHaveCount(0);

  const canvas = page.locator(".page-wrap .page").first();
  const box = await canvas.boundingBox();
  await canvas.dblclick({
    position: { x: box.width * IMAGE_POS.fx, y: box.height * IMAGE_POS.fy },
  });
  await page.waitForTimeout(300);

  // THE GUARANTEE: a live crop session — eight crop grips and the dimmed chrome.
  await expect(page.locator(".overlay .object-crop-handle")).toHaveCount(8);
  await expect(page.locator(".overlay .object-crop-rect")).toHaveCount(1);
  // And the bar stops describing the gesture it is not offering.
  await expect(page.locator(".object-context-bar")).toContainText("Drag the edges to crop");
  await expect(page.locator(".object-context-bar")).not.toContainText("Drag handles to resize");

  await page.keyboard.press("Escape");
  expect(consoleErrors).toEqual([]);
});

test("a crop drag says what size it is keeping", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await selectAt(page, IMAGE_POS);
  const canvas = page.locator(".page-wrap .page").first();
  const box = await canvas.boundingBox();
  await canvas.dblclick({
    position: { x: box.width * IMAGE_POS.fx, y: box.height * IMAGE_POS.fy },
  });
  await page.waitForTimeout(300);

  const readout = page.locator(".overlay .object-crop-rect .object-resize-readout");
  const before = await readout.textContent();
  expect(before).toMatch(/\d/);

  // Pull the east grip inward and watch the kept width fall.
  const grip = page.locator('.overlay .object-crop-handle[data-handle="3"]');
  const gb = await grip.boundingBox();
  await page.mouse.move(gb.x + gb.width / 2, gb.y + gb.height / 2);
  await page.mouse.down();
  await page.mouse.move(gb.x - 30, gb.y + gb.height / 2, { steps: 8 });
  const during = await readout.textContent();
  await page.mouse.up();

  const widthOf = (text) => Number(String(text).match(/[\d.]+/)?.[0]);
  expect(widthOf(during)).toBeLessThan(widthOf(before));

  await page.keyboard.press("Escape");
  expect(consoleErrors).toEqual([]);
});

test.describe("with a finger", () => {
  // `hasTouch` makes Chromium report a coarse primary pointer, which is what
  // puts the page on the touch path. Desktop width, because the question is the
  // gesture and not the layout.
  test.use({ hasTouch: true });

  test("dragging a floating image with a touch pointer MOVES it, and does not scroll", async ({
    page,
    consoleErrors,
  }) => {
    await gotoFloat(page);
    await selectAt(page, FLOAT_POS);

    const before = await outline(page);
    const scrollBefore = await page.evaluate(() => document.getElementById("viewport").scrollTop);

    // Playwright's touchscreen has no drag primitive, so the pointer events a
    // touch produces are driven directly — with `pointerType: "touch"`, which
    // is the thing the fix has to survive. Before pointer capture, these left
    // the gesture to the browser's own scroll and the image never moved.
    await page.evaluate(
      ([x, y, dx, dy]) => {
        const target = document.elementFromPoint(x, y);
        const fire = (type, cx, cy) =>
          target.dispatchEvent(
            new PointerEvent(type, {
              pointerId: 1,
              pointerType: "touch",
              isPrimary: true,
              clientX: cx,
              clientY: cy,
              bubbles: true,
              cancelable: true,
            }),
          );
        fire("pointerdown", x, y);
        for (let step = 1; step <= 10; step += 1) {
          fire("pointermove", x + (dx * step) / 10, y + (dy * step) / 10);
        }
        fire("pointerup", x + dx, y + dy);
      },
      [before.x + before.w / 2, before.y + before.h / 2, 130, 0],
    );
    await page.waitForTimeout(400);

    // THE GUARANTEE: the image is further right in the laid-out document, and
    // the view did not scroll in its place.
    const after = await outline(page);
    expect(after.x - before.x).toBeGreaterThan(60);
    const scrollAfter = await page.evaluate(() => document.getElementById("viewport").scrollTop);
    expect(scrollAfter).toBe(scrollBefore);

    expect(consoleErrors).toEqual([]);
  });
});
