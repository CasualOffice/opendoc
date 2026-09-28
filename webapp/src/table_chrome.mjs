// The table chrome layer: the pointer half of table editing (`docs/141` D-1).
//
// The finding this exists to fix, in the owner's words: *"table edit UX still the
// same .. nothing like google docs"*. The commands were all there — 20 invocable
// table commands on three surfaces each — and almost none of the ZONES. Our table
// was a menu-operated table: nothing responded to the pointer until you had
// clicked inside it, and then exactly one gesture (column width) was direct.
// Docs' table is a pointer-operated table: every structural edit has a hit zone
// on or beside the table itself.
//
// ## The named patterns, before any code
//
// Two, and neither is new here.
//
//  1. A **hover router over a canvas surface**. `pointer_cursor.mjs`'s own header
//     already wrote the rule down — *"ask the engine what is under the point,
//     decide from that, write the answer onto the canvas"*. This module is the
//     second half of that router: not only the cursor but the gesture.
//  2. **Indexing plus memoisation.** The engine answers one page's whole table
//     geometry in one walk (`tableChromeOnPage`), and this module caches that
//     answer per page until the next repaint. A pointer emits far more moves than
//     the screen has frames, and a repaint is the only thing that can move a
//     boundary, so the cache cannot go stale and a move costs a lookup.
//
// ## Why the perf fix came first
//
// `tableColumnResizeHandles` asked the layout for one cell rect per (row, column)
// and each of those scanned every placed fragment on every page: a 20x5 table
// cost 80 whole-page-tree scans, inside a repaint that runs on every caret move
// (`109` HF-222). A hover fires on every pointer move, so building this on that
// code would have multiplied the cost by the frame rate. The engine-side query is
// now one page walk — measured at 40 fragment visits against the per-cell route's
// 600 for the same 8-row table, and 80 against 2352 at 16 rows, i.e. linear where
// it was quadratic (`hittest.rs`
// `chrome_cost_doubles_with_the_table_where_the_per_cell_route_squared`).
//
// ## What is deliberately NOT painted
//
// No resting chrome on a table the pointer is merely near. The border is already
// drawn by the raster; the cursor change IS the affordance, which is what Docs
// does and what avoids proposing a visual change the owner has not approved
// (`docs/63`). One armed overlay child per gesture, never a handle per boundary
// at rest (`docs/141` §4.4 rule 2).

import { armTableBoundary } from "./table_chrome_zones.mjs";

/** Hit tolerance about a boundary, in CSS px — so ±5px, a 10px zone.
 *
 *  Kept at what the column handle already shipped (`.table-col-resize-handle` is
 *  10px wide with a -5px margin) rather than narrowed to ONLYOFFICE's ±3px
 *  (`CTable.prototype.private_CheckHitInBorder`, `nRadius = GetMMPerDot(3)`).
 *  A more forgiving zone is one of the few places we lead them and there is no
 *  reason to give it up. */
export const BOUNDARY_TOLERANCE_PX = 5;

/** Touch target size for a boundary pill, in CSS px — WCAG 2.5.8 Target Size
 *  (Minimum) is 24x24. Hover does not exist on touch, so the affordance has to
 *  be a visible, tappable object rather than a cursor. */
export const TOUCH_PILL_PX = 24;

/** One keyboard step, in twips: 36 = 0.025 in, the grid step the inspector's own
 *  width/height fields move in. */
export const KEYBOARD_STEP_TWIPS = 36;

/** Below this movement a drag commits nothing — the dead zone the column drag
 *  already shipped, in twips. A gesture that changes nothing must produce
 *  nothing, including no undo entry and no announcement. */
const COMMIT_DEAD_ZONE_TWIPS = 8;

/** The narrowest a band may be dragged to, in twips (0.05 in). The facade holds
 *  the same constant for the model side; this one stops the GUIDE at the floor so
 *  the pointer can keep going without the guide lying about the result. */
const MIN_BAND_TWIPS = 72;

