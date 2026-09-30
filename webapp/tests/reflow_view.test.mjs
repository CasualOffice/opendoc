// The two decisions reflow is made of, asked in `node` against a fake clock.
//
// `docs/151` §6.2 and §5. The paint-tier claims — no horizontal scroll at 390px,
// the document still editable, Paged -> Reflow -> Paged returning the same
// layout — are `tests/e2e/reflow.spec.mjs`'s, because a model-level assertion
// has passed through every real defect in this repository. What is HERE is the
// cost model, which a browser cannot assert without waiting on a wall clock:
// how many O(document) passes a gesture buys, and how many it does not.
//
// Every test below can be driven red by changing one line of `reflow_view.mjs`;
// the mutations are recorded in the commit message.
import assert from "node:assert/strict";
import test from "node:test";

import { TWIPS_PER_INCH } from "../src/units.mjs";
import {
  REFLOW_DEBOUNCE_MS,
  REFLOW_GUTTER_PX,
  REFLOW_MIN_CONTENT_TWIP,
  REFLOW_QUANTUM_PX,
  createWidthFeed,
  quantiseReflowWidth,
  reflowAvailability,
  reflowMeasure,
} from "../src/reflow_view.mjs";

/** CSS px per twip at 100%, exactly as `renderAll` computes it. */
const AT_100 = 96 / TWIPS_PER_INCH;

// ---- Quantisation -----------------------------------------------------------

test("a bucket is never wider than the width it was measured from", () => {
  // THE CLAIM THAT MATTERS: the column can only ever be narrower than the space
  // it has. Rounding to the NEAREST bucket — which is what `docs/151` §6.2
  // proposed — rounds up half the time, and a column wider than the viewport is
  // a horizontal scroll on `#viewport`, the exact defect reflow retires.
  for (let px = 1; px <= 2000; px += 1) {
    const bucket = quantiseReflowWidth(px);
    assert.ok(bucket <= px, `${px} -> ${bucket} is wider than the space it has`);
    assert.ok(px - bucket < REFLOW_QUANTUM_PX, `${px} -> ${bucket} lost more than a quantum`);
    assert.equal(bucket % REFLOW_QUANTUM_PX, 0);
  }
});

test("the quantum is wide enough to absorb a scrollbar appearing mid-gesture", () => {
  // Why 16 and not the 8 the design proposed. The slack the quantum leaves is
  // what has to swallow a scrollbar that appears while the reader is dragging —
  // 15px on Windows, 17px on a desktop GTK theme — before it becomes an
  // overflow. This is the number, asserted where it is reasoned about.
  assert.ok(REFLOW_QUANTUM_PX >= 15, "a quantum under 15px cannot cover a classic scrollbar");
});

test("a width that is not a usable number is refused rather than guessed at", () => {
  for (const bad of [0, -1, NaN, Infinity, undefined, null, "wide"]) {
    assert.equal(quantiseReflowWidth(bad), 0, `${String(bad)} should produce no bucket`);
  }
});

// ---- The measure ------------------------------------------------------------

test("the tile's TOTAL width is the bucket, gutters included", () => {
  // Getting this the other way round — making the CONTENT the bucket and adding
  // the gutters on top — is how a gutter becomes an overflow. The engine's
  // synthetic page is `content + 2 * gutter` wide, and that is what gets
  // painted, so that is what has to fit.
  const measure = reflowMeasure(390, AT_100);
  const paintedPx = (measure.contentWidthTwip + 2 * measure.gutterTwip) * AT_100;
  assert.ok(measure.totalPx <= 390);
  assert.ok(
    paintedPx <= measure.totalPx + 1,
    `${paintedPx}px painted into a ${measure.totalPx}px bucket`,
  );
});

test("the painted tile fits the window at every phone width and every zoom step", () => {
  // The guarantee stated as a property rather than as one example. A quarter of
  // a pixel of tolerance for the twip rounding; anything more than that is a
  // scrollbar waiting for a slow enough machine.
  for (const width of [320, 360, 390, 412, 430, 620, 768, 1280]) {
    for (const zoom of [0.5, 0.75, 0.9, 1, 1.25, 1.5, 2, 3]) {
      const cssPerTwip = (96 * zoom) / TWIPS_PER_INCH;
      const measure = reflowMeasure(width, cssPerTwip);
      const paintedPx = (measure.contentWidthTwip + 2 * measure.gutterTwip) * cssPerTwip;
      if (measure.contentWidthTwip === REFLOW_MIN_CONTENT_TWIP) continue; // the floor, below
      assert.ok(
        paintedPx <= width + 0.25,
        `${width}px at ${zoom * 100}%: painted ${paintedPx}px`,
      );
    }
  }
});

