// The chart settings panel's formatting sections: one series' type, axis,
// colour, line, number format, trendline and error bars; the chart's text
// fonts; and each axis' title and number format.
//
// ONLYOFFICE keeps these in Chart ▸ Advanced Settings (`common/main/lib/view/
// ChartSettingsDlg.js`: Vertical/Horizontal Axis tabs carry the axis title and
// the label number format) and in its Chart Type dialog's combo table (one row
// per series: type, secondary axis). Word puts them in the Format pane, per
// selected element. Both make the reader pick WHAT to format first and then
// show only the controls that element has — so does this: a series picker,
// then that series' controls; an element picker, then its font. A control the
// element cannot use is not shown, rather than shown dead.
//
// Every write goes through the one bridge (`chart_commands.mjs`), as one
// undoable change, and a refusal is said in the panel's note line.
//
// Cost: O(series + controls) DOM work per render, no engine reads of its own.
import { CHART_TYPE_KEYS, DASH_KEYS, ERROR_BAR_DEFAULTS, ERROR_BAR_KEYS, TRENDLINE_KEYS } from "./chart_commands.mjs";

/** Number-format presets and their keys. The engine publishes the codes
 *  (`format.numberFormats`); this names them. */
export const NUMBER_FORMAT_KEYS = Object.freeze({
  General: "chart.numFmt.general",
  0: "chart.numFmt.integer",
  "0.00": "chart.numFmt.decimal",
  "#,##0": "chart.numFmt.thousands",
  "#,##0.00": "chart.numFmt.thousandsDecimal",
  "0%": "chart.numFmt.percent",
  "0.00%": "chart.numFmt.percentDecimal",
  "$#,##0.00": "chart.numFmt.dollar",
  "€#,##0.00": "chart.numFmt.euro",
  "0.00E+00": "chart.numFmt.scientific",
});

/** What an error bar's amount field means, per kind. */
export const ERROR_BAR_AMOUNT_KEYS = Object.freeze({
  fixedVal: "chart.errorBars.amountFixed",
  percentage: "chart.errorBars.amountPercent",
  stdDev: "chart.errorBars.amountStdDev",
});

/** Which side of the point error bars extend to. */
export const ERROR_BAR_TYPE_KEYS = Object.freeze({
  both: "chart.errorBars.both",
  plus: "chart.errorBars.plus",
  minus: "chart.errorBars.minus",
});

/** The text elements a font applies to, and their keys, in Word's order. */
export const FONT_TARGET_KEYS = Object.freeze({
  chart: "chart.font.wholeChart",
  title: "chart.element.title",
  legend: "chart.element.legend",
  horizontalAxis: "chart.axis.horizontal",
  verticalAxis: "chart.axis.vertical",
});

/** Faces offered in the font field's suggestions; any installed face may be
 *  typed. Word's and ONLYOFFICE's chart defaults first. */
const SUGGESTED_FACES = ["Calibri", "Calibri Light", "Arial", "Cambria", "Georgia", "Times New Roman", "Verdana", "Segoe UI", "Inter"];

/** "" for General/automatic, else the code — the one spelling a select and
 *  the engine share. */
function formatValue(code) {
  return !code || code === "General" ? "General" : code;
}

/**
 * Builds the number-format control: the presets, the current code when it is
 * none of them, and Custom, which reveals a code field.
 *
 * @param {object} kit
 * @param {string} current  the code in use, "" for General
 * @param {string[]} presets
 * @param {(code: string) => void} onChange  "" means General
 * @param {string} control  the `data-chart-control` name
 */
export function numberFormatControl(kit, current, presets, onChange, control) {
  const { t } = kit;
  const value = formatValue(current);
  const wrap = document.createElement("div");
  wrap.className = "chart-numfmt";
  const options = (presets.length ? presets : Object.keys(NUMBER_FORMAT_KEYS)).map((code) => ({
    value: code,
    label: NUMBER_FORMAT_KEYS[code] ? `${t(NUMBER_FORMAT_KEYS[code])} — ${code}` : code,
  }));
  const known = options.some((option) => option.value === value);
  if (!known) options.push({ value, label: value });
  options.push({ value: "\u0000custom", label: t("chart.numFmt.custom") });
  const custom = document.createElement("input");
  custom.type = "text";
  custom.value = value === "General" ? "" : value;
  custom.placeholder = "#,##0.00";
  custom.spellcheck = false;
  custom.hidden = true;
  custom.disabled = !kit.editable;
  custom.dataset.chartControl = `${control}Code`;
  custom.setAttribute("aria-label", t("chart.numFmt.code"));
  const select = kit.select(options, value, (next) => {
    if (next === "\u0000custom") {
      custom.hidden = false;
      custom.focus();
      return;
    }
    onChange(next === "General" ? "" : next);
  });
  select.disabled = !kit.editable;
  select.dataset.chartControl = control;
  custom.addEventListener("change", () => onChange(custom.value.trim()));
  custom.addEventListener("keydown", (event) => {
    if (event.key === "Enter") custom.blur();
  });
  wrap.append(select, custom);
  return { wrap, select };
}

