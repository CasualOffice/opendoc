// The ¶ button: that the marks the ENGINE reports are the marks the UI shows,
// and that one switch never moves another.
//
// `docs/153` `view.formatting-marks`. The engine has painted all five marks for a
// long time and `webapp/` never called the setter, so the most-pressed button in a
// word processor did not exist (`SKILL` §9 rule 4). The chrome landed in #732; this
// is the guard that did not, and the two defects it exists for are both the kind
// that ship green.
//
// ---- DEFECT 1: THE LYING TOGGLE -------------------------------------------
//
// `createFormattingMarks` could keep its own boolean — `let on = false; on = !on`
// — and light the button from that. It would look right in every happy path and
// be wrong in every other: a patch the engine REFUSED would still leave the button
// pressed, a mark the engine declined to turn on would still read as checked, and
// a document that arrived with marks already on would read as off. The module
// avoids that by re-reading `doc.formattingMarks` on every reflect, which is a
// property nothing in the markup can express and nothing in a "the marks appeared"
// test can see.
//
// So the fake document below is a REAL STATE MACHINE, not an echo: it resolves
// `all` before the individual keys the way `set_formatting_marks_inner` does,
// refuses a key it does not declare the way the facade does, and can be driven
// from outside the chrome. An echoing fake would let a local flag pass.
//
// ---- DEFECT 2: THE PATCH THAT IS NOT A PATCH ------------------------------
//
// `setFormattingMarks` takes a patch precisely so one popover row can move one
// mark. Sending `{all: …}` from a row, or rebuilding the whole object from the
// chrome's own remembered preference, would make five switches into one — and
// again, every single-switch test would still pass, because turning `space` on
// does turn `space` on. What fails is the OTHER four, which is what is asserted.
//
// ---- MUTATION PROOF -------------------------------------------------------
//
// Both mutations were applied to `src/formatting_marks.mjs`, run, and restored.
// The output below is what they actually printed, not what they ought to have.
//
// (1) THE LYING TOGGLE. `let pressed = false;` beside the preference, `toggleAll`
// doing `pressed = !pressed; apply({ all: pressed })`, and `reflect` writing
// `String(pressed)` instead of `String(!!state.any)`:
//
//   not ok 4 - marks the chrome did not set are still the marks the chrome shows
//     error: |-
//       'false' !== 'true'
//   not ok 6 - the ¶ button is pressed when ANY mark is on, not when all are
//     error: |-
//       one mark on is the ¶ button's own state
//       'false' !== 'true'
//   not ok 8 - the person's choice is replayed onto a freshly opened document, idempotently
//     error: |-
//       'false' !== 'true'
//   # tests 13 / # pass 10 / # fail 3
//
// Worth recording which test did NOT redden: `a refused patch leaves the button
// telling the truth` stayed green under this exact flag, because `apply` returns
// before `reflect` on a refusal, so the stale attribute happened to be right. A
// flag written in `reflect` rather than read there is a second shape of the same
// defect and that test is what holds it; neither test is redundant.
//
// (2) REPAGINATION INSTEAD OF REPAINT. `apply` calling `doc.relayout()` where it
// calls `io.repaint()`:
//
//   not ok 7 - turning marks on is a view change: no operation, no revision, no repagination
//     error: |-
//       turning formatting marks on asked the engine to re-lay the document
//       1 !== 0
//   # tests 13 / # pass 12 / # fail 1
//
// That mutation is synthetic in one respect, and it matters: the real document
// handle has no `relayout`, because the chrome's only view lever is the `repaint`
// thunk `main.js` injects. The PRODUCTION shape of this defect is therefore one
// word in `main.js` — `repaint: () => void renderAll()` instead of the loop over
// `pageWindow` — and that is held by `tests/e2e/view-formatting-marks.spec.mjs`,
// which is driven red by exactly that swap. Two halves of one guarantee, each
// where it can be seen.
import assert from "node:assert/strict";
import test from "node:test";

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { setCatalogue, setLocale } from "../src/i18n.mjs";

