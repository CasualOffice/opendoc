// The table GUTTER: the row and column strips beside a table, the hover `+`
// insert discs on their boundaries, and the drag that selects several rows or
// columns (`docs/141` D-2).
//
// The owner's sentence this exists to answer is *"table UX is also not built"*.
// The commands were all there and the engine was all there; what a user TOUCHES
// was not. Docs' table has a grab strip on the outside of every row and column:
// point at it, click it, drag along it. Ours had nothing outside the border at
// all, so selecting a row meant a menu round trip and selecting three meant
// nothing at all.
//
// ## The named patterns, before any code
//
// 1. A **gutter / header strip**, the spreadsheet row-and-column-header idiom.
//    The sibling `opencalc` has the spreadsheet version; this is the same shape
//    over a table's bands.
// 2. **Indexing plus memoisation**, exactly as `table_chrome.mjs` states it:
//    the engine answers one page's whole table geometry in ONE walk
//    (`tableChromeOnPage`), that answer is cached until the next repaint, and a
//    pointer move is a few dozen comparisons over it. Nothing in this module
//    asks the engine anything during a hover. That is deliberate and it is the
//    quadratic the chrome layer just removed: `tableColumnResizeHandles` asked
//    for one cell rect per (row, column) and each scanned every fragment on
//    every page.
//
// ## What is painted, and when
//
// Only the ONE table the pointer is over, only while it is over it, and only
// the segment of the strip the pointer is in. `docs/141` §4.4 rule 2: no
// resting chrome, one armed overlay child per gesture, never a handle per
// boundary at rest. The DOM is rebuilt only when the armed key changes — the
// table, the axis, the band, whether a `+` is armed — so travelling along a
// strip inside one band costs no DOM work at all.
//
// ## Deliberately NOT in this slice
//
// **Drag-to-reorder.** `docs/141` §4.2.3 specifies it as two facade
// permutations, `moveTableRow(node, from, to)` and `moveTableColumn(...)`, both
// `Operation::ReplaceTable`. Neither exists in `casual-doc-wasm` yet and this
// lane does not own `crates/**`. Building the gesture against a method that is
// not there would ship chrome that cannot commit — "built is not reachable",
// the most expensive recurring pattern in this repository (SKILL §9.4) — so the
// gesture is absent rather than dead, and no `table.move.*` command is
// registered for it. Row and column ORDER is still changeable today by
// insert + delete from four surfaces; what is missing is the direct gesture.

import {
  STRIP_PX,
  TOUCH_STRIP_PX,
  INSERT_TARGET_PX,
  armTableGutter,
  stripRect,
  tableBands,
} from "./table_gutter_zones.mjs";

/** The two strip classes and the selection mode each one selects. */
const AXIS = Object.freeze({
  row: { strip: "table-row-strip", mode: "row" },
  column: { strip: "table-column-strip", mode: "column" },
});

/**
 * Creates the gutter layer bound to one editor host.
 *
 * @param {object} host
 * @param {() => object|null} host.doc
 * @param {(page) => Array} host.chromeOf          the memoised page chrome
 * @param {(page, event) => {x:number,y:number}} host.pointToTwip
 * @param {(page) => {sx:number,sy:number}} host.scaleOf
 * @param {(clientX:number, clientY:number) => object|null} host.pageFromClientPoint
 * @param {() => {editsBlocked:boolean, geometryBlocked:boolean}} host.state
 * @param {object} host.range                      the `table_range.mjs` store
 * @param {(fn, options?) => unknown} host.runEdit
 * @param {() => Array} host.pages                 the page records
 * @param {(text:string, kind?:string) => void} host.status
 * @param {(key:string, params?:object) => string} host.t
 * @param {() => void} host.repaint                the full selection repaint
 * @param {() => void} host.repaintOverlay         the overlay-only repaint
 * @returns {object} the gutter layer
 */