/** An axis' title field and, for a numeric axis, its number format. */
export function axisFormatFields(kit, group, role, axis, format) {
  const { t } = kit;
  const title = document.createElement("input");
  title.type = "text";
  title.value = axis.title ?? "";
  title.placeholder = t("chart.axisTitle.none");
  title.maxLength = kit.titleLimit;
  title.disabled = !kit.editable;
  title.dataset.chartControl = `${role}AxisTitle`;
  title.addEventListener("change", () => kit.writeFormat({ [`${role}Axis`]: { title: title.value } }));
  title.addEventListener("keydown", (event) => {
    if (event.key === "Enter") title.blur();
  });
  group.append(kit.field(t("chart.axisTitle.label"), title));
  if (axis.numeric) {
    const { wrap } = numberFormatControl(
      kit,
      axis.numberFormat ?? "",
      format.numberFormats ?? [],
      (code) => kit.writeFormat({ [`${role}Axis`]: { numberFormat: code } }),
      `${role}AxisNumberFormat`,
    );
    group.append(kit.field(t("chart.numFmt.label"), wrap));
  }
}

/** A colour well with an Automatic button beside it. */
function colorControl(kit, value, onChange, control, label) {
  const row = document.createElement("div");
  row.className = "chart-color-row";
  const well = document.createElement("input");
  well.type = "color";
  well.value = /^#[0-9a-f]{6}$/i.test(value ?? "") ? value : "#000000";
  well.disabled = !kit.editable;
  well.dataset.chartControl = control;
  well.setAttribute("aria-label", label);
  well.addEventListener("change", () => onChange(well.value.toUpperCase()));
  const auto = document.createElement("button");
  auto.type = "button";
  auto.className = "dialog-button chart-color-auto";
  auto.textContent = kit.t("chart.color.automatic");
  auto.disabled = !kit.editable;
  auto.dataset.chartControl = `${control}Auto`;
  auto.addEventListener("click", () => onChange(""));
  row.append(well, auto);
  return row;
}

/** A small number field that writes on change. */
function numberField(kit, value, { min, max, step }, onChange, control) {
  const input = document.createElement("input");
  input.type = "number";
  input.min = String(min);
  input.max = String(max);
  input.step = String(step);
  input.value = value === "" || value == null ? "" : String(value);
  input.disabled = !kit.editable;
  input.dataset.chartControl = control;
  input.addEventListener("change", () => {
    const number = Number(input.value);
    if (input.value !== "" && Number.isFinite(number)) onChange(number);
  });
  return input;
}

/**
 * The Series section: pick a series, then its type and axis (in a chart that
 * can combine), colour, line, number format, trendline and error bars.
 *
 * @param {object} kit       the panel's builders and writer
 * @param {object} view      the engine's chart view
 * @param {{index: number}} state  the picked series, kept across renders
 */
