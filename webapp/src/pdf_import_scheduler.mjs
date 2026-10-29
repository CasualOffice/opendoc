// Driving the engine's staged PDF import in slices the main thread can afford
// (`docs/182` §6, P2; `docs/116`).
//
// THE PATTERN is cooperative scheduling over a resumable job: the engine's
// `WasmPdfImport` does at most N pages per `step` and keeps its own place, so
// the host decides how much work one task may hold. This module is that
// decision and nothing else, kept free of the browser so it can be tested in
// node against a fake job and a fake clock (`pdf_import_scheduler.test.mjs`).
//
// One slice = one task. Inside a slice it steps ONE page at a time and stops
// as soon as the next page is predicted to overrun the slice's budget, which
// is the canonical `requestIdleCallback` loop ("while timeRemaining() > 0 and
// there is work"). Stepping a page at a time costs one wasm boundary crossing
// per page, which is nothing beside a page's import. The prediction is the
// slowest page seen so far, not the average: a slice that ends early wastes a
// few milliseconds of idle time, one that runs long is the frozen tab this
// exists to prevent. A slice always does at least one page, so a single page
// heavier than the whole budget is still imported rather than retried forever;
// that page is the one task this module cannot bound, and the per-page limits
// in the reader (`docs/21`) are what bound it.
//
// Between slices the browser gets the thread back — to paint the progress bar,
// and to deliver the Cancel click, which is checked before every page.
//
// Complexity: O(pages) steps in total, O(1) work here per page.

/** What one slice may spend, in ms. Under the 50 ms long-task threshold, so a
 *  slice the browser counts as a long task is one page's own cost and not this
 *  module's choice; and far under the 300-800 ms interaction budgets
 *  `main-thread-budget.spec.mjs` holds. */
export const SLICE_BUDGET_MS = 40;

/** How long an idle callback may wait before it runs anyway. A busy page never
 *  goes idle, and an import that waited for idleness would never finish. */
export const IDLE_TIMEOUT_MS = 100;

/**
 * Runs `job` to an end, a slice at a time.
 *
 * @param {object} job the engine's `WasmPdfImport`, or anything with its shape:
 *   `step(pages) -> "pending"|"done"|"cancelled"|"failed"`, `pagesDone`,
 *   `pageCount`, `cancel()`.
 * @param {object} io
 * @param {(slice: (budgetMs: number) => void) => void} io.nextSlice schedules
 *   one slice; calls it with the ms that slice may use.
 * @param {() => number} io.now a monotonic clock in ms.
 * @param {() => boolean} io.cancelled whether the reader asked to stop.
 * @param {(done: number, total: number) => void} [io.onProgress]
 * @returns {Promise<{status: "done"|"cancelled"|"failed", slices: number}>}
 *   `done` means every page was imported and `job.finish()` may be called.
 *   On `cancelled` the job has been told to cancel; the caller frees it.
 */
export function runStagedImport(job, io) {
  return new Promise((resolve, reject) => {
    let slowestPage = 0;
    let slices = 0;
    const report = () => io.onProgress?.(job.pagesDone, job.pageCount);
    const end = (status) => resolve({ status, slices });
    const slice = (budgetMs) => {
      try {
        slices += 1;
        const start = io.now();
        let status = "pending";
        for (let first = true; ; first = false) {
          if (io.cancelled()) {
            job.cancel();
            end("cancelled");
            return;
          }
          const spent = io.now() - start;
          if (!first && spent + slowestPage > budgetMs) break;
          const before = io.now();
          status = job.step(1);
          slowestPage = Math.max(slowestPage, io.now() - before);
          if (status !== "pending") break;
        }
        report();
        if (status === "pending") io.nextSlice(slice);
        else end(status === "done" ? "done" : status === "cancelled" ? "cancelled" : "failed");
      } catch (error) {
        reject(error);
      }
    };
    report();
    io.nextSlice(slice);
  });
}

/**
 * The browser's `nextSlice`: an idle callback where the browser has one
 * (bounded by `IDLE_TIMEOUT_MS`, and by the idle period it offers), else a
 * zero-delay task. Safari has no `requestIdleCallback`.
 *
 * @param {object} scope `window`, or a stand-in in tests.
 */
export function idleSlices(scope) {
  if (typeof scope.requestIdleCallback === "function") {
    return (slice) =>
      scope.requestIdleCallback(
        (deadline) => {
          // An idle period offers at most 50 ms; a timed-out callback offers 0.
          // Either way the slice keeps to its own budget, never less than one
          // page (the scheduler's rule), never more than SLICE_BUDGET_MS.
          const offered = deadline?.didTimeout ? SLICE_BUDGET_MS : deadline?.timeRemaining?.() ?? 0;
          slice(Math.min(SLICE_BUDGET_MS, Math.max(offered, SLICE_BUDGET_MS / 2)));
        },
        { timeout: IDLE_TIMEOUT_MS },
      );
  }
  return (slice) => scope.setTimeout(() => slice(SLICE_BUDGET_MS), 0);
}

/** Whether `bytes` are a PDF: the `%PDF-` header the engine's probe also
 *  requires (`probe_pdf`). The extension is a hint; the bytes decide, exactly
 *  as they do for every other format the editor opens. */
export function isPdfBytes(bytes) {
  return (
    bytes?.length >= 5 &&
    bytes[0] === 0x25 &&
    bytes[1] === 0x50 &&
    bytes[2] === 0x44 &&
    bytes[3] === 0x46 &&
    bytes[4] === 0x2d
  );
}
