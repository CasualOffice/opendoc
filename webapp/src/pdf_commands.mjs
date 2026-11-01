// The three commands a PDF-origin document offers (`docs/182` §4, §17a, §18,
// §20):
//
//   * "Keep exact page layout" (`file.pdf.keepExactLayout`). A PDF opens as a
//     flowing document, as Word's PDF Reflow and Google Docs open one
//     (`docs/182` §4.2); this opens the same PDF again with every page as it
//     is drawn, in positioned frames (`docs/182` §4.1), the only layout whose
//     save goes back into the original. Offered on a flowing PDF session.
//   * "Convert to flowing document" (`file.pdf.convertToFlowing`): the other
//     direction, from a session opened with its exact layout. It builds a NEW
//     document from the PDF as it was opened (`PdfImport::reflow`), never an
//     edit of the open one, and that document keeps no PDF.
//   * "Recognise text in scanned pages" (`file.pdf.recogniseText`). Acrobat
//     calls it "Recognize Text", Google Drive does it silently on upload; here
//     it runs on its own when a scanned PDF opens, as Drive's does, and by
//     hand from this command, which asks before a download.
//
// Both import the PDF again in the PDF module, behind the same progress dialog
// and Cancel as File ▸ Open (`pdf_import.mjs`), from the original the session
// kept (`WasmDocument.pdfOriginal()`), and both replace what is on screen the
// way Open does — through `confirmDiscardIfEdited` first.
//
// Surfaces (SKILL §10, at least two): the command registry (palette, the File
// menu's PDF band, the File page) and the PDF summary dialog's actions
// (`compat_findings.mjs`); the first open also offers "Keep exact page
// layout" in its own progress dialog (`pdf_import.mjs`). No command is ever
// removed: for a document that is not a PDF, a PDF already in that layout, or
// a PDF with nothing to recognise, it is DISABLED WITH THE REASON.
//
// The models: the host names a manifest (`?pdfOcrModels=`, `ocr_models.mjs`),
// or the deployment ships one. A scanned PDF is recognised on its own right
// after it opens, downloading the models without a prompt (owner, 2026-10-30);
// asked for by hand, the command confirms first, naming the size and origin,
// unless the models are already loaded or kept. Verified files are kept in
// IndexedDB, so a later use, or one offline, downloads nothing.
//
// Everything about recognition runs in the OCR worker (`ocr_service.mjs`,
// `docs/182` §17a.5): the download, the check, the cache, the engine and every
// page. The engine is kept there for the session and reused by every later
// recognition, Keep exact page layout and Convert to flowing document; the
// first scanned page the open reads starts it loading (`preload`).
import { t, n } from "./i18n.mjs";
import { hostParams } from "./capabilities.mjs";
import { fetchOcrManifest, ocrManifestUrl } from "./ocr_models.mjs";

export const CONVERT_COMMAND = "file.pdf.convertToFlowing";
export const RECOGNISE_COMMAND = "file.pdf.recogniseText";
export const KEEP_LAYOUT_COMMAND = "file.pdf.keepExactLayout";
/** View ▸ Page display ▸ PDF layout, the palette and the status-bar chip
 *  (`pdf_layout_chip.mjs`): the SWITCH between the two layouts a PDF opens in,
 *  as a pair of radio rows (owner, 2026-11-01, `docs/109` UX-051). It is
 *  symmetric by owner decision: either direction reopens the ORIGINAL PDF the
 *  session kept, so the document stays a PDF session and can be switched back.
 *  "Convert to flowing document" stays what it was — a new document that keeps
 *  no PDF — and is a different question ("make this a Word document"). */
export const LAYOUT_FLOWING_COMMAND = "view.pdfLayout.flowing";
export const LAYOUT_EXACT_COMMAND = "view.pdfLayout.exact";

/** The layout a document is in, as the switch names it: `"flowing"`,
 *  `"exact"`, or `""` for a document that did not come from a PDF. Pure, O(1). */
export function pdfLayoutOf(doc) {
  if (!doc?.pdfOrigin) return "";
  return String(doc.pdfLayout ?? "") === "fixed" ? "exact" : "flowing";
}

/** Why the switch cannot move to `target` (`"flowing"` | `"exact"`) now, as a
 *  catalogue key, or `""` when it can. Pure. */
