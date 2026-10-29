// Word count: the dialog, and the selection's count in the status bar (HF-051).
//
// THE SHAPE, FROM THE THREE REFERENCES.
//
//   * Word: Review ▸ Proofing ▸ Word Count, Ctrl+Shift+G (⌘⇧G on a Mac), and a
//     click on the status bar's word count, all open one dialog — Pages, Words,
//     Characters (no spaces), Characters (with spaces), Paragraphs, Lines. With
//     text selected the status bar reads "23 of 1,204 words" and the dialog
//     counts the selection.
//   * Google Docs: Tools ▸ Word count (Ctrl+Shift+C) — Pages, Words, Characters,
//     Characters excluding spaces, "x of y" for a selection.
//   * ONLYOFFICE: a "Word count" button in the status bar whose menu lists
//     Pages, Paragraphs, Words, Symbols and Spaces for the document
//     (`web-apps/apps/documenteditor/main/app/view/Statusbar.js:325-343`), and
//     the same block under File ▸ Document Info ▸ Statistics
//     (`view/FileMenuPanels.js:1464-1479`). Neither counts a selection.
//
// So: Word's dialog and Word's chord, reached from the Review band's Proofing
// group, the Review menu, the palette, the chord and the status bar itself, with
// a Selection column beside Document whenever there is one, and Word's "x of y
// words" in the status bar.
//
// WHERE WE DIFFER. Word's "Lines" row is not offered: a line count is a layout
// figure, and the engine counts text, not lines. Word's "Include textboxes,
// footnotes and endnotes" switch is not offered either, because the engine's
// count always includes them (and headers and footers): the dialog SAYS so in a
// sentence rather than offering a switch that could not change the number.
// Not the Ctrl+Shift+C of Google Docs: that chord is the format painter here, as
// it is in Word.
//
// COUNTING. The document figures are the engine's own (`documentStats`, O(1)
// while its ledger is current). The selection's are counted here from the text
// `copyText` returns, by the engine's rule — a word is a maximal run of
// non-whitespace, whitespace is Unicode `White_Space` (Rust's
// `char::is_whitespace`), and the paragraph separators `copyText` writes are not
// characters. A paragraph counts toward the selection when the selection holds
// at least one of its characters.
import { n, t } from "./i18n.mjs";
import { orderedSelectionEnds } from "./caret_navigation.mjs";
import { registerModal } from "./modal.mjs";

const WHITE_SPACE = /\p{White_Space}/u;

/**
 * Counts a selection's plain text the way the engine counts a paragraph.
 *
 * `text` is `copyText`'s output: paragraphs joined by `\n`. Complexity:
 * O(characters), one pass, no allocation per character.
 *
 * @param {string} text
 * @returns {{words: number, characters: number, charactersWithSpaces: number, paragraphs: number}}
 *   `characters` excludes whitespace, as the engine's field of that name does.
 */
export function countText(text) {
  let words = 0;
  let characters = 0;
  let charactersWithSpaces = 0;
  let paragraphs = 0;
  let inWord = false;
  let paragraphHasText = false;
  for (const ch of String(text ?? "")) {
    if (ch === "\n") {
      if (paragraphHasText) paragraphs += 1;
      paragraphHasText = false;
      inWord = false;
      continue;
    }
    paragraphHasText = true;
    charactersWithSpaces += 1;
    if (WHITE_SPACE.test(ch)) {
      inWord = false;
      continue;
    }
    characters += 1;
    if (!inWord) words += 1;
    inWord = true;
  }
  if (paragraphHasText) paragraphs += 1;
  return { words, characters, charactersWithSpaces, paragraphs };
}

/** True when `selection` covers at least one position (not a bare caret). */
export function selectionIsRange(selection) {
  if (!selection?.anchor || !selection?.focus) return false;
  return selection.anchor.node !== selection.focus.node || selection.anchor.offset !== selection.focus.offset;
}

/** The engine's document figures, read and freed. O(1) while its ledger is current. */
function documentFigures(doc) {
  const s = doc.documentStats();
  const out = {
    pages: s.pages,
    words: s.words,
    characters: s.characters,
    charactersWithSpaces: s.charactersWithSpaces,
    paragraphs: s.paragraphs,
  };
  s.free();
  return out;
}

/** The selection's figures, or null for a caret. O(selection). */
function selectionFigures(doc, selection) {
  if (!selectionIsRange(selection)) return null;
  const [start, end] = orderedSelectionEnds(selection, doc);
  return countText(doc.copyText(start.node, start.offset, end.node, end.offset));
}

