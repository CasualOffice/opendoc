// Edit Points (`shape_edit_points.mjs`): the arithmetic a drag runs (the
// right-click row is in `object_context_menu.test.mjs`). The engine half (convert, write, undo) is
// held by `casual-doc-wasm`'s `shape_points` tests.
import { test } from "node:test";
import assert from "node:assert/strict";
import { moveVertex, outlinePath, toPlaced, toShapeDelta, vertices } from "../src/shape_edit_points.mjs";

/** A triangle drawn back onto its start before closing, as presets are. */
const TRIANGLE = {
  widthEmu: 1000,
  heightEmu: 500,
  flipH: false,
  flipV: false,
  rotation: 0,
  paths: [
    {
      fill: "norm",
      stroke: true,
      commands: [
        { type: "moveTo", points: [[0, 500]] },
        { type: "lineTo", points: [[500, 0]] },
        { type: "lineTo", points: [[1000, 500]] },
        { type: "lineTo", points: [[0, 500]] },
        { type: "close", points: [] },
      ],
    },
  ],
};

/** A curve: a vertex with a cubic on each side. */
const CURVE = {
  widthEmu: 100,
  heightEmu: 100,
  paths: [
    {
      commands: [
        { type: "moveTo", points: [[0, 0]] },
        { type: "cubicTo", points: [[10, 0], [40, 40], [50, 50]] },
        { type: "cubicTo", points: [[60, 60], [90, 100], [100, 100]] },
      ],
    },
  ],
};

test("every command with a point has a vertex; close has none", () => {
  assert.deepEqual(vertices(TRIANGLE).map((v) => v.command), [0, 1, 2, 3]);
});

test("moving a vertex moves only it, leaving the input untouched", () => {
  const moved = moveVertex(TRIANGLE, { path: 0, command: 1 }, 100, 50);
  assert.deepEqual(moved.paths[0].commands[1].points, [[600, 50]]);
  assert.deepEqual(moved.paths[0].commands[2].points, [[1000, 500]]);
  assert.deepEqual(TRIANGLE.paths[0].commands[1].points, [[500, 0]], "pure");
});

test("a closed outline's start and its closing twin move together, from either end", () => {
  const fromStart = moveVertex(TRIANGLE, { path: 0, command: 0 }, 10, -10);
  assert.deepEqual(fromStart.paths[0].commands[0].points, [[10, 490]]);
  assert.deepEqual(fromStart.paths[0].commands[3].points, [[10, 490]]);
  const fromEnd = moveVertex(TRIANGLE, { path: 0, command: 3 }, 10, -10);
  assert.deepEqual(fromEnd.paths[0].commands[0].points, [[10, 490]]);
});

test("a vertex between two curves carries both of its control points", () => {
  const moved = moveVertex(CURVE, { path: 0, command: 1 }, 5, 5);
  assert.deepEqual(moved.paths[0].commands[1].points, [[10, 0], [45, 45], [55, 55]]);
  assert.deepEqual(moved.paths[0].commands[2].points[0], [65, 65]);
  assert.deepEqual(moved.paths[0].commands[2].points.slice(1), [[90, 100], [100, 100]]);
});

test("points map onto the placed box, mirrored by a flip, and pointer deltas map back", () => {
  const box = [1000, 2000, 500, 250]; // twips
  assert.deepEqual(toPlaced([500, 0], TRIANGLE, box), [1250, 2000]);
  const flipped = { ...TRIANGLE, flipH: true };
  assert.deepEqual(toPlaced([0, 500], flipped, box), [1500, 2250]);
  assert.deepEqual(toShapeDelta(50, 25, TRIANGLE, box), [100, 50]);
  assert.deepEqual(toShapeDelta(50, 25, flipped, box), [-100, 50]);
});

test("the preview outline is one SVG path in CSS pixels", () => {
  const d = outlinePath(TRIANGLE, [0, 0, 1000, 500], { sx: 0.1, sy: 0.1 });
  assert.equal(d, "M 0.00 50.00 L 50.00 0.00 L 100.00 50.00 L 0.00 50.00 Z");
});
