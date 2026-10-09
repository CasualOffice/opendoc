// ONE CONTINUOUS SURFACE — the first of the four things the owner named when he
// compared reflow with Google Docs' pageless view (`reflow-pageless.spec.mjs`
// has the other three; `docs/151` §4.4b has the design).
//
// Docs renders pageless as tiles that read as one column. Ours abutted in CSS px
// and still drew a hairline at every cut: at a 2x backing store a table border
// crossing a cut read dark, dark, LIGHT, dark — the raster's padding row
// squeezed onto the screen. Asserted from a SCREENSHOT across real cuts, where
// a reader would see it.
//
// On a REAL 2x screen, which is why this is a file of its own. Playwright's
// `deviceScaleFactor` emulates the ratio through DevTools, and in this Chromium
// the emulation lays out and snaps every box to whole CSS px — measured: a
// canvas whose box starts at 513.5px paints from device row 1028, not 1027, and
// a 0.5px-tall div paints two device rows. A tile whose top falls on a half CSS
// px is painted one device row low and can uncover the partial row of the tile
// above (a probe of a long table at 1440px read dark, dark, LIGHT, dark again at
// one cut of three), so under emulation this test would pass or fail on where
// the cuts happen to fall: a seam of the emulator, not of the page.
// `--force-device-scale-factor` is how Chromium models a HiDPI display — boxes
// snap to DEVICE pixels, as on a Retina laptop — and there the same probe finds
// every tile painted on the row the band put it on. A launch flag forces a new
// worker, which Playwright allows only at the top of a file. The page is opened
// here with `viewport: null`, not through the `page` fixture: the project's
// `Desktop Chrome` descriptor would emulate a 1x window over the flag. The
// window is sized by the flag beside it.
import { test, expect } from "./fixtures.mjs";
import { decodePng, longTableDocx, openInReflow } from "./wide-content-docx.mjs";

const WIDE = { width: 1440, height: 900 };

test.use({
  launchOptions: {
    args: [
      // The suite's own flags (playwright.config.mjs), which this replaces.
      "--enable-precise-memory-info",
      "--js-flags=--expose-gc",
      "--force-device-scale-factor=2",
      `--window-size=${WIDE.width},${WIDE.height}`,
    ],
  },
});

