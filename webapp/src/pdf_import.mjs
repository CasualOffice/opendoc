// Opening a PDF: the progress dialog over the engine's staged import
// (`docs/182` §6, P2).
//
// Word converts a PDF behind a modal "Word will now convert your PDF" notice
// and a progress bar; Acrobat opens the file and then rebuilds each page's
// text as you edit it. Both keep the user out of the document while the
// conversion is half done, and so does this: the dialog is modal, because the
// document on screen is about to be replaced and an edit made to it now would
// be discarded the moment the import finishes. What neither offers, and this
// does, is Cancel at any page, with the open document left exactly as it was:
// nothing reaches the editor until the engine's atomic `finish` succeeds.
//
// The work is the PDF module's; the slicing is `pdf_import_scheduler.mjs`'s.
// This module is the dialog and the sequence:
//
//   open the dialog → let it paint → load the PDF module the first time
//   (`pdf_module.mjs`, its own bar in megabytes, Cancel aborts the fetch) →
//   predefined CMaps the file names, from the host's source → `beginPdfImport`
//   (admission, one task) → slices of pages → "Laying out" → the atomic finish
//   (the PDF module's package) → the main module's `openPdfImport` → close, and
//   hand the new document to `openBytes`.
//
// The PDF module is a second WebAssembly module (`docs/182` §17a), so the
// editor's own download carries no PDF reader. Nothing about the open document
// changes while it loads; Cancel during the fetch is the same promise as
// Cancel at any page.
//
// The same dialog and sequence serve the two commands that import a PDF
// again (`pdf_commands.mjs`): "Convert to flowing document" (`mode:
// "reflow"`, finished by the job's `finishReflowed` and opened by the main
// module's `openPdfReflow` as a NEW document) and "Recognise text in scanned
// pages" (`prepareOcr`: after the module loads, the recognition models are
// obtained and loaded behind the same bar and the same Cancel, then the
// import runs with them).
//
// Escape, the backdrop and the close button all cancel, through the one modal
// contract (`modal.mjs`): a dialog whose only safe dismissal is Cancel says so
// by making every dismissal Cancel. The words are written at OPEN, never at
// boot, because the catalogue lands later (SKILL §11, `chrome-raw-keys`).
//
// The bar is a `role="progressbar"` with its value and a value text ("Page 3
// of 10") a screen reader reads on demand; the polite live region announces
// only the phases (reading, laying out, cancelling), so a reader is not handed
// a number per slice.
import { openPdfImport, openPdfReflow } from "../pkg/casual_doc_wasm.js";
import { loadPdfModule, pdfModuleLoaded } from "./pdf_module.mjs";
import { registerModal } from "./modal.mjs";
import { n, t } from "./i18n.mjs";
import { idleSlices, runStagedImport } from "./pdf_import_scheduler.mjs";

const ID = "pdfImportDialog";

/** Marks the end of the sliced phase, for the main-thread guard: tasks before
 *  it are the import's slices, the task after it is the finish and layout. */
export const FINISH_MARK = "opendoc:pdf-import:finish";

