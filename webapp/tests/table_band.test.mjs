// The Table band's markup and its operation tables must agree — the same guard
// `view_zoom.test.mjs` holds for the zoom group, for the same reason: the band is
// declared twice (`data-table-*` in `editor.html`, the operation tables in
// `table_band.mjs`) and the failure mode of any such pair is one side gaining a
// row. A button with no operation is a dead control, which the UI floor forbids
// outright; an operation with no button is code nothing can reach.
//
// It also asserts the band's Select control runs the SHARED `selectTableContext`
// rather than a copy of it. That copy is what `109` UX-005 removed, and a comment
// saying so does not stop it coming back.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import {
  TABLE_ROW_COLUMN_ACTIONS,
  TABLE_DISTRIBUTE_ACTIONS,
  bindTableBand,
} from "../src/table_band.mjs";

const html = readFileSync(new URL("../editor.html", import.meta.url), "utf8");

function tablePanel() {
  const start = html.indexOf('id="panelTable"');
  assert.ok(start > 0, "the Table panel must exist in editor.html");
  const end = html.indexOf('id="panelView"', start);
  assert.ok(end > start, "the View panel must follow the Table panel");
  return html.slice(start, end);
}

function declared(attribute) {
  return [...tablePanel().matchAll(new RegExp(`data-table-${attribute}="([a-z-]+)"`, "g"))].map(
    (match) => match[1],
  );
}

test("every band button in the markup has an operation the module can run", () => {
  for (const [attribute, table] of [
    ["action", TABLE_ROW_COLUMN_ACTIONS],
    ["distribute", TABLE_DISTRIBUTE_ACTIONS],
  ]) {
    const values = declared(attribute);
    assert.ok(values.length > 0, `expected data-table-${attribute} buttons in the band`);
    for (const value of values) {
      assert.ok(
        table[value],
        `editor.html declares data-table-${attribute}="${value}", which table_band.mjs ` +
          "cannot run — it would ship disabled rather than doing anything",
      );
    }
  }
});

test("every operation the module knows has a button in the band", () => {
  for (const [attribute, table] of [
    ["action", TABLE_ROW_COLUMN_ACTIONS],
    ["distribute", TABLE_DISTRIBUTE_ACTIONS],
  ]) {
    const values = new Set(declared(attribute));
    for (const key of Object.keys(table)) {
      assert.ok(
        values.has(key),
        `table_band.mjs can run "${key}" but no band button asks for it — unreachable`,
      );
    }
  }
});

test("no two buttons claim the same operation", () => {
  for (const attribute of ["action", "distribute", "sort", "select"]) {
    const values = declared(attribute);
    assert.equal(
      new Set(values).size,
      values.length,
      `two buttons claim the same data-table-${attribute}`,
    );
  }
});

// ---- The binding contract, driven with a stand-in DOM and a stand-in engine ---

/** A control the band can bind, and can disable. */
const control = (dataset) => ({ dataset, disabled: false });

function harness(controls) {
  const calls = [];
  const clicks = new Map();
  const root = {
    querySelectorAll: (selector) => {
      const attribute = selector.slice("[data-table-".length, -1);
      const key = `table${attribute[0].toUpperCase()}${attribute.slice(1)}`;
      return controls.filter((c) => c.dataset[key] !== undefined);
    },
  };
  const doc = {
    insertRow: (node, after) => calls.push(["insertRow", node, after]),
    deleteRow: (node) => calls.push(["deleteRow", node]),
    insertColumn: (node, after) => calls.push(["insertColumn", node, after]),
    deleteColumn: (node) => calls.push(["deleteColumn", node]),
    deleteTable: (node) => calls.push(["deleteTable", node]),
    distributeTableRows: (node) => calls.push(["distributeTableRows", node]),
    distributeTableColumns: (node) => calls.push(["distributeTableColumns", node]),
    sortTable: (node, direction, column) => calls.push(["sortTable", node, direction, column]),
  };
  bindTableBand({
    root,
    onButton: (el, handler) => clicks.set(el, handler),
    getDoc: () => doc,
    getSelection: () => ({ focus: { node: "cell-7" } }),
    runEdit: (thunk) => {
      calls.push(["runEdit"]);
      thunk();
    },
    clearTableSelection: () => calls.push(["clearTableSelection"]),
    selectTableContext: (node, mode) => calls.push(["selectTableContext", node, mode]),
    caretTableColumn: () => 3,
  });
  return { calls, click: (el) => clicks.get(el)?.(), bound: (el) => clicks.has(el) };
}

test("a structural button drops the table selection, then edits through the gate", () => {
  const deleteRow = control({ tableAction: "delete-row" });
  const h = harness([deleteRow]);
  h.click(deleteRow);
  assert.deepEqual(h.calls, [["clearTableSelection"], ["runEdit"], ["deleteRow", "cell-7"]]);
});

test("sort passes the caret's own column, not the whole table", () => {
  const ascending = control({ tableSort: "ascending" });
  const h = harness([ascending]);
  h.click(ascending);
  assert.deepEqual(h.calls, [["runEdit"], ["sortTable", "cell-7", "ascending", 3]]);
});

test("Select runs the shared selectTableContext and nothing of its own", () => {
  // The band used to inline `tableSelection = {…}; drawSelection(); setStatus(…);
  // updateToolbar(); focusEditorSurface();` — a second implementation of the
  // function the Table menu, the context menu and `table.select.*` all call.
  const selectRow = control({ tableSelect: "row" });
  const h = harness([selectRow]);
  h.click(selectRow);
  assert.deepEqual(h.calls, [["selectTableContext", "cell-7", "row"]]);
});

test("a button whose operation the module does not know ships disabled, not inert", () => {
  const unknown = control({ tableAction: "teleport-row" });
  const blankSort = control({ tableSort: "" });
  const h = harness([unknown, blankSort]);
  assert.equal(unknown.disabled, true);
  assert.equal(blankSort.disabled, true);
  assert.equal(h.bound(unknown), false, "an unknown operation must not get a live handler");
  assert.equal(h.bound(blankSort), false);
});

test("nothing happens with no caret, and nothing throws", () => {
  const deleteRow = control({ tableAction: "delete-row" });
  const calls = [];
  const clicks = new Map();
  bindTableBand({
    root: { querySelectorAll: (s) => (s.includes("action") ? [deleteRow] : []) },
    onButton: (el, handler) => clicks.set(el, handler),
    getDoc: () => null,
    getSelection: () => null,
    runEdit: () => calls.push("runEdit"),
    clearTableSelection: () => calls.push("clear"),
    selectTableContext: () => calls.push("select"),
    caretTableColumn: () => 0,
  });
  clicks.get(deleteRow)();
  assert.deepEqual(calls, [], "no document, no caret, no edit — and no throw");
});
