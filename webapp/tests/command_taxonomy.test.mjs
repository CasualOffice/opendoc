// The command-surface rules that used to be unreachable.
//
// Every assertion here is one that could not be written while the taxonomy
// lived inside `main.js` (`109` HF-085): the old `menu_taxonomy.test.mjs`
// parsed the file as text with a brace matcher, so it could check the SHAPE of
// a literal and nothing about the functions that consume it. These exercise the
// functions.
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  TABLE_MENU_LABELS,
  allMenuCommandIds,
  appMenuNames,
  flattenCommandTree,
  menuCommandIds,
  menuHomes,
  tableCommandLabel,
  tableMenuPlaceholders,
} from "../src/command_taxonomy.mjs";

/** A context-menu tree shaped like the ones `tableToolCommands` and
 *  `buildObjectContextCommands` return: leaves carry behaviour, parents carry
 *  only a label and a `submenu`. */
function sampleTree(run = () => {}) {
  return [
    { id: "table.merge", label: "Merge cells", enabled: true, run },
    {
      id: "table.delete",
      label: "Delete",
      submenu: [
        { id: "table.delete.row", label: "Row", enabled: false, disabledReason: "Only one row", run },
        {
          id: "table.layout",
          label: "Autofit & sort",
          submenu: [{ id: "table.sort.ascending", label: "Ascending", enabled: true, run }],
        },
      ],
    },
  ];
}

const describeAsTable = (entry, trail) => ({
  label: tableCommandLabel(entry.id, entry.label, trail, "palette"),
  group: "Table",
  kw: `table ${trail} ${entry.label}`.toLowerCase(),
});

// A palette row whose `run` is a submenu is a dead control — it renders, it is
// searchable, and clicking it does nothing. Both the table family and the
// object family had their own copy of this walk; now they share one, so this is
// the guard for both.
test("a submenu parent never becomes a row, and every leaf does", () => {
  const rows = flattenCommandTree(sampleTree(), describeAsTable);
  assert.deepEqual(
    rows.map((r) => r.id),
    ["table.merge", "table.delete.row", "table.sort.ascending"],
    "`table.delete` and `table.layout` are containers, not commands",
  );
  for (const row of rows) assert.equal(typeof row.run, "function");
});

test("a nested row keeps its parents' names, in order, in the label and the keywords", () => {
  const rows = flattenCommandTree(sampleTree(), describeAsTable);
  const sort = rows.find((r) => r.id === "table.sort.ascending");
  assert.equal(sort.label, "Table: Delete Autofit & sort Ascending");
  assert.equal(sort.kw, "table delete autofit & sort ascending");
  // A top-level leaf has no trail, and must not read "Table:  Merge cells".
  assert.equal(rows.find((r) => r.id === "table.merge").label, "Table: Merge cells");
});

// Enablement and the reason are the whole point of routing every surface
// through the same descriptors: a row that is disabled on the context menu and
// live in the palette would run an edit the context menu just refused.
test("flattening carries enablement, the refusal reason and the SAME run function", () => {
  const run = () => "ran";
  const rows = flattenCommandTree(sampleTree(run), describeAsTable);
  const deleteRow = rows.find((r) => r.id === "table.delete.row");
  assert.equal(deleteRow.enabled, false);
  assert.equal(deleteRow.disabledReason, "Only one row");
  assert.equal(deleteRow.run, run, "the row must invoke the original closure, not a copy");
});

test("a table row drops the \"Table:\" prefix inside the Table menu and keeps it elsewhere", () => {
  for (const surface of ["palette", "context", "compact", undefined]) {
    assert.equal(
      tableCommandLabel("table.delete.row", "Row", "Delete", surface),
      "Table: Delete Row",
      `the ${surface} surface is one flat list and the row must name its subject`,
    );
  }
  assert.equal(
    tableCommandLabel("table.delete.row", "Row", "Delete", "menu"),
    "Delete row",
    "inside a menu already called Table, the prefix stutters",
  );
  // An id with no menu label still has to read as something.
  assert.equal(tableCommandLabel("table.unknown", "Thing", "", "menu"), "Table: Thing");
});

// UX-012: with the caret outside a table the menu used to open EMPTY, which
// tells the user the editor cannot edit tables at all.
test("the caret-outside-a-table menu offers every row, disabled, with a reason", () => {
  const rows = tableMenuPlaceholders("Place the caret in a table");
  assert.deepEqual(
    rows.map((r) => r.id),
    [...TABLE_MENU_LABELS.keys()],
    "the placeholder set is the real set — a shorter one is a smaller menu",
  );
  assert.ok(rows.length > 0);
  for (const row of rows) {
    assert.equal(row.enabled, false);
    assert.equal(row.disabledReason, "Place the caret in a table");
    assert.equal(row.label, TABLE_MENU_LABELS.get(row.id));
    assert.doesNotMatch(row.label, /^Table: /, "a menu row must not stutter");
    assert.equal(typeof row.run, "function", "never a dead control that throws");
  }
});

// This is the set-equality shape `109` UX-004 asks for, on the surface that can
// have it today: the bar's contents are now a value a test can compare against,
// not a literal a regex can only sample.
test("the menu bar's id set is the union of its menus, with no id in two places", () => {
  const union = appMenuNames().flatMap((name) => menuCommandIds(name));
  assert.deepEqual(allMenuCommandIds(), union);
  assert.equal(
    new Set(union).size,
    union.length,
    "a duplicate id means one command answers twice in the bar",
  );
  assert.deepEqual(
    [...menuHomes().keys()].sort(),
    [...new Set(union)].sort(),
    "every id in the bar has exactly one recorded home",
  );
});

test("an unknown menu name yields no rows rather than throwing", () => {
  assert.deepEqual(menuCommandIds("nope"), []);
});
