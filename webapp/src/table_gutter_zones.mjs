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
// the rest of the first cell always places a caret, at every row height.
//
// ## Two lanes, each a 24px target, never over text (HF-341..HF-344)
//
// Outward from the leading border the gutter is two LANES, each one target
// thick: the select strip (`TARGET_PX` across, `INSIDE_PX` of it past the
// border) and, beyond it, the insert lane where the `+` discs sit on the
// boundaries. A disc used to sit INSIDE the strip and respond over a zone
// clamped to a fifth of the band (7px on a default row) while painting 16px,
// so a press on its lower half selected the row instead (`docs/171` §7 item 3).
// In its own lane a disc can be the full target without taking anything from
// the strip, and **the painted disc is its own hit box**: while one is shown,
// its box answers first (`opts.sticky`), so every pixel it paints inserts.
//
// The lanes live in FREE SPACE, measured by layout: `clearBefore` (the margin
// beside the rows) and `clearAbove` (the gap up to the line of text above), from
// `tableChromeOnPage`. A lane that does not fit is not drawn: the column strip
// used to reach 14px up over the paragraph above, so hovering that line offered
// a column selection and a press there took it (item 6). Where only the strip
// fits, the discs share it, but only on a boundary whose neighbouring bands are
// at least two targets long, so the strip keeps a full target of every band.
// Where not even the strip fits, it shrinks to the free space and the border.
// Nothing here is drawn over a line of text, ever; the menu, the context menu
// and the palette carry every gesture a lane that does not fit would have.

/** A pointer target for a mouse or a pen, in CSS px: WCAG 2.5.8 Target Size
 *  (Minimum). Each lane is this thick and each disc this big (HF-341, HF-342). */
export const TARGET_PX = 24;

/** The same for a coarse pointer (a finger): 44px, WCAG 2.5.5 and the platform
 *  guidelines. A finger has no hover, so this is what a tap has to land in. */
export const COARSE_TARGET_PX = 44;

/** How far the strip reaches INSIDE the table's leading border, in CSS px.
 *
 *  ONLYOFFICE's own number: `private_CheckHitInBorder` takes its radius from
 *  `GetMMPerDot(3)` and its column-selection override reaches
 *  `Y <= Y_cell_start + nRadius`, three pixels into the first row. Ours applies
 *  it to both axes, because our leading borders carry no resize gesture to keep
 *  reachable from just inside, which is the only reason ONLYOFFICE's row
 *  override stops at `X <= X_cell_start`.
 *
 *  Three pixels is inside the cell's own left margin (a default cell inset is
 *  108 twips, about 7px at 100%), so no glyph is ever covered by the zone; and
 *  {@link gutterLanes} clamps it against the leading band besides. The boundary
 *  zones start this far in, so the two never contest a pixel. */
export const INSIDE_PX = 3;

/** The target size for a pointer type: a finger gets the coarse size, a mouse
 *  and a pen the fine one. Complexity O(1). */
export function targetPxFor(pointerType) {
  return pointerType === "touch" ? COARSE_TARGET_PX : TARGET_PX;
}

/** The largest share of a band the inside reach may take.
 *
 *  The same rule and the same number as `table_chrome_zones.mjs`' boundary
 *  clamp, for the same reason: the reach is a PIXEL distance and a row is not. */
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
 * The gutter's lanes for one axis of one table, in page-local twips.
 *
 * `target` is one target's size in twips (the lane's thickness across the axis
 * and a disc's length along it); `inside` is the reach past the border. Returns
 * `{ strip, insert, shared }`: `strip` is the select lane, `insert` the disc
 * lane beyond it or `null`, and `shared` says the discs live in the strip
 * because only the strip fits. Where not even a full strip fits, the strip
 * shrinks to the free space plus the border (`short: true`, no discs) rather
 * than reaching over text.
 *
 * The free space is `clearBefore` for the row axis and `clearAbove` for the
 * column axis, from layout. A payload without them (an older engine) falls back
 * to the distance to the sheet's edge, which is what it used to assume.
 *
 * The inside reach is clamped to a fifth of the band it reaches INTO, which is
 * the leading band of the OTHER axis: the row zone reaches sideways into the
 * first column, the column zone down into the first row.
 *
 * Complexity: O(1) beyond {@link tableBands}.
 *
 * @param {object} table one `tableChromeOnPage` table
 * @param {"row"|"column"} axis
 * @param {number} target one target, twips
 * @param {number} inside the reach past the border, twips
 * @returns {{strip:object, insert:object|null, shared:boolean, short?:boolean}|null}
 */
