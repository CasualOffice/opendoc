// The `table.*` command tree, driven for real.
//
// The reason this file can exist is the extraction (`docs/141`; the ratchet note
// in `module_seams.test.mjs`): the tree used to reach the editor through bindings
// it closed over inside `main.js`, so its most important property could only be
// checked in a browser with a merged table on screen.
//
// That property is TBL-02, and it is ARITY. `splitMergedCell(node, rows, columns)`
// reaches the engine's real unmerge — `fn split_table_cell`, which clears
// `grid_span`/`vertical_merge` across a span and back-fills the vacated cells —
// only when BOTH counts are absent:
//
//     if requested_rows == 0 && requested_columns == 0 {
//         return split_table_cell(table, row_index, col_index, ids);
//     }
//
// The webapp's only call site always passed two integers, and the engine refuses
// 1x1 ("choose more than one row or column to split the cell"), so there was no
// value a person could type that unmerged a cell. A guard that only asserted
// "Unmerge exists and returned Ok" would have passed on the broken shape. This one
// asserts the ARGUMENT LIST the facade receives.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { tableToolCommands } from "../src/table_commands.mjs";
import { flattenCommandTree } from "../src/command_taxonomy.mjs";
import { setCatalogue, setLocale, resetI18n } from "../src/i18n.mjs";

// The real English catalogue, so the reasons this test reads are the sentences the
// product shows rather than bare keys. `t()` returns the key when nothing is
// registered, and a guard that compared keys to keys would pass with every
// sentence missing from `en_strings.mjs`.
const english = JSON.parse(readFileSync(new URL("../locales/en.json", import.meta.url), "utf8"));
resetI18n();
setCatalogue("en", english);
setLocale("en");

/** A stand-in engine that records every call with its FULL argument list — the
 *  `arguments.length` is the thing under test, so nothing here may normalise it. */
function stubDoc() {
  const calls = [];
  const record = (name) =>
    function (...args) {
      calls.push([name, args]);
      return { ok: true };
    };
  return {
    calls,
    insertRow: record("insertRow"),
    insertColumn: record("insertColumn"),
    deleteRow: record("deleteRow"),
    deleteColumn: record("deleteColumn"),
    deleteTable: record("deleteTable"),
    distributeTableRows: record("distributeTableRows"),
    distributeTableColumns: record("distributeTableColumns"),
    sortTable: record("sortTable"),
    mergeTableSelection: record("mergeTableSelection"),
    splitMergedCell: record("splitMergedCell"),
  };
}

function build({ regular = true, suggesting = false, tableSelection = null, rowHeightRule = "auto" } = {}) {
  const doc = stubDoc();
  const host = {
    doc: () => doc,
    tableSelection: () => tableSelection,
    clearTableSelection: () => doc.calls.push(["clearTableSelection", []]),
    plainTableInfo: () => ({ found: true, regular, table: "t1" }),
    runEdit: (thunk) => {
      doc.calls.push(["runEdit", []]);
      return thunk();
    },
    selectTableContext: (node, mode) => doc.calls.push(["selectTableContext", [node, mode]]),
    openSplitCellDialog: () => doc.calls.push(["openSplitCellDialog", []]),
    openCellFormat: () => doc.calls.push(["openCellFormat", []]),
    openTableProperties: () => doc.calls.push(["openTableProperties", []]),
  };
  const context = {
    anchor: { node: "cell-7", offset: 0 },
    table: { found: true, regular, rowHeightRule, table: "t1", column: 2 },
    suggesting,
  };
  const tree = tableToolCommands(context, host);
  const rows = flattenCommandTree(tree, () => ({ label: "", group: "", kw: "" }));
  return { doc, tree, rows, row: (id) => rows.find((entry) => entry.id === id) };
}

test("Unmerge calls splitMergedCell with ONE argument — the branch that unmerges", () => {
  const { doc, row } = build({ regular: false });
  const unmerge = row("table.unmerge");
  assert.ok(unmerge, "there must be a table.unmerge command at all");
  assert.equal(unmerge.enabled, true, "a table holding a merge must offer Unmerge");

  unmerge.run();

  const call = doc.calls.find(([name]) => name === "splitMergedCell");
  assert.ok(call, "Unmerge must reach the facade");
  // THE ASSERTION. Two integers is the shape that cannot unmerge, and the shape
  // every pre-existing call site used.
  assert.deepEqual(
    call[1],
    ["cell-7"],
    "splitMergedCell must be called with the node ALONE. Any second or third " +
      "argument lands in split_table_cell_counts' split path, which refuses 1x1 " +
      "and can never unmerge (docs/141 TBL-02)",
  );
  assert.equal(call[1].length, 1, "arity is the whole point: `undefined` passed explicitly is not None");
});