export function createTableGutter(host) {
  /** What the pointer is over, or `null`: the whole paint is a function of it. */
  let armed = null;
  /** The strip drag in flight, or `null`. */
  let drag = null;
  /** The overlay children this layer owns, so a hover can replace exactly its
   *  own chrome without a full overlay repaint. A full repaint drops the page
   *  geometry cache, which would turn every band a pointer crosses into a fresh
   *  `tableChromeOnPage` walk — the per-hover engine call this whole layer is
   *  built to avoid. */
  let owned = [];

  /** The strip thickness and insert tolerance for one event, in page-local
   *  twips. A PIXEL distance divided by the page's scale, so a zoomed-out page
   *  does not get a strip ten times as wide in document space.
   *
   *  A finger gets the 24px strip unconditionally, where the BOUNDARY zones had
   *  to wait to be armed by a tap. The difference is that a strip lies outside
   *  the table: 24px there takes nothing from any cell, so it cannot swallow the
   *  arming tap the way an enlarged boundary zone did (`docs/141` TBL-18). */
  function optsOf(page, event) {
    const { sx, sy } = host.scaleOf(page);
    const strip = event?.pointerType === "touch" ? TOUCH_STRIP_PX : STRIP_PX;
    const insert = INSERT_TARGET_PX / 2;
    return {
      stripX: strip / (sx || 1),
      stripY: strip / (sy || 1),
      insertX: insert / (sx || 1),
      insertY: insert / (sy || 1),
    };
  }

  /** The armed gutter target under an event, plus any refusal key. */
  function probe(page, event) {
    if (!page) return { target: null, refusal: "" };
    const { x, y } = host.pointToTwip(page, event);
    return armTableGutter(host.chromeOf(page), x, y, optsOf(page, event), host.state());
  }

  /** The identity of what is armed, so the DOM is rebuilt only when it changes. */
  function keyOf(target, page) {
    if (!target) return "";
    return [page.pageNumber, target.table.node, target.axis, target.kind, target.index].join("/");
  }

  /** Paints one absolutely-positioned overlay child, recording it so the next
   *  hover can take exactly this layer's chrome down again. */
  function child(page, className, left, top, width, height) {
    const el = document.createElement("div");
    el.className = className;
    el.style.left = `${left}px`;
    el.style.top = `${top}px`;
    el.style.width = `${width}px`;
    el.style.height = `${height}px`;
    page.overlay.appendChild(el);
    owned.push(el);
    return el;
  }

  return {
    /** Forgets what the pointer was over. The overlay repaint destroys the
     *  children anyway; this is what stops them being drawn again. */
    clear() {
      if (!armed) return false;
      armed = null;
      for (const el of owned) el.remove();
      owned = [];
      return true;
    },

    /** Whether a strip drag is in flight. */
    dragging() {
      return !!drag;
    },

    /** What the cursor router should be told: `"row"`, `"column"`, `"insert"`,
     *  or `""`. The strips are real elements with their own CSS cursors, so this
     *  exists for the frame in which the pointer has entered the gutter and the
     *  strip has not been painted yet. */
    targetKind(page, event) {
      const { target } = probe(page, event);
      if (!target) return "";
      return target.kind === "insert" ? "insert" : target.axis;
    },

    /**
     * Notes what the pointer is over and repaints when that changed.
     *
     * Returns whether a gutter target is armed, so the caller can skip the rest
     * of the hover router — the gutter is outside the table, so nothing else is
     * ever under the same point.
     *
     * Complexity: O(bands on the page), no engine call, and NO DOM work at all
     * unless the armed band changed.
     */
    hover(page, event) {
      const { target } = probe(page, event);
      const key = target ? keyOf(target, page) : "";
      if (key === (armed?.key ?? "")) return !!target;
      armed = target ? { key, page, target } : null;
      this.paint(host.pages());
      return !!target;
    },

    /**
     * Starts a gutter gesture if the press landed in the gutter. Returns whether
     * it did, so the caller can stop before placing a caret.
     *
     * A press on a refused zone consumes nothing and SAYS why — the one case
     * where a refusal is worth a sentence, because the user reached for a
     * gesture that is deliberately absent rather than broken.
     */
    tryBeginDrag(page, event) {
      const { target, refusal } = probe(page, event);
      if (!target) {
        if (refusal) host.status(host.t(refusal), "warn");
        return false;
      }
      event.preventDefault();
      event.stopPropagation();
      if (target.kind === "insert") {
        void commitInsert(target);
        return true;
      }
      const mode = AXIS[target.axis].mode;
      const reason = host.range.selectMode(target.anchor, mode);
      if (reason) {
        host.status(host.t(reason), "warn");
        return true;
      }
      drag = {
        page,
        axis: target.axis,
        anchor: target.anchor,
        tableNode: target.table.node,
        index: target.index,
        startIndex: target.index,
        moved: false,
      };
      event.currentTarget?.setPointerCapture?.(event.pointerId);
      host.range.announce();
      host.repaint();
      return true;
    },

    /**
     * Extends the selection along the strip — the gesture that selects three
     * rows by dragging down the gutter.
     *
     * The rectangle is squared off at both ends through
     * `tableSelectionAnchorNodes`: the anchor end takes the FIRST cell of the
     * band the press landed in and the focus end the LAST cell of the band the
     * pointer is in now, so a two-row drag selects both rows whole rather than
     * the cells the pointer happened to pass over.
     *
     * Re-asked only when the band under the pointer changes, which is what keeps
     * a drag O(1) per pointer move in the common case.
     */
    moveDrag(event) {
      if (!drag) return;
      event.preventDefault();
      const page = host.pageFromClientPoint(event.clientX, event.clientY) ?? drag.page;
      const { target } = probe(page, event);
      const band = target?.axis === drag.axis ? target : bandUnderPointer(page, event, drag.axis);
      if (!band || band.table.node !== drag.tableNode) return;
      if (band.index === drag.index && drag.moved) return;
      const mode = AXIS[drag.axis].mode;
      const first = host.range.endpointOf(drag.anchor, mode, "first");
      const last = host.range.endpointOf(band.anchor, mode, "last");
      if (!first || !last) return;
      drag.index = band.index;
      drag.moved = true;
      host.range.set(first, last, band.index === drag.startIndex ? mode : "cells");
      host.repaintOverlay();
    },

    /** Ends the strip drag and announces what it selected. */
    finishDrag(event) {
      if (!drag) return false;
      drag = null;
      event?.preventDefault?.();
      host.range.announce();
      host.repaint();
      return true;
    },

    /** Abandons the gesture (pointer cancel, window blur). The selection it made
     *  on press stays: a cancelled DRAG is not a cancelled CLICK. */
    cancelDrag() {
      drag = null;
    },

    /**
     * Draws the armed strip, its hot band and its `+` disc.
     *
     * Called from the one overlay repaint, which destroys and rebuilds every
     * overlay child — so this is where the chrome comes back, not where it is
     * decided.
     */
    paint(pages) {
      for (const el of owned) el.remove();
      owned = [];
      if (!armed) return;
      const page = pages[armed.page.pageNumber - 1];
      if (!page?.overlay) return;
      const { target } = armed;
      // The table may have moved or gone since the pointer stopped: re-read it
      // from the page's current chrome rather than trusting the armed copy.
      const table = host.chromeOf(page).find((t) => t.node === target.table.node);
      if (!table) return;
      const { sx, sy } = host.scaleOf(page);
      const rowAxis = target.axis === "row";
      const strip = stripRect(table, target.axis, rowAxis ? target.strip.w : target.strip.h);
      if (!strip) return;
      child(
        page,
        AXIS[target.axis].strip,
        strip.x * sx,
        strip.y * sy,
        strip.w * sx,
        strip.h * sy,
      );
      const bands = tableBands(table, target.axis);
      if (target.kind === "strip") {
        const band = bands.find((b) => b.i === target.index);
        if (!band) return;
        child(
          page,
          "table-gutter-band",
          rowAxis ? strip.x * sx : band.start * sx,
          rowAxis ? band.start * sy : strip.y * sy,
          rowAxis ? strip.w * sx : band.extent * sx,
          rowAxis ? band.extent * sy : strip.h * sy,
        );
        return;
      }
      const at = target.at;
      const disc = child(
        page,
        "table-insert-target",
        rowAxis ? strip.x * sx + (strip.w * sx - INSERT_TARGET_PX) / 2 : at * sx - INSERT_TARGET_PX / 2,
        rowAxis ? at * sy - INSERT_TARGET_PX / 2 : strip.y * sy + (strip.h * sy - INSERT_TARGET_PX) / 2,
        INSERT_TARGET_PX,
        INSERT_TARGET_PX,
      );
      disc.dataset.insert = target.axis;
      disc.textContent = "+";
      disc.title = host.t(rowAxis ? "table.insertRowHere" : "table.insertColumnHere");
    },
  };

  /** Inserts at the boundary the disc sits on, then says so.
   *
   *  AFTER the edit resolves, not beside it: `runEdit` is async and its
   *  `applyEditResult` writes the status line itself, so a synchronous
   *  announcement here was overwritten before a reader could see it — the live
   *  region is the only channel this affordance has. */
  async function commitInsert(target) {
    const doc = host.doc();
    const insert = target.axis === "row" ? doc.insertRow : doc.insertColumn;
    await host.runEdit(() => insert.call(doc, target.anchor, target.after), { gate: true });
    host.status(host.t(target.axis === "row" ? "table.rowInserted" : "table.columnInserted"));
  }

  /** The band under the pointer, ignoring the strip's own across-axis bound —
   *  a drag that wanders sideways off the strip must keep selecting. */
  function bandUnderPointer(page, event, axis) {
    if (!page) return null;
    const { x, y } = host.pointToTwip(page, event);
    for (const table of host.chromeOf(page)) {
      const bands = tableBands(table, axis);
      if (!bands.length) continue;
      const along = axis === "row" ? y : x;
      const band =
        along <= bands[0].start
          ? bands[0]
          : along >= bands[bands.length - 1].end
            ? bands[bands.length - 1]
            : bands.find((b) => along >= b.start && along < b.end);
      if (band) return { table, axis, index: band.i, anchor: band.anchor };
    }
    return null;
  }
}