function buildDialog(view) {
  const make = (tag, props = {}) => Object.assign(view.createElement(tag), props);
  const overlay = make("div", { id: ID, className: "dialog-overlay", hidden: true });
  overlay.setAttribute("role", "dialog");
  overlay.setAttribute("aria-modal", "true");
  overlay.setAttribute("aria-labelledby", `${ID}Title`);
  overlay.setAttribute("aria-describedby", `${ID}Description`);
  const card = make("section", { className: "dialog-card pdf-import-dialog" });
  const head = make("header", { className: "dialog-head" });
  const heading = make("div", { className: "dialog-heading" });
  const words = make("div");
  words.append(make("h2", { id: `${ID}Title` }), make("p", { id: `${ID}Description` }));
  heading.append(words);
  const close = make("button", { id: `${ID}Close`, type: "button", className: "dialog-close" });
  close.append(make("span", { className: "ms", textContent: "close" }));
  close.firstChild.setAttribute("aria-hidden", "true");
  head.append(heading, close);
  const body = make("div", { className: "dialog-body pdf-import-body" });
  const bar = make("div", { id: `${ID}Bar`, className: "pdf-import-bar" });
  bar.setAttribute("role", "progressbar");
  bar.setAttribute("aria-labelledby", `${ID}Title`);
  bar.setAttribute("aria-valuemin", "0");
  const fill = make("div", { className: "pdf-import-fill" });
  bar.append(fill);
  const count = make("p", { id: `${ID}Count`, className: "pdf-import-count" });
  const phase = make("p", { id: `${ID}Phase`, className: "pdf-import-phase" });
  phase.setAttribute("aria-live", "polite");
  body.append(bar, count, phase);
  const foot = make("footer", { className: "dialog-foot" });
  const actions = make("div", { className: "dialog-actions" });
  const cancel = make("button", { id: `${ID}Cancel`, type: "button", className: "dialog-button" });
  actions.append(cancel);
  foot.append(actions);
  card.append(head, body, foot);
  overlay.append(card);
  view.body.append(overlay);
  return { overlay, close, bar, fill, count, phase, cancel, title: words.firstChild, description: words.lastChild };
}

/** Megabytes, one decimal, for the module bar. */
const mb = (bytes) => n(Math.round(bytes / 100_000) / 10);

/** The words of each mode: the title, the intro and Cancel's name. */
const MODES = Object.freeze({
  import: Object.freeze({ title: "pdfImport.title", intro: "pdfImport.intro", cancelLabel: "pdfImport.cancelLabel" }),
  reflow: Object.freeze({ title: "pdfReflow.title", intro: "pdfReflow.intro", cancelLabel: "pdfReflow.cancelLabel" }),
  ocr: Object.freeze({ title: "pdfOcr.title", intro: "pdfOcr.intro", cancelLabel: "pdfOcr.cancelLabel" }),
});

/**
 * Fetches the predefined CMaps `names` from the host's source (`pdfCMaps`, a
 * base URL; each file is `<base>/<name>`) into the PDF module. Without a
 * source nothing is fetched and the page that needs one is refused, naming it
 * (`casual-doc-pdf-read` `provision`).
 */
async function provideCMaps(module, names, source, signal) {
  if (!source || names.length === 0) return false;
  for (const name of names) {
    const response = await fetch(new URL(encodeURIComponent(name), source.endsWith("/") ? source : `${source}/`), { signal });
    if (!response.ok) continue;
    module.provideCMap(name, new Uint8Array(await response.arrayBuffer()));
  }
  return true;
}

/**
 * @param {object} [io]
 * @param {Document} [io.view]
 * @param {object} [io.scope] `window`: its idle callbacks, frames and clock.
 * @param {() => HTMLElement} [io.fallbackFocus] where focus goes if the opener is gone.
 * @param {(io: object) => Promise<object>} [io.loadModule] `pdf_module.mjs`'s loader.
 * @param {(pkg: Uint8Array, bytes: Uint8Array) => object} [io.open] the main
 *   module's `openPdfImport`.
 * @param {(pkg: Uint8Array) => object} [io.openReflow] the main module's
 *   `openPdfReflow`.
 * @param {() => string|null} [io.cmapSource] the host's CMap base URL, or null.
 */
