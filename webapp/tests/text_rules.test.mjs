// The text rules, exercised on the strings that break them.
//
// All four were pure already; none of them could be reached from a test while
// they lived in `main.js` (`109` HF-085), so Change case, smart quotes and the
// byte/string offset arithmetic were only ever covered through the browser —
// and HF-055 (an apostrophe after a non-ASCII letter came out as an OPENING
// quote) is exactly the bug that survives that kind of coverage.
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  byteOffsetToStringIndex,
  isWholeWordAt,
  recaseRichRuns,
  smartQuoteChar,
  transformCase,
} from "../src/text_rules.mjs";

test("upper and lower are locale-aware, not ASCII", () => {
  assert.equal(transformCase("straße", "upper"), "STRASSE");
  assert.equal(transformCase("ÉCOLE", "lower"), "école");
});

test("title case capitalises words and lowercases their tails", () => {
  assert.equal(transformCase("the QUICK brown fox", "title"), "The Quick Brown Fox");
});

// An apostrophe is part of the word, not a boundary — the same rule Word's
// Capitalize Each Word applies. Treating it as a boundary is what produces
// "Don'T", and it is the reason the word pattern lists both apostrophes.
test("title case keeps an apostrophe inside the word", () => {
  assert.equal(transformCase("o'brien and o’neill", "title"), "O'brien And O’neill");
  assert.equal(transformCase("DON'T", "title"), "Don't");
});

test("sentence case re-capitalises after terminal punctuation, including through a quote", () => {
  assert.equal(
    transformCase('he said "STOP." then LEFT. why? because.', "sentence"),
    'He said "stop." Then left. Why? Because.',
  );
});

test("sentence case capitalises the first letter even behind leading space", () => {
  assert.equal(transformCase("   hello there", "sentence"), "   Hello there");
});

test("toggle swaps case and leaves caseless characters alone", () => {
  assert.equal(transformCase("Hello, World 42!", "toggle"), "hELLO, wORLD 42!");
});

test("an unknown case mode changes nothing", () => {
  assert.equal(transformCase("Leave Me", "nonsense"), "Leave Me");
  assert.equal(transformCase("", "upper"), "");
});

test("a quote at the start of a paragraph or after a space opens", () => {
  assert.equal(smartQuoteChar('"', ""), "“");
  assert.equal(smartQuoteChar("'", ""), "‘");
  assert.equal(smartQuoteChar('"', " "), "“");
  assert.equal(smartQuoteChar("'", "\n"), "‘");
});

test("a quote after a bracket or a dash opens, and after a letter closes", () => {
  for (const before of ["(", "[", "{", "—", "–", "-", "/"]) {
    assert.equal(smartQuoteChar('"', before), "“", `after ${before}`);
  }
  for (const before of ["n", "5", ")", ".", "?"]) {
    assert.equal(smartQuoteChar("'", before), "’", `after ${before}`);
  }
});

// HF-055's user-visible symptom: the apostrophe in "don't" and the one after a
// non-ASCII letter must both be a right single quote.
test("an apostrophe inside a word closes, in ASCII and outside it", () => {
  assert.equal(smartQuoteChar("'", "n"), "’");
  assert.equal(smartQuoteChar("'", "é"), "’");
  assert.equal(smartQuoteChar("'", "р"), "’");
  assert.equal(smartQuoteChar("'", "字"), "’");
});

test("anything that is not a straight quote passes through untouched", () => {
  for (const key of ["a", " ", "“", "’", "Enter", ""]) {
    assert.equal(smartQuoteChar(key, ""), key);
  }
});

test("byte offsets equal string indices while the text is ASCII", () => {
  assert.equal(byteOffsetToStringIndex("hello", 0), 0);
  assert.equal(byteOffsetToStringIndex("hello", 3), 3);
  assert.equal(byteOffsetToStringIndex("hello", 5), 5);
});

// "café" is 5 bytes and 4 characters: byte 5 is the end, byte 3 is index 3.
test("a multi-byte letter shifts every offset after it", () => {
  assert.equal(byteOffsetToStringIndex("café", 3), 3);
  assert.equal(byteOffsetToStringIndex("café", 5), 4);
  assert.equal(byteOffsetToStringIndex("日本語", 3), 1);
  assert.equal(byteOffsetToStringIndex("日本語", 9), 3);
});

