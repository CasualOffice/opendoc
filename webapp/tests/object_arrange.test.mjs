// Arrange: the rules behind the wrap row, the Position gallery, the Rotate menu
// and the drag-to-draw gesture.
//
// These are the decisions the owner's report is about — "an inline image has no
// wrap chip and In line does not exist as a mode" — and they are arithmetic and
// branching over state the caller already holds, so they are answerable in node.
// What a browser adds here is only the DOM.
import assert from "node:assert/strict";
import test from "node:test";

const {
  POSITION_PRESETS,
  ROTATE_CHOICES,
  WRAP_CHOICES,
  Z_ORDER_CHOICES,
  activeWrapChoice,
  positionAvailability,
  positionPayload,
  readGroupability,
  readPosition,
  readTransform,
  rotationPlan,
  wrapAvailability,
  wrapPlan,
} = await import("../src/object_arrange.mjs");

const {
  DEFAULT_SHAPE_TWIPS,
  MIN_SHAPE_TWIPS,
  clampToPage,
  committedRect,
  drawnRect,
  isClickNotDrag,
} = await import("../src/shape_draw.mjs");

const { galleryRowStarts, nextGalleryIndex } = await import("../src/shape_gallery.mjs");

/** What the facade answers for an inline top-level object — the DEFAULT state of
 *  an inserted picture, and the one the wrap control used to be invisible in. */
const INLINE = readPosition('{"floating":false}');
const FLOATING = readPosition(
  '{"floating":true,"horizontal":{"relativeFrom":"column","offsetEmu":0},' +
    '"vertical":{"relativeFrom":"paragraph","align":"top"},"wrap":"square","behindDoc":false}',
);
const GROUP_CHILD = readPosition('{"floating":false,"groupChild":true}');
const NOT_AN_OBJECT = readPosition("");

// ---- Wrap -------------------------------------------------------------------

test("in line is a MODE, and it leads the row", () => {
  // Google Docs puts "In line" first under a selected image; Word's Layout
  // Options puts "In Line with Text" above its six wrapping modes.
  assert.equal(WRAP_CHOICES[0].value, "inline");
  assert.equal(WRAP_CHOICES.length, 7);
});

test("an INLINE object is offered the wrap row, and reads as in line", () => {
  // The defect: `canWrap` is false for an inline object, the chip was keyed on
  // it, and inserting a picture gives you an inline one. Availability is keyed
  // on the ANCHOR KIND being rewritable instead — which `objectPosition`
  // answers for every top-level object, floating or not.
  assert.equal(wrapAvailability(INLINE).available, true);
  assert.equal(activeWrapChoice(INLINE, ""), "inline");
});

test("a group's CHILD is refused, with the reason its parent decides", () => {
  const availability = wrapAvailability(GROUP_CHILD);
  assert.equal(availability.available, false);
  assert.equal(availability.reasonKey, "object.wrap.groupChild");
});

test("something that is not an object is refused, and told apart from a child", () => {
  // `objectPosition` answers "" for "not an object" and `{groupChild:true}` for
  // a group's child. A host that could not tell them apart would say the wrong
  // thing in one of the two cases.
  assert.equal(wrapAvailability(NOT_AN_OBJECT).reasonKey, "object.wrap.notAnObject");
  assert.notEqual(wrapAvailability(GROUP_CHILD).reasonKey, "object.wrap.notAnObject");
});

test("going in line is an ANCHOR REWRITE, not a wrap mode", () => {
  // `setObjectWrap` cannot express it: an inline object has no anchor to carry
  // a wrap. The engine's `setObjectAnchorKind` rewrites the node, and this is
  // the plan that says so.
  assert.deepEqual(wrapPlan("inline", FLOATING, "square"), [{ op: "anchorKind", kind: "inline" }]);
});

test("leaving in line floats the object FIRST, then sets the mode asked for", () => {
  // Floating gives Word's conversion default (square). The user asked for
  // "behind text", so the wrap follows the conversion; without the second step
  // the chip would silently do something other than what was pressed.
  assert.deepEqual(wrapPlan("behind", INLINE, ""), [
    { op: "anchorKind", kind: "floating" },
    { op: "wrap", mode: "behind" },
  ]);
});

