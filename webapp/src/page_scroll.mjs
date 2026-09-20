// Where every page of a document sits, and how that maps onto a scrollbar a
// browser can actually build (`docs/113` §8.6).
//
// The viewer used to give every page its own sheet element in normal flow, so
// the scroll container was as tall as the document: 27,549,376 px for the
// owner's 25,556-page file. Browsers stop scrolling at 2^24 = 16,777,216 CSS
// px, so the last third of that document could not be reached at all — the
// silent-failure case `SKILL.md` §12 forbids, and the reason
// `MAX_VIEWER_BLOCKS` was pinned below what the engine can hold.
//
// This module is the answer, and it is deliberately a PURE one: no DOM, no
// engine, no globals. It owns two decisions and nothing else.
//
//  1. **Where a page is in document space.** Pages are stacked top to bottom at
//     their real CSS heights with a fixed gap, so page *i* has one unambiguous
//     y — whether or not a sheet element for it currently exists.
//  2. **How document space maps onto scroll space.** The scroll container is
//     capped at `MAX_SCROLL_PX`. Below the cap the mapping is the identity and
//     the geometry is exactly what it always was. Above it, scroll space is a
//     linear compression of document space that keeps BOTH endpoints exact:
//     scroll offset 0 shows the first page's top, and the maximum scroll offset
//     shows the last page's bottom. Reachability is therefore a property of the
//     arithmetic, not of how tall the browser lets an element be.
//
// The price of the compression, stated plainly because it is the reason the
// alternatives in `docs/113` §8.6 were weighed: above the cap one pixel of
// scrollbar travel is `scale` pixels of document, so a wheel notch moves
// `scale`× further than it would in a short document. It is bounded (`scale`
// is `docHeight / MAX_SCROLL_PX`), it applies only to documents no browser can
// represent honestly anyway, and it is the only artefact — page geometry, hit
// testing and every rect the engine hands out stay at their true scale, because
// pages are never themselves compressed. Only the emptiness between the window
// and the ends of the document is.

/** Vertical space between two page sheets, in CSS px.
 *
 *  This is the page *pitch* contribution, not a decoration: document space is
 *  built from it, so it must stay equal to the `gap` on `.pages`/`.page-band`
 *  in `style.css` or the sheets would be positioned where the gap is not.
 *  `tests/style_tokens.test.mjs` pins the two together. */
export const PAGE_GAP_PX = 22;

/** The tallest scroll container this viewer will build, in CSS px.
 *
 *  Measured, not assumed, and measured twice because the two measurements
 *  disagree about where exactly the wall is. `viewer-ceiling-measurement.spec`
 *  recorded a 16,910,594 px container whose final pages could not be reached
 *  (2^24 = 16,777,216 is the figure usually quoted); probing this Chromium
 *  directly with the cap removed, a 34,993,644 px band was CLAMPED to
 *  33,554,432 px — exactly 2^25 — and the last 271 pages of a 6,600-page
 *  document could not be scrolled to at all. Take the smaller of the two.
 *  This cap is deliberately well under half of it, for three reasons:
 *
 *  - other engines have their own limits, and this one must be the smallest;
 *  - a container near the wall degrades before it fails (sub-pixel scroll
 *    positions stop being representable long before scrolling stops);
 *  - the compression above the cap is what costs scroll *feel*, and a document
 *    that needs it is already one no browser can lay out flat, whereas a
 *    document under the cap keeps byte-identical geometry. 8,000,000 px is
 *    ~7,270 US Letter pages at 100% zoom, so every document short enough to be
 *    read page by page stays on the identity mapping. It is also under
 *    Firefox's own documented 17,895,697 px limit with room to spare.
 *
 *  Zoom multiplies document height, so this is reached by a 1,455-page document
 *  at 500% as surely as by a 7,270-page one at 100%. That is the same ceiling,
 *  met from the other direction, and it is why the cap is applied to the
 *  measured CSS height rather than to a page count. */
