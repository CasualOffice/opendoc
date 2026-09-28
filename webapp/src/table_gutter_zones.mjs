// Which gutter zone is under a point — the decision half of the table gutter
// (`docs/141` D-2), kept free of the DOM so the whole zone geometry is
// unit-testable in node.
//
// The same split `table_chrome_zones.mjs` already uses for boundaries: the
// policy has no DOM, and the DOM half has no policy.
//
// ## The named pattern, before any code
//
// A **gutter / header strip** — the spreadsheet row-and-column-header idiom —
// over the SAME memoised page chrome the boundary zones read. Nothing here asks
// the engine anything: `tableChromeOnPage` already answered the page's whole
// table geometry in one walk and `table_chrome.mjs` caches it until the next
// repaint, so a hover costs a few dozen arithmetic comparisons and no document
// read at all. That is the whole of the O(1)-per-interaction rule for this
// layer.
//
// ## Competitive standard
//
// **Google Docs** paints a thin strip outside the table's leading edges on
// hover; clicking one selects that whole row or column, dragging along it
// extends the selection, dragging a strip that is already selected reorders it,
// and a `+` disc on a boundary inserts there. **Word** has the same click-to-
// select strip (an arrow bitmap left of a row, above a column) and the same
// hover `+` on the boundary, but no drag-reorder. **We followed Docs**, because
// Docs is the product the owner named and because reorder-by-drag is the half
// that makes the strip feel like a handle rather than a checkbox.
//
// ## Why the strips take nothing from the cells
//
// `table_chrome_zones.mjs` had to clamp a boundary zone to a fifth of the band
// it separates, because a flat ±5px zone ate most of an 18px row and stole
// ordinary clicks. The strips cannot repeat that mistake by construction: every
// zone in this module lies **outside** the table's bounding box, in the page
// margin beside it. A click inside any cell still places a caret at every strip
// width. The `+` insert discs are centred on a boundary but live *inside the
// strip* for the same reason — `docs/141` §4.2.1 requires it and it is what
// keeps the affordance free.

/** The gutter strip's thickness, in CSS px. Docs' own strip measures about 12;
 *  `docs/141` §4.2.1 sets the floor at 12. 14 gives the `+` disc room to sit
 *  inside the strip without touching the table's border. */
export const STRIP_PX = 14;

/** The strip's thickness under a finger, in CSS px — WCAG 2.5.8 Target Size
 *  (Minimum) is 24x24. Widening the strip is free in a way widening a BOUNDARY
 *  zone was not: the strip is outside the table, so a 24px strip still steals no
 *  part of any cell. The other half of 2.5.8 — the band's extent along the strip
 *  — is the row's own height and is not ours to set; a short row is reached by
 *  the menu, the palette and the context menu, which is why every gesture here
 *  has a command twin. */
export const TOUCH_STRIP_PX = 24;

/** Half the `+` insert disc, in CSS px. A 16px disc (`docs/141` §4.2.1). */
export const INSERT_TARGET_PX = 16;

/** The largest share of a band the insert zone may take from either side.
 *
 *  The same rule and the same number as `table_chrome_zones.mjs`' boundary
 *  clamp, for the same reason: the zone is a PIXEL distance and a row is not. A
 *  flat ±8px insert zone on an 18px row would leave no part of the strip that
 *  selects the row, so the affordance that inserts would have eaten the
 *  affordance that selects. At 0.2 the middle 60% of even the shortest band's
 *  strip segment always selects that band. */
const MAX_BAND_SHARE = 0.2;

/**
 * The bands of one axis of one table, leading edge first.
 *
 * `tableChromeOnPage` reports one edge per band — the boundary that CLOSES it —
 * carrying the band's painted extent and a paragraph anchor inside it. A band is
 * therefore `[end - extent, end]`, which is all this derives. There is no
 * leading edge in the payload, so the table's own leading side is the first
 * band's start.
 *
 * Complexity: O(bands), no document read.
 *
 * @param {object} table one `tableChromeOnPage` table
 * @param {"row"|"column"} axis
 * @returns {Array<{i:number, start:number, end:number, extent:number, anchor:string}>}
 */
export function tableBands(table, axis) {
  const edges = (axis === "row" ? table?.rowEdges : table?.colEdges) ?? [];
  const extentKey = axis === "row" ? "height" : "width";
  const endKey = axis === "row" ? "y" : "x";
  const out = [];
  for (const edge of edges) {
    const extent = edge[extentKey] ?? 0;
    if (extent <= 0) continue;
    out.push({
      i: edge.i,
      start: edge[endKey] - extent,
      end: edge[endKey],
      extent,
      anchor: edge.anchor,
    });
  }
  return out;
}

/**
 * The strip rectangle for one axis of one table, in page-local twips.
 *
 * `thickness` is the strip's width in twips. The strip is clamped to the page's
 * own leading edge: a table indented to 0 has no room outside it, and a strip
 * drawn at a negative coordinate would be painted off the sheet where nothing
 * could ever hit it. A clamped strip narrower than `minimum` is reported as
 * `null` — the row is then reachable from the menu, the context menu and the
 * palette, which is where the >=2-surface floor is actually met.
 *
 * @returns {{x:number,y:number,w:number,h:number}|null}
 */