test("a table border runs through every tile cut without a break", async ({
  browser,
  baseURL,
}) => {
  // `deviceScaleFactor: undefined` clears the descriptor's ratio of 1, which
  // the runner merges into every new context and Playwright refuses beside a
  // null viewport.
  const context = await browser.newContext({ viewport: null, deviceScaleFactor: undefined, baseURL });
  const page = await context.newPage();
  // `consoleErrors`, for a page the fixture did not open.
  const consoleErrors = [];
  page.on("console", (msg) => {
    if (msg.type() === "error") consoleErrors.push(msg.text());
  });
  page.on("pageerror", (err) => consoleErrors.push(String(err)));
  expect(
    await page.evaluate(() => [window.innerWidth, window.devicePixelRatio]),
    "the window must be the 2x, 1440px one this test is about",
  ).toEqual([WIDE.width, 2]);
  // 330 rows: six tiles at 1440px, so five cuts — both parities of tile top.
  await openInReflow(page, longTableDocx(330));

  // Every tile is drawn 1:1: its box in device pixels IS its raster. A tile
  // squeezed into a box one row short is how the padding row reached the screen.
  const boxes = await page.evaluate(() =>
    [...document.querySelectorAll(".page-band > .page-wrap")].map((wrap) => {
      const canvas = wrap.querySelector("canvas.page");
      const box = canvas?.getBoundingClientRect();
      return canvas
        ? {
            w: canvas.width,
            h: canvas.height,
            cssW: box.width,
            cssH: box.height,
            top: parseFloat(wrap.style.top),
            boxH: parseFloat(wrap.style.height),
          }
        : null;
    }),
  );
  const placed = boxes.filter(Boolean);
  for (const box of placed) {
    expect(Math.abs(box.cssW * 2 - box.w), `a tile resampled across: ${JSON.stringify(box)}`)
      .toBeLessThan(0.05);
    expect(Math.abs(box.cssH * 2 - box.h), `a tile resampled down: ${JSON.stringify(box)}`)
      .toBeLessThan(0.05);
    // On the raster's grid: a tile that starts between two device pixels has
    // a partly-covered row at its edge, which is the seam in a different place.
    expect(
      Math.abs(box.top * 2 - Math.round(box.top * 2)),
      `a tile starts between two device pixels: ${JSON.stringify(box)}`,
    ).toBeLessThan(0.01);
  }
  // And consecutive tiles overlap by the partial row at most — never a gap.
  for (let i = 1; i < placed.length; i += 1) {
    const overlap = (placed[i - 1].top + placed[i - 1].boxH - placed[i].top) * 2;
    expect(overlap, `tiles ${i} and ${i + 1} leave a gap or overlap past a row`).toBeGreaterThanOrEqual(-0.01);
    expect(overlap).toBeLessThanOrEqual(1.01);
  }

  let checked = 0;
  // Cuts on BOTH parities: a tile whose top is on an odd device row (a half
  // CSS px) is the case a whole-pixel layout gets wrong, so it must be among
  // the cuts looked at, or this proves only the easy half.
  const oddRows = [];
  for (let cut = 1; cut <= 5; cut += 1) {
    // Bring the cut between tile `cut` and tile `cut + 1` to mid-screen.
    const where = await page.evaluate(async (n) => {
      const viewport = document.getElementById("viewport");
      const find = () =>
        document.querySelector(`.page-band > .page-wrap[data-page-number="${n + 1}"]`);
      for (let tries = 0; tries < 40 && !find(); tries += 1) {
        viewport.scrollTop += viewport.clientHeight;
        await new Promise((r) => requestAnimationFrame(r));
      }
      const below = find();
      if (!below) return null;
      const r = below.getBoundingClientRect();
      viewport.scrollTop += r.top - viewport.getBoundingClientRect().top - viewport.clientHeight / 2;
      await new Promise((r2) => requestAnimationFrame(() => requestAnimationFrame(r2)));
      const box = below.getBoundingClientRect();
      return { top: box.top, left: box.left, width: box.width, bandTop: parseFloat(below.style.top) };
    }, cut);
    if (!where) break;
    await expect
      .poll(() =>
        page.evaluate(
          (n) =>
            [n, n + 1].every(
              (k) =>
                document.querySelector(
                  `.page-band > .page-wrap[data-page-number="${k}"] canvas.page`,
                ) !== null,
            ),
          cut,
        ),
      )
      .toBe(true);
    const png = await page.screenshot({
      clip: { x: where.left, y: where.top - 20, width: where.width, height: 40 },
    });
    const { width, height, rgba } = decodePng(png);
    const dark = (x, y) => rgba[(y * width + x) * 4] < 128;
    // The table's vertical borders: columns dark in most rows of the clip.
    const verticals = [];
    for (let x = 0; x < width; x += 1) {
      let n = 0;
      for (let y = 0; y < height; y += 1) n += dark(x, y) ? 1 : 0;
      if (n > height * 0.8) verticals.push(x);
    }
    expect(
      verticals.length,
      `cut ${cut}: the long table's vertical borders must cross the clip, or this checks nothing`,
    ).toBeGreaterThan(3);
    for (const x of verticals) {
      const broken = [];
      for (let y = 0; y < height; y += 1) {
        // A border column may be anti-aliased at its edges; a seam is a row
        // where the WHOLE run of the border is light, so look at the column's
        // darkest neighbour too.
        if (!dark(x, y) && !(x > 0 && dark(x - 1, y)) && !(x + 1 < width && dark(x + 1, y))) {
          broken.push(y);
        }
      }
      expect(
        broken,
        `cut ${cut}: the vertical border at x=${x} is broken at device rows ${broken} ` +
          `of ${height} — a seam across the table where tile ${cut} meets tile ${cut + 1}`,
      ).toEqual([]);
    }
    checked += 1;
    if (Math.round(where.bandTop * 2) % 2 === 1) oddRows.push(cut);
  }
  expect(checked, "the long table must run through five tile cuts").toBe(5);
  expect(oddRows.length, "at least one cut must start its tile on an odd device row").toBeGreaterThan(0);
  expect(consoleErrors).toEqual([]);
  await context.close();
});