// The RUNTIME catalogue, for the reason `document_protection.test.mjs` gives: the
// five mark names are MARKUP keys, declared in `editor.html` beside their English,
// so they are absent from `en_strings.mjs` and the palette rows compose them into
// their own labels. Priming the script-only table would assert against dotted keys.
const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");
const EN = JSON.parse(readFileSync(join(WEBAPP, "locales/en.json"), "utf8"));
setCatalogue("en", EN);
setLocale("en");

/** The five keys, in the module's own order — written out rather than imported,
 *  so a key quietly renamed on one side of the boundary is a failure here and not
 *  a test that renames itself to match. */
const KEYS = ["tab", "space", "paragraph", "lineBreak", "pageBreak"];

// `formatting_marks.mjs` reads its three elements by id at construction, and
// `popover_manager.mjs` installs two document-level listeners when it is imported.
// Both need a `document` to exist before the import.
const elements = new Map();
globalThis.document = {
  getElementById: (id) => elements.get(id) ?? null,
  addEventListener: () => {},
  activeElement: null,
};
// `navigator` is a read-only global in Node 22, and that is fine: `authoredTitle`
// takes the platform through `keyboardPlatform(navigator)`, Node's own user agent
// matches neither mac nor iOS, so the titles restore with the standard chord
// spellings — which is also what the Linux CI runner sees (`SKILL` §11).

/** A `localStorage` that is a Map, so the preference is observable and the module
 *  needs no browser. `prefs.mjs` reaches `globalThis` by default. */
const store = new Map();
globalThis.localStorage = {
  getItem: (key) => (store.has(key) ? store.get(key) : null),
  setItem: (key, value) => void store.set(key, String(value)),
  removeItem: (key) => void store.delete(key),
};

const { FORMATTING_MARKS, FORMATTING_MARK_COMMANDS, createFormattingMarks } = await import(
  "../src/formatting_marks.mjs"
);

/**
 * A faithful stand-in for the two facade members this chrome uses.
 *
 * It is `set_formatting_marks_inner`'s own rules, not an echo:
 *
 *  - `all` resolves FIRST, so an individual key written beside it wins;
 *  - a key the facade does not declare is REFUSED with a sentence, because the
 *    setter throws rather than painting something it was not asked for;
 *  - `any` is computed by the engine, never by the caller;
 *  - the setter bumps a view epoch and NOT the revision, and issues no operation.
 *
 * `relayout` and `pageCount` exist so the "this is a view, not an edit" assertion
 * has something to be false about: a chrome that repaginated would have to call
 * one of them.
 */
function fakeDoc({ marks = {}, refuse = null } = {}) {
  const state = Object.fromEntries(KEYS.map((key) => [key, !!marks[key]]));
  const doc = {
    revision: 11,
    pageCount: 23,
    epoch: 0,
    relayouts: 0,
    patches: [],
    relayout() {
      doc.relayouts += 1;
      doc.pageCount = 23;
    },
    get formattingMarks() {
      return JSON.stringify({
        ...state,
        color: null,
        any: KEYS.some((key) => state[key]),
      });
    },
    setFormattingMarks(json) {
      const patch = JSON.parse(json);
      doc.patches.push(patch);
      if (refuse) throw new Error(refuse);
      for (const key of Object.keys(patch)) {
        if (key !== "all" && !KEYS.includes(key)) {
          throw new Error(`formatting marks: unknown field \`${key}\``);
        }
      }
      if (patch.all !== undefined) for (const key of KEYS) state[key] = !!patch.all;
      for (const key of KEYS) if (patch[key] !== undefined) state[key] = !!patch[key];
      doc.epoch += 1;
      return doc.formattingMarks;
    },
    /** What another surface does — an open, a host call, a second control. The
     *  chrome must never be told; it must read. */
    forceMarks(next) {
      for (const key of KEYS) state[key] = !!next[key];
    },
    marks: () => ({ ...state }),
  };
  return doc;
}