/** The rows the dialog shows, in Word's order. `selection: false` marks a figure
 *  that has no meaning for a selection (a selection has no page count of its own). */
const ROWS = Object.freeze([
  Object.freeze({ key: "pages", selection: false }),
  Object.freeze({ key: "words", selection: true }),
  Object.freeze({ key: "characters", selection: true }),
  Object.freeze({ key: "charactersWithSpaces", selection: true }),
  Object.freeze({ key: "paragraphs", selection: true }),
]);

/** How long the selection must sit still before the status bar recounts it. A
 *  drag-select moves the selection every frame, and the count is O(selection),
 *  so it is taken once the hand stops rather than per frame. */
const SETTLE_MS = 120;

/**
 * Wires the Word count dialog and the status bar's selection count.
 *
 * @param {object} io
 * @param {() => object|null} io.getDoc the live document, or null.
 * @param {() => object|null} io.getSelection the editor selection, or null.
 * @param {() => Element|null} io.fallbackFocus where focus goes when the opener is gone.
 * @param {(text: string, kind?: string) => void} io.setStatus
 * @returns {{open: () => void, reflect: () => void, figures: () => object|null}}
 */
export function createWordCount(io) {
  const dialog = document.getElementById("wordCountDialog");
  const done = document.getElementById("wordCountDone");
  const statsEl = document.getElementById("stats");
  const statWords = document.getElementById("statWords");
  const modal = dialog
    ? registerModal(dialog, {
      initialFocus: () => done,
      fallbackFocus: io.fallbackFocus,
      defaultAction: () => modal?.close("done"),
      // The chord that opened it closes it again, as Word's does.
      toggleChord: (e) => (e.metaKey || e.ctrlKey) && e.shiftKey && !e.altKey && e.key.toLowerCase() === "g",
    })
    : null;
  for (const button of dialog?.querySelectorAll("[data-word-count-dismiss]") ?? []) {
    button.addEventListener("click", () => modal?.close("done"));
  }

  /** Both columns' figures, or null without a document. */
  function figures() {
    const doc = io.getDoc();
    if (!doc) return null;
    return { document: documentFigures(doc), selection: selectionFigures(doc, io.getSelection()) };
  }

  /** Fills the table. The Selection column is shown only when there is one, so a
   *  reader never sees a column of zeros that could be mistaken for a count. */
  function render(f) {
    if (!dialog) return;
    const hasSelection = !!f.selection;
    for (const cell of dialog.querySelectorAll("[data-word-count-selection]")) cell.hidden = !hasSelection;
    for (const row of ROWS) {
      const tr = dialog.querySelector(`[data-stat="${row.key}"]`);
      if (!tr) continue;
      tr.querySelector("[data-word-count-document]").textContent = n(f.document[row.key]);
      tr.querySelector("[data-word-count-selection]").textContent =
        hasSelection && row.selection ? n(f.selection[row.key]) : "—";
    }
  }

  function open() {
    const f = figures();
    if (!f) {
      io.setStatus(t("command.needsDocument"), "error");
      return;
    }
    render(f);
    modal?.open();
  }

  // ---- The status bar ------------------------------------------------------
  // Word's status bar reads "23 of 1,204 words" while text is selected. This
  // writes only the words cell, and only while there is a range or right after
  // one ends: a caret move with no range on either side costs one flag test,
  // which keeps the per-interaction path O(1) (`107` §4).
  let timer = 0;
  let showingSelection = false;

  function writeWordsCell() {
    timer = 0;
    const doc = io.getDoc();
    if (!doc || !statWords) return;
    const selection = io.getSelection();
    const total = documentFigures(doc).words;
    if (selectionIsRange(selection)) {
      const selected = selectionFigures(doc, selection).words;
      statWords.textContent = t("status.wordsOfTotal", { count: total, selected: n(selected) });
      showingSelection = true;
    } else {
      statWords.textContent = t("status.words", { count: total });
      showingSelection = false;
    }
  }

  function reflect() {
    if (!selectionIsRange(io.getSelection()) && !showingSelection) return;
    if (timer) clearTimeout(timer);
    timer = setTimeout(writeWordsCell, SETTLE_MS);
  }

  // The counts open the dialog, as Word's status bar does. A span made a real
  // button for the keyboard and a screen reader: role, a tab stop, Enter and
  // Space — the same reach a click has.
  if (statsEl) {
    statsEl.addEventListener("click", () => open());
    statsEl.addEventListener("keydown", (e) => {
      if (e.key !== "Enter" && e.key !== " ") return;
      e.preventDefault();
      open();
    });
  }

  return { open, reflect, figures };
}