export function renderSeriesSection(kit, view, state) {
  const { t } = kit;
  const format = view.format ?? {};
  const series = format.series ?? [];
  if (series.length === 0) return;
  if (state.index >= series.length) state.index = 0;
  const group = kit.section(t("chart.seriesHeading"));
  group.dataset.chartSection = "series";
  const picker = kit.select(
    series.map((entry, index) => ({ value: String(index), label: entry.name || t("chart.seriesFallback", { n: index + 1 }) })),
    String(state.index),
    (value) => {
      state.index = Number(value);
      kit.render();
    },
  );
  picker.dataset.chartControl = "series";
  group.append(kit.field(t("chart.seriesPicker"), picker));

  const entry = series[state.index];
  const index = state.index;
  const write = (change) => kit.writeFormat({ series: [{ index, ...change }] });

  // Type and axis: only where a series may differ from its neighbours.
  const combinable = format.combinable ?? [];
  const canCombine = series.length >= 2 && series.every((other) => combinable.includes(other.kind));
  if (canCombine) {
    const kind = kit.select(
      combinable.map((token) => ({ value: token, label: t(CHART_TYPE_KEYS[token] ?? "chart.kind.column") })),
      entry.kind,
      (token) => write({ kind: token }),
    );
    kind.disabled = !kit.editable;
    kind.dataset.chartControl = "seriesKind";
    group.append(kit.field(t("chart.seriesType"), kind));
    group.append(
      kit.checkbox(t("chart.secondaryAxis"), entry.secondary, (on) => write({ secondary: on }), kit.editable ? "" : kit.reason),
    );
  }

  group.append(
    kit.field(
      entry.hasLine ? t("chart.lineColor") : t("chart.fillColor"),
      colorControl(kit, entry.color, (color) => write({ color }), "seriesColor", t("chart.seriesColor")),
    ),
  );
  if (entry.hasLine) {
    const row = document.createElement("div");
    row.className = "property-grid-2";
    const width = numberField(kit, entry.lineWidth || "", { min: 0.25, max: 20, step: 0.25 }, (pt) => write({ lineWidth: pt }), "seriesLineWidth");
    const dash = kit.select(
      (format.dashes ?? Object.keys(DASH_KEYS)).map((token) => ({ value: token, label: t(DASH_KEYS[token] ?? "chart.dash.solid") })),
      entry.dash || "solid",
      (token) => write({ dash: token }),
    );
    dash.disabled = !kit.editable;
    dash.dataset.chartControl = "seriesDash";
    row.append(kit.field(t("chart.lineWidth"), width), kit.field(t("chart.dashType"), dash));
    group.append(row);
  }

  const { wrap: numFmt } = numberFormatControl(
    kit,
    entry.numberFormat ?? "",
    format.numberFormats ?? [],
    (code) => write({ numberFormat: code }),
    "seriesNumberFormat",
  );
  group.append(kit.field(t("chart.numFmt.values"), numFmt));

  if (entry.admitsTrendline) {
    const trend = entry.trendline ?? { kind: "none" };
    const kind = kit.select(
      Object.entries(TRENDLINE_KEYS).map(([value, key]) => ({ value, label: t(key) })),
      trend.kind || "none",
      (value) =>
        write({
          trendline: value === "none" ? { kind: value } : { kind: value, order: trend.order || 2, period: trend.period || 2 },
        }),
    );
    kind.disabled = !kit.editable;
    kind.dataset.chartControl = "trendline";
    group.append(kit.field(t("chart.element.trendline"), kind));
    if (trend.kind === "poly") {
      group.append(
        kit.field(
          t("chart.trendline.order"),
          numberField(kit, trend.order || 2, { min: 2, max: 6, step: 1 }, (order) => write({ trendline: { kind: "poly", order } }), "trendlineOrder"),
        ),
      );
    }
    if (trend.kind === "movingAvg") {
      group.append(
        kit.field(
          t("chart.trendline.period"),
          numberField(kit, trend.period || 2, { min: 2, max: 255, step: 1 }, (period) => write({ trendline: { kind: "movingAvg", period } }), "trendlinePeriod"),
        ),
      );
    }
    if (trend.kind && trend.kind !== "none" && trend.kind !== "movingAvg") {
      const off = kit.editable ? "" : kit.reason;
      const checks = document.createElement("div");
      checks.className = "chart-panel-checks";
      checks.append(
        kit.checkbox(t("chart.trendline.equation"), trend.equation, (on) => write({ trendline: { kind: trend.kind, equation: on } }), off),
        kit.checkbox(t("chart.trendline.rSquared"), trend.rSquared, (on) => write({ trendline: { kind: trend.kind, rSquared: on } }), off),
      );
      group.append(checks);
    }
  }

  if (entry.admitsErrorBars) {
    const bars = entry.errorBars ?? { kind: "none" };
    const offered = Object.keys(ERROR_BAR_KEYS).filter((value) => value !== "cust" || bars.kind === "cust");
    const kind = kit.select(
      offered.map((value) => ({ value, label: t(ERROR_BAR_KEYS[value]) })),
      bars.kind || "none",
      (value) =>
        write({
          errorBars: value === "none" ? { kind: value } : { kind: value, value: ERROR_BAR_DEFAULTS[value] ?? "", type: bars.type || "both" },
        }),
    );
    kind.disabled = !kit.editable || bars.kind === "cust";
    if (bars.kind === "cust") kind.title = t("chart.errorBars.custReadOnly");
    kind.dataset.chartControl = "errorBars";
    group.append(kit.field(t("chart.element.errorBars"), kind));
    if (["fixedVal", "percentage", "stdDev"].includes(bars.kind)) {
      const row = document.createElement("div");
      row.className = "property-grid-2";
      const amount = document.createElement("input");
      amount.type = "text";
      amount.inputMode = "decimal";
      amount.value = bars.value ?? "";
      amount.disabled = !kit.editable;
      amount.dataset.chartControl = "errorBarValue";
      amount.addEventListener("change", () => write({ errorBars: { kind: bars.kind, value: amount.value.trim(), type: bars.type || "both" } }));
      const direction = kit.select(
        Object.entries(ERROR_BAR_TYPE_KEYS).map(([value, key]) => ({ value, label: t(key) })),
        bars.type || "both",
        (value) => write({ errorBars: { kind: bars.kind, value: bars.value, type: value } }),
      );
      direction.disabled = !kit.editable;
      direction.dataset.chartControl = "errorBarType";
      row.append(kit.field(t(ERROR_BAR_AMOUNT_KEYS[bars.kind]), amount), kit.field(t("chart.errorBars.direction"), direction));
      group.append(row);
    }
  }
}