export const MAX_SCROLL_PX = 8_000_000;

/** How far beyond the viewport, in CSS px, a page is still kept in the DOM.
 *
 *  Large enough to cover `REVIEW_WINDOW_OVERSCAN` (800 px, the band in which
 *  `main.js` mounts comment cards), so a card is never asked to anchor to a
 *  page that does not exist; `main.js` widens it to a full viewport height when
 *  the window is taller than this. */
export const PAGE_WINDOW_OVERSCAN_PX = 1000;

/**
 * The document's page stack in CSS px, plus the scroll space it maps onto.
 *
 * @param {ArrayLike<{widthTwip:number,heightTwip:number}>} sizes page boxes in
 *        twips, in page order, exactly as the engine reports them.
 * @param {number} cssPerTwip CSS px per twip at the current zoom.
 * @param {{gap?:number,maxScroll?:number}} [options]
 * @returns {{
 *   count:number, gap:number, tops:Float64Array, heights:Float64Array,
 *   widths:Float64Array, width:number, docHeight:number, height:number,
 *   scale:number
 * }} `tops`/`heights`/`widths` are per page in document space; `width` is the
 *    widest page (the band's own width, so a mixed portrait/landscape document
 *    still centres each sheet); `docHeight` is the true stacked height;
 *    `height` is the scroll container's height (`min(docHeight, maxScroll)`);
 *    `scale` is `docHeight / height`, exactly 1 when nothing is compressed.
 */
export function buildPageBand(sizes, cssPerTwip, options = {}) {
  const gap = options.gap ?? PAGE_GAP_PX;
  const maxScroll = options.maxScroll ?? MAX_SCROLL_PX;
  const count = sizes.length;
  const tops = new Float64Array(count);
  const heights = new Float64Array(count);
  const widths = new Float64Array(count);
  let y = 0;
  let width = 0;
  for (let i = 0; i < count; i++) {
    const h = sizes[i].heightTwip * cssPerTwip;
    const w = sizes[i].widthTwip * cssPerTwip;
    tops[i] = y;
    heights[i] = h;
    widths[i] = w;
    if (w > width) width = w;
    y += h + gap;
  }
  // The trailing gap is not part of the document: it would scroll past the last
  // page into nothing, and it would make the last page's bottom unreachable by
  // exactly one gap at the maximum scroll offset.
  const docHeight = count === 0 ? 0 : y - gap;
  const height = Math.min(docHeight, maxScroll);
  return {
    count,
    gap,
    tops,
    heights,
    widths,
    width,
    docHeight,
    height,
    scale: height > 0 ? docHeight / height : 1,
  };
}

/** The scrollable travel of the band itself, in scroll space. */
function scrollTravel(band, viewportHeight) {
  return Math.max(0, band.height - viewportHeight);
}

/** The scrollable travel of the document, in document space. */
function docTravel(band, viewportHeight) {
  return Math.max(0, band.docHeight - viewportHeight);
}

/**
 * Where the document is, given where the scrollbar is.
 *
 * @param {ReturnType<typeof buildPageBand>} band
 * @param {number} viewportHeight the scroller's client height in CSS px.
 * @param {number} scrollLocal the scroll offset measured from the TOP OF THE
 *        BAND, not from the top of the scroller: the ruler and the viewport's
 *        own padding sit above the band and are not part of this mapping.
 * @returns {{docY:number, offset:number}} `docY` is the document-space y at the
 *        top edge of the viewport. `offset` is what a page's document-space top
 *        must be shifted by to become its position inside the band; it is
 *        identically 0 while nothing is compressed, which is what makes a
 *        normal document's sheets sit at fixed positions that never move on
 *        scroll.
 */
