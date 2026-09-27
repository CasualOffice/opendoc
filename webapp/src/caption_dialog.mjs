// References ▸ Insert caption (OO-005).
//
// Word's dialog, over the engine's `captionLabels` / `captionEntries` /
// `insertCaption`. Everything that is a RULE rather than a widget — which
// numbering formats exist, what the composed caption reads as, which sequence
// number a new caption takes — is in `cross_reference_model.mjs` and unit-tested
// in node; what is here is the dialog, and it owns no vocabulary of its own
// beyond the nine heading levels it has to generate.
//
// The module adopts markup already in `editor.html` by id, the way
// `drop_cap.mjs` and `bookmark_manager.mjs` do, so the dialog's structure stays
// readable in the document it renders in and this file stays about behaviour.
import { has, t } from "./i18n.mjs";
import {
  NUMBER_FORMATS,
  SEPARATORS,
  captionPreview,
  captionsBefore,
  formatSequenceNumber,
  mergeCaptionLabels,
  nextCaptionNumber,
  parseCaptionEntries,
} from "./cross_reference_model.mjs";

/** What the preview puts where the chapter number will go. The real value comes
 *  from a STYLEREF to the nearest heading of the chosen level, which only the
 *  engine can resolve, so a digit here would be a guess presented as a fact. */
const CHAPTER_PLACEHOLDER = "#";

/** Word's three built-in labels, which cannot be deleted from the combo — the
 *  `type: 0` rows of `CaptionDialog.js:165-168`. The engine unions the same
 *  three into `captionLabels()`, so this list only decides what Delete refuses;
 *  it is never what populates the combo. */
const BUILT_IN_LABELS = ["Equation", "Figure", "Table"];

/**
 * Builds the Insert caption dialog over the markup already in the page.
 *
 * `io` is its whole contact with the application:
 *
 *   `getDoc()`          the open document, or null
 *   `targetNode()`      the block the caption attaches to: the caret's
 *                       paragraph, which is also the paragraph that holds a
 *                       selected picture or table
 *   `mutationBlocked()` true (having said why) when the review mode refuses
 *   `insert(options)`   Promise<boolean>; one gated, undoable engine call
 *   `inserted()`        optional; run after a successful insert, for state the
 *                       insert invalidates (the stale-caption count)
 *   `status(text, kind)` the status line
 *   `registerModal(dialog, options)` the host's modal registry
 *   `fallbackFocus()`   where focus goes when the opener has gone away
 */
