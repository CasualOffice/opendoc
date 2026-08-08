import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart } from "./fixtures.mjs";

// Dispatches a paste carrying a freshly-encoded PNG image file (a valid image the
// browser can decode), as the OS clipboard yields for a copied image.
async function pasteGeneratedImage(page) {
  await page.evaluate(async () => {
    const canvas = new OffscreenCanvas(6, 4);
    const ctx = canvas.getContext("2d");
    ctx.fillStyle = "#cc3322";
    ctx.fillRect(0, 0, 6, 4);
    const blob = await canvas.convertToBlob({ type: "image/png" });
    const file = new File([blob], "red.png", { type: "image/png" });
    const dt = new DataTransfer();
    dt.items.add(file);
    document.dispatchEvent(
      new ClipboardEvent("paste", { clipboardData: dt, bubbles: true, cancelable: true }),
    );
  });
}

test("pasting an image inserts a picture as one undoable action", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);

  await pasteGeneratedImage(page);

  // The picture was inserted: the document is dirty and there is one undoable
  // action labeled for the image insert.
  await expect(page.locator("#status")).toContainText("Picture inserted");
  await expect(page.locator("#documentState")).toHaveAttribute("data-state", "edited");
  await expect(page.locator("#undoBtn")).toBeEnabled();
  await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", /Insert image/i);

  // One undo removes it.
  await page.locator("#undoBtn").click();
  await expect(page.locator("#redoBtn")).toBeEnabled();

  expect(consoleErrors).toEqual([]);
});

test("inserting a picture is blocked (read-only) in Viewing mode", async ({ page }) => {
  await gotoEditor(page);
  await page.locator('#reviewModeControl [data-review-mode="viewing"]').click();
  await clickIntoFirstPage(page);
  await pasteGeneratedImage(page);
  await expect(page.locator("#status")).toContainText("read-only");
  await expect(page.locator("#undoBtn")).toBeDisabled();
});
