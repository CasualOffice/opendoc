// Opening a PDF: the progress dialog over the engine's staged import
// (`docs/182` §6, P2).
//
// A PDF opens as a FLOWING document (`docs/182` §4), the way Word's PDF
// Reflow and Google Docs open one: the page layout is evidence, and what
// opens is an ordinary document of paragraphs, headings, lists, tables,
// header and footer and footnotes, with no text boxes and no positioned
// frames (the PDF module's `finishReflowed`, opened by the main module's
// `openPdfReflow` with the original kept so the PDF can be opened again).
// "Keep exact page layout" is the explicit choice: a checkbox in this dialog
// (read when the pages are done, so it can be ticked while they are read),
// the host's `?pdfLayout=fixed`, and the `file.pdf.keepExactLayout` command
// (`pdf_commands.mjs`). Fixed layout is the job's `finish`, opened by
// `openPdfImport`: every page as drawn, the only layout whose save goes back
// into the original.
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
// pages" (`recognition`). An import WITH recognition never runs here: it runs
// in the OCR worker (`ocr_service.mjs`, `docs/182` §17a.5), models, engine,
// pages and finish, and this dialog only paints what the worker reports and
// terminates it on Cancel. A page's recognition is seconds inside one wasm
// call, which no slicing between pages can bound, so on this thread it froze
// the tab. Only the main module's open of the finished package runs here.
//
// PRELOAD. The import without recognition (the first open) reports the first
// scanned page it reads through `onScans`, once, so the models start loading
// in the worker while the rest of the PDF is read (`pdf_commands.mjs`).
//
// The document behind the dialog stays scrollable: the wheel over the
// backdrop scrolls it (`scrollTarget`), while focus stays in the dialog, so
// it can be read during a long run but not edited into a state the finished
// import would replace.
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
import { hostPdfLayout } from "./pdf_commands.mjs";
import { loadPdfModule, pdfModuleLoaded } from "./pdf_module.mjs";
import { registerModal } from "./modal.mjs";
import { n, t } from "./i18n.mjs";
import { idleSlices, runStagedImport } from "./pdf_import_scheduler.mjs";

const ID = "pdfImportDialog";

/** Marks the end of the reading phase, for the main-thread guards: tasks
 *  before it are the import's (slices on this thread, or nothing but progress
 *  when the worker reads), the task after it is the open and layout. */
export const FINISH_MARK = "opendoc:pdf-import:finish";

/** Bracket a recognition run, for the OCR main-thread guard
 *  (`ocr-main-thread.spec.mjs`): every task between them is this thread's
 *  while the worker recognises, and none may be a long task. */
export const OCR_START_MARK = "opendoc:pdf-ocr:start";
export const OCR_END_MARK = "opendoc:pdf-ocr:end";

function buildDialog(view) {
  const make = (tag, props = {}) => Object.assign(view.createElement(tag), props);
  const overlay = make("div", { id: ID, className: "dialog-overlay", hidden: true });
  overlay.setAttribute("role", "dialog");
  overlay.setAttribute("aria-modal", "true");
  overlay.setAttribute("aria-labelledby", `${ID}Title`);
  overlay.setAttribute("aria-describedby", `${ID}Description`);
  const card = make("section", { className: "dialog-card dialog-card-compact pdf-import-dialog" });
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
  // "Keep exact page layout": a native checkbox, so it is keyboard and
  // screen-reader operable as it is, described by its hint.
  const layout = make("div", { className: "pdf-import-layout" });
  const keep = make("input", { id: `${ID}KeepLayout`, type: "checkbox" });
  keep.setAttribute("aria-describedby", `${ID}KeepLayoutHint`);
  const keepLabel = make("label", { id: `${ID}KeepLayoutLabel` });
  keepLabel.htmlFor = keep.id;
  const keepHint = make("p", { id: `${ID}KeepLayoutHint`, className: "pdf-import-phase" });
  layout.append(keep, keepLabel, keepHint);
  body.append(bar, count, phase, layout);
  const foot = make("footer", { className: "dialog-foot" });
  const actions = make("div", { className: "dialog-actions" });
  const cancel = make("button", { id: `${ID}Cancel`, type: "button", className: "dialog-button" });
  actions.append(cancel);
  foot.append(actions);
  card.append(head, body, foot);
  overlay.append(card);
  view.body.append(overlay);
  return {
    overlay,
    close,
    bar,
    fill,
    count,
    phase,
    cancel,
    layout,
    keep,
    keepLabel,
    keepHint,
    title: words.firstChild,
    description: words.lastChild,
  };
}

/** Megabytes, one decimal, for the module bar. */
const mb = (bytes) => n(Math.round(bytes / 100_000) / 10);

