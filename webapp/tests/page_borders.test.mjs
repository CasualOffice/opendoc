// What the Page borders dialog writes (HF-193).
//
// The dialog's fields map onto the model's `PageBorders` shape, which the
// facade deserialises with `deny_unknown_fields`: a misspelt key is a refused
// edit, so the mapping is pinned here rather than discovered in a browser.
import { test } from "node:test";
import assert from "node:assert/strict";

import { BORDER_WIDTHS, bordersFromForm, hexOf, rgbOf } from "../src/page_borders.mjs";

const form = (over = {}) => ({
  edges: ["top", "bottom", "start", "end"],
  style: "double",
  sizeEighthPoints: 12,
  color: "#3355c4",
  spacePoints: 24,
  display: "allPages",
  offsetFrom: "page",
  ...over,
});

test("a box writes the same edge on all four sides, in the model's own field names", () => {
  const borders = bordersFromForm(form());
  assert.deepEqual(Object.keys(borders).sort(), ["bottom", "display", "end", "offsetFrom", "start", "top"]);
  for (const side of ["top", "bottom", "start", "end"]) {
    assert.deepEqual(borders[side], {
      style: "double",
      sizeEighthPoints: 12,
      color: { r: 0x33, g: 0x55, b: 0xc4 },
      spacePoints: 24,
    });
  }
  assert.equal(borders.display, "allPages");
  assert.equal(borders.offsetFrom, "page");
});

test("an unchecked edge is absent, and no edge at all is the empty value that removes them", () => {
  const two = bordersFromForm(form({ edges: ["top", "bottom"] }));
  assert.equal(two.start, undefined);
  assert.equal(two.end, undefined);
  assert.ok(two.top && two.bottom);
  assert.deepEqual(bordersFromForm(form({ edges: [] })), {});
});

// The dialog refuses an out-of-range spacing at the field (`reportValidity`);
// this is the second line, so a value that slipped past it still validates.
test("spacing is held to the model's 0..31 points as a second line of defence", () => {
  assert.equal(bordersFromForm(form({ spacePoints: 99 })).top.spacePoints, 31);
  assert.equal(bordersFromForm(form({ spacePoints: -4 })).top.spacePoints, 0);
  assert.equal(bordersFromForm(form({ spacePoints: "7.6" })).top.spacePoints, 8);
});

test("every offered width fits `w:sz`, and colours convert both ways", () => {
  assert.ok(BORDER_WIDTHS.every((w) => Number.isInteger(w) && w > 0 && w <= 1024));
  assert.equal(hexOf(rgbOf("#0a0B0c")), "#0a0b0c");
  assert.equal(hexOf(null), "#000000");
  assert.deepEqual(rgbOf("not a colour"), { r: 0, g: 0, b: 0 });
});
