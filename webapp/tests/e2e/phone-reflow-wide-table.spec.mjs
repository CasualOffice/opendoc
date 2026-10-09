// PAGELESS ON A PHONE — the wide-table scroller and the seamless column where
// the owner's "4 horizonal canvas connected" matters most (`docs/151` §4.4b,
// §6.3d). Runs ONLY in the `phone` project (Pixel 7: `isMobile`, `hasTouch`, a
// 2.625 device ratio and therefore a backing store UPSCALED from 2x — the case
// the desktop guards do not see).
//
// What a phone reader is promised, each asserted rather than inherited:
//   * reflow is on by default and the page does not scroll sideways — even with
//     a 12in table in the document, because the overflow is the TABLE's;
//   * that table keeps its widths and can be scrolled to its last column, and a
//     TAP there puts the caret in the last column (typed text reaches that cell);
//   * the tiles sit on the raster grid, so a cut has no gap row (the glass
//     check is the 2x desktop guard's; see the last test for why not here).
import { test, expect, gotoEditor } from "./fixtures.mjs";
import { longTableDocx, wideContentDocx } from "./wide-content-docx.mjs";

const PHONE = { width: 390, height: 844 };

async function open(page, file) {
  await page.setViewportSize(PHONE);
  await gotoEditor(page);
  await page.locator("#file").setInputFiles(file);
  await expect
    .poll(() => page.evaluate(() => document.body.dataset.fontsReady), { timeout: 45_000 })
    .toBe("true");
  await expect
    .poll(() =>
      page.evaluate(() => document.getElementById("viewport").classList.contains("is-reflow")),
    )
    .toBe(true);
}

test("on a phone a wide table scrolls by itself, the page never does, and a tap reaches its last column", async ({
  page,
  consoleErrors,
}) => {
  await open(page, wideContentDocx());
  // A SECOND document, opened while reflow is on — `gotoEditor` opened the first.
  // It used to be laid out on PAPER under a chrome that said reflow: 816px tiles
  // panning in a 390px window, because the "nothing moved" short-circuit in
  // `reflow_chrome.mjs` compared measures across documents. Its tiles must be
  // the phone's column, not the sheet.
  await expect
    .poll(
      () =>
        page.evaluate(() =>
          Math.round(document.querySelector(".page-band > .page-wrap").getBoundingClientRect().width),
        ),
      { timeout: 15_000 },
    )
    .toBeLessThanOrEqual(PHONE.width);
  await expect.poll(() => page.locator(".reflow-table-scroll").count(), { timeout: 15_000 })
    .toBeGreaterThan(0);
  const state = () =>
    page.evaluate(() => {
      const viewport = document.getElementById("viewport");
      const port = document.querySelector(".reflow-table-scroll__port");
      const box = port.getBoundingClientRect();
      return {
        scrollLeft: port.scrollLeft,
        max: port.scrollWidth - port.clientWidth,
        box: { left: box.left, top: box.top, width: box.width, height: box.height },
        pageScrollWidth: viewport.scrollWidth,
        pageClientWidth: viewport.clientWidth,
      };
    });
  const before = await state();
  expect(before.max, "a 12in table overflows a phone's column").toBeGreaterThan(300);
  expect(before.pageScrollWidth, "the page itself scrolls sideways").toBeLessThanOrEqual(
    before.pageClientWidth,
  );

  // Scroll the table's own scroller to its end — what a horizontal swipe does.
  await page.evaluate(() => {
    const port = document.querySelector(".reflow-table-scroll__port");
    port.scrollLeft = port.scrollWidth;
  });
  await expect.poll(async () => (await state()).scrollLeft).toBeGreaterThan(before.max - 2);
  const after = await state();
  expect(after.pageScrollWidth).toBeLessThanOrEqual(after.pageClientWidth);

  // Tap the last column of the third row, and type.
  await page.touchscreen.tap(
    after.box.left + after.box.width - 60,
    after.box.top + after.box.height * (2.5 / 6),
  );
  await page.keyboard.insertText("TT");
  await expect
    .poll(() =>
      page.evaluate(() =>
        [...document.querySelectorAll("#a11yDocument td, #a11yDocument [role='cell']")]
          .map((cell) => cell.textContent ?? "")
          .filter((text) => text.includes("TT")),
      ),
    )
    .toEqual([expect.stringContaining("LASTCOL")]);
  expect(consoleErrors).toEqual([]);
});

test("on a phone the tiles sit on the raster grid, abutting with no gap", async ({
  page,
  consoleErrors,
}) => {
  // WHY THIS IS A GEOMETRY ASSERTION AND NOT A SCREENSHOT. The 2x desktop guard
  // in `reflow-seams.spec.mjs` reads the seam off the glass. Here the 2.625
  // screen is fed a 2x backing store and upscales it, and that resampling blurs
  // a one-row hairline below any threshold that does not also fire on an
  // ordinary anti-aliased border: a screenshot version of this test was written,
  // stayed GREEN with the whole seam fix reverted, and was deleted (`SKILL.md`
  // §4). What does go red is the state the fix establishes: every tile starts on
  // a whole backing pixel and covers at most the one partial row of the tile
  // above — the arithmetic `page_scroll.test.mjs` proves at ratio 2, checked here
  // in the device class that runs it.
  // 3 x 1500 twips = 312px: narrower than a phone's 352px column, so the tiles
  // themselves carry the table across the cut and no scroller covers it.
  await open(page, longTableDocx(160, 1500));
  await expect
    .poll(() => page.locator(".page-band > .page-wrap").count(), { timeout: 15_000 })
    .toBeGreaterThan(1);
  const tiles = await page.evaluate(() =>
    [...document.querySelectorAll(".page-band > .page-wrap")]
      .map((wrap) => ({ top: parseFloat(wrap.style.top), height: parseFloat(wrap.style.height) }))
      .sort((a, b) => a.top - b.top),
  );
  const backing = 2; // `main.js`'s MAX_BACKING_DPR caps the 2.625 screen at 2
  for (const tile of tiles) {
    expect(
      Math.abs(tile.top * backing - Math.round(tile.top * backing)),
      `a tile starts between two raster pixels: ${JSON.stringify(tile)}`,
    ).toBeLessThan(0.01);
    expect(Math.abs(tile.height * backing - Math.round(tile.height * backing))).toBeLessThan(0.01);
  }
  for (let i = 1; i < tiles.length; i += 1) {
    const overlap = (tiles[i - 1].top + tiles[i - 1].height - tiles[i].top) * backing;
    expect(overlap, `tile ${i + 1} leaves a gap below tile ${i}`).toBeGreaterThanOrEqual(-0.01);
    expect(overlap, `tile ${i + 1} covers more than tile ${i}'s partial row`).toBeLessThanOrEqual(1.01);
  }
  expect(consoleErrors).toEqual([]);
});