test("a floating object only sets the wrap; it is not re-floated", () => {
  assert.deepEqual(wrapPlan("tight", FLOATING, "square"), [{ op: "wrap", mode: "tight" }]);
});

test("re-asserting the current mode does nothing at all", () => {
  // A chip bound to the mode it is already in must not push an undo entry.
  assert.deepEqual(wrapPlan("square", FLOATING, "square"), []);
  assert.deepEqual(wrapPlan("inline", INLINE, ""), []);
});

test("an unknown wrap token from the engine falls back to square, not to nothing", () => {
  assert.equal(activeWrapChoice(FLOATING, "wrapPolygon"), "square");
});

// ---- Position ---------------------------------------------------------------

test("the Position gallery is Word's nine cells, against the margin", () => {
  assert.equal(POSITION_PRESETS.length, 9);
  const payload = positionPayload(POSITION_PRESETS.find((preset) => preset.id === "bottomRight"));
  assert.deepEqual(payload, {
    horizontal: { relativeFrom: "margin", align: "right" },
    vertical: { relativeFrom: "margin", align: "bottom" },
    wrap: "square",
  });
});

test("a position cell sends ONLY what it decides", () => {
  // The facade keeps every omitted field, so a cell that restated the wrap
  // distances would overwrite an author's `wp:wrapSquare` insets with this
  // host's idea of them.
  const payload = positionPayload(POSITION_PRESETS[0]);
  assert.deepEqual(Object.keys(payload).sort(), ["horizontal", "vertical", "wrap"]);
  assert.ok(!("wrapDistances" in payload));
  assert.ok(!("behindDoc" in payload));
  assert.ok(!("zOrder" in payload));
});

test("positioning an in-line object floats it first, and says so", () => {
  assert.equal(positionAvailability(INLINE).floatFirst, true);
  assert.equal(positionAvailability(FLOATING).floatFirst, false);
  assert.equal(positionAvailability(GROUP_CHILD).available, false);
});

// ---- Stacking and grouping --------------------------------------------------

test("the four stacking commands are Word's four", () => {
  assert.deepEqual(Z_ORDER_CHOICES.map((choice) => choice.value), [
    "front",
    "forward",
    "backward",
    "back",
  ]);
});

test("grouping carries the ENGINE's verdict and the ENGINE's sentence", () => {
  assert.deepEqual(readGroupability('{"can":true}'), { can: true, reason: null });
  assert.deepEqual(readGroupability('{"can":false,"reason":"same page"}'), {
    can: false,
    reason: "same page",
  });
  // A malformed or absent answer must not read as permission.
  assert.equal(readGroupability("").can, false);
  assert.equal(readGroupability("not json").can, false);
});

// ---- Rotation ---------------------------------------------------------------

test("Rotate is Word's four rows", () => {
  assert.deepEqual(ROTATE_CHOICES.map((choice) => choice.value), [
    "right90",
    "left90",
    "flipV",
    "flipH",
  ]);
});

test("rotation accumulates and wraps at 360", () => {
  assert.deepEqual(rotationPlan("right90", { rotationDegrees: 300 }), { op: "rotation", degrees: 30 });
  assert.deepEqual(rotationPlan("left90", { rotationDegrees: 45 }), { op: "rotation", degrees: 315 });
  assert.deepEqual(rotationPlan("right90", null), { op: "rotation", degrees: 90 });
});

test("a flip row TOGGLES, because the facade's flags are absolute", () => {
  // Two checkboxes, not two toggles: a menu row that wrote `true` every time
  // would be a flip that only ever flips once.
  assert.deepEqual(rotationPlan("flipH", { flipH: false, flipV: true }), {
    op: "flip",
    flipH: true,
    flipV: true,
  });
  assert.deepEqual(rotationPlan("flipH", { flipH: true, flipV: true }), {
    op: "flip",
    flipH: false,
    flipV: true,
  });
});

test("an object whose model carries no transform answers nothing", () => {
  assert.equal(readTransform(""), null);
  assert.deepEqual(readTransform('{"rotationDegrees":45,"flipH":false,"flipV":false}'), {
    rotationDegrees: 45,
    flipH: false,
    flipV: false,
  });
});

