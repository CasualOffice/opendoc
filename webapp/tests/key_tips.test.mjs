// The key-tip code table, without a browser (`109` UX-044).
//
// `key-tips.spec.mjs` drives the feature end to end; this pins the RULES the
// table is built by, because the table is the part that has to stay true for
// every band and every locale and a browser run only ever sees the one in front
// of it. The two properties that matter most are the ones a sequence of keys
// depends on: every code is UNIQUE, and no code is a PREFIX of another — or a
// person typing one tip's letters would fire a different tip half-way through.
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  TAB_KEYTIPS,
  assignKeyTips,
  chordLetter,
  isForeignTextEntry,
  keyTipChar,
  latinLetters,
  matchKeyTips,
  tabKeyTips,
} from "../src/key_tips.mjs";
import { nextRowIndex, readSegments } from "../src/header_mode.mjs";

/** Unique and prefix-free, the two properties the key sequence relies on. */
function assertPrefixFree(codes) {
  assert.equal(new Set(codes).size, codes.length, `duplicate codes: ${codes.join(" ")}`);
  for (const a of codes) {
    for (const b of codes) {
      if (a !== b) assert.ok(!b.startsWith(a), `${a} is a prefix of ${b}`);
    }
  }
}

test("a band's codes are unique and prefix-free, however many controls it has", () => {
  for (const count of [1, 5, 26, 35, 36, 43, 80]) {
    const names = Array.from({ length: count }, (_, i) => `Control number ${i}`);
    const codes = assignKeyTips(names);
    assert.equal(codes.length, count);
    assert.ok(codes.every(Boolean), `a control was left without a code at ${count}`);
    assertPrefixFree(codes);
  }
});

test("the assignment is deterministic", () => {
  const names = ["Track changes", "Show changes", "Accept change", "Reject change", "Accept all"];
  assert.deepEqual(assignKeyTips(names), assignKeyTips([...names]));
});

test("a control's own chord letter wins, and the plainest chord wins a clash", () => {
  // Bullets are ⌘⇧L and Left align ⌘L: the letter belongs to the one typed with
  // a single modifier, whichever comes first on the band.
  const codes = assignKeyTips([
    { name: "Bulleted list", chord: "⌘⇧L" },
    { name: "Bold", chord: "⌘B" },
    { name: "Align left", chord: "⌘L" },
    { name: "Accept and move on", chord: "⌘⌥⏎" },
  ]);
  assert.equal(codes[1], "B");
  assert.equal(codes[2], "L");
  assert.notEqual(codes[0], "L");
  // A chord on a named key carries no letter; the name decides.
  assert.equal(codes[3], "A");
});

test("a word's initial comes before any other letter, and an earlier control keeps its initial", () => {
  const codes = assignKeyTips(["Track changes", "Show changes", "Total"]);
  assert.equal(codes[0], "T");
  assert.equal(codes[1], "S");
  // "Total" lost T to an earlier control and takes another of its own letters.
  assert.equal(codes[2], "O");
});

test("a name with no Latin letters still gets a code a person can type", () => {
  const codes = assignKeyTips(["変更履歴の記録", "スペル", "文法"]);
  assert.deepEqual(codes, ["A", "B", "C"]);
  assert.deepEqual(latinLetters("Édition"), ["E", "D", "I", "T", "I", "O", "N"]);
});

test("overflow codes sit under a prefix that is never a code of its own", () => {
  const codes = assignKeyTips(Array.from({ length: 40 }, () => "x"));
  const overflow = codes.filter((code) => code.length === 2);
  assert.equal(overflow.length, 5);
  assert.ok(overflow.every((code) => code.startsWith("0")));
  assert.ok(!codes.includes("0"));
  assertPrefixFree(codes);
});

test("tabs take Word's letters, and the contextual Table tab its two", () => {
  const tabs = ["file", "home", "insert", "layout", "references", "review", "view", "table"];
  const codes = tabKeyTips(tabs.map((key) => ({ key, name: key })));
  assert.deepEqual(codes, ["F", "H", "N", "P", "S", "R", "W", "JT"]);
  assert.deepEqual(codes, tabs.map((key) => TAB_KEYTIPS[key]));
  assertPrefixFree(codes);
});

