// View and text extras: Dark document, Multiple pages, Print selection, the
// personal dictionary, Alt+X and Insert text from file (`docs/153`
// shell.dark-document, shell.multiple-pages-view, shell.print-selection,
// review.personal-dictionary-manage, text.unicode-code-toggle, page.textFromFile).
//
// One module for six small capabilities because each is a few dozen lines of
// chrome over an engine call or a pure rule that lives elsewhere
// (`page_scroll.mjs`'s page grid, `unicode_toggle.mjs`, `print_selection.rs`,
// `insert_file.rs`), and `main.js` pays one import, one construction and one
// context getter for all of them. It reaches the editor only through `ed`.
//
// Every one of them is reachable from the palette (`paletteRows`) and from a
// menu (`command_taxonomy.mjs`); Print selection is on the text context menu as
// well, where ONLYOFFICE puts it, and Alt+X is a chord (`keymap.mjs`).
import { t } from "./i18n.mjs";
import { editRefusalMessage } from "./edit_errors.mjs";
import { registerModal } from "./modal.mjs";
import { loadPrefObject, savePrefObject } from "./prefs.mjs";
import { printDocument } from "./print.mjs";
import { insertFileLossMessage } from "./paste_loss.mjs";
import { PAGE_GAP_PX, setPageGrid } from "./page_scroll.mjs";
import { stringIndexToByteOffset } from "./text_rules.mjs";
import { toggleBeforeCaret, toggleSelection } from "./unicode_toggle.mjs";

/** View preferences: per reader, never in the file (Word keeps both as view
 *  state; ONLYOFFICE keeps Multiple pages in local storage). */
const PREF_KEY = "opendoc.viewExtras";
const PREF_DEFAULTS = Object.freeze({ darkDocument: false, multiplePages: false });

/** CSS px per twip at 100% zoom. */
const CSS_PER_TWIP_AT_100 = 96 / 1440;

/** The viewport's horizontal breathing room either side of a row of pages. */
const ROW_MARGIN_PX = 48;

/** The zoom Multiple pages drops to so that two pages sit side by side, at most
 *  the current zoom and never below 25%: the widest page twice, plus the gap,
 *  inside the viewport. Pure, so `editor_view_text.test.mjs`-style questions
 *  are arithmetic. */
export function multiplePagesZoom({ viewportWidth, pageWidthTwip, currentZoom, minZoom = 0.25 }) {
  const pageAt100 = pageWidthTwip * CSS_PER_TWIP_AT_100;
  if (!(pageAt100 > 0) || !(viewportWidth > 0)) return currentZoom;
  const fit = (viewportWidth - ROW_MARGIN_PX - PAGE_GAP_PX) / (2 * pageAt100);
  return Math.max(minZoom, Math.min(currentZoom, Math.floor(fit * 20) / 20));
}

/** The usable width a row of pages may take in the viewport. */
export function rowWidthFor(viewportWidth) {
  return Math.max(0, viewportWidth - ROW_MARGIN_PX);
}

/** The format id `insertDocumentFile` is asked to read a file as: Markdown by
 *  its name (Markdown is plain text, so the bytes cannot say), anything else
 *  detected from the bytes as Open does. */
export function insertFormatFor(name) {
  return /\.(md|markdown)$/i.test(String(name ?? "")) ? "text.markdown" : "";
}

/** The command ids `paletteRows` declares, in its order. A test reads the
 *  palette's own source for ids, and these rows are built here, so the list is
 *  exported for it (`keymap.test.mjs` binds Alt+X to one of them) and
 *  `editor_view_text.test.mjs` holds `paletteRows` to exactly these. */
export const VIEW_TEXT_COMMAND_IDS = Object.freeze([
  "view.darkDocument",
  "view.multiplePages",
  "file.printSelection",
  "tools.personalDictionary",
  "insert.symbolByCode",
  "insert.textFromFile",
]);

