// The AutoCorrect switches and their application as you type (docs/174 TA-05,
// TA-06), against a stand-in engine: which engine call a keystroke turns into,
// that it is ONE call (one undo step), and that each switch turns its rule off.
import assert from "node:assert/strict";
import test from "node:test";
import { createAutoCorrect } from "../src/autocorrect.mjs";

function harness(text, { prefs = {}, editing = true, doc: extra = {} } = {}) {
  const calls = [];
  const runs = [];
  const stored = { ...prefs };
  const statuses = [];
  const caret = { node: "p1", offset: new TextEncoder().encode(text).length };
  const doc = {
    copyText: (node, start, _n, end) => new TextDecoder().decode(new TextEncoder().encode(text).slice(start, end)),
    listStyleAt: () => "",
    autoFormatList: (...args) => calls.push(["autoFormatList", ...args]),
    replaceSelection: (...args) => calls.push(["replaceSelection", ...args]),
    setHyperlink: (...args) => calls.push(["setHyperlink", ...args]),
    ...extra,
  };
  const toggle = () => {
    const listeners = [];
    return { checked: null, addEventListener: (_type, fn) => listeners.push(fn), fire: () => listeners.forEach((fn) => fn()) };
  };
  const toggles = { smartQuotes: toggle(), lists: toggle(), autocorrect: toggle() };
  const autoCorrect = createAutoCorrect({
    readPref: (key) => stored[key] ?? null,
    writePref: (key, value) => {
      stored[key] = value;
    },
    t: (key, params = {}) => `${key}${Object.keys(params).length ? JSON.stringify(params) : ""}`,
    getDoc: () => doc,
    getSelection: () => ({ anchor: caret, focus: caret }),
    editing: () => editing,
    runEdit: async (thunk, options) => {
      runs.push(options ?? null);
      return thunk();
    },
    setStatus: (s) => statuses.push(s),
    toggles,
  });
  return { autoCorrect, calls, runs, stored, statuses, toggles };
}

test("a typed list marker is ONE engine call: the marker's bytes, the caret, the kind", async () => {
  const { autoCorrect, calls } = harness("1. ");
  assert.equal(await autoCorrect.afterTyped(" "), true);
  assert.deepEqual(calls, [["autoFormatList", "p1", 3, 3, "numbered"]]);
});

test("a correction replaces from the word to the CARET, carrying the typed space", async () => {
  const { autoCorrect, calls } = harness("so teh ");
  await autoCorrect.afterTyped(" ");
  // Only the word just finished is touched; "so" was decided when it was typed.
  assert.deepEqual(calls, [["replaceSelection", "p1", 3, "p1", 7, "the "]]);
});

test("byte offsets, not string indices, reach the engine", async () => {
  const { autoCorrect, calls } = harness("Café teh ");
  await autoCorrect.afterTyped(" ");
  // "Café " is 6 bytes (é is two), so "teh " starts at byte 6 and the caret is at 10.
  assert.deepEqual(calls, [["replaceSelection", "p1", 6, "p1", 10, "the "]]);
});

test("nothing happens off Editing, on a non-trigger key, or when the rule is off", async () => {
  assert.equal(await harness("1. ", { editing: false }).autoCorrect.afterTyped(" "), false);
  assert.equal(await harness("teh", {}).autoCorrect.afterTyped("h"), false);
  const off = harness("1. ", { prefs: { "opendoc.autoFormatLists": "off" } });
  assert.equal(await off.autoCorrect.afterTyped(" "), false);
  const noFix = harness("so teh ", { prefs: { "opendoc.autocorrect": "off" } });
  assert.equal(await noFix.autoCorrect.afterTyped(" "), false);
});

test("the switches are remembered, reflected in Settings, and said", () => {
  const { autoCorrect, stored, statuses, toggles } = harness("");
  assert.equal(autoCorrect.enabled("lists"), true, "on by default, as in Word");
  assert.equal(toggles.lists.checked, true);
  autoCorrect.toggle("lists");
  assert.equal(stored["opendoc.autoFormatLists"], "off");
  assert.equal(toggles.lists.checked, false);
  assert.match(statuses.at(-1), /^autocorrect\.statusOff/);
  // The Settings checkbox drives the same switch.
  toggles.autocorrect.checked = false;
  toggles.autocorrect.fire();
  assert.equal(autoCorrect.enabled("autocorrect"), false);
  assert.equal(stored["opendoc.autocorrect"], "off");
  // Smart quotes keep the key they always had, so an existing choice survives.
  assert.equal(harness("", { prefs: { "opendoc.smartQuotes": "off" } }).autoCorrect.enabled("smartQuotes"), false);
});

