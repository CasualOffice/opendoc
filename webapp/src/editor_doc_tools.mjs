// Formatting marks, measurement units, access chrome, document protection, and
// Compare documents. Moved out of `main.js` unchanged (docs/177); it reaches
// the rest of the editor only through the context `ed`.
import { open, beginVersionDiff, defaultDiffSlice, decimalSeparatorForLanguage, defaultMeasurementUnit, formatMeasurement, measurementUnits, parseMeasurement } from "../pkg/casual_doc_wasm.js";
import { bindComparePanel, buildRedline, comparableBytes } from "./compare_documents.mjs";
import { mutationBlockedMessage } from "./edit_errors.mjs";
import { registerModal } from "./modal.mjs";
import { activeLocale, t } from "./i18n.mjs";
import { createFormattingMarks } from "./formatting_marks.mjs";
import { createMeasurementUnits } from "./measurement_units.mjs";
import { createDocumentProtection } from "./document_protection.mjs";
import { bindRadioGroup } from "./radio_group.mjs";
import { createAccessChrome } from "./access_chrome.mjs";
import { revealOnSettings } from "./surface_reveal.mjs";
import { createWordCount } from "./word_count.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createDocTools(ed) {
  // ---- Four engine capabilities that had no control at all --------------------
  // Each of these was reachable from the engine in one call and from the product in
  // none (`SKILL` §9 rule 4). What stays here is the editor state they need; every
  // decision — the competitor shape, the labels, the refusals, the command rows —
  // lives with its own control.
  //
  // `repaint` for the marks is the page WINDOW, not the document: turning marks on
  // is a repaint and never a repagination, so re-rastering what is on screen is the
  // whole of the chrome's duty and it is O(window) rather than O(document).
  const formattingMarks = createFormattingMarks({
    getDoc: () => ed.doc,
    repaint: () => {
      for (let i = ed.pageWindow.first; i <= ed.pageWindow.last; i++) ed.repaintPage(i);
    },
    setStatus: ed.setStatus,
  });
  const measurement = createMeasurementUnits({
    engine: {
      measurementUnits,
      defaultMeasurementUnit,
      decimalSeparatorForLanguage,
      parseMeasurement,
      formatMeasurement,
    },
    select: document.getElementById("measurementUnitSelect"),
    locale: () => activeLocale(),
    // The browser's tag, because it carries a REGION: the first-run default is
    // inches only for the US and Canada, and the UI locale is a language choice
    // that is usually just "en".
    region: () => navigator.language || activeLocale(),
    onChanged: () => ed.pageSetup.reflectUnits(),
    setStatus: ed.setStatus,
    openChooser: () => {
      revealOnSettings(ed.SETTINGS_SURFACE, document.getElementById("measurementUnitSelect"), {
        group: ".settings-section",
      });
    },
  });
  /** "Who may do what", in one handle — `access_chrome.mjs`, which composes the
   *  rights dialog and the persistent access indicator over the same four
   *  authorities. One call here rather than two modules wired separately, because a
   *  permission resolved at two call sites is a permission two call sites can
   *  disagree about. Constructed after `documentProtection` because it reads that
   *  module's state, which is the DOCUMENT's own authority. */
  const accessChrome = createAccessChrome({
    getDoc: () => ed.doc,
    getTransport: () => ed.collab,
    getGrant: () => ed.SESSION.grant,
    getProtection: () => documentProtection.state(),
    getCapabilities: () => ed.HOST_CAPS,
    getReadOnlyReason: () => ed.readOnlyReason,
    registerModal,
    fallbackFocus: () => ed.pagesEl,
    setStatus: ed.setStatus,
    t: (key, values) => t(key, values),
  });
  const documentProtection = createDocumentProtection({
    getDoc: () => ed.doc,
    runEdit: ed.runEdit,
    registerModal,
    fallbackFocus: () => ed.pagesEl,
    bindRadioGroup,
    setStatus: ed.setStatus,
    participantRefusal: () => ed.SESSION.refusalFor("review.restrictEditing"),
    onChanged: () => ed.updateToolbar(),
  });
  // Review ▸ Compare's seams (ADR-061, ADR-065), inline because this module has no
  // mount seam (`109` HF-085); `landed` is also the capability test for "Keep".
  const yieldFrame = () => new Promise((resolve) => requestAnimationFrame(() => resolve()));
  const redlineOf = (io) => buildRedline({ ...io, engine: { begin: beginVersionDiff, slice: defaultDiffSlice, open }, yieldToHost: yieldFrame });
  /** Takes the reader to one change on a redline (ADR-065): the selection and the scroll, focus left on the control they stepped with. */
  const showChange = (a) => { ed.selection = { anchor: { node: a.node, offset: Number(a.start) || 0 }, focus: { node: a.node, offset: Number(a.end) || Number(a.start) || 0 } }; ed.drawSelection(); if (ed.scrollModelRectIntoView(ed.selectionModelRect(), "center")) ed.paintOverlayLayer(); };
  const comparePanel = bindComparePanel({ doc: () => ed.doc, currentBytes: () => comparableBytes(ed.doc, ed.currentSourceFormat), redline: redlineOf, showView: (view) => (view ? ed.showVersionPreview(view, { markup: true, reason: t("compare.viewReadOnly") }) : ed.showVersionPreview(null)), beforeView: () => ed.versionHistory.closePreview(), yieldToHost: yieldFrame, setStatus: (text, kind) => ed.setStatus(text, kind), allowed: () => ed.HOST_CAPS.has("open"), refusedReason: t("capability.notGranted"), blockedReason: () => (ed.blockMutationInViewing() ? mutationBlockedMessage({ editingUnavailableReason: ed.readOnlyReason }) : ""), readOnlyReason: () => ed.readOnlyReason, landed: async (res) => { await ed.applyEditResult(res); await ed.setShowingChanges(true); ed.scheduleReviewMarginRender(); }, navigate: showChange });

  // The band's structural controls, declared in `table_band.mjs` (`109` UX-005).
  // Its Select handler used to be a second copy of `selectTableContext`.
  // Word count (HF-051): the dialog and the status bar's "x of y words".
  const wordCount = createWordCount({
    getDoc: () => ed.doc,
    getSelection: () => ed.selection,
    fallbackFocus: () => ed.pagesEl,
    setStatus: (text, kind) => ed.setStatus(text, kind),
  });
  return {
    accessChrome, comparePanel, documentProtection, formattingMarks, measurement, redlineOf,
    showChange, wordCount,
  };
}
