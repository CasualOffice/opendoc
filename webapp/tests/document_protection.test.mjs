// Restrict Editing: the five states a document can arrive in, and the ONE-WAY
// DOOR that is the whole reason this control has to exist.
//
// `docs/153` `review.restrict-editing` / `review.protect-document`, ADR-052
// (enforcement at the operation) and ADR-059 (the operation that installs or
// lifts one). The engine could obey a restriction and, until the setter landed,
// could not change one — so a document that arrived with `w:documentProtection`
// set to `readOnly` was PERMANENTLY read-only in this editor. ADR-059 removed
// that door in `casual-doc-edit` with `exempt_from_protection`.
//
// THE DEFECT THIS GUARD EXISTS FOR is that the chrome can rebuild exactly the
// same door one layer up, and it is a one-character change to do it: routing the
// protection write through `runEdit` WITH the suggesting gate. The gate is
// `blockUntrackedInSuggesting`, which stops an untracked CONTENT edit slipping
// past review. It is not an authority check, and a policy change cannot be
// expressed as a tracked suggestion at all, so gating it would refuse the one
// operation that can lift a restriction — on precisely the documents that need
// it lifted.
//
// So the assertion is the GUARANTEE, not the mechanism (`SKILL` §10): the fake
// `runEdit` below SIMULATES the gate rather than merely recording whether it was
// asked for, and the test asserts that a `readOnly` document can be unprotected.
// A test that read `options.gate` and asserted it was falsy would be checking an
// argument shape; this one checks that the door opens.
//
// MUTATION PROOF. Passing `{ gate: true }` to `runEdit` in `apply()`:
//
//   not ok 6 - an enforced readOnly document can be unprotected -- the one-way door
//     error: |-
//       a readOnly document could not be unprotected: the chrome refused the one
//       operation ADR-059 exists to allow
//
//       false !== true
//     expected: true
//     actual: false
//
// and `not ok 7 - a protected document can be moved straight to another level`
// with the same cause. The other seven stay green, which is the point: the
// mutation is invisible to every test that does not start from a protected
// document, and that is how it would have shipped.
import assert from "node:assert/strict";
import test from "node:test";

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { setCatalogue, setLocale } from "../src/i18n.mjs";

// The RUNTIME catalogue, not `EN_STRINGS`. The five level labels are MARKUP keys
// — they live in `editor.html` beside their English and so are absent from
// `en_strings.mjs` — but `apply()` composes one of them into the sentence it
// announces, through `t(row.labelKey)`. So `locales/en.json` is what this control
// actually reads from, and priming the script-only table instead would make the
// status assertions pass against dotted keys.
const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");
const EN = JSON.parse(readFileSync(join(WEBAPP, "locales/en.json"), "utf8"));
setCatalogue("en", EN);
setLocale("en");

// `createDocumentProtection` reads its markup by id at construction, so the
// module needs a `document` to exist before it is imported.
const elements = new Map();
globalThis.document = {
  getElementById: (id) => elements.get(id) ?? null,
  activeElement: null,
  createElement: () => ({ append() {}, setAttribute() {} }),
};

const { PROTECTION_LEVELS, createDocumentProtection, readProtection } = await import(
  "../src/document_protection.mjs"
);

/** A `documentProtection()` payload. */
const json = (edit, enforcement, formatting = false) =>
  JSON.stringify({ edit, enforcement, formatting });

// ---------------------------------------------------------------------------
// `readProtection` is pure, so the five arrival states need no browser at all.
// ---------------------------------------------------------------------------

test("an absent w:documentProtection restricts nothing", () => {
  const state = readProtection(null);
  assert.equal(state.edit, null);
  assert.equal(state.value, "off");
  assert.equal(state.active, false);
});

test('edit="none" is a real state and is NOT the element being absent', () => {
  // Word round-trips a restriction an author set up and switched off. Both read
  // as "no restriction" in the radio group because neither restricts anything;
  // what keeps them apart is that Apply only writes a real difference, so
  // opening this document and pressing Apply must not quietly rewrite it.
  const none = readProtection(json("none", false));
  assert.equal(none.value, "off");
  assert.equal(none.active, false);
  assert.equal(none.edit, "none", "the token is carried, not flattened to null");
  assert.notEqual(none.edit, readProtection(null).edit);
});