/** Installs this domain on the editor context `ed` (docs/177). */
export function createViewTextExtras(ed) {
  let preference = loadPrefObject(PREF_KEY, PREF_DEFAULTS);

  // ---- Dark document ---------------------------------------------------------
  // The page's raster inverted with its hue turned back, so black text on white
  // reads white on near-black and a red stays red. A paint-time filter only: the
  // file, the export and the print are untouched, which is the whole contract
  // ("a view setting that never changes the file"). Pictures are inverted with
  // the page — the raster does not separate them — which is the one place this
  // differs from Word's dark page; ONLYOFFICE's content theme has the same
  // limit for images.
  function applyDarkDocument() {
    ed.viewportEl?.classList.toggle("dark-document", preference.darkDocument);
  }

  function setDarkDocument(on) {
    preference = { ...preference, darkDocument: Boolean(on) };
    savePrefObject(PREF_KEY, preference);
    applyDarkDocument();
    ed.setStatus(t(on ? "darkDocument.on" : "darkDocument.off"));
  }

  // ---- Multiple pages --------------------------------------------------------
  let resizeTimer = 0;

  function viewportWidth() {
    return ed.viewportEl?.clientWidth ?? 0;
  }

  function applyPageGrid() {
    setPageGrid({ on: preference.multiplePages, availableWidth: rowWidthFor(viewportWidth()) });
  }

  function setMultiplePages(on) {
    preference = { ...preference, multiplePages: Boolean(on) };
    savePrefObject(PREF_KEY, preference);
    applyPageGrid();
    const widest = (ed.pages ?? []).reduce((most, page) => Math.max(most, page.wTwip ?? 0), 0);
    if (on && widest > 0) {
      const zoom = multiplePagesZoom({ viewportWidth: viewportWidth(), pageWidthTwip: widest, currentZoom: ed.zoomFactor });
      if (zoom !== ed.zoomFactor) {
        ed.setZoom(zoom, false);
        ed.setStatus(t("multiplePages.on"));
        return;
      }
    }
    void ed.renderAll?.();
    ed.setStatus(t(on ? "multiplePages.on" : "multiplePages.off"));
  }

  // The row width follows the window, as Word's and ONLYOFFICE's do: a wider
  // window holds more pages across.
  if (typeof ResizeObserver === "function" && ed.viewportEl) {
    let lastWidth = viewportWidth();
    new ResizeObserver(() => {
      if (!preference.multiplePages) return;
      const width = viewportWidth();
      if (width === lastWidth) return;
      lastWidth = width;
      clearTimeout(resizeTimer);
      resizeTimer = setTimeout(() => {
        applyPageGrid();
        void ed.renderAll?.();
      }, 150);
    }).observe(ed.viewportEl);
  }

  // ---- Print selection -------------------------------------------------------
  /** Prints only the selection: a throwaway copy of the document holding it
   *  (`selectionDocument`) through the same print path as File ▸ Print. */
  async function printSelection() {
    const selection = ed.selection;
    if (!ed.doc || !selection || !ed.hasRange()) {
      ed.setStatus(t("printSelection.needsRange"), "error");
      return;
    }
    let copy = null;
    try {
      copy = ed.doc.selectionDocument(selection.anchor.node, selection.anchor.offset, selection.focus.node, selection.focus.offset);
    } catch (err) {
      ed.setStatus(editRefusalMessage(err, { routeRefusal: ed.SESSION?.sentenceFor }), "error");
      return;
    }
    try {
      await printDocument(copy);
    } finally {
      copy.free();
    }
  }

  // ---- Personal dictionary ---------------------------------------------------
  const dictionaryDialog = document.getElementById("dictionaryDialog");
  const dictionaryWords = document.getElementById("dictionaryWords");
  const dictionaryEmpty = document.getElementById("dictionaryEmpty");
  const dictionaryStatus = document.getElementById("dictionaryStatus");
  const dictionaryWord = document.getElementById("dictionaryWord");
  const dictionaryClear = document.getElementById("dictionaryClear");
  const dictionaryModal = dictionaryDialog
    ? registerModal(dictionaryDialog, {
      initialFocus: () => dictionaryWord,
      fallbackFocus: () => ed.pagesEl,
    })
    : null;
  for (const button of dictionaryDialog?.querySelectorAll("[data-dictionary-dismiss]") ?? []) {
    button.addEventListener("click", () => dictionaryModal?.close("done"));
  }

  function renderDictionary() {
    if (!dictionaryWords) return;
    const words = [...(ed.spellChecker?.personalWords() ?? [])].sort((a, b) => a.localeCompare(b));
    dictionaryWords.replaceChildren(
      ...words.map((word) => {
        const item = document.createElement("li");
        item.className = "dictionary-word";
        const text = document.createElement("span");
        text.textContent = word;
        const remove = document.createElement("button");
        remove.type = "button";
        remove.className = "dialog-button dictionary-remove";
        remove.textContent = t("dictionaryDialog.remove");
        remove.setAttribute("aria-label", t("dictionaryDialog.removeWord", { word }));
        remove.addEventListener("click", () => void removeWord(word));
        item.append(text, remove);
        return item;
      }),
    );
    if (dictionaryEmpty) dictionaryEmpty.hidden = words.length > 0;
    if (dictionaryClear) dictionaryClear.disabled = words.length === 0;
  }

  /** Says what happened, and says it differently when storage refused: a word
   *  that only changed in memory is back after a reload (`docs/112` §4.1). */
  function sayDictionary(stored, done, word) {
    if (dictionaryStatus) dictionaryStatus.textContent = stored ? done : t("dictionaryDialog.notSaved", { word });
  }

  async function removeWord(word) {
    const stored = await ed.spellChecker.removeFromDictionary(word);
    renderDictionary();
    sayDictionary(stored, t("dictionaryDialog.removed", { word }), word);
    dictionaryWord?.focus({ preventScroll: true });
  }

  document.getElementById("dictionaryAddForm")?.addEventListener("submit", async (event) => {
    event.preventDefault();
    const word = dictionaryWord.value.trim();
    if (!word || /\s/.test(word)) {
      if (dictionaryStatus) dictionaryStatus.textContent = t("dictionaryDialog.oneWord");
      return;
    }
    const stored = await ed.spellChecker.addToDictionary(word);
    dictionaryWord.value = "";
    renderDictionary();
    sayDictionary(stored, t("dictionaryDialog.added", { word }), word);
  });

  dictionaryClear?.addEventListener("click", async () => {
    const stored = await ed.spellChecker.clearDictionary();
    renderDictionary();
    sayDictionary(stored, t("dictionaryDialog.cleared"), "");
    dictionaryWord?.focus({ preventScroll: true });
  });

  function openDictionary() {
    if (!ed.spellChecker) return;
    if (dictionaryStatus) dictionaryStatus.textContent = "";
    renderDictionary();
    dictionaryModal?.open();
  }

  // ---- Alt+X -----------------------------------------------------------------
  /** Word's Alt+X at the caret or over the selection (`unicode_toggle.mjs`). */
  async function toggleUnicode() {
    const selection = ed.selection;
    if (!ed.doc || !selection) return;
    const { anchor, focus } = selection;
    if (ed.hasRange()) {
      if (anchor.node !== focus.node) return;
      const selected = ed.doc.copyText(anchor.node, anchor.offset, focus.node, focus.offset);
      const replacement = toggleSelection(selected);
      if (replacement === null) {
        ed.setStatus(t("unicodeToggle.nothing"));
        return;
      }
      await ed.runEdit(() => ed.doc.replaceSelection(anchor.node, anchor.offset, focus.node, focus.offset, replacement));
      return;
    }
    const before = ed.doc.copyText(anchor.node, 0, anchor.node, anchor.offset);
    const edit = toggleBeforeCaret(before);
    if (!edit) {
      ed.setStatus(t("unicodeToggle.nothing"));
      return;
    }
    const start = stringIndexToByteOffset(before, before.length - edit.remove);
    await ed.runEdit(() => ed.doc.replaceSelection(anchor.node, start, anchor.node, anchor.offset, edit.insert));
  }

  // ---- Insert text from file -------------------------------------------------
  const insertFileInput = document.getElementById("insertFromFileInput");
  insertFileInput?.addEventListener("change", async () => {
    const file = insertFileInput.files?.[0];
    insertFileInput.value = "";
    if (!file || !ed.doc || !ed.selection) return;
    const bytes = new Uint8Array(await file.arrayBuffer());
    const caret = ed.selection.focus;
    let loss = null;
    const applied = await ed.runEdit(
      () => {
        const result = ed.doc.insertDocumentFile(caret.node, caret.offset, bytes, insertFormatFor(file.name));
        loss = insertFileLossMessage(result.pasteLoss, file.name);
        return result;
      },
      { gate: true },
    );
    if (!applied) return;
    ed.setStatus(loss ?? t("insertFile.done", { name: file.name }), loss ? "error" : "");
  });

  function insertFromFile() {
    if (!ed.doc || !ed.selection) {
      ed.setStatus(t("paragraph.caretRequired"), "error");
      return;
    }
    if (ed.blockMutationInViewing()) return;
    insertFileInput?.click();
  }

  applyDarkDocument();
  applyPageGrid();

  /** The palette rows. Labels read as the switch's state, the shape
   *  `view.compactRibbon` and `tools.spellCheck` already use. */
  function paletteRows() {
    const caret = !!ed.doc && !!ed.selection;
    return [
      { id: "view.darkDocument", label: t(preference.darkDocument ? "darkDocument.commandOn" : "darkDocument.commandOff"), group: "View", kw: "dark document page night invert black background reading mode theme", noDoc: true, run: () => setDarkDocument(!preference.darkDocument) },
      { id: "view.multiplePages", label: t(preference.multiplePages ? "multiplePages.commandOn" : "multiplePages.commandOff"), group: "View", kw: "multiple pages side by side two pages spread zoom out across overview", enabled: !ed.reflowView?.isOn?.(), disabledReason: t("multiplePages.reflow"), run: () => setMultiplePages(!preference.multiplePages) },
      { id: "file.printSelection", label: t("printSelection.command"), group: "File", kw: "print selection selected text only part", enabled: ed.HOST_CAPS.has("print") && !!ed.doc && ed.hasRange(), disabledReason: ed.HOST_CAPS.has("print") ? t("printSelection.needsRange") : t("capability.notGranted"), run: () => void printSelection() },
      { id: "tools.personalDictionary", label: t("dictionaryDialog.command"), group: "Tools", kw: "personal custom dictionary words added remove delete clear spelling edit word list", noDoc: true, run: () => openDictionary() },
      { id: "insert.symbolByCode", label: t("unicodeToggle.command"), group: "Insert", kw: "unicode code point hex alt x character convert symbol", enabled: caret && ed.reviewMode !== "viewing", disabledReason: t("paragraph.caretRequired"), run: () => void toggleUnicode() },
      { id: "insert.textFromFile", label: t("insertFile.command"), group: "Insert", kw: "insert text from file document merge combine object docx odt markdown txt", enabled: caret && ed.reviewMode !== "viewing", disabledReason: t("paragraph.caretRequired"), run: () => insertFromFile() },
    ];
  }

  return {
    darkDocumentOn: () => preference.darkDocument,
    multiplePagesOn: () => preference.multiplePages,
    insertFromFile,
    openDictionary,
    paletteRows,
    printSelection,
    setDarkDocument,
    setMultiplePages,
    toggleUnicode,
  };
}
