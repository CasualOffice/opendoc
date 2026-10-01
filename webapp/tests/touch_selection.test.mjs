// The touch-selection arithmetic (`docs/105` UX-018, `docs/148` §9 item 2).
//
// Everything here is driven in node, which is the point of the module being
// pure: Playwright cannot put a finger 4px from the top edge of a phone and
// then ask where the loupe went, and a threshold that only a browser can reach
// is a threshold nothing checks. The browser-side guarantees — that a long
// press selects a word, that the handles are big enough to hit, that dragging
// one changes the text, that a short drag scrolls — are
// `tests/e2e/phone-touch-selection.spec.mjs`, where they belong.
//
// Every guard below was driven RED by mutating `src/touch_selection.mjs`; the
// mutations and their failures are recorded in the commit message.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const {
  LONG_PRESS_MS,
  MOVE_MIN_DIST_PX,
  TRACK_TARGET_EPS_PX,
  HANDLE_DOT_PX,
  HANDLE_TARGET_PX,
  MAGNIFIER_PX,
  MAGNIFIER_SCALE,
  MAGNIFIER_GAP_PX,
  movedPastThreshold,
  handleAtPoint,
  rectIsBefore,
  orderEnds,
  magnifierPlacement,
  magnifierSource,
  pointerDragSelects,
} = await import("../src/touch_selection.mjs");

const { MIN_TOUCH_TARGET_PX } = await import("../src/phone_chrome.mjs");

// ── The tunables are ONLYOFFICE's measured numbers, not taste ───────────────
//
// `mobileTouchManagerBase.js` lines 682, 686, 699 and 136. They are asserted
// because the reason to prefer them over a round 500/10/16 is that they were
// measured against a real finger, and a later edit that "tidies" one of them
// is exactly the change this file should refuse.

test("the tunables are the figures read out of ONLYOFFICE's touch manager", () => {
  assert.equal(LONG_PRESS_MS, 750, "ReadingGlassTime");
  assert.equal(MOVE_MIN_DIST_PX, 20, "MoveMinDist");
  assert.equal(TRACK_TARGET_EPS_PX, 20, "TrackTargetEps");
  assert.equal(HANDLE_DOT_PX, 14, "MOBILE_SELECT_TRACK_ROUND");
  assert.equal(MAGNIFIER_PX, 100, "CheckGlass circle");
  assert.equal(MAGNIFIER_SCALE, 2, "CheckGlass magnification");
  assert.equal(MAGNIFIER_GAP_PX, 25, "CheckGlass offset above the finger");
});

test("the handle's target is at least WCAG 2.5.8's floor, and its dot is smaller", () => {
  assert.ok(
    HANDLE_TARGET_PX >= MIN_TOUCH_TARGET_PX,
    `a ${HANDLE_TARGET_PX}px target is below the ${MIN_TOUCH_TARGET_PX}px floor`,
  );
  assert.ok(
    HANDLE_DOT_PX < HANDLE_TARGET_PX,
    "draw small, hit big: the dot must be smaller than the box it sits in",
  );
});

// ── The press / scroll decision ─────────────────────────────────────────────

test("a press that has not moved 20px on either axis is still a press", () => {
  const down = { x: 100, y: 200 };
  assert.equal(movedPastThreshold(down, { x: 100, y: 200 }), false, "no movement at all");
  assert.equal(movedPastThreshold(down, { x: 120, y: 220 }), false, "exactly the slop");
  assert.equal(movedPastThreshold(down, { x: 80, y: 180 }), false, "the slop, backwards");
});

test("a press that HAS moved past the slop is a scroll, on either axis alone", () => {
  const down = { x: 100, y: 200 };
  assert.equal(movedPastThreshold(down, { x: 121, y: 200 }), true, "horizontal");
  assert.equal(movedPastThreshold(down, { x: 100, y: 179 }), true, "vertical, backwards");
});

// ── Which pointer may drag-select ──────────────────────────────────────────
//
// The boundary between this module and `main.js`'s drag-selection machine. Both
// were reading the same `pointermove` stream, so a finger drove two selection
// machines and the slop-free one won every race: a phone scroll selected text.
// Keyed on the pointer and never on the device — a touchscreen laptop keeps its
// mouse drag-selection and a tablet loses its finger one.

