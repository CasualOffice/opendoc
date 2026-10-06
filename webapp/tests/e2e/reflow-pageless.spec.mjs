// PAGELESS, AS GOOGLE DOCS DOES IT — asserted on the glass (`docs/151` §4.4b,
// §6.2a, §6.3d).
//
// The owner checked our reflow against Google Docs' pageless view and named four
// things, and each test below is one of them, created by the test rather than
// inherited from a fixture that might happen to satisfy it:
//
//   1. ONE CONTINUOUS SURFACE — in `reflow-seams.spec.mjs`, because it needs a
//      real 2x screen and that is a launch flag, which takes a file of its own.
//   2. IMAGES FIT THE COLUMN — "images will adjust to your screen size"
//      (Google, answer 11528737). Asserted from the tile's own raster: both of
//      the picture's coloured edges are painted, inside the tile.
//   3. WIDE TABLES SCROLL SIDEWAYS — "you can create wide tables and view them by
//      scrolling left and right" (same page). Asserted as the guarantee: the
//      table keeps its declared width, the page does not scroll sideways, and
//      after scrolling the table its LAST column is on screen and a click there
//      lands in it — typed text reaches that cell in the model.
//   4. THE COLUMN IS WIDER BY DEFAULT, and the reader can choose. At 1440px the
//      default was a 469px column, narrower than the same document on paper.
import { test, expect, gotoEditor, mirrorBlocks, runPaletteCommand } from "./fixtures.mjs";
import {
  IMAGE_LEFT_RGB,
  IMAGE_RIGHT_RGB,
  WIDE_TABLE,
  decodePng,
  lastColumnText,
  openInReflow,
  wideContentDocx,
} from "./wide-content-docx.mjs";

const DESKTOP = { width: 1280, height: 800 };
const WIDE = { width: 1440, height: 900 };

// ---- 2. Images fit -----------------------------------------------------------

test("a picture wider than the column is scaled into it, both edges painted", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize(DESKTOP);
  await openInReflow(page, wideContentDocx());
  const found = await page.evaluate(
    ([left, right]) => {
      const wrap = document.querySelector(".page-band > .page-wrap");
      const canvas = wrap?.querySelector("canvas.page");
      if (!canvas) return null;
      const ctx = canvas.getContext("2d");
      const data = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
      const near = (i, rgb) =>
        Math.abs(data[i] - rgb[0]) < 24 &&
        Math.abs(data[i + 1] - rgb[1]) < 24 &&
        Math.abs(data[i + 2] - rgb[2]) < 24;
      let minLeft = Infinity;
      let maxRight = -1;
      for (let y = 0; y < canvas.height; y += 2) {
        for (let x = 0; x < canvas.width; x += 1) {
          const i = (y * canvas.width + x) * 4;
          if (near(i, left)) minLeft = Math.min(minLeft, x);
          if (near(i, right)) maxRight = Math.max(maxRight, x);
        }
      }
      return { minLeft, maxRight, width: canvas.width, ratio: canvas.width / wrap.clientWidth };
    },
    [IMAGE_LEFT_RGB, IMAGE_RIGHT_RGB],
  );
  expect(found, "the first tile has a raster").not.toBeNull();
  expect(found.maxRight, "the picture's RIGHT edge was not painted at all — it was cut off")
    .toBeGreaterThan(0);
  expect(Number.isFinite(found.minLeft), "the picture's left edge was not painted").toBe(true);
  // Inside the column: the 16px gutters stay clear on both sides.
  const gutter = 16 * found.ratio;
  expect(found.minLeft, "the picture starts in the left gutter").toBeGreaterThanOrEqual(gutter - 2);
  expect(found.maxRight, "the picture runs into the right gutter").toBeLessThanOrEqual(
    found.width - gutter + 2,
  );
  expect(consoleErrors).toEqual([]);
});

// ---- 3. Wide tables scroll sideways ----------------------------------------

/** The strip over the wide table, and the page's own horizontal overflow. */
async function stripState(page) {
  return page.evaluate(() => {
    const viewport = document.getElementById("viewport");
    const strip = document.querySelector(".page-wrap > .reflow-table-scroll");
    const port = strip?.querySelector(".reflow-table-scroll__port");
    const canvas = port?.querySelector("canvas");
    const box = strip?.getBoundingClientRect();
    return {
      present: !!strip,
      scrollLeft: port?.scrollLeft ?? 0,
      scrollWidth: port?.scrollWidth ?? 0,
      clientWidth: port?.clientWidth ?? 0,
      canvasCssWidth: canvas ? canvas.getBoundingClientRect().width : 0,
      box: box ? { left: box.left, top: box.top, width: box.width, height: box.height } : null,
      pageScrollWidth: viewport.scrollWidth,
      pageClientWidth: viewport.clientWidth,
      thumb: strip?.querySelector(".reflow-table-scroll__thumb")?.getBoundingClientRect().width ?? 0,
    };
  });
}

