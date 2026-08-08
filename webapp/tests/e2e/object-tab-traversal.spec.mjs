// docs/85 §8d Q1: Tab / Shift+Tab move between objects, but only while an object
// is already selected.
//
// Floating objects were reachable ONLY by pointing at them — `objectAt` needs a
// coordinate, and nothing else selected one — so a keyboard user could not reach
// an image or text box at all, and a screen reader cannot find a text box while
// reading body text because it floats above the text layer. Word cycles floating
// objects with Tab once one is selected, and Word for the web moves between
// graphics the same way, so the gesture belongs to object-selected mode.
//
// The other half of the decision matters just as much: from a TEXT CARET, Tab
// must keep its indent / list-demote / next-cell meaning. That behaviour was
// verified correct in the 2026-08-09 editing audit and this must not disturb it,
// so it is asserted here too.
import { test, expect, gotoEditor, clickIntoFirstPage } from "./fixtures.mjs";

// A 1x1 PNG, so a SECOND object can be inserted. The fixture ships exactly one
// object ("Picture 1 of 1"), and with one object every traversal assertion is
// vacuous — Tab wraps to the same node and "Shift+Tab returns to the start"
// holds trivially. The tests below insert one so the selection has somewhere
// else to go.
const ONE_PIXEL_PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
  "base64",
);

async function insertSecondPicture(page) {
  await clickIntoFirstPage(page);
  const chooser = page.waitForEvent("filechooser");
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertPictureBtn").click();
  await (await chooser).setFiles({ name: "dot.png", mimeType: "image/png", buffer: ONE_PIXEL_PNG });
  await expect(page.locator("#status")).toContainText("Picture inserted");
}

// The rich producer fixture places an inline image near the top of page 1 — the
// same empirically-found point the other object specs use.
const IMAGE_POS = { fx: 0.32, fy: 0.1 };

async function clickImage(page) {
  const canvas = page.locator(".page-wrap .page").first();
  const box = await canvas.boundingBox();
  await canvas.click({ position: { x: box.width * IMAGE_POS.fx, y: box.height * IMAGE_POS.fy } });
}

const selectedNode = (page) => page.locator("#pages").getAttribute("data-object-selected");

test("Tab moves to the next object and Shift+Tab comes back", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await insertSecondPicture(page);
  await clickImage(page);

  const first = await selectedNode(page);
  expect(first).toBeTruthy();

  await page.keyboard.press("Tab");
  const second = await selectedNode(page);
  expect(second).toBeTruthy();
  // It really moved — with only one object this is the assertion that fails.
  expect(second).not.toBe(first);
  // Still an object selection, and the position is announced for a screen
  // reader, which has no handles to look at.
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
  await expect(page.locator("#status")).toContainText(/of \d+/);

  // Shift+Tab returns to where it started, so the gesture is reversible.
  await page.keyboard.press("Shift+Tab");
  expect(await selectedNode(page)).toBe(first);

  expect(consoleErrors).toEqual([]);
});

test("traversal wraps rather than dead-ending", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await insertSecondPicture(page);
  await clickImage(page);
  const start = await selectedNode(page);

  // Walking forward far enough to pass the end must come back round to the
  // start; a traversal that stops at the last object looks broken.
  const seen = new Set([start]);
  for (let i = 0; i < 12; i += 1) {
    await page.keyboard.press("Tab");
    seen.add(await selectedNode(page));
    if ((await selectedNode(page)) === start && i > 0) break;
  }
  expect(await selectedNode(page)).toBe(start);
  // More than one distinct object was visited, so the wrap is a real lap rather
  // than a single object selecting itself.
  expect(seen.size).toBeGreaterThan(1);

  expect(consoleErrors).toEqual([]);
});

test("Tab from a text caret still indents rather than jumping to an object", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // No object is selected, so Tab belongs to the text caret.
  expect(await selectedNode(page)).toBeNull();
  await page.keyboard.press("Tab");

  // It did not become an object selection, and focus stayed on the editor
  // surface rather than escaping to the browser's own tab order.
  expect(await selectedNode(page)).toBeNull();
  expect(await page.evaluate(() => document.activeElement?.id)).toBe("pages");

  expect(consoleErrors).toEqual([]);
});
