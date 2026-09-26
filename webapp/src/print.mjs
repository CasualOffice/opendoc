// Printing (⌘/Ctrl+P) — every page, at full resolution, off-DOM.
//
// Extracted from `main.js` (`109` HF-085) alongside the page-band work that
// made the split obvious: printing is the one path that must touch EVERY page
// of a document while the viewport deliberately holds only a window of them
// (`docs/113` §8.6), so it shares nothing with the viewport's own rendering and
// has no business sitting next to it. It takes the open document as an
// argument and owns the rest.

import { TWIPS_PER_INCH } from "./units.mjs";

// Printing must reproduce EVERY page, but the viewport keeps a live raster only
// for on-screen pages (virtualization), so `window.print()` alone would emit
// mostly-blank sheets. A dedicated print path renders each page independently
// with `doc.renderPage` into an off-DOM `#printContainer` (one canvas per page
// at the page's real physical size), calls the browser print dialog, then tears
// the container down. It never touches the live `.page-wrap`/`.overlay`/canvas
// set, so the normal virtualized state is preserved automatically — nothing to
// restore. Each transient bitmap is `free()`d right after the blit (as
// `paintPageCanvas` does) so a long document's print build never balloons tab
// memory beyond the sheet canvases it must hold to print.

// Print raster resolution. High enough for crisp printed text, low enough that
// the transient per-page RGBA buffer (freed immediately) and the retained sheet
// canvases stay modest even for a long document.
const PRINT_DPI = 150;
let printStyleEl = null;

/** Remove the off-DOM print container and its injected stylesheet, if present.
 *  Idempotent, so it is safe to call defensively before a build and in the
 *  `finally` after `window.print()`. */
function teardownPrint() {
  document.getElementById("printContainer")?.remove();
  printStyleEl?.remove();
  printStyleEl = null;
}

/** Build the print-only stylesheet. On screen `#printContainer` is hidden; in
 *  print it is the ONLY visible element (all editor chrome is hidden) and each
 *  page sheet breaks to its own physical page. `@page` is sized to the document
 *  page with zero margin — the rendered raster already includes the document's
 *  own margins, so a sheet margin here would double them. */
function buildPrintStyle(wIn, hIn) {
  const style = document.createElement("style");
  style.id = "printStyle";
  style.textContent = `
#printContainer { display: none; }
@media print {
  html, body { margin: 0 !important; padding: 0 !important; background: #fff !important; }
  body > *:not(#printContainer) { display: none !important; }
  #printContainer { display: block !important; }
  #printContainer .print-page { display: block; break-after: page; page-break-after: always; }
  #printContainer .print-page:last-child { break-after: auto; page-break-after: auto; }
  @page { size: ${wIn}in ${hIn}in; margin: 0; }
}`;
  return style;
}

/** The engine's own id for the real-text PDF writer (`casual_doc_io::formats`),
 *  the same one File ▸ Export as PDF dispatches through. */
const PDF_FORMAT = "application.pdf";

/** How long to keep the print frame alive when the browser never reports
 *  `afterprint`. Revoking the blob or removing the frame while the print
 *  preview still needs it cancels the job, so the cleanup errs late. */
const PRINT_FRAME_TTL_MS = 120_000;

/** Print through the real-text PDF, which is what `PDF-PRINT-0` in `docs/98`
 *  specifies: "PDF → browser print handoff (`window.print()` on a PDF
 *  object/hidden frame)".
 *
 *  Returns false — without printing — when the engine cannot produce a PDF, so
 *  the caller can fall back to the raster path rather than leaving the user with
 *  a Print command that silently did nothing.
 */