export function createPdfImport(io = {}) {
  const view = io.view ?? globalThis.document;
  const scope = io.scope ?? globalThis;
  const loadModule = io.loadModule ?? loadPdfModule;
  const openImport = io.open ?? openPdfImport;
  const openReflow = io.openReflow ?? openPdfReflow;
  const cmapSource = io.cmapSource ?? (() => null);
  const parts = buildDialog(view);
  /** The import in flight, or null. One at a time: the dialog is modal. */
  let active = null;
  const modal = registerModal(parts.overlay, {
    initialFocus: () => parts.cancel,
    fallbackFocus: io.fallbackFocus,
    // Every dismissal but our own close is Cancel.
    onClose: (reason) => {
      if (reason !== "done" && active) requestCancel();
    },
  });

  function requestCancel() {
    if (!active || active.cancelRequested) return;
    active.cancelRequested = true;
    active.abort.abort();
    parts.cancel.disabled = true;
    parts.phase.textContent = t("pdfImport.cancelling");
  }

  function paintProgress(done, total) {
    if (total > 0) {
      parts.bar.setAttribute("aria-valuemax", String(total));
      parts.bar.setAttribute("aria-valuenow", String(done));
      const text = t("pdfImport.progress", { done: n(done), total: n(total) });
      parts.bar.setAttribute("aria-valuetext", text);
      parts.count.textContent = text;
      parts.fill.style.inlineSize = `${Math.round((done / total) * 100)}%`;
      parts.bar.dataset.state = "determinate";
    } else {
      parts.bar.removeAttribute("aria-valuenow");
      parts.bar.removeAttribute("aria-valuetext");
      parts.count.textContent = "";
      parts.bar.dataset.state = "indeterminate";
    }
  }

  function renderOpen(name, mode) {
    const words = MODES[mode] ?? MODES.import;
    parts.overlay.dataset.mode = mode;
    parts.title.textContent = t(words.title, { name });
    parts.description.textContent = t(words.intro);
    parts.close.setAttribute("aria-label", t(words.cancelLabel));
    parts.close.title = t("pdfImport.cancel");
    parts.cancel.textContent = t("pdfImport.cancel");
    parts.cancel.disabled = false;
    parts.phase.textContent = t("pdfImport.reading");
    paintProgress(0, 0);
  }

  /** One frame and one task, so what was just written is on screen before the
   *  next synchronous engine call holds the thread. */
  const painted = () =>
    new Promise((resolve) => {
      if (typeof scope.requestAnimationFrame === "function") {
        scope.requestAnimationFrame(() => scope.setTimeout(resolve, 0));
      } else scope.setTimeout(resolve, 0);
    });

  /** The module bar: megabytes received of the total, or indeterminate. */
  function paintModule(received, total) {
    if (total > 0) {
      parts.bar.setAttribute("aria-valuemax", String(total));
      parts.bar.setAttribute("aria-valuenow", String(Math.min(received, total)));
      const text = t("pdfImport.moduleProgress", { done: mb(received), total: mb(total) });
      parts.bar.setAttribute("aria-valuetext", text);
      parts.count.textContent = text;
      parts.fill.style.inlineSize = `${Math.round((Math.min(received, total) / total) * 100)}%`;
      parts.bar.dataset.state = "determinate";
    } else paintProgress(0, 0);
  }

  /** Loads the PDF module behind the open dialog, the first time only. */
  async function ensureModule() {
    if (pdfModuleLoaded() && !io.loadModule) return loadModule();
    parts.phase.textContent = t("pdfImport.loadingModule");
    paintProgress(0, 0);
    parts.bar.dataset.phase = "module";
    try {
      return await loadModule({ onProgress: paintModule, signal: active.abort.signal });
    } finally {
      delete parts.bar.dataset.phase;
    }
  }

  /** One staged import; `null` when the reader cancelled. */
  async function importOnce(module, bytes, engine, mode) {
    let job = null;
    try {
      job = engine ? module.beginPdfImportWithOcr(bytes, engine) : module.beginPdfImport(bytes);
      await provideCMaps(module, job.missingCMaps, cmapSource(), active.abort.signal);
      if (active.cancelRequested) return null;
      paintProgress(0, job.pageCount);
      parts.phase.textContent = t("pdfImport.reading");
      const clock = scope.performance ?? globalThis.performance;
      const result = await runStagedImport(job, {
        nextSlice: idleSlices(scope),
        now: () => clock.now(),
        cancelled: () => active.cancelRequested,
        onProgress: paintProgress,
      });
      if (result.status === "cancelled") return null;
      parts.phase.textContent = t("pdfImport.finishing");
      parts.cancel.disabled = true;
      await painted();
      if (active.cancelRequested) return null;
      clock.mark?.(FINISH_MARK);
      try {
        const pkg = mode === "reflow" ? job.finishReflowed() : job.finish();
        const ocr = job.ocrSummaryJson ? JSON.parse(job.ocrSummaryJson) : null;
        return { pkg, ocr };
      } catch (error) {
        // A page needed predefined CMaps nobody had: fetch them and import
        // once more, if the host has a source and they are new.
        const needed = job.neededCMaps ?? [];
        const err = new Error(String(error?.message ?? error));
        err.neededCMaps = needed;
        throw err;
      }
    } finally {
      job?.free();
    }
  }

  /**
   * Imports `bytes` behind the dialog.
   *
   * @param {Uint8Array} bytes
   * @param {string} name
   * @param {object} [options]
   * @param {object} [options.ocr] a loaded `PdfOcrEngine`: recognise text in
   *   scanned pages.
   * @param {(module: object, ui: object) => Promise<object>} [options.prepareOcr]
   *   obtains and loads the engine after the module, behind this dialog: `ui`
   *   is `{ progress(received, total), phase(text), signal, painted() }`.
   * @param {"import"|"reflow"|"ocr"} [options.mode] what the dialog says and
   *   how the job finishes: `reflow` opens the converted document instead.
   * @returns {Promise<{doc: object, ocr: object|null} | {cancelled: true}>}
   *   the new document, not yet adopted by the editor, and what OCR did; or a
   *   cancellation, with nothing changed.
   * @throws the engine's refusal or a failed page, as an `Error`.
   */
  async function run(bytes, name, options = {}) {
    if (active) throw new Error(t("pdfImport.busy"));
    active = { cancelRequested: false, abort: new AbortController() };
    const mode = options.mode ?? (options.prepareOcr ? "ocr" : "import");
    renderOpen(name, mode);
    modal.open();
    try {
      await painted();
      if (active.cancelRequested) return { cancelled: true };
      let module;
      try {
        module = await ensureModule();
      } catch (error) {
        if (active.cancelRequested || error?.name === "AbortError") return { cancelled: true };
        throw new Error(t("pdfImport.moduleFailed", { reason: String(error?.message ?? error) }));
      }
      if (active.cancelRequested) return { cancelled: true };
      let engine = options.ocr ?? null;
      if (options.prepareOcr) {
        parts.bar.dataset.phase = "models";
        try {
          engine = await options.prepareOcr(module, {
            progress: paintModule,
            phase: (text) => {
              parts.phase.textContent = text;
            },
            signal: active.abort.signal,
            painted,
          });
        } catch (error) {
          if (active.cancelRequested || error?.name === "AbortError") return { cancelled: true };
          throw new Error(t("pdfOcr.failed", { reason: String(error?.message ?? error) }));
        } finally {
          delete parts.bar.dataset.phase;
        }
        if (active.cancelRequested) return { cancelled: true };
      }
      let imported;
      try {
        imported = await importOnce(module, bytes, engine, mode);
      } catch (error) {
        const source = cmapSource();
        if (!source || !error.neededCMaps?.length) throw error;
        await provideCMaps(module, error.neededCMaps, source, active.abort.signal);
        parts.cancel.disabled = false;
        imported = await importOnce(module, bytes, engine, mode);
      }
      if (!imported) return { cancelled: true };
      const doc = mode === "reflow" ? openReflow(imported.pkg) : openImport(imported.pkg, bytes);
      return { doc, ocr: imported.ocr, engine };
    } finally {
      active = null;
      if (modal.isOpen) modal.close("done");
    }
  }

  parts.cancel.addEventListener("click", requestCancel);
  parts.close.addEventListener("click", () => modal.close("cancel"));

  return {
    run,
    /** Whether an import is in flight (a second open waits for it). */
    get running() {
      return active !== null;
    },
  };
}
