// The scroll model behind the page band (`docs/113` §8.6).
//
// What these guard is one sentence: **every page of a document of any length
// is reachable, in a scroll container a browser will actually build.** The
// viewer used to give every page a sheet in normal flow, so a 25,556-page
// document had a 27,549,376 px scroll container, a browser stopped scrolling
// at 2^24 = 16,777,216 px, and the last third could not be reached — with no
// error, which is the failure `SKILL.md` §12 forbids.
//
// These are unit tests rather than browser tests because the property is
// arithmetic: the browser half (a real scroller, a real last page, real ink on
// it) is `viewer-scroll-ceiling.spec.mjs`.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  MAX_BACKING_RATIO,
  MAX_SCROLL_PX,
  PAGE_GAP_PX,
  backingRatio,
  buildPageBand,
  rasterExtent,
  docToScroll,
  pageBandPitch,
  pageClientRect,
  pageIndexAtDocY,
  pageRangeAt,
  scrollToDoc,
} from "../src/page_scroll.mjs";

/** The wall a browser stops scrolling at, measured in the ceiling probe. */
const BROWSER_SCROLL_WALL = 16_777_216;

/** US Letter at 100% zoom: 816 × 1056 CSS px. */
const LETTER = { widthTwip: 12_240, heightTwip: 15_840 };
const CSS_PER_TWIP = 96 / 1440;

/** A band of `count` identical Letter pages, built the way `renderAll` does. */
function band(count, cssPerTwip = CSS_PER_TWIP) {
  return buildPageBand(Array.from({ length: count }, () => LETTER), cssPerTwip);
}

test("the scroll container never passes the browser's limit, at any length", () => {
  // Including the owner's own file (25,556 pages) and an order of magnitude
  // past it, and including a short document at maximum zoom — the ceiling is
  // reached from that direction too.
  for (const count of [0, 1, 2, 14, 5_141, 13_726, 25_556, 250_000]) {
    for (const zoom of [0.25, 1, 5]) {
      const b = band(count, (96 * zoom) / 1440);
      assert.ok(
        b.height <= MAX_SCROLL_PX,
        `${count} pages at ${zoom}× built a ${b.height} px container`,
      );
      assert.ok(b.height < BROWSER_SCROLL_WALL, `${count} pages at ${zoom}× passed the wall`);
    }
  }
});

test("the last page's bottom is exactly reachable, at any length", () => {
  for (const count of [1, 2, 14, 5_141, 25_556, 250_000]) {
    const b = band(count);
    for (const viewportHeight of [400, 900, 2_000]) {
      const travel = Math.max(0, b.height - viewportHeight);
      const { docY, offset } = scrollToDoc(b, viewportHeight, travel);
      // At the bottom of the scroll, the document's own bottom is at the
      // bottom of the viewport: not one pixel short, which is what "the last
      // third is unreachable" looked like.
      const expected = Math.max(0, b.docHeight - viewportHeight);
      assert.ok(
        Math.abs(docY - expected) < 1e-6,
        `${count} pages: bottom of scroll showed ${docY}, not ${expected}`,
      );
      // ...and the last page's sheet is inside the band it is placed in.
      const top = offset + b.tops[count - 1];
      assert.ok(top >= -1e-6 && top + b.heights[count - 1] <= b.height + 1e-6,
        `${count} pages: last sheet placed at ${top} in a ${b.height} px band`);
    }
  }
});

test("the first page's top is exactly reachable, at any length", () => {
  for (const count of [1, 14, 25_556]) {
    const b = band(count);
    const { docY, offset } = scrollToDoc(b, 900, 0);
    assert.equal(docY, 0);
    assert.equal(offset + b.tops[0], 0);
  }
});

test("a document under the cap is not remapped at all", () => {
  // The identity is what keeps an ordinary document's geometry byte-identical
  // to what it was before any of this: sheets at fixed positions that do not
  // move when the viewport scrolls.
  const b = band(200);
  assert.equal(b.scale, 1);
  assert.ok(b.docHeight < MAX_SCROLL_PX);
  for (const s of [0, 1, 999, 12_345, b.height - 900]) {
    const { docY, offset } = scrollToDoc(b, 900, s);
    assert.equal(offset, 0, `offset at ${s}`);
    assert.equal(docY, Math.max(0, Math.min(b.height - 900, s)));
  }
});

test("scrolling is monotonic and covers the document once", () => {
  const b = band(25_556);
  const viewportHeight = 900;
  const travel = b.height - viewportHeight;
  let previous = -1;
  for (let i = 0; i <= 1_000; i++) {
    const { docY } = scrollToDoc(b, viewportHeight, (travel * i) / 1_000);
    assert.ok(docY >= previous, `scroll position ${i} went backwards`);
    previous = docY;
  }
});

