// Text entry that is not a keystroke: soft keyboards and dictation (docs/105
// UX-001), and AutoCorrect as you type. Moved out of `main.js` unchanged
// (docs/177); it reaches the rest of the editor only through the context `ed`.
import { t } from "./i18n.mjs";
import { createAutoCorrect } from "./autocorrect.mjs";
import { readPref, writePref } from "./prefs.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createTextInput(ed) {
  // ---- Soft-keyboard / dictation text entry (docs/105 UX-001) ----------------
  // The keydown path below handles hardware keyboards, and it is still the only
  // thing that inserts a character there. It cannot carry touch input: Android
  // and iOS soft keyboards deliver printable characters as `keyCode 229` /
  // `key: "Unidentified"`, and swipe-typing, dictation and autocorrect emit no
  // usable keydown at all. `beforeinput` is the event those surfaces do fire.
  //
  // Deliberately narrow, so this does NOT become a second, divergent apply path —
  // the defect class HF-007 recorded when four copy-pasted edit paths drifted:
  //   * insertion reuses `pasteText(..., "typing")`, the exact primitive
  //     `commitComposedText` already uses, so review mode, selection replacement
  //     and history coalescing behave identically to an IME commit;
  //   * DELETION is intentionally left to keydown. Backspace and Delete are real
  //     named keys that soft keyboards do report, so there is no gap to close and
  //     every reason not to open a second delete path.
  // If you are here to add `deleteContentBackward`, first prove keydown misses it.
  document.addEventListener("beforeinput", async (e) => {
    if (!ed.editorTextInputEvent(e)) return;
    // A composition in flight owns its own text; `compositionend` commits it.
    if (ed.composingText) return;
    const type = e.inputType;
    if (type === "insertText" || type === "insertFromPaste" || type === "insertReplacementText") {
      const data = e.data ?? "";
      if (!data) return;
      e.preventDefault();
      ed.pendingFormat = null;
      await ed.pasteText(data, "typing");
      return;
    }
    if (type === "insertLineBreak" || type === "insertParagraph") {
      e.preventDefault(); // An IME / on-screen-keyboard soft break IS Shift+Enter, not a new paragraph (HF-296, `docs/174` TA-01).
      await (type === "insertLineBreak" ? ed.insertLineBreakAtSelection() : ed.pasteText("\n", "typing"));
    }
  });

  // The proxy must never accumulate text: the engine is the source of truth, and
  // a textarea holding a stale value would resend it on the next composition.
  if (ed.editorTextInputEl) {
    ed.editorTextInputEl.addEventListener("input", () => {
      if (ed.composingText) return; // clearing mid-composition cancels the IME
      ed.editorTextInputEl.value = "";
    });
  }

  document.addEventListener("compositionstart", (e) => {
    if (!ed.editorTextInputEvent(e)) return;
    ed.breakTypingSession();
    ed.composingText = true;
    ed.pendingFormat = null;
    if (ed.selection) ed.showImePreedit(ed.selection.focus.node, ed.selection.focus.offset, e.data || "");
  });
  document.addEventListener("compositionupdate", (e) => {
    if (!ed.composingText) return;
    ed.updateImePreedit(e.data || "");
  });
  document.addEventListener("compositionend", async (e) => {
    ed.hideImePreedit();
    if (!ed.editorTextInputEvent(e)) {
      ed.composingText = false;
      return;
    }
    e.preventDefault();
    ed.composingText = false;
    // `preventDefault` on compositionend does not reliably stop the browser from
    // leaving the composed text in the textarea, so clear it explicitly.
    if (ed.editorTextInputEl) ed.editorTextInputEl.value = "";
    await ed.commitComposedText(e.data || "");
  });

  // ---- AutoCorrect: smart quotes, automatic lists, corrections (`autocorrect.mjs`) --
  // The quote decision is `smartQuoteForTyped` (text_rules.mjs); the list and
  // correction rules are `input_rules.mjs`; each conversion is its own undo step.
  const autoCorrect = createAutoCorrect({
    readPref,
    writePref,
    t,
    getDoc: () => ed.doc,
    getSelection: () => ed.selection,
    editing: () => ed.reviewMode === "editing",
    runEdit: (thunk) => ed.runEdit(thunk),
    setStatus: (text) => ed.setStatus(text),
    reflect: () => ed.updateToolbar(),
    toggles: Object.fromEntries([["smartQuotes", "smartQuotesToggle"], ["lists", "autoFormatListsToggle"], ["autocorrect", "autocorrectToggle"]].map(([key, id]) => [key, document.getElementById(id)])),
  });

  /** Adds a word and SAYS whether it was stored. A personal dictionary that
   *  silently failed to persist is indistinguishable from one that worked until
   *  the next reload, and there is no management surface yet to notice with. */
  async function addWordToDictionary(word) {
    const stored = await ed.spellChecker.addToDictionary(word);
    ed.setStatus(
      stored
        ? `Added “${word}” to your dictionary`
        : `“${word}” is accepted for this session — your dictionary could not be saved`,
      stored ? "" : "error",
    );
  }

  return { addWordToDictionary, autoCorrect };
}
