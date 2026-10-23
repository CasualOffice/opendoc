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

/** The scroll left to move to, or `null` when the marker is already in view
 *  horizontally (`docs/174` TA-10).
 *
 *  Zoomed past the window's width, the page scrolls sideways, and a caret moved
 *  to the end of a line has to be followed that way too — Word and Docs both
 *  do; this rule used to exist only for `scrollTop`. Always "nearest": the
 *  minimum sideways move, never a recentre, so the reading position is kept.
 *  The horizontal axis is never compressed (`scale` is the vertical band's), so
 *  a screen delta is a scroll delta.
 *
 *  A marker wider than the viewport aligns its START edge; otherwise the edge
 *  it is past is brought in, with `margin`.
 *
 *  O(1).
 *
 *  @param {object} o
 *  @param {{left: number, right: number}} o.marker    screen rect
 *  @param {{left: number, right: number}} o.viewport  visible screen box, scrollbar excluded
 *  @param {number} o.current  the scroller's current `scrollLeft`
 *  @param {number} o.max      the largest valid `scrollLeft`
 *  @param {number} o.margin   breathing room when nudging an edge into view
 *  @returns {number|null}
 */
export function scrollLeftFor({ marker, viewport, current, max, margin }) {
  let delta;
  if (marker.left < viewport.left) {
    delta = marker.left - viewport.left - margin;
  } else if (marker.right > viewport.right) {
    const tooWide = marker.right - marker.left + 2 * margin > viewport.right - viewport.left;
    if (tooWide && marker.left <= viewport.right) return null; // its start is already shown
    delta = tooWide ? marker.left - viewport.left - margin : marker.right - viewport.right + margin;
  } else {
    return null;
  }
  const target = Math.max(0, Math.min(max, current + delta));
  return target === current ? null : target;
}
