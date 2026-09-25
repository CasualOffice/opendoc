/** Returns the visible element a popover should measure. */
export function popoverAnchor(requested, fallback) {
  return requested?.getClientRects().length ? requested : fallback;
}

/** Places a fixed popover below its anchor, or above when the viewport requires it. */
export function popoverPosition(anchor, menu, viewport, gutter = 8, gap = 4) {
  const left = Math.min(
    Math.max(gutter, anchor.left),
    Math.max(gutter, viewport.width - menu.width - gutter),
  );
  const below = anchor.bottom + gap;
  const above = anchor.top - menu.height - gap;
  const top =
    below + menu.height <= viewport.height - gutter
      ? below
      : Math.max(gutter, above);
  return { left: Math.round(left), top: Math.round(top) };
}