/** The three elements `editor.html` carries, as plain objects. `dataset` holds the
 *  authored title the way the markup does, so `authoredTitle` has something real
 *  to restore and the "disabled carries a reason" assertion is not vacuous. */
function fakeMarkup() {
  const attrs = new Map();
  const node = (title) => ({
    disabled: true,
    title,
    dataset: { enabledTitle: title },
    attributes: attrs,
    addEventListener: () => {},
    setAttribute: (name, value) => void attrs.set(name, value),
    getAttribute: (name) => attrs.get(name) ?? null,
  });
  const toggle = node("Formatting marks");
  const menuButton = node("Choose which formatting marks show");
  const rows = new Map(
    KEYS.map((key) => {
      const checked = new Map();
      return [
        key,
        {
          dataset: { mark: key },
          addEventListener: () => {},
          setAttribute: (name, value) => void checked.set(name, value),
          getAttribute: (name) => checked.get(name) ?? null,
        },
      ];
    }),
  );
  const menu = {
    hidden: true,
    addEventListener: () => {},
    querySelectorAll: (selector) =>
      selector === "[data-mark]" ? [...rows.values()] : [],
  };
  elements.clear();
  elements.set("formattingMarksBtn", toggle);
  elements.set("formattingMarksMenuBtn", menuButton);
  elements.set("formattingMarksMenu", menu);
  return { toggle, menuButton, menu, rows };
}

function build({ marks = {}, refuse = null, noDoc = false, preference = null } = {}) {
  const markup = fakeMarkup();
  store.clear();
  if (preference) store.set("opendoc.formattingMarks", JSON.stringify(preference));
  const doc = fakeDoc({ marks, refuse });
  const status = [];
  let repaints = 0;
  const control = createFormattingMarks({
    getDoc: () => (noDoc ? null : doc),
    repaint: () => void (repaints += 1),
    setStatus: (text, kind) => void status.push({ text, kind }),
  });
  return {
    control,
    doc,
    status,
    ...markup,
    repaints: () => repaints,
    /** The ¶ button as a screen reader would read it. */
    pressed: () => markup.toggle.getAttribute("aria-pressed"),
    /** One popover row, likewise. */
    checked: (key) => markup.rows.get(key).getAttribute("aria-checked"),
  };
}

// ---------------------------------------------------------------------------
// The marks the engine reports are the marks the UI shows.
// ---------------------------------------------------------------------------

test("every mark the engine can paint has a switch, and every switch a command", () => {
  assert.deepEqual(
    FORMATTING_MARKS.map((mark) => mark.key),
    KEYS,
    "the five keys are the engine's own patch fields, in Word's Display order",
  );
  // Six rows: the ¶ gesture and the five switches. A palette that offered the
  // gesture only would leave the parts reachable from one surface.
  assert.deepEqual(FORMATTING_MARK_COMMANDS, [
    "view.formattingMarks",
    ...KEYS.map((key) => `view.formattingMarks.${key}`),
  ]);
  for (const mark of FORMATTING_MARKS) assert.ok(EN[mark.labelKey], mark.labelKey);
});

test("pressing ¶ shows every mark, and the UI reads back what the engine holds", () => {
  const h = build();
  h.control.reflect();
  assert.equal(h.pressed(), "false", "a fresh document shows no marks");
  const [paragraphRow] = h.control.commands();
  paragraphRow.run();
  assert.deepEqual(
    h.doc.marks(),
    Object.fromEntries(KEYS.map((key) => [key, true])),
    "Word's ¶ shows ALL of them rather than restoring a remembered subset",
  );
  assert.equal(h.pressed(), "true");
  for (const key of KEYS) assert.equal(h.checked(key), "true", key);
  // And off again from one press, which is the `any`-decides-the-direction rule.
  paragraphRow.run();
  assert.equal(h.pressed(), "false");
  for (const key of KEYS) assert.equal(h.checked(key), "false", key);
});

