// Zoom anchoring's arithmetic (audit CANVAS-08, CANVAS-V-02). The browser half
// is `e2e/zoom-anchor.spec.mjs`; this is the part that needs no browser.
import assert from "node:assert/strict";
import test from "node:test";

import { SIDE_GUTTER_PX, anchorPoint, anchorScrollDelta, captureAnchor, sideGutterScroll } from "../src/zoom_anchor.mjs";

test("the anchored document point returns to its client position after a zoom", () => {
  // A page at (252, 100), 1 CSS px per twip-unit for simplicity; the pointer at
  // (800, 600) is document point (548, 500) on that page.
  const before = { left: 252, top: 100 };
  const anchor = captureAnchor(0, before, 1, 1, { x: 800, y: 600 });
  assert.deepEqual([anchor.tx, anchor.ty], [548, 500]);
  // At 300% the page is three times the scale and has moved.
  const after = { left: 100, top: 100 };
  const { dx, dy } = anchorScrollDelta(anchor, after, 3, 3);
  // Scrolling by (dx, dy) moves the page left/up by that much, putting the
  // document point back under the pointer.
  assert.equal(after.left - dx + anchor.tx * 3, 800);
  assert.equal(after.top - dy + anchor.ty * 3, 600);
});

test("no movement means no scroll", () => {
  const rect = { left: 10, top: 20 };
  const anchor = captureAnchor(2, rect, 0.5, 0.5, { x: 110, y: 220 });
  assert.deepEqual(anchorScrollDelta(anchor, rect, 0.5, 0.5), { dx: 0, dy: 0 });
});

test("the pointer wins, then a caret in view, then the middle of the view", () => {
  const view = { left: 0, top: 100, right: 1000, bottom: 800 };
  assert.equal(anchorPoint({ x: 5, y: 6 }, { x: 50, y: 400 }, view).source, "pointer");
  assert.equal(anchorPoint(null, { x: 50, y: 400 }, view).source, "caret");
  // A caret scrolled out of view is not what the reader is looking at.
  const away = anchorPoint(null, { x: 50, y: 2000 }, view);
  assert.equal(away.source, "view");
  assert.deepEqual([away.x, away.y], [500, 450]);
});

test("a page wider than the window keeps at most a 24px side gutter (CANVAS-V-02)", () => {
  const view = { left: 40, right: 1280 };
  // Measured: 161% with the caret at the left margin left the page's left edge
  // at 233 (a 193px band past the viewport's edge) and its right edge off screen.
  const wide = { left: 233, right: 233 + 1314 };
  const dx = sideGutterScroll(wide, view);
  assert.equal(wide.left - dx - view.left, SIDE_GUTTER_PX, "the near edge comes to the gutter");
  // The mirror case: a band on the right while the left edge is off screen.
  const right = { left: -400, right: -400 + 1314 };
  const back = sideGutterScroll(right, view);
  assert.equal(view.right - (right.right - back), SIDE_GUTTER_PX);
  // Both edges past the view, or inside the gutter: nothing to do.
  assert.equal(sideGutterScroll({ left: 10, right: 1400 }, view), 0);
  assert.equal(sideGutterScroll({ left: 60, right: 1300 }, view), 0);
  // A page that fits is centred by the layout, never pulled to one side.
  assert.equal(sideGutterScroll({ left: 300, right: 1100 }, view), 0);
});
