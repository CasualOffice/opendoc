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

/** Holds the PDF module's download, so an open stays in its dialog. The pages
 *  are read in the worker (HF-423), and the worker's first job is fetching the
 *  module: held there, the import is up and cancellable with no page read.
 *  Installed on the context, so the worker's request is caught too. Cancel
 *  ends the worker, so the next open fetches (and is held) again.
 *  `hold()` closes the gate, `release()` opens it. */
export async function holdPdfModule(page) {
  let gate = null;
  let open = null;
  const held = {
    hold() {
      gate ??= new Promise((resolve) => {
        open = resolve;
      });
    },
    release() {
      open?.();
      gate = null;
    },
  };
  await page.context().route(/casual_doc_wasm_pdf_bg\.wasm/, async (route) => {
    if (gate) await gate;
    // A cancelled import ended the worker that asked: nothing to answer.
    await route.continue().catch(() => {});
  });
  return held;
}

/** Starts opening `file` with the PDF module held; returns once the dialog is
 *  up. `held` is `holdPdfModule`'s handle. */
export async function startHeldPdfImport(page, held, file = PDF_FIXTURE) {
  held.hold();
  await page.locator("#file").setInputFiles(file);
  await expect(page.locator("#pdfImportDialog")).toBeVisible();
}

/** A page of printed text as 8-bit grey pixels, drawn by the browser. */
export async function printedPage(page, width, height) {
  const base64 = await page.evaluate(
    ({ width, height }) => {
      const canvas = new OffscreenCanvas(width, height);
      const g = canvas.getContext("2d");
      g.fillStyle = "#fff";
      g.fillRect(0, 0, width, height);
      g.fillStyle = "#000";
      g.font = "30px sans-serif";
      const lines = [
        "Quarterly report of the northern office",
        "Revenue grew by twelve percent this year",
        "The board approved the new budget plan",
        "Shipping times fell to three working days",
        "Customer complaints dropped by one half",
        "Next review is scheduled for the spring",
      ];
      lines.forEach((line, i) => g.fillText(line, 70, 120 + i * 70));
      const rgba = g.getImageData(0, 0, width, height).data;
      const grey = new Uint8Array(width * height);
      for (let i = 0; i < grey.length; i += 1) grey[i] = rgba[i * 4];
      let text = "";
      for (let i = 0; i < grey.length; i += 0x8000) text += String.fromCharCode(...grey.subarray(i, i + 0x8000));
      return btoa(text);
    },
    { width, height },
  );
  return Buffer.from(base64, "base64");
}
