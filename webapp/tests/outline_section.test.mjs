// The Outline's "which section holds the caret" lookup (`docs/109` UX-055,
// audit CRITIC-08) runs on every toolbar pass while the outline is open, and each
// heading position it reads is one engine `caretRect`. Reading all of them per
// keystroke was O(headings) engine calls per keystroke (SKILL §2, §8).
//
// The bound is asserted by DOUBLING, not by a time threshold: the number of
// positions read for 2n headings may exceed the count for n by only a constant,
// which is what a binary search does and a linear scan cannot.
import { test } from "node:test";
import assert from "node:assert/strict";
import { outlineSectionAt, outlineSectionAtBy } from "../src/outline_panel.mjs";

/** `n` headings, two per page, at y 100 and 500 (twips). */
const headingsOf = (n) =>
  Array.from({ length: n }, (_, i) => ({ node: `h${i}`, page: Math.floor(i / 2) + 1, y: i % 2 ? 500 : 100 }));

function reads(n, page, y) {
  const places = headingsOf(n);
  let count = 0;
  const node = outlineSectionAtBy(places.length, (i) => (count++, places[i]), page, y);
  return { node, count };
}

test("the section is the last heading at or before the point", () => {
  const places = headingsOf(8);
  assert.equal(outlineSectionAt(places, 1, 50), null, "before the first heading");
  assert.equal(outlineSectionAt(places, 1, 100), "h0", "on the first heading");
  assert.equal(outlineSectionAt(places, 1, 300), "h0", "under the first heading");
  assert.equal(outlineSectionAt(places, 2, 50), "h1", "top of page 2 is still h1's section");
  assert.equal(outlineSectionAt(places, 3, 600), "h5");
  assert.equal(outlineSectionAt(places, 99, 0), "h7", "past the last heading");
  assert.equal(outlineSectionAt([], 1, 1), null, "no headings");
});

test("a heading with no place on the pages is skipped, not taken as a boundary", () => {
  const places = headingsOf(8);
  const holes = new Set([3, 4]);
  const placeOf = (i) => (holes.has(i) ? null : places[i]);
  assert.equal(outlineSectionAtBy(8, placeOf, 3, 300), "h2", "h3 and h4 are unplaced, so the last PLACED heading before the point wins");
  assert.equal(outlineSectionAtBy(8, placeOf, 3, 600), "h5");
  assert.equal(outlineSectionAtBy(8, () => null, 3, 600), null, "nothing placed");
});

test("the lookup reads O(log headings) positions: doubling the headings adds a constant", () => {
  for (const n of [64, 256, 1024]) {
    // Several points, including the worst cases at both ends.
    for (const [page, y] of [[1, 50], [1, 300], [n / 4, 300], [n / 2 + 1, 50], [n, 900]]) {
      const small = reads(n, page, y);
      const large = reads(2 * n, page, y);
      assert.ok(
        large.count <= small.count + 2,
        `${n} → ${2 * n} headings at page ${page}: ${small.count} → ${large.count} reads (a linear scan doubles)`,
      );
      assert.ok(large.count <= Math.ceil(Math.log2(2 * n)) + 2, `${2 * n} headings: ${large.count} reads`);
    }
  }
});
