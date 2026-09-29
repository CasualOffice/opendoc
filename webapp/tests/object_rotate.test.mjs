// Free rotation: the arithmetic, against ONLYOFFICE's own rule.
//
// These are the assertions that can be made without a browser — the gesture
// itself (grip, cursor, commit) is `object-rotation.spec.mjs`. What is pinned
// here is the part that is easy to get wrong and impossible to eyeball: which
// way a positive angle turns, where the snap lands, and the mapping between the
// page's axes and the object's that makes a turned object resize correctly.
import assert from "node:assert/strict";
import test from "node:test";

const {
  CARDINAL_SNAP_DEGREES,
  ROTATE_HANDLE_KIND,
  ROTATE_GRIP_REACH,
  ROTATE_HANDLE_REACH,
  SHIFT_SNAP_DEGREES,
  boxCentre,
  dragInObjectSpace,
  normaliseDegrees,
  pointerAngle,
  rotatePoint,
  rotatedBounds,
  rotatedResizePlacement,
  snapRotation,
} = await import("../src/object_rotate.mjs");

const close = (actual, expected, tolerance, message) =>
  assert.ok(
    Math.abs(actual - expected) <= tolerance,
    `${message}: ${actual} is not within ${tolerance} of ${expected}`,
  );

test("a positive angle turns CLOCKWISE on the page, which is what a:xfrm@rot means", () => {
  // Page coordinates run y-DOWNWARD. A point directly above the centre must end
  // up directly to its RIGHT after a quarter turn; the opposite sign convention
  // is the single most likely way to get this backwards, and it would show as an
  // object that turns the wrong way under the pointer.
  const turned = rotatePoint({ x: 0, y: -10 }, { x: 0, y: 0 }, 90);
  close(turned.x, 10, 0.001, "the point swung to the right");
  close(turned.y, 0, 0.001, "and onto the centre's own row");
});

test("the pointer angle is measured from the grip's upright bearing", () => {
  const centre = { x: 100, y: 100 };
  // The grip sits straight above the object when it is upright, so a pointer
  // there must read 0 — not 270, which is what a bare screen bearing gives.
  close(pointerAngle(centre, { x: 100, y: 40 }), 0, 0.001, "straight up is zero");
  close(pointerAngle(centre, { x: 160, y: 100 }), 90, 0.001, "to the right is a quarter turn");
  close(pointerAngle(centre, { x: 100, y: 160 }), 180, 0.001, "below is a half turn");
  close(pointerAngle(centre, { x: 40, y: 100 }), 270, 0.001, "to the left is three quarters");
});

test("the quarter turns pull the angle in, at ONLYOFFICE's own tolerance", () => {
  // `MIN_ANGLE` is 0.07 radians in `RotateTracks.js:46` — 4.01 degrees.
  close(CARDINAL_SNAP_DEGREES, 4.01, 0.01, "MIN_ANGLE in degrees");
  assert.equal(snapRotation(2, false), 0, "2 degrees off true straightens");
  assert.equal(snapRotation(358, false), 0, "and so does 2 degrees the other way");
  assert.equal(snapRotation(91, false), 90, "a near-quarter turn snaps to the quarter");
  assert.equal(snapRotation(268, false), 270, "and a near-three-quarter to three quarters");
  assert.equal(snapRotation(30, false), 30, "but 30 degrees is left exactly where it is");
  assert.equal(snapRotation(85, false), 85, "and so is 85, which is outside the tolerance");
});

test("Shift constrains the rotation to 15 degrees", () => {
  assert.equal(SHIFT_SNAP_DEGREES, 15);
  assert.equal(snapRotation(38, true), 45, "38 with Shift lands on the nearest step");
  assert.equal(snapRotation(52, true), 45, "and so does 52, from the other side");
  assert.equal(snapRotation(7, true), 0, "a step can be zero");
  assert.equal(snapRotation(352, true), 345, "352 is nearer 345 than 360, and lands there");
  assert.equal(snapRotation(353, true), 0, "353 rounds up past 360 and WRAPS, rather than authoring 360");
  // ONLYOFFICE FLOORS to the step (`Math.floor(12 * rot / PI)`), so 52 would
  // land on 45 there too but 38 would land on 30 — the object always lagging the
  // pointer. Rounding is Word's and Docs' behaviour and is the stated divergence.
  assert.equal(snapRotation(38, false), 38, "and without Shift nothing is stepped");
});

