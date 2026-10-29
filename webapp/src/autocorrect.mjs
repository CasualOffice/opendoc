/** AutoCorrect options and their application as you type (`docs/174` TA-05,
 *  TA-06; `docs/109` OO-016, RM-19; `docs/153` `text.autocapitalise`,
 *  `text.autocorrect-replace`, `text.autoformat-hyperlink`,
 *  `text.autoformat-dashes`).
 *
 *  The switches are the ones Word's AutoCorrect and "AutoFormat As You Type"
 *  tabs and ONLYOFFICE's AutoCorrect dialog open with:
 *  - `smartQuotes`: straight quotes become typographic ones (the pref key is
 *    the one it always had, so a user's choice survives);
 *  - `lists`: `1. `, `1) `, `- ` or `* ` at the start of a paragraph becomes a
 *    numbered or bulleted list;
 *  - `autocorrect`: replace text as you type, from the per-language editable
 *    table in `autocorrect_table.mjs` (`(c)` → ©, `teh` → the);
 *  - `capitalise`: the first letter of a sentence, with the per-language
 *    first-letter exceptions; `capitaliseCells`: the first letter of a table
 *    cell, ONLYOFFICE's separate switch (`RunAutoCorrect.js:785-789`);
 *  - `dashes`: `--` and ` - ` become em and en dashes;
 *  - `hyperlinks`: a typed or pasted address becomes a link.
 *
 *  `capitalise` and `dashes` used to be part of `autocorrect`. A user who had
 *  turned that off keeps them off: an unset new switch inherits the old one.
 *
 *  Each is a remembered preference, reachable from the AutoCorrect options
 *  dialog, the command palette and (for the first three) Settings (SKILL §10:
 *  two surfaces), and on by default as in Word.
 *
 *  THE UNDO CONTRACT, which Word, Docs and ONLYOFFICE share: what was typed lands
 *  first, as typing; the conversion is then applied as its OWN undoable action
 *  (`runEdit` without `typing`, which breaks the typing session), so an
 *  immediate Undo gives back exactly what was typed ("1. ", "teh ", the address
 *  without its link). The list conversion is one engine action
 *  (`autoFormatList`) for the same reason.
 *
 *  Only in Editing mode and at a collapsed caret: a tracked conversion would
 *  need its deletion and its property change tracked together, which the
 *  engine refuses rather than half-does.
 *
 *  `io` supplies: `readPref(key)`, `writePref(key, value)`, `getDoc()`,
 *  `getSelection()`, `editing()` (Editing mode, not Suggesting or Viewing),
 *  `runEdit(thunk, options)` (main.js's), `setStatus(text)`, `reflect()`
 *  (re-reflect the chrome), `t(key, params)`, `defaultLanguage()` (the tag to
 *  use where the text declares none), `tables` (an `autocorrect_table.mjs`
 *  store; built over the prefs when absent), and `toggles` — the checkboxes by
 *  switch name, each an element or a list of elements, any of which may be
 *  absent.
 *
 *  Complexity: one `copyText` of the caret's paragraph prefix and one
 *  `languageSpans` of its paragraph per word-ending keystroke, and an `inTable`
 *  only for a paragraph's FIRST word; never O(document) per keystroke.
 */

import { autocorrectFor, hyperlinkFor, listTriggerFor } from "./input_rules.mjs";
import { stringIndexToByteOffset } from "./text_rules.mjs";
import { createAutoCorrectTables } from "./autocorrect_table.mjs";
import { spanLanguageAt } from "./language_selection.mjs";

/** The switches, their remembered pref key, their display-name key, and the
 *  older switch an unset one inherits. */
const SWITCHES = {
  smartQuotes: { pref: "opendoc.smartQuotes", name: "autocorrect.smartQuotes" },
  lists: { pref: "opendoc.autoFormatLists", name: "autocorrect.lists" },
  autocorrect: { pref: "opendoc.autocorrect", name: "autocorrect.replace" },
  capitalise: { pref: "opendoc.autoCapitalise", name: "autocorrect.capitalise", inherits: "opendoc.autocorrect" },
  capitaliseCells: { pref: "opendoc.autoCapitaliseCells", name: "autocorrect.capitaliseCells", inherits: "opendoc.autoCapitalise" },
  dashes: { pref: "opendoc.autoFormatDashes", name: "autocorrect.dashes", inherits: "opendoc.autocorrect" },
  hyperlinks: { pref: "opendoc.autoFormatHyperlink", name: "autocorrect.hyperlinks" },
};

