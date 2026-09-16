// Keyboard navigation for the ribbon band, following the WAI-ARIA toolbar
// pattern: the whole band is ONE Tab stop and the arrow keys move between the
// controls inside it. Before this the Home tab alone cost roughly forty-five Tab
// presses to cross, which meant a keyboard user reaching the document body had
// to walk every formatting button on the way — the ribbon was a wall, not a
// toolbar.
//
// The index arithmetic lives here, apart from the DOM, so it can be tested
// directly; `main.js` owns collecting the controls and moving focus.

/** The keys a toolbar owns, and where each one lands.
 *
 * Wraps at both ends, matching the tab strip and the styles gallery already in
 * this editor. `index` is -1 when focus is inside the band but not on a
 * collected item (a control that was just removed by overflow, say), and the
 * arrows then behave as if entering from the corresponding edge.
 *
 * Returns `null` for every key the toolbar does not own, so the caller leaves
 * the event alone rather than swallowing it.
 */
export function rovingIndex(key, index, count) {
  if (!Number.isInteger(count) || count <= 0) return null;
  switch (key) {
    case "ArrowRight":
      return index < 0 ? 0 : (index + 1) % count;
    case "ArrowLeft":
      return index < 0 ? count - 1 : (index - 1 + count) % count;
    case "Home":
      return 0;
    case "End":
      return count - 1;
    default:
      return null;
  }
}

/** Which collected item should hold `tabindex="0"` on the next sync.
 *
 * The band remembers where focus last sat, but the remembered control can
 * disappear between syncs — undo/redo disable themselves, and overflow moves
 * whole groups into the "⋯" menu. When the remembered item is gone the stop
 * falls back to the first item rather than leaving the band with no Tab stop at
 * all, which is the failure that makes a toolbar unreachable.
 */
export function tabStopIndex(items, remembered) {
  if (!items.length) return -1;
  const index = items.indexOf(remembered);
  return index < 0 ? 0 : index;
}
