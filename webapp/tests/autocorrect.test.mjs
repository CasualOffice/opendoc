// The AutoCorrect switches and their application as you type (docs/174 TA-05,
// TA-06), against a stand-in engine: which engine call a keystroke turns into,
// that it is ONE call (one undo step), and that each switch turns its rule off.
import assert from "node:assert/strict";
import test from "node:test";
import { createAutoCorrect } from "../src/autocorrect.mjs";

function harness(text, { prefs = {}, editing = true } = {}) {
  const calls = [];
  const stored = { ...prefs };
  const statuses = [];
  const caret = { node: "p1", offset: new TextEncoder().encode(text).length };
  const doc = {
    copyText: (node, start, _n, end) => new TextDecoder().decode(new TextEncoder().encode(text).slice(start, end)),
    listStyleAt: () => "",
    autoFormatList: (...args) => calls.push(["autoFormatList", ...args]),
    replaceSelection: (...args) => calls.push(["replaceSelection", ...args]),
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
    runEdit: async (thunk) => thunk(),
    setStatus: (s) => statuses.push(s),
    toggles,
  });
  return { autoCorrect, calls, stored, statuses, toggles };
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
