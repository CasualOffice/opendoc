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
      combinable: ["column", "column-stacked", "line", "line-markers", "area", "area-stacked"],
      series: [
        { name: "S1", kind: "column", secondary: false, admitsTrendline: true, admitsErrorBars: true, trendline: { kind: "none" }, errorBars: { kind: "none" } },
      ],
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

const twoSeries = (extra = {}) =>
  view({
    series: ["S1", "S2"],
    cells: [["1", "3"], ["2", "4"]],
    format: {
      series: [
        { name: "S1", kind: "column", secondary: false, admitsTrendline: true, admitsErrorBars: true, trendline: { kind: "none" }, errorBars: { kind: "none" } },
        { name: "S2", kind: "column", secondary: false, admitsTrendline: true, admitsErrorBars: true, trendline: { kind: "none" }, errorBars: { kind: "none" } },
      ],
      ...extra,
    },
  });

test("an axis title row turns the title on with Word's placeholder, and off again", () => {
  assert.deepEqual(patchWrittenBy("chart.axisTitle.vertical").patch.format, {
    verticalAxis: { title: "chart.defaultAxisTitle" },
  });
  const titled = view({ format: { horizontalAxis: { present: true, visible: true, title: "Quarter" } } });
  const { patch, command } = patchWrittenBy("chart.axisTitle.horizontal", titled);
  assert.deepEqual(patch.format, { horizontalAxis: { title: "" } });
  assert.equal(command.shortcut, "✓");
});

test("a trendline row writes that trendline to every series that can carry one, and only those", () => {
  const mixed = twoSeries();
  mixed.format.series[1].admitsTrendline = false;
  assert.deepEqual(patchWrittenBy("chart.trendline.movingAvg", mixed).patch.format, {
    series: [{ index: 0, trendline: { kind: "movingAvg", period: 2 } }],
  });
  assert.deepEqual(patchWrittenBy("chart.trendline.poly", twoSeries()).patch.format.series, [
    { index: 0, trendline: { kind: "poly", order: 2 } },
    { index: 1, trendline: { kind: "poly", order: 2 } },
  ]);
});

test("error-bar rows write Word's default amounts", () => {
  assert.deepEqual(patchWrittenBy("chart.errorBars.percentage").patch.format.series, [
    { index: 0, errorBars: { kind: "percentage", value: "5", type: "both" } },
  ]);
  assert.deepEqual(patchWrittenBy("chart.errorBars.stdDev").patch.format.series[0].errorBars.value, "1");
  assert.deepEqual(patchWrittenBy("chart.errorBars.none").patch.format.series, [{ index: 0, errorBars: { kind: "none" } }]);
});

test("a family with no trendlines disables the rows, saying why", () => {
  const pie = view({ kind: "pie", format: { series: [{ name: "S1", kind: "pie", admitsTrendline: false, admitsErrorBars: false }] } });
  const { command } = patchWrittenBy("chart.trendline.linear", pie);
  assert.equal(command.enabled, false);
  assert.equal(command.disabledReason, "chart.noTrendline");
  assert.equal(patchWrittenBy("chart.errorBars.stdErr", pie).command.disabledReason, "chart.noErrorBars");
  // "More options" still opens the panel, where the reason is shown.
  assert.deepEqual(patchWrittenBy("chart.trendline.more", pie).opened, [["settings", "n1"]]);
});

test("a combo preset writes each series' type and axis, keeping the last series apart", () => {
  assert.deepEqual(patchWrittenBy("chart.type.combo.columnLineSecondary", twoSeries()).patch.format, {
    series: [
      { index: 0, kind: "column", secondary: false },
      { index: 1, kind: "line", secondary: true },
    ],
  });
  assert.deepEqual(
    patchWrittenBy("chart.type.combo.areaColumn", twoSeries()).patch.format.series.map((entry) => entry.kind),
    ["area-stacked", "column"],
  );
});

test("a combination of one series is refused before it is offered", () => {
  const { command } = patchWrittenBy("chart.type.combo.columnLine");
  assert.equal(command.enabled, false);
  assert.equal(command.disabledReason, "chart.combo.needsTwoSeries");
});

test("the combo preset in use is checked", () => {
  const combo = twoSeries({
    combo: true,
    series: [
      { name: "S1", kind: "column", secondary: false },
      { name: "S2", kind: "line", secondary: false },
    ],
  });
  assert.equal(patchWrittenBy("chart.type.combo.columnLine", combo).command.shortcut, "✓");
  assert.equal(patchWrittenBy("chart.type.combo.columnLineSecondary", combo).command.shortcut, "");
  assert.equal(patchWrittenBy("chart.type.combo.custom", combo).command.shortcut, "");
});
