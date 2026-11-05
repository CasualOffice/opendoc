// ZOOM ANCHORING (audit CANVAS-08, CANVAS-V-02).
//
// A zoom used to re-lay the band out and leave the scroll offsets where they
// were, so the document grew away from the top-left corner of the view:
// measured, the caret on line 2 of page 1 sat at y=456 at 100% and at y=969 at
// 300%, below the 770px viewport, and Ctrl+wheel at (800,600) zoomed around the
// top centre rather than the pointer. Word and ONLYOFFICE keep the point the
// reader is looking at where it was: the caret for the buttons, the keys and
// the menu, the pointer for Ctrl+wheel and the trackpad pinch.
//
// The pattern is the map viewer's: capture the DOCUMENT point under the anchor
// (page, twips) and its CLIENT position before the relayout; after it, scroll
// by however far that document point moved. Both halves are pure here, so the
// arithmetic has a node test; `editor_zoom.mjs` feeds them the live geometry.
//
// Complexity: O(1) — one page rectangle before and one after.

/**
 * The document point under `client` on a page whose sheet occupies `rect` at
 * scale `sx`/`sy` CSS px per twip.
 *
 * @returns {{pageIndex:number, tx:number, ty:number, clientX:number, clientY:number}}
 */
export function captureAnchor(pageIndex, rect, sx, sy, client) {
  return {
    pageIndex,
    tx: (client.x - rect.left) / sx,
    ty: (client.y - rect.top) / sy,
    clientX: client.x,
    clientY: client.y,
  };
}

/**
 * How far to scroll so the anchored document point is back at its client
 * position, given where its page now sits.
 *
 * @returns {{dx:number, dy:number}} add to scrollLeft / scrollTop
 */
export function anchorScrollDelta(anchor, rect, sx, sy) {
  const nowX = rect.left + anchor.tx * sx;
  const nowY = rect.top + anchor.ty * sy;
  return { dx: nowX - anchor.clientX, dy: nowY - anchor.clientY };
}

/**
 * Which point a zoom keeps still: the pointer when the zoom came from the
 * wheel or a pinch; else the caret when it is in view; else the middle of the
 * view (so a zoom while reading elsewhere keeps what is being read).
 *
 * @param {{x:number,y:number}|null} pointer
 * @param {{x:number,y:number}|null} caret the caret's client point, if any
 * @param {{left:number,top:number,right:number,bottom:number}} view
 */
export function anchorPoint(pointer, caret, view) {
  if (pointer) return { x: pointer.x, y: pointer.y, source: "pointer" };
  if (
    caret &&
    caret.x >= view.left &&
    caret.x <= view.right &&
    caret.y >= view.top &&
    caret.y <= view.bottom
  ) {
    return { x: caret.x, y: caret.y, source: "caret" };
  }
  return { x: (view.left + view.right) / 2, y: (view.top + view.bottom) / 2, source: "view" };
}
