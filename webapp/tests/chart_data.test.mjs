// The chart panel's pure half: the grid arithmetic every structural edit and
// every paste goes through. The engine owns validation and the model; what can
// go wrong HERE is index arithmetic — a row inserted under the wrong label, a
// pasted block shifted by one, a series deleted with someone else's numbers.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

const {
  LEGEND_CHOICES,
  insertRow,
  insertSeries,
  isChartNumber,
  normalisePastedNumber,
  parseClipboardGrid,
  pasteBlock,
  patchFromView,
  removeRow,
  removeSeries,
} = await import("../src/chart_data.mjs");

const VIEW = {
  kind: "column",
  title: "Sales",
  legend: "right",
  series: ["North", "South"],
  labels: ["Q1", "Q2", "Q3"],
  cells: [
    ["1", "2"],
    ["3", "4"],
    ["5", "6"],
  ],
  // Fields the write ignores, present so `patchFromView` is proven to drop them.
  editable: true,
  kinds: ["column"],
  maxRows: 5,
  maxSeries: 3,
};
const NAMES = { row: (n) => `Category ${n}`, series: (n) => `Series ${n}` };

test("a patch is exactly the six fields the engine reads, and shares nothing with the view", () => {
  const patch = patchFromView(VIEW);
  assert.deepEqual(Object.keys(patch).sort(), ["cells", "kind", "labels", "legend", "series", "title"]);
  patch.cells[0][0] = "99";
  patch.series.push("West");
  assert.equal(VIEW.cells[0][0], "1", "editing the draft reached the view it was read from");
  assert.equal(VIEW.series.length, 2);
});

test("a row is inserted with its label in place and blank cells, never zeros", () => {
  const next = insertRow(patchFromView(VIEW), 0, "Q1.5");
  assert.deepEqual(next.labels, ["Q1", "Q1.5", "Q2", "Q3"]);
  assert.deepEqual(next.cells[1], ["", ""]);
  assert.deepEqual(next.cells[2], ["3", "4"], "the row below kept its own numbers");
});

test("removing a row takes its label and its numbers together", () => {
  const next = removeRow(patchFromView(VIEW), 1);
  assert.deepEqual(next.labels, ["Q1", "Q3"]);
  assert.deepEqual(next.cells, [
    ["1", "2"],
    ["5", "6"],
  ]);
});

test("a series is inserted as a column in every row", () => {
  const next = insertSeries(patchFromView(VIEW), 0, "Centre");
  assert.deepEqual(next.series, ["North", "Centre", "South"]);
  assert.deepEqual(
    next.cells.map((row) => row.join(",")),
    ["1,,2", "3,,4", "5,,6"],
  );
});

test("removing a series removes that column and only that column", () => {
  const next = removeSeries(patchFromView(VIEW), 0);
  assert.deepEqual(next.series, ["South"]);
  assert.deepEqual(next.cells, [["2"], ["4"], ["6"]]);
});

test("the last row and the last series are never removed", () => {
  const one = { ...patchFromView(VIEW), labels: ["Q1"], cells: [["1", "2"]] };
  assert.deepEqual(removeRow(one, 0).labels, ["Q1"]);
  const single = { ...patchFromView(VIEW), series: ["North"], cells: [["1"], ["3"], ["5"]] };
  assert.deepEqual(removeSeries(single, 0).series, ["North"]);
});

test("a spreadsheet's clipboard is rows of tab-separated cells, trailing newline dropped", () => {
  assert.deepEqual(parseClipboardGrid("a\tb\r\nc\td\r\n"), [
    ["a", "b"],
    ["c", "d"],
  ]);
  assert.deepEqual(parseClipboardGrid("7"), [["7"]]);
  assert.deepEqual(parseClipboardGrid("1\t\t3\n"), [["1", "", "3"]], "an empty cell stays a cell");
});

test("a pasted block lands with its top-left on the cell it was pasted into", () => {
  const { patch, clipped } = pasteBlock(patchFromView(VIEW), 1, 0, [["10", "20"], ["30", "40"]], VIEW, NAMES);
  assert.equal(clipped, 0);
  assert.deepEqual(patch.cells, [
    ["1", "2"],
    ["10", "20"],
    ["30", "40"],
  ]);
  assert.deepEqual(patch.labels, VIEW.labels, "pasting numbers renamed nothing");
});

test("a block copied WITH its headers and pasted on the corner fills names, labels and cells", () => {
  const block = parseClipboardGrid("\tEast\tWest\nJan\t1,200\t3\nFeb\t4\t5\n");
  const { patch } = pasteBlock(patchFromView(VIEW), -1, -1, block, VIEW, NAMES);
  assert.deepEqual(patch.series, ["East", "West"]);
  assert.deepEqual(patch.labels, ["Jan", "Feb", "Q3"]);
  assert.deepEqual(patch.cells[0], ["1200", "3"], "the spreadsheet's grouping comma was undone");
  assert.deepEqual(patch.cells[2], ["5", "6"], "rows below the block were left alone");
});

test("a paste grows the grid to fit, names what it added, and counts what did not fit", () => {
  const block = [
    ["1", "2", "3", "4"],
    ["5", "6", "7", "8"],
  ];
  // maxRows 5, maxSeries 3: pasted at row 3 needs rows 3-4 (fits) and four
  // series (one too many).
  const { patch, clipped } = pasteBlock(patchFromView(VIEW), 3, 0, block, VIEW, NAMES);
  assert.equal(patch.labels.length, 5);
  assert.deepEqual(patch.labels.slice(3), ["Category 4", "Category 5"]);
  assert.deepEqual(patch.series, ["North", "South", "Series 3"]);
  assert.deepEqual(patch.cells[4], ["5", "6", "7"]);
  assert.equal(clipped, 2, "the fourth column of both rows was reported, not silently dropped");
});

test("only the unambiguous grouping shape is undone in a pasted number", () => {
  assert.equal(normalisePastedNumber(" 1,234 "), "1234");
  assert.equal(normalisePastedNumber("-12,345.6"), "-12345.6");
  assert.equal(normalisePastedNumber("1,5"), "1,5", "a decimal comma is never guessed at");
  assert.equal(normalisePastedNumber("12,34"), "12,34");
});

test("the number grammar matches the engine's: finite f64 text, or empty", () => {
  for (const ok of ["", " ", "4.3", "-2", "+.5", "5.", "1e3", "2.5E-2"]) assert.ok(isChartNumber(ok), ok);
  for (const bad of ["abc", "1,000", "0x10", "Infinity", "NaN", "1e400", "4.3.1", "--1"]) {
    assert.ok(!isChartNumber(bad), bad);
  }
});

test("the legend menu offers every position the engine accepts, so no chart's legend reads as blank", () => {
  // Derived from the engine's own parser, so a sixth position added there fails
  // here rather than arriving as a legend the menu shows as blank.
  const source = readFileSync(new URL("../../crates/casual-doc-wasm/src/chart.rs", import.meta.url), "utf8");
  const parser = source.slice(source.indexOf("fn legend_position"), source.indexOf("fn plots_x_values"));
  const accepted = [...parser.matchAll(/^\s*"([A-Za-z]+)" =>/gm)].map((m) => m[1]);
  assert.ok(accepted.length >= 5, `read ${accepted.length} tokens from the engine's legend parser`);
  assert.deepEqual([...LEGEND_CHOICES].sort(), accepted.sort());
});