// An astral character is one code point and TWO JavaScript units; walking by
// unit would land between the surrogates and corrupt the slice.
test("an astral character is never split", () => {
  assert.equal(byteOffsetToStringIndex("a😀b", 1), 1);
  assert.equal(byteOffsetToStringIndex("a😀b", 5), 3);
});

test("an offset past the end clamps, and a negative offset is the start", () => {
  assert.equal(byteOffsetToStringIndex("abc", 99), 3);
  assert.equal(byteOffsetToStringIndex("abc", -1), 0);
  assert.equal(byteOffsetToStringIndex("", 4), 0);
});

test("whole-word matching honours the paragraph edges as boundaries", () => {
  assert.equal(isWholeWordAt("cat", 0, 3), true);
  assert.equal(isWholeWordAt("the cat sat", 4, 7), true);
  assert.equal(isWholeWordAt("concatenate", 3, 6), false);
  assert.equal(isWholeWordAt("cat_alog", 0, 3), false, "_ is a word character");
  assert.equal(isWholeWordAt("(cat)", 1, 4), true);
});

test("whole-word matching is Unicode-aware, not [A-Za-z]", () => {
  assert.equal(isWholeWordAt("naïveté here", 0, 7), true);
  assert.equal(isWholeWordAt("ünmatched", 0, 2), false, "a letter follows, so it is not a word");
  assert.equal(isWholeWordAt("λόγος", 0, 5), true);
});

// ---------------------------------------------------------------------------
// `recaseRichRuns` — the cross-run half of Change case.
//
// It moved out of `main.js` with the folding chrome (`109` FOLD-004), and it is
// the part of Change case that was never testable: `transformCase` above is a
// string rule, while this is where the SELECTION meets the formatting runs, and
// both of its branches go wrong in ways a browser run would not notice.

// Mutation that reddens it: case each run on its own — replace the body with
// `runs.map((r) => ({ ...r, text: transformCase(String(r.text ?? ""), mode) }))`,
// dropping the join and the re-slice. "hello. world" split by a bold boundary
// then comes back as "Hello. Wor" + "Ld", because the second run's first letter
// looks like the start of a sentence to a rule that cannot see the run before it.
test("sentence case reads across a formatting boundary, not within one run", () => {
  const runs = [{ text: "hello. wor" }, { text: "ld", bold: true }];
  assert.deepEqual(
    recaseRichRuns(runs, "sentence").map((r) => r.text),
    ["Hello. Wor", "ld"],
    "the capital belongs on the w of `world`, and nowhere else",
  );
});

// Mutation that reddens it: count a `paragraphBreak` run as 0 characters
// (`const len = r.paragraphBreak ? 0 : ...`). Every run after the first
// paragraph then takes its text one character early.
test("a paragraph break counts as the one character the join writes", () => {
  const runs = [{ text: "one" }, { paragraphBreak: true }, { text: "two" }];
  assert.deepEqual(
    recaseRichRuns(runs, "upper").map((r) => r.text ?? "(break)"),
    ["ONE", "(break)", "TWO"],
    "offsets must stay in step across a paragraph, or the text lands in the wrong run",
  );
});

// Mutation that reddens it: always re-slice, by changing the length guard to
// `if (false)`. `ß` upper-cases to two characters, so the re-slice runs long:
// the first run swallows a character that belongs to the second and the tail of
// the selection is dropped outright.
test("a transform that changes length falls back rather than mis-slicing", () => {
  const runs = [{ text: "straße" }, { text: "x", bold: true }];
  assert.deepEqual(
    recaseRichRuns(runs, "upper").map((r) => r.text),
    ["STRASSE", "X"],
    "`ß` becomes `SS`, so the whole-selection re-slice cannot be used — using it anyway " +
      "shifts every later run's text and loses the end of the selection",
  );
});

// Mutation that reddens it: `const out = runs;` instead of a copy. The caller's
// payload is then already cased when it is written back, which stays invisible
// until something reads the original — undo, a loss report, a second transform.
test("the caller's runs are not mutated, and formatting is carried through", () => {
  const runs = [{ text: "abc", bold: true, sizeHalfPoints: 24 }];
  const out = recaseRichRuns(runs, "upper");
  assert.equal(runs[0].text, "abc", "the input is left alone");
  assert.deepEqual(out, [{ text: "ABC", bold: true, sizeHalfPoints: 24 }]);
});