test("a refused patch leaves the button telling the truth", () => {
  // THE LYING-TOGGLE CASE. The setter throws for a patch it does not understand,
  // and a control bound to its own wish would light up anyway — reporting marks
  // the page is not painting, with no way for a reader to tell.
  const h = build({ refuse: "formatting marks: expected a JSON object" });
  h.control.reflect(); // what `boot()` does, so the baseline is the engine's
  assert.equal(h.pressed(), "false", "precondition: nothing is showing");
  const [row] = h.control.commands();
  row.run();
  assert.equal(
    h.pressed(),
    "false",
    "the button reported itself pressed after the engine refused the patch",
  );
  for (const key of KEYS) assert.equal(h.checked(key), "false", key);
  assert.equal(h.status.length, 1, "and the refusal is shown rather than swallowed");
  assert.equal(h.status[0].kind, "error");
  assert.match(h.status[0].text, /formatting marks/);
  assert.equal(h.repaints(), 0, "nothing changed, so nothing was repainted");
});

test("marks the chrome did not set are still the marks the chrome shows", () => {
  // A document can arrive with marks on — a host call, a second surface, a
  // restored session. `reflect` must read the engine, not a remembered boolean.
  const h = build();
  h.control.reflect();
  assert.equal(h.pressed(), "false");
  h.doc.forceMarks({ space: true, tab: true });
  h.control.reflect();
  assert.equal(h.pressed(), "true", "a document that arrived with marks on read as off");
  assert.equal(h.checked("space"), "true");
  assert.equal(h.checked("tab"), "true");
  assert.equal(h.checked("paragraph"), "false", "and only the marks that are on");
  assert.equal(h.control.isOn(), true);
  assert.equal(h.control.state().space, true);
});

// ---------------------------------------------------------------------------
// One switch moves one mark.
// ---------------------------------------------------------------------------

test("toggling one mark does not disturb the other four", () => {
  const h = build();
  const row = (key) =>
    h.control.commands().find((entry) => entry.id === `view.formattingMarks.${key}`);
  row("space").run();
  assert.deepEqual(h.doc.marks(), {
    tab: false,
    space: true,
    paragraph: false,
    lineBreak: false,
    pageBreak: false,
  });
  // The patch carried ONE key. A row that sent the whole object, or `all`, would
  // satisfy "space is on" and fail here.
  assert.deepEqual(h.doc.patches.at(-1), { space: true });

  row("pageBreak").run();
  assert.equal(h.doc.marks().space, true, "turning the break rule on switched the space dots off");
  assert.equal(h.doc.marks().pageBreak, true);
  assert.deepEqual(h.doc.patches.at(-1), { pageBreak: true });

  // And off again, one at a time, with the other survivor untouched.
  row("space").run();
  assert.equal(h.doc.marks().space, false);
  assert.equal(h.doc.marks().pageBreak, true, "switching one mark off switched another off too");
  assert.equal(h.checked("pageBreak"), "true");
  assert.equal(h.checked("space"), "false");
});

test("the ¶ button is pressed when ANY mark is on, not when all are", () => {
  const h = build();
  const row = h.control
    .commands()
    .find((entry) => entry.id === "view.formattingMarks.lineBreak");
  row.run();
  assert.equal(h.pressed(), "true", "one mark on is the ¶ button's own state");
  // And the gesture from there is OFF, not "fill in the other four".
  h.control.commands()[0].run();
  assert.equal(h.control.isOn(), false);
  assert.deepEqual(h.doc.patches.at(-1), { all: false });
});

// ---------------------------------------------------------------------------
// It is a view change.
// ---------------------------------------------------------------------------

