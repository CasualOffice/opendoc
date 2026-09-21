// A page's raster must match the display it is shown on.
//
// The viewer rasterizes each page into a canvas and presents that canvas at the
// logical page size. Two densities are in play: the canvas *backing store* (how
// many real pixels were drawn) and the CSS box (how big it is presented). When
// the backing store is coarser than the display, the browser upscales — and
// upscaling a raster is exactly the "pixelated" that was reported on the
// Medical-form drawings, logos and body text alike.
//
// `MAX_BACKING_DPR` was 1.5, so a Retina display (dpr 2) drew at 1.5x and
// stretched by 1.33x to present. Nothing caught it, because every geometry
// spec asserts the *logical* size — which is dpr-independent by design, and was
// correct the whole time. Sharpness has no logical consequence, so it needs its
// own guard, and it has to run at more than one density: a cap only shows up
// above it.
import { test, expect } from "@playwright/test";

/** The canvas's backing pixels per CSS pixel — the presented sharpness. */
async function backingDensity(page) {
  return page.evaluate(() => {
    const canvas = document.querySelector(".page-wrap canvas");
    const box = canvas.getBoundingClientRect();
    return {
      backing: canvas.width,
      css: Math.round(box.width),
      ratio: Number((canvas.width / box.width).toFixed(2)),
    };
  });
}

// dpr 1 proves the cap does not *inflate* a plain display; dpr 2 is the
// regression itself. A fresh context per density because deviceScaleFactor is
// fixed when the context is created.
for (const dpr of [1, 2]) {
  test(`a page rasterizes at the display's density (dpr ${dpr})`, async ({
    browser,
  }) => {
    const context = await browser.newContext({
      deviceScaleFactor: dpr,
      viewport: { width: 1280, height: 900 },
    });
    const page = await context.newPage();
    try {
      await page.goto("/editor.html?fixture=rich");
      await page.waitForFunction(
        () =>
          document.querySelectorAll(".page-wrap canvas").length > 0 &&
          document.body.dataset.fontsReady === "true",
        null,
        { timeout: 45_000 },
      );

      const { backing, css, ratio } = await backingDensity(page);
      // Allowing 0.05 of slack: the backing store is a rounded pixel count of a
      // fractional logical width, so the quotient lands just under the integer.
      expect(
        ratio,
        `the backing store must match the display at dpr ${dpr}, not be upscaled ` +
          `(${backing}px drawn for a ${css}px box)`,
      ).toBeGreaterThanOrEqual(Math.min(dpr, 2) - 0.05);
    } finally {
      await context.close();
    }
  });
}
