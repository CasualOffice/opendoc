// Every chart action, built once and offered on every surface.
//
// ONLYOFFICE gives a selected chart a contextual Chart tab (Chart Elements,
// Edit Data, chart type, styles, Advanced Settings — `common/main/lib/view/
// ChartTab.js`), a right-hand chart settings panel, and the same actions on the
// right-click menu. Word does the same with Chart Design, the Format pane and
// the chart's context menu. Four surfaces, one set of actions: so the actions
// are declared HERE, once, as a command tree, and the ribbon's dropdowns, the
// object right-click menu, the command palette and the settings panel all read
// it. A surface cannot offer an action the others lack, and an action cannot
// behave differently depending on where it was clicked (`105` UX-004).
//
// The bridge below is the only code that talks to the engine's `chartData` /
// `setChartData`. Every write sends the WHOLE grid plus at most one formatting
// change, through one serialised queue, as one undo step.
//
// Cost: building the tree is O(gallery + palettes + positions), all constant;
// a write is one engine call, O(series × rows) in the one chart.
import { editRefusalMessage } from "./edit_errors.mjs";
import { LEGEND_LABEL_KEYS, patchFromView } from "./chart_data.mjs";

/** The chart types in ONLYOFFICE's picker order, grouped by family. The engine
 *  publishes the tokens (`view.kinds`); this only groups and labels them, and a
 *  token the engine adds that is not grouped here still appears, in "Other". */
export const CHART_TYPE_GROUPS = Object.freeze([
  Object.freeze({ key: "chart.familyColumn", kinds: ["column", "column-stacked", "column-percent"] }),
  Object.freeze({ key: "chart.familyLine", kinds: ["line", "line-markers", "line-stacked", "line-percent"] }),
  Object.freeze({ key: "chart.familyPie", kinds: ["pie", "doughnut"] }),
  Object.freeze({ key: "chart.familyBar", kinds: ["bar", "bar-stacked", "bar-percent"] }),
  Object.freeze({ key: "chart.familyArea", kinds: ["area", "area-stacked", "area-percent"] }),
  Object.freeze({ key: "chart.familyScatter", kinds: ["scatter", "scatter-smooth"] }),
]);

/** Each type's catalogue key, spelled out so the locale extractor sees it. */
export const CHART_TYPE_KEYS = Object.freeze({
  column: "chart.kind.column",
  "column-stacked": "chart.kind.columnStacked",
  "column-percent": "chart.kind.columnPercent",
  line: "chart.kind.line",
  "line-markers": "chart.kind.lineMarkers",
  "line-stacked": "chart.kind.lineStacked",
  "line-percent": "chart.kind.linePercent",
  pie: "chart.kind.pie",
  doughnut: "chart.kind.doughnut",
  bar: "chart.kind.bar",
  "bar-stacked": "chart.kind.barStacked",
  "bar-percent": "chart.kind.barPercent",
  area: "chart.kind.area",
  "area-stacked": "chart.kind.areaStacked",
  "area-percent": "chart.kind.areaPercent",
  scatter: "chart.kind.scatter",
  "scatter-smooth": "chart.kind.scatterSmooth",
});

/** Each type's icon in the self-hosted Material Symbols font. */
export const CHART_TYPE_ICONS = Object.freeze({
  column: "bar_chart",
  "column-stacked": "stacked_bar_chart",
  "column-percent": "stacked_bar_chart",
  line: "show_chart",
  "line-markers": "timeline",
  "line-stacked": "stacked_line_chart",
  "line-percent": "stacked_line_chart",
  pie: "pie_chart",
  doughnut: "donut_large",
  bar: "align_horizontal_left",
  "bar-stacked": "align_horizontal_left",
  "bar-percent": "align_horizontal_left",
  area: "area_chart",
  "area-stacked": "area_chart",
  "area-percent": "area_chart",
  scatter: "scatter_plot",
  "scatter-smooth": "scatter_plot",
});

/** Legend positions and their keys — one list, `chart_data.mjs`'s. */
export const LEGEND_KEYS = LEGEND_LABEL_KEYS;

