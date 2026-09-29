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
// ## What is painted, and when — and why the table has to be ENTERED first
//
// Only the ONE table the pointer is over, only while it is over it, and only
// the segment of the strip the pointer is in. `docs/141` §4.4 rule 2: no
// resting chrome, one armed overlay child per gesture, never a handle per
// boundary at rest. The DOM is rebuilt only when the armed key changes — the
// table, the axis, the band, whether a `+` is armed — so travelling along a
// strip inside one band costs no DOM work at all.
//
// The gutter ARMS by the pointer entering the table, and only then does the
// strip beside it become a target. That is Docs' behaviour and it is not
// cosmetic: the column strip is a 14px band directly above the table, and a
// paragraph frequently sits in it. Arming on proximity alone made hovering the
// line above a table report a column strip — and a press there would have
// selected a column instead of placing the caret in that paragraph, which is the
// exact failure the boundary zones had to be clamped for on the other axis. The
// pointer-cursor sweep caught it: a form checkbox one line above a table stopped
// being a control.
//
// ## Drag-to-REORDER (`docs/141` §4.2.3)
//
// `moveTableRow(node, from, to)` and `moveTableColumn(...)` now exist — one
// `Operation::ReplaceTable` under `HistoryKind::TableStructure`, so a whole drag
// is ONE undo step — and this is the gesture on top of them.
//
// **Which gesture, and from which product.** *Word* has no drag-reorder on the
// gutter at all: you select a row and move it with Alt+Shift+Up/Down. *Docs*
// reorders by dragging a strip **that is already selected**, which is also how
// it tells reorder apart from the select-drag that shares the same 14px strip.
// **We followed Docs for the pointer and Word for the keyboard**: the strip
// reorders once its band is the selected one, and `table.move.*` carries the
// same capability to the menu, the palette and a chord — so the gesture is not
// mouse-only capability, which is the rule every other gesture in this layer
// already follows.
//
// One band moves, never several. The facade moves one row per call, and N calls
// is N undo entries — so a multi-band selection keeps the select-drag rather
// than committing something the word "one undo step" would stop describing.
//
// **The refusals are the feature, not the edge case.** A row inside a `vMerge`
// run, a drop that would land between a `restart` and its `continue`, a column
// whose grid index names no cell because of `gridSpan`, and a drop back where it
// started are all refused BY THE ENGINE with a sentence naming the row. Those
// sentences reach the status line verbatim: `runEdit` translates engine errors
// into one generic sentence (`edit_errors.mjs`), which is right for internal
// vocabulary and wrong for a refusal that already names the row the user is
// looking at. So this module catches the throw itself and says what it said.
//
// **Cost.** ONE `tableInfo` call, at the press, converts the page-local band
// index into the model index the facade wants; every pointer move after that is
// arithmetic over the same memoised page chrome the rest of this layer reads.
// A per-move `tableInfo` would be the quadratic the chrome layer removed.

