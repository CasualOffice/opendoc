// The Styles control's decisions, separated from its DOM.
//
// `docs/115`. Which styles the band offers is a rule with four inputs and a cap,
// and it was answered inline in main.js where the only way to check it was to
// open a browser and count what appeared. It is the rule the owner has now
// reported on four times; it belongs where a test can ask it directly.

/** What the band PREFERS to offer, in priority order: the paragraph styles
 *  Word's default template marks as Quick Styles, intersected with Google
 *  Docs' six.
 *
 *  Paragraph styles only. `Strong`, `Emphasis`, `Subtle Reference` and the
 *  rest of Word's quick set are CHARACTER styles; `listStyles()` returns
 *  paragraph styles alone, so naming them here was dead weight that could
 *  never match. `No Spacing` was missing outright — `docs/115` §2 listed it
 *  and the code did not.
 *
 *  This is a preference, not the whole list: see `offeredStyleNames`. */
export const RECOMMENDED_STYLES = [
  "Normal",
  "No Spacing",
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
];

/** Styles that exist to support the document's machinery rather than to be
 *  applied to body text: running content, comment/annotation scaffolding,
 *  revision bookkeeping, table scaffolding.
 *
 *  Ranked LAST rather than excluded. Excluding them would leave a document
 *  whose only other styles are these with an empty control — which is the
 *  failure this whole rule exists to prevent — and Word does reach them from
 *  the Styles pane, so they are not forbidden, merely unhelpful first. */
const UTILITY_STYLE_PATTERN =
  /^(header|footer|annotation|comment|endnote|footnote|revision|table |toc |index |caption$|line number|page number|placeholder|no list|default paragraph font|bibliograph|macro)/i;

/** How many styles the SUGGESTED group offers before the full list.
 *
 *  SIX: exactly what Google Docs offers, and the bottom of the range Word's
 *  gallery shows at 1280px. The 3 that shipped before was chosen against a width
 *  budget rather than against a competitor, and the 14-entry dropdown beside it
 *  was never a decision at all. Raising this re-admits the long list to the band.
 */
export const OFFERED_STYLE_COUNT = 6;

/** Style names that belong to a particular PLACE in a document.
 *
 *  A document's own styles say where they are for — `header`, `footer`,
 *  `Table Paragraph`, `List Paragraph`, `annotation text`. When the caret is
 *  in one of those places those are the styles worth offering first, and the
 *  general-purpose ones are noise. This is the narrowing by context: the list
 *  is filtered by WHERE THE CURSOR IS, not by a fixed set of names. */
const CONTEXT_PATTERNS = {
  header: /^header/i,
  footer: /^footer/i,
  comment: /^(annotation|comment)/i,
  list: /(^|\s)(list|bullet|number)/i,
  table: /table/i,
};

/**
 * Where the caret is, as the style list cares about it — most specific first.
 *
 * @param {object} at
 * @param {string} [at.story]  "body", "header", "footer", "comment", "textbox".
 * @param {boolean} [at.inTable]
 * @param {string} [at.listKind]  "", "bullet", "numbered", "checklist".
 * @returns {string[]}
 */
export function caretContexts({
  story = "body",
  inTable = false,
  listKind = "",
} = {}) {
  const keys = [];
  if (story === "header" || story === "footer" || story === "comment") {
    keys.push(story);
  }
  if (listKind) keys.push("list");
  if (inTable) keys.push("table");
  return keys;
}

/**
 * The short list the control offers, in menu order.
 *
 * Four sources, in priority order, FILLED to `cap` rather than merely capped
 * at it:
 *
 *   1. styles belonging to WHERE THE CARET IS — a header style in a header,
 *      a table style in a table, a list style in a list;
 *   2. recommended styles the document defines — Word's quick set;
 *   3. styles the document is known to be using;
 *   4. the document's remaining ordinary paragraph styles;
 *   5. its utility styles (header, footer, annotation, revision, …) last.
 *
 * Step 3 is the one that was missing, and its absence is the defect this
 * function exists to prevent. The rule used to be "recommended ∩ defined",
 * which silently assumed every document uses Word's English style names.
 * Measured over the owner's 15-document corpus, SIX of them offered fewer
 * than six styles while defining more, and the Medical Incident Report Form —
 * which defines `Normal`, `p1`, `No Spacing`, `header`, `footer` — offered
 * exactly ONE. A control that shows one of the five styles the document has
 * is not a short list, it is a broken one.
 *
 * The style at the caret is guaranteed a slot even when the cap is full: it
 * displaces the last entry rather than falling off, because a control that
 * cannot show you the style you are in is worse than one offering one style
 * fewer. Word reaches the same guarantee by scrolling its gallery.
 *
 * @param {object} input
 * @param {string[]} input.recommended  Recommended names the document defines,
 *   already in priority order.
 * @param {Iterable<string>} input.inUse  Styles the chrome has seen in use.
 * @param {Set<string>} input.defined  Every paragraph style the document defines.
 * @param {string} input.active  The style at the caret, or "".
 * @param {number} [input.cap]
 * @returns {string[]}
 */
export function offeredStyleNames({
  recommended,
  inUse,
  defined,
  active,
  contexts = [],
  cap = OFFERED_STYLE_COUNT,
}) {
  const offered = [];
  const add = (name) => {
    if (name && defined.has(name) && !offered.includes(name))
      offered.push(name);
  };
  // WHERE THE CARET IS comes first. In a header, the document's own `header`
  // style is the one worth offering; a list that ignores the caret makes the
  // reader hunt for it in a place it was never promoted to.
  for (const key of contexts) {
    const pattern = CONTEXT_PATTERNS[key];
    if (!pattern) continue;
    for (const name of defined) if (pattern.test(name)) add(name);
  }
  for (const name of recommended) add(name);
  for (const name of inUse) add(name);
  // Fill from what the document actually has, ordinary styles before the
  // machinery ones, so the order stays useful rather than alphabetical luck.
  const rest = [...defined].filter((name) => !offered.includes(name));
  for (const name of rest.filter((name) => !UTILITY_STYLE_PATTERN.test(name)))
    add(name);
  for (const name of rest.filter((name) => UTILITY_STYLE_PATTERN.test(name)))
    add(name);

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

/**
 * The full menu: the suggested group, then everything else the document
 * defines, filtered by `query`.
 *
 * Returned as groups rather than one list because the two halves mean
 * different things to a reader — "what you probably want here" and "what this
 * document has" — and Word's Styles pane makes the same split with its
 * Recommended / All filter. A query narrows both; when it matches nothing the
 * caller shows its empty state rather than an unexplained blank popover.
 *
 * @returns {{suggested: string[], rest: string[]}}
 */
export function styleMenuGroups({ suggested, defined, query = "" }) {
  const needle = query.trim().toLowerCase();
  const matches = (name) => !needle || name.toLowerCase().includes(needle);
  const top = suggested.filter(matches);
  const rest = [...defined]
    .filter((name) => !suggested.includes(name))
    .filter(matches)
    .sort((a, b) => {
      // Machinery last here too, so a filtered list keeps the same order the
      // unfiltered one had.
      const au = UTILITY_STYLE_PATTERN.test(a);
      const bu = UTILITY_STYLE_PATTERN.test(b);
      return au === bu ? a.localeCompare(b) : Number(au) - Number(bu);
    });
  return { suggested: top, rest };
}