/**
 * The Text section: pick an element, then its font face, size, bold, italic
 * and colour — Word's Home ▸ Font group on a selected chart element.
 *
 * @param {object} kit
 * @param {object} view
 * @param {{target: string}} state  the picked element, kept across renders
 */
export function renderTextSection(kit, view, state) {
  const { t } = kit;
  const fonts = view.format?.fonts;
  if (!fonts) return;
  const targets = Object.keys(FONT_TARGET_KEYS).filter((key) => key === "chart" || fonts[key]?.present);
  if (!targets.includes(state.target)) state.target = "chart";
  const group = kit.section(t("chart.textHeading"));
  group.dataset.chartSection = "text";
  const picker = kit.select(
    targets.map((key) => ({ value: key, label: t(FONT_TARGET_KEYS[key]) })),
    state.target,
    (value) => {
      state.target = value;
      kit.render();
    },
  );
  picker.dataset.chartControl = "fontTarget";
  group.append(kit.field(t("chart.font.element"), picker));

  const font = fonts[state.target] ?? {};
  const write = (change) => kit.writeFormat({ fonts: { [state.target]: change } });

  const face = document.createElement("input");
  face.type = "text";
  face.value = font.typeface ?? "";
  face.disabled = !kit.editable;
  face.dataset.chartControl = "fontFace";
  face.setAttribute("list", "chartFontFaces");
  face.spellcheck = false;
  face.addEventListener("change", () => write({ typeface: face.value.trim() }));
  face.addEventListener("keydown", (event) => {
    if (event.key === "Enter") face.blur();
  });
  let list = document.getElementById("chartFontFaces");
  if (!list) {
    list = document.createElement("datalist");
    list.id = "chartFontFaces";
    for (const name of SUGGESTED_FACES) list.append(new Option(name));
    document.body.append(list);
  }
  const size = numberField(kit, font.size || "", { min: 1, max: 4000, step: 0.5 }, (pt) => write({ size: pt }), "fontSize");
  const row = document.createElement("div");
  row.className = "property-grid-2";
  row.append(kit.field(t("chart.font.face"), face), kit.field(t("chart.font.size"), size));
  group.append(row);

  const toggles = document.createElement("div");
  toggles.className = "chart-font-toggles";
  toggles.setAttribute("role", "group");
  toggles.setAttribute("aria-label", t("chart.font.style"));
  for (const [key, icon, label] of [
    ["bold", "format_bold", t("chart.font.bold")],
    ["italic", "format_italic", t("chart.font.italic")],
  ]) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "chart-font-toggle";
    button.dataset.chartControl = `font${key[0].toUpperCase()}${key.slice(1)}`;
    button.setAttribute("aria-pressed", String(!!font[key]));
    button.setAttribute("aria-label", label);
    button.title = label;
    button.disabled = !kit.editable;
    button.innerHTML = `<span class="ms" aria-hidden="true">${icon}</span>`;
    button.addEventListener("click", () => write({ [key]: !font[key] }));
    toggles.append(button);
  }
  group.append(toggles);
  group.append(kit.field(t("chart.font.color"), colorControl(kit, font.color, (color) => write({ color }), "fontColor", t("chart.font.color"))));
}
