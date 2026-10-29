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
//     it is explicit because the recognition models are downloaded, and a
//     download the reader did not agree to, sized or not, is not ours to make.
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
// The models: the host names a manifest (`?pdfOcrModels=`, `ocr_models.mjs`);
// the editor bundles none and fetches none before the reader agrees to a
// dialog naming the download's size and origin. Verified files are kept in
// IndexedDB, so a second use, or a use offline, asks nothing and downloads
// nothing. The loaded engine is kept for the session and reused by every
// later recognition and by Convert to flowing document.
import { t, n } from "./i18n.mjs";
import { hostParams } from "./capabilities.mjs";
import {
  fetchOcrManifest,
  modelsStored,
  obtainModels,
  ocrManifestUrl,
  openOcrStore,
  sha256Hex,
} from "./ocr_models.mjs";

export const CONVERT_COMMAND = "file.pdf.convertToFlowing";
export const RECOGNISE_COMMAND = "file.pdf.recogniseText";
export const KEEP_LAYOUT_COMMAND = "file.pdf.keepExactLayout";

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
 * @param {() => string} [io.search] the page's query (the host's options).
 * @param {object} [io.view] `window`: its `fetch`, `indexedDB`, `crypto` and `location`.
 */
export function createPdfCommands(ed, io) {
  const view = io.view ?? globalThis;
  const search = io.search ?? (() => view.location?.search ?? "");
  /** The loaded engine and the manifest it came from, kept for the session. */
  let loaded = null;

  const state = () => ({
    pdfOrigin: Boolean(ed.doc?.pdfOrigin),
    pdfLayout: String(ed.doc?.pdfLayout ?? ""),
    ocr: ocrSummaryOf(ed.doc),
    manifestUrl: ocrManifestUrl(search()),
  });

  /** The engine, from this session's cache, or obtained behind the import
   *  dialog: the store first, then the download the reader agreed to. */
  function enginePreparer(url, manifest, store) {
    return async (module, ui) => {
      if (loaded?.url === url) return loaded.engine;
      ui.phase(t("pdfOcr.downloading"));
      ui.progress(0, manifest.bytes);
      const files = await obtainModels(manifest, {
        store,
        fetch: (target, init) => view.fetch(target, init),
        digest: (bytes) => sha256Hex(bytes, view.crypto?.subtle),
        onProgress: ui.progress,
        signal: ui.signal,
      });
      ui.signal.throwIfAborted?.();
      ui.phase(t("pdfOcr.loading"));
      await ui.painted();
      const models = new module.PdfOcrModels(files.detection);
      for (const pack of files.packs) models.addPack(pack.name, pack.model, pack.dictionary);
      const engine = module.PdfOcrEngine.load(models);
      loaded = { url, engine };
      return engine;
    };
  }

  /** Re-imports the session's PDF with text recognition and puts the result
   *  on screen in its place. */
  async function recogniseText() {
    const refusal = recogniseRefusal(state());
    if (refusal) {
      ed.setStatus(t(refusal), "error");
      return;
    }
    const original = ed.doc.pdfOriginal();
    const name = ed.currentName;
    if (!(await ed.confirmDiscardIfEdited())) return;
    const url = ocrManifestUrl(search());
    let manifest;
    let store = null;
    try {
      manifest = await fetchOcrManifest(url, { fetch: (target, init) => view.fetch(target, init) });
      store = await openOcrStore({ indexedDB: view.indexedDB }).catch(() => null);
    } catch (error) {
      ed.setStatus(t("pdfOcr.failed", { reason: String(error?.message ?? error) }), "error");
      return;
    }
    if (loaded?.url !== url && !(store && (await modelsStored(manifest, store).catch(() => false)))) {
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
      prepare: () => io.pdfImport.run(original, name, { prepareOcr: enginePreparer(url, manifest, store), layout }),
    });
  }

  /** Opens the session's PDF again with every page as it is drawn. */
  async function keepExactLayout() {
    const refusal = keepLayoutRefusal(state());
    if (refusal) {
      ed.setStatus(t(refusal), "error");
      return;
    }
    const original = ed.doc.pdfOriginal();
    const name = ed.currentName;
    const engine = ocrSummaryOf(ed.doc)?.provider && loaded ? loaded.engine : null;
    if (!(await ed.confirmDiscardIfEdited())) return;
    await io.openBytes(original, name, undefined, undefined, {
      origin: "pdf",
      cancelledStatus: t("pdfFixed.cancelled", { name }),
      failedStatus: (reason) => t("pdfFixed.failed", { name, reason }),
      prepare: () => io.pdfImport.run(original, name, { mode: "fixed", ocr: engine }),
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
    // A session whose scans were read is converted with the same engine, so
    // the flowing document has the text the reader already saw.
    const engine = ocrSummaryOf(ed.doc)?.provider && loaded ? loaded.engine : null;
    if (!(await ed.confirmDiscardIfEdited())) return;
    await io.openBytes(original, flowingName(name), undefined, undefined, {
      origin: "pdfReflow",
      sourceFormat: DOCX,
      cancelledStatus: t("pdfReflow.cancelled", { name }),
      failedStatus: (reason) => t("pdfReflow.failed", { name, reason }),
      prepare: () => io.pdfImport.run(original, name, { mode: "reflow", ocr: engine }),
    });
  }

  return {
    recogniseText,
    convertToFlowing,
    keepExactLayout,
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