export function gutterLanes(table, axis, target, inside = 0) {
  if (!table || !(target > 0)) return null;
  const row = axis === "row";
  const reachInto = (band) => {
    const leading = tableBands(table, band)[0]?.extent;
    return Number.isFinite(leading) && leading > 0 ? Math.min(inside, leading * MAX_BAND_SHARE) : inside;
  };
  const reach = reachInto(row ? "column" : "row");
  // The column lanes start where the row strip's inside reach ends, so the two
  // strips never share the corner pixel where the leading borders meet.
  const corner = row ? 0 : reachInto("column");
  const border = row ? table.x : table.y;
  const free = Math.max(0, (row ? table.clearBefore : table.clearAbove) ?? border);
  const stripOut = target - reach;
  const across = (lo, hi) =>
    row
      ? { x: lo, y: table.y, w: hi - lo, h: table.h }
      : { x: table.x + corner, y: lo, w: table.w - corner, h: hi - lo };
  // Not room for a full target: the strip SHRINKS to the free space and the
  // border itself rather than reaching over the text. Directly under a
  // paragraph that is the border and its inside reach, which is ONLYOFFICE's
  // whole target there; the menu and the palette carry the rest. Its discs stay
  // while the strip is at least the 16px the disc used to be (a heading's
  // spacing-after, measured at 19px), and are as thick as the strip, so a disc
  // is never drawn bigger than the box it answers over.
  if (free < stripOut) {
    if (free + reach <= 0) return null;
    const strip = across(border - free, border + reach);
    return { strip, insert: null, shared: free + reach >= (target * 2) / 3, short: true };
  }
  const strip = across(border - stripOut, border + reach);
  const fits = free >= stripOut + target;
  const insert = fits ? across(border - stripOut - target, border - stripOut) : null;
  return { strip, insert, shared: !fits };
}

/**
 * The strip's own rectangle (the select lane), or `null` when it does not fit.
 * Complexity O(1).
 *
 * @param {object} table
 * @param {"row"|"column"} axis
 * @param {number} target one target, twips
 * @param {number} inside the reach past the border, twips
 * @returns {{x:number,y:number,w:number,h:number}|null}
 */
