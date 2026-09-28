// The cell-selection store (`docs/141` D-3, the UI half).
//
// The properties that matter here are the ones a browser test cannot see: that
// the engine is asked ONCE per cell the pointer enters rather than once per
// pointer move, that a formatting write with no selection is the SAME path as
// one with a selection, and that an extension which hits a wall leaves the
// selection alone instead of destroying it.
import test from "node:test";
import assert from "node:assert/strict";

import { createTableRange } from "../src/table_range.mjs";

/** A stand-in engine over a `rows x cols` regular grid of cells named `r{c}`. */
function stubDoc({ rows = 3, cols = 3, expanded = false } = {}) {
  const at = (id) => {
    // `r:c` is a cell; `r:c#p` is the p-th PARAGRAPH of that same cell, which is
    // what a drag inside one multi-paragraph cell walks through.
    const m = /^(\d+):(\d+)(?:#\d+)?$/.exec(id);
    return m ? { r: Number(m[1]), c: Number(m[2]) } : null;
  };
  const calls = [];
  return {
    calls,
    tableCellRange(a, f) {
      calls.push(["tableCellRange", a, f]);
      const p = at(a);
      const q = at(f);
      if (!p || !q) return { found: false, reason: "caret is not inside a table", table: "" };
      const [r0, r1] = [Math.min(p.r, q.r), Math.max(p.r, q.r)];
      const [c0, c1] = [Math.min(p.c, q.c), Math.max(p.c, q.c)];
      return {
        found: true,
        table: "t1",
        firstRow: r0,
        lastRow: r1,
        firstColumn: c0,
        lastColumn: c1,
        cells: (r1 - r0 + 1) * (c1 - c0 + 1),
        expanded,
        reason: "",
      };
    },
    tableCellRangeRects: (a, f) => (calls.push(["rects", a, f]), [1, 0, 0, 10, 10]),
    tableCellRangeAnchorNodes: (a, f) => (calls.push(["anchors", a, f]), [a, f]),
    tableSelectionAnchorNodes(node, mode) {
      calls.push(["tableSelectionAnchorNodes", node, mode]);
      const p = at(node);
      if (!p) return [];
      if (mode === "row") return Array.from({ length: cols }, (_, c) => `${p.r}:${c}`);
      if (mode === "column") return Array.from({ length: rows }, (_, r) => `${r}:${p.c}`);
      if (mode === "table") {
        const out = [];
        for (let r = 0; r < rows; r++) for (let c = 0; c < cols; c++) out.push(`${r}:${c}`);
        return out;
      }
      return [];
    },
  };
}

function build(options = {}) {
  const doc = stubDoc(options);
  const said = [];
  const edits = [];
  const range = createTableRange({
    doc: () => doc,
    pages: () => [],
    scaleOf: () => ({ rect: { left: 0, top: 0 }, sx: 1, sy: 1 }),
    runEdit: (thunk) => {
      edits.push("runEdit");
      return thunk();
    },
    status: (text, kind) => said.push([text, kind ?? ""]),
    t: (key, params) => (params ? `${key}:${params.count}` : key),
  });
  return { doc, range, said, edits };
}

test("a row selection is the SAME rectangle type as a drag, from the engine's anchors", () => {
  // The whole point of D-3's type: three degenerate rectangles and a real one are
  // one thing. A row is the first and last cell of `tableSelectionAnchorNodes`,
  // never a second `{ node, mode }` representation.
  const { range, doc } = build();
  assert.equal(range.selectMode("1:1", "row"), "");
  assert.deepEqual(range.get(), { anchorNode: "1:0", focusNode: "1:2", mode: "row" });
  assert.equal(range.cellCount(), 3);
  assert.ok(doc.calls.some(([name, , mode]) => name === "tableSelectionAnchorNodes" && mode === "row"));
});

test("a column of a table that cannot express one refuses with the product's sentence", () => {
  const doc = stubDoc();
  doc.tableSelectionAnchorNodes = () => [];
  const range = createTableRange({
    doc: () => doc,
    pages: () => [],
    scaleOf: () => ({ rect: { left: 0, top: 0 }, sx: 1, sy: 1 }),
    runEdit: (thunk) => thunk(),
    status: () => {},
    t: (k) => k,
  });
  assert.equal(range.selectMode("0:0", "column"), "table.reason.merged");
  assert.equal(range.get(), null);
});

test("a drag asks the engine ONCE per cell entered, not once per pointer move", () => {
  // This is the performance guarantee of the gesture. Without the memo a drag
  // across a table costs one O(document) table lookup per frame.
  const { range, doc } = build();
  const gesture = {};
  for (let i = 0; i < 20; i++) assert.equal(range.dragTo("0:0", "1:1", gesture), true);
  const queries = doc.calls.filter(([name]) => name === "tableCellRange").length;
  assert.equal(queries, 1, `the engine was asked ${queries} times for one cell`);
  // Entering a second cell costs exactly one more.
  range.dragTo("0:0", "2:2", gesture);
  range.dragTo("0:0", "2:2", gesture);
  assert.equal(doc.calls.filter(([name]) => name === "tableCellRange").length, 2);
});

test("a drag that never leaves its cell is NOT a cell selection", () => {
  // One cell is a caret with a fill on it; the text path must keep the gesture.
  // The second case is the one that matters: the pointer DID move to another
  // paragraph, so the node changed, and the rectangle is still one cell.
  const { range } = build();
  const gesture = {};
  assert.equal(range.dragTo("0:0", "0:0", gesture), false);
  assert.equal(range.get(), null);
  assert.equal(range.dragTo("0:0", "0:0#2", gesture), false);
  assert.equal(range.get(), null);
});

test("formatting with no selection is the caret's own one-cell range, not a refusal", () => {
  // The old behaviour was `setStatus("…put the caret in the cell to format it")`.
  // The refusal is gone because one path now serves both cases.
  const { range, edits } = build();
  const seen = [];
  assert.equal(range.formatRange((a, f) => seen.push([a, f]), "2:1"), true);
  assert.deepEqual(seen, [["2:1", "2:1"]]);
  assert.deepEqual(edits, ["runEdit"]);

  range.selectMode("0:0", "row");
  assert.equal(range.formatRange((a, f) => seen.push([a, f]), "2:1"), true);
  assert.deepEqual(seen[1], ["0:0", "0:2"], "with a selection the RANGE's endpoints are used");
  assert.equal(edits.length, 2, "one action per call — therefore one undo entry");
});

test("an extension that hits a wall leaves the selection exactly as it was", () => {
  const { range, said } = build();
  range.selectMode("1:1", "row");
  const before = { ...range.get() };
  assert.equal(range.extendTo("not-a-cell"), false);
  assert.deepEqual(range.get(), before);
  assert.deepEqual(said, [], "a probe that finds a wall says nothing");
});

test("the announcement says how many cells, and says when the engine EXPANDED the rectangle", () => {
  const plain = build();
  plain.range.set("0:0", "1:1", "cells");
  plain.range.announce();
  assert.deepEqual(plain.said, [["table.cellsSelected:4", ""]]);

  const grown = build({ expanded: true });
  grown.range.set("0:0", "1:1", "cells");
  grown.range.announce();
  assert.equal(
    grown.said[0][0],
    "table.cellsSelected:4 — table.selectionExpanded",
    "an expanded rectangle must SAY it grew, or it reads as the editor selecting " +
      "more than was asked for",
  );
});

test("mergeable is two cells, which is what the refusal sentence now claims", () => {
  const { range } = build();
  range.set("0:0", "0:0", "cells");
  assert.equal(range.mergeable(), false);
  range.set("0:0", "0:1", "cells");
  assert.equal(range.mergeable(), true);
});
