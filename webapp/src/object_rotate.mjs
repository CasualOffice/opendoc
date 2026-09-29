// Free rotation of an object: the arithmetic of the handle drag, and of a
// resize drag on an object that is already turned.
//
// Same split as `object_snap.mjs` / `object_guides.mjs`: this decides the
// numbers, `main.js` owns the gesture and the engine call, and nothing here
// touches the DOM or a global. Every function is O(1) — a rotation drag fires on
// every pointer move (`docs/107` §4), so none of this may consult the document.
//
// The rule followed is ONLYOFFICE's, read from
// `common/Drawings/TrackObjects/RotateTracks.js` on the reference checkout, not
// invented here; the places we knowingly differ say so.

/** How close to a quarter turn the handle must come before it snaps there.
 *
 *  ONLYOFFICE's `MIN_ANGLE` is `0.07` radians (`RotateTracks.js:46`) — 4.01
 *  degrees — applied around 0, 90, 180 and 270. Kept in degrees here because
 *  every other angle in this editor is in degrees, and converting at one place
 *  is cheaper than remembering which unit a constant is in.
 */
export const CARDINAL_SNAP_DEGREES = 0.07 * (180 / Math.PI);

/** The step Shift constrains a rotation to.
 *
 *  ONLYOFFICE: `new_rot = (Math.PI/12) * Math.floor(12 * new_rot / Math.PI)`
 *  (`RotateTracks.js:520`) — 15 degrees. Word and Google Docs both use 15 too,
 *  so this is the one number all three agree on.
 */
export const SHIFT_SNAP_DEGREES = 15;

/** How far the rotation grip floats beyond the object's top edge, in CSS pixels.
 *
 *  ONLYOFFICE's `TRACK_DISTANCE_ROTATE` is 25 screen pixels (`common/Overlay.js:46`)
 *  and is a SCREEN distance there too — their `convertPixToMM` turns it into
 *  document units against the current zoom, so the grip keeps the same size on
 *  screen whatever the zoom is. The engine therefore publishes the grip's anchor
 *  ON the top edge and this is applied here, along the object's own axis.
 *
 *  24 rather than 25 so the tether is exactly the 24px WCAG 2.5.8 target tall,
 *  which makes the gap between the grip's target and the north grip's target
 *  nil — there is no dead band to fall into between them.
 */
export const ROTATE_HANDLE_REACH = 24;

/** How far the rotation grip's hit target reaches above the object, in pixels.
 *
 *  The reach, plus the grip's own half-height and its `--grip-grow`. The object
 *  bar has to clear THIS, not `GRIP_REACH`: a bar placed for the resize grips
 *  alone lands squarely on the rotation grip, which is the same defect
 *  `objectBarPosition` already carries a comment about for the north grips
 *  (`docs/104` HF-058 neighbourhood).
 */
export const ROTATE_GRIP_REACH = ROTATE_HANDLE_REACH + 20;

/** The engine's handle kind for the rotation grip, past the eight resize kinds.
 *  ONLYOFFICE's own numbering: `hitToHandles` returns 0..7 for the resize
 *  markers and 8 for the rotation one (`common/Drawings/Format/Shape.js:307`). */
export const ROTATE_HANDLE_KIND = 8;

/** Normalises any angle into `[0, 360)`. O(1). */
export function normaliseDegrees(degrees) {
  return ((degrees % 360) + 360) % 360;
}

/**
 * The angle, clockwise from the object's upright position, that the pointer is
 * currently asking for.
 *
 * Measured from the object's CENTRE to the pointer, minus the bearing the
 * rotation grip sits at when the object is upright (straight up, i.e. -90
 * degrees in screen bearing terms). The grip is grabbed wherever it is drawn and
 * the object follows the pointer from there, which is what every product does
 * and what stops the object jumping on pointer-down.
 *
 * Page coordinates run y-DOWNWARD, so `atan2(dy, dx)` already increases
 * clockwise — the same sense as `a:xfrm@rot` — and needs no sign flip.
 *
 * O(1).
 *
 * @param {{x: number, y: number}} centre the object's centre, in any one space
 * @param {{x: number, y: number}} pointer the pointer, in that same space
 * @returns {number} degrees clockwise in `[0, 360)`
 */
