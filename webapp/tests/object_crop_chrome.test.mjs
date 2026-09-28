// The crop clamping rule, driven with plain numbers.
//
// The guarantee is that a crop drag can never destroy the picture: no edge may
// pass the image bound, and no pair of opposite edges may leave less than
// MIN_CROP_KEEP of an axis between them — however far the pointer goes. Before
// this lived in a module the only way to ask that was a browser and a picture.
import assert from "node:assert/strict";
import test from "node:test";

import { MIN_CROP_KEEP, cropFromDrag, keptRect } from "../src/object_crop_chrome.mjs";

const NONE = { l: 0, t: 0, r: 0, b: 0 };

test("an east grip moves only the right edge", () => {
  const crop = cropFromDrag(NONE, 3, -0.25, 0.4);
  assert.equal(crop.r, 0.25);
  assert.equal(crop.l, 0);
  assert.equal(crop.t, 0, "a horizontal grip must not touch a vertical edge");
  assert.equal(crop.b, 0);
});

test("a NW grip moves the left and top edges together", () => {
  const crop = cropFromDrag(NONE, 0, 0.2, 0.3);
  assert.equal(crop.l, 0.2);
  assert.equal(crop.t, 0.3);
  assert.equal(crop.r, 0);
  assert.equal(crop.b, 0);
});

test("no drag can push an edge past the image bound", () => {
  const crop = cropFromDrag(NONE, 0, -50, -50);
  assert.equal(crop.l, 0);
  assert.equal(crop.t, 0);
});

test("opposite edges always leave MIN_CROP_KEEP of the axis between them", () => {
  // Drag the left grip all the way across an image already cropped 30% on the
  // right: the left inset must stop at 1 - 0.30 - MIN_CROP_KEEP.
  const start = { l: 0, t: 0, r: 0.3, b: 0 };
  const crop = cropFromDrag(start, 0, 5, 0);
  assert.equal(crop.l, 1 - 0.3 - MIN_CROP_KEEP);

  const kept = keptRect([0, 0, 1000, 1000], crop);
  assert.ok(
    kept.w >= 1000 * MIN_CROP_KEEP - 1e-9,
    `the kept width collapsed to ${kept.w}, below the ${MIN_CROP_KEEP} floor`,
  );
});

test("the kept rectangle is the box minus the cropped edges", () => {
  const kept = keptRect([100, 200, 1000, 500], { l: 0.1, t: 0.2, r: 0.3, b: 0.4 });
  // Rounded to a whole twip: these are fractions of a source image, so the
  // arithmetic is binary floating point and 600.0000000000001 is the honest
  // answer. A twip is 1/1440in — nothing on screen can tell them apart.
  const round = (value) => Math.round(value);
  assert.deepEqual(
    { x: round(kept.x), y: round(kept.y), w: round(kept.w), h: round(kept.h) },
    { x: 200, y: 300, w: 600, h: 200 },
  );
});

test("a corner drag lets one edge reach its limit while the other keeps moving", () => {
  // SE grip on an image already cropped 80% from the left: the right edge is at
  // its limit immediately, but the bottom edge must still follow the pointer.
  const start = { l: 1 - MIN_CROP_KEEP, t: 0, r: 0, b: 0 };
  const crop = cropFromDrag(start, 4, -0.5, -0.25);
  assert.ok(
    Math.abs(crop.r) < 1e-9,
    `the right edge could not move without collapsing the axis, but it moved to ${crop.r}`,
  );
  assert.equal(crop.b, 0.25, "the bottom edge still followed the pointer");
});
