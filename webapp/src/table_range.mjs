// The table CELL SELECTION — one rectangle of cells, and everything the editor
// asks of it (`docs/141` D-3, the UI half).
//
// ## One representation, not two
//
// The editor used to carry `tableSelection = { node, mode }`, where `mode` was
// one of three degenerate rectangles — a row, a column, or the whole table. A
// drag across two cells had no representation at all, so there was no way to
// say it, paint it, merge it or format it, and the cell-format controls refused
// with *"put the caret in the cell to format it"* precisely because this type
// did not exist.
//
// Everything is now ONE rectangle, `{ anchorNode, focusNode }`, two paragraph
// node ids the engine normalises into a cell rectangle. Row / column / table
// selection is that same rectangle with its endpoints taken from
// `tableSelectionAnchorNodes(node, mode)`, so there is a single painter, a
// single containment test, a single merge and a single formatting path. SKILL §8:
// *prefer one mechanism over two — when a design needs a parallel path, that is
// evidence the abstraction is wrong.*
//
// ## Competitive standard
//
// **Google Docs**: press in a cell and drag — the rectangle highlights, and it
// grows to swallow any merged cell it clips; Shift+Arrow extends it; formatting
// applies to every selected cell at once. **Word**: the same drag and the same
// Shift+Arrow, and it additionally allows a non-rectangular multi-select with
// Ctrl. **We followed Docs**: one rectangle, expanded over merges. Ctrl
// multi-select is deliberately out of scope — the engine's `CellRange` is a
// rectangle, and a second, discontiguous selection type would be exactly the
// parallel path the paragraph above refuses.
//
// ## Complexity
//
// Every engine query here is O(document) to find the table plus O(cells in the
// range) — the cost every table facade query already pays (`docs/141` TBL-15 is
// the row that fixes it for all of them at once). What this module adds is the
// MEMO: a pointer drag re-asks only when the endpoint pair changes, i.e. when
// the pointer crosses a cell edge, not on every move. The memo is dropped by
// `invalidate()` from the one overlay repaint, the same lifetime rule
// `table_chrome.mjs` uses for page geometry, because a repaint is the only thing
// that can change what a range covers.

/** The catalogue key per selection mode. Glueing a translated noun onto a fixed
 *  English verb is what a key per mode avoids. */
const MODE_STATUS = Object.freeze({
  row: "table.selectedRow",
  column: "table.selectedColumn",
  table: "table.selectedTable",
});

/**
 * Creates the cell-selection store bound to one editor host.
 *
 * @param {object} host
 * @param {() => object|null} host.doc          the live engine wrapper
 * @param {() => Array} host.pages              the page records
 * @param {(page) => {rect:DOMRect,sx:number,sy:number}} host.scaleOf
 * @param {(fn, options?) => unknown} host.runEdit
 * @param {(text:string, kind?:string) => void} host.status
 * @param {(key:string, params?:object) => string} host.t
 * @returns {object} the store
 */
