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
  TABLE_BAND_PRECONDITIONS,
  TABLE_BAND_REASON_KEYS,
  TABLE_ROW_COLUMN_ACTIONS,
  TABLE_DISTRIBUTE_ACTIONS,
  bindTableBand,
  tableBandStates,
  tableContextLabel,
} from "../src/table_band.mjs";
import { EN_STRINGS } from "../src/en_strings.mjs";

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

// ---- Disabled with a REASON (`docs/141` TBL-03) ------------------------------
// Band buttons shipped disabled with no stated reason, while the very same
// commands explain themselves in the Table menu and the palette. The rules are now
// a declaration, so "does every disabled band control name its precondition" is
// answerable here rather than only in a browser — and it has to hold for EVERY
// control, which is the class fix rather than the instance (`SKILL.md` §10).

/** A control the enablement sweep can read: `matches` against the small selector
 *  vocabulary the rules use, plus the `id` and `dataset` the real buttons carry. */
function bandControl({ id = "", dataset = {} } = {}) {
  return {
    id,
    dataset,
    disabled: false,
    title: "",
    matches(selector) {
      if (selector.startsWith("#")) return this.id === selector.slice(1);
      const attribute = selector.match(/data-table-([a-z]+)/)?.[1];
      if (!attribute) return false;
      const key = `table${attribute[0].toUpperCase()}${attribute.slice(1)}`;
      const value = this.dataset[key];
      if (value === undefined) return false;
      const wanted = selector.match(/[*^$]?="([^"]+)"/)?.[1];
      if (wanted === undefined) return true;
      return selector.includes('*="') ? value.includes(wanted) : value === wanted;
    },
  };
}

const bandRoot = (controls) => ({ querySelectorAll: () => controls });

/** The buttons the MARKUP really declares, so these tests cannot pass on a
 *  convenient subset. Derived from `editor.html`, never typed out. */
function realBandControls() {
  return [...tablePanel().matchAll(/<button([^>]*)>/g)].map((match) => {
    const attributes = match[1];
    const id = attributes.match(/\bid="([^"]+)"/)?.[1] ?? "";
    const dataset = {};
    for (const [, name, value] of attributes.matchAll(/\bdata-table-([a-z]+)="([^"]*)"/g)) {
      dataset[`table${name[0].toUpperCase()}${name.slice(1)}`] = value;
    }
    return bandControl({ id, dataset });
  });
}

test("with the caret outside a table, every band button is disabled AND says why", () => {
  const controls = realBandControls();
  assert.equal(controls.length, 19, "the band is nineteen buttons; re-measure if the markup changed");
  const states = tableBandStates(bandRoot(controls), { inTable: false });
  assert.equal(states.length, 19);
  for (const state of states) {
    assert.equal(state.enabled, false);
    assert.equal(
      state.reasonKey,
      "table.reason.caretOutsideTable",
      "outside a table the reason is the caret, never the merge — 'Unavailable for " +
        "merged or spanned tables' would be true of nothing and send the reader hunting",
    );
  }
});

test("a merged table names the merge, and only for the commands the merge gates", () => {
  const controls = realBandControls();
  const states = tableBandStates(bandRoot(controls), {
    inTable: true,
    regular: false,
    rowHeightRule: "exact",
    hasCellSelection: true,
  });
  const merged = states.filter((state) => state.reasonKey === "table.reason.merged");
  assert.deepEqual(
    merged
      .map((state) =>
        state.control.dataset.tableAction ??
        state.control.dataset.tableSelect ??
        state.control.dataset.tableDistribute ??
        state.control.dataset.tableSort)
      .sort(),
    [
      "ascending",
      "column",
      "columns",
      "delete-column",
      "descending",
      "insert-column-left",
      "insert-column-right",
    ],
    "exactly the column-axis commands are gated on a regular grid — the same set the " +
      "Table menu gates, which is the parity this declaration exists to keep",
  );
  // And the row-axis commands stay live: one merge must not disable the whole band.
  for (const key of ["insert-row-above", "insert-row-below", "delete-row", "delete-table"]) {
    const state = states.find((entry) => entry.control.dataset.tableAction === key);
    assert.equal(state.enabled, true, `${key} does not depend on a regular grid`);
    assert.equal(state.reasonKey, "");
  }
  const merge = states.find((state) => state.control.id === "mergeCellsBtn");
  assert.equal(merge.enabled, true, "a live cell selection enables Merge");
});

test("distribute rows names the height rule rather than the merge", () => {
  const rows = bandControl({ dataset: { tableDistribute: "rows" } });
  const [state] = tableBandStates(bandRoot([rows]), {
    inTable: true,
    regular: true,
    rowHeightRule: "auto",
  });
  assert.equal(state.enabled, false);
  assert.equal(state.reasonKey, "table.reason.rowHeights");
});

test("Merge says to select something first, in the band's own words", () => {
  const merge = bandControl({ id: "mergeCellsBtn" });
  const [state] = tableBandStates(bandRoot([merge]), {
    inTable: true,
    regular: true,
    rowHeightRule: "exact",
    hasCellSelection: false,
  });
  assert.equal(state.enabled, false);
  assert.equal(state.reasonKey, "table.reason.mergeSelection");
});

test("every reason key the rules can produce is declared in the catalogue", () => {
  // A key with no entry renders as the key itself — deliberately ugly, and this is
  // where it gets caught rather than in a screenshot (`i18n.mjs` `t`).
  for (const key of Object.values(TABLE_BAND_REASON_KEYS)) {
    assert.ok(key in EN_STRINGS, `${key} is produced by the band but absent from en_strings.mjs`);
  }
  // …and every precondition the table names has a sentence, in both directions.
  for (const [, requires] of TABLE_BAND_PRECONDITIONS) {
    assert.ok(
      TABLE_BAND_REASON_KEYS[requires],
      `the precondition "${requires}" has no sentence — it would disable a control silently`,
    );
  }
});

test("the context hint names the grid, the caret's cell, and a merge when there is one", () => {
  // The hint is the one place a reader learns the table is merged BEFORE a command
  // refuses, so "does it say so" is a property worth holding rather than a string
  // three e2e specs happen to match on. 1-based in the words, 0-based in the model.
  assert.equal(
    tableContextLabel({ rows: 3, columns: 3, row: 0, column: 1, regular: true }),
    "3×3 table · row 1, column 2",
  );
  assert.equal(
    tableContextLabel({ rows: 3, columns: 3, row: 0, column: 0, regular: false }),
    "3×3 table · row 1, column 1 · merged/spanned",
  );
});

test("every band button can have its authored title restored", () => {
  // The band writes the REASON into `title` while disabled and restores the
  // authored one when enabled, and `authoredTitle` prefers `data-i18n-title`. A
  // button without one falls back to the live `title` — which by then is the
  // reason — so this is the property that keeps the restore honest.
  const missing = [...tablePanel().matchAll(/<button([^>]*)>/g)]
    .map((match) => match[1])
    .filter((attributes) => !/\bdata-i18n-title="/.test(attributes))
    .map((attributes) => attributes.match(/\b(id|data-table-[a-z]+)="([^"]*)"/)?.[0] ?? attributes.trim());
  assert.deepEqual(missing, [], "a band button with no data-i18n-title cannot get its tooltip back");
});