test("a finger does not drag-select; every other pointer does", () => {
  assert.equal(pointerDragSelects({ pointerType: "touch" }), false, "a finger");
  assert.equal(pointerDragSelects({ pointerType: "mouse" }), true, "a mouse");
  assert.equal(pointerDragSelects({ pointerType: "pen" }), true, "a pen/stylus");
});

test("an event with no pointerType at all still drag-selects", () => {
  // The selection auto-scroll's tick synthesizes a point rather than forwarding
  // a real event, so a synthesized point must not be mistaken for a finger: the
  // gesture it belongs to was already decided at pointer-down.
  assert.equal(pointerDragSelects({}), true, "no pointerType");
  assert.equal(pointerDragSelects(undefined), true, "no event");
});

// ── The handle hit radius ───────────────────────────────────────────────────

const HANDLES = [
  { which: "start", x: 100, y: 100 },
  { which: "end", x: 180, y: 100 },
];

test("a touch within the hit radius grabs the handle, and one outside it grabs nothing", () => {
  assert.equal(handleAtPoint(HANDLES, { x: 119, y: 119 }), "start");
  assert.equal(handleAtPoint(HANDLES, { x: 121, y: 100 }), null, "one pixel past the radius");
  assert.equal(handleAtPoint(HANDLES, { x: 100, y: 130 }), null, "past it on the other axis");
  assert.equal(handleAtPoint([], { x: 100, y: 100 }), null, "nothing painted, nothing to grab");
});

test("when both handles are in reach the NEAREST wins, not the first", () => {
  // A two-character selection: the two handles are 14px apart and every point
  // between them is inside both 20px radii. ONLYOFFICE take the first match
  // (`:958` `if pos1 … else if pos4`), which makes the end handle of a short
  // selection unreachable — you can only ever grow it leftwards.
  const tight = [
    { which: "start", x: 100, y: 100 },
    { which: "end", x: 114, y: 100 },
  ];
  assert.equal(handleAtPoint(tight, { x: 113, y: 100 }), "end");
  assert.equal(handleAtPoint(tight, { x: 101, y: 100 }), "start");
});

// ── Endpoint ordering, and the crossing swap ────────────────────────────────

const at = (id) => ({ node: id, offset: 0 });
const end = (id, rect) => ({ at: at(id), rect });

test("document order is page, then line, then position on the line", () => {
  assert.equal(rectIsBefore([1, 900, 500, 2, 20], [2, 100, 100, 2, 20]), true, "earlier page");
  assert.equal(rectIsBefore([1, 900, 100, 2, 20], [1, 100, 500, 2, 20]), true, "higher line");
  assert.equal(rectIsBefore([1, 900, 100, 2, 20], [1, 100, 100, 2, 20]), false, "same line");
});

test("dragging an endpoint past the other SWAPS the handles instead of collapsing", () => {
  const fixed = end("p1", [1, 500, 100, 2, 20]);

  // The finger started right of the fixed end: it is holding the END handle.
  const before = orderEnds(fixed, end("p1", [1, 900, 100, 2, 20]));
  assert.equal(before.start.at.node, "p1");
  assert.equal(before.start.rect[1], 500, "the fixed end is the start");
  assert.equal(before.swapped, false);

  // It has now crossed to the left of the fixed end. The range is still a
  // range — both endpoints are still here, neither is the other — and the
  // handle the finger holds has become the START.
  const after = orderEnds(fixed, end("p1", [1, 200, 100, 2, 20]));
  assert.equal(after.swapped, true, "the pair has traded places");
  assert.equal(after.start.rect[1], 200, "the finger's end is now the start");
  assert.equal(after.end.rect[1], 500, "and the fixed end is now the end");
  assert.notEqual(after.start.rect[1], after.end.rect[1], "it did not collapse");
});

test("a right-to-left line orders by document position, not by which is left", () => {
  // Two endpoints on one RTL line: the LOGICAL start sits further right, which
  // is why the handles are placed from caret rects ordered by document
  // position rather than from the left and right edges of a bounding box.
  const logicalStart = end("rtl", [1, 9000, 100, 2, 20]);
  const logicalEnd = end("rtl", [1, 7000, 100, 2, 20]);
  const ends = orderEnds(logicalStart, logicalEnd);
  // Nothing in the module claims the start is the leftmost: it claims the
  // caller hands it the anchor and the focus, and it orders THOSE.
  assert.equal(ends.start.rect[1], 7000);
  assert.equal(ends.swapped, true, "anchor after focus, so the pair is reversed");
});

// ── The loupe stays inside the window ───────────────────────────────────────

