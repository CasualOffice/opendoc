// Measuring the rest of a long document between frames.
//
// `docs/116` §7. A document past the engine's open budget opens on a measured
// PREFIX: the pages it reports are real, but there are fewer of them than the
// document has, and the page count is marked inexact until the rest has been
// measured. This drives that rest.
//
// The whole design problem is that the work is on the main thread and cannot
// be interrupted once started, so the only lever is how much is started at
// once. A fixed block budget cannot be right: the same 4,000 blocks are 20 ms
// of a fast machine and 200 ms of a slow one, and 200 ms is a dropped frame
// and a click that does not land. So the budget is measured and adapted — the
// caller says how many milliseconds a tick may take, and this converges on the
// block count that costs that.
//
// No DOM and no engine: the ticker, the clock and the engine calls are all
// arguments. That is what lets the convergence be a unit test rather than
// something observed in a browser and hoped about.

/** Blocks measured on the first tick, before there is a rate to scale from.
 *  Small on purpose: the first tick is the one competing with the first
 *  paint, and guessing high there is the jank this exists to avoid. */
const FIRST_TICK_BLOCKS = 1_000;

/** Bounds on the adapted budget. The floor keeps a pathologically slow
 *  document (or a throttled background tab) from converging on a budget so
 *  small it never finishes; the ceiling keeps a fast machine from taking a
 *  bite big enough to be felt if the estimate is wrong. */
const MIN_TICK_BLOCKS = 200;
const MAX_TICK_BLOCKS = 50_000;

/**
 * Drives `extend` from idle time until the document is measured whole.
 *
 * @param {object} io
 * @param {(blocks: number) => boolean} io.extend  Measures that many more
 *   blocks; returns whether the document is now complete.
 * @param {() => void} [io.onProgress]  After every tick that measured
 *   something and did not finish.
 * @param {() => void} [io.onDone]  Once, after the tick that completed it.
 * @param {(run: () => void) => unknown} io.schedule  Queues the next tick.
 * @param {() => number} io.now  Monotonic milliseconds.
 * @param {number} [io.budgetMs]  Main thread one tick may take.
 * @returns {{start: () => void, stop: () => void, tickBlocks: () => number}}
 */
export function createBackgroundMeasure({
  extend,
  onProgress,
  onDone,
  schedule,
  now,
  budgetMs = 30,
}) {
  let blocks = FIRST_TICK_BLOCKS;
  let running = false;
  let stopped = false;

  function tick() {
    if (stopped) return;
    const started = now();
    const complete = extend(blocks);
    const elapsed = now() - started;

    // Scale toward the budget from the rate this tick actually measured.
    // Guarded against a zero reading: a tick that the clock says took no time
    // must not multiply the budget by infinity.
    if (elapsed > 0) {
      const scaled = Math.round((blocks * budgetMs) / elapsed);
      blocks = Math.min(MAX_TICK_BLOCKS, Math.max(MIN_TICK_BLOCKS, scaled));
    } else {
      blocks = Math.min(MAX_TICK_BLOCKS, blocks * 2);
    }

    if (complete) {
      running = false;
      stopped = true;
      onDone?.();
      return;
    }
    onProgress?.();
    schedule(tick);
  }

  return {
    /** Begins, unless it is already running or has finished. Idempotent, so a
     *  caller can arm it from more than one place without tracking which. */
    start() {
      if (running || stopped) return;
      running = true;
      schedule(tick);
    },
    /** Abandons the run — the document was closed or replaced, and the engine
     *  the ticks would call is about to be freed. */
    stop() {
      running = false;
      stopped = true;
    },
    /** The block budget the next tick will use. Exposed for the guard: the
     *  adaptation is the thing worth asserting and it is otherwise invisible. */
    tickBlocks() {
      return blocks;
    },
  };
}

/**
 * The total to show in the page indicator, and whether it is approximate.
 *
 * Kept here rather than inlined at the one call site because it is the rule
 * `AGENTS.md` cares about — an estimate is never presented as exact — and a
 * rule stated in one function can be tested; the same rule spelled out at each
 * surface is the one that gets forgotten at the next surface.
 */
export function pageTotalLabel(total, estimate, exact) {
  return exact ? String(total) : `~${Math.max(estimate, total)}`;
}