export function layoutSwitchRefusal(doc, target) {
  const layout = pdfLayoutOf(doc);
  if (!layout) return "pdfLayout.notPdf";
  if (layout === target) return target === "exact" ? "pdfFixed.already" : "pdfReflow.alreadyFlowing";
  return "";
}

/** The layout the host opens PDFs in (`?pdfLayout=`, matched in any case
 *  like every host option, `capabilities.mjs`'s `hostParams`): `"fixed"`
 *  when it asks to keep the exact page layout, else `"flowing"`. Pure. */
export function hostPdfLayout(search) {
  try {
    return String(hostParams(search ?? "").get("pdflayout") ?? "").toLowerCase() === "fixed" ? "fixed" : "flowing";
  } catch {
    return "flowing";
  }
}

/** The DOCX format id a converted document saves as by default. */
const DOCX = "org.openxmlformats.wordprocessingml.document";

/** What OCR did when this session's PDF was imported, or null. */
export function ocrSummaryOf(doc) {
  try {
    const json = doc?.pdfOcrSummaryJson;
    return json ? JSON.parse(json) : null;
  } catch {
    return null;
  }
}

/**
 * Why "Recognise text in scanned pages" cannot run now, as a catalogue key, or
 * "" when it can. Pure, so the rule is tested without an engine.
 *
 * @param {{pdfOrigin: boolean, ocr: object|null, manifestUrl: string|null}} state
 */
export function recogniseRefusal({ pdfOrigin, ocr, manifestUrl }) {
  if (!pdfOrigin) return "pdfOcr.notPdf";
  const scanned = (ocr?.rasterPages ?? 0) + (ocr?.hybridPages ?? 0);
  if (scanned === 0) return "pdfOcr.noScans";
  if (ocr?.provider) return "pdfOcr.done";
  if (!manifestUrl) return "pdfOcr.noModels";
  return "";
}

/** Why "Convert to flowing document" cannot run now, or "": it converts a
 *  PDF opened with its exact layout; a flowing PDF already is one. */
export function convertRefusal({ pdfOrigin, pdfLayout }) {
  if (!pdfOrigin) return "pdfReflow.notPdf";
  return pdfLayout === "flowing" ? "pdfReflow.alreadyFlowing" : "";
}

/** Why "Keep exact page layout" cannot run now, or "": it reopens a flowing
 *  PDF session with every page as drawn. */
export function keepLayoutRefusal({ pdfOrigin, pdfLayout }) {
  if (!pdfOrigin) return "pdfFixed.notPdf";
  return pdfLayout === "fixed" ? "pdfFixed.already" : "";
}

/** `report.pdf` → `report (flowing).docx`. */
export function flowingName(name) {
  const stem = String(name ?? "").replace(/\.pdf$/i, "") || "document";
  return t("pdfReflow.fileName", { name: stem });
}

/**
 * @param {object} ed the editor context (`docs/177`).
 * @param {object} io
 * @param {object} io.pdfImport `createPdfImport`'s controller.
 * @param {Function} io.openBytes `editor_file_open.mjs`'s open, with `options.prepare`.
 * @param {object} io.ocrService `createOcrService`'s client: the OCR worker.
 * @param {() => string} [io.search] the page's query (the host's options).
 * @param {object} [io.view] `window`: its `fetch` (the manifest the consent
 *   names) and `location`.
 */
