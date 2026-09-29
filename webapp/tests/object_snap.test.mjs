// The snap rule, driven with plain numbers.
//
// The guarantee under test is not "a guide element exists" — it is that an
// object dropped NEAR a line comes to rest ON it, and that one dropped far from
// every line is left exactly where it was put. Those are different assertions
// and only the first one is the feature; the second is what stops a snap that
// swallows every placement.
import assert from "node:assert/strict";
import test from "node:test";

import { objectBarPosition, GRIP_REACH } from "../src/object_guides.mjs";
import { pageSnapTargets, resizeFromDrag, snapBox, snapEdge } from "../src/object_snap.mjs";

// US Letter with 1in margins, in twips.
const PAGE = {
  widthTwip: 12240,
  heightTwip: 15840,
  marginStartTwip: 1440,
  marginEndTwip: 1440,
};
const TOLERANCE = 120; // ~7 CSS px at 100%

test("a page offers the two margins, its centre line and its middle", () => {
  const targets = pageSnapTargets(PAGE);
  assert.deepEqual(
    targets.vertical.map((line) => [line.id, line.at]),
    [["leftMargin", 1440], ["pageCentre", 6120], ["rightMargin", 10800]],
  );
  assert.deepEqual(
    targets.horizontal.map((line) => [line.id, line.at]),
    [["pageMiddle", 7920]],
  );
});

test("an object dropped just off the page centre comes to rest ON it", () => {
  const targets = pageSnapTargets(PAGE);
  // A 2in-wide image whose centre is 80 twips right of the page centre line.
  const width = 2880;
  const near = { x: 6120 - width / 2 + 80, y: 3000, w: width, h: 1440 };
  const snapped = snapBox(near, targets, TOLERANCE);

  // The guarantee: the object's CENTRE is on the page's centre line afterwards.
  assert.equal(snapped.x + width / 2, 6120);
  assert.deepEqual(
    snapped.guides.map((g) => [g.axis, g.id]),
    [["vertical", "pageCentre"]],
  );
});

test("an object dropped away from every line is left exactly where it was put", () => {
  const targets = pageSnapTargets(PAGE);
  // Centre at 4000: 2120 from the page centre, 1120 from the left margin edge —
  // far outside the tolerance on every reference.
  const box = { x: 4000 - 1440, y: 3000, w: 2880, h: 1440 };
  const snapped = snapBox(box, targets, TOLERANCE);

  assert.equal(snapped.x, box.x);
  assert.equal(snapped.y, box.y);
  assert.deepEqual(snapped.guides, []);
});

test("each axis resolves on its own: horizontal snap, vertical free", () => {
  const targets = pageSnapTargets(PAGE);
  const box = { x: 1440 - 30, y: 500, w: 2880, h: 1440 }; // left edge near the margin
  const snapped = snapBox(box, targets, TOLERANCE);

  assert.equal(snapped.x, 1440, "the left edge landed on the left margin");
  assert.equal(snapped.y, 500, "nothing was near a horizontal line, so y is untouched");
  assert.deepEqual(snapped.guides.map((g) => g.axis), ["vertical"]);
});

test("the nearest line wins when two are in range", () => {
  const targets = { vertical: [{ at: 1000, id: "far" }, { at: 1100, id: "near" }], horizontal: [] };
  const snapped = snapBox({ x: 1090, y: 0, w: 10, h: 10 }, targets, 200);
  assert.equal(snapped.guides[0].id, "near");
});

test("a moving edge snaps; the pinned edge is not a candidate", () => {
  const targets = pageSnapTargets(PAGE).vertical;
  const { edge, guide } = snapEdge(10800 - 40, targets, "vertical", TOLERANCE);
  assert.equal(edge, 10800);
  assert.equal(guide.id, "rightMargin");

  const away = snapEdge(5000, targets, "vertical", TOLERANCE);
  assert.equal(away.edge, 5000);
  assert.equal(away.guide, null);
});