async function printViaPdf(doc) {
  let bytes;
  try {
    const artifact = doc.exportAs(PDF_FORMAT, "semantic");
    bytes = artifact.bytes;
    artifact.free();
  } catch (err) {
    console.warn("print: PDF export unavailable, falling back to raster:", err?.message ?? err);
    return false;
  }
  const url = URL.createObjectURL(new Blob([bytes], { type: "application/pdf" }));
  const frame = document.createElement("iframe");
  frame.id = "printFrame";
  frame.setAttribute("aria-hidden", "true");
  frame.setAttribute("tabindex", "-1");
  // Off-screen rather than `display:none`: a frame that is not laid out has no
  // print view to invoke in Chromium.
  frame.style.cssText =
    "position:fixed; right:0; bottom:0; width:1px; height:1px; opacity:0; border:0; pointer-events:none;";
  const loaded = new Promise((resolve, reject) => {
    frame.addEventListener("load", resolve, { once: true });
    frame.addEventListener("error", reject, { once: true });
  });
  frame.src = url;
  document.body.appendChild(frame);
  try {
    await loaded;
    const view = frame.contentWindow;
    if (!view) throw new Error("print frame has no view");
    let cleaned = false;
    const cleanup = () => {
      if (cleaned) return;
      cleaned = true;
      frame.remove();
      URL.revokeObjectURL(url);
    };
    view.addEventListener?.("afterprint", cleanup, { once: true });
    setTimeout(cleanup, PRINT_FRAME_TTL_MS);
    view.focus();
    view.print();
    return true;
  } catch (err) {
    console.warn("print: PDF handoff failed, falling back to raster:", err?.message ?? err);
    frame.remove();
    URL.revokeObjectURL(url);
    return false;
  }
}

/** Print the document. Read-only, always allowed (no mutation gate, no
 *  unsaved-changes requirement).
 *
 *  Prefers the PDF handoff so the printed output carries REAL TEXT. The raster
 *  path below prints a 150-DPI picture of each page, which is visibly soft on a
 *  600-DPI printer and — because "Print" is how most people produce a PDF —
 *  produced PDFs with no selectable, searchable or screen-readable text at all.
 *  It stays as the fallback for a build whose engine cannot write PDF.
 */
export async function printDocument(doc) {
  if (!doc) return;
  if (await printViaPdf(doc)) return;
  printViaRaster(doc);
}

/** The 150-DPI page-image path. Fallback only — see `printDocument`. */
function printViaRaster(doc) {
  teardownPrint(); // clear any stale build from an interrupted prior print
  const count = doc.pageCount;
  if (!count) return;

  // The sheet size (for `@page`) comes from the first page in inches. Each page
  // canvas is additionally sized to its own physical dimensions, so a document
  // with mixed page sizes still prints each page at its true proportion.
  const first = doc.pageSize(0);
  const sheetWIn = first.widthTwip / TWIPS_PER_INCH;
  const sheetHIn = first.heightTwip / TWIPS_PER_INCH;
  first.free();

  const container = document.createElement("div");
  container.id = "printContainer";
  container.setAttribute("aria-hidden", "true");

  for (let i = 0; i < count; i++) {
    let bmp;
    try {
      bmp = doc.renderPage(i, PRINT_DPI);
    } catch (err) {
      console.error(`print render page ${i}`, err);
      continue;
    }
    const canvas = document.createElement("canvas");
    canvas.className = "print-page";
    canvas.width = bmp.widthPx;
    canvas.height = bmp.heightPx;
    canvas.getContext("2d").putImageData(new ImageData(bmp.rgba, bmp.widthPx, bmp.heightPx), 0, 0);
    bmp.free(); // return the RGBA buffer to WASM now, not at GC.
    // Present the high-res raster at the page's true physical size so it fills
    // the (margin-0) sheet exactly and prints at full resolution.
    const size = doc.pageSize(i);
    canvas.style.width = `${size.widthTwip / TWIPS_PER_INCH}in`;
    canvas.style.height = `${size.heightTwip / TWIPS_PER_INCH}in`;
    size.free();
    container.appendChild(canvas);
  }

  printStyleEl = buildPrintStyle(sheetWIn, sheetHIn);
  document.head.appendChild(printStyleEl); // hides the container on screen first
  document.body.appendChild(container);
  try {
    window.print();
  } finally {
    // `window.print()` blocks until the dialog is dismissed in Chromium/Firefox,
    // so the sheets are gone as soon as printing ends — the viewport's live
    // virtualized canvases were never disturbed.
    teardownPrint();
  }
}
