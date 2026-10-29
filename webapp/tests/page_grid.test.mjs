// Multiple pages (`docs/153` shell.multiple-pages-view): the band model with
// pages set side by side. Every vertical question the editor asks of the band
// must keep its answer; the one new answer is where a page sits across.
import assert from "node:assert/strict";
import test from "node:test";

import {
  PAGE_GAP_PX,
  buildPageBand,
  pageBandLeft,
  pageBandPitch,
  pageClientRect,
  pageGridOn,
  pageIndexAtDocY,
  pageRangeAt,
  setPageGrid,
} from "../src/page_scroll.mjs";

const LETTER = { widthTwip: 12240, heightTwip: 15840 };
const CSS_PER_TWIP = 96 / 1440 / 2; // 50% zoom: a Letter page is 408 x 528 px

function band(count, rowWidth) {
  return buildPageBand(Array.from({ length: count }, () => LETTER), CSS_PER_TWIP, {
    gap: PAGE_GAP_PX,
    rowWidth,
  });
}

test("pages fill a row as wide as the viewport allows, in reading order", () => {
  // Three 408px pages and two gaps need 1268px; 1300px fits three.
  const grid = band(7, 1300);
  assert.equal(grid.columns, 3);
  assert.deepEqual([...grid.tops].slice(0, 4), [0, 0, 0, 528 + PAGE_GAP_PX]);
  assert.deepEqual(
    [0, 1, 2].map((i) => pageBandLeft(grid, i)),
    [0, 408 + PAGE_GAP_PX, 2 * (408 + PAGE_GAP_PX)],
  );
  // The short last row is centred under the full ones.
  const last = pageBandLeft(grid, 6);
  assert.equal(last, Math.round((grid.width - 408) / 2));
  assert.equal(grid.docHeight, 3 * 528 + 2 * PAGE_GAP_PX);
});

test("a viewport too narrow for two is one column, exactly as before", () => {
  const narrow = band(3, 500);
  const single = buildPageBand([LETTER, LETTER, LETTER], CSS_PER_TWIP, { gap: PAGE_GAP_PX });
  assert.deepEqual([...narrow.tops], [...single.tops]);
  assert.equal(narrow.docHeight, single.docHeight);
  assert.equal(pageBandLeft(single, 1), 0, "a single column centres in its own width");
});

test("a y inside a row resolves to the row's FIRST page, so the window holds the whole row", () => {
  const grid = band(6, 1300);
  assert.equal(pageIndexAtDocY(grid, 10), 0);
  assert.equal(pageIndexAtDocY(grid, 528 + PAGE_GAP_PX + 10), 3);
  assert.deepEqual(pageRangeAt(grid, 10, 20), { first: 0, last: 2 });
  assert.deepEqual(pageRangeAt(grid, 10, 528 + PAGE_GAP_PX + 1), { first: 0, last: 5 });
});

test("a page's client rect is its grid cell", () => {
  const grid = band(4, 1300);
  const rect = pageClientRect({ left: 100, top: 50 }, grid, 1, 0);
  assert.equal(rect.left, 100 + 408 + PAGE_GAP_PX);
  assert.equal(rect.top, 50);
  assert.equal(rect.width, 408);
});

test("the grid is a paper view: reflow never gets it, and it switches off", () => {
  setPageGrid({ on: true, availableWidth: 1300 });
  try {
    assert.equal(pageGridOn(), true);
    assert.equal(pageBandPitch(false).rowWidth, 1300);
    assert.equal(pageBandPitch(true, 2).rowWidth, undefined, "reflow has no pages to set side by side");
  } finally {
    setPageGrid({ on: false });
  }
  assert.equal(pageBandPitch(false).rowWidth, undefined);
});