test("a locked corner drag keeps the ratio, whichever axis the pointer favoured", () => {
  const start = { x: 1000, y: 1000, w: 2000, h: 1000, aspect: 2 };
  // Pointer moved far in x and barely in y on the SE grip (kind 4).
  const box = resizeFromDrag(start, 4, 600, 20, { lockAspect: true, minEdge: 144 });
  assert.equal(box.lockedAspect, true);
  assert.equal(box.w, 2600);
  assert.equal(box.h, 1300, "height followed the ratio, not the pointer");
  assert.equal(box.x, 1000, "the NW corner is pinned on an SE drag");
  assert.equal(box.y, 1000);
});

test("a free corner drag takes both axes from the pointer", () => {
  const start = { x: 1000, y: 1000, w: 2000, h: 1000, aspect: 2 };
  const box = resizeFromDrag(start, 4, 600, 20, { lockAspect: false, minEdge: 144 });
  assert.equal(box.lockedAspect, false);
  assert.equal(box.w, 2600);
  assert.equal(box.h, 1020);
});

test("a NW drag moves the north and west edges and pins the south-east corner", () => {
  const start = { x: 1000, y: 1000, w: 2000, h: 1000, aspect: 2 };
  const box = resizeFromDrag(start, 0, -400, -200, { lockAspect: false, minEdge: 144 });
  assert.equal(box.w, 2400);
  assert.equal(box.h, 1200);
  assert.equal(box.x + box.w, 3000, "the east edge did not move");
  assert.equal(box.y + box.h, 2000, "the south edge did not move");
  assert.equal(box.movesWest, true);
  assert.equal(box.movesNorth, true);
});

test("no drag can collapse an object below the minimum edge", () => {
  const start = { x: 1000, y: 1000, w: 2000, h: 1000, aspect: 2 };
  const box = resizeFromDrag(start, 4, -99999, -99999, { lockAspect: false, minEdge: 144 });
  assert.equal(box.w, 144);
  assert.equal(box.h, 144);
});

// ---- The object bar's placement (docs/104 HF-058) ---------------------------
// It moved into `object_guides.mjs` with the rest of what a selected object
// shows. The guarantees are that the bar never sits behind the ribbon and never
// outlives its object's visibility — the defect the row records is a live
// Delete button left aimed at an object the user can no longer see. In
// `main.js` this was four `style.top` assignments in a repaint and unaskable.

test("the bar sits above its object, clear of the resize grips", () => {
  // 500 - 40 (the bar) - 20 (`GRIP_REACH`) = 440. It used to be 452, an 8px gap
  // that put the bar INSIDE the north-west grip's 24px target, so the top-left
  // corner of a selected picture could not be grabbed: `elementFromPoint` there
  // answered with the bar. The number is derived, not chosen — 5px of centring
  // margin plus `--grip-grow`.
  assert.deepEqual(
    objectBarPosition({ left: 200, top: 500, bottom: 620 }, { top: 100, bottom: 900 }, 40),
    { left: 200, top: 440 },
  );
  // And the guarantee behind the number: whatever the clearance is, the bar's
  // bottom edge never reaches into the grip's target.
  const placed = objectBarPosition({ left: 200, top: 500, bottom: 620 }, { top: 100, bottom: 900 }, 40);
  assert.ok(
    placed.top + 40 <= 500 - GRIP_REACH,
    `the bar's bottom (${placed.top + 40}) must clear the grip target's top (${500 - GRIP_REACH})`,
  );
});

test("an object at the top of the view does not push the bar behind the ribbon", () => {
  const at = objectBarPosition({ left: 200, top: 110, bottom: 240 }, { top: 100, bottom: 900 }, 40);
  assert.equal(at.top, 108, "clamped to the page view's own top, not the window's");
});

test("the bar is hidden once its object has scrolled out of the page view", () => {
  assert.equal(
    objectBarPosition({ left: 200, top: -300, bottom: -100 }, { top: 100, bottom: 900 }, 40),
    null,
    "scrolled off the top",
  );
  assert.equal(
    objectBarPosition({ left: 200, top: 950, bottom: 1100 }, { top: 100, bottom: 900 }, 40),
    null,
    "scrolled off the bottom",
  );
});

test("the bar never runs off the left edge", () => {
  const at = objectBarPosition({ left: -40, top: 500, bottom: 620 }, { top: 100, bottom: 900 }, 40);
  assert.equal(at.left, 8);
});
