// HF-088 — the comment column at narrow widths.
//
// The card arithmetic for both shapes (anchored margin column, narrow-width
// bottom sheet), plus the one fact that has to be true in two files at once:
// the width where the shape changes. `style.css` owns the box and main.js owns
// the layout, so if those two numbers ever disagree the sheet's CSS applies to
// a margin layout — a column's worth of absolutely-positioned cards inside a
// 50vh scroller, each one parked at its document Y.
//
// Whether the sheet actually keeps the document usable at 390px is not
// arithmetic and is not asserted here: `tests/e2e/narrow-review-column.spec.mjs`
// resizes a real browser and hit-tests the page.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import {
  REVIEW_CARD_GAP,
  reviewStackHeight,
  stackReviewCards,
} from "../src/review_layout.mjs";

const read = (name) => readFileSync(new URL(`../src/${name}`, import.meta.url), "utf8");

test("margin cards ride their own anchors when nothing overlaps", () => {
  const tops = stackReviewCards({
    anchorY: [100, 400, 900],
    heights: [60, 60, 60],
  });
  assert.deepEqual(tops, [100, 400, 900]);
});

test("margin cards destack instead of overlapping", () => {
  const tops = stackReviewCards({ anchorY: [100, 110, 120], heights: [60, 60, 60] });
  assert.deepEqual(tops, [100, 168, 236]);
});

test("the selected margin card keeps its own anchor and the rest move", () => {
  const tops = stackReviewCards({
    anchorY: [400, 410, 420],
    heights: [60, 60, 60],
    activeIndex: 2,
  });
  assert.equal(tops[2], 420, "the selected card stays locked to its marker");
  assert.ok(tops[1] + 60 <= tops[2], "the card above is pushed up clear of it");
  assert.ok(tops[0] + 60 <= tops[1]);
});

test("a crowded cluster at the document top shifts down rather than off-screen", () => {
  // The one case where the selected card gives up its exact anchor: the cards
  // above it are collectively taller than the space above its marker.
  const tops = stackReviewCards({
    anchorY: [100, 110, 120],
    heights: [60, 60, 60],
    activeIndex: 2,
  });
  assert.deepEqual(tops, [8, 76, 144]);
  assert.ok(tops[0] >= REVIEW_CARD_GAP, "no card is pushed above the top edge");
});

test("sheet cards are a list, not a set of anchors", () => {
  // The whole point: in the sheet an anchor Y is a DOCUMENT coordinate and the
  // scroller is a 50vh box, so honouring it parks a card thousands of pixels
  // out of reach. Cards stack from the top of the sheet in document order.
  const heights = [50, 70, 40];
  const tops = stackReviewCards({
    anchorY: [900, 40, 4000],
    heights,
    activeIndex: 2,
    sheet: true,
  });
  assert.deepEqual(tops, [8, 66, 144]);
  for (let i = 1; i < tops.length; i++) {
    assert.equal(tops[i] - (tops[i - 1] + heights[i - 1]), REVIEW_CARD_GAP);
  }
  assert.equal(reviewStackHeight(tops, heights), 192);
});

test("an empty stack reserves nothing", () => {
  assert.deepEqual(stackReviewCards({ anchorY: [], heights: [] }), []);
  assert.equal(reviewStackHeight([], []), 0);
});

test("the stylesheet and main.js change shape at the same width", () => {
  const main = read("main.js");
  const declared = main.match(/const REVIEW_SHEET_MAX_WIDTH = (\d+);/);
  assert.ok(declared, "main.js must declare REVIEW_SHEET_MAX_WIDTH");
  const width = Number(declared[1]);

  assert.match(
    main,
    new RegExp(`matchMedia\\?\\.\\(\`\\(max-width: \\$\\{REVIEW_SHEET_MAX_WIDTH\\}px\\)\``),
    "the media query main.js listens to must be built from the constant",
  );

  const css = read("style.css");
  const rung = new RegExp(
    `@media \\(max-width: ${width}px\\)\\s*\\{[^}]*\\.viewport\\.has-review-sidebar \\.pages`,
  );
  assert.match(
    css,
    rung,
    `style.css must drop the comment gutter at the same ${width}px the sheet ` +
      "takes over, or the page keeps a reserved margin with nothing in it",
  );
  assert.match(
    css,
    /\.viewport\.review-sheet\.has-review-sidebar \.review-sidebar \{[^}]*position: fixed/,
    "the sheet rules must be more specific than `.viewport.has-review-sidebar " +
      ".review-sidebar`, which is the tie the 860px rung lost",
  );
});
