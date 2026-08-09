// THE CONTEXT AUDIT: does the editor keep you where you are working?
//
// The defects behind this file were all one class — a rule the host INFERRED
// (from a hit-test miss, from `pages[0]`, from its own copy of the page setup)
// instead of a rule stated once. The header was fixed that way. This sweeps the
// same rule across every other place a user can be working, and across the
// chrome that can silently take the keyboard away from them.
//
// Two questions per context:
//   Does clicking inside it — on EMPTY space, not on a glyph — keep me here?
//   Does using the chrome (ribbon tab, panel) lose my next keystroke?
import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  moveCaretToDocStart,
  MOD,
} from "./fixtures.mjs";

/** Inserts a text box through the palette and leaves the caret inside it. */
async function insertTextBox(page) {
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.press(`${MOD}+Shift+P`);
  await page.locator("#cmdInput").fill("Text box");
  await page.locator("#cmdList .cmd-item", { hasText: "Text box" }).first().click();
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "editing");
  await expect(page.locator(".overlay .caret")).toHaveCount(1);
}

/** The selected object's page rect in client pixels, from the engine's own
 *  geometry via the selection outline the editor draws. */
async function objectRect(page) {
  const box = await page.locator(".object-handle").first().boundingBox();
  expect(box, "a selected object draws handles").not.toBeNull();
  return box;
}

test("a click on empty space INSIDE a text box keeps you editing it", async ({
  page,
  consoleErrors,
}) => {
  // The header's defect, asked of the text box: empty space inside the box is
  // still the box. A hit-test that finds no glyph there must not read as
  // "clicked away".
  await gotoEditor(page);
  await insertTextBox(page);
  await page.keyboard.type("HI");

  // The box's real extent, from the product's own chrome: Escape once drops to
  // "selected", which draws the eight resize handles at its corners and edges.
  // Their bounding box IS the box, so the click point below is inside it by
  // construction rather than by a guessed fraction of the page.
  await page.keyboard.press("Escape");
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
  const handles = page.locator(".overlay .object-handle");
  await expect(handles).toHaveCount(8);
  const rect = await page.evaluate(() => {
    const boxes = [...document.querySelectorAll(".overlay .object-handle")].map((el) =>
      el.getBoundingClientRect(),
    );
    const left = Math.min(...boxes.map((b) => b.left));
    const right = Math.max(...boxes.map((b) => b.right));
    const top = Math.min(...boxes.map((b) => b.top));
    const bottom = Math.max(...boxes.map((b) => b.bottom));
    return { left, top, width: right - left, height: bottom - top };
  });

  // Back into the box, then click its empty right-hand end.
  await page.mouse.dblclick(rect.left + rect.width * 0.1, rect.top + rect.height * 0.5);
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "editing");
  await page.mouse.click(rect.left + rect.width * 0.8, rect.top + rect.height * 0.5);

  await expect(
    page.locator("#pages"),
    "clicking empty space inside the box must not throw you out",
  ).toHaveAttribute("data-object-mode", "editing");
  await page.keyboard.type("MORE");
  await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", "Undo Typing");

  expect(consoleErrors).toEqual([]);
});

test("switching ribbon tabs does not swallow the next keystroke", async ({
  page,
  consoleErrors,
}) => {
  // Found while auditing the toggles: inserting from the Insert tab left focus
  // on the ribbon, so the very next thing typed went nowhere. In Word and Docs
  // the ribbon never takes the caret away from the document.
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  await page.locator("#tabInsert").click();
  await expect(page.locator("#panelInsert")).toBeVisible();
  await page.locator("#tabHome").click();

  await page.keyboard.type("AFTERTAB");
  await expect(
    page.locator("#undoBtn"),
    "typing after a tab switch must reach the document",
  ).toHaveAttribute("aria-label", "Undo Typing");
  await expect(page.locator("#a11yDocument")).toContainText("AFTERTAB");

  expect(consoleErrors).toEqual([]);
});

test("inserting from the ribbon leaves you able to type", async ({ page, consoleErrors }) => {
  // The concrete case: Insert ▸ Text box from the RIBBON (not the palette). The
  // command puts the caret in the new box; the ribbon must not then hold the
  // keyboard.
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.locator("#tabInsert").click();
  await page.locator("#insertTextBoxBtn").click();
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "editing");

  await page.keyboard.type("TYPED");
  await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", "Undo Typing");

  expect(consoleErrors).toEqual([]);
});

test("Escape steps out of a text box in one step, then out of the object", async ({
  page,
  consoleErrors,
}) => {
  // The grammar docs/85 §4 defines: editing → selected → text caret. A context
  // you can enter but not leave predictably is as bad as one you fall out of.
  await gotoEditor(page);
  await insertTextBox(page);

  await page.keyboard.press("Escape");
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
  await page.keyboard.press("Escape");
  await expect(page.locator("#pages")).not.toHaveAttribute("data-object-mode", "selected");

  expect(consoleErrors).toEqual([]);
});

test("a click in the body is the way out of a text box", async ({ page, consoleErrors }) => {
  // A tall enough window that a point below the box is actually ON SCREEN — a
  // click outside the window hits nothing and reports a product failure that is
  // really a measurement one.
  await page.setViewportSize({ width: 1280, height: 1000 });
  await gotoEditor(page);
  await insertTextBox(page);
  const canvas = page.locator(".page-wrap .page").first();
  let box = null;
  await expect
    .poll(async () => {
      box = await canvas.boundingBox();
      return box?.width ?? 0;
    })
    .toBeGreaterThan(0);

  // Below the 1-inch-tall box, in ordinary body prose, and inside the window.
  const target = { x: box.x + box.width * 0.5, y: box.y + 420 };
  expect(
    await page.evaluate((p) => document.elementFromPoint(p.x, p.y) !== null, target),
    "the point must be inside the window",
  ).toBe(true);
  await page.mouse.click(target.x, target.y);
  await expect(page.locator("#pages")).not.toHaveAttribute("data-object-mode", "editing");
  await page.keyboard.type("BODY");
  await expect(page.locator("#a11yDocument")).toContainText("BODY");

  expect(consoleErrors).toEqual([]);
});