test("a drag on a turned object is measured along the object's own axes", () => {
  // The east grip of a shape rotated 90 degrees is dragged DOWNWARD on screen,
  // and what the user is changing is the shape's WIDTH. If this term is missing
  // the drag changes the height instead, which is the defect that makes resizing
  // a rotated object feel broken.
  const local = dragInObjectSpace(0, 100, 90);
  close(local.dx, 100, 0.001, "a downward drag is a rightward one in object space");
  close(local.dy, 0, 0.001, "and moves nothing along the object's own y");
  const upright = dragInObjectSpace(7, -3, 0);
  assert.deepEqual(
    { dx: upright.dx, dy: upright.dy },
    { dx: 7, dy: -3 },
    "an upright object is untouched by the mapping",
  );
});

test("a rotated resize keeps the pinned corner where it is ON SCREEN", () => {
  // The guarantee: drag the NW grip of a turned object and the SE corner — the
  // one the user is not touching — must not move. `resizeFromDrag` pins it in
  // the OBJECT's space, which for a turned object is a different point on the
  // page, so without the re-placement the whole shape swings as it grows.
  const start = { x: 100, y: 100, w: 200, h: 100 };
  const degrees = 37;
  const grown = { x: 60, y: 70, w: 240, h: 130 }; // an NW drag: origin moved, size up
  const placed = rotatedResizePlacement(grown, start, 0, degrees);

  const pinnedBefore = rotatePoint(
    { x: start.x + start.w, y: start.y + start.h },
    boxCentre(start),
    degrees,
  );
  const pinnedAfter = rotatePoint(
    { x: placed.x + placed.w, y: placed.y + placed.h },
    boxCentre(placed),
    degrees,
  );
  close(pinnedAfter.x, pinnedBefore.x, 1, "the south-east corner stayed put in x");
  close(pinnedAfter.y, pinnedBefore.y, 1, "and in y");
  assert.deepEqual(
    [placed.w, placed.h],
    [grown.w, grown.h],
    "and the size is whatever the drag asked for — only the placement moved",
  );

  // An upright object must come through completely unchanged, or every existing
  // resize would shift by a rounding error.
  assert.deepEqual(rotatedResizePlacement(grown, start, 0, 0), { ...grown });
  // Resizing about the centre pins the centre, which already cannot move.
  assert.deepEqual(rotatedResizePlacement(grown, start, 0, degrees, true), { ...grown });
});

test("the turned bounds are what outside chrome has to clear", () => {
  const square = rotatedBounds({ x: 0, y: 0, w: 100, h: 100 }, 45);
  close(square.w, Math.SQRT2 * 100, 0.01, "a square turned 45 degrees is sqrt(2) wide");
  close(square.h, Math.SQRT2 * 100, 0.01, "and as tall");
  close(square.x, (100 - Math.SQRT2 * 100) / 2, 0.01, "centred on the same middle");
  assert.deepEqual(
    rotatedBounds({ x: 3, y: 4, w: 5, h: 6 }, 0),
    { x: 3, y: 4, w: 5, h: 6 },
    "an upright box is its own bounds",
  );
  // 180 degrees is the case a naive min/max over two corners gets right by luck
  // and a wrong centre gets wrong: the box must land back on itself.
  const halfTurn = rotatedBounds({ x: 10, y: 20, w: 40, h: 30 }, 180);
  for (const key of ["x", "y", "w", "h"]) {
    close(halfTurn[key], { x: 10, y: 20, w: 40, h: 30 }[key], 0.001, `half turn ${key}`);
  }
});

test("angles normalise into [0, 360) so the engine never sees a third lap", () => {
  assert.equal(normaliseDegrees(-90), 270);
  assert.equal(normaliseDegrees(405), 45);
  assert.equal(normaliseDegrees(360), 0);
});

test("the grip's numbers are ONLYOFFICE's, and the bar clears the grip", () => {
  assert.equal(ROTATE_HANDLE_KIND, 8, "their `hitToHandles` returns 8 for the rotation marker");
  assert.equal(ROTATE_HANDLE_REACH, 24, "one WCAG 2.5.8 target above the edge");
  assert.ok(
    ROTATE_GRIP_REACH > ROTATE_HANDLE_REACH,
    "the bar must clear the grip's TARGET, not the point it is drawn at — a bar " +
      "inside the target means `elementFromPoint` answers with the bar and the " +
      "grip cannot be grabbed at all",
  );
});
