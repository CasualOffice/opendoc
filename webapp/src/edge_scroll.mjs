// How far a drag held near the edge of the scrolling viewport scrolls it, per
// animation frame.
//
// The rule every editor uses for a drag that runs out of window: hold the
// pointer within a band at an edge and the view scrolls towards it, faster the
// deeper into the band. It was written inline in `main.js`'s drag-selection
// loop, which meant the next drag that needed it — moving an in-line picture to
// a place further down the document (`docs/109` UX-OB-02) — would have been a
// second copy of the arithmetic. One rule, two gestures.
//
// Pure: a rectangle and a point in, a step out. O(1).

/** The band, in CSS pixels, inside each edge of the viewport where a held drag
 *  scrolls. */
export const EDGE_SCROLL_BAND_PX = 56;

/** The largest step one frame scrolls, in CSS pixels, reached at the very edge. */
export const EDGE_SCROLL_MAX_PX = 24;

/**
 * The scroll step for a pointer at `(x, y)` over a viewport whose client box is
 * `rect`: negative towards the top or left, positive towards the bottom or
 * right, zero outside the bands. Both axes, because at a zoom where the sheet is
 * wider than the window a drag that only scrolled vertically stopped at the
 * window's side and the end of the line was out of reach.
 *
 * @param {{top: number, bottom: number, left: number, right: number}} rect
 * @param {number} x pointer client x
 * @param {number} y pointer client y
 * @returns {{dx: number, dy: number}}
 */
export function edgeScrollStep(rect, x, y) {
  const along = (value, low, high) => {
    if (value < low + EDGE_SCROLL_BAND_PX) {
      const ratio = Math.min(1, (low + EDGE_SCROLL_BAND_PX - value) / EDGE_SCROLL_BAND_PX);
      return -Math.ceil(ratio * EDGE_SCROLL_MAX_PX);
    }
    if (value > high - EDGE_SCROLL_BAND_PX) {
      const ratio = Math.min(1, (value - (high - EDGE_SCROLL_BAND_PX)) / EDGE_SCROLL_BAND_PX);
      return Math.ceil(ratio * EDGE_SCROLL_MAX_PX);
    }
    return 0;
  };
  return { dx: along(x, rect.left, rect.right), dy: along(y, rect.top, rect.bottom) };
}
