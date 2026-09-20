// The vertical goal column, as a state machine (HF-164).
//
// The engine cannot hold this state and the DOM is not involved in deciding
// it, so it is a plain object here and every rule is asserted directly: what
// sets the column, what keeps it, and what ends the run. The numbers are the
// ones the owner measured — a caret at x = 1456 on a long line, ArrowDown onto
// a short line that can only reach 392, then an empty paragraph at 346.
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  VERTICAL_PROBE_STEPS,
  createVerticalGoal,
  movedVertically,
  probeVerticalNeighbour,
  recoverVerticalMove,
  verticalProbePoints,
} from "../src/caret_navigation.mjs";

const LONG = { node: "p1", offset: 120 };
const SHORT = { node: "p2", offset: 8 };
const EMPTY = { node: "p3", offset: 0 };

/** A measurement that fails the test if the column is taken when it should
 *  have been carried — "never measured" is half of what these assert. */
function measures(value, log = []) {
  return {
    log,
    measure: () => {
      log.push(value);
      return value;
    },
  };
}

test("the first vertical move sets the column from the caret's own x", () => {
  const goal = createVerticalGoal();
  const { measure, log } = measures(1456);
  assert.equal(goal.columnFor("down", LONG, measure), 1456);
  assert.deepEqual(log, [1456], "the column has to be measured once");
});

test("a short line clamps the caret without taking the column", () => {
  const goal = createVerticalGoal();
  goal.keep(1456, LONG);

  // Down onto a short line: the engine is still told to aim at 1456 and the
  // caret is clamped to 392, which must NOT become the new column.
  const short = measures(392);
  assert.equal(goal.columnFor("down", LONG, short.measure), 1456);
  goal.keep(1456, SHORT);
  assert.deepEqual(short.log, [], "the caret's clamped x must not be consulted");

  // Down onto an empty paragraph: same again.
  const empty = measures(346);
  assert.equal(goal.columnFor("down", SHORT, empty.measure), 1456);
  goal.keep(1456, EMPTY);

  // …and the long line below them is entered at the original column.
  assert.equal(goal.columnFor("down", EMPTY, measures(346).measure), 1456);
});

test("Page Up and Page Down are vertical moves and keep the column", () => {
  const goal = createVerticalGoal();
  goal.keep(1456, LONG);
  for (const dir of ["pageDown", "pageUp", "up", "down"]) {
    assert.equal(goal.columnFor(dir, LONG, measures(392).measure), 1456, dir);
  }
});

test("a horizontal move has no column, and ends the run", () => {
  const goal = createVerticalGoal();
  goal.keep(1456, LONG);
  for (const dir of ["left", "right", "wordLeft", "wordRight", "lineStart", "lineEnd"]) {
    assert.equal(goal.columnFor(dir, LONG, measures(392).measure), null, dir);
  }
  // The caller passes that null straight back, which is what clears it: the
  // next vertical move measures again.
  goal.keep(null, LONG);
  assert.equal(goal.columnFor("down", LONG, measures(392).measure), 392);
});

test("a click or an edit clears the column", () => {
  const goal = createVerticalGoal();
  goal.keep(1456, LONG);
  goal.clear();
  assert.equal(goal.columnFor("down", LONG, measures(392).measure), 392);
});

test("a caret that moved elsewhere cannot inherit the column", () => {
  const goal = createVerticalGoal();
  goal.keep(1456, LONG);
  // Anything that puts the caret somewhere else — find, a command, a restored
  // draft — leaves the column behind whether or not it cleared it.
  assert.equal(goal.columnFor("down", SHORT, measures(392).measure), 392);
  // Same node, different offset: still not the caret the column belongs to.
  assert.equal(
    goal.columnFor("down", { node: LONG.node, offset: LONG.offset + 1 }, measures(500).measure),
    500,
  );
});

test("a position with no geometry yields no column rather than a bad one", () => {
  const goal = createVerticalGoal();
  assert.equal(
    goal.columnFor("down", LONG, () => null),
    null,
  );
  assert.equal(
    goal.columnFor("down", LONG, () => undefined),
    null,
  );
});

// ---- The rescue for a vertical move the engine could not make ----------------
//
// When the engine's answer is accepted, when it is overridden, and — the part
// that matters most — when nothing must happen at all, because a fallback that
// invents movement at the first or last line is worse than the dead end it
// replaces.

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