test("scrolling to a page and reading back where that is agree", () => {
  const b = band(25_556);
  const viewportHeight = 900;
  for (const page of [0, 1, 999, 12_000, 25_554, 25_555]) {
    const want = b.tops[page];
    const { docY } = scrollToDoc(b, viewportHeight, docToScroll(b, viewportHeight, want));
    // Within one page: the round trip is exact in real arithmetic, and the
    // point of the assertion is that a jump to page N lands ON page N.
    assert.equal(
      pageIndexAtDocY(b, docY),
      Math.min(page, pageIndexAtDocY(b, Math.max(0, b.docHeight - viewportHeight))),
      `jumping to page ${page + 1} landed elsewhere`,
    );
  }
});

test("the visible range is the pages a reader can actually see", () => {
  const b = band(25_556);
  // A viewport in the middle of the document, no overscan: exactly the pages
  // it overlaps, and never a page it does not.
  const docY = b.tops[10_000] + 200;
  const { first, last } = pageRangeAt(b, docY, docY + 900);
  assert.equal(first, 10_000);
  assert.ok(last >= 10_000 && last <= 10_001);
  for (let i = first; i <= last; i++) {
    assert.ok(b.tops[i] < docY + 900 && b.tops[i] + b.heights[i] > docY - b.gap);
  }
});

test("the gap belongs between pages, never after the last one", () => {
  const b = band(3);
  assert.equal(b.gap, PAGE_GAP_PX);
  assert.equal(b.tops[1] - (b.tops[0] + b.heights[0]), PAGE_GAP_PX);
  assert.equal(b.docHeight, b.tops[2] + b.heights[2]);
});

test("a mixed-size document keeps every page its own size, and centres each", () => {
  const landscape = { widthTwip: 15_840, heightTwip: 12_240 };
  const b = buildPageBand([LETTER, landscape, LETTER], CSS_PER_TWIP);
  assert.equal(b.width, landscape.widthTwip * CSS_PER_TWIP);
  assert.equal(b.heights[0], LETTER.heightTwip * CSS_PER_TWIP);
  assert.equal(b.heights[1], landscape.heightTwip * CSS_PER_TWIP);
  const bandRect = { left: 100, top: 50 };
  const portrait = pageClientRect(bandRect, b, 0, 0);
  const wide = pageClientRect(bandRect, b, 1, 0);
  assert.equal(wide.left, 100);
  assert.ok(portrait.left > wide.left, "the narrower page is centred in the band");
  assert.equal(portrait.top, 50);
  assert.equal(wide.top, 50 + b.tops[1]);
});

test("an empty document has nothing to scroll and no page to place", () => {
  const b = band(0);
  assert.equal(b.docHeight, 0);
  assert.equal(b.height, 0);
  assert.deepEqual(scrollToDoc(b, 900, 0), { docY: 0, offset: 0 });
  assert.deepEqual(pageRangeAt(b, 0, 900), { first: 0, last: -1 });
});

test("a document shorter than the viewport does not move when scrolled", () => {
  const b = band(1);
  const { docY, offset } = scrollToDoc(b, 5_000, 120);
  assert.equal(docY, 0);
  assert.equal(offset, 0);
});

// ---- The pitch between two page boxes is a property of the VIEW --------------
// It used to be a ternary at the one call site in `main.js`, where no `node` test
// could reach it. The claim is not "0 is smaller":
// a reflow tile is a rasterisation unit cut mid-paragraph at a line boundary, so
// any pitch at all paints a band through the middle of a sentence.

test("the pitch between page boxes is the desk on paper and nothing in reflow", () => {
  assert.deepEqual(pageBandPitch(false), { gap: PAGE_GAP_PX, maxScroll: MAX_SCROLL_PX });
  assert.deepEqual(pageBandPitch(true, 2), { gap: 0, maxScroll: MAX_SCROLL_PX, snap: 2 });

  // And it composes, which is the half a constant comparison cannot see: the
  // band positions sheets from the pitch, so a reflow tile must ABUT the one
  // above it and a paper sheet must not. (Letter is a whole number of pixels at
  // 100%, so here abutting is exact; the fractional case is the next test.)
  const sizes = [LETTER, LETTER, LETTER];
  const reflowed = buildPageBand(sizes, CSS_PER_TWIP, pageBandPitch(true, 1));
  const paper = buildPageBand(sizes, CSS_PER_TWIP, pageBandPitch(false));
  for (let i = 1; i < sizes.length; i += 1) {
    assert.equal(
      reflowed.tops[i] - reflowed.tops[i - 1],
      reflowed.heights[i - 1],
      `tile ${i} does not abut tile ${i - 1}: there is a seam between them`,
    );
    assert.equal(
      paper.tops[i] - paper.tops[i - 1],
      paper.heights[i - 1] + PAGE_GAP_PX,
      `sheet ${i} lost the desk above it`,
    );
  }
  // The scroll bound is NOT a view choice and must survive either answer.
  assert.equal(pageBandPitch(true).maxScroll, pageBandPitch(false).maxScroll);
});