export function createTableRange(host) {
  /** `{ anchorNode, focusNode, mode }` or `null`. `mode` is only a LABEL for the
   *  announcement — "row", "column", "table" or "cells" — never a second
   *  representation of the rectangle. */
  let selection = null;
  /** The memoised `tableCellRange` answer for `selection`, or `null`. */
  let memo = null;
  /** The memoised flat rects for `selection`, or `null`. */
  let memoRects = null;
  /** The rectangle the accessibility mirror was last built for. */
  let mirrored = "";

  /** Reads `tableCellRange` at most once per endpoint pair per repaint. */
  function info() {
    if (!selection) return null;
    if (memo) return memo;
    const doc = host.doc();
    if (!doc?.tableCellRange) return null;
    const answer = doc.tableCellRange(selection.anchorNode, selection.focusNode);
    // The wasm getter object is copied into a plain record: the caller may hold
    // it past the next edit, and a plain record cannot be dangling.
    memo = {
      found: !!answer.found,
      table: answer.table,
      firstRow: answer.firstRow,
      lastRow: answer.lastRow,
      firstColumn: answer.firstColumn,
      lastColumn: answer.lastColumn,
      cells: answer.cells,
      expanded: !!answer.expanded,
      reason: answer.reason,
    };
    answer.free?.();
    return memo;
  }

  /** The flat `[page, x, y, w, h, …]` rectangles, memoised alongside the info. */
  function rects() {
    if (!selection) return [];
    if (memoRects) return memoRects;
    const doc = host.doc();
    memoRects = doc?.tableCellRangeRects
      ? doc.tableCellRangeRects(selection.anchorNode, selection.focusNode)
      : [];
    return memoRects;
  }

  /** The endpoint pair for a `"row"`/`"column"`/`"table"` selection around
   *  `node`, from the engine's own anchor list — never re-derived here, which is
   *  the whole reason `tableSelectionAnchorNodes` exists. `null` when the mode
   *  cannot produce one (a column of a merged table). */
  function endpointsForMode(node, mode) {
    const doc = host.doc();
    const anchors = doc?.tableSelectionAnchorNodes?.(node, mode) ?? [];
    if (anchors.length === 0) return null;
    return { anchorNode: anchors[0], focusNode: anchors[anchors.length - 1] };
  }

  return {
    /** Drops the memo. Called from the one overlay repaint, for the same reason
     *  `table_chrome.mjs` drops its page cache there. */
    invalidate() {
      memo = null;
      memoRects = null;
    },

    /** The selection record, or `null`. */
    get() {
      return selection;
    },

    /** The selection plus the engine's reading of it — what a command tree needs
     *  to decide enablement without a second `tableInfo` walk of its own. The
     *  range already knows which table it is in, which is the question
     *  `plainTableInfo(tableSelection.node)?.table` used to cost a document walk
     *  to answer. */
    descriptor() {
      if (!selection) return null;
      const range = info();
      if (!range?.found) return null;
      return {
        anchorNode: selection.anchorNode,
        focusNode: selection.focusNode,
        mode: selection.mode,
        table: range.table,
        cells: range.cells,
        expanded: range.expanded,
      };
    },

    /** Whether a cell selection exists AND covers more than one cell — the
     *  precondition merge states in its own refusal. */
    mergeable() {
      return (info()?.cells ?? 0) > 1;
    },

    /** How many cells are selected (0 when there is no selection). */
    cellCount() {
      return info()?.cells ?? 0;
    },

    /** Whether the engine grew the rectangle past the gesture to swallow whole
     *  merged regions. */
    expanded() {
      return info()?.expanded === true;
    },

    /** Forgets the selection. */
    clear() {
      selection = null;
      memo = null;
      memoRects = null;
    },

    /** Sets the rectangle from two cell endpoints. `mode` labels it for the
     *  announcement only. Returns whether a rectangle was produced.
     *
     *  `quiet` suppresses the engine's refusal sentence, for the callers that
     *  PROBE — Shift+Arrow asks "is there a cell that way?" and a wall is an
     *  answer, not an error worth a toast. */
    set(anchorNode, focusNode, mode = "cells", quiet = false) {
      if (!anchorNode || !focusNode) return false;
      selection = { anchorNode, focusNode, mode };
      memo = null;
      memoRects = null;
      const range = info();
      if (!range?.found) {
        const reason = range?.reason;
        selection = null;
        memo = null;
        if (reason && !quiet) host.status(reason, "warn");
        return false;
      }
      return true;
    },

    /** Moves the focus end, keeping the anchor — what a drag and Shift+Arrow
     *  both do. A focus the engine will not accept (the probe walked out of the
     *  table) leaves the selection exactly as it was: an extension that finds a
     *  wall must not destroy what the user already had. */
    extendTo(focusNode) {
      if (!selection) return false;
      const previous = selection;
      if (this.set(selection.anchorNode, focusNode, "cells", true)) return true;
      selection = previous;
      memo = null;
      memoRects = null;
      return false;
    },

    /** Selects the whole row / column / table around `node`.
     *
     *  Returns the i18n key of the refusal, or `""`. A column of a merged table
     *  has no anchor list, so it refuses with the catalogue's existing sentence
     *  rather than painting an empty selection. */
    selectMode(node, mode) {
      const ends = endpointsForMode(node, mode);
      if (!ends) return "table.reason.merged";
      if (!this.set(ends.anchorNode, ends.focusNode, mode)) return "table.reason.merged";
      return "";
    },

    /** The endpoint of a whole row/column, used to square off a strip drag: the
     *  FIRST cell for the anchor end, the LAST for the focus end. */
    endpointOf(node, mode, end) {
      const ends = endpointsForMode(node, mode);
      if (!ends) return "";
      return end === "last" ? ends.focusNode : ends.anchorNode;
    },

    /** The sentence the status bar says for the current selection.
     *
     *  Two facts, in one line: how many cells, and — only when it happened —
     *  that the rectangle GREW past the gesture to contain whole merged cells.
     *  Saying that is what stops an expanded selection reading as a bug
     *  (`docs/141` §4.3.5). */
    announce() {
      const range = info();
      if (!range?.found) return;
      const named = MODE_STATUS[selection.mode];
      const text = named
        ? host.t(named)
        : host.t("table.cellsSelected", { count: range.cells });
      host.status(range.expanded ? `${text} — ${host.t("table.selectionExpanded")}` : text);
    },

    /** One anchor paragraph per selected cell — what a per-cell command needs.
     *  Not memoised: only the commands that iterate cells ask for it. */
    anchorNodes() {
      if (!selection) return [];
      const doc = host.doc();
      return doc?.tableCellRangeAnchorNodes?.(selection.anchorNode, selection.focusNode) ?? [];
    },

    /**
     * Extends the selection one cell in `dir` (`"up"`/`"down"`/`"left"`/
     * `"right"`) — the keyboard twin of the drag.
     *
     * The neighbour is found by PROBING the layout just outside the focus cell's
     * painted box rather than by index arithmetic over the grid. That is what
     * makes it correct on a merged table, where "the cell below row 2, column 1"
     * is not a grid coordinate at all, and it costs one page-scoped hit test
     * instead of a walk of the table's cells. A probe that lands outside the
     * table is a WALL: the key is consumed, the selection is untouched, and
     * nothing is said.
     *
     * Returns whether the key was consumed. `false` means "not a cell move at
     * all" and the caller's ordinary text extension runs.
     */
    extendByStep(dir) {
      const step = { up: [0, -1], down: [0, 1], left: [-1, 0], right: [1, 0] }[dir];
      const doc = host.doc();
      if (!step || !selection || !doc) return false;
      const rect = doc.cellRect(selection.focusNode); // [page, x, y, w, h] or []
      if (rect.length < 5) return false;
      const [page, x, y, w, h] = rect;
      const px = step[0] === 0 ? x + Math.floor(w / 2) : step[0] < 0 ? x - 1 : x + w + 1;
      const py = step[1] === 0 ? y + Math.floor(h / 2) : step[1] < 0 ? y - 1 : y + h + 1;
      const hit = px < 0 || py < 0 ? null : doc.hitTest(page, px, py);
      const node = hit?.node ?? "";
      hit?.free?.();
      if (!node || node === selection.focusNode) return true;
      this.extendTo(node);
      return true;
    },

    /**
     * Turns a drag that left its starting cell into a rectangular selection.
     * Returns whether the drag is a CELL drag, so the caller can skip the text
     * path.
     *
     * `gesture` is the caller's own pointer-gesture record, used as the memo:
     * the engine is asked once per CELL the pointer enters, not once per pointer
     * move. That distinction is the whole performance story of this gesture.
     */
    dragTo(press, focusNode, gesture) {
      if (!press || !focusNode || !gesture) return false;
      if (focusNode !== gesture.lastCellProbe) {
        gesture.lastCellProbe = focusNode;
        gesture.cellRange =
          focusNode !== press &&
          this.set(press, focusNode, "cells", true) &&
          this.cellCount() > 1;
        if (!gesture.cellRange) this.clear();
        else this.announce();
      }
      return gesture.cellRange === true;
    },

    /** Whether the selected RECTANGLE has changed since this was last asked —
     *  the trigger for rebuilding the accessibility mirror, which is O(window)
     *  and must not run on every caret move. O(1) with no selection, one memoised
     *  engine read with one. */
    mirrorChanged() {
      const rect = this.gridRect();
      const key = rect
        ? `${rect.firstRow}:${rect.lastRow}:${rect.firstColumn}:${rect.lastColumn}`
        : "";
      if (key === mirrored) return false;
      mirrored = key;
      return true;
    },

    /** The flat rectangles, for the painter and the containment test. */
    rects,

    /** Whether the client point falls inside the painted selection — the test a
     *  click uses to decide whether it is confirming the selection or replacing
     *  it. O(1) when nothing is selected. */
    containsClientPoint(clientX, clientY) {
      if (!selection) return false;
      const flat = rects();
      const pages = host.pages();
      for (let i = 0; i + 4 < flat.length; i += 5) {
        const [pageNumber, x, y, width, height] = flat.slice(i, i + 5);
        const page = pages[pageNumber - 1];
        if (!page) continue;
        const { rect, sx, sy } = host.scaleOf(page);
        if (
          clientX >= rect.left + x * sx &&
          clientX <= rect.left + (x + width) * sx &&
          clientY >= rect.top + y * sy &&
          clientY <= rect.top + (y + height) * sy
        ) {
          return true;
        }
      }
      return false;
    },

    /** The selected rectangle in grid coordinates, for the accessibility mirror,
     *  or `null`. */
    gridRect() {
      const range = info();
      if (!range?.found) return null;
      return {
        firstRow: range.firstRow,
        lastRow: range.lastRow,
        firstColumn: range.firstColumn,
        lastColumn: range.lastColumn,
      };
    },

    /**
     * Runs one of the three RANGE cell-property writes over the selection, or
     * over the caret's own cell when nothing is selected.
     *
     * `apply(anchorNode, focusNode)` is the engine call. Passing the same node
     * twice is a one-cell range, so the caret case is the same code path rather
     * than a second one — which is what removes the old refusal instead of
     * papering over it. ONE action, therefore ONE undo entry for the whole
     * block.
     */
    formatRange(apply, caretNode) {
      const ends = selection
        ? [selection.anchorNode, selection.focusNode]
        : [caretNode, caretNode];
      if (!ends[0]) return false;
      host.runEdit(() => apply(ends[0], ends[1]), { gate: true });
      return true;
    },
  };
}
