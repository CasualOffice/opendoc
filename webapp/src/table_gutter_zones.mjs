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
// ## Competitive standard — read from ONLYOFFICE's source, not from a memory
//
// The owner's correction, after the first cut drew a separate bar beside the
// table: *"instead of adding new header on left and top.. why not highlighting
// the existing border.. and using that"*. He is describing what ONLYOFFICE
// actually does, and `CTable.prototype.private_CheckHitInBorder`
// (`sdkjs/word/Editor/Table.js:15712`) is the whole rule in twenty lines:
//
//   1. A ±3px radius about the CELL's own edges sets `Border` — `0` top, `2`
//      bottom, `3` left, `1` right. That is the resize hit.
//   2. Then two overrides, each of which sets **`Border = -1`**, cancelling the
//      resize it just found:
//        * `if (0 === nCurCell && X <= X_cell_start)` -> `RowSelection = true`
//        * `else if (0 === nCurRow && Y <= Y_cell_start + nRadius)` ->
//          `ColumnSelection = true`
//
// So selection is not a strip beside the table at all: it is **the leading
// border itself and everything outside it**, and where the two gestures meet,
// selection wins. `bInnerTableBorder` (`:5020`) carries the same distinction
// into the drag. Word reads the same way to a user — the outside edge selects,
// the border between two cells resizes.
//
// **This module is now that rule.** The zone spans from `STRIP_PX` outside the
// table's leading edge to a hair INSIDE it, so the painted border is inside the
// target rather than two pixels away from it. Google Docs was NOT checked from
// this machine and is not cited here; the behaviour is ONLYOFFICE's, verified in
// its source, and it is what the owner asked for.
//
// ## Why this does not collide with the resize gesture
//
// It cannot, and the reason is structural rather than a tuning choice.
// `tableChromeOnPage` reports one boundary per band, at the band's **closing**
// edge (`hittest.rs`: row edges at `row.top + row.height`, column edges at
// `cell.x + cell.width`). The table's leading top and left borders are therefore
// **not boundaries at all** and carry no resize zone — so the gutter is free to
// take them, exactly as ONLYOFFICE's `Border = -1` takes them there. The
// trailing outer edges stay with the resize, as they do in ONLYOFFICE.
//
// ## Why the inside reach cannot eat a cell
//
// `table_chrome_zones.mjs` had to clamp a boundary zone to a fifth of the band
// it separates, because a flat ±5px zone ate most of an 18px row and stole
// ordinary clicks. The inside reach here is the same shape of risk and takes the
// same clamp: `INSIDE_PX` is capped at `MAX_BAND_SHARE` of the leading band, so
// the rest of the first cell always places a caret, at every row height. The `+`
// insert discs are centred on a boundary but live *inside the strip* for the
// same reason — `docs/141` §4.2.1 requires it and it is what keeps the
// affordance free.

/** The gutter strip's thickness, in CSS px. Docs' own strip measures about 12;
 *  `docs/141` §4.2.1 sets the floor at 12. 14 gives the `+` disc room to sit
 *  inside the strip without touching the table's border. */
export const STRIP_PX = 14;

/** How far the zone reaches INSIDE the table's leading border, in CSS px.
 *
 *  ONLYOFFICE's own number: `private_CheckHitInBorder` takes its radius from
 *  `GetMMPerDot(3)` and its column-selection override reaches
 *  `Y <= Y_cell_start + nRadius` — three pixels into the first row. Ours applies
 *  it to both axes, because our leading borders carry no resize gesture to keep
 *  reachable from just inside, which is the only reason ONLYOFFICE's row
 *  override stops at `X <= X_cell_start`.
 *
 *  Three pixels is inside the cell's own left margin (a default cell inset is
 *  108 twips, about 7px at 100%), so no glyph is ever covered by the zone; and
 *  {@link stripRect} clamps it against the leading band besides. */
