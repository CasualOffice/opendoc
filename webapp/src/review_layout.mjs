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
