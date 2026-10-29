// The table tools (`docs/153` table rows), the half that needs no browser: the
// command tree's new rows, what each one calls, and the dialogs' pure rules.
//
// The browser half — each dialog driven from its ribbon face and the palette,
// against the real engine — is `e2e/table-tools.spec.mjs`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { tableToolCommands } from "../src/table_commands.mjs";
import { flattenCommandTree, TABLE_MENU_LABELS, TABLE_MENU_LABEL_KEYS, tableMenuLabel } from "../src/command_taxonomy.mjs";
import {
  DELETE_SHIFTS,
  INSERT_SHIFTS,
  LOOK_OPTIONS,
  TEXT_DIRECTIONS,
  cellShiftCall,
  separatorArgs,
  wholeNumberIn,
} from "../src/table_tools.mjs";
import { setCatalogue, setLocale, resetI18n } from "../src/i18n.mjs";

const english = JSON.parse(readFileSync(new URL("../locales/en.json", import.meta.url), "utf8"));
resetI18n();
setCatalogue("en", english);
setLocale("en");

/** The tree over a stand-in host that records what each row asks for. */
function build({ lookOn = [], direction = "horizontal", suggesting = false } = {}) {
  const calls = [];
  const host = {
    doc: () => ({}),
    status: () => {},
    tableSelection: () => null,
    clearTableSelection: () => {},
    plainTableInfo: () => ({ found: true, regular: true, table: "t1" }),
    runEdit: (thunk) => thunk(),
    selectTableContext: () => {},
    openSplitCellDialog: () => {},
    openCellFormat: () => {},
    openTableProperties: () => {},
    openTableToolDialog: (kind) => calls.push(["dialog", kind]),
    splitTable: () => calls.push(["split"]),
    lookOptions: () => lookOn,
    toggleTableLook: (option, on) => calls.push(["look", option, on]),
    cellTextDirection: () => direction,
    setCellTextDirection: (value) => calls.push(["direction", value]),
  };
  const context = {
    anchor: { node: "cell-7", offset: 0 },
    table: { found: true, regular: true, merged: false, rowHeightRule: "auto", table: "t1", column: 0 },
    suggesting,
  };
  const tree = tableToolCommands(context, host);
  const rows = flattenCommandTree(tree, () => ({ label: "", group: "", kw: "" }));
  const leaf = (id) => {
    const walk = (entries) => {
      for (const entry of entries) {
        if (entry.id === id) return entry;
        const found = entry.submenu && walk(entry.submenu);
        if (found) return found;
      }
      return null;
    };
    return walk(tree);
  };
  return { calls, rows, leaf, row: (id) => rows.find((entry) => entry.id === id) };
}

// Mutation: make a look row call `toggleTableLook(option, on)` instead of `!on`
// in `table_commands.mjs`, and the first assertion turns red — a checked box
// that, clicked, turns itself on again.
test("each style option is a checkbox carrying its state, and toggles to the opposite", () => {
  const { calls, leaf } = build({ lookOn: ["headerRow", "bandedRows"] });
  const header = leaf("table.look.headerRow");
  assert.equal(header.checked, true);
  header.run();
  assert.deepEqual(calls.at(-1), ["look", "headerRow", false]);
  const total = leaf("table.look.totalRow");
  assert.equal(total.checked, false);
  total.run();
  assert.deepEqual(calls.at(-1), ["look", "totalRow", true]);
  for (const option of LOOK_OPTIONS) {
    assert.ok(leaf(`table.look.${option}`), `the tree offers table.look.${option}`);
  }
});

test("text direction is one of three, the current one checked, each setting its own value", () => {
  const { calls, leaf } = build({ direction: "rotateUp" });
  for (const value of TEXT_DIRECTIONS) {
    const entry = leaf(`table.textDirection.${value}`);
    assert.equal(entry.radio, true);
    assert.equal(entry.checked, value === "rotateUp", value);
    entry.run();
    assert.deepEqual(calls.at(-1), ["direction", value]);
  }
});

test("split, convert and the two cell dialogs reach the tools, and Suggesting refuses them", () => {
  const { calls, row } = build();
  row("table.splitTable").run();
  row("table.convertToText").run();
  row("table.insertCells").run();
  row("table.deleteCells").run();
  assert.deepEqual(calls, [["split"], ["dialog", "tableToText"], ["dialog", "insertCells"], ["dialog", "deleteCells"]]);

  const suggesting = build({ suggesting: true });
  for (const id of ["table.splitTable", "table.convertToText", "table.insertCells", "table.deleteCells", "table.look.headerRow", "table.textDirection.rotateDown"]) {
    const entry = suggesting.row(id);
    assert.equal(entry.enabled, false, `${id} is a grid change Suggesting cannot track`);
    assert.ok(entry.disabledReason, `${id} says why`);
  }
});

test("every new Table menu row has words in the menu, routed through the catalogue", () => {
  for (const [id, key] of TABLE_MENU_LABEL_KEYS) {
    assert.ok(TABLE_MENU_LABELS.has(id), `${id} is a Table menu row`);
    assert.ok(english[key], `${key} is in the catalogue`);
    assert.equal(tableMenuLabel(id), english[key]);
  }
});

test("a dialog number is whole and in range, or nothing", () => {
  assert.equal(wholeNumberIn("12", 1, 20), 12);
  assert.equal(wholeNumberIn(" 3 ", 1, 20), 3);
  for (const bad of ["", "0", "21", "2.5", "-1", "1e1", "abc"]) {
    assert.equal(wholeNumberIn(bad, 1, 20), null, JSON.stringify(bad));
  }
});

test("a separator is named for the engine, and Other needs a character", () => {
  assert.deepEqual(separatorArgs("comma"), { separator: "comma", character: "" });
  assert.deepEqual(separatorArgs("other", "|"), { separator: "other", character: "|" });
  assert.deepEqual(separatorArgs("other", "𝄞x"), { separator: "other", character: "𝄞" }, "one character, not one UTF-16 unit");
  assert.equal(separatorArgs("other", ""), null);
  assert.equal(separatorArgs("other", " "), null);
  assert.deepEqual(separatorArgs("nonsense"), { separator: "tab", character: "" });
});

// Mutation: map "row" to `insertTableCells` in `cellShiftCall`, and the
// whole-row assertion turns red — the dialog would shift one cell where the
// reader asked for a row.
test("the cell dialogs' whole-band choices are the band commands, the shifts are the new calls", () => {
  assert.deepEqual(INSERT_SHIFTS, ["right", "down", "row", "column"]);
  assert.deepEqual(DELETE_SHIFTS, ["left", "up", "row", "column"]);
  assert.deepEqual(cellShiftCall("insert", "right", "a", "b"), ["insertTableCells", ["a", "b", "right"]]);
  assert.deepEqual(cellShiftCall("insert", "down", "a", ""), ["insertTableCells", ["a", "", "down"]]);
  assert.deepEqual(cellShiftCall("insert", "row", "a", "b"), ["insertRow", ["a", false]]);
  assert.deepEqual(cellShiftCall("insert", "column", "a", "b"), ["insertColumn", ["a", false]]);
  assert.deepEqual(cellShiftCall("delete", "left", "a", "b"), ["deleteTableCells", ["a", "b", "left"]]);
  assert.deepEqual(cellShiftCall("delete", "up", "a", ""), ["deleteTableCells", ["a", "", "up"]]);
  assert.deepEqual(cellShiftCall("delete", "row", "a", "b"), ["deleteRow", ["a"]]);
  assert.deepEqual(cellShiftCall("delete", "column", "a", "b"), ["deleteColumn", ["a"]]);
});
