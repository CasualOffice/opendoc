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
// The work is the engine's; the slicing is `pdf_import_scheduler.mjs`'s. This
// module is the dialog and the sequence:
//
//   open the dialog → let it paint → `beginPdfImport` (admission, one task)
//   → slices of pages, the bar advancing between them → "Laying out" → the
//   atomic finish → close, and hand the new document to `openBytes`.
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
import { beginPdfImport } from "../pkg/casual_doc_wasm.js";
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

/**
 * @param {object} [io]
 * @param {Document} [io.view]
 * @param {object} [io.scope] `window`: its idle callbacks, frames and clock.
 * @param {() => HTMLElement} [io.fallbackFocus] where focus goes if the opener is gone.
 * @param {(bytes: Uint8Array) => object} [io.begin] the engine's `beginPdfImport`.
 */
export function createPdfImport(io = {}) {
  const view = io.view ?? globalThis.document;
  const scope = io.scope ?? globalThis;
  const begin = io.begin ?? beginPdfImport;
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

  function renderOpen(name) {
    parts.title.textContent = t("pdfImport.title", { name });
    parts.description.textContent = t("pdfImport.intro");
    parts.close.setAttribute("aria-label", t("pdfImport.cancelLabel"));
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

  /**
   * Imports `bytes` behind the dialog.
   *
   * @returns {Promise<{doc: object} | {cancelled: true}>} the new document, not
   *   yet adopted by the editor; or a cancellation, with nothing changed.
   * @throws the engine's refusal or a failed page, as an `Error`.
   */
  async function run(bytes, name) {
    if (active) throw new Error(t("pdfImport.busy"));
    active = { cancelRequested: false };
    renderOpen(name);
    modal.open();
    let job = null;
    try {
      await painted();
      if (active.cancelRequested) return { cancelled: true };
      job = begin(bytes);
      paintProgress(0, job.pageCount);
      const clock = scope.performance ?? globalThis.performance;
      const result = await runStagedImport(job, {
        nextSlice: idleSlices(scope),
        now: () => clock.now(),
        cancelled: () => active.cancelRequested,
        onProgress: paintProgress,
      });
      if (result.status === "cancelled") return { cancelled: true };
      parts.phase.textContent = t("pdfImport.finishing");
      parts.cancel.disabled = true;
      await painted();
      if (active.cancelRequested) return { cancelled: true };
      clock.mark?.(FINISH_MARK);
      // Consumes the job: wasm-bindgen frees it whether this returns or throws.
      const finishing = job;
      job = null;
      return { doc: finishing.finish() };
    } finally {
      job?.free();
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