export function stripRect(table, axis, target, inside = 0) {
  return gutterLanes(table, axis, target, inside)?.strip ?? null;
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

/** Whether the page-local point lies in `rect`, half-open. */
function inRect(rect, x, y) {
  return !!rect && x >= rect.x && x < rect.x + rect.w && y >= rect.y && y < rect.y + rect.h;
}

/** Boundary `k` of a band list: `bands.length + 1` of them, leading edge first. */
function boundaryAtIndex(bands, k) {
  return k === bands.length ? bands[k - 1].end : bands[k].start;
}

/** The insert target at boundary `k`, with its disc box: one target long,
 *  centred on the boundary, and as thick as the lane it sits in. */
function insertTarget(table, axis, bands, k, lane, target, strip) {
  const at = boundaryAtIndex(bands, k);
  const after = k > 0;
  const band = after ? bands[k - 1] : bands[0];
  const half = target / 2;
  const disc =
    axis === "row"
      ? { x: lane.x, y: at - half, w: lane.w, h: target }
      : { x: at - half, y: lane.y, w: target, h: lane.h };
  return { kind: "insert", axis, table, index: k, band, anchor: band.anchor, after, at, strip, disc };
}

/**
 * The gutter target under the page-local point `(x, y)`, or `null`.
 *
 * Coordinates and every size are twips. `opts` carries one target's size per
 * axis (`targetX` across the row gutter, `targetY` across the column gutter: a
 * PIXEL size divided by the page's scale by the caller, so a zoomed-out page
 * does not get a zone ten times as wide in document space), the inside reach
 * (`insideX`/`insideY`), and `sticky`: the disc painted right now, as
 * `{ table, axis, index, disc }`. **The painted disc answers first**, over its
 * whole box, so what is drawn is what responds (HF-341).
 *
 * In the insert lane a point resolves to the NEAREST boundary within half a
 * target: one disc, never two. In a shared strip a disc arms only on a boundary
 * whose neighbouring bands are each at least two targets long, so selecting a
 * band always keeps a full target of it.
 *
 * Complexity: O(bands on the page), a few dozen comparisons, no document read.
 *
 * @param {Array} tables the page's `tableChromeOnPage` tables
 * @param {number} x page-local twips
 * @param {number} y page-local twips
 * @param {{targetX:number, targetY:number, insideX?:number, insideY?:number,
 *          sticky?:object|null}} opts
 * @returns {object|null} `{ kind, axis, table, index, band, anchor, after, at, strip, disc? }`
 */
export function gutterAt(tables, x, y, opts) {
  const sticky = opts.sticky;
  if (sticky && inRect(sticky.disc, x, y)) {
    const table = (tables ?? []).find((t) => t.node === sticky.table);
    const bands = table ? tableBands(table, sticky.axis) : [];
    const target = sticky.axis === "row" ? opts.targetX : opts.targetY;
    const inside = (sticky.axis === "row" ? opts.insideX : opts.insideY) ?? 0;
    const lanes = table ? gutterLanes(table, sticky.axis, target, inside) : null;
    if (lanes && bands.length && sticky.index <= bands.length) {
      const hit = insertTarget(table, sticky.axis, bands, sticky.index, sticky.disc, target, lanes.strip);
      hit.disc = sticky.disc;
      return hit;
    }
  }
  for (const table of tables ?? []) {
    for (const axis of ["row", "column"]) {
      const row = axis === "row";
      const across = row ? x : y;
      const along = row ? y : x;
      const target = row ? opts.targetX : opts.targetY;
      const lanes = gutterLanes(table, axis, target, (row ? opts.insideX : opts.insideY) ?? 0);
      if (!lanes) continue;
      const bands = tableBands(table, axis);
      if (!bands.length) continue;
      const { strip, insert } = lanes;
      const lo = (rect) => (row ? rect.x : rect.y);
      const hi = (rect) => lo(rect) + (row ? rect.w : rect.h);
      // The nearest boundary to the pointer along the axis, and whether the
      // pointer is within half a target of it.
      let k = 0;
      for (let i = 1; i <= bands.length; i++) {
        const closer = Math.abs(along - boundaryAtIndex(bands, i));
        if (closer < Math.abs(along - boundaryAtIndex(bands, k))) k = i;
      }
      const near = Math.abs(along - boundaryAtIndex(bands, k)) <= target / 2;
      if (insert && across >= lo(insert) && across < hi(insert)) {
        if (near) return insertTarget(table, axis, bands, k, insert, target, strip);
        continue;
      }
      if (across < lo(strip) || across >= hi(strip)) continue;
      if (along < bands[0].start || along > bands[bands.length - 1].end) continue;
      if (lanes.shared && near) {
        const shortest = Math.min(bands[k - 1]?.extent ?? Infinity, bands[k]?.extent ?? Infinity);
        if (shortest >= 2 * target) return insertTarget(table, axis, bands, k, strip, target, strip);
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
  // A merged table selects, inserts and moves columns like any other (`docs/171`).
  if (target.kind === "strip") return { target, refusal: "" };
  if (mode.editsBlocked) return { target: null, refusal: "" };
  if (mode.geometryBlocked) return { target: null, refusal: "table.reason.notTracked" };
  return { target, refusal: "" };
}