test("an unenforced level is shown but is not in force", () => {
  // A state Word writes: the restriction is kept in the file without being
  // applied. The radio shows it; the ribbon button is not pressed.
  const state = readProtection(json("readOnly", false));
  assert.equal(state.value, "readOnly", "the dialog still shows what is stored");
  assert.equal(state.enforcement, false);
  assert.equal(state.active, false, "nothing is restricted, so nothing is pressed");
});

test("an edit token this build cannot name falls back to the STRICTEST row", () => {
  // Reading a restriction nobody can name as "none" is the one answer that
  // could lose a protection, so an unknown token reads as readOnly.
  const state = readProtection(json("someFutureLevel", true));
  assert.equal(state.value, "readOnly");
  assert.equal(state.edit, "someFutureLevel", "and the token survives for Apply");
  assert.equal(state.active, true);
});

test("every level the dialog offers maps onto a distinct engine token", () => {
  const edits = PROTECTION_LEVELS.map((level) => level.edit);
  assert.deepEqual(edits, [null, "readOnly", "trackedChanges", "comments", "forms"]);
  assert.equal(new Set(edits).size, edits.length);
  // Word's order, strictest first, and a fifth row so Stop Protection is the
  // same control rather than a conditional second button.
  assert.equal(PROTECTION_LEVELS[0].value, "off");
  for (const level of PROTECTION_LEVELS) assert.ok(EN[level.labelKey], level.labelKey);
});

// ---------------------------------------------------------------------------
// The dialog, over fakes. The gate is SIMULATED, not inspected.
// ---------------------------------------------------------------------------

/** A stand-in for the elements `editor.html` carries, which RECORDS the
 *  listeners the module installs — so Apply is pressed through the real wiring
 *  rather than through a seam added to production code for a test. */
function fakeMarkup() {
  const listeners = new Map();
  const node = (id) => ({
    addEventListener: (type, handler) => void listeners.set(`${id}:${type}`, handler),
  });
  const levels = { querySelector: () => null, querySelectorAll: () => [] };
  const enforce = {
    checked: false,
    disabled: false,
    addEventListener: (type, handler) => void listeners.set(`enforce:${type}`, handler),
  };
  elements.clear();
  elements.set("restrictEditingDialog", node("dialog"));
  elements.set("restrictEditingLevels", levels);
  elements.set("restrictEditingEnforce", enforce);
  elements.set("restrictEditingEnforceRow", { title: "" });
  elements.set("restrictEditingApply", node("apply"));
  elements.set("restrictEditingCancel", node("cancel"));
  elements.set("restrictEditingClose", node("close"));
  return { levels, enforce, listeners };
}

/**
 * Builds the control over a document whose protection starts at `start`.
 *
 * `runEdit` behaves the way the real one does, INCLUDING the gate: when it is
 * asked for and the document is restricted to `readOnly`, the edit is refused.
 * That is what makes this a guard against the one-way door rather than an
 * assertion about an argument.
 */
function build(start) {
  const { enforce, listeners } = fakeMarkup();
  let stored = start;
  const doc = {
    documentProtection: () => stored,
    setDocumentProtection(edit, enforcement, formatting) {
      stored = json(edit, enforcement, formatting);
      return true;
    },
  };
  const status = [];
  let closed = 0;
  let changed = 0;
  let selected = "off";
  const protection = createDocumentProtection({
    getDoc: () => doc,
    async runEdit(thunk, options) {
      const state = readProtection(stored);
      // The real `blockUntrackedInSuggesting`: an untracked edit is refused
      // while the document restricts editing to review. A policy change cannot
      // be a tracked suggestion, so if the chrome asks for the gate here, the
      // operation that lifts a restriction is refused by the restriction.
      if (options?.gate && state.active) return false;
      thunk();
      return true;
    },
    registerModal: () => ({ open() {}, close: () => void (closed += 1) }),
    fallbackFocus: () => null,
    bindRadioGroup: () => ({
      value: () => selected,
      selected: () => null,
      reflect: (value) => void (selected = value),
    }),
    setStatus: (text, kind) => void status.push({ text, kind }),
    onChanged: () => void (changed += 1),
  });
  return {
    protection,
    enforce,
    status,
    listeners,
    closed: () => closed,
    changed: () => changed,
    choose: (value) => void (selected = value),
    stored: () => readProtection(stored),
  };
}

