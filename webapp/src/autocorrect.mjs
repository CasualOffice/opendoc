/** AutoCorrect options and their application as you type (`docs/174` TA-05,
 *  TA-06; `docs/109` OO-016).
 *
 *  Three switches, the ones Word's "AutoFormat As You Type" and "AutoCorrect"
 *  tabs open with:
 *  - `smartQuotes`: straight quotes become typographic ones (moved here from
 *    `main.js`, where it was the only one; the pref key is unchanged, so a user's
 *    choice survives the move);
 *  - `lists`: `1. `, `1) `, `- ` or `* ` at the start of a paragraph becomes a
 *    numbered or bulleted list;
 *  - `autocorrect`: `--` becomes a dash, `(c)` ©, `(r)` ®, `(tm)` ™, common typos
 *    are fixed, and the first letter of a sentence is capitalised.
 *
 *  Each is a remembered preference, reachable from Settings and from the command
 *  palette (SKILL §10: two surfaces), and on by default as it is in Word.
 *
 *  THE UNDO CONTRACT, which Word, Docs and ONLYOFFICE share: what was typed lands
 *  first, as typing; the conversion is then applied as its OWN undoable action,
 *  so an immediate Undo gives back exactly what was typed ("1. ", "teh "). The
 *  list conversion is one engine action (`autoFormatList`) for the same reason:
 *  removing the marker and making the list item are a single step or the first
 *  Undo would leave a list item with its marker gone.
 *
 *  Only in Editing mode and at a collapsed caret: a tracked conversion would
 *  need its deletion and its property change tracked together, which the
 *  engine refuses rather than half-does.
 *
 *  `io` supplies: `readPref(key)`, `writePref(key, value)`, `getDoc()`,
 *  `getSelection()`, `editing()` (Editing mode, not Suggesting or Viewing),
 *  `runEdit(thunk)` (main.js's), `setStatus(text)`, `reflect()` (re-reflect the
 *  chrome), `t(key, params)`, and `toggles` — the Settings checkboxes by switch
 *  name, any of which may be absent.
 *
 *  Complexity: one `copyText` of the caret's paragraph prefix per word-ending
 *  keystroke (the cost smart quotes already pays), never O(document).
 */

import { autocorrectFor, listTriggerFor } from "./input_rules.mjs";
import { stringIndexToByteOffset } from "./text_rules.mjs";

/** The switches, their remembered pref key, and their display-name key. */
const SWITCHES = {
  smartQuotes: { pref: "opendoc.smartQuotes", name: "autocorrect.smartQuotes" },
  lists: { pref: "opendoc.autoFormatLists", name: "autocorrect.lists" },
  autocorrect: { pref: "opendoc.autocorrect", name: "autocorrect.text" },
};

/** Characters after which a rule might fire. Anything else returns at once. */
const TRIGGERS = new Set([" ", ".", ",", ";", ":", "!", "?", ")"]);

export function createAutoCorrect(io) {
  const { t } = io;
  const state = {};
  for (const [key, { pref }] of Object.entries(SWITCHES)) state[key] = io.readPref(pref) !== "off";

  function reflectToggles() {
    for (const [key, element] of Object.entries(io.toggles ?? {})) {
      if (element) element.checked = state[key];
    }
  }

  function label(key) {
    return t("autocorrect.command", {
      name: t(SWITCHES[key].name),
      state: t(state[key] ? "autocorrect.stateOn" : "autocorrect.stateOff"),
    });
  }

  function set(key, on) {
    if (!(key in SWITCHES)) return;
    state[key] = Boolean(on);
    io.writePref(SWITCHES[key].pref, state[key] ? "on" : "off");
    io.setStatus(t(state[key] ? "autocorrect.statusOn" : "autocorrect.statusOff", { name: t(SWITCHES[key].name) }));
    reflectToggles();
    io.reflect?.();
  }

  for (const [key, element] of Object.entries(io.toggles ?? {})) {
    element?.addEventListener?.("change", () => set(key, element.checked));
  }
  reflectToggles();

  /** The edit `typed` should turn into at the caret, or null. Exported through
   *  the returned object for tests; `afterTyped` applies it. */
  function pending(typed) {
    if (!TRIGGERS.has(typed) || (!state.lists && !state.autocorrect) || !io.editing()) return null;
    const doc = io.getDoc();
    const selection = io.getSelection();
    if (!doc || !selection) return null;
    const { anchor, focus } = selection;
    if (anchor.node !== focus.node || anchor.offset !== focus.offset) return null;
    const { node, offset } = focus;
    const prefix = doc.copyText(node, 0, node, offset);
    // A paragraph whose text does not map byte for byte onto the engine's
    // offsets (an inline object or field before the caret) is left alone
    // rather than corrected at the wrong place.
    if (stringIndexToByteOffset(prefix, prefix.length) !== offset) return null;
    if (state.lists && typed === " ") {
      const trigger = listTriggerFor(prefix);
      if (trigger && !doc.listStyleAt(node)) {
        const marker = stringIndexToByteOffset(trigger.marker, trigger.marker.length);
        return { kind: "list", run: () => doc.autoFormatList(node, marker, offset, trigger.kind) };
      }
    }
    if (state.autocorrect) {
      const fix = autocorrectFor(prefix);
      if (fix) {
        // The replaced range runs to the CARET, carrying what was typed after the
        // word, so the caret lands where it was and Undo restores both.
        const start = stringIndexToByteOffset(prefix, fix.start);
        const text = fix.replacement + prefix.slice(fix.end);
        return { kind: "autocorrect", run: () => doc.replaceSelection(node, start, node, offset, text) };
      }
    }
    return null;
  }

  return {
    /** Whether a switch is on. */
    enabled: (key) => state[key] === true,
    /** The palette and menu label: the switch name and its state, composed by the catalogue. */
    label,
    set,
    toggle: (key) => set(key, !state[key]),
    pending,
    /** Applies the conversion `typed` calls for, as its own undo step. Returns
     *  whether one was applied. */
    async afterTyped(typed) {
      const edit = pending(typed);
      if (!edit) return false;
      await io.runEdit(edit.run);
      return true;
    },
  };
}
