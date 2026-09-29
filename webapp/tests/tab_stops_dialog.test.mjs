// The rules the Tab stops dialog runs on, away from the DOM (`docs/148` §9
// item 7).
//
// What is checkable here is exactly the part that has edge cases: what counts as
// a typed position, what the list of a paragraph's stops looks like after a
// document has been imported rather than written by us, what a Set does to that
// list, and how many engine calls Clear all becomes. The widget itself — focus,
// Escape, the ruler agreeing with the list — needs a browser and is
// `tests/e2e/tab-stops.spec.mjs`.
//
// Every assertion below was driven RED first (SKILL §4); the mutations are in
// the commit message.
import test from "node:test";
import assert from "node:assert/strict";

import {
  ALIGNMENTS,
  MAX_POSITION_INCHES,
  clearAllCalls,
  codeFor,
  parsePosition,
  stopsFromFlat,
  withStop,
} from "../src/tab_stops_dialog.mjs";
import { TWIPS_PER_INCH } from "../src/units.mjs";

const scale = { twipsPerInch: TWIPS_PER_INCH };

// ---- Typing a position ------------------------------------------------------

test("a typed position becomes twips at the scale the ruler uses", () => {
  assert.deepEqual(parsePosition("1", scale), { twips: 1440 });
  assert.deepEqual(parsePosition("1.5", scale), { twips: 2160 });
  assert.deepEqual(parsePosition("0.25", scale), { twips: 360 });
  // Whitespace is a typo, not a value.
  assert.deepEqual(parsePosition("  2  ", scale), { twips: 2880 });
  // Zero is a position — the leading margin — and is not "no value".
  assert.deepEqual(parsePosition("0", scale), { twips: 0 });
});

test("a fraction of a twip is rounded rather than carried", () => {
  // 0.333 in is 479.52 twips. A stop the engine cannot hold has to become one
  // it can, and the engine's own positions are integers.
  assert.deepEqual(parsePosition("0.333", scale), { twips: 480 });
});

// THE RULE THIS FILE EXISTS FOR. `inchesToTwips` answers 0 for a blank box AND
// for "abc" — right for a margin field, wrong here. Three inputs, three
// different answers, none of them a silent stop at the margin.
test("nonsense is refused by its own name, never clamped to the margin", () => {
  assert.deepEqual(parsePosition("", scale), { error: "needPosition" });
  assert.deepEqual(parsePosition("   ", scale), { error: "needPosition" });
  assert.deepEqual(parsePosition(null, scale), { error: "needPosition" });
  assert.deepEqual(parsePosition("abc", scale), { error: "badPosition" });
  assert.deepEqual(parsePosition("1.5in", scale), { error: "badPosition" });
  assert.deepEqual(parsePosition("--2", scale), { error: "badPosition" });
});

test("a position off the paper is refused, in both directions", () => {
  assert.deepEqual(parsePosition("-1", scale), { error: "outOfRange" });
  assert.deepEqual(parsePosition(String(MAX_POSITION_INCHES + 0.01), scale), {
    error: "outOfRange",
  });
  // The ceiling itself is legal — `editor.html` gives the page-size fields the
  // same 22.
  assert.deepEqual(parsePosition(String(MAX_POSITION_INCHES), scale), {
    twips: MAX_POSITION_INCHES * TWIPS_PER_INCH,
  });
});

test("Infinity and NaN are not positions, whatever Number() thinks of them", () => {
  assert.deepEqual(parsePosition("Infinity", scale), { error: "badPosition" });
  assert.deepEqual(parsePosition("NaN", scale), { error: "badPosition" });
});

// ---- Reading the paragraph --------------------------------------------------

test("the engine's flat pairs become rows, ascending, whatever order they arrive in", () => {
  assert.deepEqual(stopsFromFlat([2160, 2, 720, 0, 1440, 3]), [
    { position: 720, align: "start" },
    { position: 1440, align: "decimal" },
    { position: 2160, align: "end" },
  ]);
});

test("every alignment the engine codes survives the round trip", () => {
  for (const { value, code } of ALIGNMENTS) {
    assert.deepEqual(stopsFromFlat([1440, code]), [{ position: 1440, align: value }]);
    assert.equal(codeFor(value), code);
  }
  // Bar is code 4 and is offered; `clear` (5) is not one of ours and must not
  // be silently renamed into one that is — it falls back to the default.
  assert.equal(ALIGNMENTS.at(-1).code, 4);
  assert.deepEqual(stopsFromFlat([1440, 5]), [{ position: 1440, align: "start" }]);
});

// An imported DOCX is not written by us: `w:tabs` can carry two `w:tab` at one
// position, and a list showing the same inch twice is a list nobody can act on.
test("two stops at one position collapse to one, the last winning", () => {
  assert.deepEqual(stopsFromFlat([1440, 0, 1440, 2]), [{ position: 1440, align: "end" }]);
});

test("a truncated or empty answer is a paragraph with no stops, not a crash", () => {
  assert.deepEqual(stopsFromFlat([]), []);
  assert.deepEqual(stopsFromFlat(undefined), []);
  // A trailing position with no alignment beside it is not half a stop.
  assert.deepEqual(stopsFromFlat([1440]), []);
});

// ---- Setting one ------------------------------------------------------------

test("Set adds a stop and keeps the list ascending", () => {
  const before = [
    { position: 720, align: "start" },
    { position: 2160, align: "end" },
  ];
  assert.deepEqual(withStop(before, 1440, "center"), [
    { position: 720, align: "start" },
    { position: 1440, align: "center" },
    { position: 2160, align: "end" },
  ]);
});

test("Set on a position that already has a stop retypes it rather than doubling it", () => {
  const before = [{ position: 1440, align: "start" }];
  assert.deepEqual(withStop(before, 1440, "decimal"), [{ position: 1440, align: "decimal" }]);
});

test("Set does not mutate the list it was given", () => {
  const before = [{ position: 1440, align: "start" }];
  withStop(before, 720, "end");
  assert.deepEqual(before, [{ position: 1440, align: "start" }]);
});

// ---- Clear all --------------------------------------------------------------

// THE UNDO GUARANTEE. `clearTabStops` empties the whole list inside one
// `apply_paragraph_props`, so one press of undo puts every stop back. Removing
// them one at a time would be N edits and N undo steps for one button, which is
// precisely the shape this plan exists to refuse.
test("Clear all is ONE engine call however many stops the paragraph holds", () => {
  for (const count of [1, 2, 7, 40]) {
    const stops = Array.from({ length: count }, (_, i) => ({
      position: (i + 1) * 720,
      align: "start",
    }));
    assert.deepEqual(
      clearAllCalls(stops),
      [{ op: "clearTabStops" }],
      `${count} stops must still be one call`,
    );
  }
});

test("Clear all on a paragraph with no stops is no call at all", () => {
  assert.deepEqual(clearAllCalls([]), []);
});
