// The floating-chrome arbiter's decisions (docs/184 D-12, audit CANVAS-06,
// CRITIC-10). The browser half is `e2e/overlay-arbiter.spec.mjs`.
import assert from "node:assert/strict";
import test from "node:test";

import {
  FLOATING_PRIORITY,
  arbitrate,
  miniToolbarOpacity,
  placeClear,
  rectsOverlap,
} from "../src/floating_arbiter.mjs";

test("of the surfaces that want to show, the decision's order picks one", () => {
  assert.equal(arbitrate([]), null);
  assert.equal(arbitrate(["selectionToolbar", "linkChip"]), "linkChip");
  assert.equal(arbitrate(["pasteOptions", "selectionToolbar"]), "selectionToolbar");
  assert.equal(arbitrate(["tableInsert", "selectionToolbar", "pasteOptions"]), "tableInsert");
  assert.equal(arbitrate(["objectBar", "tableInsert"]), "objectBar");
  // The order itself, so a reordering is a deliberate edit to a named decision.
  assert.deepEqual(
    Object.entries(FLOATING_PRIORITY).sort((a, b) => b[1] - a[1]).map(([id]) => id),
    ["linkChip", "objectBar", "tableInsert", "selectionToolbar", "pasteOptions"],
  );
});

test("a surface over the selection moves below it, else above it", () => {
  const bounds = { left: 8, top: 8, right: 1272, bottom: 792 };
  const selection = { left: 300, top: 400, right: 600, bottom: 440 };
  const size = { width: 250, height: 40 };
  // Overlapping: goes just below.
  assert.deepEqual(placeClear({ left: 320, top: 410 }, size, selection, bounds), { left: 320, top: 448 });
  // No room below: just above.
  const low = { left: 300, top: 740, right: 600, bottom: 780 };
  assert.deepEqual(placeClear({ left: 320, top: 745 }, size, low, bounds), { left: 320, top: 692 });
  // Already clear: untouched.
  assert.deepEqual(placeClear({ left: 320, top: 300 }, size, selection, bounds), { left: 320, top: 300 });
  // A selection taller than the window leaves the owner's placement alone.
  const tall = { left: 0, top: 0, right: 1280, bottom: 800 };
  assert.deepEqual(placeClear({ left: 320, top: 300 }, size, tall, bounds), { left: 320, top: 300 });
  assert.equal(rectsOverlap({ left: 0, top: 0, right: 10, bottom: 10 }, { left: 10, top: 0, right: 20, bottom: 10 }), false);
});

test("the mini-toolbar fades with the pointer's distance, as Word's does", () => {
  const bar = { left: 100, top: 100, right: 350, bottom: 140 };
  assert.equal(miniToolbarOpacity(bar, 200, 120), 1, "over it");
  assert.equal(miniToolbarOpacity(bar, 200, 160), 1, "near it");
  assert.equal(miniToolbarOpacity(bar, 200, 400), 0, "far from it");
  const mid = miniToolbarOpacity(bar, 200, 232);
  assert.ok(mid > 0 && mid < 1, `between: ${mid}`);
});
