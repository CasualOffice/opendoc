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
 *
 *  **THE CALLER MUST REPAINT WHEN THIS RESOLVES.** `withPagedLayout` borrows two
 *  view parameters and puts both back — the layout view and the fold set — and
 *  neither restore repaints by itself. Without a repaint the canvas, the outline
 *  tree and the accessibility mirror are all left describing the borrowed view
 *  after the reader's own has been restored underneath them, which is a shell
 *  that disagrees with its own engine. Stated here rather than assumed because
 *  the fold restore made it observable and the layout-view restore had the same
 *  property unnoticed.
 */
export async function printDocument(doc) {
  if (!doc) return;
  await withPagedLayout(doc, async () => {
    if (await printViaPdf(doc)) return;
    printViaRaster(doc);
  });
}

/**
 * Runs `build` with the engine laid out on the document's own paper, whatever
 * view the reader has on screen, and puts the view back afterwards.
 *
 * PAPER IS PAPER (`docs/151` §6.4, ADR-046). Reflow lays the body out at the
 * reader's window width and cuts it into 11in TILES, which is the right answer
 * for a phone and the wrong one for a printer: the sheets would come out at the
 * width of somebody's browser, every tile boundary would fall mid-paragraph, and
 * every header, footer, page border and watermark the author wrote would be
 * missing, because reflow suppresses all four. That is not a rendering
 * preference, it is a different document, so this is a correctness requirement
 * rather than a nicety.
 *
 * It lives HERE, next to the two printers, rather than in the shell, because
 * `printDocument` has two entry points already (`file.print` and the Print
 * shortcut) and the PDF writer it prefers is the same one `file.export.pdf`
 * uses. A rule enforced beside the thing it is a rule about cannot be forgotten
 * by the next caller.
 *
 * COST, stated rather than hidden: switching the view is O(document) in each
 * direction, because the galley cache is width-scoped. Printing is already the
 * one path that touches every page at full resolution, so it is the one
 * interaction where a whole re-shape is in proportion. The restore is in a
 * `finally`, so a printer error or a dismissed dialog cannot strand the reader
 * on paper they did not ask for.
 *
 * `setLayoutView` is a VIEW and issues no operation, so none of this touches the
 * document, its revision, its undo history or its dirty state (ADR-046 §3.1).
 */
export async function withPagedLayout(doc, build) {
  let restore = null;
  try {
    const view = JSON.parse(doc.layoutView ?? "{}");
    if (view.reflow) {
      restore = [view.contentWidthTwip, view.tileHeightTwip, view.gutterTwip];
      doc.setLayoutView(0, 0, 0);
    }
  } catch {
    // An engine with no layout-view seam at all, or one that refused: print what
    // it is showing rather than not printing. Silent because the alternative is
    // a message about an internal view parameter in front of a print dialog.
    restore = null;
  }
  const refold = expandFolds(doc);
  try {
    await build();
  } finally {
    refold();
    if (restore) doc.setLayoutView(...restore);
  }
}

/**
 * Expands every collapsed heading for the duration of an export, and returns the
 * thunk that puts the reader's folds back.
 *
 * **A FOLD IS A VIEW; PAPER IS THE DOCUMENT.** A collapsed heading hides content
 * on screen and the page count falls with it (`fold.rs`, ADR-049) — which is
 * right for reading and is silent data loss on paper. Printing the folded
 * document drops whole sections out of the sheets, out of the PDF, and out of
 * anything produced through the print path, with no message and nothing in the
 * output to say a section was omitted. `AGENTS.md` forbids exactly that: *no
 * silent data loss in release behavior*. So the rule is not "print what is on
 * screen"; it is **print the document**, and the folds come back afterwards.
 *
 * It is enforced HERE rather than at each caller for the reason the paper rule
 * above is: `printDocument` already has two entry points, and `withPagedLayout`
 * is what both of them go through. A rule placed beside the thing it is a rule
 * about cannot be forgotten by the next printer added next to these two.
 *
 * MEASURED, so the scope of the bug is not overstated: today the real-text PDF
 * path is already expanded by accident — `casual_doc_io::pdf` paginates from the
 * MODEL (`export_document(request.document, …)`) and the session's fold set
 * never reaches it — and so is `exportAs` for DOCX/ODT/text, which encode the
 * model directly. The path that was really printing folded is the 150-DPI raster
 * fallback, which reads `doc.pageCount` and `doc.renderPage(i)` straight off the
 * live, fold-filtered layout. Enforcing it at the seam covers that fallback AND
 * the planned change `casual_doc_io::pdf`'s own doc comment names — seeding the
 * export's pagination from the session — which would hand the live fold set to
 * the PDF writer and turn the accident into the bug.
 *
 * COST: one re-layout out and one back, each O(document), on top of the view
 * switch above and only when something is actually folded. `setFoldSet` restores
 * the whole set in ONE re-layout rather than one per heading, so a reader who
 * collapsed two hundred headings pays two passes and not two hundred.
 *
 * A fold issues no `Operation`, so none of this touches the document, its
 * revision, its undo history or its dirty state.
 *
 * @param {any} doc the open engine document
 * @returns {() => void} idempotent-enough restore; a no-op when nothing moved
 */
function expandFolds(doc) {
  let folded = [];
  try {
    folded = JSON.parse(doc.foldState()).folded ?? [];
    if (!folded.length) return () => {};
    doc.unfoldAll();
  } catch {
    // An engine with no fold seam, or one that refused to unfold: print what it
    // has rather than not printing. Silent, like the layout-view arm above — but
    // note the asymmetry, because it matters: a refusal HERE means the output may
    // be short, so the fold set is left alone and nothing is restored either.
    return () => {};
  }
  return () => {
    try {
      doc.setFoldSet(JSON.stringify(folded));
    } catch {
      // The reader keeps an expanded document rather than a half-restored one.
      // Visible and recoverable (one Collapse All), unlike a silently short PDF.
    }
  };
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
