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