export function stripRect(table, axis, thickness, minimum = 0) {
  if (!table) return null;
  if (axis === "row") {
    const x = Math.max(0, table.x - thickness);
    const w = table.x - x;
    return w > minimum ? { x, y: table.y, w, h: table.h } : null;
  }
  const y = Math.max(0, table.y - thickness);
  const h = table.y - y;
  return h > minimum ? { x: table.x, y, w: table.w, h } : null;
}

/** Whether `v` lies in `[lo, hi)`. Half-open so two adjacent bands never both
 *  claim the point on their shared boundary. */
function inBand(v, lo, hi) {
  return v >= lo && v < hi;
}

/** The insert zone's half-width at boundary `k`, reduced so it cannot swallow
 *  either band it sits between. Complexity O(1). */
function insertTolerance(bands, k, tol) {
  const before = bands[k - 1]?.extent;
  const after = bands[k]?.extent;
  const shorter = Math.min(before ?? Infinity, after ?? Infinity);
  if (!Number.isFinite(shorter) || shorter <= 0) return tol;
  return Math.min(tol, shorter * MAX_BAND_SHARE);
}

/**
 * The gutter target under the page-local point `(x, y)`, or `null`.
 *
 * Coordinates and every tolerance are twips. `opts` carries the strip thickness
 * per axis (a PIXEL distance divided by the page's scale by the caller, so a
 * zoomed-out page does not get a strip ten times as wide in document space) and
 * the insert disc's half-width per axis.
 *
 * An **insert** target beats a **strip** target, because it is the smaller zone
 * drawn inside the larger one and a user aiming at a disc has aimed at it.
 *
 * Complexity: O(bands on the page) — a few dozen comparisons, no document read.
 *
 * @param {Array} tables the page's `tableChromeOnPage` tables
 * @param {number} x page-local twips
 * @param {number} y page-local twips
 * @param {{stripX:number, stripY:number, insertX:number, insertY:number}} opts
 * @returns {object|null} `{ kind, axis, table, index, band, anchor, after, rect }`
 */
export function gutterAt(tables, x, y, opts) {
  for (const table of tables ?? []) {
    for (const axis of ["row", "column"]) {
      const across = axis === "row" ? x : y;
      const along = axis === "row" ? y : x;
      const thickness = axis === "row" ? opts.stripX : opts.stripY;
      const strip = stripRect(table, axis, thickness);
      if (!strip) continue;
      const lo = axis === "row" ? strip.x : strip.y;
      const hi = lo + (axis === "row" ? strip.w : strip.h);
      // Half-open at the table's own edge: a point exactly ON the border belongs
      // to the caret, not to the strip. Without this the table's top-left corner
      // resolved to the gutter and the first cell lost its corner pixel.
      if (across < lo || across >= hi) continue;
      const bands = tableBands(table, axis);
      if (!bands.length) continue;
      if (along < bands[0].start || along > bands[bands.length - 1].end) continue;
      const insertTol = axis === "row" ? opts.insertY : opts.insertX;
      // Boundary k sits before band k; boundary bands.length is the trailing
      // edge. Insert BEFORE band k, i.e. after band k-1.
      for (let k = 0; k <= bands.length; k++) {
        const at = k === bands.length ? bands[k - 1].end : bands[k].start;
        if (Math.abs(along - at) > insertTolerance(bands, k, insertTol)) continue;
        const after = k > 0;
        const band = after ? bands[k - 1] : bands[0];
        return {
          kind: "insert",
          axis,
          table,
          index: k,
          band,
          anchor: band.anchor,
          after,
          at,
          strip,
        };
      }
      const band = bands.find((b) => inBand(along, b.start, b.end)) ?? bands[bands.length - 1];
      return {
        kind: "strip",
        axis,
        table,
        index: band.i,
        band,
        anchor: band.anchor,
        after: false,
        at: band.start,
        strip,
      };
    }
  }
  return null;
}

/**
 * The gutter target a gesture may actually start on, plus the i18n key of the
 * reason when one is there and the mode refuses it.
 *
 * **Refuse by not arming the zone**, the rule `table_chrome_zones.mjs` already
 * states: the cursor never promises what the mode will not do, and the refusal
 * arrives on the press rather than after the gesture completes.
 *
 * Selecting a row or column is NOT an edit, so a strip arms in every mode. An
 * insert disc is a structural edit, so it needs Editing; in Suggesting it is not
 * painted and a press says why, and in Viewing it is not painted and a press is
 * an ordinary caret placement with nothing said — the same three-way split
 * `docs/141` §4.2.5 specifies.
 *
 * A column strip on a merged or spanned table refuses: `tableSelectionAnchorNodes`
 * returns nothing for `"column"` there, so a selection would silently be empty.
 *
 * @param {Array} tables the page's tables
 * @param {number} x page-local twips
 * @param {number} y page-local twips
 * @param {object} opts see {@link gutterAt}
 * @param {{editsBlocked?:boolean, geometryBlocked?:boolean}} mode the review mode
 * @returns {{target: object|null, refusal: string}}
 */
export function armTableGutter(tables, x, y, opts, mode = {}) {
  const target = gutterAt(tables, x, y, opts);
  if (!target) return { target: null, refusal: "" };
  if (target.axis === "column" && !target.table.regular) {
    return { target: null, refusal: mode.editsBlocked ? "" : "table.reason.merged" };
  }
  if (target.kind === "strip") return { target, refusal: "" };
  if (mode.editsBlocked) return { target: null, refusal: "" };
  if (mode.geometryBlocked) return { target: null, refusal: "table.reason.notTracked" };
  return { target, refusal: "" };
}
