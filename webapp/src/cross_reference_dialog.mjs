// References ▸ Cross-reference (OO-005).
//
// Word's dialog, over the engine's `referenceTargets` / `insertCrossReference`.
// The mapping it turns on — which "Insert reference to" options a reference type
// offers, and when Word offers "Include above/below" — is in
// `cross_reference_model.mjs`, because it is a table and not a widget, and
// because ONLYOFFICE carries the same table as a switch statement inside its own
// view (`CrossReferenceDialog.js:289-350`) where nothing can test it.
//
// Insert leaves the dialog OPEN, as Word's does and as ONLYOFFICE's does
// (`insertReference()` at `:238` never closes); a document being cross-referenced
// usually needs several in a row.
import { t } from "./i18n.mjs";
import {
  CAPTION_KIND,
  REFERENCE_KINDS,
  aboveBelowEnabled,
  parseReferenceTargets,
  referenceToOptions,
} from "./cross_reference_model.mjs";

/** Every string this dialog composes, as LITERAL `t()` calls.
 *
 *  Spelled out rather than built as `t(`crossRefDialog.refTo.${name}`)`, which
 *  reads better and does not work: `build-locale.mjs` extracts keys by reading
 *  the call sites, so an interpolated key is invisible to it and the build
 *  reports it as "called but undeclared". `file_pane.mjs` records the same
 *  lesson. A key a tool cannot see is a key no translator is ever shown. */
const REF_TO_TEXT = Object.freeze({
  entireCaption: () => t("crossRefDialog.refTo.entireCaption"),
  labelAndNumber: () => t("crossRefDialog.refTo.labelAndNumber"),
  captionText: () => t("crossRefDialog.refTo.captionText"),
  pageNumber: () => t("crossRefDialog.refTo.pageNumber"),
  aboveBelow: () => t("crossRefDialog.refTo.aboveBelow"),
  headingText: () => t("crossRefDialog.refTo.headingText"),
  headingNumber: () => t("crossRefDialog.refTo.headingNumber"),
  headingNumberNoContext: () => t("crossRefDialog.refTo.headingNumberNoContext"),
  headingNumberFullContext: () => t("crossRefDialog.refTo.headingNumberFullContext"),
  bookmarkText: () => t("crossRefDialog.refTo.bookmarkText"),
  paragraphNumber: () => t("crossRefDialog.refTo.paragraphNumber"),
  paragraphNumberNoContext: () => t("crossRefDialog.refTo.paragraphNumberNoContext"),
  paragraphNumberFullContext: () => t("crossRefDialog.refTo.paragraphNumberFullContext"),
  footnoteNumber: () => t("crossRefDialog.refTo.footnoteNumber"),
  endnoteNumber: () => t("crossRefDialog.refTo.endnoteNumber"),
});

const TYPE_TEXT = Object.freeze({
  heading: () => t("crossRefDialog.type.heading"),
  bookmark: () => t("crossRefDialog.type.bookmark"),
  footnote: () => t("crossRefDialog.type.footnote"),
  endnote: () => t("crossRefDialog.type.endnote"),
});

/** The heading over the target list. Word renames it per type ("For which
 *  heading", "For which footnote") because "For which caption" over a list of
 *  headings is a sentence that contradicts what is under it. */
const WHICH_TEXT = Object.freeze({
  [CAPTION_KIND]: () => t("crossRefDialog.forWhich.caption"),
  heading: () => t("crossRefDialog.forWhich.heading"),
  bookmark: () => t("crossRefDialog.forWhich.bookmark"),
  footnote: () => t("crossRefDialog.forWhich.footnote"),
  endnote: () => t("crossRefDialog.forWhich.endnote"),
});

/** `"k:heading"` for one of the engine's fixed kinds, `"c:Figure"` for a caption
 *  label. Prefixed because a document may legitimately use a caption label
 *  spelled "heading", and an unprefixed value would then name two things. */
function typeValue(kind, label) {
  return label === undefined ? `k:${kind}` : `c:${label}`;
}

/** The `{ kind, engineKind, label }` a type value stands for. `kind` is what the
 *  option mapping keys on; `engineKind` is what `referenceTargets` takes, which
 *  for a caption IS the label. */
function typeOf(value) {
  const rest = String(value ?? "").slice(2);
  return String(value ?? "").startsWith("c:")
    ? { kind: CAPTION_KIND, engineKind: rest, label: rest }
    : { kind: rest, engineKind: rest, label: "" };
}