/** Presses Apply through the listener the module actually installed, and
 *  reports whether the document ended up changed.
 *
 *  The listener is `() => void apply()`, so it discards its own promise; nothing
 *  under it is really asynchronous, so draining the microtask queue once is
 *  enough and there is no clock to wait on (`SKILL` §6). */
async function applyVia(h) {
  const before = JSON.stringify(h.stored());
  const press = h.listeners.get("apply:click");
  assert.ok(press, "the Apply button had no click listener");
  press();
  await new Promise((resolve) => setImmediate(resolve));
  return JSON.stringify(h.stored()) !== before;
}

test("choosing a level and applying it writes that level, enforced, once", async () => {
  const h = build(null);
  h.protection.open();
  h.choose("comments");
  h.enforce.checked = true;
  await applyVia(h);
  assert.equal(h.stored().edit, "comments");
  assert.equal(h.stored().enforcement, true);
  assert.equal(h.stored().active, true);
  assert.equal(h.changed(), 1);
  assert.match(h.status.at(-1).text, /comments/i, "it says what it did");
});

test("an enforced readOnly document can be unprotected -- the one-way door", async () => {
  // THE central case. A document arrives restricted to readOnly and enforced.
  // Lifting that restriction is an edit made ON a document that refuses edits,
  // which is why the engine needed `exempt_from_protection` and why the chrome
  // must not re-gate it.
  const h = build(json("readOnly", true));
  assert.equal(h.stored().active, true, "precondition: the document IS restricted");
  h.protection.open();
  h.choose("off");
  h.enforce.checked = false;
  const lifted = await applyVia(h);
  assert.equal(
    lifted,
    true,
    "a readOnly document could not be unprotected: the chrome refused the one operation ADR-059 exists to allow",
  );
  assert.equal(h.stored().edit, null);
  assert.equal(h.stored().active, false);
  assert.equal(h.status.at(-1).text, EN["protect.removed"]);
});

test("a protected document can be moved straight to another level", async () => {
  // The same door, in the direction people actually use: relaxing readOnly to
  // comments without stopping protection first.
  const h = build(json("readOnly", true));
  h.protection.open();
  h.choose("comments");
  h.enforce.checked = true;
  const moved = await applyVia(h);
  assert.equal(moved, true, "a readOnly document could not be relaxed to comments");
  assert.equal(h.stored().edit, "comments");
  assert.equal(h.stored().active, true);
});

test("Apply writes nothing when nothing the reader can see has changed", async () => {
  // Opening the dialog on an `edit="none"` document and pressing Apply must not
  // rewrite the file, which is what keeps a no-op out of the undo stack.
  const h = build(json("none", false));
  h.protection.open();
  h.choose("off");
  h.enforce.checked = false;
  await applyVia(h);
  assert.equal(h.stored().edit, "none", "the token is untouched, not rewritten to null");
  assert.equal(h.changed(), 0, "and no repaint was requested");
  assert.equal(h.status.length, 0, "and nothing was announced");
  assert.equal(h.closed(), 1, "the dialog still closes");
});

test("w:formatting is carried through rather than defaulted away", async () => {
  // This dialog does not offer Word's style whitelist, so Apply must not switch
  // off a `w:formatting` restriction the document already carries.
  const h = build(json("readOnly", true, true));
  h.protection.open();
  h.choose("comments");
  h.enforce.checked = true;
  await applyVia(h);
  assert.equal(h.stored().formatting, true, "the formatting lock was dropped behind the reader");
});

test("the command row is live on a document that is ALREADY protected", () => {
  // A restriction nobody can reach the dialog to lift is worse than no
  // restriction at all, so this row is never disabled by the protection itself.
  const h = build(json("readOnly", true));
  const [row] = h.protection.commands();
  assert.equal(row.id, "review.restrictEditing");
  assert.equal(row.enabled, true);
  assert.equal(row.disabledReason, "");
  assert.equal(h.protection.isActive(), true, "while the ribbon button reads as pressed");
});
