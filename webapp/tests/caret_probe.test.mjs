// The decision behind "the arrow key did nothing, find the line myself".
//
// All geometry and model access is injected, so the rules are testable without
// a browser: when the engine's answer is accepted, when it is overridden, and —
// the part that matters most — when nothing must happen at all, because a
// fallback that invents movement at the first or last line is worse than the
// dead end it replaces.
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  VERTICAL_PROBE_STEPS,
  movedVertically,
  probeVerticalNeighbour,
  recoverVerticalMove,
  verticalProbePoints,
} from "../src/caret_probe.mjs";

const CARET = { left: 300, top: 500, bottom: 516 };

/** A fake document: a list of lines, each with a page, a y and a position. */
function engine(lines, { caretRect = () => CARET } = {}) {
  const byId = new Map(lines.map((l) => [`${l.node}:${l.offset}`, l]));
  return {
    calls: [],
    caretRect,
    // Resolves a probe point to the NEAREST line, which is what the real hit
    // test does: it clamps a point in a margin or a page gap into the page box
    // and answers with the closest caret rather than nothing.
    resolveAt(x, y) {
      this.calls.push(Math.round(y));
      let best = null;
      let distance = Infinity;
      for (const line of lines) {
        const d = Math.abs(line.clientTop + 8 - y);
        if (d < distance) {
          distance = d;
          best = line;
        }
      }
      return best ? { node: best.node, offset: best.offset } : null;
    },
    positionRect(position) {
      const line = byId.get(`${position.node}:${position.offset}`);
      return line ? { page: line.page, y: line.y } : null;
    },
  };
}

const LINES = [
  { node: "p1", offset: 0, page: 1, y: 100, clientTop: 300 },
  { node: "p2", offset: 0, page: 1, y: 200, clientTop: 400 },
  { node: "p3", offset: 0, page: 1, y: 300, clientTop: 500 }, // where the caret is
  { node: "p3", offset: 7, page: 1, y: 300, clientTop: 500 }, // same line, further along
  { node: "p4", offset: 0, page: 1, y: 400, clientTop: 600 },
];

test("probe points step away from the caret, nearest first", () => {
  const up = verticalProbePoints(CARET, "up");
  assert.equal(up.length, VERTICAL_PROBE_STEPS.length);
  assert.ok(up[0].y < CARET.top, "the first probe clears the current line");
  assert.ok(up[1].y < up[0].y, "and each one goes further");
  assert.equal(up[0].x, CARET.left, "the column is held");

  const down = verticalProbePoints(CARET, "down");
  assert.ok(down[0].y > CARET.bottom);
  assert.ok(down.at(-1).y > down[0].y);
});

test("the direction test uses page then y", () => {
  assert.equal(movedVertically({ page: 2, y: 50 }, { page: 1, y: 900 }, "up"), true);
  assert.equal(movedVertically({ page: 1, y: 300 }, { page: 1, y: 200 }, "up"), true);
  assert.equal(movedVertically({ page: 1, y: 300 }, { page: 1, y: 300 }, "up"), false);
  assert.equal(movedVertically({ page: 1, y: 300 }, { page: 2, y: 10 }, "up"), false);
  assert.equal(movedVertically(null, { page: 1, y: 1 }, "up"), false);
});

test("an engine move that really moved is left alone", () => {
  const io = engine(LINES);
  const result = recoverVerticalMove(
    "up",
    { node: "p3", offset: 0 },
    { node: "p2", offset: 0 },
    io,
  );
  assert.deepEqual(result, { node: "p2", offset: 0 });
  assert.deepEqual(io.calls, [], "no probing when the engine did its job");
});

test("a sideways 'move' to the same line is overridden", () => {
  // The defect's exact signature: `moveCaret(up)` returns a DIFFERENT model
  // position on the SAME line, so an id comparison says it moved while the
  // caret visibly has not.
  const io = engine(LINES);
  const result = recoverVerticalMove(
    "up",
    { node: "p3", offset: 7 },
    { node: "p3", offset: 0 },
    io,
  );
  assert.deepEqual(result, { node: "p2", offset: 0 });
  assert.ok(io.calls.length > 0, "it had to probe");
});

test("a dead end that returns the same position is overridden", () => {
  const io = engine(LINES);
  const from = { node: "p3", offset: 0 };
  assert.deepEqual(recoverVerticalMove("up", from, from, io), { node: "p2", offset: 0 });
  assert.deepEqual(recoverVerticalMove("down", from, from, engine(LINES)), {
    node: "p4",
    offset: 0,
  });
});

test("nothing above the first line means nothing happens", () => {
  // The fallback must not manufacture a move. `p1` is the top line; every probe
  // above it resolves to nothing, so the engine's own answer stands.
  const io = engine(LINES, { caretRect: () => ({ left: 300, top: 300, bottom: 316 }) });
  const from = { node: "p1", offset: 0 };
  assert.deepEqual(recoverVerticalMove("up", from, from, io), from);
  assert.equal(probeVerticalNeighbour("up", from, io), null);
});

test("a probe that resolves backwards is rejected", () => {
  // Hit tests clamp a point outside a page into that page's box, so a probe
  // above the first line can come back as a position BELOW the caret. Taking
  // it would send ArrowUp downwards.
  const io = engine(LINES, { caretRect: () => ({ left: 300, top: 300, bottom: 316 }) });
  io.resolveAt = () => ({ node: "p4", offset: 0 }); // always answers "further down"
  const from = { node: "p1", offset: 0 };
  assert.equal(probeVerticalNeighbour("up", from, io), null);
});

test("no painted caret means no guess", () => {
  const io = engine(LINES, { caretRect: () => null });
  const from = { node: "p3", offset: 0 };
  assert.equal(probeVerticalNeighbour("up", from, io), null);
});