export function createPdfCommands(ed, io) {
  const view = io.view ?? globalThis;
  const search = io.search ?? (() => view.location?.search ?? "");
  /** The OCR worker's client (`ocr_service.mjs`): it owns the models, the
   *  engine and every page's recognition, kept for the session. */
  const ocr = io.ocrService;

  const state = () => ({
    pdfOrigin: Boolean(ed.doc?.pdfOrigin),
    pdfLayout: String(ed.doc?.pdfLayout ?? ""),
    ocr: ocrSummaryOf(ed.doc),
    manifestUrl: ocrManifestUrl(search()),
  });

  /** What a re-import with recognition hands the dialog. */
  const recognition = (manifestUrl) => ({ service: ocr, manifestUrl });

  /** Re-imports the session's PDF with text recognition and puts the result
   *  on screen in its place. */
  async function recogniseText({ auto = false } = {}) {
    const refusal = recogniseRefusal(state());
    if (refusal) {
      ed.setStatus(t(refusal), "error");
      return;
    }
    const original = ed.doc.pdfOriginal();
    const name = ed.currentName;
    if (!auto && !(await ed.confirmDiscardIfEdited())) return;
    const url = ocrManifestUrl(search());
    // Run on its own right after a scanned PDF opens, the models download when
    // they are needed, without a prompt (owner, 2026-10-30): the import dialog
    // shows the progress and carries Cancel. Asked for by hand, it confirms
    // first unless the models are already loaded or kept in this browser.
    if (!auto && !ocr.isWarm(url) && !(await ocr.stored(url).catch(() => false))) {
      let manifest;
      try {
        manifest = await fetchOcrManifest(url, { fetch: (target, init) => view.fetch(target, init) });
      } catch (error) {
        ed.setStatus(t("pdfOcr.failed", { reason: String(error?.message ?? error) }), "error");
        return;
      }
      const agreed = await ed.confirmModal({
        title: t("pdfOcr.consentTitle"),
        message: t("pdfOcr.consentMessage", {
          size: n(Math.max(0.1, Math.round(manifest.bytes / 100_000) / 10)),
          host: new URL(manifest.detection.url).host,
        }),
        confirmLabel: t("pdfOcr.consentConfirm"),
        cancelLabel: t("pdfOcr.consentCancel"),
        icon: "download",
      });
      if (!agreed) return;
    }
    // Read again in the layout the session has: a flowing PDF stays flowing.
    const layout = state().pdfLayout === "fixed" ? "fixed" : "flowing";
    await io.openBytes(original, name, undefined, undefined, {
      origin: "pdf",
      cancelledStatus: t("pdfOcr.cancelled"),
      prepare: () => io.pdfImport.run(original, name, { recognition: recognition(url), layout }),
    });
  }

  /** A session whose scans were read is re-imported with recognition again,
   *  so the new document has the text the reader already saw. */
  const recognitionIfRead = () => {
    const url = ocrManifestUrl(search());
    return ocrSummaryOf(ed.doc)?.provider && url ? recognition(url) : undefined;
  };

  /** Opens the session's PDF again with every page as it is drawn. */
  async function keepExactLayout() {
    const refusal = keepLayoutRefusal(state());
    if (refusal) {
      ed.setStatus(t(refusal), "error");
      return;
    }
    const original = ed.doc.pdfOriginal();
    const name = ed.currentName;
    const read = recognitionIfRead();
    if (!(await ed.confirmDiscardIfEdited())) return;
    await io.openBytes(original, name, undefined, undefined, {
      origin: "pdf",
      cancelledStatus: t("pdfFixed.cancelled", { name }),
      failedStatus: (reason) => t("pdfFixed.failed", { name, reason }),
      prepare: () => io.pdfImport.run(original, name, { mode: "fixed", recognition: read }),
    });
  }

  /** Reopens the session's PDF FLOWING, as File ▸ Open opens one, keeping the
   *  original — the switch's way back from the exact layout, so the reader can
   *  switch again (owner, 2026-11-01). The same import, dialog and Cancel as
   *  Open; a session whose scans were read is read again. */
  async function reopenFlowing() {
    const refusal = layoutSwitchRefusal(ed.doc, "flowing");
    if (refusal) {
      ed.setStatus(t(refusal), "error");
      return;
    }
    const original = ed.doc.pdfOriginal();
    const name = ed.currentName;
    const read = recognitionIfRead();
    if (!(await ed.confirmDiscardIfEdited())) return;
    await io.openBytes(original, name, undefined, undefined, {
      origin: "pdf",
      cancelledStatus: t("pdfFixed.cancelled", { name }),
      failedStatus: (reason) => t("pdfFixed.failed", { name, reason }),
      prepare: () => io.pdfImport.run(original, name, { layout: "flowing", recognition: read }),
    });
  }

  /** Builds the flowing document from the session's PDF and opens it as a new
   *  document. */
  async function convertToFlowing() {
    const refusal = convertRefusal(state());
    if (refusal) {
      ed.setStatus(t(refusal), "error");
      return;
    }
    const original = ed.doc.pdfOriginal();
    const name = ed.currentName;
    const read = recognitionIfRead();
    if (!(await ed.confirmDiscardIfEdited())) return;
    await io.openBytes(original, flowingName(name), undefined, undefined, {
      origin: "pdfReflow",
      sourceFormat: DOCX,
      cancelledStatus: t("pdfReflow.cancelled", { name }),
      failedStatus: (reason) => t("pdfReflow.failed", { name, reason }),
      prepare: () => io.pdfImport.run(original, name, { mode: "reflow", recognition: read }),
    });
  }

  return {
    recogniseText,
    /** Recognises a just-opened PDF's scanned pages on its own when models are
     *  served; a no-op otherwise. Once OCR has run, the summary names its
     *  provider and the refusal is "done", so this cannot loop. */
    autoRecognise() {
      if (this.willAutoRecognise()) void recogniseText({ auto: true });
    },
    /** Whether `autoRecognise` will recognise the open document: a PDF with
     *  scanned pages, not yet recognised, with models served. The open that
     *  starts it holds its summary back for the recognised document's, so
     *  the summary appears once. Pure over the session's state. */
    willAutoRecognise() {
      return recogniseRefusal(state()) === "";
    },
    /** The PDF import read its first scanned page: start the models loading
     *  in the worker now, in parallel with the rest of the import, so the
     *  recognition `autoRecognise` starts next begins warm (`docs/182`
     *  §17a.5). From IndexedDB when this browser kept them, else downloaded,
     *  under the same rule as the automatic run (owner, 2026-10-30). Never
     *  called for a document that is not a PDF or a PDF without scans; a
     *  no-op without a host's or the deployment's models. */
    preload() {
      const url = ocrManifestUrl(search());
      if (url) ocr.warm(url).catch(() => {});
    },
    convertToFlowing,
    keepExactLayout,
    reopenFlowing,
    /** The layout switch's rows: View ▸ Page display ▸ PDF layout, the palette
     *  and the status-bar chip. Radio rows, the current layout checked. Both
     *  are enabled for a PDF session, the checked one included, so a reader
     *  can land on it and hear that it is checked; running it says "already
     *  open as …" in the status line. Disabled with the reason for any other
     *  document (SKILL §10). O(1). */
    layoutCommands() {
      const layout = pdfLayoutOf(ed.doc);
      const reason = t("pdfLayout.notPdf");
      return [
        {
          id: LAYOUT_FLOWING_COMMAND,
          label: t("pdfLayout.flowingCommand"),
          group: "View",
          kw: "pdf layout flowing reflow paragraphs editable switch view",
          enabled: layout !== "",
          disabledReason: reason,
          checked: layout === "flowing",
          radio: true,
          run: () => void reopenFlowing(),
        },
        {
          id: LAYOUT_EXACT_COMMAND,
          label: t("pdfLayout.exactCommand"),
          group: "View",
          kw: "pdf layout exact fixed page as drawn original frames switch view",
          enabled: layout !== "",
          disabledReason: reason,
          checked: layout === "exact",
          radio: true,
          run: () => void keepExactLayout(),
        },
      ];
    },
    /** The summary dialog's actions: the same two commands, as buttons. */
    summaryActions() {
      return this.commands().map((command) => ({
        id: command.id,
        label: command.label,
        enabled: command.enabled,
        reason: command.disabledReason,
        run: command.run,
      }));
    },
    /** The registry rows: palette, File menu, File page. */
    commands() {
      const now = state();
      const ocrRefusal = recogniseRefusal(now);
      const reflowRefusal = convertRefusal(now);
      const keepRefusal = keepLayoutRefusal(now);
      return [
        {
          id: KEEP_LAYOUT_COMMAND,
          label: t("pdfFixed.command"),
          group: "File",
          kw: "keep exact page layout fixed frames original pages pdf as drawn positioned",
          enabled: !keepRefusal,
          disabledReason: t(keepRefusal || "pdfFixed.notPdf"),
          run: () => void keepExactLayout(),
        },
        {
          id: RECOGNISE_COMMAND,
          label: t("pdfOcr.command"),
          group: "File",
          kw: "ocr recognise recognize text scan scanned image picture pdf read",
          enabled: !ocrRefusal,
          disabledReason: t(ocrRefusal || "pdfOcr.notPdf"),
          run: () => void recogniseText(),
        },
        {
          id: CONVERT_COMMAND,
          label: t("pdfReflow.command"),
          group: "File",
          kw: "convert reflow flowing pdf editable paragraphs headings word docx export",
          enabled: !reflowRefusal,
          disabledReason: t(reflowRefusal || "pdfReflow.notPdf"),
          run: () => void convertToFlowing(),
        },
      ];
    },
  };
}
