// Print support (⌘/Ctrl+P). Word and Google Docs both have first-class print;
// OpenDoc had none. Because the viewport virtualizes page canvases (off-screen
// pages have no live raster), a naive `window.print()` would emit mostly-blank
// sheets. The Print command instead renders EVERY page independently into an
// off-DOM `#printContainer` (one canvas per page), opens the print dialog, then
// tears the container down — leaving the live virtualized viewport untouched.
import { test, expect, MOD, documentPageCount, shortcutHint, openAppMenu } from "./fixtures.mjs";

// Open the default editor on the shipped sample (multi-page, so page-canvas
// virtualization is genuinely in play — only on-screen pages have a live
// raster). Waits for the engine to boot and the first render to settle.
async function gotoSampleEditor(page) {
  await page.goto("/editor.html");
  await page.waitForFunction(
    () => {
      const status = document.getElementById("status");
      return (
        status !== null &&
        status.textContent === "" &&
        !status.classList.contains("error") &&
        document.querySelectorAll(".page-wrap").length > 0
      );
    },
    null,
    { timeout: 45_000 },
  );
}

// Capture every blob the app hands to `URL.createObjectURL` before any app
// script loads, so the test can read the bytes Print actually produced. The
// frame's own `print()` is never reached in headless Chromium, which is fine:
// what this asserts is WHAT was handed to the browser, not that a dialog opened.
async function capturePrintArtifact(page) {
  await page.addInitScript(() => {
    window.__blobs = [];
    const original = URL.createObjectURL.bind(URL);
    URL.createObjectURL = (object) => {
      window.__blobs.push(object);
      return original(object);
    };
    window.__printCalls = 0;
    window.print = () => {
      window.__printCalls += 1;
    };
  });
}

/** The PDF Print handed the browser, once it exists. Polled rather than slept
 *  on: the export runs across later turns of the event loop. */
async function printedPdf(page) {
  const read = () =>
    page.evaluate(async () => {
      const blob = window.__blobs.find((b) => b && b.type === "application/pdf");
      if (!blob) return null;
      const text = await blob.text();
      return {
        header: text.slice(0, 8),
        hasTextFont: text.includes("/Subtype/Type0"),
        hasToUnicode: text.includes("/ToUnicode"),
      };
    });
  await expect
    .poll(async () => (await read()) !== null, { message: "Print must hand the browser a PDF" })
    .toBe(true);
  return read();
}

test("⌘P prints REAL TEXT, not a picture of the pages", async ({ page, consoleErrors }) => {
  // The whole point of the change this guards. Print used to rasterize every
  // page at 150 DPI and print the images: soft on a 600-DPI printer, and —
  // because Print is how most people produce a PDF — a PDF with no selectable,
  // searchable or screen-readable text in it at all. `docs/98` PDF-PRINT-0
  // specifies the handoff this asserts.
  await capturePrintArtifact(page);
  await gotoSampleEditor(page);

  const pageCount = await documentPageCount(page);
  expect(pageCount).toBeGreaterThan(1); // the demo is multi-page
  const sheetsBefore = await page.locator(".page-wrap").count();
  expect(sheetsBefore).toBeLessThan(pageCount); // the viewport holds a window

  await page.keyboard.press(`${MOD}+p`);

  const pdf = await printedPdf(page);

  expect(pdf.header).toBe("%PDF-1.7");
  // A `Type0` font and a `ToUnicode` map are the difference between text a
  // reader can select, search and hear, and a picture of that text.
  expect(pdf.hasTextFont, "printed PDF must embed a real text font").toBe(true);
  expect(pdf.hasToUnicode, "printed PDF must carry a ToUnicode map").toBe(true);

  // The raster fallback must NOT have run: no page images were built.
  expect(await page.locator("#printContainer").count()).toBe(0);
  expect(await page.locator(".print-page").count()).toBe(0);

  // The viewport is untouched — printing never disturbs the virtualized set.
  await expect(page.locator("#viewport")).toBeVisible();
  expect(await page.locator(".page-wrap").count()).toBe(sheetsBefore);
  expect(await page.locator(".page-wrap .page").count()).toBeLessThan(pageCount);

  expect(consoleErrors).toEqual([]);
});

test("Print is reachable from the command palette with its ⌘P hint", async ({
  page,
  consoleErrors,
}) => {
  await capturePrintArtifact(page);
  await gotoSampleEditor(page);

  await page.keyboard.press(`${MOD}+Shift+p`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  await page.locator("#cmdInput").fill("Print");

  const item = page.locator(".cmd-item", { hasText: "Print" }).first();
  await expect(item).toBeVisible();
  await expect(item.locator(".cmd-hint")).toHaveText(shortcutHint("⌘P"));
  await item.click();

  // Reachability is the claim, so the assertion is that running it from HERE
  // produces the same real-text PDF the shortcut does.
  expect((await printedPdf(page)).hasTextFont).toBe(true);
  await expect(page.locator("#printContainer")).toHaveCount(0);

  expect(consoleErrors).toEqual([]);
});

test("Print is offered in the File menu", async ({ page, consoleErrors }) => {
  await capturePrintArtifact(page);
  await gotoSampleEditor(page);

  await openAppMenu(page, "file");
  const item = page.locator('#appMenuPopover .app-menu-item[data-command="file.print"]');
  await expect(item).toBeVisible();
  await item.click();

  expect((await printedPdf(page)).hasTextFont).toBe(true);

  expect(consoleErrors).toEqual([]);
});
