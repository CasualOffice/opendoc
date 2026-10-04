/** Which ribbon groups leave the band when the band is too narrow.
 *
 *  Extracted from `main.js`'s `updateRibbonOverflow`, which had to measure the
 *  DOM and decide at the same time — so the DECISION, which is arithmetic over
 *  widths, could not be tested without a laid-out ribbon. The DOM half stays
 *  with the DOM; this half is a function of numbers.
 *
 *  The rule, unchanged: relocate unpinned groups from right to left until the
 *  inline set fits beside the overflow button, and only if that is still not
 *  enough, start moving pinned groups too — preserving Clipboard, which is the
 *  one group that never leaves. `docs/63` and the ribbon's width budget are the
 *  reason the order is right-to-left rather than by size: the band's leftmost
 *  groups are the ones a reader reaches for most.
 */

/** Groups to move to the overflow menu, in the order they should be moved.
 *
 *  O(groups), and `groups` is the active panel's own list — tens, not a
 *  function of document size.
 *
 *  @param {object} o
 *  @param {Array<T>} o.groups           the panel's groups, in canonical left-to-right order
 *  @param {(g: T) => number} o.widthOf  each group's measured width
 *  @param {number} o.avail              the panel's usable width
 *  @param {number} o.reserve            room kept for the overflow button
 *  @param {(g: T) => boolean} o.isPinned
 *  @param {(g: T) => boolean} o.isLastResort  the group that must never move (Clipboard)
 *  @returns {Array<T>}
 *  @template T
 */
export function groupsToOverflow({ groups, widthOf, avail, reserve, isPinned, isLastResort }) {
  const fits = (width) => width <= avail - reserve;
  let width = groups.reduce((sum, g) => sum + widthOf(g), 0);
  const moved = [];
  const take = (skip) => {
    for (let i = groups.length - 1; i >= 0 && !fits(width); i -= 1) {
      const group = groups[i];
      if (skip(group)) continue;
      moved.push(group);
      width -= widthOf(group);
    }
  };
  take((g) => isPinned(g));
  // Last resort: a pinned composition that still cannot fit. Clipboard stays.
  take((g) => moved.includes(g) || isLastResort(g));
  return moved;
}
