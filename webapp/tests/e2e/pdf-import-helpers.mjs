// Shared by `pdf-import.spec.mjs` and the modal roster (`modal-roster.mjs`):
// opening the committed PDF fixture with the import's slices held, so the
// progress dialog can be looked at in a state a fast machine passes through in
// a frame. The slices are delayed, never skipped.
import { fileURLToPath } from "node:url";
import { expect } from "./fixtures.mjs";

export const PDF_FIXTURE = fileURLToPath(new URL("../../../fixtures/pdf/synthetic-prose.pdf", import.meta.url));

/** Holds every idle callback while `window.__holdIdle` is true; installed
 *  before the editor loads. `window.__releaseIdle()` lets them run. */
export async function installIdleGate(page) {
  await page.addInitScript(() => {
    const real = window.requestIdleCallback?.bind(window);
    if (!real) return;
    const held = [];
    window.__holdIdle = false;
    window.requestIdleCallback = (callback, options) => {
      if (!window.__holdIdle) return real(callback, options);
      held.push([callback, options]);
      return 0;
    };
    window.__releaseIdle = () => {
      window.__holdIdle = false;
      for (const [callback, options] of held.splice(0)) real(callback, options);
    };
  });
}

/** Starts opening `file` with the slices held; returns once the dialog is up. */
export async function startHeldPdfImport(page, file = PDF_FIXTURE) {
  await page.evaluate(() => {
    window.__holdIdle = true;
  });
  await page.locator("#file").setInputFiles(file);
  await expect(page.locator("#pdfImportDialog")).toBeVisible();
}
