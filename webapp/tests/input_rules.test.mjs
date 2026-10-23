// List auto-format and AutoCorrect rules (docs/174 TA-05, TA-06): what typing
// one character turns into, decided from the text before the caret.
import assert from "node:assert/strict";
import test from "node:test";
import { autocorrectFor, listTriggerFor } from "../src/input_rules.mjs";

/** Applies a rule's answer to `prefix`, the way the host does. */
function after(prefix, options) {
  const fix = autocorrectFor(prefix, options);
  if (!fix) return prefix;
  return prefix.slice(0, fix.start) + fix.replacement + prefix.slice(fix.end);
}

test("list triggers: 1. and 1) number, - and * bullet, only as the whole prefix", () => {
  assert.deepEqual(listTriggerFor("1. "), { kind: "numbered", marker: "1. " });
  assert.deepEqual(listTriggerFor("1) "), { kind: "numbered", marker: "1) " });
  assert.deepEqual(listTriggerFor("- "), { kind: "bullet", marker: "- " });
  assert.deepEqual(listTriggerFor("* "), { kind: "bullet", marker: "* " });
  for (const no of ["1.", "2. ", "a. ", " - ", "x- ", "-  ", "1. x"]) assert.equal(listTriggerFor(no), null, no);
});

test("symbols complete on the closing bracket, case-insensitively", () => {
  assert.equal(after("Copyright (c)"), "Copyright ©");
  assert.equal(after("Acme(TM)"), "Acme™");
  assert.equal(after("Brand (r)"), "Brand ®");
  assert.equal(after("(x)"), "(x)");
});

test("dashes: -- between words is an em dash, alone between spaces an en dash", () => {
  assert.equal(after("well--maybe "), "well—maybe ");
  assert.equal(after("a -- "), "a – ");
  assert.equal(after("--- "), "--- ");
  assert.equal(after("a--"), "a--", "nothing until the word is finished");
});

test("common typos are fixed, keeping the case typed", () => {
  assert.equal(after("see teh "), "see the ");
  assert.equal(after("Teh "), "The ");
  assert.equal(after("we recieve,"), "we receive,");
  assert.equal(after("tehx "), "Tehx ", "not a listed typo; capitalised as a sentence start only");
});

test("the first letter of a sentence is capitalised, and a lone i is I", () => {
  assert.equal(after("hello "), "Hello ");
  assert.equal(after("Hello. world "), "Hello. World ");
  assert.equal(after("Really? yes "), "Really? Yes ");
  assert.equal(after("So i "), "So I ");
  // Not sentence ends: abbreviations, initials, decimals, and mid-sentence words.
  assert.equal(after("See e.g. this "), "See e.g. this ");
  assert.equal(after("J. smith "), "J. smith ");
  assert.equal(after("It is 1.5 times "), "It is 1.5 times ");
  assert.equal(after("The cat "), "The cat ");
  // Words that are not plain lowercase are left alone.
  assert.equal(after("iPhone "), "iPhone ");
  assert.equal(after("x2 "), "x2 ");
});

test("each rule can be switched off on its own", () => {
  assert.equal(after("hello ", { capitalize: false }), "hello ");
  assert.equal(after("So teh ", { typos: false }), "So teh ");
  assert.equal(after("(c)", { symbols: false }), "(c)");
  assert.equal(after("a--b ", { dashes: false }), "a--b ");
});

test("nothing happens on a character that does not end a word", () => {
  assert.equal(autocorrectFor("teh"), null);
  assert.equal(autocorrectFor(""), null);
  assert.equal(autocorrectFor(" "), null);
});
