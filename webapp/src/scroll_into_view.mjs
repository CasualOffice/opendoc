/** Where to scroll so an overlay marker is in view.
 *
 *  Extracted from `main.js`, where the arithmetic sat between two
 *  `getBoundingClientRect()` calls and a `scrollTo`, so the one subtle rule in
 *  it could not be asserted: a pixel of scroll is not a pixel of content once
 *  the document is compressed onto a bounded scroll range, it is `scale` of
 *  them. A delta measured on screen must be divided by that before it becomes a
 *  scroll position — and above a scale of 2 an undivided delta OSCILLATES
 *  rather than converging, which is a bug you cannot see in a unit test that
 *  does not exist.
 */

/** The scroll top to move to, or `null` when the marker is already in view.
 *
 *  O(1).
 *
 *  @param {object} o
 *  @param {{top: number, bottom: number, height: number}} o.marker   screen rect
 *  @param {{top: number, bottom: number, height: number}} o.viewport screen rect
 *  @param {number} o.current  the scroller's current `scrollTop`
 *  @param {number} o.max      the largest valid `scrollTop`
 *  @param {number} o.scale    content pixels per scroll pixel (>= 1)
 *  @param {"nearest"|"center"} o.block
 *  @param {number} o.margin   breathing room when nudging an edge into view
 *  @returns {number|null}
 */
export function scrollTargetFor({ marker, viewport, current, max, scale, block, margin }) {
  let delta;
  if (block === "center") {
    delta = marker.top + marker.height / 2 - (viewport.top + viewport.height / 2);
  } else if (marker.top < viewport.top) {
    delta = marker.top - viewport.top - margin;
  } else if (marker.bottom > viewport.bottom) {
    delta = marker.bottom - viewport.bottom + margin;
  } else {
    return null;
  }
  const perScrollPx = scale > 1 ? scale : 1;
  return Math.max(0, Math.min(max, current + delta / perScrollPx));
}

/**
 * The same three-way decision for the MODEL path, in document space.
 *
 * `scrollTargetFor` above answers for an overlay marker, from screen rects. Its
 * twin answers for a rectangle the ENGINE reported — a find match twenty
 * thousand pages away, a comment anchor, a caret after a jump — none of which
 * has an overlay element until its page is materialized, and whose page is not
 * materialized until something scrolls there.
 *
 * Here rather than in `main.js` so the two copies of one rule sit beside each
 * other: they are the same "is it above, below, or already in view" question in
 * two coordinate systems, and a rule stated twice in two files is a rule that
 * drifts. The caller still owns the band arithmetic and the `scrollTo`, which is
 * the half that needs a DOM.
 *
 * O(1).
 *
 * @param {object} o
 * @param {number} o.top            the rect's top, in document space
 * @param {number} o.bottom         the rect's bottom, in document space
 * @param {number} o.docY           the document-space offset currently on screen
 * @param {number} o.viewportHeight
 * @param {"nearest"|"center"} o.block
 * @param {number} o.margin         breathing room when nudging an edge into view
 * @returns {number|null} the document-space offset to show, or `null` when the
 *   rect is already in view and nothing should move.
 */
export function docScrollTargetFor({ top, bottom, docY, viewportHeight, block, margin }) {
  if (block === "center") return top + (bottom - top) / 2 - viewportHeight / 2;
  if (top < docY) return top - margin;
  if (bottom > docY + viewportHeight) return bottom - viewportHeight + margin;
  return null;
}
