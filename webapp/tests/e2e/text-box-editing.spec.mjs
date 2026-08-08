// Editing the text inside an inline text box.
//
// "Edit mode" existed as a state flag and nothing else: double-clicking a text
// box changed its border and left no caret, so there was nowhere for a keystroke
// to go. Three layers were missing, the same three header editing needed —
// resolution (the ops could not reach a paragraph inside a box), geometry (a
// text box's lines hang off a LINE, not the page's block list, so its content
// contributed no line boxes at all), and an entry point for resolving a click.
//
// No fixture in the repo contained a text box, which is why this went unverified
// for so long. `fixtures/generated/inline-text-box.docx` is a minimal document
// with one VML text box, built for exactly this.
import { test, expect, gotoEditor } from "./fixtures.mjs";

const FIXTURE = "../fixtures/generated/inline-text-box.docx";

// Where the fixture's box sits, as a fraction of the page. Found by scanning for
// `data-object-kind`, not guessed.
const BOX = { fx: 0.18, fy: 0.11 };
const BOX_END = { fx: 0.28, fy: 0.11 };

async function openFixture(page) {
  await gotoEditor(page);
  await page.locator("#file").setInputFiles(FIXTURE);
  await expect
    .poll(() => page.locator("#docTitle").inputValue(), { timeout: 30_000 })
    .toContain("inline-text-box");
  const canvas = page.locator(".page-wrap .page").first();
  await expect(canvas).toBeVisible();
  // Poll for a laid-out box: the canvas can be attached before it has geometry.
  await expect.poll(async () => (await canvas.boundingBox())?.width ?? 0).toBeGreaterThan(0);
  return canvas.boundingBox();
}

const at = (box, spot) => [box.x + box.width * spot.fx, box.y + box.height * spot.fy];

test("a single click selects the box, a double-click puts the caret inside it", async ({
  page,
  consoleErrors,
}) => {
  const box = await openFixture(page);

  await page.mouse.click(...at(box, BOX));
  await expect(page.locator("#pages")).toHaveAttribute("data-object-kind", "textbox");
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");

  // Word's grammar: click selects the object, double-click enters its text.
  await page.mouse.dblclick(...at(box, BOX));
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "editing");
  await expect(page.locator(".overlay .caret")).toHaveCount(1);
  await expect(page.locator("#status")).toContainText("Editing the text box");

  expect(consoleErrors).toEqual([]);
});

test("typing edits the box's own text, not the body", async ({ page, consoleErrors }) => {
  const box = await openFixture(page);
  const bodyBefore = await page.locator("#a11yDocument").textContent();

  await page.mouse.dblclick(...at(box, BOX));
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "editing");
  await page.keyboard.type("EDITED");

  // A real, undoable edit — and the body projection is untouched, so it went
  // into the box.
  await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", "Undo Typing");
  expect(await page.locator("#a11yDocument").textContent()).toBe(bodyBefore);

  expect(consoleErrors).toEqual([]);
});

test("the caret lands where the double-click aimed, not at the box's start", async ({
  page,
  consoleErrors,
}) => {
  const box = await openFixture(page);

  // Entry used to probe the box top-down and take the first hit, so every entry
  // put the caret near offset 0 whatever the user clicked.
  await page.mouse.dblclick(...at(box, BOX_END));
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "editing");

  const offset = await page.evaluate(() => {
    const caret = document.querySelector(".overlay .caret");
    return caret ? Math.round(Number.parseFloat(caret.style.left)) : -1;
  });
  const start = await page.evaluate(() => {
    const outline = document.querySelector(".overlay .object-outline");
    return outline ? Math.round(Number.parseFloat(outline.style.left)) : -1;
  });
  expect(offset).toBeGreaterThan(start + 20);

  expect(consoleErrors).toEqual([]);
});

test("Escape steps out to the box, then out of it", async ({ page, consoleErrors }) => {
  const box = await openFixture(page);
  await page.mouse.dblclick(...at(box, BOX));
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "editing");

  // The two-step Escape (docs/85 §4.3): text → object → document.
  await page.keyboard.press("Escape");
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
  await page.keyboard.press("Escape");
  await expect(page.locator("#pages")).not.toHaveAttribute("data-object-mode", "selected");

  expect(consoleErrors).toEqual([]);
});
