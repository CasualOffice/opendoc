// The background measure is a state machine over a clock, so it is tested as
// one. The behaviour that matters is not "it finishes" — a `while` loop
// finishes — it is that no single tick blocks the main thread for longer than
// the budget, which is the only reason this exists instead of a loop.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  createBackgroundMeasure,
  pageTotalLabel,
} from "../src/background_measure.mjs";

/** A fake engine: `blocksPerMs` blocks of work take one millisecond. */
function engine(totalBlocks, blocksPerMs) {
  let measured = 0;
  let clock = 0;
  const ticks = [];
  return {
    now: () => clock,
    extend(blocks) {
      const took = Math.min(blocks, totalBlocks - measured);
      measured += took;
      clock += took / blocksPerMs;
      ticks.push({ blocks, ms: took / blocksPerMs });
      return measured >= totalBlocks;
    },
    ticks,
    measured: () => measured,
  };
}

function driver(totalBlocks, blocksPerMs, budgetMs = 30) {
  const eng = engine(totalBlocks, blocksPerMs);
  const queue = [];
  let done = false;
  const measure = createBackgroundMeasure({
    extend: eng.extend,
    onDone: () => {
      done = true;
    },
    schedule: (run) => queue.push(run),
    now: eng.now,
    budgetMs,
  });
  measure.start();
  let rounds = 0;
  while (queue.length) {
    if (++rounds > 10_000)
      throw new Error("the background measure did not terminate");
    queue.shift()();
  }
  return { eng, done, rounds };
}

test("it measures the whole document", () => {
  const { eng, done } = driver(120_000, 40);
  assert.equal(eng.measured(), 120_000);
  assert.equal(done, true);
});

test("no tick spends more than about the budget on the main thread", () => {
  // 40 blocks/ms is the ~25 µs/block a slow machine measured. The first tick
  // is the fixed 1,000-block probe; every tick after it has a measured rate
  // to size itself from, and none of them may blow the budget.
  const { eng } = driver(400_000, 40, 30);
  const after = eng.ticks.slice(1);
  const worst = Math.max(...after.map((t) => t.ms));
  assert.ok(
    worst <= 30 * 1.5,
    `a tick took ${worst.toFixed(1)} ms against a 30 ms budget; the adaptation ` +
      `is not holding (ticks: ${after.map((t) => t.ms.toFixed(1)).join(", ")})`,
  );
});

test("a fast machine takes bigger bites than a slow one", () => {
  // The point of adapting at all. Same document, same budget, 25x the speed:
  // the fast machine must do it in far fewer ticks, not in the same number of
  // deliberately small ones.
  const slow = driver(400_000, 20);
  const fast = driver(400_000, 500);
  assert.ok(
    fast.rounds * 4 < slow.rounds,
    `adaptation is not happening: ${fast.rounds} ticks fast vs ${slow.rounds} slow`,
  );
});

test("a clock with no resolution does not produce an infinite budget", () => {
  // `performance.now()` can read the same value twice across a short tick.
  // Dividing by that elapsed time is a division by zero, and a budget of
  // Infinity blocks is the freeze this whole change exists to remove.
  const eng = engine(100_000, 40);
  const queue = [];
  const measure = createBackgroundMeasure({
    extend: (blocks) => eng.extend(blocks),
    schedule: (run) => queue.push(run),
    now: () => 0, // frozen
    budgetMs: 30,
  });
  measure.start();
  for (let i = 0; i < 50 && queue.length; i++) {
    queue.shift()();
    assert.ok(
      Number.isFinite(measure.tickBlocks()) && measure.tickBlocks() <= 50_000,
      `budget ran away to ${measure.tickBlocks()}`,
    );
  }
});

test("stop abandons the run", () => {
  const eng = engine(100_000, 40);
  const queue = [];
  const measure = createBackgroundMeasure({
    extend: eng.extend,
    schedule: (run) => queue.push(run),
    now: eng.now,
  });
  measure.start();
  queue.shift()();
  measure.stop();
  while (queue.length) queue.shift()();
  assert.ok(eng.measured() < 100_000, "stop must stop it");
});

test("start is idempotent", () => {
  const eng = engine(10_000, 40);
  const queue = [];
  const measure = createBackgroundMeasure({
    extend: eng.extend,
    schedule: (run) => queue.push(run),
    now: eng.now,
  });
  measure.start();
  measure.start();
  measure.start();
  assert.equal(
    queue.length,
    1,
    "arming it three times must not run three loops",
  );
});

test("an inexact total is never shown as exact", () => {
  assert.equal(pageTotalLabel(50, 29_600, false), "~29600");
  assert.equal(pageTotalLabel(29_621, 29_621, true), "29621");
  // An estimate that has fallen behind the pages actually measured still
  // reads as at-least, never as a number below what the reader can scroll to.
  assert.equal(pageTotalLabel(80, 50, false), "~80");
});
