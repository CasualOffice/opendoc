// List auto-format and AutoCorrect rules (docs/174 TA-05, TA-06): what typing
// one character turns into, decided from the text before the caret.
import assert from "node:assert/strict";
import test from "node:test";
import { autocorrectFor, hyperlinkFor, listTriggerFor } from "../src/input_rules.mjs";

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

// ---- RM-19: the editable table, exceptions, cells, " - " and addresses -------

test("the replace table is the host's: an added entry fires, an absent one does not", () => {
  const table = new Map([["omw", "on my way"], ["(c)", "©"]]);
  assert.equal(after("see omw ", { table }), "see on my way ");
  assert.equal(after("see Omw ", { table }), "see On my way ", "a lowercase entry matches any case, re-cased");
  assert.equal(after("see teh ", { table }), "see teh ", "teh is not in this table");
  assert.equal(after("x (c)", { table }), "x ©");
  assert.equal(after("see omw ", { table, replace: false }), "see omw ");
  // An entry ending in punctuation completes on that character; one that starts
  // with a letter must start a word ("teh)" is not inside "steh)").
  const arrows = new Map([["->", "→"], ["teh)", "the)"]]);
  assert.equal(after("a ->", { table: arrows }), "a →");
  assert.equal(after("a steh)", { table: arrows }), "a steh)");
});

test("first-letter exceptions are the host's list, per language", () => {
  const exceptions = new Set(["approx."]);
  assert.equal(after("It is approx. ten ", { exceptions }), "It is approx. ten ");
  assert.equal(after("It is etc. ten ", { exceptions }), "It is etc. Ten ", "etc. is not in this list");
});

test("the first word of a paragraph follows capitalizeStart, the cell switch inside a table", () => {
  assert.equal(after("hello ", { capitalize: true, capitalizeStart: false }), "hello ");
  assert.equal(after("hello ", { capitalize: false, capitalizeStart: true }), "Hello ");
  // Mid-paragraph sentences follow `capitalize` alone.
  assert.equal(after("Done. next ", { capitalize: false, capitalizeStart: true }), "Done. next ");
  // A bare scheme is an address being typed, not a sentence.
  assert.equal(after("www."), "www.");
  assert.equal(after("https:"), "https:");
});

test("' - ' between words becomes an en dash when the next word is finished", () => {
  assert.equal(after("pages 10 - 12 "), "pages 10 – 12 ");
  assert.equal(after("yes - no,"), "yes – no,", "punctuation finishes the word too");
  assert.equal(after("a - "), "a - ", "not before a word follows");
  assert.equal(after("- item "), "- item ", "a list marker at the paragraph start is not a dash");
  assert.equal(after("a - b ", { dashes: false }), "a - b ");
});

test("addresses: what is linked, where the link ends, and what it points at", () => {
  assert.deepEqual(hyperlinkFor("see https://example.org/a?b=1 "), { start: 4, end: 29, target: "https://example.org/a?b=1" });
  assert.deepEqual(hyperlinkFor("www.example.com "), { start: 0, end: 15, target: "http://www.example.com" });
  assert.deepEqual(hyperlinkFor("mail me@example.org "), { start: 5, end: 19, target: "mailto:me@example.org" });
  // Sentence punctuation stays outside; a bracket stays outside unless the address opened one.
  assert.deepEqual(hyperlinkFor("go to www.example.com. "), { start: 6, end: 21, target: "http://www.example.com" });
  assert.deepEqual(hyperlinkFor("(see https://x.org/a) "), { start: 5, end: 20, target: "https://x.org/a" });
  assert.equal(hyperlinkFor("see https://x.org/a_(b) ")?.target, "https://x.org/a_(b)");
  // Not addresses, and not on a non-space trigger.
  for (const no of ["e.g. ", "file.txt ", "a@b ", "https:// ", "www.example.com", "www.example.com,"]) {
    assert.equal(hyperlinkFor(no), null, no);
  }
});