/** Data-label position tokens and their keys. */
export const LABEL_POSITION_KEYS = Object.freeze({
  outsideEnd: "chart.labels.outsideEnd",
  insideEnd: "chart.labels.insideEnd",
  center: "chart.labels.center",
  insideBase: "chart.labels.insideBase",
  top: "chart.labels.top",
  bottom: "chart.labels.bottom",
  left: "chart.labels.left",
  right: "chart.labels.right",
});

/** Palette tokens and their keys. */
export const PALETTE_KEYS = Object.freeze({
  colorful: "chart.palette.colorful",
  "mono-1": "chart.palette.mono1",
  "mono-2": "chart.palette.mono2",
  "mono-3": "chart.palette.mono3",
  "mono-4": "chart.palette.mono4",
  "mono-5": "chart.palette.mono5",
  "mono-6": "chart.palette.mono6",
});

/** Trendline tokens and their keys, in Word's Chart Elements order. */
export const TRENDLINE_KEYS = Object.freeze({
  none: "chart.trendline.none",
  linear: "chart.trendline.linear",
  exp: "chart.trendline.exp",
  log: "chart.trendline.log",
  power: "chart.trendline.power",
  poly: "chart.trendline.poly",
  movingAvg: "chart.trendline.movingAvg",
});

/** Error-bar tokens and their keys. `cust` is shown, never offered: its
 *  lengths are per-point data this editor does not author. */
export const ERROR_BAR_KEYS = Object.freeze({
  none: "chart.errorBars.none",
  stdErr: "chart.errorBars.stdErr",
  percentage: "chart.errorBars.percentage",
  stdDev: "chart.errorBars.stdDev",
  fixedVal: "chart.errorBars.fixedVal",
  cust: "chart.errorBars.cust",
});

/** The amount Word's Chart Elements gives each error-bar kind it adds:
 *  5% for percentage, one standard deviation, and a fixed 1. */
export const ERROR_BAR_DEFAULTS = Object.freeze({ percentage: "5", stdDev: "1", fixedVal: "1", stdErr: "" });

/** Line dash tokens and their keys. */
export const DASH_KEYS = Object.freeze({
  solid: "chart.dash.solid",
  sysDot: "chart.dash.roundDot",
  sysDash: "chart.dash.squareDot",
  dash: "chart.dash.dash",
  dashDot: "chart.dash.dashDot",
  lgDash: "chart.dash.longDash",
  lgDashDot: "chart.dash.longDashDot",
  lgDashDotDot: "chart.dash.longDashDotDot",
  dot: "chart.dash.dot",
  sysDashDot: "chart.dash.sysDashDot",
  sysDashDotDot: "chart.dash.sysDashDotDot",
});

/** Word's Change Chart Type ▸ Combo presets, as the per-series kinds each
 *  writes for `n` series. Every preset keeps the last series apart, which is
 *  how Word builds all three. */
export const COMBO_PRESETS = Object.freeze({
  columnLine: (n) => Array.from({ length: n }, (_, i) => ({ kind: i === n - 1 ? "line" : "column", secondary: false })),
  columnLineSecondary: (n) =>
    Array.from({ length: n }, (_, i) => ({ kind: i === n - 1 ? "line" : "column", secondary: i === n - 1 })),
  areaColumn: (n) => Array.from({ length: n }, (_, i) => ({ kind: i === n - 1 ? "column" : "area-stacked", secondary: false })),
});

/** Each combo preset's key. */
export const COMBO_KEYS = Object.freeze({
  columnLine: "chart.combo.columnLine",
  columnLineSecondary: "chart.combo.columnLineSecondary",
  areaColumn: "chart.combo.areaColumn",
});

/** Word's own default title text for a chart that is given one. */
const DEFAULT_TITLE_KEY = "chart.defaultTitle";
/** Word's own default text for an axis title that is turned on. */
const DEFAULT_AXIS_TITLE_KEY = "chart.defaultAxisTitle";

/** The trendline every series that can carry one shares, as a token: one
 *  kind when they all agree, "" when they differ. */
