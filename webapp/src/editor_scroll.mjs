// Repainting a page and scrolling a model rectangle, the caret, a marker, a
// review selection or a Find match into view. Moved out of `main.js` unchanged
// (docs/177); it reaches the rest of the editor only through the context `ed`.
import { docToScroll, scrollToDoc } from "./page_scroll.mjs";
import { scrollLeftFor, scrollTargetFor } from "./scroll_into_view.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createScroll(ed) {
  function repaintPage(i) {
    const page = ed.pages[i];
    if (!page) return;
    ed.releasePageCanvas(page);
    if (page.visible) ed.paintPageCanvas(page, i);
  }

  /** Scroll to a rectangle the ENGINE reported — `[page, x, y, w, h]` in twips —
   *  rather than to a DOM node that may not exist.
   *
   *  A find match 20,000 pages away, a comment anchor, a caret after a jump: none
   *  has an overlay element until its page is materialized, and its page is not
   *  materialized until something scrolls there. Model geometry plus the band's
   *  arithmetic answers "where is that, in scroll coordinates" without either.
   *  `block` matches `scrollOverlayIntoView`. Returns whether it scrolled. */
  /** Breathing room between a revealed caret and the edge it was revealed past;
   *  flush lasts one frame and the next repaint puts it back outside. */
  const SCROLL_INTO_VIEW_MARGIN = 8;

  function scrollModelRectIntoView(flat, block = "nearest") {
    if (!ed.pageBandModel || !flat || flat.length < 5) return false;
    const [pageNumber, , y, , h] = flat;
    const page = ed.pages[pageNumber - 1];
    if (!page) return false;
    const { sy } = ed.scaleOf(page);
    const top = ed.pageBandModel.tops[pageNumber - 1] + y * sy;
    const bottom = top + Math.max(1, h * sy);
    const viewportHeight = ed.viewportEl.clientHeight;
    const { docY } = scrollToDoc(ed.pageBandModel, viewportHeight, ed.viewportEl.scrollTop - ed.bandTopInScroller);
    let wanted = docY;
    const margin = SCROLL_INTO_VIEW_MARGIN; // as the overlay path, in doc space
    if (block === "center") wanted = top + (bottom - top) / 2 - viewportHeight / 2;
    else if (top < docY) wanted = top - margin;
    else if (bottom > docY + viewportHeight) wanted = bottom - viewportHeight + margin;
    else return false;
    const target = ed.bandTopInScroller + docToScroll(ed.pageBandModel, viewportHeight, Math.max(0, wanted));
    const max = Math.max(0, ed.viewportEl.scrollHeight - ed.viewportEl.clientHeight);
    ed.viewportEl.scrollTo({ top: Math.max(0, Math.min(max, target)), behavior: "auto" });
    // Synchronously, not on the scroll event: the caller is about to look for the
    // marker it just asked to be shown, and an event a frame later would hand it
    // an empty page.
    ed.updatePageWindow();
    ed.paintPagesInView();
    return true;
  }

  /** The engine rectangle the caret or selection occupies, or null. */
  function selectionModelRect() {
    if (!ed.doc || !ed.selection) return null;
    const { anchor, focus } = ed.selection;
    if (anchor.node !== focus.node || anchor.offset !== focus.offset) {
      const rects = ed.doc.selectionRects(anchor.node, anchor.offset, focus.node, focus.offset);
      if (rects.length >= 5) return rects.slice(0, 5);
    }
    const caret = ed.doc.caretRect(focus.node, focus.offset);
    return caret.length >= 5 ? caret.slice(0, 5) : null;
  }

  /** Scroll one engine-derived overlay marker in the editor viewport (not an
   * arbitrary page ancestor). The selection is painted before this runs, so its
   * DOM rectangle is only a projection of model geometry, never a source of
   * document state. */
  function scrollOverlayIntoView(marker, block = "nearest") {
    if (!marker) return;
    scrollMarkerIntoViewX(marker); // sideways first: it moves nothing vertically (TA-10)
    const target = scrollTargetFor({
      marker: marker.getBoundingClientRect(),
      viewport: ed.viewportEl.getBoundingClientRect(),
      current: ed.viewportEl.scrollTop,
      max: Math.max(0, ed.viewportEl.scrollHeight - ed.viewportEl.clientHeight),
      scale: ed.pageBandModel?.scale ?? 1,
      block,
      margin: SCROLL_INTO_VIEW_MARGIN,
    });
    if (target !== null) ed.viewportEl.scrollTo({ top: target, behavior: "auto" });
  }

  /** Follows a marker sideways when zoom makes the page wider than the window
   *  (`docs/174` TA-10; Word and Docs do). Every scroll-into-view path ends here,
   *  the model path included once it has painted the marker. */
  function scrollMarkerIntoViewX(marker) {
    if (!marker || ed.viewportEl.scrollWidth <= ed.viewportEl.clientWidth) return;
    const { left } = ed.viewportEl.getBoundingClientRect();
    const max = ed.viewportEl.scrollWidth - ed.viewportEl.clientWidth;
    const viewport = { left, right: left + ed.viewportEl.clientWidth };
    const target = scrollLeftFor({ marker: marker.getBoundingClientRect(), viewport, current: ed.viewportEl.scrollLeft, max, margin: SCROLL_INTO_VIEW_MARGIN });
    if (target !== null) ed.viewportEl.scrollLeft = target;
  }

  /** Scroll the caret in the editor viewport. Navigation callers can request a
   * centered target so headings/anchors retain useful reading room. */
  function scrollCaretIntoView(block = "nearest") {
    const marker = ed.pagesEl.querySelector(".overlay .caret");
    // Two cases take the model path. No marker: the caret's page has no sheet,
    // which is where a 25,556-page document spends most of its time. And a
    // COMPRESSED band: a target computed from a screen delta is only as exact as
    // the compression factor it is divided by, and near the ends of the scroll
    // range it is not exact at all — measured 57 px short at the bottom of a
    // 3,300-page document, which is a caret just off screen after an arrow key.
    // Document space has no such error: `docToScroll` is the exact inverse of
    // the mapping that placed the page.
    if (!marker || ed.pageBandModel?.scale > 1) {
      if (scrollModelRectIntoView(selectionModelRect(), block)) ed.paintOverlayLayer();
      return scrollMarkerIntoViewX(ed.pagesEl.querySelector(".overlay .caret"));
    }
    scrollOverlayIntoView(marker, block);
  }

  /** Bring the current review selection's OWN on-canvas marker just into view when
   *  focusing a comment/change from the sidebar or Next/Previous. A review target
   *  is a range, so it paints a highlight (not a caret) — scrolling only `.caret`
   *  did nothing, leaving an off-screen item unreachable. And a "center" scroll
   *  overshot: for a clustered paragraph it recentred on the whole selection and
   *  pushed the very item the reviewer picked out of the viewport. "nearest" fixes
   *  both — an already-visible marker never moves (no overshoot), and an off-screen
   *  one is revealed by the minimum scroll to its own rect, not the paragraph's. */
  function scrollReviewSelectionIntoView() {
    // Prefer the ACTIVE review item's own marker: in a clustered paragraph several
    // items paint highlights, and `.highlight` in DOM order can belong to an
    // earlier item — scrolling to it lands on the paragraph top, not the item the
    // reviewer selected. The active marker (painted from the item resolved in
    // `syncActiveReviewCommentToCaret`) is unambiguous; fall back to the selection
    // highlight, then the caret.
    const marker =
      ed.pagesEl.querySelector(".overlay .review-comment-marker-active, .overlay .review-revision-marker-active")
      || ed.pagesEl.querySelector(".overlay .highlight")
      || ed.pagesEl.querySelector(".overlay .caret");
    if (!marker || ed.pageBandModel?.scale > 1) {
      if (scrollModelRectIntoView(selectionModelRect(), "nearest")) ed.paintOverlayLayer();
      return;
    }
    scrollOverlayIntoView(marker, "nearest");
  }

  /** Find selects a real range, so paintSelection deliberately emits highlights
   * and no caret. Scroll its first rectangle into view; querying only `.caret`
   * made Previous/Next update the selection on an off-screen page without moving
   * the canvas. */
  function scrollFindMatchIntoView() {
    const marker = ed.pagesEl.querySelector(".overlay .highlight");
    // A match found on a page the reader is nowhere near has no highlight to
    // scroll to until its page exists. This is the guard `find-far-page` covers:
    // Find used to update the selection on an off-screen page and leave the
    // canvas exactly where it was.
    if (!marker || ed.pageBandModel?.scale > 1) {
      if (scrollModelRectIntoView(selectionModelRect(), "center")) ed.paintOverlayLayer();
      return scrollMarkerIntoViewX(ed.pagesEl.querySelector(".overlay .highlight"));
    }
    scrollOverlayIntoView(marker, "center");
  }

  return {
    repaintPage, scrollCaretIntoView, scrollFindMatchIntoView, scrollModelRectIntoView,
    scrollOverlayIntoView, scrollReviewSelectionIntoView, selectionModelRect,
  };
}