export function createCaptionDialog(io) {
  const el = (id) => document.getElementById(id);
  const dialog = el("captionDialog");
  if (!dialog) return { open() {}, close() {} };

  const form = el("captionForm");
  const textInput = el("captionText");
  const labelSelect = el("captionLabel");
  const positionSelect = el("captionPosition");
  const newLabelBtn = el("captionNewLabelBtn");
  const deleteLabelBtn = el("captionDeleteLabelBtn");
  const newLabelRow = el("captionNewLabelRow");
  const newLabelInput = el("captionNewLabelInput");
  const addLabelBtn = el("captionAddLabelBtn");
  const excludeLabel = el("captionExcludeLabel");
  const numberFormat = el("captionNumberFormat");
  const includeChapter = el("captionIncludeChapter");
  const chapterLevel = el("captionChapterLevel");
  const separator = el("captionSeparator");
  const chapterNote = el("captionChapterNote");
  const preview = el("captionPreview");
  const closeBtn = el("captionClose");
  const cancelBtn = el("captionCancel");

  /** Labels the author added in this session and has not used yet. They are not
   *  in the document, so the engine cannot report them; ONLYOFFICE keeps the
   *  same list in `localStorage` (`CaptionDialog.js:159-163`). Ours is
   *  session-scoped deliberately: a label is document content, and a list that
   *  outlives the document it was invented for is how one document's vocabulary
   *  leaks into the next. */
  const sessionLabels = [];

  /** The document's captions, in document order, read ONCE per open. Two
   *  separate walks (one for the labels, one for the entries) would make opening
   *  the dialog O(2·document); the engine already promises one walk each, and
   *  this reads each exactly once and then previews from memory as the user
   *  types. */
  let entries = [];
  let targetIndex = 0;

  /** The nine heading levels, filled ON OPEN rather than when this factory runs.
   *
   *  They used to be built here, in the factory body. Catalogues arrive
   *  ASYNCHRONOUSLY — `setCatalogue` is called "once a locale's JSON arrives" —
   *  and `t()` returns the KEY for a miss, deliberately, so it shows up. This ran
   *  before the catalogue did, stamped all nine options with the literal string
   *  `captionDialog.headingLevel`, and never rebuilt them: the raw key was on
   *  screen for the life of the tab, in every language.
   *
   *  The existing guards could not catch it. `locale_coverage` and
   *  `no_unrouted_strings` both check that a key EXISTS — and it does, in all 19
   *  catalogues. Nothing checked that a key had been resolved by the time it was
   *  painted. `dialog-i18n-keys.spec.mjs` now does.
   *
   *  Idempotent, because `open()` can be called repeatedly. */
  function fillChapterLevels() {
    const wanted = 9;
    if (chapterLevel.options.length === wanted && chapterLevel.dataset.i18nReady === "1") return;
    chapterLevel.replaceChildren();
    for (let level = 1; level <= wanted; level += 1) {
      const option = document.createElement("option");
      option.value = String(level);
      option.textContent = t("captionDialog.headingLevel", { level });
      chapterLevel.append(option);
    }
    // Only mark it done when the catalogue actually answered. If it has not yet,
    // the next open rebuilds rather than keeping a key on screen forever — which
    // is the whole defect this replaces.
    if (has("captionDialog.headingLevel")) chapterLevel.dataset.i18nReady = "1";
  }

  const modal = io.registerModal(dialog, {
    initialFocus: () => textInput,
    fallbackFocus: io.fallbackFocus,
  });

  function close() {
    modal.close();
  }

  /** The label the combo is on, or "" while the document has none at all. */
  function currentLabel() {
    return labelSelect.value ?? "";
  }

  function reflectChapterState() {
    const on = includeChapter.checked;
    chapterLevel.disabled = !on;
    separator.disabled = !on;
    chapterNote.hidden = !on;
  }

  /** Delete is for a label the AUTHOR invented and has not used. A built-in and
   *  a label some caption in the document already carries both have to stay, or
   *  the combo would stop offering a label the document is full of. */
  function reflectDeleteState() {
    const label = currentLabel();
    const removable =
      !!label &&
      !BUILT_IN_LABELS.includes(label) &&
      !entries.some((entry) => entry.label === label);
    deleteLabelBtn.disabled = !removable;
  }

  /** Repaints the whole composed caption. This is the one thing Word gets for
   *  free by prefilling its input and we have to do deliberately. */
  function reflectPreview() {
    const label = currentLabel();
    const number = formatSequenceNumber(
      nextCaptionNumber(entries, label, targetIndex),
      numberFormat.value,
    );
    preview.value = captionPreview({
      label,
      number,
      text: textInput.value,
      excludeLabel: excludeLabel.checked,
      chapter: includeChapter.checked ? CHAPTER_PLACEHOLDER : "",
      separator: separator.value,
    });
  }

  function reflectAll() {
    reflectChapterState();
    reflectDeleteState();
    reflectPreview();
  }

  /** Refills the label combo, keeping the author on the label they had. */
  function fillLabels(labels, keep) {
    labelSelect.replaceChildren();
    for (const label of labels) {
      const option = document.createElement("option");
      option.value = label;
      option.textContent = label;
      labelSelect.append(option);
    }
    if (keep && labels.includes(keep)) labelSelect.value = keep;
  }

  function open() {
    const doc = io.getDoc();
    const node = io.targetNode();
    if (!doc || !node || io.mutationBlocked()) return;
    let labels = [];
    try {
      labels = mergeCaptionLabels(doc.captionLabels(), sessionLabels);
      entries = parseCaptionEntries(doc.captionEntries());
    } catch (error) {
      console.warn("caption read ignored:", error?.message ?? error);
      io.status(error?.message ?? String(error), "error");
      return;
    }
    targetIndex = captionsBefore(entries, node);
    fillChapterLevels();
    fillLabels(labels, currentLabel());
    newLabelRow.hidden = true;
    reflectAll();
    modal.open();
  }

  newLabelBtn.addEventListener("click", () => {
    newLabelRow.hidden = false;
    newLabelInput.focus();
  });

  function addLabel() {
    const label = newLabelInput.value.trim();
    if (!label) {
      io.status(t("captionDialog.labelNameRequired"), "error");
      newLabelInput.focus();
      return;
    }
    sessionLabels.push(label);
    fillLabels(mergeCaptionLabels([...labelSelect.options].map((o) => o.value), sessionLabels), label);
    newLabelInput.value = "";
    newLabelRow.hidden = true;
    reflectAll();
    labelSelect.focus();
  }

  addLabelBtn.addEventListener("click", addLabel);
  // Enter inside the inline label field adds the label rather than submitting
  // the form, which would insert a caption the author has not finished
  // describing. The row is inside `#captionForm` so the browser would otherwise
  // treat Enter as "press the submit button".
  newLabelInput.addEventListener("keydown", (event) => {
    if (event.key !== "Enter") return;
    event.preventDefault();
    addLabel();
  });

  deleteLabelBtn.addEventListener("click", () => {
    const label = currentLabel();
    const at = sessionLabels.indexOf(label);
    if (at === -1) {
      io.status(t("captionDialog.labelNotRemovable"), "error");
      return;
    }
    sessionLabels.splice(at, 1);
    const kept = [...labelSelect.options].map((o) => o.value).filter((value) => value !== label);
    fillLabels(kept, kept[0]);
    reflectAll();
    labelSelect.focus();
  });

  for (const control of [labelSelect, numberFormat, separator]) {
    control.addEventListener("change", reflectAll);
  }
  for (const control of [excludeLabel, includeChapter]) {
    control.addEventListener("change", reflectAll);
  }
  textInput.addEventListener("input", reflectPreview);
  closeBtn.addEventListener("click", close);
  cancelBtn.addEventListener("click", close);

  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    const label = currentLabel();
    if (!label) return;
    const inserted = await io.insert({
      targetNode: io.targetNode(),
      label,
      text: textInput.value,
      position: positionSelect.value,
      excludeLabel: excludeLabel.checked,
      numberFormat: NUMBER_FORMATS.includes(numberFormat.value) ? numberFormat.value : "arabic",
      chapterLevel: includeChapter.checked ? Number(chapterLevel.value) || 1 : 0,
      separator: SEPARATORS.includes(separator.value) ? separator.value : "hyphen",
    });
    if (!inserted) return;
    close();
    io.inserted?.();
    io.status(t("caption.inserted"));
  });

  return { open, close };
}
