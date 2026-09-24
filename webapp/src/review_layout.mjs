// Where each comment/suggestion card sits, as arithmetic.
//
// Two shapes, one function, because the product has two of them:
//
//  * MARGIN — the desktop column. Every card is pinned to its own anchor's Y in
//    document-scroll coordinates and destacked so neighbours never overlap. The
//    selected card keeps true anchor alignment and the rest flow outward from
//    it (the Google Docs "stacked chips" behaviour, REVIEW-GAP-019); a plain
//    top-down pass could only push the selected card DOWN past its marker.
//  * SHEET — the narrow-width bottom sheet (HF-088). There is no margin to
//    anchor to: the sheet is a list, so cards stack in document order from the
//    top of the sheet and the sheet scrolls them.
//
// It is pure so both shapes are unit-testable without a browser, and so the
// sheet's stacking is provable without measuring pixels — the rest of the
// review layer (measure, mount, virtualize) is unchanged by which shape is in
// force, which is what keeps the sheet from being a second implementation.

/** Vertical space between two cards, in CSS pixels. */
export const REVIEW_CARD_GAP = 8;

/** The margin comment affordance's box, in CSS pixels. Duplicated in
 *  `style.css` as `.review-margin-add`, and `review_layout.test.mjs` reads both
 *  so the two cannot drift: the placement arithmetic below decides whether the
 *  button fits in the margin, and it decides that from this number. */
export const COMMENT_AFFORDANCE_SIZE = 32;

/** Space between the page's right edge and the affordance, and the least space
 *  left over on its far side. */
export const COMMENT_AFFORDANCE_GAP = 12;

/**
 * Where the right-margin "add a comment" button goes — or `null` when it has no
 * business being there.
 *
 * Google Docs' margin button: beside the page, on the line the caret is in,
 * vertically centred against that line. `rect` is the caret's line, or a
 * selection's FIRST line (what `selectionRects` answers with first), so a
 * selection spanning pages still puts the button beside the sentence the reader
 * began with.
 *
 * The one refusal worth having is the narrow one. `HF-088` is on record for what
 * happens when review chrome is put in a margin that is not there, and a button
 * floating over the text it points at is worse than no button. So the margin is
 * measured, and if it cannot hold the button with a gap on both sides there is
 * no position; the rail's Comments toggle and the Review band are still the
 * durable entry points.
 *
 * This measurement is the ONLY gate, and that is deliberate. Measured on the
 * rich fixture (794px sheet, 55px rail), the right margin is -458px at a 390px
 * window, -148px at 700 — where the column becomes a bottom sheet — 26px at
 * 900, and 56px at 960, which is the first width that clears `gap + size + gap`.
 * So every width the bottom sheet covers is a width this already refuses: a
 * `reviewSheetMode()` check beside it would be a condition that can never be
 * the reason, which is a guard no test can drive red. One was written, and the
 * spec meant to prove it passed whether it was there or not.
 *
 * Coordinates come back in BAND coordinates (see `mountReviewWindow`): the
 * caller adds the live `bandOffset` back when it applies them, so a compressed
 * document scrolls the button with its text instead of away from it.
 *
 * @param {object}  options
 * @param {?object} options.rect          selection's first line, client coords,
 *                                        with `top`, `bottom` and `pageRight`
 * @param {number}  options.viewportLeft  viewport's client left edge
 * @param {number}  options.viewportTop   viewport's client top edge
 * @param {number}  options.viewportWidth viewport's `clientWidth` (no scrollbar)
 * @param {number}  [options.scrollLeft]
 * @param {number}  [options.scrollTop]
 * @param {number}  [options.bandOffset]
 * @param {number}  [options.size]
 * @param {number}  [options.gap]
 * @returns {?{left: number, top: number}}
 */
export function commentAffordanceSpot({
  rect,
  viewportLeft,
  viewportTop,
  viewportWidth,
  scrollLeft = 0,
  scrollTop = 0,
  bandOffset = 0,
  size = COMMENT_AFFORDANCE_SIZE,
  gap = COMMENT_AFFORDANCE_GAP,
}) {
  if (!rect) return null;
  const margin = viewportLeft + viewportWidth - rect.pageRight;
  if (margin < gap + size + gap) return null;
  return {
    left: Math.round(rect.pageRight - viewportLeft + scrollLeft + gap),
    top: Math.round(
      rect.top + (rect.bottom - rect.top - size) / 2 - viewportTop + scrollTop - bandOffset,
    ),
  };
}

/**
 * Card tops, in the scroll coordinates of whichever container owns the scroll.
 *
 * @param {object} options
 * @param {number[]} options.anchorY   each card's natural anchor Y (margin only)
 * @param {number[]} options.heights   each card's measured height
 * @param {number}   [options.activeIndex] index of the expanded card, or -1
 * @param {number}   [options.gap]     space between cards
 * @param {boolean}  [options.sheet]   stack as a list instead of by anchor
 * @returns {number[]} one top per card, in the same order
 */
