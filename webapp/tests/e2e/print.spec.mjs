// Print support (⌘/Ctrl+P). Word and Google Docs both have first-class print;
// OpenDoc had none. Because the viewport virtualizes page canvases (off-screen
// pages have no live raster), a naive `window.print()` would emit mostly-blank
// sheets. The Print command instead renders EVERY page independently into an
// off-DOM `#printContainer` (one canvas per page), opens the print dialog, then
// tears the container down — leaving the live virtualized viewport untouched.
import { test, expect, MOD } from "./fixtures.mjs";

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

// Stub `window.print` before any app script loads so triggering the Print
// command never opens a real dialog. The stub records how many print-page
// canvases existed in the container at the instant print was called (the build
// is torn down synchronously right after), so the test can assert one per page.
async function stubPrint(page) {
  await page.addInitScript(() => {
    window.__printCalls = 0;
    window.__printPageCounts = [];
    window.print = () => {
      window.__printCalls += 1;
      window.__printPageCounts.push(
        document.querySelectorAll("#printContainer .print-page").length,
      );
    };
  });
}

test("⌘P builds one print canvas per page, prints, then restores the viewport", async ({
  page,
  consoleErrors,
}) => {
  await stubPrint(page);
  await gotoSampleEditor(page);

  const pageCount = await page.locator(".page-wrap").count();
  expect(pageCount).toBeGreaterThan(1); // the demo is multi-page

  await page.keyboard.press(`${MOD}+p`);

  // (b) window.print was called exactly once...
  const calls = await page.evaluate(() => window.__printCalls);
  expect(calls).toBe(1);

  // (a) ...and at that moment the print container held one canvas PER PAGE.
  const printedCounts = await page.evaluate(() => window.__printPageCounts);
  expect(printedCounts).toEqual([pageCount]);

  // (c) After printing, the container and its stylesheet are gone, the viewport
  // is intact, and the live virtualized page canvases are back.
  await expect(page.locator("#printContainer")).toHaveCount(0);
  await expect(page.locator("#printStyle")).toHaveCount(0);
  await expect(page.locator("#viewport")).toBeVisible();
  expect(await page.locator(".page-wrap").count()).toBe(pageCount);
  expect(await page.locator(".page-wrap .page").count()).toBeGreaterThan(0);

  // Memory-budget invariant: printing must not leave every page's canvas alive.
  // Only a viewport-bounded handful of live rasters remain (never one per page),
  // and no print-page canvases linger anywhere.
  expect(await page.locator(".page-wrap .page").count()).toBeLessThan(pageCount);
  expect(await page.locator(".print-page").count()).toBe(0);

  expect(consoleErrors).toEqual([]);
});

test("Print is reachable from the command palette with its ⌘P hint", async ({
  page,
  consoleErrors,
}) => {
  await stubPrint(page);
  await gotoSampleEditor(page);

  await page.keyboard.press(`${MOD}+Shift+p`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  await page.locator("#cmdInput").fill("Print");

  const item = page.locator(".cmd-item", { hasText: "Print" }).first();
  await expect(item).toBeVisible();
  await expect(item.locator(".cmd-hint")).toHaveText("⌘P");
  await item.click();

  const pageCount = await page.locator(".page-wrap").count();
  const calls = await page.evaluate(() => window.__printCalls);
  const printedCounts = await page.evaluate(() => window.__printPageCounts);
  expect(calls).toBe(1);
  expect(printedCounts).toEqual([pageCount]);
  await expect(page.locator("#printContainer")).toHaveCount(0);

  expect(consoleErrors).toEqual([]);
});

test("Print is offered in the File menu", async ({ page, consoleErrors }) => {
  await stubPrint(page);
  await gotoSampleEditor(page);

  await page.locator('.app-menu-button[data-menu="file"]').click();
  const item = page.locator('#appMenuPopover .app-menu-item[data-command="file.print"]');
  await expect(item).toBeVisible();
  await item.click();

  const pageCount = await page.locator(".page-wrap").count();
  const printedCounts = await page.evaluate(() => window.__printPageCounts);
  expect(printedCounts).toEqual([pageCount]);

  expect(consoleErrors).toEqual([]);
});