test("Unmerge is refused with a reason on a table that has no merge", () => {
  const { row } = build({ regular: true });
  const unmerge = row("table.unmerge");
  assert.equal(unmerge.enabled, false);
  assert.equal(
    unmerge.disabledReason,
    "This table has no merged cells",
    "never a dead control: a regular grid says why rather than failing at the engine",
  );
});

test("Unmerge is refused with the tracking reason in Suggesting mode", () => {
  // The gate matters: a grid rewrite has no tracked-revision representation, and
  // the row must not simply go quiet when the mode changes.
  const { row } = build({ regular: false, suggesting: true });
  const unmerge = row("table.unmerge");
  assert.equal(unmerge.enabled, false);
  assert.equal(unmerge.disabledReason, "This structural change cannot be tracked in Suggesting mode");
});

test("Split cell… still opens the dialog, and is a different command from Unmerge", () => {
  const { doc, row } = build({ regular: false });
  row("table.split").run();
  assert.deepEqual(doc.calls, [["openSplitCellDialog", []]]);
  assert.notEqual(row("table.split").id, row("table.unmerge").id);
});

test("every column-axis command explains itself on a merged table, in the menu's own words", () => {
  const { row } = build({ regular: false });
  for (const id of [
    "table.insert.columnLeft",
    "table.insert.columnRight",
    "table.delete.column",
    "table.select.column",
    "table.distribute.columns",
    "table.sort.ascending",
    "table.sort.descending",
  ]) {
    assert.equal(row(id).enabled, false, `${id} must be unavailable on a merged table`);
    assert.equal(
      row(id).disabledReason,
      "Unavailable for merged or spanned tables",
      `${id} must give the reason the BAND gives — one catalogue entry, both surfaces`,
    );
  }
});

test("distribute rows names the height rule, not the merge, when that is what is missing", () => {
  const { row } = build({ regular: true, rowHeightRule: "auto" });
  assert.equal(row("table.distribute.rows").enabled, false);
  assert.equal(
    row("table.distribute.rows").disabledReason,
    "Rows need a fixed or minimum height before distribution",
  );
});

test("Merge cells refuses with the ONE sentence the product has for it", () => {
  // `#mergeCellsBtn`'s own click handler used to carry a second, divergent copy
  // ("Select a table row, column, or table first") on a branch its `disabled` made
  // unreachable. One meaning, one sentence (`docs/141` TBL-05).
  const { row } = build({ regular: true, tableSelection: null });
  assert.equal(row("table.merge").enabled, false);
  assert.equal(row("table.merge").disabledReason, "Select a row, column, or table before merging");
});

test("Merge acts on the SELECTION's node and mode, then drops the selection", () => {
  const { doc, row } = build({ regular: true, tableSelection: { node: "cell-1", mode: "row" } });
  assert.equal(row("table.merge").enabled, true);
  return row("table.merge").run().then(() => {
    assert.deepEqual(doc.calls, [
      ["runEdit", []],
      ["mergeTableSelection", ["cell-1", "row"]],
      ["clearTableSelection", []],
    ]);
  });
});

test("no command in the tree is enabled without a run, or disabled without a reason", () => {
  // The "never a dead control" floor (`SKILL.md` §10), asserted over the whole
  // tree rather than per row, so a command added later cannot skip it.
  for (const regular of [true, false]) {
    for (const suggesting of [true, false]) {
      const { rows } = build({ regular, suggesting });
      assert.ok(rows.length >= 19, `only ${rows.length} leaf rows — the tree shrank`);
      for (const entry of rows) {
        assert.equal(typeof entry.run, "function", `${entry.id} has no run`);
        if (entry.enabled === false) {
          assert.ok(
            entry.disabledReason,
            `${entry.id} is disabled with no stated reason (regular=${regular}, suggesting=${suggesting})`,
          );
        }
      }
    }
  }
});