// ---- RM-19 ----------------------------------------------------------------

test("a typed address is linked over its own bytes, as its own step, and the caret is kept", async () => {
  const { autoCorrect, calls, runs } = harness("café https://x.org ");
  assert.equal(await autoCorrect.afterTyped(" "), true);
  // "café " is 6 bytes, the address 13: the link covers [6, 19), not the space.
  assert.deepEqual(calls, [["setHyperlink", "p1", 6, 19, "https://x.org", undefined]]);
  assert.deepEqual(runs, [{ keepSelection: true }]);
});

test("an address typed inside an existing link is left alone", async () => {
  const { autoCorrect, calls } = harness("https://x.org ", { doc: { hyperlinkTouches: () => true } });
  await autoCorrect.afterTyped(" ");
  assert.equal(calls.some(([name]) => name === "setHyperlink"), false);
  const off = harness("https://x.org ", { prefs: { "opendoc.autoFormatHyperlink": "off" } });
  await off.autoCorrect.afterTyped(" ");
  assert.equal(off.calls.some(([name]) => name === "setHyperlink"), false);
});

test("Enter corrects the word it finished, without the break, and keeps the caret in the new paragraph", async () => {
  // The break is not in the paragraph's text: the old paragraph ends at "teh".
  const { autoCorrect, calls, runs } = harness("so teh");
  assert.equal(await autoCorrect.afterBreak("p1", 6), true);
  assert.deepEqual(calls, [["replaceSelection", "p1", 3, "p1", 6, "the"]]);
  assert.deepEqual(runs, [{ keepSelection: true }]);
});

test("a pasted lone address is linked; a pasted sentence is not", async () => {
  const linked = harness("see www.example.com");
  assert.equal(await linked.autoCorrect.afterPaste("www.example.com"), true);
  assert.deepEqual(linked.calls, [["setHyperlink", "p1", 4, 19, "http://www.example.com", undefined]]);
  const prose = harness("see www.example.com now");
  assert.equal(await prose.autoCorrect.afterPaste("www.example.com now"), false);
  assert.deepEqual(prose.calls, []);
});

test("the replace table is the one for the language at the caret", async () => {
  // A French run: the English typo list does not apply, the symbols still do.
  const french = { languageSpans: () => JSON.stringify([{ start: 0, end: 7, language: "fr-FR" }]) };
  const typo = harness("so teh ", { doc: french });
  assert.equal(await typo.autoCorrect.afterTyped(" "), false);
  const symbol = harness("so (c)", { doc: french });
  await symbol.autoCorrect.afterTyped(")");
  assert.deepEqual(symbol.calls, [["replaceSelection", "p1", 3, "p1", 6, "©"]]);
  // The user's own entry for English fires.
  const custom = harness("so omw ");
  custom.autoCorrect.tables.setReplacement("en-GB", "omw", "on my way");
  await custom.autoCorrect.afterTyped(" ");
  assert.deepEqual(custom.calls, [["replaceSelection", "p1", 3, "p1", 7, "on my way "]]);
});

test("a table cell's first word follows the cell switch, not the sentence switch", async () => {
  const inCell = { inTable: () => true };
  const off = harness("hello ", { doc: inCell, prefs: { "opendoc.autoCapitaliseCells": "off" } });
  assert.equal(await off.autoCorrect.afterTyped(" "), false);
  const on = harness("hello ", { doc: inCell, prefs: { "opendoc.autoCapitalise": "off", "opendoc.autoCapitaliseCells": "on" } });
  await on.autoCorrect.afterTyped(" ");
  assert.deepEqual(on.calls, [["replaceSelection", "p1", 0, "p1", 6, "Hello "]]);
  // Outside a table the same first word follows the sentence switch, which is off.
  const body = harness("hello ", { prefs: { "opendoc.autoCapitalise": "off", "opendoc.autoCapitaliseCells": "on" } });
  assert.equal(await body.autoCorrect.afterTyped(" "), false);
});

test("switches split out of the old one inherit it while unset", () => {
  const old = harness("", { prefs: { "opendoc.autocorrect": "off" } }).autoCorrect;
  assert.equal(old.enabled("capitalise"), false);
  assert.equal(old.enabled("capitaliseCells"), false);
  assert.equal(old.enabled("dashes"), false);
  assert.equal(old.enabled("hyperlinks"), true, "hyperlinks never were part of it");
  const chosen = harness("", { prefs: { "opendoc.autocorrect": "off", "opendoc.autoFormatDashes": "on" } }).autoCorrect;
  assert.equal(chosen.enabled("dashes"), true, "a choice made since wins over the inheritance");
});