test("below the engine's floor the column stops shrinking, and says so by its value", () => {
  // At a large enough zoom an inch of text no longer fits the window. The engine
  // refuses a column under an inch at the seam, so the shell stops shrinking
  // rather than sending a refusal — the same arbitration `docs/151` §6.3 gives a
  // table too wide to fit, and honest for the same reason: what is on screen
  // really is wider than the screen. The no-horizontal-scroll guarantee is
  // stated at the zoom a phone opens at, which `FIT_ON_OPEN_FLOOR` pins at 100%.
  const at400 = (96 * 4) / TWIPS_PER_INCH;
  const measure = reflowMeasure(390, at400);
  assert.equal(measure.contentWidthTwip, REFLOW_MIN_CONTENT_TWIP);
  assert.equal(measure.gutterTwip, 0, "a gutter wider than its column is what the seam refuses");
});

test("a viewport that has not been laid out yet produces no measure at all", () => {
  assert.equal(reflowMeasure(0, AT_100), null);
  assert.equal(reflowMeasure(390, 0), null);
});

test("the gutter is the same number of PIXELS at every zoom", () => {
  // The gutter is chrome, not content: it should look the same whatever the text
  // is doing. It is declared in px and converted per zoom, which is what makes
  // that true — and it is asserted because converting it once at 100% and
  // reusing the twips would make it grow with the zoom.
  for (const zoom of [0.5, 1, 2]) {
    const cssPerTwip = (96 * zoom) / TWIPS_PER_INCH;
    const { gutterTwip } = reflowMeasure(1280, cssPerTwip);
    assert.ok(
      Math.abs(gutterTwip * cssPerTwip - REFLOW_GUTTER_PX) < 1,
      `${zoom * 100}%: gutter painted at ${gutterTwip * cssPerTwip}px`,
    );
  }
});

// ---- Availability -----------------------------------------------------------

test("reflow is refused exactly where the engine refuses it, with a sentence", () => {
  assert.deepEqual(reflowAvailability("", "no"), { available: true, reason: "" });
  const windowed = reflowAvailability("This document is open one page-window at a time.", "no");
  assert.deepEqual(windowed, { available: false, reason: "no" });
  // A control that is off must carry WHY. "Never a dead control" is the rule and
  // an empty reason is how it gets broken quietly.
  assert.notEqual(windowed.reason, "");
});

// ---- The width feed: what a gesture costs -----------------------------------

/** A feed driven by a fake clock, so the coalescing is a question about
 *  behaviour rather than about how long a test is willing to sleep. */
function feedOnAFakeClock() {
  const passes = [];
  let timers = [];
  const feed = createWidthFeed({
    onSettled: (px) => passes.push(px),
    setTimer: (fn, ms) => {
      const handle = { fn, ms };
      timers.push(handle);
      return handle;
    },
    clearTimer: (handle) => {
      timers = timers.filter((t) => t !== handle);
    },
  });
  return {
    feed,
    passes,
    /** Fire every timer that is due, exactly as a settled gesture would. */
    settle() {
      const due = timers;
      timers = [];
      for (const timer of due) timer.fn();
    },
    scheduled: () => timers.length,
  };
}

test("a drag inside one bucket costs no document work at all", () => {
  // THE O(1) GUARANTEE (`docs/107` §4, an owner constraint). Sixty resize events
  // that do not cross a bucket boundary must produce zero relayouts and zero
  // scheduled ones — not "one, eventually", which is what a debounce alone
  // gives. Quantising BEFORE debouncing is what buys this.
  const { feed, passes, scheduled } = feedOnAFakeClock();
  feed.adopt(320);
  for (let i = 0; i < 60; i += 1) {
    assert.equal(feed.observe(320 + (i % 15)), false, `event ${i} was not free`);
  }
  assert.deepEqual(passes, []);
  assert.equal(scheduled(), 0, "nothing should even be pending");
});

test("a drag ACROSS buckets is coalesced into one pass at its final width", () => {
  // The worst case, bounded. A slow drag from 390 to 1280 crosses 56 buckets and
  // must still cost exactly one O(document) re-shape, at the width the reader
  // stopped on — not 56, and not the width they started from.
  const { feed, passes, settle } = feedOnAFakeClock();
  feed.adopt(390);
  for (let px = 391; px <= 1280; px += 1) feed.observe(px);
  assert.deepEqual(passes, [], "nothing may run while the gesture is still moving");
  settle();
  assert.deepEqual(passes, [quantiseReflowWidth(1280)]);
});