test("a wide table keeps its widths, scrolls sideways, and its last column takes a click", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize(DESKTOP);
  await openInReflow(page, wideContentDocx());
  await expect.poll(async () => (await stripState(page)).present, { timeout: 15_000 }).toBe(true);
  const before = await stripState(page);

  // The table is NOT narrowed: its strip is the declared 12in plus the gutters.
  const declaredPx = (WIDE_TABLE.cols * WIDE_TABLE.colTwip * 96) / 1440;
  expect(
    before.canvasCssWidth,
    `the table strip is ${before.canvasCssWidth}px; the table declares ${declaredPx}px`,
  ).toBeGreaterThanOrEqual(declaredPx);
  expect(before.scrollWidth, "the table overflows its scroller").toBeGreaterThan(
    before.clientWidth + 100,
  );
  // The overflow is the TABLE's, never the page's (A-5).
  expect(before.pageScrollWidth).toBeLessThanOrEqual(before.pageClientWidth);
  // A-3: the scrollbar is there without hovering.
  expect(before.thumb, "the table's scrollbar is visible").toBeGreaterThan(20);

  // Scroll the table all the way, the way a reader's trackpad does.
  await page.mouse.move(before.box.left + before.box.width / 2, before.box.top + 10);
  for (let i = 0; i < 30; i += 1) await page.mouse.wheel(400, 0);
  await expect
    .poll(async () => {
      const s = await stripState(page);
      return s.scrollLeft + s.clientWidth >= s.scrollWidth - 1;
    })
    .toBe(true);
  const after = await stripState(page);
  expect(after.pageScrollWidth, "scrolling a table scrolled the page sideways").toBeLessThanOrEqual(
    after.pageClientWidth,
  );

  // The last column is ON SCREEN: its right border is painted inside the strip.
  const png = await page.screenshot({
    clip: { x: after.box.left, y: after.box.top, width: after.box.width, height: after.box.height },
  });
  const shot = decodePng(png);
  const darkColumns = [];
  for (let x = 0; x < shot.width; x += 1) {
    let n = 0;
    for (let y = 0; y < shot.height; y += 1) n += shot.rgba[(y * shot.width + x) * 4] < 128 ? 1 : 0;
    if (n > shot.height * 0.6) darkColumns.push(x);
  }
  const lastBorder = Math.max(...darkColumns);
  expect(
    lastBorder,
    "the table's right edge is not on screen after scrolling to the end",
  ).toBeGreaterThan(shot.width * 0.5);

  // And a click there lands in the LAST column: type, and the model's last cell
  // of that row has the text. The engine moved the rows by the same offset the
  // strip scrolled, which is what this proves — a strip that scrolled pixels the
  // engine did not know about would put the text in a middle column.
  const rowY = after.box.top + after.box.height * (2.5 / 6);
  const clickX = after.box.left + after.box.width * (lastBorder / shot.width) - 40;
  await page.mouse.click(clickX, rowY);
  await page.keyboard.insertText("QQ");
  await expect
    .poll(async () => (await mirrorBlocks(page)).join(" | "), { timeout: 15_000 })
    .toMatch(/QQ/);
  const cells = await page.evaluate(() =>
    [...document.querySelectorAll("#a11yDocument td, #a11yDocument [role='cell']")].map(
      (cell) => cell.textContent ?? "",
    ),
  );
  const typedInto = cells.filter((text) => text.includes("QQ"));
  expect(typedInto.length, `exactly one cell took the text: ${typedInto}`).toBe(1);
  expect(
    typedInto[0],
    "the click on the scrolled-in last column put the caret in another column",
  ).toContain("LASTCOL");
  expect(lastColumnText(WIDE_TABLE.label, 2)).toContain("LASTCOL");
  expect(consoleErrors).toEqual([]);
});

// ---- 4. The column is wider by default, and the reader chooses ----------------

/** The tile's painted width and the paper text column's, in CSS px. */
async function columnWidth(page) {
  return page.evaluate(() => {
    const sheet = document.querySelector(".page-band > .page-wrap");
    return sheet ? sheet.getBoundingClientRect().width - 32 : 0;
  });
}

test("by default the pageless column is the page's width, not narrower than paper", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize(WIDE);
  await gotoEditor(page);
  // The paper's own text column, measured from the sheet: 6.5in on Letter.
  const paperColumn = 624;
  await page.locator('.ribbon-tab[data-tab="view"]').click();
  await page.locator("#viewReflowBtn").click();
  await expect.poll(() => columnWidth(page)).toBeGreaterThan(0);
  const column = await columnWidth(page);
  expect(
    column,
    `the default pageless column is ${column}px — narrower than the ${paperColumn}px the ` +
      "same document shows on paper, which is the owner's 'width of page is too small'",
  ).toBeGreaterThan(paperColumn);
  await expect(page.locator("#viewTextWidthBtn [data-width-label]")).toHaveText(/wide/i);

  // The reader chooses, from the palette as well as the ribbon (≥2 surfaces), and
  // the choice moves the measure: Reading is WCAG's 80 characters.
  await runPaletteCommand(page, "view.textWidth.reading", "text width");
  await expect.poll(() => columnWidth(page)).toBeLessThan(column - 100);
  await runPaletteCommand(page, "view.textWidth.wide", "text width");
  await expect.poll(() => columnWidth(page)).toBe(column);
  // Persisted per viewer: a reload keeps the choice.
  await runPaletteCommand(page, "view.textWidth.reading", "text width");
  const reading = await columnWidth(page);
  await page.reload();
  await page.waitForFunction(() => document.querySelectorAll(".page-wrap").length > 0, null, {
    timeout: 45_000,
  });
  await expect.poll(() => columnWidth(page), { timeout: 30_000 }).toBe(reading);
  expect(consoleErrors).toEqual([]);
});
