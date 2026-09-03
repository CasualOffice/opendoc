// Everything you can do to an image, shape or text box was behind a mouse.
//
// The floating context bar was the only reliable surface, and one capability —
// object properties — existed on that bar and nowhere else in the product. The
// command palette, which this repo documents as "the keyboard fallback for
// commands with no other keyboard route", contained no object command at all.
//
// Worst of all, the FIRST selection was unreachable: `traverseObjects` returned
// early unless an object was already selected, so Tab cycled objects only once
// you had clicked one. The comment beside that handler asserted it was "the only
// way to reach an object without a pointer" — which made the whole surface
// mouse-gated, since nothing else could select one either.
import { test, expect, stableBox } from "./fixtures.mjs";

async function gotoFloat(page) {
  await page.goto("/editor.html?fixture=float");
  await page.waitForFunction(
    () => document.querySelectorAll(".page-wrap").length > 0,
    null,
    { timeout: 45_000 },
  );
}

async function selectFloatWithMouse(page) {
  const canvas = page.locator(".page-wrap .page").first();
  // stableBox, not boundingBox: under parallel load the canvas reports null and
  // the failure reads like a broken editor rather than a busy machine.
  const box = await stableBox(canvas);
  await canvas.click({ position: { x: box.width * 0.14, y: box.height * 0.11 } });
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
  return { canvas, box };
}

async function paletteCommands(page, query) {
  await page.keyboard.press("Meta+Shift+KeyP");
  await expect(page.locator("#cmdInput")).toBeVisible();
  await page.locator("#cmdInput").fill(query);
  return page.locator("#cmdList [role=option]");
}

test("an object can be selected with no mouse at all", async ({ page, consoleErrors }) => {
  await gotoFloat(page);
  // Nothing selected, nothing clicked — the state the old guard could not leave.
  await expect(page.locator("#pages")).not.toHaveAttribute("data-object-mode", "selected");

  const options = await paletteCommands(page, "select next object");
  await expect(options.first()).toContainText("Select next object");
  await page.keyboard.press("Enter");

  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
  await expect(page.locator("#pages")).toHaveAttribute("data-object-kind", "image");

  // And having arrived, Tab keeps cycling — the behaviour that already existed
  // and had no entry point.
  await page.keyboard.press("Tab");
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");

  expect(consoleErrors).toEqual([]);
});

test("a selected object's commands are in the command palette", async ({
  page,
  consoleErrors,
}) => {
  await gotoFloat(page);
  await selectFloatWithMouse(page);

  const options = await paletteCommands(page, "object");
  const labels = await options.allTextContents();

  // The whole offered set, not a sample: a hand-wired list can satisfy any one
  // row, and the defect this guards is precisely someone adding a capability to
  // the bar and forgetting the other surfaces.
  for (const label of ["Alt text", "Wrap text", "Properties", "Delete"]) {
    expect(
      labels.some((text) => text.includes(label)),
      `no palette command for "${label}" — found: ${labels.join(" | ")}`,
    ).toBe(true);
  }

  expect(consoleErrors).toEqual([]);
});

test("object properties is reachable from the right-click menu, not only the bar", async ({
  page,
  consoleErrors,
}) => {
  await gotoFloat(page);
  const { canvas, box } = await selectFloatWithMouse(page);
  await canvas.click({
    position: { x: box.width * 0.14, y: box.height * 0.11 },
    button: "right",
  });

  const properties = page.locator('.editor-context-menu [data-command-id="object.properties"]');
  await expect(properties, "the right-click menu still has no properties row").toHaveCount(1);
  await properties.click();
  await expect(page.locator(".object-inspector")).toBeVisible();

  expect(consoleErrors).toEqual([]);
});

test("resizing an object shows the size you are dragging to", async ({
  page,
  consoleErrors,
}) => {
  await gotoFloat(page);
  await selectFloatWithMouse(page);

  // Grab the south-east handle and drag, checking mid-gesture — the readout is
  // only useful while the pointer is still down, so releasing first would test
  // nothing.
  const handle = page.locator(".overlay .object-handle").nth(4);
  const grip = await stableBox(handle);
  await page.mouse.move(grip.x + grip.width / 2, grip.y + grip.height / 2);
  await page.mouse.down();
  // Wait for the drag to actually start before moving. Under parallel load the
  // pointer events can outrun the app's paint, and asserting on the readout
  // before the preview exists fails as "no live size readout" when the real
  // story is a busy machine.
  await page.mouse.move(grip.x + 20, grip.y + 16, { steps: 4 });
  await expect(page.locator(".object-resize-preview")).toBeVisible();
  await page.mouse.move(grip.x + 90, grip.y + 70, { steps: 8 });

  const readout = page.locator(".object-resize-readout");
  await expect(readout, "no live size readout during the drag").toBeVisible();
  // Inches to two places, because that is the unit the properties panel accepts
  // — a readout in a unit you cannot type back is decoration.
  await expect(readout).toHaveText(/^\d+\.\d{2} × \d+\.\d{2} in$/);

  await page.mouse.up();
  // Gone on release — the readout belongs to the gesture, and the committed size
  // is what the properties panel is for. It deliberately does NOT also write to
  // #status: the apply path owns that line and clears it on the repaint that
  // follows, so an announcement there is a race, not a feature.
  await expect(readout).toHaveCount(0);

  expect(consoleErrors).toEqual([]);
});
