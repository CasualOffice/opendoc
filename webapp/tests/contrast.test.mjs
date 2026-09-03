import test from "node:test";
import assert from "node:assert/strict";
import {
  contrastRatio,
  relativeLuminance,
  composite,
  previewInkIsLegible,
  PREVIEW_CONTRAST_FLOOR,
} from "../src/contrast.mjs";

const WHITE = { r: 255, g: 255, b: 255, a: 1 };
const BLACK = { r: 0, g: 0, b: 0, a: 1 };
// The two surfaces a Styles gallery card is actually painted on.
const DARK_SURFACE = { r: 33, g: 36, b: 41, a: 1 };
// Word's built-in Heading 1/2/Title colour, and the reason this module exists.
const WORD_HEADING = { r: 47, g: 84, b: 150, a: 1 };

test("contrast is anchored to the WCAG reference values", () => {
  // Black on white is the definition of 21:1; anything else means the luminance
  // curve is wrong, which every other assertion here would inherit.
  assert.equal(Math.round(contrastRatio(BLACK, WHITE)), 21);
  assert.equal(contrastRatio(WHITE, WHITE), 1);
  assert.equal(relativeLuminance(WHITE), 1);
  assert.equal(relativeLuminance(BLACK), 0);
  // Symmetric: the ratio does not know which colour is the text.
  assert.equal(contrastRatio(BLACK, WHITE), contrastRatio(WHITE, BLACK));
  // #767676 on white is the canonical "exactly passes AA body text" grey.
  const aaGrey = { r: 0x76, g: 0x76, b: 0x76, a: 1 };
  assert.ok(contrastRatio(aaGrey, WHITE) >= 4.5);
  assert.ok(contrastRatio({ r: 0x77, g: 0x77, b: 0x77, a: 1 }, WHITE) < 4.5);
});

test("a translucent ink is judged on what it composites to", () => {
  const halfBlack = { r: 0, g: 0, b: 0, a: 0.5 };
  assert.deepEqual(composite(halfBlack, WHITE), { r: 127.5, g: 127.5, b: 127.5, a: 1 });
  // Fully transparent ink IS the background: 1:1, never legible.
  assert.equal(contrastRatio(composite({ r: 0, g: 0, b: 0, a: 0 }, WHITE), WHITE), 1);
  assert.equal(previewInkIsLegible({ r: 0, g: 0, b: 0, a: 0 }, WHITE), false);
});

test("Word's own heading colour previews on light and falls back on dark", () => {
  // The exact case from the bug report: the label was painted, correctly, in a
  // colour nobody could see.
  assert.ok(contrastRatio(WORD_HEADING, DARK_SURFACE) < PREVIEW_CONTRAST_FLOOR);
  assert.equal(previewInkIsLegible(WORD_HEADING, DARK_SURFACE), false);

  // And it must still preview as authored where it IS readable — a fix that
  // simply stopped previewing colours would pass every other assertion here.
  assert.ok(contrastRatio(WORD_HEADING, WHITE) > 7);
  assert.equal(previewInkIsLegible(WORD_HEADING, WHITE), true);
});

test("the floor is applied, not merely defined", () => {
  // Straddle the boundary: a colour just under the floor is refused and one just
  // over it is accepted, so the comparison cannot be inverted or short-circuited.
  const under = { r: 0x59, g: 0x59, b: 0x59, a: 1 };
  const over = { r: 0x5c, g: 0x5c, b: 0x5c, a: 1 };
  assert.ok(contrastRatio(under, BLACK) < PREVIEW_CONTRAST_FLOOR);
  assert.ok(contrastRatio(over, BLACK) >= PREVIEW_CONTRAST_FLOOR);
  assert.equal(previewInkIsLegible(under, BLACK), false);
  assert.equal(previewInkIsLegible(over, BLACK), true);
});

test("a missing colour is never legible rather than throwing", () => {
  assert.equal(previewInkIsLegible(null, WHITE), false);
  assert.equal(previewInkIsLegible(WORD_HEADING, null), false);
  assert.equal(previewInkIsLegible(undefined, undefined), false);
});