/** Characters after which a rule might fire. Anything else returns at once. */
const TRIGGERS = new Set([" ", "\t", ".", ",", ";", ":", "!", "?", ")"]);

/** The switches that edit the text before the caret on a word boundary. */
const WORD_RULES = ["autocorrect", "capitalise", "capitaliseCells", "dashes", "hyperlinks"];

export function createAutoCorrect(io) {
  const { t } = io;
  const tables = io.tables ?? createAutoCorrectTables(io);
  const state = {};
  for (const [key, { pref, inherits }] of Object.entries(SWITCHES)) {
    let stored = io.readPref(pref);
    // An unset switch inherits the one it was split out of, through the chain.
    for (let from = inherits; stored === null && from; ) {
      stored = io.readPref(from);
      from = Object.values(SWITCHES).find((s) => s.pref === from)?.inherits;
    }
    state[key] = stored !== "off";
  }

  const elementsFor = (key) => [io.toggles?.[key]].flat().filter(Boolean);

  function reflectToggles() {
    for (const key of Object.keys(SWITCHES)) {
      for (const element of elementsFor(key)) element.checked = state[key];
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

  for (const key of Object.keys(SWITCHES)) {
    for (const element of elementsFor(key)) element.addEventListener?.("change", () => set(key, element.checked));
  }
  reflectToggles();

  /** The language the text at `offset` of `node` is in, for the per-language
   *  lists: the run's own `w:lang`, else the host's default. */
  function languageAt(doc, node, offset) {
    let spans = [];
    try {
      spans = JSON.parse(doc.languageSpans?.(node) ?? "[]");
    } catch {
      spans = [];
    }
    return spanLanguageAt(spans, offset) || io.defaultLanguage?.() || "en";
  }

  /** The edit the text before `offset` of `node` calls for, now that `typed`
   *  ended it, or null. `virtual` marks a boundary that is not in the text (a
   *  paragraph break typed after the word): the rules see a space there, and
   *  the replacement stops short of it. */
  function editFor(doc, node, offset, typed, virtual) {
    const text = doc.copyText(node, 0, node, offset);
    // A paragraph whose text does not map byte for byte onto the engine's
    // offsets (an inline object or field before the caret) is left alone
    // rather than corrected at the wrong place.
    if (stringIndexToByteOffset(text, text.length) !== offset) return null;
    const prefix = virtual ? `${text} ` : text;
    if (!virtual && state.lists && typed === " ") {
      const trigger = listTriggerFor(prefix);
      if (trigger && !doc.listStyleAt(node)) {
        const marker = stringIndexToByteOffset(trigger.marker, trigger.marker.length);
        return { kind: "list", run: () => doc.autoFormatList(node, marker, offset, trigger.kind) };
      }
    }
    if (state.hyperlinks && /\s/.test(prefix.at(-1))) {
      const link = hyperlinkFor(prefix);
      if (link) {
        const start = stringIndexToByteOffset(prefix, link.start);
        const end = stringIndexToByteOffset(prefix, link.end);
        // Never re-target a link the address is being typed inside.
        if (!doc.hyperlinkTouches?.(node, start, end)) {
          return {
            kind: "hyperlink",
            run: () => doc.setHyperlink(node, start, end, link.target, undefined),
            // The link wraps text that is already there, so the caret stays put.
            keepSelection: true,
          };
        }
      }
    }
    if (!state.autocorrect && !state.capitalise && !state.capitaliseCells && !state.dashes) return null;
    const language = languageAt(doc, node, offset);
    // The first word of a paragraph is governed by the cell switch inside a
    // table cell and by the sentence switch elsewhere. `inTable` is asked only
    // then — once per paragraph, not per keystroke.
    const firstWord = /^\s*\S+$/.test(prefix.slice(0, -1));
    const inCell = firstWord && Boolean(doc.inTable?.(node));
    const fix = autocorrectFor(prefix, {
      replace: state.autocorrect,
      dashes: state.dashes,
      capitalize: state.capitalise,
      capitalizeStart: inCell ? state.capitaliseCells : state.capitalise,
      table: tables.replacements(language),
      exceptions: tables.exceptions(language),
    });
    if (!fix) return null;
    // The replaced range runs to the CARET, carrying what was typed after the
    // word, so the caret lands where it was and Undo restores both.
    const start = stringIndexToByteOffset(prefix, fix.start);
    let replacement = fix.replacement + prefix.slice(fix.end);
    if (virtual) replacement = replacement.slice(0, -1);
    return { kind: "autocorrect", run: () => doc.replaceSelection(node, start, node, offset, replacement), keepSelection: virtual };
  }

  /** The edit `typed` should turn into at the caret, or null. Exported through
   *  the returned object for tests; `afterTyped` applies it. */
  function pending(typed) {
    if (!TRIGGERS.has(typed) || !io.editing()) return null;
    if (!state.lists && !WORD_RULES.some((key) => state[key])) return null;
    const doc = io.getDoc();
    const selection = io.getSelection();
    if (!doc || !selection) return null;
    const { anchor, focus } = selection;
    if (anchor.node !== focus.node || anchor.offset !== focus.offset) return null;
    return editFor(doc, focus.node, focus.offset, typed, false);
  }

  async function apply(edit) {
    if (!edit) return false;
    await io.runEdit(edit.run, edit.keepSelection ? { keepSelection: true } : undefined);
    return true;
  }

  return {
    /** Whether a switch is on. */
    enabled: (key) => state[key] === true,
    /** The switch names, in the dialog's order. */
    switches: () => Object.keys(SWITCHES),
    /** The palette and menu label: the switch name and its state, composed by the catalogue. */
    label,
    set,
    toggle: (key) => set(key, !state[key]),
    pending,
    /** The per-language lists the dialog edits. */
    tables,
    /** Applies the conversion `typed` calls for, as its own undo step. Returns
     *  whether one was applied. */
    afterTyped: async (typed) => apply(pending(typed)),
    /** After a paragraph break typed at the end of `node` (its old caret
     *  `offset`): the word the break finished is corrected as a space would
     *  have corrected it, on the paragraph it stayed in, and the caret stays in
     *  the new one. Word does this on Enter too. */
    async afterBreak(node, offset) {
      if (!io.editing() || !WORD_RULES.some((key) => state[key])) return false;
      const doc = io.getDoc();
      if (!doc || !node) return false;
      return apply(editFor(doc, node, offset, "\n", true));
    },
    /** After a plain-text paste of `text` that left the caret collapsed: a
     *  pasted lone address is linked at once, as Google Docs does, and as its
     *  own undo step, so the first Undo leaves the address as plain text. */
    async afterPaste(text) {
      const pasted = String(text ?? "").trim();
      if (!state.hyperlinks || !pasted || /\s/.test(pasted) || !io.editing()) return false;
      const doc = io.getDoc();
      const selection = io.getSelection();
      if (!doc || !selection) return false;
      const { anchor, focus } = selection;
      if (anchor.node !== focus.node || anchor.offset !== focus.offset) return false;
      const prefix = doc.copyText(focus.node, 0, focus.node, focus.offset);
      if (stringIndexToByteOffset(prefix, prefix.length) !== focus.offset || !prefix.endsWith(pasted)) return false;
      const link = hyperlinkFor(`${prefix} `);
      // Only when the address IS the paste, not a word that merely ends with it.
      if (!link || link.end !== prefix.length || link.start !== prefix.length - pasted.length) return false;
      const start = stringIndexToByteOffset(prefix, link.start);
      if (doc.hyperlinkTouches?.(focus.node, start, focus.offset)) return false;
      return apply({ run: () => doc.setHyperlink(focus.node, start, focus.offset, link.target, undefined), keepSelection: true });
    },
  };
}
