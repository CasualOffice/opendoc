import assert from "node:assert/strict";
import test from "node:test";
import { scrollTargetFor } from "../src/scroll_into_view.mjs";

const viewport = { top: 100, bottom: 500, height: 400 };
const base = { viewport, current: 1000, max: 5000, scale: 1, block: "nearest", margin: 10 };

test("a marker already in view does not scroll at all", () => {
  const marker = { top: 200, bottom: 240, height: 40 };
  assert.equal(scrollTargetFor({ ...base, marker }), null);
});

test("a marker above the viewport scrolls up to it, with the margin", () => {
  const marker = { top: 60, bottom: 100, height: 40 };
  // delta = 60 - 100 - 10 = -50
  assert.equal(scrollTargetFor({ ...base, marker }), 950);
});

test("a marker below the viewport scrolls down to it, with the margin", () => {
  const marker = { top: 520, bottom: 560, height: 40 };
  // delta = 560 - 500 + 10 = 70
  assert.equal(scrollTargetFor({ ...base, marker }), 1070);
});

test("center ignores whether it is already in view and centres it", () => {
  const marker = { top: 200, bottom: 240, height: 40 };
  // marker centre 220, viewport centre 300 -> delta -80
  assert.equal(scrollTargetFor({ ...base, marker, block: "center" }), 920);
});

// The rule the extraction exists for.
test("a compressed scroll range divides the delta by the scale, or it OVERSHOOTS", () => {
  const marker = { top: 700, bottom: 740, height: 40 };
  // delta = 740 - 500 + 10 = 250
  assert.equal(scrollTargetFor({ ...base, marker, scale: 1 }), 1250, "uncompressed: the full delta");
  assert.equal(
    scrollTargetFor({ ...base, marker, scale: 5 }),
    1050,
    "compressed 5x: a 250px on-screen delta is 50 scroll pixels — undivided it would " +
      "overshoot fivefold, and above a scale of 2 that oscillates instead of converging",
  );
});

test("a scale below 1 is treated as 1, so a stretched range never multiplies the delta", () => {
  const marker = { top: 700, bottom: 740, height: 40 };
  assert.equal(scrollTargetFor({ ...base, marker, scale: 0.25 }), 1250);
});

test("the target is clamped into the scroller's real range at both ends", () => {
  const up = { top: -9000, bottom: -8960, height: 40 };
  assert.equal(scrollTargetFor({ ...base, marker: up }), 0, "never negative");
  const down = { top: 90000, bottom: 90040, height: 40 };
  assert.equal(scrollTargetFor({ ...base, marker: down }), 5000, "never past max");
});
