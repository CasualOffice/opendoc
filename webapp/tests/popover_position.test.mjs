import test from "node:test";
import assert from "node:assert/strict";
import { popoverAnchor, popoverPosition } from "../src/popover_position.mjs";

test("a visible command control replaces a hidden owning trigger", () => {
  const visible = { getClientRects: () => [{}] };
  const hidden = { getClientRects: () => [] };
  assert.equal(popoverAnchor(visible, hidden), visible);
  assert.equal(popoverAnchor(hidden, visible), visible);
});

test("a popover opens below its trigger and stays inside the viewport", () => {
  assert.deepEqual(
    popoverPosition(
      { left: 180, top: 40, bottom: 70 },
      { width: 240, height: 160 },
      { width: 800, height: 600 },
    ),
    { left: 180, top: 74 },
  );
  assert.deepEqual(
    popoverPosition(
      { left: 740, top: 520, bottom: 550 },
      { width: 240, height: 160 },
      { width: 800, height: 600 },
    ),
    { left: 552, top: 356 },
  );
});
