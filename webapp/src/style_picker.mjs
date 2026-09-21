// The Styles control's decisions, separated from its DOM.
//
// `docs/115`. Which styles the band offers is a rule with four inputs and a cap,
// and it was answered inline in main.js where the only way to check it was to
// open a browser and count what appeared. It is the rule the owner has now
// reported on four times; it belongs where a test can ask it directly.

/** What the band MAY offer, in priority order: Word's Quick Styles set intersected
 *  with the six names Google Docs offers. Only the ones a document really defines
 *  are offered, and only up to `OFFERED_STYLE_COUNT` of them. */
export const RECOMMENDED_STYLES = [
  "Normal",
  "Body Text",
  "Title",
  "Subtitle",
  "Heading 1",
  "Heading 2",
  "Heading 3",
  "Heading 4",
  "Quote",
  "Intense Quote",
  "List Paragraph",
  "List",
  "Caption",
  "Strong",
  "Emphasis",
];

/** How many styles the band may offer at once.
 *
 *  SIX: exactly what Google Docs offers, and the bottom of the range Word's
 *  gallery shows at 1280px. The 3 that shipped before was chosen against a width
 *  budget rather than against a competitor, and the 14-entry dropdown beside it
 *  was never a decision at all. Raising this re-admits the long list to the band.
 */
export const OFFERED_STYLE_COUNT = 6;

/**
 * The short list the control offers, in menu order.
 *
 * Recommended styles the document defines, then styles it is known to be using,
 * capped at `cap`. The style at the caret is guaranteed a slot even when the cap
 * is full — it displaces the last entry rather than falling off, because a
 * control that cannot show you the style you are in is worse than one that
 * offers one style fewer. Word reaches the same guarantee by scrolling its
 * gallery to the applied style.
 *
 * @param {object} input
 * @param {string[]} input.recommended  Recommended names the document defines,
 *   already in priority order.
 * @param {Iterable<string>} input.inUse  Styles the chrome has seen in use.
 * @param {Set<string>} input.defined  Every style the document defines.
 * @param {string} input.active  The style at the caret, or "".
 * @param {number} [input.cap]
 * @returns {string[]}
 */
export function offeredStyleNames({
  recommended,
  inUse,
  defined,
  active,
  cap = OFFERED_STYLE_COUNT,
}) {
  const offered = [...recommended];
  for (const name of inUse) {
    if (name && defined.has(name) && !offered.includes(name))
      offered.push(name);
  }
  const capped = offered.slice(0, cap);
  if (active && defined.has(active) && !capped.includes(active)) {
    if (capped.length >= cap) capped.pop();
    capped.push(active);
  }
  return capped;
}

/** A style name as a CSS class suffix. */
export function styleSlug(name) {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/(^-|-$)/g, "");
}

/**
 * The px size a menu row should render a style's point size at.
 *
 * Clamped, because the list has to stay a list: a 28pt Title drawn at 28pt is a
 * heading with a menu around it, and a 6pt Caption drawn at 6pt is unreadable.
 * The range is wide enough that Title, Heading 1 and Normal are visibly three
 * different things, which is the only reason the rows are drawn in their styles
 * at all.
 */
export function previewPx(sizePoints) {
  if (!(sizePoints > 0)) return null;
  return Math.max(12, Math.min(22, Math.round(sizePoints * 0.95)));
}
