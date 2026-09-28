// Drawing a shape out on the page — the gesture, not the chrome.
//
// COMPETITIVE STANDARD. In Word you pick a preset from Insert ▸ Shapes, the
// pointer becomes a crosshair, and you DRAG on the page to draw the shape at
// the size you want; a bare click drops it at a default size. PowerPoint,
// Keynote, Figma and Google Drawings all behave the same way. The gallery pick
// is the choice of preset; the drag is the choice of geometry. Shipping only
// "click a row and a 2x1 inch rectangle appears somewhere near the caret" is
// the same failure the crop dialog was: a numeric default where the competitor
// has a direct-manipulation gesture.
//
// Shift constrains the drag to a square, which is Word's modifier and the only
// one it has for this gesture. Alt (draw from the centre) is deliberately NOT
// here: Word does not offer it for shapes in a document body, and inventing a
// modifier nobody expects is worse than omitting one.
//
// Everything in this file is arithmetic on a pair of points, so it is tested in
// node rather than through a browser: the interesting questions — does dragging
// up-and-left still produce a positive rectangle, is a two-pixel wobble a click
// or a drag, does the rectangle stay on the page — are not browser questions.
//
// Cost: O(1) per pointermove. The gesture reads NOTHING from the document while
// it runs; the page's own size is passed in once when the drag starts.

/** Below this, a press-and-release is a CLICK, not a drag. 40 twips is about
 *  2px at 100% — a hand tremor, not an intent. Word's own threshold is of this
 *  order; what matters is that a shaky click still yields the default size
 *  rather than a 3-twip sliver the user then has to find and delete. */
export const DRAW_CLICK_THRESHOLD_TWIPS = 40;

/** The size a bare click gets: 2" x 1", which is what `insertShape` authors.
 *  Stated here as well so the click path and the drag path agree about what
 *  "default" means without one of them asking the engine mid-gesture. */
export const DEFAULT_SHAPE_TWIPS = Object.freeze({ width: 2880, height: 1440 });

/** The smallest shape a drag may produce, in twips. Under this the object is
 *  unselectable with a pointer, which turns a mis-drag into litter that cannot
 *  be picked up again. 144 twips is 0.1". */
export const MIN_SHAPE_TWIPS = 144;

/**
 * The rectangle a drag from `start` to `current` describes, in page-local
 * twips, normalised so a drag in any of the four directions is positive.
 *
 * `square` (Shift) takes the larger of the two extents on both axes and keeps
 * the corner the drag started from fixed, which is what Word does — the
 * rectangle grows towards the pointer rather than jumping under it.
 *
 * O(1).
 *
 * @param {{x: number, y: number}} start
 * @param {{x: number, y: number}} current
 * @param {{square?: boolean}} [options]
 */
export function drawnRect(start, current, options = {}) {
  let dx = current.x - start.x;
  let dy = current.y - start.y;
  if (options.square) {
    const side = Math.max(Math.abs(dx), Math.abs(dy));
    dx = Math.sign(dx || 1) * side;
    dy = Math.sign(dy || 1) * side;
  }
  return {
    left: Math.min(start.x, start.x + dx),
    top: Math.min(start.y, start.y + dy),
    width: Math.abs(dx),
    height: Math.abs(dy),
  };
}

/**
 * Whether a finished gesture was a click rather than a drag — which is what
 * decides between "the size you drew" and "the default size here".
 *
 * O(1).
 */
export function isClickNotDrag(rect) {
  return rect.width < DRAW_CLICK_THRESHOLD_TWIPS && rect.height < DRAW_CLICK_THRESHOLD_TWIPS;
}

/**
 * The rectangle a gesture commits: the drawn one, or the default size dropped
 * with its top-left at the press point when the gesture was a click.
 *
 * A drawn rectangle is floored at `MIN_SHAPE_TWIPS` on each axis rather than
 * refused: someone who dragged 50 twips meant to draw something, and refusing
 * silently is the worst of the three options.
 *
 * O(1).
 */
export function committedRect(rect) {
  if (isClickNotDrag(rect)) {
    return {
      left: rect.left,
      top: rect.top,
      width: DEFAULT_SHAPE_TWIPS.width,
      height: DEFAULT_SHAPE_TWIPS.height,
    };
  }
  return {
    left: rect.left,
    top: rect.top,
    width: Math.max(rect.width, MIN_SHAPE_TWIPS),
    height: Math.max(rect.height, MIN_SHAPE_TWIPS),
  };
}

/**
 * Keeps a committed rectangle inside the page it was drawn on, in page-local
 * twips. A shape hanging off the sheet is placeable in the model and invisible
 * to the reader, so the gesture that would produce one slides it back instead.
 *
 * O(1).
 *
 * @param {{left:number, top:number, width:number, height:number}} rect
 * @param {{width:number, height:number}} pageTwips
 */
export function clampToPage(rect, pageTwips) {
  const width = Math.min(rect.width, pageTwips.width);
  const height = Math.min(rect.height, pageTwips.height);
  return {
    left: Math.max(0, Math.min(rect.left, pageTwips.width - width)),
    top: Math.max(0, Math.min(rect.top, pageTwips.height - height)),
    width,
    height,
  };
}