/**
 * Creates the chrome layer bound to one editor host.
 *
 * Every accessor is a live getter, not a value: the document, the review mode and
 * the page records all change between frames.
 *
 * @param {object} host
 * @param {() => object|null} host.doc                 the live engine wrapper
 * @param {(page, event) => {x:number,y:number}} host.pointToTwip
 * @param {(page) => {sx:number,sy:number}} host.scaleOf
 * @param {() => {editsBlocked:boolean, geometryBlocked:boolean}} host.state
 * @param {(fn, options?) => unknown} host.runEdit     the gated edit runner
 * @param {(text:string, kind?:string) => void} host.status
 * @param {(key:string, params?:object) => string} host.t
 * @param {() => void} host.repaint
 * @returns {object} the chrome layer
 */
export function createTableChrome(host) {
  /** Per-page chrome, parsed once and reused until the next repaint. The key is
   *  the 1-based page number; `invalidate()` empties it. */
  let cache = new Map();
  /** The gesture in flight, or `null`. */
  let drag = null;
  /** Where the touch pills are painted, or `null`: `{ pageNumber, table }`. */
  let touchTable = null;

  /** The chrome for one page, from the cache or from the engine.
   *
   *  O(1) after the first call per page per repaint; the first call is
   *  O(fragments on the page) inside the engine. */
  function chromeOf(page) {
    const number = page?.pageNumber;
    if (!number) return [];
    const hit = cache.get(number);
    if (hit) return hit;
    const doc = host.doc();
    if (!doc?.tableChromeOnPage) return [];
    let tables = [];
    try {
      tables = JSON.parse(doc.tableChromeOnPage(number))?.tables ?? [];
    } catch {
      tables = [];
    }
    cache.set(number, tables);
    return tables;
  }

  /** The page-local twip point and the per-axis tolerance for one pointer event.
   *  The tolerance is a PIXEL distance, so it is divided by the page's scale —
   *  a zoomed-out page must not have a zone ten times as wide in document space
   *  as a zoomed-in one.
   *
   *  A TOUCH gets half the pill's width instead of the mouse's ±5px, because the
   *  pill is what the finger is aiming at and a finger is bigger than a cursor.
   *  This is the first `pointerType` read in the product. */
  function probeOf(page, event) {
    const { x, y } = host.pointToTwip(page, event);
    const { sx, sy } = host.scaleOf(page);
    const px = event?.pointerType === "touch" ? TOUCH_PILL_PX / 2 : BOUNDARY_TOLERANCE_PX;
    return { x, y, tolX: px / (sx || 1), tolY: px / (sy || 1) };
  }

  return {
    /** Drops the per-page cache. Called from the one repaint choke point, which
     *  is the only thing that can move a boundary. */
    invalidate() {
      cache = new Map();
    },

    /** The armed boundary under a pointer event, plus the reason when a boundary
     *  is there and the mode will not allow it. `{ boundary, refusal }`. */
    at(page, event) {
      const { x, y, tolX, tolY } = probeOf(page, event);
      return armTableBoundary(chromeOf(page), x, y, tolX, tolY, host.state());
    },

    /** What `pointer_cursor.mjs` should be told about this point: `"row"`,
     *  `"column"`, or `""`. Only an ARMED boundary reports, so the cursor never
     *  promises a gesture the mode refuses (`docs/141` §4.1.5, TBL-35). */
    targetKind(page, event) {
      return this.at(page, event).boundary?.kind ?? "";
    },

    /** The gesture in flight, as `pointer_cursor.mjs` names it, or `""`. */
    dragKind() {
      if (!drag) return "";
      return drag.kind === "row" ? "table-row" : "table-column";
    },

    /** Whether a resize gesture is in flight. */
    dragging() {
      return !!drag;
    },

    /**
     * Starts a resize if the press landed on an armed boundary. Returns whether
     * it did, so the caller can stop before placing a caret.
     *
     * A press on a boundary the mode refuses consumes nothing and SAYS why —
     * that is the one case where a refusal is worth a sentence, because the user
     * reached for a gesture that is deliberately absent rather than broken.
     */
    tryBeginDrag(page, event) {
      const { boundary, refusal } = this.at(page, event);
      if (!boundary) {
        if (refusal) host.status(host.t(refusal), "warn");
        return false;
      }
      const guide = document.createElement("div");
      guide.className =
        boundary.kind === "row" ? "table-row-resize-preview" : "table-col-resize-preview";
      const { sx, sy } = host.scaleOf(page);
      // The guide spans the TABLE, not the page. The column drag used to take its
      // height from `page.overlay.clientHeight`, which drew a full-page rule for a
      // three-row table.
      if (boundary.kind === "row") {
        guide.style.left = `${boundary.x * sx}px`;
        guide.style.top = `${boundary.y * sy}px`;
        guide.style.width = `${boundary.w * sx}px`;
      } else {
        guide.style.left = `${boundary.x * sx}px`;
        guide.style.top = `${boundary.y * sy}px`;
        guide.style.height = `${boundary.h * sy}px`;
      }
      page.overlay.appendChild(guide);
      drag = {
        kind: boundary.kind,
        page,
        guide,
        anchor: boundary.anchor,
        index: boundary.i,
        outer: boundary.outer,
        startExtent: boundary.kind === "row" ? boundary.height : boundary.width,
        neighbourExtent: neighbourExtentOf(boundary),
        startClient: boundary.kind === "row" ? event.clientY : event.clientX,
        delta: 0,
      };
      event.currentTarget?.setPointerCapture?.(event.pointerId);
      event.preventDefault();
      event.stopPropagation();
      return true;
    },

    /** Moves the guide. The guide CLAMPS at the floor and the pointer keeps
     *  going: a toast per pixel of over-drag would be noise, and the dead zone
     *  already establishes that a movement changing nothing produces nothing. */
    moveDrag(event) {
      if (!drag) return;
      const { sx, sy } = host.scaleOf(drag.page);
      const scale = drag.kind === "row" ? sy : sx;
      const raw = ((drag.kind === "row" ? event.clientY : event.clientX) - drag.startClient) /
        (scale || 1);
      drag.delta = clampDelta(Math.round(raw), drag);
      const px = drag.delta * scale;
      drag.guide.style.transform =
        drag.kind === "row" ? `translateY(${px}px)` : `translateX(${px}px)`;
      event.preventDefault();
    },

    /**
     * Commits the resize on release, or abandons it inside the dead zone.
     * Returns whether a gesture was in flight.
     *
     * Row height goes through `setTableRowHeight`, which existed with ZERO
     * webapp callers (`docs/141` §1.6) — the panel routed everything through
     * `applyTableProperties`, so the method was live capability behind a dead
     * surface. Column width goes through `moveTableColumnBoundary`, whose
     * `"border"` mode is what makes an internal drag MOVE the border instead of
     * widening the table.
     */
    finishDrag(event) {
      if (!drag) return false;
      const gesture = drag;
      drag = null;
      gesture.guide.remove();
      event?.preventDefault?.();
      if (Math.abs(gesture.delta) < COMMIT_DEAD_ZONE_TWIPS) {
        host.repaint();
        return true;
      }
      const doc = host.doc();
      if (gesture.kind === "row") {
        const height = Math.max(MIN_BAND_TWIPS, gesture.startExtent + gesture.delta);
        host.runEdit(() => doc.setTableRowHeight(gesture.anchor, height, "atLeast"), {
          gate: true,
        });
        host.status(host.t("table.rowHeightSet", { size: inches(height) }));
      } else {
        const mode = gesture.outer ? "table" : "border";
        host.runEdit(
          () => doc.moveTableColumnBoundary(gesture.anchor, gesture.index, gesture.delta, mode),
          { gate: true },
        );
        host.status(
          host.t("table.columnWidthSet", { size: inches(gesture.startExtent + gesture.delta) }),
        );
      }
      return true;
    },

    /** Abandons a gesture (pointer cancel, window blur), committing nothing. */
    cancelDrag() {
      if (!drag) return;
      drag.guide.remove();
      drag = null;
    },

    /**
     * Resizes the caret's row or column from the keyboard.
     *
     * `axis` is `"row"` or `"column"`, `sign` is +1 or -1. Bound to
     * Alt+Shift+Arrow, which was measured free: `keymap.mjs` declared 36 chords
     * and only `⌥H`/`⌘⌥A` used Alt at all.
     *
     * It reads the same page geometry the pointer does, so "the caret's column"
     * means the same band the pointer would grab — there is no second notion of
     * which column is current.
     */
    stepFromCaret(page, caretNode, cellRect, axis, sign) {
      const doc = host.doc();
      if (!doc || !caretNode) return false;
      const band = bandOfCell(chromeOf(page), cellRect, axis);
      if (!band) return false;
      if (host.state().geometryBlocked) {
        host.status(host.t("table.reason.notTracked"), "warn");
        return true;
      }
      if (axis === "column" && !band.table.regular) {
        host.status(host.t("table.reason.merged"), "warn");
        return true;
      }
      const step = KEYBOARD_STEP_TWIPS * sign;
      if (axis === "row") {
        const height = Math.max(MIN_BAND_TWIPS, band.edge.height + step);
        host.runEdit(() => doc.setTableRowHeight(caretNode, height, "atLeast"), { gate: true });
        host.status(host.t("table.rowHeightSet", { size: inches(height) }));
      } else {
        const mode = band.edge.outer ? "table" : "border";
        host.runEdit(() => doc.moveTableColumnBoundary(caretNode, band.edge.i, step, mode), {
          gate: true,
        });
        host.status(host.t("table.columnWidthSet", { size: inches(band.edge.width + step) }));
      }
      return true;
    },

    /**
     * Notes a touch tap so the boundaries of the tapped table become visible,
     * tappable pills. Hover does not exist on touch, and before this the column
     * handle was the only table gesture a touch could reach at all — at 10px
     * against WCAG 2.5.8's 24px minimum.
     *
     * Returns whether the tap was a touch inside a table.
     */
    noteTouch(page, event) {
      if (event.pointerType !== "touch") {
        if (touchTable) {
          touchTable = null;
          return false;
        }
        return false;
      }
      const { x, y } = probeOf(page, event);
      const table = chromeOf(page).find(
        (t) => x >= t.x && x <= t.x + t.w && y >= t.y && y <= t.y + t.h,
      );
      touchTable = table ? { pageNumber: page.pageNumber, table: table.node } : null;
      return !!table;
    },

    /** Forgets the touch pills (the caret left the table, or the mode changed). */
    clearTouch() {
      touchTable = null;
    },

    /**
     * Draws the resting column grips for the table the caret is in.
     *
     * These are now only half the story — the hover zones arm a boundary on ANY
     * table on the page, on both axes, with no caret needed — but they stay,
     * because a caret in a table is a strong signal that the user is about to
     * size it, and they are the one place the geometry is visible before the
     * pointer is near it.
     *
     * `cellRect` is the existing `cellRect(node)` answer, `[page, x, y, w, h]`,
     * so this costs no extra engine call: the overlay repaint already asks it for
     * the active-cell outline. Identifying the table by GEOMETRY rather than by a
     * `tableInfo` lookup is deliberate — `tableInfo` is three whole-document
     * walks (`docs/141` §1.13) and this page's chrome is already in hand.
     *
     * Nothing is painted in Viewing or Suggesting: a grip that refuses on
     * release is a dead control that looks live (TBL-35).
     */
    paintCaretColumnHandles(pages, cellRect) {
      if (host.state().geometryBlocked) return;
      if (!cellRect || cellRect.length < 5) return;
      const [number, x, y] = cellRect;
      const page = pages[number - 1];
      if (!page?.overlay) return;
      const table = chromeOf(page).find(
        (t) => x >= t.x && x < t.x + t.w && y >= t.y && y < t.y + t.h,
      );
      if (!table?.regular) return;
      const { sx, sy } = host.scaleOf(page);
      for (const edge of table.colEdges ?? []) {
        if (edge.outer) continue;
        const el = document.createElement("div");
        el.className = "table-col-resize-handle";
        el.dataset.col = String(edge.i);
        el.style.left = `${edge.x * sx}px`;
        el.style.top = `${edge.y * sy}px`;
        el.style.height = `${edge.h * sy}px`;
        el.addEventListener("pointerdown", (event) => this.tryBeginDrag(page, event));
        page.overlay.appendChild(el);
      }
    },

    /**
     * Paints the touch pills, if a touch tap armed them. One overlay child per
     * internal boundary rather than a continuous strip, so two adjacent
     * boundaries never merge into one target.
     *
     * Called from the overlay repaint, after `invalidate()`.
     */
    paintTouchPills(pages) {
      if (!touchTable) return;
      if (host.state().geometryBlocked) return;
      const page = pages[touchTable.pageNumber - 1];
      if (!page?.overlay) return;
      const table = chromeOf(page).find((t) => t.node === touchTable.table);
      if (!table) return;
      const { sx, sy } = host.scaleOf(page);
      const pill = (className, left, top, width, height, dataset) => {
        const el = document.createElement("div");
        el.className = className;
        el.style.left = `${left}px`;
        el.style.top = `${top}px`;
        el.style.width = `${width}px`;
        el.style.height = `${height}px`;
        Object.assign(el.dataset, dataset);
        page.overlay.appendChild(el);
      };
      for (const edge of table.rowEdges ?? []) {
        if (edge.outer) continue;
        pill(
          "table-row-touch-pill",
          edge.x * sx,
          edge.y * sy - TOUCH_PILL_PX / 2,
          Math.max(TOUCH_PILL_PX, edge.w * sx),
          TOUCH_PILL_PX,
          { tableRow: String(edge.i) },
        );
      }
      if (!table.regular) return;
      for (const edge of table.colEdges ?? []) {
        if (edge.outer) continue;
        pill(
          "table-col-touch-pill",
          edge.x * sx - TOUCH_PILL_PX / 2,
          edge.y * sy,
          TOUCH_PILL_PX,
          Math.max(TOUCH_PILL_PX, edge.h * sy),
          { tableColumn: String(edge.i) },
        );
      }
    },
  };
}