export function stackReviewCards({
  anchorY,
  heights,
  activeIndex = -1,
  gap = REVIEW_CARD_GAP,
  sheet = false,
}) {
  const count = heights.length;
  const tops = new Array(count);

  // Sheet: a plain list. Anchors are deliberately ignored — a card pinned to a
  // document Y inside a 50vh sheet would sit thousands of pixels below its own
  // scroll container, which is the bug the sheet exists to end.
  if (sheet) {
    let y = gap;
    for (let i = 0; i < count; i++) {
      tops[i] = y;
      y += heights[i] + gap;
    }
    return tops;
  }

  if (activeIndex >= 0 && activeIndex < count) {
    tops[activeIndex] = Math.max(gap, anchorY[activeIndex]);
    for (let i = activeIndex + 1; i < count; i++) {
      tops[i] = Math.max(anchorY[i], tops[i - 1] + heights[i - 1] + gap);
    }
    for (let i = activeIndex - 1; i >= 0; i--) {
      tops[i] = Math.min(anchorY[i], tops[i + 1] - heights[i] - gap);
    }
    // Only if the cards above are collectively too tall to fit above the
    // selected card's anchor (a cluster crowding the document top) does the
    // whole stack shift down — the selected card yields its exact anchor solely
    // when the geometry leaves no alternative.
    const overflow = gap - tops[0];
    if (overflow > 0) for (let i = 0; i < count; i++) tops[i] += overflow;
    return tops;
  }

  // No selection: stack top-down (anchor top, pushed down to clear the card
  // above).
  let nextY = gap;
  for (let i = 0; i < count; i++) {
    tops[i] = Math.max(gap, anchorY[i], nextY);
    nextY = tops[i] + heights[i] + gap;
  }
  return tops;
}

/** Total height the card body must reserve for a stack. */
export function reviewStackHeight(tops, heights, gap = REVIEW_CARD_GAP) {
  let bottom = 0;
  for (let i = 0; i < tops.length; i++) {
    bottom = Math.max(bottom, tops[i] + heights[i]);
  }
  return tops.length ? bottom + gap : 0;
}

// ---- Card reuse -------------------------------------------------------------
// The other half of the same virtualization mechanism: `stackReviewCards` says
// where a card goes, and the signature below says whether the card already in
// the cache can be reused there without rebuilding and re-measuring it.

/** Whether `comment` is a threaded reply to `parent`. A reply carries a
 *  non-null `parentParaId` that joins to the parent's `paraId` (the DOCX
 *  `w15:paraIdParent` → `w14:paraId` link) or, as a fallback, to the parent's
 *  comment id. A thread root has a null/absent `parentParaId` and is a reply to
 *  nothing. The `parentParaId != null` guard is load-bearing: `listComments`
 *  projects a comment with no join key as `paraId: null` / `parentParaId: null`
 *  (e.g. imported comments with no `commentsExtended`/`w14:paraId`), and without
 *  the guard `null === null` would count every top-level comment as a reply to
 *  every other — an O(n²) reply-DOM (and signature-string) blowup that makes a
 *  comment-heavy document consume gigabytes of memory. */
export function reviewCommentIsReplyTo(comment, parent) {
  const parentKey = comment?.parentParaId;
  if (parentKey == null) return false;
  return parentKey === parent.paraId || parentKey === parent.id;
}

/** A stable string that changes whenever anything affecting a card's rendered
 *  DOM or measured height changes, so the card cache reuses a card only when it
 *  would render identically. The composer is never cached (always rebuilt so
 *  its live textarea/focus stays correct).
 *
 *  `activeItemId` and `deleteConfirmId` are passed in rather than read from
 *  module state: they are the two pieces of UI state that change a card's
 *  height without changing its content. */
export function reviewCardSignature(item, comments, { activeItemId, deleteConfirmId } = {}) {
  if (item.type === "composer") return "composer";
  const d = item.data;
  const itemId = `${item.type}:${d.id}`;
  const expanded = activeItemId === itemId;
  const confirm = deleteConfirmId === d.id;
  const replies = item.type === "comment"
    ? comments
      .filter((c) => reviewCommentIsReplyTo(c, d))
      .map((r) => `${r.id}${r.resolved ? 1 : 0}${r.text}${r.author}${r.date}`)
      .join("")
    : "";
  return JSON.stringify([
    item.type, d.id, d.kind || "", expanded ? 1 : 0, d.resolved ? 1 : 0, confirm ? 1 : 0,
    d.text || "", d.oldText || "", d.newText || "", d.author || "", d.initials || "", d.date || "",
    d.groupId || "", d.movePair ? 1 : 0,
    Array.isArray(d.formattingDelta) ? d.formattingDelta : 0,
    d.anchor ? `${d.anchor.node}:${d.anchor.start}:${d.anchor.end}` : "",
    replies,
  ]);
}
