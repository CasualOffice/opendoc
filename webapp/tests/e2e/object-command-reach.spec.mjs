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
import { test, expect, stableBox, MOD } from "./fixtures.mjs";

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
  await page.keyboard.press(`${MOD}+Shift+KeyP`);
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
  // Two places, in THE UNIT IN FORCE — the guarantee, not a hard-coded "in".
  // The readout must agree with the measurement preference, because a readout in
  // a unit you cannot type back into the properties panel is decoration. Pinned
  // to the literal "in" this assertion did two wrong things at once: it failed
  // honestly when the default resolved to centimetres, and it would have passed
  // just as happily on `2.89 × 1.45 {unit}` had the suffix been a different
  // placeholder — which is exactly what shipped, an unsubstituted `{unit}`
  // rendered to the reader.
  const suffix = await page.evaluate(() => {
    const select = document.getElementById("measurementUnitSelect");
    return select?.value ?? "";
  });
  const suffixes = { inch: "in", cm: "cm", mm: "mm", point: "pt", pica: "pi" };
  const expected = suffixes[suffix] ?? "in";
  await expect(readout).toHaveText(
    new RegExp(`^\\d+\\.\\d{2} × \\d+\\.\\d{2} ${expected}$`),
  );

  await page.mouse.up();
  // Gone on release — the readout belongs to the gesture, and the committed size
  // is what the properties panel is for. It deliberately does NOT also write to
  // #status: the apply path owns that line and clears it on the repaint that
  // follows, so an announcement there is a race, not a feature.
  await expect(readout).toHaveCount(0);

  expect(consoleErrors).toEqual([]);
});


// ---- The object commands' availability follows the document (`109` HF-183) ----
// `documentHasObjects()` is what decides whether "Select next object" is offered
// or greyed with a reason, and it is now MEMOIZED, because it called
// `objectOrder()` and parsed the whole paint order to answer a yes/no question —
// O(objects) on a path the command palette runs on every keystroke and the chord
// dispatcher runs on every press (`docs/107` §4: per-interaction work is O(1) in
// document size).
//
// The cost of a cache is a stale answer, so this drives the transition in a real
// browser rather than trusting the invalidation: a document with no objects at
// all, then one inserted, with no reload in between. The complexity itself is
// guarded in node (`tests/object_presence.test.mjs`), where engine calls and
// payload bytes can be counted at n and 2n objects; a browser could only offer a
// stopwatch.
test("inserting the first object enables the object commands, with no reload", async ({
  page,
  consoleErrors,
}) => {
  await page.goto("/editor.html?fixture=float");
  await page.waitForFunction(() => document.querySelectorAll(".page-wrap").length > 0, null, {
    timeout: 45_000,
  });

  // A blank document: the one state where the answer is "no objects".
  await page.keyboard.press(`${MOD}+Shift+KeyP`);
  await page.locator("#cmdInput").fill("New blank");
  await page.locator("#cmdList [role=option]").first().click();
  await expect(page.locator("#docTitle")).toHaveValue("Untitled document.docx");

  const selectNext = async () => {
    await page.keyboard.press(`${MOD}+Shift+KeyP`);
    await page.locator("#cmdInput").fill("select next object");
    const row = page.locator('#cmdList [data-command-id="object.selectNext"]').first();
    await expect(row).toBeVisible();
    const state = {
      disabled: await row.isDisabled(),
      reason: (await row.getAttribute("title")) ?? "",
    };
    await page.keyboard.press("Escape");
    return state;
  };

  // Greyed, and it says why — never a dead control (SKILL.md §10).
  expect(await selectNext()).toEqual({
    disabled: true,
    reason: "This document has no images, shapes or text boxes",
  });

  // Insert one. Nothing reloads; the only thing that can make the answer change
  // is the invalidation at the edit choke point.
  await page.keyboard.press(`${MOD}+Shift+KeyP`);
  await page.locator("#cmdInput").fill("text box");
  await page.locator('#cmdList [data-command-id="insert.textbox"]').first().click();
  // A fresh text box opens for TEXT ENTRY rather than merely being selected, which
  // is Word's behaviour; what matters here is that an object now exists.
  await expect(page.locator("#pages")).toHaveAttribute("data-object-kind", "textbox");

  const afterInsert = await selectNext();
  expect(
    afterInsert.disabled,
    "the object commands must notice the object that was just inserted — a stale " +
      "memo greys them out with a text box on the page",
  ).toBe(false);

  // And back again: undo removes it, so the answer must return to "no objects".
  await page.keyboard.press(`${MOD}+KeyZ`);
  await expect.poll(async () => (await selectNext()).disabled).toBe(true);

  expect(consoleErrors).toEqual([]);
});