/** The extent of the band on the far side of a boundary, or `null` for the
 *  table's own trailing edge (nothing absorbs a delta there). */
function neighbourExtentOf(boundary) {
  if (boundary.outer) return null;
  const edges = boundary.kind === "row" ? boundary.table.rowEdges : boundary.table.colEdges;
  const next = (edges ?? []).find((edge) => edge.i === boundary.i + 1);
  if (!next) return null;
  return boundary.kind === "row" ? next.height : next.width;
}

/** The delta the guide may show: neither band may go under the floor. */
function clampDelta(delta, gesture) {
  const low = MIN_BAND_TWIPS - gesture.startExtent;
  let clamped = Math.max(delta, low);
  // A row drag grows one row and reflows the rest; only a COLUMN border drag has
  // a neighbour that pays for the delta, and that is the one that can be stopped
  // from the other side too.
  if (gesture.kind === "column" && gesture.neighbourExtent !== null) {
    clamped = Math.min(clamped, gesture.neighbourExtent - MIN_BAND_TWIPS);
  }
  return clamped;
}

/** The row or column band the caret's CELL sits in, with the boundary that closes
 *  it — the keyboard path's equivalent of a hover. `null` outside a table.
 *
 *  Resolved by geometry, from the `cellRect(node)` the repaint already asked for,
 *  rather than by matching the caret against a boundary's anchor: an anchor is the
 *  FIRST paragraph of a cell, so a caret in a cell's second paragraph would match
 *  nothing and the chord would silently do nothing. */
function bandOfCell(tables, cellRect, axis) {
  if (!cellRect || cellRect.length < 5) return null;
  const [, x, y, w, h] = cellRect;
  const cx = x + w / 2;
  const cy = y + h / 2;
  for (const table of tables) {
    if (cx < table.x || cx > table.x + table.w) continue;
    if (cy < table.y || cy > table.y + table.h) continue;
    const edges = axis === "row" ? table.rowEdges : table.colEdges;
    for (const edge of edges ?? []) {
      const end = axis === "row" ? edge.y : edge.x;
      const start = end - (axis === "row" ? edge.height : edge.width);
      const at = axis === "row" ? cy : cx;
      if (at >= start && at <= end) return { table, edge };
    }
  }
  return null;
}

/** A twip length as inches to two places — what the inspector's own fields show,
 *  so the announcement and the panel agree about the number. */
function inches(twips) {
  return (twips / 1440).toFixed(2);
}