test("a tab Word does not have is assigned around the fixed ones, never under J", () => {
  const codes = tabKeyTips([
    { key: "home", name: "Home" },
    { key: "table", name: "Table" },
    { key: "jumble", name: "Jumble" },
    { key: "help", name: "Help" },
  ]);
  assert.equal(codes[0], "H");
  assert.equal(codes[1], "JT");
  assert.ok(!codes[2].startsWith("J"), `a new tab took ${codes[2]}, shadowing JT`);
  assert.notEqual(codes[3], "H");
  assertPrefixFree(codes);
});

test("typing filters to the codes still live, and completes exactly one", () => {
  const codes = ["F", "H", "JT", "R"];
  assert.deepEqual(matchKeyTips(codes, "J"), { live: [2], exact: -1 });
  assert.deepEqual(matchKeyTips(codes, "JT"), { live: [2], exact: 2 });
  assert.deepEqual(matchKeyTips(codes, "R"), { live: [3], exact: 3 });
  assert.deepEqual(matchKeyTips(codes, "Q"), { live: [], exact: -1 });
});

test("a keystroke reads as its Latin letter on any layout", () => {
  assert.equal(keyTipChar({ key: "r", code: "KeyR" }), "R");
  assert.equal(keyTipChar({ key: "7", code: "Digit7" }), "7");
  // A Russian layout reports "к" for the physical R key.
  assert.equal(keyTipChar({ key: "к", code: "KeyR" }), "R");
  assert.equal(keyTipChar({ key: "Tab", code: "Tab" }), null);
  assert.equal(keyTipChar({ key: "ArrowLeft", code: "ArrowLeft" }), null);
});

test("chordLetter reads the key a chord ends in", () => {
  assert.equal(chordLetter("⌘B"), "B");
  assert.equal(chordLetter("⌘⇧E"), "E");
  assert.equal(chordLetter("⌘⌥⏎"), null);
  assert.equal(chordLetter("⇧Tab"), null);
  assert.equal(chordLetter(undefined), null);
});

test("only a field that is not the document counts as somewhere a person is typing", () => {
  const el = (tagName, extra = {}) => ({
    tagName,
    isContentEditable: false,
    classList: { contains: (c) => (extra.classes ?? []).includes(c) },
    getAttribute: (name) => extra.attrs?.[name] ?? null,
    ownerDocument: { body: null },
    ...extra.props,
  });
  assert.equal(isForeignTextEntry(el("INPUT")), true);
  assert.equal(isForeignTextEntry(el("INPUT", { attrs: { type: "search" } })), true);
  assert.equal(isForeignTextEntry(el("TEXTAREA")), true);
  assert.equal(isForeignTextEntry(el("SELECT")), true);
  assert.equal(isForeignTextEntry(el("DIV", { props: { isContentEditable: true } })), true);
  // The document's own text input is where Word users press Alt.
  assert.equal(isForeignTextEntry(el("TEXTAREA", { classes: ["editor-text-input"] })), false);
  assert.equal(isForeignTextEntry(el("BUTTON")), false);
  assert.equal(isForeignTextEntry(el("INPUT", { attrs: { type: "checkbox" } })), false);
  assert.equal(isForeignTextEntry(null), false);
});

// ---- the header's mode selector reads the status bar ------------------------

const segment = (mode, { pressed = false, disabled = false, title = "" } = {}) => ({
  dataset: { reviewMode: mode },
  disabled,
  getAttribute: (name) => (name === "aria-pressed" ? String(pressed) : name === "title" ? title : null),
});

test("the header's mode is the pressed segment, with each row's availability and reason", () => {
  const state = readSegments([
    segment("editing", { disabled: true, title: "This document is read-only" }),
    segment("suggesting", { disabled: true, title: "This document is read-only" }),
    segment("viewing", { pressed: true }),
  ]);
  assert.equal(state.current, "viewing");
  assert.deepEqual(state.rows[0], {
    mode: "editing",
    pressed: false,
    disabled: true,
    reason: "This document is read-only",
  });
  assert.equal(state.rows[2].reason, "");
});

test("with no segment pressed the header says Editing, setReviewMode's own fallback", () => {
  assert.equal(readSegments([segment("editing"), segment("suggesting")]).current, "editing");
});

test("the menu's arrow keys wrap, and Home and End jump", () => {
  assert.equal(nextRowIndex("ArrowDown", 2, 3), 0);
  assert.equal(nextRowIndex("ArrowUp", 0, 3), 2);
  assert.equal(nextRowIndex("ArrowDown", -1, 3), 0);
  assert.equal(nextRowIndex("Home", 1, 3), 0);
  assert.equal(nextRowIndex("End", 0, 3), 2);
  assert.equal(nextRowIndex("Enter", 0, 3), -1);
});