// ---- SEAMLESS TILES (`docs/151` §4.4b) ---------------------------------------
//
// A trimmed reflow tile is a fractional number of pixels tall; the engine
// rasterises the CEILING of it. Measured at 2x before this: a table border that
// crosses a cut read dark, dark, LIGHT, dark — the raster's padding row squeezed
// onto the screen, and every tile resampled into a slot one row short of it.
// The guarantee is stated on the grid, where it is exact:
//
//   1. every tile's box is EXACTLY the engine's raster, in whole backing pixels,
//      so it is blitted 1:1 and never resampled;
//   2. every tile starts on a whole backing pixel;
//   3. consecutive tiles never leave a gap, and overlap by at most the one
//      partial row — so no partly-covered row is ever on screen.
//
// Real trimmed heights from the measured fixture, plus awkward ones, at the
// ratios a browser actually reports.

/** Tile heights in twips that are NOT whole pixels at 100% — the shape a trim
 *  produces. The first three are the heights measured on `wide.docx`'s tiles. */
const TRIMMED = [15_811, 15_832, 15_733, 15_840, 7, 14_401, 9_999];
const TILE_WIDTH = 7_520;

for (const ratio of [1, 1.25, 1.5, 2]) {
  for (const zoom of [1, 1.1, 0.9, 1.5]) {
    test(`reflow tiles sit on the raster grid with no seam at ratio ${ratio}, zoom ${zoom}`, () => {
      const cssPerTwip = (96 * zoom) / 1440;
      const sizes = TRIMMED.map((heightTwip) => ({ widthTwip: TILE_WIDTH, heightTwip }));
      const b = buildPageBand(sizes, cssPerTwip, pageBandPitch(true, ratio));
      // `main.js`'s `currentDpi()`, which the tiles are rendered at.
      const dpi = 96 * zoom * ratio;
      const grid = (px) => Math.round(px * ratio * 1e6) / 1e6;
      for (let i = 0; i < sizes.length; i += 1) {
        const rows = Math.ceil(rasterExtent(sizes[i].heightTwip, dpi));
        const cols = Math.ceil(rasterExtent(sizes[i].widthTwip, dpi));
        assert.equal(grid(b.heights[i]), rows, `tile ${i} box is not its ${rows}-row raster`);
        assert.equal(grid(b.widths[i]), cols, `tile ${i} box is not its ${cols}-column raster`);
        assert.ok(Number.isInteger(grid(b.tops[i])), `tile ${i} starts between two pixels`);
        if (i === 0) continue;
        const bottom = grid(b.tops[i - 1] + b.heights[i - 1]);
        const top = grid(b.tops[i]);
        assert.ok(
          top <= bottom,
          `a ${top - bottom}-pixel gap between tile ${i - 1} and tile ${i}: a seam`,
        );
        // Exactly the partial row is covered: one row when the raster above ends
        // on a fraction of a pixel (its last row is part content, part padding,
        // and must not be seen), none when it ends on a whole one.
        const above = rasterExtent(sizes[i - 1].heightTwip, dpi);
        const partial = Math.ceil(above) > above ? 1 : 0;
        assert.equal(
          bottom - top,
          partial,
          `tile ${i} covers ${bottom - top} rows of tile ${i - 1}; its partial row is ${partial} ` +
            "— a partial row left on screen is the hairline seam",
        );
      }
      assert.equal(
        grid(b.docHeight),
        grid(b.tops[sizes.length - 1] + b.heights[sizes.length - 1]),
        "the document ends where its last tile's box does",
      );
    });
  }
}

test("paper is not snapped: a sheet keeps its exact CSS geometry", () => {
  const sizes = [{ widthTwip: 12_240, heightTwip: 15_811 }];
  const b = buildPageBand(sizes, CSS_PER_TWIP, pageBandPitch(false));
  assert.equal(b.heights[0], 15_811 * CSS_PER_TWIP);
  assert.equal(b.widths[0], 12_240 * CSS_PER_TWIP);
});

test("the raster extent is the engine's f32 arithmetic, not f64's", () => {
  // `Twip::to_device_px` is `(twip as f32) * dpi / 1440.0` in f32. 15,811 twips
  // at 192 dpi is 2108.1333… in f64 and 2108.1333 in f32 — the same ceiling —
  // so the check that matters is that rounding happens in f32 at each step.
  const f = Math.fround;
  assert.equal(rasterExtent(15_811, 192), f(f(f(15_811) * f(192)) / 1440));
  assert.equal(Math.ceil(rasterExtent(15_840, 96)), 1056, "a whole-pixel page stays whole");
});

test("the backing ratio is main.js's: one cap, mirrored, never two", () => {
  // `main.js` exports nothing, so its cap is mirrored here; a band snapped to a
  // grid the rasters are not painted at would be resampled again. Read from the
  // source so the two cannot drift.
  const main = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
  const declared = main.match(/const MAX_BACKING_DPR = (\d+(?:\.\d+)?);/);
  assert.ok(declared, "main.js no longer declares MAX_BACKING_DPR where this reads it");
  assert.equal(Number(declared[1]), MAX_BACKING_RATIO);
  assert.equal(backingRatio({ devicePixelRatio: 3 }), MAX_BACKING_RATIO);
  assert.equal(backingRatio({ devicePixelRatio: 1.25 }), 1.25);
  assert.equal(backingRatio({}), 1);
});