/**
 * Builds the Cross-reference dialog over the markup already in the page.
 *
 * `io`:
 *   `getDoc()`          the open document, or null
 *   `caret()`           `{ node, offset }` the reference is inserted at
 *   `aboveBelowSupported()` whether this build's `insertCrossReference` carries
 *                       the include-above/below argument
 *   `mutationBlocked()` true (having said why) when the review mode refuses
 *   `insert(options)`   Promise<boolean>; one gated, undoable engine call
 *   `status(text, kind)` the status line
 *   `registerModal(dialog, options)` the host's modal registry
 *   `fallbackFocus()`   where focus goes when the opener has gone away
 */
export function createCrossReferenceDialog(io) {
  const el = (id) => document.getElementById(id);
  const dialog = el("crossRefDialog");
  if (!dialog) return { open() {}, close() {} };

  const typeSelect = el("crossRefType");
  const toSelect = el("crossRefTo");
  const hyperlink = el("crossRefHyperlink");
  const aboveBelow = el("crossRefAboveBelow");
  const aboveBelowRow = el("crossRefAboveBelowRow");
  const whichLabel = el("crossRefWhich");
  const list = el("crossRefList");
  const empty = el("crossRefEmpty");
  const insertBtn = el("crossRefInsert");
  const closeBtn = el("crossRefClose");
  const doneBtn = el("crossRefDone");

  let targets = [];
  let selectedNode = "";

  const modal = io.registerModal(dialog, {
    initialFocus: () => typeSelect,
    fallbackFocus: io.fallbackFocus,
  });

  function close() {
    modal.close();
  }

  /** "Include above/below" is live only where Word offers it AND where the engine
   *  can carry it. Two different refusals, and the difference matters to the
   *  reader: "Word does not offer it for this kind of reference" is a rule they
   *  can work with, "the engine cannot do it yet" is a gap. Collapsing them into
   *  one greyed box with one sentence would be the more comfortable lie.
   *
   *  The engine half is DETECTED rather than assumed, from the arity of the
   *  binding this build is running against, so the switch comes alive the moment
   *  `insertCrossReference` grows its sixth parameter instead of waiting for a
   *  webapp commit to notice. */
  function reflectAboveBelow() {
    const { kind } = typeOf(typeSelect.value);
    const offered = aboveBelowEnabled(kind, toSelect.value);
    const carried = io.aboveBelowSupported();
    aboveBelow.disabled = !offered || !carried;
    if (aboveBelow.disabled) aboveBelow.checked = false;
    aboveBelowRow.title = !offered
      ? t("crossRefDialog.aboveBelowNotForThis")
      : carried
        ? ""
        : t("crossRefDialog.aboveBelowUnavailable");
  }

  /** Paints the target list and returns whether anything is in it. */
  function fillTargets() {
    list.replaceChildren();
    for (const target of targets) {
      const li = document.createElement("li");
      li.className = "bookmark-row";
      const button = document.createElement("button");
      button.type = "button";
      button.className = "bookmark-goto crossref-target";
      button.dataset.node = target.node;
      button.setAttribute("role", "option");
      button.setAttribute("aria-selected", String(target.node === selectedNode));
      button.textContent = target.text;
      li.append(button);
      list.append(li);
    }
    empty.hidden = targets.length > 0;
    list.hidden = targets.length === 0;
    insertBtn.disabled = !selectedNode;
    return targets.length > 0;
  }

  function selectTarget(node) {
    selectedNode = node;
    for (const button of list.querySelectorAll(".crossref-target")) {
      button.setAttribute("aria-selected", String(button.dataset.node === node));
      button.classList.toggle("is-active", button.dataset.node === node);
    }
    insertBtn.disabled = !node;
  }

  /** Reads the targets for the selected type. ONE engine walk per type change,
   *  which is what `referenceTargets` promises; nothing here calls it per row. */
  function loadTargets() {
    const doc = io.getDoc();
    const { engineKind } = typeOf(typeSelect.value);
    if (!doc || !engineKind) {
      targets = [];
      selectedNode = "";
      fillTargets();
      return;
    }
    try {
      targets = parseReferenceTargets(doc.referenceTargets(engineKind));
    } catch (error) {
      targets = [];
      io.status(error?.message ?? String(error), "error");
    }
    selectedNode = targets[0]?.node ?? "";
    fillTargets();
  }

  /** Rebuilds the "Insert reference to" combo for the selected type, keeping the
   *  author's choice when the new type also offers it — Word does the same
   *  (`CrossReferenceDialog.js:352-356`). */
  function fillReferenceTo() {
    const { kind } = typeOf(typeSelect.value);
    const previous = toSelect.value;
    const options = referenceToOptions(kind);
    toSelect.replaceChildren();
    for (const { referenceTo, name } of options) {
      const option = document.createElement("option");
      option.value = referenceTo;
      option.textContent = REF_TO_TEXT[name]?.() ?? name;
      toSelect.append(option);
    }
    if (options.some((option) => option.referenceTo === previous)) toSelect.value = previous;
    whichLabel.textContent = WHICH_TEXT[kind]?.() ?? "";
    reflectAboveBelow();
  }

  /** Fills the type combo: the engine's four fixed kinds, then one row per
   *  caption label the document uses. The labels come from the engine's own
   *  union with Word's built-ins, so "Figure" is offered in a document that has
   *  no figures yet — and a label the document invented is offered too, which is
   *  the case a hardcoded list of three cannot serve. */
  function fillTypes(labels) {
    const previous = typeSelect.value;
    typeSelect.replaceChildren();
    for (const kind of REFERENCE_KINDS) {
      const option = document.createElement("option");
      option.value = typeValue(kind);
      option.textContent = TYPE_TEXT[kind]?.() ?? kind;
      typeSelect.append(option);
    }
    for (const label of labels) {
      const option = document.createElement("option");
      option.value = typeValue(CAPTION_KIND, label);
      option.textContent = label;
      typeSelect.append(option);
    }
    if ([...typeSelect.options].some((option) => option.value === previous)) {
      typeSelect.value = previous;
    }
  }

  function open() {
    const doc = io.getDoc();
    if (!doc || !io.caret() || io.mutationBlocked()) return;
    let labels = [];
    try {
      labels = doc.captionLabels();
    } catch (error) {
      io.status(error?.message ?? String(error), "error");
      return;
    }
    fillTypes(labels);
    fillReferenceTo();
    loadTargets();
    modal.open();
  }

  typeSelect.addEventListener("change", () => {
    fillReferenceTo();
    loadTargets();
  });
  toSelect.addEventListener("change", reflectAboveBelow);
  closeBtn.addEventListener("click", close);
  doneBtn.addEventListener("click", close);

  list.addEventListener("click", (event) => {
    const button = event.target.closest(".crossref-target");
    if (button) selectTarget(button.dataset.node);
  });
  // Double-click inserts, as ONLYOFFICE's list does (`item:dblclick` at `:177`).
  list.addEventListener("dblclick", (event) => {
    if (event.target.closest(".crossref-target")) void insert();
  });
  list.addEventListener("keydown", (event) => {
    const buttons = [...list.querySelectorAll(".crossref-target")];
    const at = buttons.indexOf(document.activeElement);
    if (event.key === "Enter" && at !== -1) {
      event.preventDefault();
      selectTarget(buttons[at].dataset.node);
      void insert();
      return;
    }
    const step = event.key === "ArrowDown" ? 1 : event.key === "ArrowUp" ? -1 : 0;
    if (!step || at === -1) return;
    event.preventDefault();
    const next = buttons[Math.min(buttons.length - 1, Math.max(0, at + step))];
    next?.focus();
    if (next) selectTarget(next.dataset.node);
  });

  async function insert() {
    const caret = io.caret();
    if (!caret || !selectedNode) return;
    const inserted = await io.insert({
      caretNode: caret.node,
      caretOffset: caret.offset,
      targetNode: selectedNode,
      referenceTo: toSelect.value,
      hyperlink: hyperlink.checked,
      // Passed whatever the binding's arity: a sixth argument to a five-argument
      // function is ignored in JS, and the checkbox is disabled-and-clear in that
      // build anyway, so the value is `false` either way.
      includeAboveBelow: aboveBelow.checked,
    });
    if (!inserted) return;
    io.status(t("crossReference.inserted"));
    // The dialog stays open (Word, ONLYOFFICE), so the list has to be refreshed
    // rather than left describing the document as it was before the insert —
    // page numbers move, and the reference itself is now part of the flow.
    loadTargets();
  }

  insertBtn.addEventListener("click", () => void insert());

  return { open, close };
}