import {
  STRIP_PX,
  TOUCH_STRIP_PX,
  INSERT_TARGET_PX,
  armTableGutter,
  dropBoundaryAt,
  moveTargetIndex,
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
  /** What the pointer is over, or `null`: the whole paint is a function of it.
   *  `{ key, page, tableNode, target }`, where `target` is the strip band or
   *  insert disc under the pointer and is `null` while the pointer is simply
   *  inside the table. */
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

  /** The table whose own box contains the point, or `null`. This is what ARMS
   *  the gutter: a strip is a target only for a table the pointer has entered. */
  function tableUnder(page, event) {
    if (!page) return null;
    const { x, y } = host.pointToTwip(page, event);
    return (
      host.chromeOf(page).find((t) => x >= t.x && x < t.x + t.w && y >= t.y && y < t.y + t.h) ??
      null
    );
  }

  /** The identity of what is armed, so the DOM is rebuilt only when it changes. */
  function keyOf(pageNumber, tableNode, target) {
    if (!tableNode) return "";
    return [pageNumber, tableNode, target?.axis ?? "", target?.kind ?? "", target?.index ?? ""].join(
      "/",
    );
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

    /**
     * Moves the caret's row or column one step — the command twin of the drag,
     * and what puts reordering on the menu, the context menu and the palette
     * rather than leaving it mouse-only.
     *
     * `sign` is -1 (up / left) or +1 (down / right). `to` is the post-move
     * index, which for a one-step nudge is simply `from + sign`; the engine
     * refuses a step off either end, and refuses the merge cases by name.
     *
     * Complexity: ONE `tableInfo` read per invocation — a command, not a
     * keystroke — and one engine edit.
     */
    moveBand(node, axis, sign) {
      if (!node) return false;
      const grid = gridAt(node);
      const from = axis === "row" ? grid?.row : grid?.column;
      if (from == null) {
        host.status(host.t("table.reason.caretOutsideTable"), "warn");
        return true;
      }
      void commitReorder({ axis, anchor: node, from }, from + sign);
      return true;
    },

    /** The gesture in flight as `pointer_cursor.mjs` names it, or `""`. The
     *  cursor has to say which of the two strip drags this is: one is selecting
     *  (`cell`, the strip's own shape) and one is carrying a row to a new place
     *  (`grabbing`), and a pointer that outruns the strip must not lose either. */
    dragKind() {
      if (!drag) return "";
      return drag.kind === "move" ? "table-strip-move" : "table-strip-select";
    },

    /** What the cursor router should be told: `"row"`, `"column"`, `"insert"`,
     *  or `""`. The strips are real elements with their own CSS cursors, so this
     *  exists for the frame in which the pointer has entered the gutter and the
     *  strip has not been painted yet. */
    targetKind(page, event) {
      const target = armedTargetAt(page, event);
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
      const inside = tableUnder(page, event);
      const target = inside ? null : armedTargetAt(page, event);
      const tableNode = inside?.node ?? target?.table.node ?? "";
      const touch = event?.pointerType === "touch";
      const key = `${keyOf(page?.pageNumber, tableNode, target)}${touch ? "/t" : ""}`;
      if (key === (armed?.key ?? "")) return !!target;
      armed = tableNode ? { key, page, tableNode, target, touch } : null;
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
      // Only a strip of the ARMED table takes a press. Without that condition a
      // press in the 14px band above any table would select a column instead of
      // placing the caret in the paragraph that lives there.
      if (!armed?.tableNode || tableUnder(page, event)) {
        // A TOUCH tap inside the table ARMS the gutter, because hover — the thing
        // every branch above depends on — does not exist on touch. The tap still
        // places the caret: it is the arming tap, not the gesture, which is the
        // rule the boundary pills already follow (`docs/141` TBL-18). Without it
        // the whole gutter is unreachable from a finger.
        if (event.pointerType === "touch" && tableUnder(page, event)) this.hover(page, event);
        return false;
      }
      const { target, refusal } = probe(page, event);
      if (!target || target.table.node !== armed.tableNode) {
        if (refusal) host.status(host.t(refusal), "warn");
        return false;
      }
      event.preventDefault();
      event.stopPropagation();
      if (target.kind === "insert") {
        void commitInsert(target);
        return true;
      }
      // Docs' rule: a strip whose band is ALREADY the selection reorders it. A
      // press that has to select first cannot also be a move, or a single click
      // on an unselected row would arm a gesture the user has not asked for.
      const reorder = beginReorder(page, target);
      if (reorder) {
        drag = reorder;
        event.currentTarget?.setPointerCapture?.(event.pointerId);
        host.repaintOverlay();
        return true;
      }
      const mode = AXIS[target.axis].mode;
      const reason = host.range.selectMode(target.anchor, mode);
      if (reason) {
        host.status(host.t(reason), "warn");
        return true;
      }
      drag = {
        kind: "select",
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
      if (drag.kind === "move") {
        moveReorder(event);
        return;
      }
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

    /** Ends the strip drag: commits the reorder, or announces what the select
     *  drag selected. */
    finishDrag(event) {
      if (!drag) return false;
      const ending = drag;
      drag = null;
      event?.preventDefault?.();
      if (ending.kind === "move") {
        void commitReorder(ending, moveTargetIndex(ending.from, ending.pageOrigin + ending.boundary));
        return true;
      }
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
      const table = host.chromeOf(page).find((t) => t.node === armed.tableNode);
      if (!table) return;
      const { sx, sy } = host.scaleOf(page);
      // BOTH strips, for the armed table. A user who has entered the table can
      // then travel to either one; painting only the axis the pointer already
      // reached would mean the strips only appear once you have found them.
      // The column strip is omitted on a merged table, where a column selection
      // refuses — a painted strip that refuses on click is a dead control.
      const thickness = defaultThickness(page);
      for (const axis of ["row", "column"]) {
        if (axis === "column" && !table.regular) continue;
        const rect = stripRect(table, axis, axis === "row" ? thickness.x : thickness.y);
        if (!rect) continue;
        child(page, AXIS[axis].strip, rect.x * sx, rect.y * sy, rect.w * sx, rect.h * sy);
        // The selected rows or columns keep an ACCENT bar whether or not the
        // pointer is on them. That is what makes colour mean SELECTION here: a
        // strip is grey because it exists and accent because it is chosen, which
        // is Docs' rule and the reason the resting strip could go neutral.
        // ONE box over the whole selected span rather than one per band — a row
        // or column selection is contiguous, so the union is exact and the
        // element count does not grow with the table.
        const span = selectedSpan(page, table, axis);
        if (!span) continue;
        const alongY = axis === "row";
        const bar = child(
          page,
          "table-gutter-selection",
          alongY ? rect.x * sx : span.lo * sx,
          alongY ? span.lo * sy : rect.y * sy,
          alongY ? rect.w * sx : (span.hi - span.lo) * sx,
          alongY ? (span.hi - span.lo) * sy : rect.h * sy,
        );
        bar.dataset.axis = axis;
      }
      paintDropIndicator(page, table, sx, sy);
      if (!target) return;
      const rowAxis = target.axis === "row";
      const strip = stripRect(table, target.axis, rowAxis ? target.strip.w : target.strip.h);
      if (!strip) return;
      const bands = tableBands(table, target.axis);
      if (target.kind === "strip") {
        const band = bands.find((b) => b.i === target.index);
        if (!band) return;
        const el = child(
          page,
          "table-gutter-band",
          rowAxis ? strip.x * sx : band.start * sx,
          rowAxis ? band.start * sy : strip.y * sy,
          rowAxis ? strip.w * sx : band.extent * sx,
          rowAxis ? band.extent * sy : strip.h * sy,
        );
        // The bar is drawn by CSS against the leading edge of the strip, so it
        // has to know which edge that is; and a hovered band that is ALSO the
        // selection stays accent rather than reverting to the hover grey.
        el.dataset.axis = target.axis;
        if (inSpan(selectedSpan(page, table, target.axis), band)) el.dataset.selected = "1";
        // A band that IS the selection is a handle, and says so with `grab`.
        // Not while a reorder is in flight: the band being carried keeps the
        // `grabbing` the router holds for the whole drag.
        if (!drag && !host.state().editsBlocked && movableBand(page, table, target.axis, target)) {
          el.dataset.move = "ready";
          el.title = host.t(rowAxis ? "table.dragToMoveRow" : "table.dragToMoveColumn");
        }
        return;
      }
      const at = target.at;
      const size = discSize();
      const disc = child(
        page,
        "table-insert-target",
        rowAxis ? strip.x * sx + (strip.w * sx - size) / 2 : at * sx - size / 2,
        rowAxis ? at * sy - size / 2 : strip.y * sy + (strip.h * sy - size) / 2,
        size,
        size,
      );
      disc.dataset.insert = target.axis;
      disc.textContent = "+";
      disc.title = host.t(rowAxis ? "table.insertRowHere" : "table.insertColumnHere");
    },
  };

  /** The grid coordinates of the cell holding `node`, or `null`.
   *
   *  The ONE document read a reorder costs. `tableChromeOnPage` numbers its row
   *  edges per PAGE, so on the second page of a split table band 0 is not row 0
   *  of the model — and the facade indexes the model. Asked once, at the press,
   *  and turned into a page-to-model offset the whole drag then reuses.
   *
   *  Complexity: O(document) for this one call. Never called from `hover`,
   *  `moveDrag` or `paint`. */
  function gridAt(node) {
    const doc = host.doc();
    if (!doc?.tableInfo) return null;
    const info = doc.tableInfo(node);
    const value = info?.found ? { row: info.row, column: info.column } : null;
    info?.free?.();
    return value;
  }

  /**
   * Whether a band is the one the current selection covers — the precondition
   * for the reorder gesture, and for the `grab` cursor that advertises it.
   *
   * Answered in PAGE space, against the rectangles the selection painter already
   * holds, and never by asking the engine which row is selected: this runs from
   * `paint`, which runs on every overlay repaint, so a document read here would
   * put an O(document) call on the repaint path. `gridRect` is the range's own
   * memo and costs nothing new.
   *
   * Complexity: O(selected cells).
   */
  function movableBand(page, table, axis, target) {
    if (target?.kind !== "strip" || !page?.pageNumber) return false;
    const selection = host.range.descriptor();
    if (!selection || selection.mode !== AXIS[axis].mode || selection.table !== table.node) {
      return false;
    }
    const rect = host.range.gridRect();
    if (!rect) return false;
    // Several bands selected: one facade call moves one band, and N calls is N
    // undo entries, so the strip keeps its select-drag rather than committing
    // something "one undo step" would stop describing.
    const single = axis === "row" ? rect.firstRow === rect.lastRow : rect.firstColumn === rect.lastColumn;
    if (!single) return false;
    const bands = tableBands(table, axis);
    const band = bands.find((b) => b.i === target.index);
    if (!band) return false;
    return inSpan(selectedSpan(page, table, axis), band);
  }

  /**
   * The extent of the current row or column selection along `axis`, in
   * page-local twips, or `null` when the selection is not one of this table's
   * bands on this page.
   *
   * Answered in PAGE space from the rectangles the selection painter already
   * holds, and never by asking the engine which row is selected: this runs from
   * `paint`, which runs on every overlay repaint, so a document read here would
   * put an O(document) call on the repaint path. A row or column selection is
   * contiguous, so the UNION of its rectangles is its extent and one box paints
   * it — the element count does not follow the selection's size.
   *
   * Complexity: O(selected cells on this page). No document read.
   */
  function selectedSpan(page, table, axis) {
    if (!page?.pageNumber) return null;
    const selection = host.range.descriptor();
    if (!selection || selection.mode !== AXIS[axis].mode || selection.table !== table.node) {
      return null;
    }
    const flat = host.range.rects();
    let lo = Infinity;
    let hi = -Infinity;
    for (let i = 0; i + 4 < flat.length; i += 5) {
      const [pageNumber, x, y, width, height] = flat.slice(i, i + 5);
      if (pageNumber !== page.pageNumber) continue;
      const start = axis === "row" ? y : x;
      const extent = axis === "row" ? height : width;
      lo = Math.min(lo, start);
      hi = Math.max(hi, start + extent);
    }
    return hi > lo ? { lo, hi } : null;
  }

  /** Whether a band's MIDDLE lies inside a selected span — the midpoint rather
   *  than an overlap test, so a selection rectangle that rounds a twip past a
   *  boundary never claims the neighbouring band. Complexity O(1). */
  function inSpan(span, band) {
    if (!span || !band) return false;
    const middle = (band.start + band.end) / 2;
    return middle >= span.lo && middle < span.hi;
  }

  /** Starts a reorder if this press is one, or `null` if it is a select drag.
   *
   *  The two preconditions are Docs' own: the pressed band is exactly the band
   *  the current selection covers, and the mode allows a structural edit. A
   *  refusal the MODE is responsible for is said here, because the strip is
   *  still a legal select-drag afterwards and a user who pressed a selected row
   *  meant to move it. */
  function beginReorder(page, target) {
    if (!movableBand(page, target.table, target.axis, target)) return null;
    const state = host.state();
    if (state.editsBlocked) return null; // Viewing: the strip still selects, silently
    if (state.geometryBlocked) {
      host.status(host.t("table.reason.notTracked"), "warn");
      return null;
    }
    const grid = gridAt(target.anchor);
    const from = target.axis === "row" ? grid?.row : grid?.column;
    if (from == null) return null;
    return {
      kind: "move",
      page,
      axis: target.axis,
      anchor: target.anchor,
      tableNode: target.table.node,
      // The model index of the FIRST band painted on this page, so every later
      // boundary converts with one addition and no further document read.
      pageOrigin: from - target.index,
      from,
      boundary: target.index,
      moved: false,
    };
  }

  /** Tracks the drop boundary under the pointer and repaints the indicator.
   *
   *  The drop is clamped to the page the press started on: the strip belongs to
   *  one page's chrome, and a boundary index read off another page's bands would
   *  name a different row of the model. A table that continues overleaf is
   *  reordered from the page the row is on, which is the page the user is
   *  looking at. */
  function moveReorder(event) {
    const page = host.pageFromClientPoint(event.clientX, event.clientY);
    if (page && page.pageNumber !== drag.page.pageNumber) return;
    const table = host.chromeOf(drag.page).find((t) => t.node === drag.tableNode);
    if (!table) return;
    const bands = tableBands(table, drag.axis);
    const { x, y } = host.pointToTwip(drag.page, event);
    const boundary = dropBoundaryAt(bands, drag.axis === "row" ? y : x);
    if (boundary === drag.boundary && drag.moved) return;
    drag.boundary = boundary;
    drag.moved = true;
    host.repaintOverlay();
  }

  /** The drop line, while a reorder is in flight.
   *
   *  A 2px rule across the table at the boundary the drop would use — Docs'
   *  indicator — plus the strip's own band highlight, which the armed target
   *  already paints on the row being carried. Painted from the repaint, so it
   *  survives the overlay being rebuilt mid-drag. */
  function paintDropIndicator(page, table, sx, sy) {
    if (drag?.kind !== "move" || drag.tableNode !== table.node) return;
    if (page.pageNumber !== drag.page.pageNumber) return;
    const bands = tableBands(table, drag.axis);
    if (!bands.length) return;
    const k = Math.min(Math.max(drag.boundary, 0), bands.length);
    const at = k === bands.length ? bands[bands.length - 1].end : bands[k].start;
    const rowAxis = drag.axis === "row";
    const line = child(
      page,
      "table-move-indicator",
      rowAxis ? table.x * sx : at * sx,
      rowAxis ? at * sy : table.y * sy,
      rowAxis ? table.w * sx : 0,
      rowAxis ? 0 : table.h * sy,
    );
    line.dataset.axis = drag.axis;
  }

  /** Commits the reorder, and says what the engine said when it refuses.
   *
   *  `runEdit` maps a thrown engine error onto one generic sentence, which is
   *  correct for `Unsupported`/`CrossParagraph` and wrong for these four: the
   *  engine already wrote a sentence naming the row and what to do about it. So
   *  the throw is caught here and re-announced AFTER `runEdit` has written its
   *  own, which is the ordering that leaves the specific sentence on screen. */
  async function commitReorder(gesture, to) {
    const doc = host.doc();
    let refusal = "";
    const move = gesture.axis === "row" ? doc.moveTableRow : doc.moveTableColumn;
    await host.runEdit(
      () => {
        try {
          return move.call(doc, gesture.anchor, gesture.from, to);
        } catch (error) {
          refusal = String(error?.message ?? error ?? "");
          throw error;
        }
      },
      { gate: true },
    );
    if (refusal) {
      host.status(refusal, "warn");
      host.repaintOverlay();
      return;
    }
    host.status(
      host.t(gesture.axis === "row" ? "table.rowMoved" : "table.columnMoved", {
        from: gesture.from + 1,
        to: to + 1,
      }),
    );
    host.range.clear();
    host.repaint();
  }

  /** The gutter target under the pointer, but only for the table the gutter is
   *  armed on. `null` everywhere else, so neither the cursor router nor the press
   *  path can claim a strip that is not on screen. */
  function armedTargetAt(page, event) {
    if (!armed?.tableNode) return null;
    const { target } = probe(page, event);
    return target && target.table.node === armed.tableNode ? target : null;
  }

  /** The strip thickness for the resting paint, in page-local twips. The paint
   *  has no pointer event of its own, so the armed record carries whether a
   *  finger put it there — and a finger gets the 24px strip WCAG 2.5.8 asks for,
   *  which costs no cell area because the strip is outside the table. */
  function defaultThickness(page) {
    const { sx, sy } = host.scaleOf(page);
    const px = armed?.touch ? TOUCH_STRIP_PX : STRIP_PX;
    return { x: px / (sx || 1), y: px / (sy || 1) };
  }

  /** The insert disc's diameter in CSS px — 24 under a finger, for the same
   *  reason and at the same cost. */
  function discSize() {
    return armed?.touch ? TOUCH_STRIP_PX : INSERT_TARGET_PX;
  }

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
