// Regular-expression Find & Replace (`docs/153` shell.find-regex): the pure
// half — compiling, matching over the engine's paragraphs in UTF-8 offsets,
// and expanding `$1` with the paragraph as context.
import assert from "node:assert/strict";
import test from "node:test";

import { compileFindPattern, expandReplacement, regexMatches } from "../src/find_regex.mjs";

const paragraphs = [
  ["1:1", "Call 555-1234 or 555-9876."],
  ["1:2", "café 42 naïve"],
  ["1:3", ""],
];

test("an invalid pattern is an inline error, never an exception", () => {
  const compiled = compileFindPattern("(unclosed");
  assert.ok("error" in compiled, "an error is returned");
  assert.match(compiled.error, /\S/, "the error says something");
  assert.doesNotMatch(compiled.error, /^Invalid regular expression/, "the repeated prefix is dropped");
  assert.deepEqual(compileFindPattern(""), { error: "" }, "an empty query is neither a match nor an error");
});

test("matches are model ranges in UTF-8 bytes, in document order", () => {
  const { regex } = compileFindPattern("\\d+");
  const found = regexMatches(paragraphs, regex);
  assert.deepEqual(
    found.map((m) => [m.startNode, m.startOffset, m.endOffset]),
    [
      ["1:1", 5, 8],
      ["1:1", 9, 13],
      ["1:1", 17, 20],
      ["1:1", 21, 25],
      // "café " is six UTF-8 bytes before the digits, not five.
      ["1:2", 6, 8],
    ],
  );
});

test("match case and whole words compose with the pattern", () => {
  assert.equal(regexMatches(paragraphs, compileFindPattern("CALL").regex).length, 1);
  assert.equal(regexMatches(paragraphs, compileFindPattern("CALL", { matchCase: true }).regex).length, 0);
  // "naïve" is one word: whole-word "na" must not match inside it, and the
  // boundary is Unicode-aware (ï is a letter).
  assert.equal(regexMatches(paragraphs, compileFindPattern("na", { wholeWord: true }).regex).length, 0);
  assert.equal(regexMatches(paragraphs, compileFindPattern("na\\S+", { wholeWord: true }).regex).length, 1);
});

test("an empty match is skipped and the scan still ends", () => {
  const { regex } = compileFindPattern("x*");
  assert.deepEqual(regexMatches([["1:1", "axxb"]], regex).map((m) => m.startOffset), [1]);
});

test("the cap bounds the scan", () => {
  const { regex } = compileFindPattern("\\d");
  assert.equal(regexMatches(paragraphs, regex, 3).length, 3);
});

test("$1, $<name>, $& and $$ expand per match, as String.replace does", () => {
  const { regex } = compileFindPattern("(?<area>\\d{3})-(\\d{4})");
  const [first, second] = regexMatches(paragraphs, regex);
  assert.equal(expandReplacement(regex, first, "$2/$<area>"), "1234/555");
  assert.equal(expandReplacement(regex, second, "[$&] $$"), "[555-9876] $");
});

test("a lookbehind still sees the paragraph when the replacement is expanded", () => {
  const { regex } = compileFindPattern("(?<=or )(\\d+)");
  const [match] = regexMatches(paragraphs, regex);
  assert.equal(match.startOffset, 17);
  assert.equal(expandReplacement(regex, match, "<$1>"), "<555>");
});