export const INSIDE_PX = 3;

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
 * The grab zone for one axis of one table, in page-local twips: from `outside`
 * before the table's leading border to `inside` past it.
 *
 * The zone straddles the border because the border **is** the affordance
 * (ONLYOFFICE's `Border = -1` overrides, quoted at the top of this file). The
 * outside half is clamped to the page's own edge — a table indented to 0 has no
 * room beside it, and a zone drawn at a negative coordinate would be painted off
 * the sheet where nothing could ever hit it — and the inside half is clamped to
 * `MAX_BAND_SHARE` of the band it reaches INTO, which is the leading band of the
 * **other** axis: the row zone reaches sideways into the first COLUMN, and the
 * column zone reaches down into the first ROW. Clamping each against its own
 * axis would have measured the wrong thing entirely — a 3px reach down into an
 * 18px first row is the case that matters, and the first column's width has
 * nothing to say about it.
 *
 * **A table flush against the sheet's edge still gets a zone**, which the old
 * outside-only strip could not: the inside half survives the clamp to 0. That
 * was a real gap — a table indented to the margin had no pointer affordance at
 * all and fell back to the menu.
 *
 * A zone thinner than `minimum` is reported as `null`.
 *
 * @param {object} table one `tableChromeOnPage` table
 * @param {"row"|"column"} axis
 * @param {number} outside twips before the leading border
 * @param {number} inside twips past it
 * @param {number} minimum the narrowest zone worth reporting, twips
 * @returns {{x:number,y:number,w:number,h:number}|null}
 */
export function stripRect(table, axis, outside, inside = 0, minimum = 0) {
  if (!table) return null;
  // The band the inside reach eats into is on the OTHER axis: a row zone reaches
  // into the first column, a column zone into the first row.
  const leading = tableBands(table, axis === "row" ? "column" : "row")[0]?.extent;
  const reach =
    Number.isFinite(leading) && leading > 0 ? Math.min(inside, leading * MAX_BAND_SHARE) : inside;
  if (axis === "row") {
    const x = Math.max(0, table.x - outside);
    const w = table.x - x + reach;
    return w > minimum ? { x, y: table.y, w, h: table.h } : null;
  }
  const y = Math.max(0, table.y - outside);
  const h = table.y - y + reach;
  return h > minimum ? { x: table.x, y, w: table.w, h } : null;
}

/**
 * Which BOUNDARY a reorder drag would drop on, for a pointer at `along`.
 *
 * `0` is before the first band, `bands.length` after the last: the insertion
 * point idiom, decided by the midpoint of the band the pointer is in, so the
 * indicator flips the moment the pointer passes the middle of a row rather than
 * only at its edge. That is what makes the drop predictable while the pointer is
 * still moving, which is the half a drag without an indicator was missing.
 *
 * Clamped at both ends, because a drag that runs past the table has not stopped
 * being a drag — Docs keeps showing the first/last drop rather than dropping the
 * indicator, and a pointer two pixels above the table still means "put it on
 * top".
 *
 * Complexity: O(bands) — the same arithmetic over the same memoised page chrome
 * every other zone here reads. Nothing is asked of the document.
 *
 * @param {Array<{start:number,end:number}>} bands from {@link tableBands}
 * @param {number} along page-local twips on the band axis
 * @returns {number} a boundary index in `[0, bands.length]`
 */
export function dropBoundaryAt(bands, along) {
  if (!bands?.length) return 0;
  for (let i = 0; i < bands.length; i++) {
    const band = bands[i];
    if (along < (band.start + band.end) / 2) return i;
    if (along < band.end) return i + 1;
  }
  return bands.length;
}

/**
 * The post-move index a move to `boundary` lands on, given the band is at
 * `from`.
 *
 * The facade takes `to` as the index the band occupies AFTER the move, which is
 * one less than the boundary whenever the band travels forwards — it vacates a
 * slot on the way past. Doing this arithmetic in one named place is deliberate:
 * an off-by-one here moves the right row to the wrong place and still commits,
 * which no refusal can catch.
 *
 * Complexity: O(1).
 *
 * @param {number} from the band's current index
 * @param {number} boundary a drop boundary, as {@link dropBoundaryAt} returns
 * @returns {number} the `to` argument for `moveTableRow` / `moveTableColumn`
 */
export function moveTargetIndex(from, boundary) {
  return boundary > from ? boundary - 1 : boundary;
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
 * Coordinates and every tolerance are twips. `opts` carries the zone's reach
 * per axis — `stripX`/`stripY` outside the leading border and `insideX`/
 * `insideY` past it (a PIXEL distance divided by the page's scale by the caller,
 * so a zoomed-out page does not get a zone ten times as wide in document space)
 * — and the insert disc's half-width per axis.
 *
 * An **insert** target beats a **strip** target, because it is the smaller zone
 * drawn inside the larger one and a user aiming at a disc has aimed at it.
 *
 * Complexity: O(bands on the page) — a few dozen comparisons, no document read.
 *
 * @param {Array} tables the page's `tableChromeOnPage` tables
 * @param {number} x page-local twips
 * @param {number} y page-local twips
 * @param {{stripX:number, stripY:number, insideX:number, insideY:number,
 *          insertX:number, insertY:number}} opts
 * @returns {object|null} `{ kind, axis, table, index, band, anchor, after, rect }`
 */
export function gutterAt(tables, x, y, opts) {
  for (const table of tables ?? []) {
    for (const axis of ["row", "column"]) {
      const across = axis === "row" ? x : y;
      const along = axis === "row" ? y : x;
      const thickness = axis === "row" ? opts.stripX : opts.stripY;
      const inside = (axis === "row" ? opts.insideX : opts.insideY) ?? 0;
      const strip = stripRect(table, axis, thickness, inside);
      if (!strip) continue;
      const lo = axis === "row" ? strip.x : strip.y;
      const hi = lo + (axis === "row" ? strip.w : strip.h);
      // The table's own leading border is INSIDE this zone now, which is the
      // whole of the owner's correction and is ONLYOFFICE's `X <= X_cell_start`.
      // Still half-open at the far end, so the first pixel that belongs to the
      // caret belongs to it unambiguously.
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
