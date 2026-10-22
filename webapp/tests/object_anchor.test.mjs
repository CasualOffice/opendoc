// The shared anchored-popover placement (`object_anchor.mjs`, `docs/172`).
//
// Every guarantee the object toolbar, the link card and the block menu rely on,
// asked as arithmetic: below by preference, flipped when there is no room,
// clear of the grips, inside the view, centred (or start-aligned by reading
// direction), and hidden when the anchor is out of view.
import assert from "node:assert/strict";
import test from "node:test";

const { ANCHOR_GAP, placeAnchored } = await import("../src/object_anchor.mjs");

const VIEW = { left: 100, top: 100, right: 1300, bottom: 900 };
const SIZE = { width: 300, height: 40 };

test("below the anchor by preference, clear of the grips below it", () => {
  const anchor = { left: 500, top: 300, right: 700, bottom: 400 };
  const at = placeAnchored(anchor, SIZE, VIEW, { clearance: { below: 20 } });
  assert.equal(at.side, "below");
  assert.equal(at.top, 400 + 20 + ANCHOR_GAP, "the grip band plus the gap");
  assert.ok(at.top >= anchor.bottom + 20, "never inside a grip's target");
});

test("centred on the anchor", () => {
  const at = placeAnchored({ left: 500, top: 300, right: 700, bottom: 400 }, SIZE, VIEW);
  assert.equal(at.left, 600 - 150);
});

test("flips above when there is no room below, clearing the rotation grip", () => {
  const anchor = { left: 500, top: 700, right: 700, bottom: 880 };
  const at = placeAnchored(anchor, SIZE, VIEW, { clearance: { above: 44, below: 20 } });
  assert.equal(at.side, "above");
  assert.equal(at.top, 700 - 44 - ANCHOR_GAP - 40);
  assert.ok(at.top + SIZE.height <= anchor.top - 44, "the bar's bottom clears the rotation grip");
});

test("an anchor taller than the view gets the surface INSIDE the view, never off screen", () => {
  const anchor = { left: 500, top: 50, right: 700, bottom: 1200 };
  const at = placeAnchored(anchor, SIZE, VIEW);
  assert.equal(at.side, "inside");
  assert.ok(at.top >= VIEW.top && at.top + SIZE.height <= VIEW.bottom, "reachable");
});

test("never pinned over the text above an object that scrolled up under the ribbon", () => {
  // The defect: the old bar clamped to the view top whenever the object's top
  // was above it, covering the heading the reader was looking at. Now an
  // object whose top has scrolled out still gets its bar BELOW it.
  const anchor = { left: 500, top: 60, right: 700, bottom: 300 };
  const at = placeAnchored(anchor, SIZE, VIEW, { clearance: { above: 44, below: 20 } });
  assert.equal(at.side, "below");
  assert.ok(at.top >= anchor.bottom + 20);
});

test("kept inside the view horizontally at both edges", () => {
  const left = placeAnchored({ left: 90, top: 300, right: 110, bottom: 320 }, SIZE, VIEW);
  assert.equal(left.left, VIEW.left + ANCHOR_GAP);
  const right = placeAnchored({ left: 1280, top: 300, right: 1300, bottom: 320 }, SIZE, VIEW);
  assert.equal(right.left, VIEW.right - ANCHOR_GAP - SIZE.width);
});

test("start alignment follows the reading direction", () => {
  const anchor = { left: 500, top: 300, right: 900, bottom: 400 };
  assert.equal(placeAnchored(anchor, SIZE, VIEW, { align: "start" }).left, 500);
  assert.equal(placeAnchored(anchor, SIZE, VIEW, { align: "start", dir: "rtl" }).left, 900 - 300);
});

test("hidden when the anchor is out of view on either axis", () => {
  for (const anchor of [
    { left: 500, top: -300, right: 700, bottom: 90 },
    { left: 500, top: 950, right: 700, bottom: 1000 },
    { left: -400, top: 300, right: 50, bottom: 400 },
    { left: 1400, top: 300, right: 1500, bottom: 400 },
  ]) {
    assert.equal(placeAnchored(anchor, SIZE, VIEW), null, JSON.stringify(anchor));
  }
});