export function sharedSeriesValue(series, read) {
  const values = new Set(series.map(read));
  return values.size === 1 ? [...values][0] : "";
}

/** The title state as a token: "none", "above" or "overlay". */
export function titleState(view) {
  if (!view.title) return "none";
  return view.format?.titleOverlay ? "overlay" : "above";
}

/** Groups the engine's type tokens for a picker, keeping any it adds. */
export function groupedTypes(kinds) {
  const known = new Set(CHART_TYPE_GROUPS.flatMap((group) => group.kinds));
  const groups = CHART_TYPE_GROUPS.map((group) => ({
    key: group.key,
    kinds: group.kinds.filter((kind) => kinds.includes(kind)),
  })).filter((group) => group.kinds.length > 0);
  const other = kinds.filter((kind) => !known.has(kind));
  if (other.length) groups.push({ key: "chart.familyOther", kinds: other });
  return groups;
}

/** The talk-to-the-engine half every chart surface shares.
 *
 *  @param {object} io
 *  @param {() => any} io.doc
 *  @param {() => boolean} io.blocked          true (after saying why) when no edit may apply
 *  @param {(result: unknown) => Promise<unknown>} io.apply
 *  @param {(text: string, kind?: string) => void} io.setStatus
 *  @param {(key: string, params?: object) => string} io.t
 */
export function createChartBridge(io) {
  const REFUSAL_KEYS = {
    "chart.partial-coverage": "chart.refused.partial",
    "chart.scatter-combo": "chart.refused.combo",
    "chart.not-combinable": "chart.refused.notCombinable",
    "chart.no-trendline": "chart.refused.noTrendline",
    "chart.no-error-bars": "chart.refused.noErrorBars",
  };
  const routeRefusal = (code) => (REFUSAL_KEYS[code] ? io.t(REFUSAL_KEYS[code]) : "");
  let queue = Promise.resolve(true);

  /** The authoring view of the chart at `node`, or null. O(series × rows). */
  function view(node) {
    const doc = io.doc();
    if (!doc || !node || typeof doc.chartData !== "function") return null;
    const json = doc.chartData(node) || "";
    if (!json) return null;
    try {
      return { json, ...JSON.parse(json) };
    } catch {
      return null;
    }
  }

  /** Writes `mutate(patch)` to the chart at `node`, serialised behind any write
   *  already in flight. Resolves `true` when the chart changed, `false` when it
   *  was refused (and the reason was handed to `onRefused`, or the status line)
   *  or when nothing changed. */
  function write(node, mutate, { onRefused } = {}) {
    queue = queue.then(
      () => writeNow(node, mutate, onRefused),
      () => writeNow(node, mutate, onRefused),
    );
    return queue;
  }

  async function writeNow(node, mutate, onRefused) {
    const doc = io.doc();
    const current = view(node);
    if (!doc || !current) return false;
    const say = (sentence) => (onRefused ? onRefused(sentence) : io.setStatus(sentence, "error"));
    if (!current.editable) {
      say(routeRefusal(current.code) || current.reason);
      return false;
    }
    if (io.blocked()) return false;
    const patch = mutate(patchFromView(current));
    if (!patch) return false;
    let result;
    try {
      result = doc.setChartData(node, JSON.stringify(patch));
    } catch (err) {
      // An unchanged chart is not a failure: a re-chosen type, a cell left as
      // it was. The engine declines to push an undo step that undoes nothing.
      if (err?.code === "chart.unchanged") return false;
      say(editRefusalMessage(err, { routeRefusal }));
      return false;
    }
    await io.apply(result);
    return true;
  }

  return { view, write, routeRefusal };
}