/** The words of each mode: the title, the intro and Cancel's name. */
const MODES = Object.freeze({
  import: Object.freeze({ title: "pdfImport.title", intro: "pdfImport.introFlowing", cancelLabel: "pdfImport.cancelLabel" }),
  fixed: Object.freeze({ title: "pdfFixed.title", intro: "pdfImport.intro", cancelLabel: "pdfImport.cancelLabel" }),
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
 * @param {() => "flowing"|"fixed"} [io.layout] the host's default layout
 *   (`hostPdfLayout` of the page's query).
 * @param {() => object|null} [io.worker] the OCR service (`ocr_service.mjs`),
 *   whose worker also runs plain imports off this thread; null (or absent)
 *   keeps the sliced main-thread import, for a host without workers.
 * @param {() => void} [io.onScans] called once per import, the first time a
 *   page it read is a scan: the preload's trigger.
 * @param {() => HTMLElement|null} [io.scrollTarget] what the wheel over the
 *   dialog's backdrop scrolls (the editor's viewport).
 */
export function createPdfImport(io = {}) {
  const view = io.view ?? globalThis.document;
  const scope = io.scope ?? globalThis;
  const loadModule = io.loadModule ?? loadPdfModule;
  const openImport = io.open ?? openPdfImport;
  const openReflow = io.openReflow ?? openPdfReflow;
  const cmapSource = io.cmapSource ?? (() => null);
  const hostLayout = io.layout ?? (() => hostPdfLayout(scope.location?.search ?? ""));
  const parts = buildDialog(view);
  /** The import in flight, or null. One at a time: the dialog is modal. */
  let active = null;
  /** Whether the worker has its PDF module (it has reported a page). */
  let workerReady = false;
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
    say(t("pdfImport.cancelling"));
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

  function renderOpen(name, mode, layout) {
    const words = MODES[mode] ?? MODES.import;
    // The choice is offered where it is still open: a first open, and a
    // recognition run (which opens the PDF again in whichever layout).
    const choosing = mode === "import" || mode === "ocr";
    parts.layout.hidden = !choosing;
    parts.keep.checked = layout === "fixed";
    parts.keep.disabled = !choosing;
    parts.keepLabel.textContent = t("pdfImport.keepLayout");
    parts.keepHint.textContent = t("pdfImport.keepLayoutHint");
    parts.overlay.dataset.mode = mode;
    parts.title.textContent = t(words.title, { name });
    parts.description.textContent = t(words.intro);
    parts.close.setAttribute("aria-label", t(words.cancelLabel));
    parts.close.title = t("pdfImport.cancel");
    parts.cancel.textContent = t("pdfImport.cancel");
    parts.cancel.disabled = false;
    say(t("pdfImport.reading"));
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
    say(t("pdfImport.loadingModule"));
    paintProgress(0, 0);
    parts.bar.dataset.phase = "module";
    try {
      return await loadModule({ onProgress: paintModule, signal: active.abort.signal });
    } finally {
      delete parts.bar.dataset.phase;
    }
  }

  /** Writes the dialog's status line, and hands it to a background run's
   *  status (the status bar) when the run has no dialog on screen. */
  function say(text) {
    if (active?.onStatus) active.onStatus(text);
    else parts.phase.textContent = text;
  }

  /** One staged import without recognition, sliced on this thread; `null`
   *  when the reader cancelled. */
  async function importOnce(module, bytes, mode) {
    let job = null;
    let scansNoted = false;
    try {
      job = module.beginPdfImport(bytes);
      await provideCMaps(module, job.missingCMaps, cmapSource(), active.abort.signal);
      if (active.cancelRequested) return null;
      paintProgress(0, job.pageCount);
      say(t("pdfImport.reading"));
      const clock = scope.performance ?? globalThis.performance;
      const result = await runStagedImport(job, {
        nextSlice: idleSlices(scope),
        now: () => clock.now(),
        cancelled: () => active.cancelRequested,
        onProgress: (done, total) => {
          paintProgress(done, total);
          // The first scan read starts the models loading in the worker, in
          // parallel with the pages still to read (`docs/182` §17a.5).
          if (!scansNoted && (job.scannedPages ?? 0) > 0) {
            scansNoted = true;
            try {
              io.onScans?.();
            } catch {
              // A preload that cannot start is the recognition run's to report.
            }
          }
        },
      });
      if (result.status === "cancelled") return null;
      say(t("pdfImport.finishing"));
      parts.cancel.disabled = true;
      // The layout is decided now, when the pages are read: the reader may
      // have ticked "Keep exact page layout" while they were.
      const layout = layoutOf(mode);
      parts.keep.disabled = true;
      await painted();
      if (active.cancelRequested) return null;
      clock.mark?.(FINISH_MARK);
      try {
        const pkg = layout === "flowing" ? job.finishReflowed() : job.finish();
        const ocr = job.ocrSummaryJson ? JSON.parse(job.ocrSummaryJson) : null;
        return { pkg, ocr, layout };
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

  /** One plain import (no recognition) in the worker: the pages are read off
   *  this thread, which only paints the progress. A scanned page costs ~600 ms
   *  to decode and cannot be split into slices, so the sliced import below
   *  froze the tab once per scanned page (owner report 2026-11-02: an 8-page
   *  scan blocked the main thread for 4.7 s). `null` when cancelled. */
  async function workerImport(bytes, mode, service) {
    let scansNoted = false;
    // Until the first page is reported the worker is fetching its PDF module
    // (the first time) or admitting the file.
    say(t(workerReady ? "pdfImport.reading" : "pdfImport.loadingModule"));
    paintProgress(0, 0);
    try {
      const imported = await service.recognise(bytes, {
        manifestUrl: null,
        mode,
        cmapSource: cmapSource(),
        signal: active.abort.signal,
        onPages: (done, total, scanned) => {
          if (!workerReady) say(t("pdfImport.reading"));
          workerReady = true;
          paintProgress(done, total);
          // The first scan read starts the models loading, in parallel with
          // the pages still to read (`docs/182` §17a.5), as before.
          if (!scansNoted && scanned > 0) {
            scansNoted = true;
            try {
              io.onScans?.();
            } catch {
              // A preload that cannot start is the recognition run's to report.
            }
          }
        },
        onRead: () => {
          say(t("pdfImport.finishing"));
          parts.cancel.disabled = true;
        },
        layout: () => {
          parts.keep.disabled = true;
          return layoutOf(mode);
        },
      });
      // The pages were read and finished in the worker: from here, this
      // thread's work is opening the package (`pdf-import.spec` partitions on it).
      (scope.performance ?? globalThis.performance).mark?.(FINISH_MARK);
      return imported;
    } catch (error) {
      // Cancel ends the worker, and its module with it.
      workerReady = false;
      if (active.cancelRequested || error?.name === "AbortError") return null;
      throw error;
    }
  }

  /** One import WITH recognition, in the OCR worker; `null` when the reader
   *  cancelled. Nothing of it runs on this thread but painting the progress
   *  the worker reports (`ocr_service.mjs`). */
  async function recogniseOnce(bytes, mode, recognition) {
    const clock = scope.performance ?? globalThis.performance;
    parts.bar.dataset.phase = "models";
    say(t("pdfOcr.loading"));
    clock.mark?.(OCR_START_MARK);
    try {
      const result = await recognition.service.recognise(bytes, {
        manifestUrl: recognition.manifestUrl,
        mode,
        cmapSource: cmapSource(),
        signal: active.abort.signal,
        onModels: (done, total) => {
          parts.bar.dataset.phase = "models";
          say(t("pdfOcr.downloading"));
          paintModule(done, total);
        },
        onLoading: () => {
          parts.bar.dataset.phase = "models";
          say(t("pdfOcr.loading"));
          paintProgress(0, 0);
        },
        onPages: (done, total) => {
          delete parts.bar.dataset.phase;
          // A scanned page takes seconds in the worker, and the count moves
          // only when it is finished: name the page being read now, so
          // "Page 0 of 8" never reads as stuck (HF-414).
          say(done < total ? t("pdfOcr.recognisingPage", { page: n(done + 1), total: n(total) }) : t("pdfImport.reading"));
          paintProgress(done, total);
        },
        onRead: () => {
          say(t("pdfImport.finishing"));
        },
        // The layout is decided when the pages are read: the reader may have
        // ticked "Keep exact page layout" while they were.
        layout: () => {
          parts.keep.disabled = true;
          return layoutOf(mode);
        },
      });
      clock.mark?.(OCR_END_MARK);
      return result;
    } catch (error) {
      if (active.cancelRequested || error?.name === "AbortError") return null;
      throw new Error(t("pdfOcr.failed", { reason: String(error?.message ?? error) }));
    } finally {
      delete parts.bar.dataset.phase;
    }
  }

  /**
   * Imports `bytes` behind the dialog.
   *
   * @param {Uint8Array} bytes
   * @param {string} name
   * @param {object} [options]
   * @param {{service: object, manifestUrl: string}} [options.recognition]
   *   recognise text in scanned pages, in the OCR worker (`ocr_service.mjs`'s
   *   service and the host's manifest).
   * @param {"import"|"fixed"|"reflow"|"ocr"} [options.mode] what the dialog
   *   says and how the job finishes: `import` and `ocr` in the layout the
   *   dialog's checkbox ends on, `fixed` in the exact page layout, `reflow`
   *   as a new flowing document that keeps no PDF.
   * @param {{onStatus: (text: string) => void}} [options.background] run with
   *   no dialog, reporting progress to `onStatus`; `cancel()` stops it.
   * @param {"flowing"|"fixed"} [options.layout] where the checkbox starts
   *   (default: the host's `?pdfLayout=`, else flowing).
   * @returns {Promise<{doc: object, ocr: object|null, layout: string} | {cancelled: true}>}
   *   the new document, not yet adopted by the editor, and what OCR did; or a
   *   cancellation, with nothing changed.
   * @throws the engine's refusal or a failed page, as an `Error`.
   */
  async function run(bytes, name, options = {}) {
    if (active) throw new Error(t("pdfImport.busy"));
    // `background`: recognition after a scan opened runs with no dialog on
    // screen, its progress in the status bar, so the scanned pages stay
    // readable and scrollable for the minutes it takes (owner report
    // 2026-11-02: a modal over an 8-page OCR run made the editor unusable).
    const background = options.background ?? null;
    active = {
      cancelRequested: false,
      abort: new AbortController(),
      onStatus: background?.onStatus ?? null,
      // A background run has no checkbox to read: its layout is the one asked for.
      layout: background ? (options.layout ?? hostLayout()) : null,
    };
    const recognition = options.recognition ?? null;
    const mode = options.mode ?? (recognition ? "ocr" : "import");
    // A background run leaves the dialog as it is: it is not on screen, and an
    // import that opens while it runs must find the dialog in its own state.
    if (!background) renderOpen(name, mode, options.layout ?? hostLayout());
    if (!background) modal.open();
    // The run's state where a host, an assistive reader's script or a test
    // can see it, since no dialog says it: removed when the run ends.
    else if (view.body) view.body.dataset.pdfRecognition = "running";
    try {
      await painted();
      if (active.cancelRequested) return { cancelled: true };
      let imported;
      const worker = recognition ? null : io.worker?.();
      if (recognition) {
        imported = await recogniseOnce(bytes, mode, recognition);
      } else if (worker) {
        // The worker loads its own PDF module; this thread loads none, so the
        // module is fetched and held in memory once, not twice.
        imported = await workerImport(bytes, mode, worker);
      } else {
        let module;
        try {
          module = await ensureModule();
        } catch (error) {
          if (active.cancelRequested || error?.name === "AbortError") return { cancelled: true };
          throw new Error(t("pdfImport.moduleFailed", { reason: String(error?.message ?? error) }));
        }
        if (active.cancelRequested) return { cancelled: true };
        try {
          imported = await importOnce(module, bytes, mode);
        } catch (error) {
          const source = cmapSource();
          if (!source || !error.neededCMaps?.length) throw error;
          await provideCMaps(module, error.neededCMaps, source, active.abort.signal);
          parts.cancel.disabled = false;
          imported = await importOnce(module, bytes, mode);
        }
      }
      if (!imported || active.cancelRequested) return { cancelled: true };
      // A flowing document keeps the PDF it was rebuilt from, so it can be
      // opened again in its exact layout or with OCR; a conversion from an
      // open fixed-layout PDF is a new document and keeps none.
      const doc =
        imported.layout === "fixed"
          ? openImport(imported.pkg, bytes)
          : openReflow(imported.pkg, mode === "reflow" ? undefined : bytes);
      return { doc, ocr: imported.ocr, layout: imported.layout };
    } finally {
      active = null;
      if (view.body) delete view.body.dataset.pdfRecognition;
      if (modal.isOpen) modal.close("done");
    }
  }

  /** The layout `mode` finishes in: a conversion is flowing, a reopen with
   *  the exact layout is fixed, and an open (or a recognition run) is what
   *  the checkbox says. */
  function layoutOf(mode) {
    if (mode === "reflow") return "flowing";
    if (mode === "fixed") return "fixed";
    if (active?.layout) return active.layout;
    return parts.keep.checked ? "fixed" : "flowing";
  }

  parts.cancel.addEventListener("click", requestCancel);
  parts.close.addEventListener("click", () => modal.close("cancel"));
  // The wheel over the backdrop scrolls the document behind it, so a long
  // recognition run can be spent reading the pages. Focus stays in the
  // dialog: reading, not editing, a document the import is about to replace.
  parts.overlay.addEventListener(
    "wheel",
    (event) => {
      if (!active || parts.overlay.firstChild?.contains(event.target)) return;
      const target = io.scrollTarget?.();
      if (!target) return;
      const unit = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? target.clientHeight : 1;
      target.scrollBy({ top: event.deltaY * unit, left: event.deltaX * unit });
    },
    { passive: true },
  );

  return {
    run,
    /** Stops the import in flight, a background one included. */
    cancel: () => {
      if (active) requestCancel();
    },
    /** Whether the import in flight runs with no dialog (background OCR). */
    get background() {
      return Boolean(active?.onStatus);
    },
    /** Whether an import is in flight (a second open waits for it). */
    get running() {
      return active !== null;
    },
  };
}
