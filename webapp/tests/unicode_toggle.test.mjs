// Alt+X (`docs/153` text.unicode-code-toggle): Word's toggle between a code
// point and its character, as pure text rules.
import assert from "node:assert/strict";
import test from "node:test";

import { formatCodePoint, parseCode, toggleBeforeCaret, toggleSelection } from "../src/unicode_toggle.mjs";

test("hex digits before the caret become their character", () => {
  assert.deepEqual(toggleBeforeCaret("Snow 2603"), { remove: 4, insert: "☃" });
  assert.deepEqual(toggleBeforeCaret("Snow U+2603"), { remove: 6, insert: "☃" }, "U+ is consumed too");
  assert.deepEqual(toggleBeforeCaret("smile 1F600"), { remove: 5, insert: "😀" });
});

test("a character before the caret becomes its code, and the code turns back", () => {
  const forward = toggleBeforeCaret("café");
  assert.deepEqual(forward, { remove: 1, insert: "00E9" });
  const after = "caf" + forward.insert;
  assert.deepEqual(toggleBeforeCaret(after), { remove: 4, insert: "é" });
});

test("an astral character is one character, not two halves", () => {
  assert.deepEqual(toggleBeforeCaret("x😀"), { remove: 2, insert: "1F600" });
});

test("a value that names nothing printable converts the character instead", () => {
  // "a" is hex for a line feed; Word gives the letter's code, not a control.
  assert.deepEqual(toggleBeforeCaret("a"), { remove: 1, insert: "0061" });
  // Seven digits: the six-digit window past U+10FFFF shrinks until it names one.
  assert.deepEqual(toggleBeforeCaret("x1234567"), { remove: 5, insert: String.fromCodePoint(0x34567) });
  // A letter a-f before a code is a letter, not the top digit of a private-use code.
  assert.deepEqual(toggleBeforeCaret("caf00E9"), { remove: 4, insert: "é" });
  assert.equal(parseCode("D800"), null, "a surrogate is not a character");
});

test("nothing before the caret is nothing to do", () => {
  assert.equal(toggleBeforeCaret(""), null);
});

test("a selection toggles as a whole", () => {
  assert.equal(toggleSelection("U+00E9"), "é");
  assert.equal(toggleSelection("é"), "00E9");
  assert.equal(toggleSelection("two words"), null);
  assert.equal(toggleSelection("one\ntwo"), null);
  assert.equal(formatCodePoint(0x41), "0041");
});