test("a drag out and back costs nothing, because nothing changed", () => {
  // Found by writing this test, and it was a real defect: the first version left
  // a pass scheduled by an earlier width standing when the reader came back to
  // the width the engine already held, so a drag out and back relayed the
  // document out to the width it no longer was. The LAST width wins, including
  // when the last width is the one already in effect — at which point the honest
  // answer is no pass at all.
  const { feed, passes, settle, scheduled } = feedOnAFakeClock();
  feed.adopt(320);
  for (const px of [400, 500, 600, 500, 400, 320]) feed.observe(px);
  assert.equal(scheduled(), 0, "a gesture that ended where it began must leave nothing pending");
  settle();
  assert.deepEqual(passes, []);
});

test("two settled gestures are two passes, not one", () => {
  // The debounce coalesces a gesture; it must not swallow a second one, or a
  // phone rotated twice would be laid out for its first orientation.
  const { feed, passes, settle } = feedOnAFakeClock();
  feed.adopt(390);
  feed.observe(844);
  settle();
  feed.adopt(844);
  feed.observe(390);
  settle();
  assert.deepEqual(passes, [quantiseReflowWidth(844), quantiseReflowWidth(390)]);
});

test("the debounce is long enough to cover a gesture and short enough to feel attached", () => {
  assert.ok(REFLOW_DEBOUNCE_MS >= 100, "under 100ms a drag settles mid-gesture");
  assert.ok(REFLOW_DEBOUNCE_MS <= 200, "over 200ms the view stops feeling attached to the drag");
});

// ---- Print puts the paper back ---------------------------------------------
//
// `docs/151` §6.4: printing what is on screen would print TILES — sheets the
// width of somebody's browser, boundaries mid-paragraph, and no header, footer,
// page border or watermark, because reflow suppresses all four. In the browser
// this is only reachable through a print dialog that blocks the renderer
// (`ribbon-command-faces.spec.mjs` calls `file.print` pointer-unsafe for exactly
// that reason), so the guarantee is asserted here against a stand-in engine.

/** An engine that records what it was told, and reports the view it holds in the
 *  same JSON shape `WasmDocument::layoutView` returns. */
function engineInReflow() {
  const calls = [];
  let view = { reflow: true, contentWidthTwip: 8000, tileHeightTwip: 15840, gutterTwip: 240 };
  return {
    calls,
    get layoutView() {
      return JSON.stringify(view);
    },
    setLayoutView(contentWidthTwip, tileHeightTwip, gutterTwip) {
      calls.push([contentWidthTwip, tileHeightTwip, gutterTwip]);
      view =
        contentWidthTwip > 0
          ? { reflow: true, contentWidthTwip, tileHeightTwip, gutterTwip }
          : { reflow: false, contentWidthTwip: null, tileHeightTwip: null, gutterTwip: null };
      return JSON.stringify({ reflow: view.reflow, approximations: [] });
    },
  };
}

test("printing goes to paper and comes back to the reader's view", async () => {
  const { withPagedLayout } = await import("../src/print.mjs");
  const doc = engineInReflow();
  let sawReflowWhilePrinting = true;
  await withPagedLayout(doc, () => {
    sawReflowWhilePrinting = JSON.parse(doc.layoutView).reflow;
  });
  assert.equal(sawReflowWhilePrinting, false, "the printer was handed tiles, not paper");
  assert.deepEqual(doc.calls, [
    [0, 0, 0],
    [8000, 15840, 240],
  ]);
  assert.equal(JSON.parse(doc.layoutView).reflow, true, "the reader was left on paper");
});

test("a printer that throws still gives the reader their view back", async () => {
  // The restore is in a `finally` because the alternative — stranding a phone
  // reader on a 794px page after a failed print — is a worse failure than the
  // print failing.
  const { withPagedLayout } = await import("../src/print.mjs");
  const doc = engineInReflow();
  await assert.rejects(
    withPagedLayout(doc, () => {
      throw new Error("no printer");
    }),
    /no printer/,
  );
  assert.equal(JSON.parse(doc.layoutView).reflow, true);
});

test("a document that was never reflowed is not touched at all", async () => {
  const { withPagedLayout } = await import("../src/print.mjs");
  const doc = { layoutView: JSON.stringify({ reflow: false }), setLayoutView: () => assert.fail("no") };
  await withPagedLayout(doc, () => {});
});
