/** Which tracked-change card the caret is inside.
 *
 *  Extracted from `main.js`'s `syncActiveReviewCommentToCaret`, where the rule
 *  — "the smallest containing range wins when several stack" (REVIEW-GAP-019) —
 *  sat in a loop over module state and could not be asserted at all. Several
 *  ranges stacking on one caret is exactly the case that is hard to set up
 *  through the chrome and trivial to state here.
 */

/** The smallest range in `index` that contains `offset` within `node`, or null.
 *
 *  O(index). The index is the review anchors of the rendered window, not of the
 *  document.
 *
 *  @param {Array<{node: string, start: number, end: number, itemId: string}>} index
 *  @param {string} node
 *  @param {number} offset
 */
export function smallestContaining(index, node, offset) {
  let best = null;
  for (const entry of index) {
    if (entry.node !== node) continue;
    if (offset < entry.start || offset > entry.end) continue;
    if (!best || entry.end - entry.start < best.end - best.start) best = entry;
  }
  return best;
}
