// Which table boundary is under a point — the decision half of the table chrome
// layer, kept free of the DOM so the whole zone geometry is unit-testable in node.
//
// The same split `pointer_cursor.mjs` / `pointer_hover.mjs` already uses: the
// policy has no DOM, and the DOM half has no policy.

/** Whether `v` lies in `[lo, hi]` widened by `tol` on both ends. */
function within(v, lo, hi, tol) {
  return v >= lo - tol && v <= hi + tol;
}

/** The largest share of a band a boundary zone may take from either side.
 *
 *  The zone is a PIXEL distance, and a row is not. A table inserted at default
 *  height paints rows about 18px tall, so a flat ±5px zone claimed ten of those
 *  eighteen pixels and a click in the bottom quarter of an ordinary row started
 *  a resize instead of placing the caret. Word and ONLYOFFICE both keep the grab
 *  zone small relative to the band — ONLYOFFICE at ±3px absolute — and a
 *  proportion holds that property at every row height instead of trading a big
 *  row's forgiveness for a small row's correctness.
 *
 *  At 0.2 the zone is at most 40% of the shorter band it separates, so the
 *  middle 60% of even the shortest row always belongs to the text in it. */
const MAX_BAND_SHARE = 0.2;

/** `tol`, reduced so the zone cannot swallow either band this boundary closes.
 *
 *  `edges[i].height`/`.width` is the painted extent of the band the boundary
 *  closes (the one before it); the band after it is the next boundary's own
 *  extent. The trailing outer edge has no band after it. Complexity O(1). */
function clampedTolerance(edges, index, tol, extent) {
  const before = edges[index]?.[extent];
  const after = edges[index + 1]?.[extent];
  const shorter = Math.min(before ?? Infinity, after ?? Infinity);
  if (!Number.isFinite(shorter) || shorter <= 0) return tol;
  return Math.min(tol, shorter * MAX_BAND_SHARE);
}

/**
 * The table boundary within tolerance of the page-local point `(x, y)`, or
 * `null`. Coordinates and tolerances are twips.
 *
 * **Row wins at a crossing, and that is a decision rather than an accident.**
 * ONLYOFFICE tests `Border = 0` (top) and `2` (bottom) before `3`/`1`
 * (left/right) in `CTable.prototype.private_CheckHitInBorder`, so a corner
 * resolves to the horizontal edge there too; and a user travelling along a row
 * edge must not have the gesture change under them at every column it crosses.
 *
 * `tables` is the `tableChromeOnPage` payload for one page. Complexity is
 * O(boundaries on the page) — a few dozen, with no document read at all.
 *
 * @param {Array} tables the page's tables
 * @param {number} x page-local twips
 * @param {number} y page-local twips
 * @param {number} tolX tolerance across the column axis, twips
 * @param {number} tolY tolerance across the row axis, twips
 * @returns {object|null} `{ kind, table, i, x, y, w, h, width, height, outer, anchor }`
 */
export function boundaryAt(tables, x, y, tolX, tolY) {
  for (const table of tables ?? []) {
    if (!within(x, table.x, table.x + table.w, tolX)) continue;
    if (!within(y, table.y, table.y + table.h, tolY)) continue;
    const rowEdges = table.rowEdges ?? [];
    for (const [index, edge] of rowEdges.entries()) {
      if (Math.abs(y - edge.y) > clampedTolerance(rowEdges, index, tolY, "height")) continue;
      if (!within(x, edge.x, edge.x + edge.w, tolX)) continue;
      return { kind: "row", table, ...edge };
    }
    const colEdges = table.colEdges ?? [];
    for (const [index, edge] of colEdges.entries()) {
      if (Math.abs(x - edge.x) > clampedTolerance(colEdges, index, tolX, "width")) continue;
      if (!within(y, edge.y, edge.y + edge.h, tolY)) continue;
      return { kind: "column", table, ...edge };
    }
  }
  return null;
}

/**
 * The boundary a gesture may actually start on, plus the i18n key of the reason
 * when one is there and the mode refuses it.
 *
 * **Refuse by not arming the zone, not by painting a handle and failing on
 * release.** The old column handles were painted and draggable in Viewing and
 * Suggesting and the refusal arrived on pointer-up — a dead control that looked
 * live (`docs/141` TBL-35). Here the cursor never promises what the mode will not
 * do.
 *
 * @param {Array} tables the page's tables
 * @param {number} x page-local twips
 * @param {number} y page-local twips
 * @param {number} tolX tolerance, twips
 * @param {number} tolY tolerance, twips
 * @param {{editsBlocked?:boolean, geometryBlocked?:boolean}} mode the review mode
 * @returns {{boundary: object|null, refusal: string}}
 */
export function armTableBoundary(tables, x, y, tolX, tolY, mode = {}) {
  const boundary = boundaryAt(tables, x, y, tolX, tolY);
  if (!boundary) return { boundary: null, refusal: "" };
  if (mode.editsBlocked) return { boundary: null, refusal: "" };
  if (mode.geometryBlocked) return { boundary: null, refusal: "table.reason.notTracked" };
  if (boundary.kind === "column" && !boundary.table.regular) {
    return { boundary: null, refusal: "table.reason.merged" };
  }
  return { boundary, refusal: "" };
}
