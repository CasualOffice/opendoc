// "Page X of Y" follows the reader while scrolling (audit CANVAS-09).
//
// The status bar's page number followed only the caret, so scrolling to page 6
// still said "Page 1 of 14". Word and ONLYOFFICE show the page most in view
// while you scroll; the caret's page comes back the moment the caret moves or
// typing starts, because `main.js`'s `updatePageNumber` runs on every
// selection repaint and writes the caret's page as before. This only adds the
// scroll half.
//
// Complexity: O(materialized sheets) per animation frame of scrolling, a
// handful of rectangles; no document call except the page total.
import { pageIndicator } from "./status_counts.mjs";
import { pageTotalLabel } from "./background_measure.mjs";

/**
 * The 0-based index (into `sheets`) of the sheet with the largest visible area
 * in `view`, or -1 when none is visible. Pure, for its test.
 *
 * @param {Array<{top:number,bottom:number,left:number,right:number}>} sheets
 * @param {{top:number,bottom:number,left:number,right:number}} view
 */
export function dominantSheet(sheets, view) {
  let best = -1;
  let bestArea = 0;
  sheets.forEach((r, i) => {
    const h = Math.min(r.bottom, view.bottom) - Math.max(r.top, view.top);
    const w = Math.min(r.right, view.right) - Math.max(r.left, view.left);
    const area = h > 0 && w > 0 ? h * w : 0;
    if (area > bestArea) {
      bestArea = area;
      best = i;
    }
  });
  return best;
}

export function createPageInView(ed) {
  const statPages = document.getElementById("statPages");
  let frame = 0;
  function update() {
    frame = 0;
    if (!statPages || !ed.doc || !ed.pages?.length) return;
    if (ed.reflowView?.isOn?.()) return; // reflow reports a reading position, not a page
    const view = ed.viewportEl.getBoundingClientRect();
    // While the caret is on screen its page is the answer (and the one
    // `updatePageNumber` already wrote); a scroll that typing caused must not
    // swap it for the neighbouring page.
    const caret = ed.pagesEl.querySelector(".overlay .caret")?.getBoundingClientRect();
    if (caret && caret.height > 0 && caret.top >= view.top && caret.bottom <= view.bottom) return;
    const sheets = [...ed.materializedPages()].filter((page) => page.wrap);
    const at = dominantSheet(sheets.map((page) => page.wrap.getBoundingClientRect()), view);
    if (at < 0) return;
    const total = pageTotalLabel(ed.pages.length, ed.doc.estimatedPageCount, ed.doc.pageCountIsExact);
    statPages.textContent = pageIndicator(sheets[at].pageNumber, total);
  }
  ed.viewportEl.addEventListener(
    "scroll",
    () => {
      if (!frame) frame = requestAnimationFrame(update);
    },
    { passive: true },
  );
  return { update };
}
