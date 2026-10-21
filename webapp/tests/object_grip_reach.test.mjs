// How far a side grip's target reaches (`object_guides.mjs` sideGripReach,
// `docs/109` OL-05): far enough that a press a few pixels off a large picture's
// side grip resizes it, and never so far on a small one that a side band covers
// another grip's centre or the picture's middle.
import assert from "node:assert/strict";
import test from "node:test";

const { SIDE_GRIP_ALONG, SIDE_GRIP_INWARD, sideGripReach } = await import("../src/object_guides.mjs");

/** The eight grips of an upright box, as the engine's flat quads, at scale 1. */
function gripsOf(x, y, w, h) {
  const points = [
    [x, y], [x + w / 2, y], [x + w, y], [x + w, y + h / 2],
    [x + w, y + h], [x + w / 2, y + h], [x, y + h], [x, y + h / 2],
  ];
  return points.flatMap(([cx, cy], kind) => [1, cx, cy, kind]);
}
const pageOf = () => ({ scale: { sx: 1, sy: 1 } });

test("a large picture's side grips reach their full band and inward reach", () => {
  const reach = sideGripReach(gripsOf(100, 100, 132, 132), pageOf);
  for (const side of [1, 3, 5, 7]) {
    assert.equal(reach.along[side], SIDE_GRIP_ALONG, `side ${side} along`);
    assert.equal(reach.inward[side], SIDE_GRIP_INWARD, `side ${side} inward`);
  }
});

test("a 16px picture's side grips do not grow at all, so no band covers another grip or the middle", () => {
  const reach = sideGripReach(gripsOf(100, 100, 16, 16), pageOf);
  for (const side of [1, 3, 5, 7]) {
    assert.equal(reach.along[side], 0, `side ${side} along`);
    assert.equal(reach.inward[side], 0, `side ${side} inward`);
  }
});

test("no side band ever reaches a neighbouring corner's centre or half-way to the middle", () => {
  for (const size of [12, 20, 28, 40, 64, 200]) {
    const reach = sideGripReach(gripsOf(0, 0, size, size), pageOf);
    for (const side of [1, 3, 5, 7]) {
      // Half the grip (4.5px) plus the band must stop short of the corner (size/2 away).
      assert.ok(4.5 + reach.along[side] < size / 2, `size ${size} side ${side} along ${reach.along[side]}`);
      // And the inward reach must leave the inner half of the object to the move.
      assert.ok(4.5 + reach.inward[side] <= size / 4 || reach.inward[side] === 0, `size ${size} side ${side} inward`);
    }
  }
});

test("a grip set that is not the full eight gets no measured reach", () => {
  assert.equal(sideGripReach(gripsOf(0, 0, 100, 100).slice(0, 12), pageOf), null);
});