/** The chart command tree for the chart at `view.node`.
 *
 *  Shape is the menu shape every surface already renders: `{id, label, group,
 *  icon?, shortcut?, enabled, disabledReason?, run}` leaves, `submenu` parents.
 *  A checked choice carries `shortcut: "✓"`, the convention the Wrap menu uses.
 *
 *  @param {object} view   the engine's `chartData` view
 *  @param {object} io
 *  @param {(key: string, params?: object) => string} io.t
 *  @param {(node: string) => void} io.openData      the data dialog
 *  @param {(node: string, section?: string) => void} io.openSettings  the settings panel, optionally scrolled to a section
 *  @param {(node: string, mutate: Function) => Promise<boolean>} io.write
 *  @param {() => string} [io.blockedReason]  why nothing may change now, or ""
 */
export function buildChartCommands(view, io) {
  const t = io.t;
  const node = view.node;
  const format = view.format ?? {};
  const blocked = io.blockedReason?.() ?? "";
  const reason = !view.editable ? view.reason : blocked;
  const editable = !reason;
  const check = (on) => (on ? "✓" : "");
  const leaf = (id, label, run, extra = {}) => ({
    id,
    label,
    group: "chart",
    enabled: editable,
    disabledReason: reason,
    run,
    ...extra,
  });
  const write = (mutate) => void io.write(node, mutate);
  const withFormat = (change) => write((patch) => ({ ...patch, format: change }));

  // One group per family, so the menu separates them the way a picker does.
  const typeMenu = groupedTypes(view.kinds ?? []).flatMap((group) =>
    group.kinds.map((kind) =>
      leaf(
        `chart.type.${kind}`,
        t(CHART_TYPE_KEYS[kind] ?? "chart.kind.column"),
        () => write((patch) => ({ ...patch, kind })),
        { group: group.key, shortcut: check(view.kind === kind) },
      ),
    ),
  );

  const title = titleState(view);
  const titleMenu = [
    leaf("chart.title.none", t("chart.titleState.none"), () => write((patch) => ({ ...patch, title: "" })), {
      shortcut: check(title === "none"),
    }),
    leaf(
      "chart.title.above",
      t("chart.titleState.above"),
      () =>
        write((patch) => ({
          ...patch,
          title: patch.title || t(DEFAULT_TITLE_KEY),
          format: { titleOverlay: false },
        })),
      { shortcut: check(title === "above") },
    ),
    leaf(
      "chart.title.overlay",
      t("chart.titleState.overlay"),
      () =>
        write((patch) => ({
          ...patch,
          title: patch.title || t(DEFAULT_TITLE_KEY),
          format: { titleOverlay: true },
        })),
      { shortcut: check(title === "overlay") },
    ),
  ];

  const legendMenu = Object.entries(LEGEND_KEYS).map(([position, key]) =>
    leaf(`chart.legend.${position}`, t(key), () => write((patch) => ({ ...patch, legend: position })), {
      shortcut: check(view.legend === position),
    }),
  );
  if (view.legend !== "none") {
    legendMenu.push(
      leaf("chart.legend.overlay", t("chart.legendOverlay"), () => withFormat({ legendOverlay: !format.legendOverlay }), {
        group: "chart.legendOverlay",
        shortcut: check(format.legendOverlay),
      }),
    );
  }

  const positions = format.labelPositions ?? [];
  const labelsMenu = [
    leaf("chart.labels.none", t("chart.labels.none"), () => withFormat({ dataLabels: false }), {
      shortcut: check(!format.dataLabels),
    }),
  ];
  if (positions.length === 0) {
    labelsMenu.push(
      leaf("chart.labels.show", t("chart.labels.show"), () => withFormat({ dataLabels: true }), {
        shortcut: check(format.dataLabels),
      }),
    );
  } else {
    for (const position of positions) {
      labelsMenu.push(
        leaf(
          `chart.labels.${position}`,
          t(LABEL_POSITION_KEYS[position] ?? "chart.labels.show"),
          () => withFormat({ dataLabels: true, labelPosition: position }),
          {
            shortcut: check(
              format.dataLabels && (format.labelPosition === position || (!format.labelPosition && position === positions[0])),
            ),
          },
        ),
      );
    }
  }

  const noAxes = format.hasAxes ? "" : t("chart.noAxes");
  const axisLeaf = (id, label, on, change) =>
    leaf(id, label, () => withFormat(change), {
      shortcut: check(on),
      enabled: editable && !noAxes,
      disabledReason: reason || noAxes,
    });
  const horizontal = format.horizontalAxis ?? {};
  const vertical = format.verticalAxis ?? {};
  const axesMenu = [
    axisLeaf("chart.axis.horizontal", t("chart.axis.horizontal"), horizontal.visible, {
      horizontalAxis: { visible: !horizontal.visible },
    }),
    axisLeaf("chart.axis.vertical", t("chart.axis.vertical"), vertical.visible, {
      verticalAxis: { visible: !vertical.visible },
    }),
  ];
  // Gridlines named by the direction they RUN, as both references name them:
  // horizontal gridlines come from the vertical axis.
  const gridlinesMenu = [
    axisLeaf("chart.gridlines.horizontal", t("chart.gridlines.horizontal"), vertical.gridlines, {
      verticalAxis: { gridlines: !vertical.gridlines },
    }),
    axisLeaf("chart.gridlines.vertical", t("chart.gridlines.vertical"), horizontal.gridlines, {
      horizontalAxis: { gridlines: !horizontal.gridlines },
    }),
  ];

  // Axis titles: Word's Chart Elements ▸ Axis Titles. Turning one on writes
  // Word's own placeholder text, which the settings panel then edits.
  const axisTitleLeaf = (role, axis) =>
    axisLeaf(`chart.axisTitle.${role}`, t(role === "horizontal" ? "chart.axis.horizontal" : "chart.axis.vertical"), !!axis.title, {
      [`${role}Axis`]: { title: axis.title ? "" : t(DEFAULT_AXIS_TITLE_KEY) },
    });
  const axisTitlesMenu = [];
  if (horizontal.present !== false) axisTitlesMenu.push(axisTitleLeaf("horizontal", horizontal));
  if (vertical.present !== false) axisTitlesMenu.push(axisTitleLeaf("vertical", vertical));
  if (axisTitlesMenu.length === 0) axisTitlesMenu.push(axisTitleLeaf("vertical", vertical));

  // Trendlines and error bars apply to every series that can carry one, as
  // Word's Chart Elements does when no single series is selected; one series
  // at a time is the settings panel's Series section.
  const allSeries = format.series ?? [];
  const seriesPatch = (admits, make) =>
    allSeries.flatMap((entry, index) => (entry[admits] ? [{ index, ...make(entry) }] : []));
  const trendlineSeries = allSeries.filter((entry) => entry.admitsTrendline);
  const errorSeries = allSeries.filter((entry) => entry.admitsErrorBars);
  const noTrendline = trendlineSeries.length ? "" : t("chart.noTrendline");
  const noErrorBars = errorSeries.length ? "" : t("chart.noErrorBars");
  const currentTrend = sharedSeriesValue(trendlineSeries, (entry) => entry.trendline?.kind ?? "none");
  const currentError = sharedSeriesValue(errorSeries, (entry) => entry.errorBars?.kind ?? "none");
  const trendLeaf = (kind, extra = {}) =>
    leaf(
      `chart.trendline.${kind}`,
      t(TRENDLINE_KEYS[kind]),
      () =>
        withFormat({
          series: seriesPatch("admitsTrendline", () => ({ trendline: { kind, ...extra } })),
        }),
      { shortcut: check(currentTrend === kind), enabled: editable && !noTrendline, disabledReason: reason || noTrendline },
    );
  const trendlineMenu = [
    trendLeaf("none"),
    trendLeaf("linear"),
    trendLeaf("exp"),
    trendLeaf("log"),
    trendLeaf("power"),
    trendLeaf("poly", { order: 2 }),
    trendLeaf("movingAvg", { period: 2 }),
    { ...leaf("chart.trendline.more", t("chart.moreOptions"), () => io.openSettings(node)), enabled: true, group: "chart.more" },
  ];
  const errorLeaf = (kind) =>
    leaf(
      `chart.errorBars.${kind}`,
      t(ERROR_BAR_KEYS[kind]),
      () =>
        withFormat({
          series: seriesPatch("admitsErrorBars", () => ({
            errorBars: kind === "none" ? { kind } : { kind, value: ERROR_BAR_DEFAULTS[kind] ?? "", type: "both" },
          })),
        }),
      { shortcut: check(currentError === kind), enabled: editable && !noErrorBars, disabledReason: reason || noErrorBars },
    );
  const errorBarsMenu = [
    errorLeaf("none"),
    errorLeaf("stdErr"),
    errorLeaf("percentage"),
    errorLeaf("stdDev"),
    { ...leaf("chart.errorBars.more", t("chart.moreOptions"), () => io.openSettings(node)), enabled: true, group: "chart.more" },
  ];

  // Combo presets: Word's Change Chart Type ▸ Combo. Two series at least —
  // a combination of one series is not one.
  const combinable = new Set(format.combinable ?? []);
  const comboReason = allSeries.length < 2 ? t("chart.combo.needsTwoSeries") : "";
  for (const [preset, kinds] of Object.entries(COMBO_PRESETS)) {
    const wanted = kinds(allSeries.length);
    const current = allSeries.length >= 2 && wanted.every((entry, index) =>
      allSeries[index]?.kind === entry.kind && !!allSeries[index]?.secondary === entry.secondary);
    typeMenu.push(
      leaf(
        `chart.type.combo.${preset}`,
        t(COMBO_KEYS[preset]),
        () =>
          withFormat({
            series: wanted.filter((entry) => combinable.has(entry.kind)).map((entry, index) => ({ index, ...entry })),
          }),
        {
          group: "chart.familyCombo",
          shortcut: check(current),
          enabled: editable && !comboReason,
          disabledReason: reason || comboReason,
        },
      ),
    );
  }
  typeMenu.push({
    ...leaf("chart.type.combo.custom", t("chart.combo.custom"), () => io.openSettings(node, "series")),
    group: "chart.familyCombo",
    enabled: true,
    shortcut: check(!!format.combo && !typeMenu.some((entry) => entry.group === "chart.familyCombo" && entry.shortcut)),
  });

  const styleMenu = (format.palettes ?? []).map((palette) =>
    leaf(`chart.style.${palette}`, t(PALETTE_KEYS[palette] ?? "chart.palette.colorful"), () => withFormat({ palette }), {
      shortcut: check(format.palette === palette),
    }),
  );

  return [
    {
      id: "chart.editData",
      label: t("chart.editData"),
      group: "chart",
      icon: "tableLayout",
      // Reading the data is useful when it cannot be changed; the dialog shows
      // it read-only and says why.
      enabled: true,
      run: () => io.openData(node),
    },
    { id: "chart.type", label: t("chart.typeHeading"), group: "chart", submenu: typeMenu },
    {
      id: "chart.elements",
      label: t("chart.elements"),
      group: "chart",
      submenu: [
        { id: "chart.elements.title", label: t("chart.element.title"), group: "chart", submenu: titleMenu },
        { id: "chart.elements.legend", label: t("chart.element.legend"), group: "chart", submenu: legendMenu },
        { id: "chart.elements.labels", label: t("chart.element.labels"), group: "chart", submenu: labelsMenu },
        { id: "chart.elements.axes", label: t("chart.element.axes"), group: "chart", submenu: axesMenu },
        { id: "chart.elements.axisTitles", label: t("chart.element.axisTitles"), group: "chart", submenu: axisTitlesMenu },
        { id: "chart.elements.errorBars", label: t("chart.element.errorBars"), group: "chart", submenu: errorBarsMenu },
        { id: "chart.elements.gridlines", label: t("chart.element.gridlines"), group: "chart", submenu: gridlinesMenu },
        { id: "chart.elements.trendline", label: t("chart.element.trendline"), group: "chart", submenu: trendlineMenu },
      ],
    },
    { id: "chart.style", label: t("chart.style"), group: "chart", submenu: styleMenu },
    {
      id: "chart.settings",
      label: t("chart.settings"),
      group: "chart",
      icon: "tune",
      enabled: true,
      run: () => io.openSettings(node),
    },
  ];
}
