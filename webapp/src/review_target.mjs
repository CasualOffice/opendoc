// Which review item the caret is currently on.
//
// Extracted from `main.js` (`109` HF-085): it is a pure predicate over a
// selection and a list of ranges, with more reasoning in it than code, and in a
// module that reasoning can be tested directly instead of only through Next and
// Previous in a browser.

export function reviewCurrentTargetIndex(targets, selection) {
  const focus = selection?.focus;
  const anchor = selection?.anchor;
  if (!focus) return -1;
  for (let i = 0; i < targets.length; i++) {
    const r = targets[i].range;
    if (anchor && r.startNode === anchor.node && r.endNode === focus.node) {
      const forward = r.startOffset === anchor.offset && r.endOffset === focus.offset;
      const backward = r.startOffset === focus.offset && r.endOffset === anchor.offset;
      if (forward || backward) return i;
    }
  }
  const collapsed = !anchor
    || (anchor.node === focus.node && anchor.offset === focus.offset);
  for (let i = 0; i < targets.length; i++) {
    const r = targets[i].range;
    if (r.startNode === focus.node && r.endNode === focus.node
      && focus.offset >= r.startOffset && focus.offset <= r.endOffset) {
      // A collapsed caret resting exactly on a non-empty item's START boundary
      // is at the threshold *before* the item, not inside it — the reviewer has
      // not visited it yet. Report "not on any item" so Next steps onto the item
      // (not past it) and Previous walks to the one before it. A caret at the END
      // boundary, or strictly inside, is genuinely on the item, so Next advances
      // past it — which also preserves stepping between two items that share a
      // boundary (a change starting exactly where a comment ends). Without this,
      // a fresh caret at document start (offset 0, the leading edge of a comment
      // anchored there) was treated as already-on the comment, so the first Next
      // skipped straight to the next item.
      if (collapsed && focus.offset === r.startOffset && r.startOffset !== r.endOffset) {
        continue;
      }
      return i;
    }
  }
  return -1;
}
