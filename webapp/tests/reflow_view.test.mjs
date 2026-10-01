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

import { readFileSync } from "node:fs";

import { TWIPS_PER_INCH } from "../src/units.mjs";
import {
  FULL_WIDTH_ADVANCE_EM,
  REFLOW_DEBOUNCE_MS,
  REFLOW_GUTTER_PX,
  REFLOW_MAX_CONTENT_TWIP,
  REFLOW_MIN_CONTENT_TWIP,
  REFLOW_QUANTUM_PX,
  REFLOW_WIDTH_DEFAULT,
  REFLOW_WIDTH_STEPS,
  charTargetTwip,
  createWidthFeed,
  quantiseReflowWidth,
  reflowAvailability,
  reflowCapTwip,
  reflowMeasure,
  reflowWidthStep,
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

// ---- The CAP (`docs/154` §5, ADR-048) ---------------------------------------
//
// These are the measure-tier claims. The PAINT-tier ones — a capped, centred
// column at 1440px, no horizontal scroll at 390px in either policy, a viewport
// past the old 22in threshold laying out rather than refusing, and
// Reading -> Fit -> Reading returning the same layout — are
// `tests/e2e/reflow.spec.mjs`'s, because a pure-function assertion about a width
// says nothing about what a reader sees.

/** The default face's numbers, as `stylePreview("Normal")` reports them for a
 *  document that has not overridden `docDefaults` — Word's own default. */
const CALIBRI_11 = { face: "Calibri", fontSizePt: 11 };

test("the published cap is 80 characters, and in twips it is the published number", () => {
  // The two numbers `docs/154` §5.1 and ADR-048 publish, pinned where they are
  // computed. 80 x 0.3991em x 11pt x 20 twips/pt = 7,020 twips = 468 CSS px at
  // 100%. If the mean advance, the target or the arithmetic moves, the published
  // figure moves with it and this is where that is noticed.
  //
  // 7,024 and not `docs/154` §5.1's 7,020: that figure was 468 CSS px converted
  // BACK to twips, so it carried the pixel rounding twice. 468 px is right to the
  // pixel and 7,024 is the exact twip value; `154` and ADR-048 are corrected in
  // place rather than this guard being bent to a double-rounded number.
  assert.equal(reflowWidthStep("reading").chars, 80, "WCAG 2.1 SC 1.4.8's number");
  const cap = reflowCapTwip("reading", CALIBRI_11);
  assert.equal(cap, 7024, "the Reading cap in twips");
  assert.equal(Math.round(cap * (96 / TWIPS_PER_INCH)), 468, "the Reading cap in CSS px");
});

test("an unknown face falls back to 0.40 em per character, with its error stated", () => {
  // The fallback is an approximation and is allowed to be one; what it may not be
  // is far enough out to matter. Asserted as a BOUND against the measured spread
  // (0.393-0.431 em across the four bundled base text faces) rather than as an
  // equality, because the claim being made is "within a few per cent", not "this
  // number".
  const measured = reflowCapTwip("reading", CALIBRI_11);
  const unknown = reflowCapTwip("reading", { face: "Nonesuch Display", fontSizePt: 11 });
  assert.ok(Math.abs(unknown - measured) / measured < 0.05, `${unknown} vs ${measured}`);
  for (const face of ["Times New Roman", "Cambria", "Arial"]) {
    const other = reflowCapTwip("reading", { face, fontSizePt: 11 });
    assert.ok(Math.abs(other - unknown) / unknown < 0.08, `${face}: ${other} vs ${unknown}`);
  }
});

test("a full-width face gets WCAG's 40 glyphs, not its 80 characters", () => {
  // The CJK branch, driven by MEASUREMENT rather than by a family name: an
  // ideograph is one em wide, so a face whose mean advance is near 1.0 em is
  // full-width and 1.4.8's 40 applies. REACHABLE BUT NOT REACHED in the shipped
  // product — this engine bundles no CJK face, so no entry in the advance table
  // is full-width and the Latin fallback is what a CJK document gets today. That
  // is a font-provisioning gap, recorded as one; the rule is exercised here so it
  // is a live branch and not a comment.
  const fullWidth = charTargetTwip(40, 11, 1.0);
  const latin = charTargetTwip(80, 11, 0.3991);
  assert.ok(FULL_WIDTH_ADVANCE_EM > 0.431, "a Latin text face must not read as full-width");
  assert.ok(FULL_WIDTH_ADVANCE_EM < 1.0, "an ideographic face must");
  assert.equal(fullWidth, 8800);
  assert.ok(fullWidth > latin, "40 full-width glyphs are wider than 80 Latin characters");
});

test("the measure is capped at 1440px, and the cap is what gets painted", () => {
  // THE DEFECT, stated as its own guard. `docs/154` §3.2: 1,408 CSS px and 241
  // characters at a 1440px window, measured from the shipped formula. With the
  // cap the column is 7,020 twips whatever the window does above it.
  const cap = reflowCapTwip("reading", CALIBRI_11);
  const capped = reflowMeasure(1440, AT_100, { capTwip: cap });
  assert.equal(capped.contentWidthTwip, cap);
  assert.equal(capped.capped, true);
  // And it is narrower than the space it has, which is what "centre it on the
  // desk" needs to be true of the tile.
  assert.ok(capped.totalPx < capped.availablePx, `${capped.totalPx} vs ${capped.availablePx}`);

  // Characters, the unit every source states its answer in. 241 -> 80.
  const perChar = 0.3991 * 11 * 20; // twips per character of 11pt Calibri
  const uncapped = reflowMeasure(1440, AT_100);
  assert.equal(Math.round(uncapped.contentWidthTwip / perChar), 241, "the defect, reproduced");
  assert.equal(Math.round(capped.contentWidthTwip / perChar), 80, "and closed");
});

test("every step is min(available, X) — one mechanism, four values", () => {
  // Monotonic, and each one no wider than the space available. A step that could
  // produce a column WIDER than the window would be the horizontal scroll reflow
  // exists to retire, reintroduced through the control.
  const docMeasureTwip = 9360; // a Letter page's own 6.5in text column
  const widths = REFLOW_WIDTH_STEPS.map((step) => {
    const cap = reflowCapTwip(step.id, { ...CALIBRI_11, docMeasureTwip });
    const measure = reflowMeasure(1920, AT_100, { capTwip: cap });
    return [step.id, measure.contentWidthTwip];
  });
  const values = widths.map(([, twip]) => twip);
  assert.deepEqual(
    values,
    [...values].sort((a, b) => a - b),
    `the steps must widen in order: ${JSON.stringify(widths)}`,
  );
  const available = reflowMeasure(1920, AT_100, {}).contentWidthTwip;
  for (const [id, twip] of widths) {
    assert.ok(twip <= available, `${id} asked for ${twip} of an available ${available}`);
  }
  // Fit comes from the DOCUMENT and invents no constant: 6.5in of text column.
  assert.equal(reflowCapTwip("fit", { docMeasureTwip }), docMeasureTwip);
  // And Reading is narrower than the paper, which is the whole point — a Letter
  // page's own column is 107 characters, already past WCAG's 80.
  assert.ok(reflowCapTwip("reading", CALIBRI_11) < docMeasureTwip);
});

test("the desktop default is Reading, which is the one line that changes it", () => {
  // The call `docs/154` §5 left open and ADR-048 records as the owner's. Named
  // here so that changing it is a visible, single-line decision rather than a
  // silent drift, and so the owner overruling it moves one assertion with it.
  assert.equal(REFLOW_WIDTH_DEFAULT, "reading");
  assert.equal(reflowWidthStep(null).id, "reading", "never chosen resolves to the default");
  assert.equal(reflowWidthStep("nonsense").id, "reading", "and so does a bad preference");
});

test("a viewport past the old 22in refusal lays out instead of being refused", () => {
  // DEFECT TWO (`docs/154` §3.3), which is the one that must not come back.
  // `LayoutView::reflow` refuses `content_width > 31,680` twips; with no ceiling
  // upstream, `reflowMeasure` asked for more at 2,160px/100% and at 1,104px/50%,
  // and `sync` then reverted the reader to paper with a message about twips.
  //
  // Asserted for EVERY step including `full`, because the cap only removes the
  // refusal for the steps that cap — and a named Full that threw would be the
  // same defect reached through the new control.
  const cases = [
    [2160, 1],
    [2560, 1],
    [3840, 1],
    [1104, 0.5],
    [1440, 0.5],
    [1632, 0.75],
    [3216, 1.5],
  ];
  for (const [width, zoom] of cases) {
    const cssPerTwip = (96 * zoom) / TWIPS_PER_INCH;
    for (const step of REFLOW_WIDTH_STEPS) {
      const cap = reflowCapTwip(step.id, { ...CALIBRI_11, docMeasureTwip: 9360 });
      const measure = reflowMeasure(width, cssPerTwip, { capTwip: cap });
      assert.ok(
        measure.contentWidthTwip <= REFLOW_MAX_CONTENT_TWIP,
        `${step.id} at ${width}px/${zoom * 100}% asks the engine for ` +
          `${measure.contentWidthTwip} twips, which it refuses above ${REFLOW_MAX_CONTENT_TWIP}`,
      );
    }
  }
  // And the thresholds themselves, derived from the shipped formula rather than
  // quoted from `docs/154` §3.3: with the ceiling lifted — which is what the code
  // did before this change — 2,160px at 100% and 1,104px at 50% both ask the
  // engine for more than 22in, and one quantum lower neither does. That is the
  // defect reproduced and its boundary pinned, in the same test that proves the
  // ceiling closes it.
  const lifted = (px, zoom) =>
    reflowMeasure(px, (96 * zoom) / TWIPS_PER_INCH, { maxContentTwip: Infinity }).contentWidthTwip;
  for (const [px, zoom] of [
    [2160, 1],
    [1104, 0.5],
    [1632, 0.75],
    [3216, 1.5],
  ]) {
    assert.ok(lifted(px, zoom) > REFLOW_MAX_CONTENT_TWIP, `${px}px at ${zoom * 100}% was refused`);
    assert.ok(
      lifted(px - REFLOW_QUANTUM_PX, zoom) <= REFLOW_MAX_CONTENT_TWIP,
      `${px}px at ${zoom * 100}% is not the FIRST width refused`,
    );
  }
});

test("the phone is untouched: at 390px the default is still the whole window", () => {
  // The guard the missing cap needed and did not have. A phone-only evaluation is
  // exactly what hid the defect (`docs/154` §3.2), so the fix gets a phone-only
  // guard of its own in the other direction.
  //
  // THREE of the four steps reduce to `available` at the phone rung, and the
  // DEFAULT is one of them, so the 60 characters and ADR-044's retired
  // horizontal-scroll exemption are untouched by this feature as shipped.
  //
  // `narrow` is the exception, and it is a FINDING rather than a regression: 55
  // characters is 4,829 twips and a 390px window offers 5,280 — 60 characters — so
  // Narrow genuinely binds on a phone. ADR-048's "on a phone both reduce to
  // available" was said of the two POLICIES (Fit and Reading) and is true of both;
  // it is not true of a step a reader deliberately chose to be narrower than their
  // screen, and it should not be. What must hold for EVERY step is the direction:
  // never wider than the window, which is the no-horizontal-scroll guarantee.
  const available = reflowMeasure(390, AT_100, {});
  assert.equal(
    Math.round(available.contentWidthTwip / (0.3991 * 11 * 20)),
    60,
    "the 60 characters `docs/154` §3.2 calls correct",
  );
  for (const step of REFLOW_WIDTH_STEPS) {
    const cap = reflowCapTwip(step.id, { ...CALIBRI_11, docMeasureTwip: 9360 });
    const measure = reflowMeasure(390, AT_100, { capTwip: cap });
    assert.ok(
      measure.contentWidthTwip <= available.contentWidthTwip,
      `${step.id} asked for ${measure.contentWidthTwip} of a phone's ${available.contentWidthTwip}`,
    );
    const paintedPx = (measure.contentWidthTwip + 2 * measure.gutterTwip) * AT_100;
    assert.ok(paintedPx <= 390 + 0.25, `${step.id} painted ${paintedPx}px into 390`);
    if (step.id === "narrow") continue;
    assert.equal(
      measure.contentWidthTwip,
      available.contentWidthTwip,
      `${step.id} changed the phone's column from ${available.contentWidthTwip}`,
    );
    assert.equal(measure.capped, false, `${step.id} capped a phone`);
    assert.equal(measure.totalPx, measure.availablePx);
  }
  // And the DEFAULT specifically, because that is what a phone reader is given
  // without choosing anything at all.
  assert.equal(
    reflowMeasure(390, AT_100, { capTwip: reflowCapTwip(REFLOW_WIDTH_DEFAULT, CALIBRI_11) })
      .contentWidthTwip,
    available.contentWidthTwip,
  );
});

test("above the cap a resize changes nothing the engine is told", () => {
  // The consequence ADR-048 claims: most desktop resizes become free for a second
  // and better reason than the 16px bucket. Asserted as "the three numbers the
  // engine is handed, and the bucket the feed reads, are identical" — because the
  // bucket is what makes the resize free and the three numbers are what make it
  // correct.
  const cap = reflowCapTwip("reading", CALIBRI_11);
  const at = (px) => reflowMeasure(px, AT_100, { capTwip: cap });
  const first = at(1280);
  for (const px of [1281, 1366, 1440, 1600, 1920, 2560]) {
    const later = at(px);
    assert.equal(later.contentWidthTwip, first.contentWidthTwip, `${px}px moved the column`);
    assert.equal(later.gutterTwip, first.gutterTwip);
    assert.equal(
      quantiseReflowWidth(later.totalPx),
      quantiseReflowWidth(first.totalPx),
      `${px}px moved the width bucket, so the feed would schedule an O(document) pass`,
    );
  }
});

test("a cap that cannot be resolved falls back to the window, never to a failure", () => {
  // A reading comfort must not be the reason a document fails to lay out. Every
  // way the resolution can come up short answers `Infinity`, which is `available`
  // — i.e. exactly the behaviour before this feature.
  for (const doc of [{}, { face: null }, { fontSizePt: 0 }, { face: "", fontSizePt: NaN }]) {
    assert.ok(Number.isFinite(reflowCapTwip("reading", doc)), "a face is optional, 11pt is assumed");
  }
  assert.equal(reflowCapTwip("fit", {}), Infinity, "no document measure -> no cap");
  assert.equal(reflowCapTwip("fit", { docMeasureTwip: 0 }), Infinity);
  assert.equal(reflowCapTwip("full", CALIBRI_11), Infinity, "Full is uncapped by policy");
  const measure = reflowMeasure(1440, AT_100, { capTwip: Infinity });
  assert.deepEqual(measure.contentWidthTwip, reflowMeasure(1440, AT_100, {}).contentWidthTwip);
});

test("every width step is fully labelled, in English, in the shipped catalogue", () => {
  // The step table carries four catalogue keys per step and the catalogue is
  // built from two sources — `editor.html` for the rows and the default's short
  // label, `en_strings.mjs` for the rest, because `build-locale.mjs` refuses a key
  // declared in both. That split is a real hazard: a fifth step, or a renamed one,
  // would half-label itself and the product would print a dotted key on a ribbon
  // button. This is the guard that makes the split safe.
  const english = JSON.parse(readFileSync(new URL("../locales/en.json", import.meta.url), "utf8"));
  const missing = [];
  for (const step of REFLOW_WIDTH_STEPS) {
    for (const key of [step.shortKey, step.rowKey, step.titleKey, step.commandKey]) {
      if (typeof english[key] !== "string" || english[key].trim() === "") missing.push(key);
    }
  }
  for (const key of ["textWidth.command", "textWidth.pagedWithheld", "menuGroup.textWidth"]) {
    if (typeof english[key] !== "string") missing.push(key);
  }
  assert.deepEqual(missing, [], "a step with no sentence is a control that cannot say what it is");
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
