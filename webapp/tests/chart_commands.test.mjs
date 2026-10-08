// The chart command tree: the one set of actions the Chart tab, the right-click
// menu, the palette and the settings panel all run. A pixel check in a browser
// cannot tell "vertical gridlines" from "horizontal gridlines" — both change the
// picture — so what each command WRITES is pinned here, exactly.
import assert from "node:assert/strict";
import test from "node:test";

const { buildChartCommands, groupedTypes, titleState, CHART_TYPE_KEYS } = await import("../src/chart_commands.mjs");

const KINDS = [
  "column", "column-stacked", "column-percent", "line", "line-markers", "line-stacked",
  "line-percent", "pie", "doughnut", "bar", "bar-stacked", "bar-percent", "area",
  "area-stacked", "area-percent", "scatter", "scatter-smooth",
];

function view(overrides = {}) {
  return {
    node: "n1",
    kind: "column",
    kinds: KINDS,
    editable: true,
    reason: "",
    title: "",
    legend: "right",
    series: ["S1"],
    labels: ["A", "B"],
    cells: [["1"], ["2"]],
    format: {
      hasAxes: true,
      titleOverlay: false,
      legendOverlay: false,
      dataLabels: false,
      labelPosition: "",
      labelPositions: ["outsideEnd", "insideEnd", "center", "insideBase"],
      horizontalAxis: { present: true, visible: true, numeric: false, gridlines: false },
      verticalAxis: { present: true, visible: true, numeric: true, gridlines: true },
      palette: "colorful",
      palettes: ["colorful", "mono-1"],
      swatches: [],
      ...overrides.format,
    },
    ...Object.fromEntries(Object.entries(overrides).filter(([key]) => key !== "format")),
  };
}

/** Builds the tree with a spy write, runs `id`, returns the patch it wrote. */
function patchWrittenBy(id, v = view()) {
  const writes = [];
  const opened = [];
  const tree = buildChartCommands(v, {
    t: (key) => key,
    write: (node, mutate) => writes.push(mutate({ kind: v.kind, title: v.title, legend: v.legend, series: v.series, labels: v.labels, cells: v.cells })),
    openData: (node) => opened.push(["data", node]),
    openSettings: (node) => opened.push(["settings", node]),
  });
  const find = (entries) => {
    for (const entry of entries) {
      if (entry.id === id) return entry;
      if (entry.submenu) {
        const found = find(entry.submenu);
        if (found) return found;
      }
    }
    return null;
  };
  const command = find(tree);
  assert.ok(command, `no command ${id}`);
  command.run();
  return { patch: writes[0], opened, command };
}

test("gridlines are named by the direction they run: horizontal ones come from the vertical axis", () => {
  assert.deepEqual(patchWrittenBy("chart.gridlines.vertical").patch.format, {
    horizontalAxis: { gridlines: true },
  });
  assert.deepEqual(patchWrittenBy("chart.gridlines.horizontal").patch.format, {
    verticalAxis: { gridlines: false },
  });
});

test("an axis row toggles that axis", () => {
  assert.deepEqual(patchWrittenBy("chart.axis.horizontal").patch.format, { horizontalAxis: { visible: false } });
  assert.deepEqual(patchWrittenBy("chart.axis.vertical").patch.format, { verticalAxis: { visible: false } });
});

test("a title above the chart gets Word's default text when there was none, and none removes it", () => {
  const above = patchWrittenBy("chart.title.above").patch;
  assert.equal(above.title, "chart.defaultTitle");
  assert.deepEqual(above.format, { titleOverlay: false });
  assert.equal(patchWrittenBy("chart.title.overlay").patch.format.titleOverlay, true);
  assert.equal(patchWrittenBy("chart.title.none", view({ title: "Sales" })).patch.title, "");
  // An existing title keeps its words when it moves.
  assert.equal(patchWrittenBy("chart.title.overlay", view({ title: "Sales" })).patch.title, "Sales");
});

test("data labels offer the family's positions, and each writes that position", () => {
  assert.deepEqual(patchWrittenBy("chart.labels.insideBase").patch.format, { dataLabels: true, labelPosition: "insideBase" });
  assert.deepEqual(patchWrittenBy("chart.labels.none").patch.format, { dataLabels: false });
  // A family that cannot place labels offers one "show" row instead.
  const pie = view({ kind: "pie", format: { labelPositions: [] } });
  assert.deepEqual(patchWrittenBy("chart.labels.show", pie).patch.format, { dataLabels: true });
});

test("a type row writes that type, and every engine type is offered once, grouped by family", () => {
  assert.equal(patchWrittenBy("chart.type.bar-percent").patch.kind, "bar-percent");
  const groups = groupedTypes(KINDS);
  assert.deepEqual(groups.flatMap((group) => group.kinds).sort(), [...KINDS].sort());
  for (const kind of KINDS) assert.ok(CHART_TYPE_KEYS[kind], `${kind} has no label key`);
  // A type the engine adds later is still offered, not silently dropped.
  assert.deepEqual(groupedTypes([...KINDS, "radar"]).at(-1).kinds, ["radar"]);
});

test("a palette row writes that palette; legend rows write positions, and overlay toggles", () => {
  assert.deepEqual(patchWrittenBy("chart.style.mono-1").patch.format, { palette: "mono-1" });
  assert.equal(patchWrittenBy("chart.legend.bottom").patch.legend, "bottom");
  assert.deepEqual(patchWrittenBy("chart.legend.overlay").patch.format, { legendOverlay: true });
});

test("the current choice is checked, and only it", () => {
  const { command } = patchWrittenBy("chart.legend.right");
  assert.equal(command.shortcut, "✓");
  assert.equal(patchWrittenBy("chart.legend.top").command.shortcut, "");
  assert.equal(titleState(view({ title: "T", format: { titleOverlay: true } })), "overlay");
  assert.equal(titleState(view()), "none");
});

test("a chart that cannot change disables every change with the engine's reason, and still opens its data", () => {
  const locked = view({ editable: false, reason: "This chart uses features…" });
  const { command } = patchWrittenBy("chart.legend.top", locked);
  assert.equal(command.enabled, false);
  assert.equal(command.disabledReason, "This chart uses features…");
  const data = patchWrittenBy("chart.editData", locked);
  assert.equal(data.command.enabled, true, "the data is readable even when it cannot change");
  assert.deepEqual(data.opened, [["data", "n1"]]);
});

test("a family with no axes disables the axis and gridline rows, saying so", () => {
  const pie = view({ kind: "pie", format: { hasAxes: false } });
  const { command } = patchWrittenBy("chart.gridlines.vertical", pie);
  assert.equal(command.enabled, false);
  assert.equal(command.disabledReason, "chart.noAxes");
});
