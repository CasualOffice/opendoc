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

/** Print the rendered document pages. Read-only, always allowed (no mutation
 *  gate, no unsaved-changes requirement). */
export function printDocument(doc) {
  if (!doc) return;
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