export function scrollToDoc(band, viewportHeight, scrollLocal) {
  const travel = scrollTravel(band, viewportHeight);
  // Clamped, not wrapped: scrolling above the band (the ruler is on screen) and
  // below it (the viewport's bottom padding) are both real positions, and both
  // must pin the document to its corresponding end rather than run past it.
  const s = Math.max(0, Math.min(travel, scrollLocal));
  if (travel === 0) return { docY: 0, offset: 0 };
  // Nothing is compressed: the mapping is the identity, and it is written as
  // one rather than computed. `(s / travel) * travel` is not `s` in floating
  // point — it is off by ~1e-13 — and an offset that is 1e-13 instead of 0 is
  // an offset that CHANGED, so every scroll event would rewrite every sheet's
  // `top` with a sub-pixel value for no reason.
  if (band.docHeight <= band.height) return { docY: s, offset: 0 };
  const docY = (s / travel) * docTravel(band, viewportHeight);
  return { docY, offset: s - docY };
}

/**
 * Where the scrollbar must be to put `docY` at the top edge of the viewport —
 * the exact inverse of {@link scrollToDoc}, in band-local scroll coordinates.
 *
 * @param {ReturnType<typeof buildPageBand>} band
 * @param {number} viewportHeight
 * @param {number} docY
 * @returns {number}
 */
export function docToScroll(band, viewportHeight, docY) {
  const travel = scrollTravel(band, viewportHeight);
  const doc = docTravel(band, viewportHeight);
  if (travel === 0 || doc === 0) return 0;
  const d = Math.max(0, Math.min(doc, docY));
  if (band.docHeight <= band.height) return d; // identity, exactly (see above)
  return (d / doc) * travel;
}

/**
 * The pages overlapping a document-space band, as an inclusive index range.
 *
 * Binary search, because this runs on every scroll event of a document with a
 * million paragraphs: a linear scan over 25,556 pages per frame is the kind of
 * cost that turns a fixed scroll ceiling into a janky one.
 *
 * @param {ReturnType<typeof buildPageBand>} band
 * @param {number} top document-space y of the band's top edge.
 * @param {number} bottom document-space y of its bottom edge.
 * @returns {{first:number, last:number}} clamped to the document; `last` is
 *          `first - 1` (an empty range) only when the document has no pages.
 */
export function pageRangeAt(band, top, bottom) {
  if (band.count === 0) return { first: 0, last: -1 };
  const first = pageIndexAtDocY(band, top);
  let last = first;
  while (last + 1 < band.count && band.tops[last + 1] <= bottom) last += 1;
  return { first, last };
}

/**
 * Where page `index` sits on screen, given where its band sits on screen.
 *
 * The same arithmetic that positions a sheet, run for the pages that have no
 * sheet — so a question about a page 20,000 pages away comes back with the
 * same shape, and the same fields, as a `getBoundingClientRect()`.
 *
 * @param {{left:number, top:number}} bandRect the band's client rect.
 * @param {ReturnType<typeof buildPageBand>} band
 * @param {number} index 0-based page index.
 * @param {number} offset the current document→band shift (`scrollToDoc`).
 */
export function pageClientRect(bandRect, band, index, offset) {
  const width = band.widths[index];
  const height = band.heights[index];
  const left = bandRect.left + Math.round((band.width - width) / 2);
  const top = bandRect.top + offset + band.tops[index];
  return { left, top, width, height, right: left + width, bottom: top + height, x: left, y: top };
}

/**
 * The index of the page containing `docY`, clamped into the document.
 *
 * A y in the gap between two pages resolves to the page ABOVE it, which is what
 * makes `pageRangeAt` include a page whose bottom edge is the only part still
 * on screen.
 *
 * @param {ReturnType<typeof buildPageBand>} band
 * @param {number} docY
 * @returns {number}
 */
export function pageIndexAtDocY(band, docY) {
  if (band.count === 0) return 0;
  let lo = 0;
  let hi = band.count - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (band.tops[mid] <= docY) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}
