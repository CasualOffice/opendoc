// The footer's counts, as sentences (docs/124).
//
// Pure: it takes four numbers and returns the strings the status bar shows.
// The reason it is a module rather than four lines in `updateStats` is that
// the same four figures are rendered FIVE times — three visible counts, the
// characters tooltip, and the whole-region tooltip that carries everything the
// narrow-window ladder has shed — and they were five separately written
// sentences with the plural rule hand-rolled into each. One of them is now one
// call, and a translator sees one set of keys.
import { n, t } from "./i18n.mjs";

/**
 * @param {{words: number, characters: number, charactersNoSpaces: number, paragraphs: number}} stats
 * @returns {{words: string, characters: string, paragraphs: string, charactersTitle: string, allTitle: string}}
 */
export function countLabels({ words, characters, charactersNoSpaces, paragraphs }) {
  const withSpaces = t("status.characters.withSpaces", { count: characters });
  const noSpaces = t("status.characters.noSpaces", { count: charactersNoSpaces });
  return {
    words: t("status.words", { count: words }),
    characters: t("status.characters", { count: characters }),
    paragraphs: t("status.paragraphs", { count: paragraphs }),
    // Word distinguishes with- from without-spaces; both are on the hover.
    charactersTitle: `${withSpaces}\n${noSpaces}`,
    // Narrow windows shed the lower-priority counts from the bar. Word keeps
    // the full set one gesture away in its Word Count dialog; until we have
    // that dialog, the whole region carries every figure so nothing shed
    // becomes unobtainable.
    allTitle: [
      t("status.words", { count: words }),
      withSpaces,
      noSpaces,
      t("status.paragraphs", { count: paragraphs }),
    ].join("\n"),
  };
}

/** "Page 3 of 12" — the footer's position indicator.
 *
 *  Here rather than in the shell for the same reason the counts are: it is a
 *  sentence built from numbers, and the numbers need the locale's own digits
 *  and grouping. `total` arrives already formatted because it can be an
 *  ESTIMATE ("~12") while a long document is still being measured, which is a
 *  string decision rather than a numeric one.
 *
 *  IN REFLOW IT IS NOT A PAGE NUMBER (`docs/151` §6.5, ADR-046), and this is the
 *  one place the shell can be honest about that. Reflow cuts the galley into
 *  fixed-height TILES; `page` is then a tile index, and a tile boundary falls
 *  wherever 11in of column happens to end — mid-paragraph, at a line. "Page 3 of
 *  12" built from those numbers would be a sentence that is wrong in both halves
 *  and indistinguishable from one that is right, which is exactly the class of
 *  claim `SKILL.md` §9 exists to stop. So reflow reports how far through the
 *  document the reader is instead, which the tile index CAN answer truthfully,
 *  and says nothing about pages. Word and Docs both replace the counter in their
 *  equivalent views; we differ from Word in giving a proportion rather than a
 *  tile count, deliberately, because a tile count invites being read as a page
 *  count.
 *
 *  Complexity: O(1).
 *
 * @param {number} page the caret's page, or in reflow its tile, 1-based.
 * @param {string} total the formatted page total, unused in reflow.
 * @param {number|null} [through] `readerPosition`'s 0..1 fraction when the view
 *        is reflowed, `null` on paper. Zero is a real answer (the top of the
 *        document), which is why the paged case is a `null` and not a falsy
 *        number — getting that wrong would print "Page 1 of 12" over a reflowed
 *        document exactly when the reader had not yet scrolled.
 */
export function pageIndicator(page, total, through = null) {
  if (through !== null) {
    return t("status.readingPosition", { percent: n(Math.round(through * 100)) });
  }
  return t("status.pageOf", { page: n(page), total });
}

/**
 * How far through the document the caret is, as a 0..1 fraction.
 *
 * Computed from the BAND rather than from the tile index, and that is the whole
 * point of it: a tile is 11in of column, so a document that reflows to less than
 * one tile has exactly one of them, and `tile / tiles` would report a reader at
 * the very top of it as "100% through". A position that is wrong in the obvious
 * case is not an improvement on a page number that is wrong in the subtle one.
 * The band already carries every tile's top and height in document space, so the
 * honest answer costs one array lookup.
 *
 * Complexity: O(1) — two indexed reads, whatever the document's length.
 *
 * @param {ArrayLike<number>} caret `[page, x, y, w, h]` from `caretRect`, page
 *        1-based and the rest in PAGE-LOCAL twips; empty when there is no caret.
 * @param {{tops:ArrayLike<number>, heights:ArrayLike<number>, docHeight:number}} band
 * @param {ArrayLike<{hTwip:number}>} pages the page records, for the twip height
 *        the band's CSS height corresponds to.
 * @returns {number} 0..1, and 0 when the geometry cannot answer.
 */
export function readerPosition(caret, band, pages) {
  const index = (caret?.length ? caret[0] : 1) - 1;
  const tile = pages?.[index];
  if (!band?.docHeight || !tile?.hTwip || !(index >= 0)) return 0;
  const intoTile = (caret.length > 2 ? caret[2] : 0) * (band.heights[index] / tile.hTwip);
  return Math.min(1, Math.max(0, (band.tops[index] + intoTile) / band.docHeight));
}
