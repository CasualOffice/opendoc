// Shared by `pdf-import.spec.mjs` and the modal roster (`modal-roster.mjs`):
// opening the committed PDF fixture with the import's slices held, so the
// progress dialog can be looked at in a state a fast machine passes through in
// a frame. The slices are delayed, never skipped.
import { fileURLToPath } from "node:url";
import { deflateSync } from "node:zlib";
import { expect } from "./fixtures.mjs";

export const PDF_FIXTURE = fileURLToPath(new URL("../../../fixtures/pdf/synthetic-prose.pdf", import.meta.url));

/**
 * A PDF whose every page is only a picture: a scan with nothing native on it
 * (`casual-doc-io`'s `blank_scan`). By default each page is an 8 x 8 white
 * image; `pixels` (8-bit grey, `width` x `height`, row-major) gives every page
 * that picture instead, Flate-compressed.
 */
export function scanPdf({ pages = 1, width = 8, height = 8, pixels = null } = {}) {
  const grey = pixels ?? Buffer.alloc(width * height, 255);
  const data = pixels ? deflateSync(grey) : grey;
  const filter = pixels ? " /Filter /FlateDecode" : "";
  const content = Buffer.from("q 612 0 0 792 0 0 cm /Im0 Do Q", "latin1");
  const kids = Array.from({ length: pages }, (_, i) => `${3 + i * 2} 0 R`).join(" ");
  const image = 3 + pages * 2;
  const objects = [
    Buffer.from("<< /Type /Catalog /Pages 2 0 R >>", "latin1"),
    Buffer.from(`<< /Type /Pages /Kids [${kids}] /Count ${pages} >>`, "latin1"),
  ];
  for (let i = 0; i < pages; i += 1) {
    objects.push(
      Buffer.from(
        `<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /XObject << /Im0 ${image} 0 R >> >> /Contents ${4 + i * 2} 0 R >>`,
        "latin1",
      ),
      Buffer.concat([Buffer.from(`<< /Length ${content.length} >>\nstream\n`, "latin1"), content, Buffer.from("\nendstream", "latin1")]),
    );
  }
  objects.push(
    Buffer.concat([
      Buffer.from(
        `<< /Type /XObject /Subtype /Image /Width ${width} /Height ${height} /ColorSpace /DeviceGray /BitsPerComponent 8${filter} /Length ${data.length} >>\nstream\n`,
        "latin1",
      ),
      data,
      Buffer.from("\nendstream", "latin1"),
    ]),
  );
  const parts = [Buffer.from("%PDF-1.7\n", "latin1")];
  let length = parts[0].length;
  const offsets = [];
  objects.forEach((body, i) => {
    offsets.push(length);
    const chunk = Buffer.concat([Buffer.from(`${i + 1} 0 obj\n`, "latin1"), body, Buffer.from("\nendobj\n", "latin1")]);
    parts.push(chunk);
    length += chunk.length;
  });
  let tail = `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`;
  for (const offset of offsets) tail += `${String(offset).padStart(10, "0")} 00000 n \n`;
  tail += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${length}\n%%EOF\n`;
  parts.push(Buffer.from(tail, "latin1"));
  return Buffer.concat(parts);
}

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