export function pointerAngle(centre, pointer) {
  const bearing = Math.atan2(pointer.y - centre.y, pointer.x - centre.x) * (180 / Math.PI);
  return normaliseDegrees(bearing + 90);
}

/**
 * ONLYOFFICE's snapping rule for a rotation drag, in the order they apply it.
 *
 * First the four quarter turns pull the angle in when it comes within
 * `MIN_ANGLE`; then Shift, if held, constrains what is left to 15 degrees.
 * The ORDER matters and is theirs: Shift is applied last, over the cardinal
 * result, so holding Shift near 0 still lands on 0 rather than being pushed off
 * it by the step.
 *
 * Two deliberate differences, both stated rather than silent:
 *
 *  - They FLOOR to the 15 degree step (`Math.floor(12 * rot / PI)`), so the
 *    object always lags the pointer by up to 15 degrees and reaching 90 exactly
 *    means overshooting it. This ROUNDS, so the nearest step wins — which is
 *    what Word and Google Docs do, and what the user is aiming at.
 *  - Their cardinal snap is a bare `if` chain that a Shift hold then overrides
 *    anyway; rounding makes the two agree, since every quarter turn is a
 *    multiple of 15.
 *
 * O(1).
 *
 * @param {number} degrees the raw angle the pointer asks for
 * @param {boolean} shiftKey whether Shift is held
 * @returns {number} the angle to apply, in `[0, 360)`
 */
export function snapRotation(degrees, shiftKey) {
  let angle = normaliseDegrees(degrees);
  for (const cardinal of [0, 90, 180, 270, 360]) {
    if (Math.abs(angle - cardinal) < CARDINAL_SNAP_DEGREES) {
      angle = cardinal % 360;
      break;
    }
  }
  if (shiftKey === true) {
    angle = normaliseDegrees(Math.round(angle / SHIFT_SNAP_DEGREES) * SHIFT_SNAP_DEGREES);
  }
  return angle;
}

/** Rotates `point` clockwise about `centre` by `degrees`. O(1). */
export function rotatePoint(point, centre, degrees) {
  if (!degrees) return { x: point.x, y: point.y };
  const radians = degrees * (Math.PI / 180);
  const [cos, sin] = [Math.cos(radians), Math.sin(radians)];
  const [dx, dy] = [point.x - centre.x, point.y - centre.y];
  return {
    x: centre.x + dx * cos - dy * sin,
    y: centre.y + dx * sin + dy * cos,
  };
}

/** The centre of a `{x, y, w, h}` box. O(1). */
export function boxCentre(box) {
  return { x: box.x + box.w / 2, y: box.y + box.h / 2 };
}

/**
 * The pointer delta expressed in the OBJECT's own axes.
 *
 * This is the whole trick to resizing something that is turned: drag the grip on
 * the right-hand edge of a shape rotated 90 degrees and the pointer moves
 * DOWNWARD, but what the user is changing is the shape's width. Rotating the
 * delta backwards by the object's angle puts it back in the space
 * `resizeFromDrag` already reasons in, so the eight-grip rules — which axis a
 * grip drives, which edge it pins, the aspect lock — need no rotation term at
 * all.
 *
 * It is also exactly what ONLYOFFICE does: `hitToHandles`
 * (`common/Drawings/Format/Shape.js:193`) runs the pointer through
 * `getInvertTransform()` before comparing it to any marker, so every handle
 * test there is in object space too.
 *
 * O(1).
 *
 * @param {number} dx pointer dx in page space
 * @param {number} dy pointer dy in page space
 * @param {number} degrees the object's clockwise rotation
 * @returns {{dx: number, dy: number}} the delta along the object's own axes
 */
export function dragInObjectSpace(dx, dy, degrees) {
  const turned = rotatePoint({ x: dx, y: dy }, { x: 0, y: 0 }, -degrees);
  return { dx: turned.x, dy: turned.y };
}

