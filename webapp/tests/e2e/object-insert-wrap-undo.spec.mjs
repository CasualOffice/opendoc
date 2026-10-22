// Three findings from the ONLYOFFICE comparison that need `main.js` and the
// engine facade (`docs/109` OL-03, OL-04, OL-06). Each is what a user sees.
import { MOD, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart, stableBox, test } from "./fixtures.mjs";

const IMAGE_POS = { fx: 0.32, fy: 0.1 };

async function selectImage(page) {
  const canvas = page.locator(".page-wrap .page").first();
  const box = await stableBox(canvas);
  await canvas.click({ position: { x: box.width * IMAGE_POS.fx, y: box.height * IMAGE_POS.fy } });
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
}

async function pasteGeneratedImage(page) {
  await page.evaluate(async () => {
    const canvas = new OffscreenCanvas(60, 40);
    const ctx = canvas.getContext("2d");
    ctx.fillStyle = "#cc3322";
    ctx.fillRect(0, 0, 60, 40);
    const blob = await canvas.convertToBlob({ type: "image/png" });
    const dt = new DataTransfer();
    dt.items.add(new File([blob], "red.png", { type: "image/png" }));
    document.dispatchEvent(new ClipboardEvent("paste", { clipboardData: dt, bubbles: true, cancelable: true }));
  });
}

/** The painted page raster itself — the canvas, without the selection chrome
 *  or the caret drawn over it. */
async function sheetPixels(page) {
  return page.locator(".page-wrap canvas").first().evaluate((canvas) => canvas.toDataURL());
}

/** Forces every page to be painted afresh: zoom in, then back. */
async function repaintEverything(page) {
  await page.locator("#zoomIn").click();
  await page.waitForTimeout(400);
  await page.locator("#zoomOut").click();
  await page.waitForTimeout(400);
}

// OL-03. Word, Docs and ONLYOFFICE all leave an inserted picture selected.
test("an inserted picture is selected, with its handles and its toolbar", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await pasteGeneratedImage(page);
  await expect(page.locator("#status")).toContainText("Picture inserted");
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
  await expect(page.locator("#pages")).toHaveAttribute("data-object-kind", "image");
  await expect(page.locator(".overlay .object-handle:not(.object-rotate-handle)")).toHaveCount(8);
  await expect(page.locator(".object-context-bar")).toHaveAttribute("aria-label", "Image actions");
  expect(consoleErrors).toEqual([]);
});

// OL-04. Choosing a wrap on an in-line picture reflows the text at once, and
// is ONE undoable action.
test("a wrap chosen on an in-line picture repaints at once and undoes in one step", async ({ page }) => {
  await gotoEditor(page);
  await selectImage(page);
  // A picture big enough that the reflow moves the text.
  const se = await stableBox(page.locator('.overlay .object-handle[data-handle="4"]'));
  await page.mouse.move(se.x + se.width / 2, se.y + se.height / 2);
  await page.mouse.down();
  await page.mouse.move(se.x + 120, se.y + 120, { steps: 8 });
  await page.mouse.up();

  await page.locator('.object-wrap-btn[data-wrap="tight"]').click();
  await expect(page.locator('.object-wrap-btn[data-wrap="tight"]')).toHaveAttribute("aria-pressed", "true");
  await page.waitForTimeout(300);
  const afterWrap = await sheetPixels(page);
  // What a full repaint draws for the SAME document state. If the wrap's
  // repaint missed the page, the sheet still shows the in-line layout and the
  // two differ.
  await repaintEverything(page);
  const fresh = await sheetPixels(page);
  expect(afterWrap === fresh, "the page after the wrap must be what a full repaint draws").toBe(true);

  // One gesture, one Undo.
  await page.keyboard.press("Escape");
  await page.keyboard.press(`${MOD}+z`);
  await selectImage(page);
  await expect(page.locator('.object-wrap-btn[data-wrap="inline"]')).toHaveAttribute("aria-pressed", "true");
});

// OL-06. The undo entry names the gesture.
test("undoing a rotation is offered as a rotation, not as a move", async ({ page }) => {
  await gotoEditor(page);
  await selectImage(page);
  await page.locator('.object-bar-btn[data-object-menu="objectRotateMenu"]').click();
  await page.locator('#objectRotateMenu [data-rotate="right90"]').click();
  await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", /Rotate object/);
  await expect(page.locator("#undoBtn")).not.toHaveAttribute("aria-label", /Object move/);
});