// ---- Drawing a shape --------------------------------------------------------

test("a drag in any direction produces a positive rectangle", () => {
  const up = drawnRect({ x: 900, y: 900 }, { x: 300, y: 400 });
  assert.deepEqual(up, { left: 300, top: 400, width: 600, height: 500 });
});

test("Shift keeps the pressed corner and squares the drag", () => {
  const square = drawnRect({ x: 100, y: 100 }, { x: 400, y: 200 }, { square: true });
  assert.deepEqual(square, { left: 100, top: 100, width: 300, height: 300 });
  const upLeft = drawnRect({ x: 500, y: 500 }, { x: 200, y: 400 }, { square: true });
  assert.deepEqual(upLeft, { left: 200, top: 200, width: 300, height: 300 });
});

test("a press that barely moved is a CLICK, and a click gets the default size", () => {
  const wobble = drawnRect({ x: 100, y: 100 }, { x: 110, y: 96 });
  assert.equal(isClickNotDrag(wobble), true);
  assert.deepEqual(committedRect(wobble), {
    left: 100,
    top: 96,
    width: DEFAULT_SHAPE_TWIPS.width,
    height: DEFAULT_SHAPE_TWIPS.height,
  });
});

test("a real drag gets the size that was drawn, floored so it stays grabbable", () => {
  const drawn = drawnRect({ x: 200, y: 200 }, { x: 1640, y: 920 });
  assert.deepEqual(committedRect(drawn), { left: 200, top: 200, width: 1440, height: 720 });
  // Past the click threshold on one axis only: the other is floored rather than
  // left as a sliver nobody can select again.
  const sliver = { left: 0, top: 0, width: 2000, height: 50 };
  assert.equal(committedRect(sliver).height, MIN_SHAPE_TWIPS);
});

test("a shape drawn off the sheet is slid back on, not placed where nobody sees it", () => {
  const page = { width: 12240, height: 15840 };
  assert.deepEqual(clampToPage({ left: 12000, top: 15700, width: 2880, height: 1440 }, page), {
    left: 9360,
    top: 14400,
    width: 2880,
    height: 1440,
  });
  // Bigger than the page: the size comes down rather than the origin going
  // negative.
  assert.deepEqual(clampToPage({ left: -500, top: -500, width: 99999, height: 99999 }, page), {
    left: 0,
    top: 0,
    width: 12240,
    height: 15840,
  });
});

// ---- The gallery's keyboard grid -------------------------------------------

test("Left and Right walk the flat order and wrap at both ends", () => {
  const rows = galleryRowStarts(6);
  assert.equal(nextGalleryIndex("ArrowRight", 21, rows, 22), 0);
  assert.equal(nextGalleryIndex("ArrowLeft", 0, rows, 22), 21);
  assert.equal(nextGalleryIndex("Home", 9, rows, 22), 0);
  assert.equal(nextGalleryIndex("End", 0, rows, 22), 21);
});

test("Down moves a row and stops at the last one instead of falling off", () => {
  const rows = galleryRowStarts(6);
  // Row 0 is Lines (one cell); row 1 is Rectangles (two).
  assert.equal(nextGalleryIndex("ArrowDown", 0, rows, 22), 1);
  assert.equal(nextGalleryIndex("ArrowUp", 0, rows, 22), 0);
  assert.equal(nextGalleryIndex("ArrowDown", 21, rows, 22), 21);
});

test("a column past the end of the next row lands on that row's last cell", () => {
  const rows = galleryRowStarts(6);
  // Rectangles has two cells; from its second, Down must land inside Basic
  // Shapes rather than on an index that does not exist.
  const next = nextGalleryIndex("ArrowDown", 2, rows, 22);
  assert.ok(next >= 3 && next < 22, `landed on ${next}`);
});

test("a key the gallery does not own is refused, so the menu keeps it", () => {
  assert.equal(nextGalleryIndex("Enter", 0, galleryRowStarts(6), 22), -1);
  assert.equal(nextGalleryIndex("Escape", 0, galleryRowStarts(6), 22), -1);
});