test("turning marks on is a view change: no operation, no revision, no repagination", () => {
  const h = build();
  const revisionBefore = h.doc.revision;
  const pagesBefore = h.doc.pageCount;
  h.control.commands()[0].run();
  assert.equal(
    h.doc.relayouts,
    0,
    "turning formatting marks on asked the engine to re-lay the document",
  );
  assert.equal(h.doc.revision, revisionBefore, "a view change must not bump the revision");
  assert.equal(h.doc.pageCount, pagesBefore, "nor move a page boundary");
  assert.equal(h.doc.epoch, 1, "one patch, so one view epoch");
  // ONE view refresh for the gesture, not one per mark: `setFormattingMarks` takes
  // the whole of `all` in a single call precisely so this is one repaint.
  assert.equal(h.repaints(), 1);
  assert.equal(h.doc.patches.length, 1);
});

test("the person's choice is replayed onto a freshly opened document, idempotently", () => {
  // `adopt()`. The preference outlives the document; the STATE lives on the
  // handle, because that is where the renderer reads it.
  const h = build({ preference: { tab: true, space: true } });
  h.control.adopt();
  assert.equal(h.doc.marks().tab, true, "the choice was not replayed onto the new document");
  assert.equal(h.doc.marks().space, true);
  assert.equal(h.doc.marks().paragraph, false, "and only the choice");
  assert.equal(h.pressed(), "true");

  // The default case costs nothing: no patch at all when nothing is wanted and
  // nothing is on, so opening a document does not pay for a feature nobody used.
  const fresh = build();
  fresh.control.adopt();
  assert.deepEqual(fresh.doc.patches, [], "a no-op adopt still sent a patch");
  assert.equal(fresh.repaints(), 0);
});

test("the chosen marks outlive the session", () => {
  const h = build();
  h.control.commands().find((row) => row.id === "view.formattingMarks.tab").run();
  assert.deepEqual(
    JSON.parse(store.get("opendoc.formattingMarks")),
    { tab: true, space: false, paragraph: false, lineBreak: false, pageBreak: false },
    "the preference is written, not merely held",
  );
});

// ---------------------------------------------------------------------------
// Never a dead control (`SKILL` §10).
// ---------------------------------------------------------------------------

test("with no document both halves are disabled CARRYING THE REASON", () => {
  const h = build({ noDoc: true });
  h.control.reflect();
  for (const half of [h.toggle, h.menuButton]) {
    assert.equal(half.disabled, true);
    assert.equal(half.title, EN["command.needsDocument"], "greyed out in silence");
  }
  for (const row of h.control.commands()) {
    assert.equal(row.enabled, false, row.id);
    assert.equal(row.disabledReason, EN["command.needsDocument"], row.id);
  }
});

test("with a document open the authored title comes back, not the refusal", () => {
  const h = build();
  h.control.reflect();
  assert.equal(h.toggle.disabled, false);
  assert.equal(h.toggle.title, "Formatting marks");
  assert.equal(h.menuButton.disabled, false);
  assert.equal(h.menuButton.title, "Choose which formatting marks show");
});

test("the command rows say what state each mark is in", () => {
  // The `view.compactRibbon` / `tools.smartQuotes` shape: a reader sees the state
  // in the palette without opening the popover.
  const h = build({ marks: { space: true } });
  const rows = h.control.commands();
  assert.equal(rows[0].label, EN["formattingMarks.commandOn"]);
  const space = rows.find((row) => row.id === "view.formattingMarks.space");
  assert.equal(space.label, "Spaces: on");
  const tab = rows.find((row) => row.id === "view.formattingMarks.tab");
  assert.equal(tab.label, "Tab characters: off");
  for (const row of rows) assert.equal(row.group, "View", row.id);
});

test("an unparseable getter reads as off rather than throwing out of a render pass", () => {
  const h = build();
  Object.defineProperty(h.doc, "formattingMarks", {
    configurable: true,
    get: () => "not json",
  });
  assert.doesNotThrow(() => h.control.reflect());
  assert.equal(h.pressed(), "false");
  assert.equal(h.control.isOn(), false);
});