/** The object-local offset, from the box centre, of the corner a grip pins.
 *
 *  Index by handle kind, in the engine's NW,N,NE,E,SE,S,SW,W order: each entry
 *  is the OPPOSITE corner's direction in half-extents. A north-west drag pins
 *  the south-east corner, and a north drag pins the southern EDGE, whose x is
 *  irrelevant — recorded as 0, since only the axis the grip drives can move. */
const PINNED_CORNER = [
  [1, 1], // NW pins SE
  [0, 1], // N  pins S
  [-1, 1], // NE pins SW
  [-1, 0], // E  pins W
  [-1, -1], // SE pins NW
  [0, -1], // S  pins N
  [1, -1], // SW pins NE
  [1, 0], // W  pins E
];

/**
 * Re-places a resized box so the corner the grip pins stays where it is ON
 * SCREEN, for an object that is rotated.
 *
 * `resizeFromDrag` works in the object's own space and pins the opposite corner
 * THERE. For an upright object those are the same thing. For a turned one they
 * are not: keeping the object-space top-left fixed while the box grows swings
 * the whole shape about the page, because the rotation is about the box's
 * centre and the centre has moved. So the new centre is derived from the pinned
 * corner instead — the corner is rotated into page space at both the old and the
 * new size, and the box is translated by the difference.
 *
 * With `fromCentre` (Ctrl/Cmd) nothing is pinned but the centre itself, which is
 * already fixed, so the box is returned unchanged.
 *
 * O(1).
 *
 * @param {{x: number, y: number, w: number, h: number}} box `resizeFromDrag`'s result
 * @param {{x: number, y: number, w: number, h: number}} start the box at pointer-down
 * @param {number} handleKind 0..7
 * @param {number} degrees the object's clockwise rotation
 * @param {boolean} [fromCentre] whether the drag resizes about the centre
 * @returns {{x: number, y: number, w: number, h: number}} the placed box
 */
export function rotatedResizePlacement(box, start, handleKind, degrees, fromCentre = false) {
  const pin = PINNED_CORNER[handleKind];
  if (!degrees || !pin || fromCentre === true) return { ...box };
  const startCentre = boxCentre(start);
  // Where the pinned corner is on the page right now.
  const anchored = rotatePoint(
    {
      x: startCentre.x + (pin[0] * start.w) / 2,
      y: startCentre.y + (pin[1] * start.h) / 2,
    },
    startCentre,
    degrees,
  );
  // Where it would land if the resized box kept `resizeFromDrag`'s placement.
  const naiveCentre = boxCentre(box);
  const drifted = rotatePoint(
    {
      x: naiveCentre.x + (pin[0] * box.w) / 2,
      y: naiveCentre.y + (pin[1] * box.h) / 2,
    },
    naiveCentre,
    degrees,
  );
  return {
    x: Math.round(box.x + anchored.x - drifted.x),
    y: Math.round(box.y + anchored.y - drifted.y),
    w: box.w,
    h: box.h,
  };
}

/**
 * The tightest axis-aligned rectangle containing a rotated box.
 *
 * Chrome that has to sit OUTSIDE the object — the object bar, the scroll-into-
 * view rect — needs this rather than the frame, or it overlaps a turned object's
 * corners. The drawn outline and the grips do not: they are turned with the
 * object.
 *
 * O(1) — four corners.
 *
 * @param {{x: number, y: number, w: number, h: number}} box the unrotated frame
 * @param {number} degrees the clockwise rotation
 * @returns {{x: number, y: number, w: number, h: number}}
 */
export function rotatedBounds(box, degrees) {
  if (!degrees) return { ...box };
  const centre = boxCentre(box);
  const corners = [
    { x: box.x, y: box.y },
    { x: box.x + box.w, y: box.y },
    { x: box.x + box.w, y: box.y + box.h },
    { x: box.x, y: box.y + box.h },
  ].map((corner) => rotatePoint(corner, centre, degrees));
  const xs = corners.map((corner) => corner.x);
  const ys = corners.map((corner) => corner.y);
  const [left, top] = [Math.min(...xs), Math.min(...ys)];
  return { x: left, y: top, w: Math.max(...xs) - left, h: Math.max(...ys) - top };
}