const PHONE = { viewWidth: 390, viewHeight: 844 };

test("the loupe sits clear above the finger when there is room", () => {
  const place = magnifierPlacement({ x: 195, y: 500, ...PHONE });
  assert.equal(place.flipped, false);
  assert.equal(place.top, 500 - MAGNIFIER_GAP_PX - MAGNIFIER_PX, "gap, then the circle");
  assert.equal(place.left, 195 - MAGNIFIER_PX / 2, "centred on the finger");
  assert.ok(place.top >= 0 && place.top + MAGNIFIER_PX <= PHONE.viewHeight);
});

test("at the top of the screen it FLIPS below the finger rather than painting off-window", () => {
  const place = magnifierPlacement({ x: 195, y: 40, ...PHONE });
  assert.equal(place.flipped, true);
  assert.equal(place.top, 40 + MAGNIFIER_GAP_PX, "below, by the same gap");
  assert.ok(place.top >= 0, "nothing may paint above the window");
  assert.ok(place.top + MAGNIFIER_PX <= PHONE.viewHeight, "…or below it");
});

test("it is clamped horizontally at both edges, so it never leaves the window", () => {
  const left = magnifierPlacement({ x: 4, y: 500, ...PHONE });
  assert.equal(left.left, 0);
  const right = magnifierPlacement({ x: 388, y: 500, ...PHONE });
  assert.equal(right.left, PHONE.viewWidth - MAGNIFIER_PX);
  assert.ok(right.left + MAGNIFIER_PX <= PHONE.viewWidth);
});

test("a window too short for either placement still gets the loupe fully inside it", () => {
  const place = magnifierPlacement({ x: 100, y: 60, viewWidth: 320, viewHeight: 120 });
  assert.ok(place.top >= 0);
  assert.ok(place.top + MAGNIFIER_PX <= 120);
});

// ── The loupe's source rectangle ────────────────────────────────────────────

test("the source square is centred on the finger when it fits inside the raster", () => {
  const src = magnifierSource({ x: 300, y: 400, width: 1000, height: 1400, size: 50 });
  assert.deepEqual(src, { sx: 275, sy: 375, sw: 50, sh: 50, dx: 0, dy: 0 });
});

test("at the raster's top-left the square is clamped and the DESTINATION shifts to match", () => {
  // Without the destination shift the whole image slides under the loupe and
  // the finger stops being over what it is magnifying — ONLYOFFICE's
  // `CheckGlass` does this and it is the only non-obvious part of their glass.
  const src = magnifierSource({ x: 10, y: 5, width: 1000, height: 1400, size: 50 });
  assert.equal(src.sx, 0);
  assert.equal(src.sy, 0);
  assert.equal(src.dx, 15 * MAGNIFIER_SCALE, "shifted by what was clamped, magnified");
  assert.equal(src.dy, 20 * MAGNIFIER_SCALE);
});

test("at the raster's bottom-right the square shrinks instead of reading past the edge", () => {
  const src = magnifierSource({ x: 990, y: 1390, width: 1000, height: 1400, size: 50 });
  assert.ok(src.sx + src.sw <= 1000, `${src.sx}+${src.sw} reads past the raster`);
  assert.ok(src.sy + src.sh <= 1400);
  assert.ok(src.sw > 0 && src.sh > 0);
});

// ── The stylesheet draws the numbers the module publishes ───────────────────

test("style.css draws the dot and the loupe at the sizes the module declares", () => {
  const css = readFileSync(fileURLToPath(new URL("../src/style.css", import.meta.url)), "utf8");
  const dot = css.match(/\.overlay \.touch-handle::after \{[^}]*\}/)?.[0] ?? "";
  assert.match(
    dot,
    new RegExp(`width:\\s*${HANDLE_DOT_PX}px`),
    "the drawn dot must be MOBILE_SELECT_TRACK_ROUND, or the constant is fiction",
  );
  const glass = css.match(/\n\.touch-magnifier \{[^}]*\}/)?.[0] ?? "";
  assert.match(glass, new RegExp(`width:\\s*${MAGNIFIER_PX}px`));
  assert.match(glass, /touch-action|pointer-events:\s*none/);
  assert.match(
    css.match(/\.overlay \.touch-handle \{[^}]*\}/)?.[0] ?? "",
    /touch-action:\s*none/,
    "a handle without `touch-action: none` is scrolled away before any handler runs",
  );
});
