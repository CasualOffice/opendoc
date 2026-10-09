// The Chart settings panel — ONLYOFFICE's right-hand chart panel and Word's
// Format Chart pane, for the selected chart.
//
// ONLYOFFICE's document editor shows a chart's settings in its right panel
// (`documenteditor/main/app/template/ChartSettings.template`: size, wrap, type
// and style, data, advanced settings) and keeps axis bounds and label
// positions in Advanced Settings. Word puts the same in the Format pane. This
// panel is one place for all of it, grouped the way a reader looks for it:
// Type, Style, Elements, Axes, Data — and nothing a reader cannot use: a
// family with no axes shows no axis controls, a family that cannot place its
// labels offers no positions.
//
// Every control here runs the SAME command object the Chart tab, the
// right-click menu and the palette run (`chart_commands.mjs`), looked up by
// id. The axis bounds are the one exception — they are typed values, not
// choices — and go through the same bridge.
//
// Cost: a render is one `chartData` read and O(controls) DOM work; `sync` on
// repaint is one read and a string compare while the panel is open.
import { buildChartCommands, CHART_TYPE_KEYS, groupedTypes, LEGEND_KEYS, LABEL_POSITION_KEYS, PALETTE_KEYS, titleState } from "./chart_commands.mjs";

/** Every leaf of a command tree, by id. O(commands). */
function commandIndex(tree) {
  const index = new Map();
  const walk = (entries) => {
    for (const entry of entries) {
      if (entry.submenu) walk(entry.submenu);
      else if (entry.id) index.set(entry.id, entry);
    }
  };
  walk(tree);
  return index;
}

/**
 * @param {object} io
 * @param {ReturnType<import("./chart_commands.mjs").createChartBridge>} io.bridge
 * @param {object} io.commands   the `io` `buildChartCommands` takes
 * @param {() => {node: string, kind: string} | null} io.selection
 * @param {(key: string, params?: object) => string} io.t
 * @param {() => void} [io.returnFocus]
 */
export function createChartPanel(io) {
  const t = io.t;
  let el = null;
  let node = null;
  let lastJson = "";
  let commands = new Map();
  const parts = {};

  const run = (id) => {
    const command = commands.get(id);
    if (command && command.enabled !== false) command.run();
  };

  function section(heading) {
    const group = document.createElement("fieldset");
    group.className = "dialog-group property-section chart-panel-section";
    const legend = document.createElement("legend");
    legend.textContent = heading;
    group.append(legend);
    parts.body.append(group);
    return group;
  }

  function field(label, control) {
    const wrap = document.createElement("label");
    wrap.className = "dialog-field";
    wrap.append(label, control);
    return wrap;
  }

  function select(options, value, onChange) {
    const control = document.createElement("select");
    for (const option of options) {
      if (option.group) {
        const group = document.createElement("optgroup");
        group.label = option.group;
        for (const child of option.options) group.append(new Option(child.label, child.value));
        control.append(group);
      } else {
        control.append(new Option(option.label, option.value));
      }
    }
    control.value = value;
    control.addEventListener("change", () => onChange(control.value));
    return control;
  }

  function checkbox(label, checked, onChange, disabledReason = "") {
    const wrap = document.createElement("label");
    wrap.className = "chart-panel-check";
    const box = document.createElement("input");
    box.type = "checkbox";
    box.checked = !!checked;
    box.disabled = !!disabledReason;
    if (disabledReason) wrap.title = disabledReason;
    box.addEventListener("change", () => onChange(box.checked));
    wrap.append(box, label);
    return wrap;
  }

  function axisFields(heading, role, axis, editable) {
    if (!axis?.present) return;
    const group = section(heading);
    if (axis.numeric) {
      const bound = (which, value) => {
        const input = document.createElement("input");
        input.type = "text";
        input.inputMode = "decimal";
        input.value = value;
        input.placeholder = t("chart.axis.auto");
        input.dataset.axis = `${role}.${which}`;
        input.disabled = !editable;
        input.addEventListener("change", () => {
          const change = { [which]: input.value.trim() };
          void io.bridge.write(node, (patch) => ({ ...patch, format: { [`${role}Axis`]: change } }), {
            onRefused: (sentence) => note(sentence, true),
          }).then(render);
        });
        return input;
      };
      const row = document.createElement("div");
      row.className = "property-grid-2";
      row.append(
        field(t("chart.axis.minimum"), bound("minimum", axis.minimum)),
        field(t("chart.axis.maximum"), bound("maximum", axis.maximum)),
      );
      group.append(row);
    }
    group.append(
      checkbox(t("chart.axis.reverse"), axis.reverse, (on) => {
        void io.bridge.write(node, (patch) => ({ ...patch, format: { [`${role}Axis`]: { reverse: on } } })).then(render);
      }, editable ? "" : t("chart.readOnly")),
    );
  }

  function note(message, isError = false) {
    parts.note.textContent = message || "";
    parts.note.classList.toggle("error", !!isError && !!message);
  }

  function render() {
    if (!el || el.hidden || !node) return;
    const view = io.bridge.view(node);
    if (!view) {
      close();
      return;
    }
    lastJson = view.json;
    const tree = buildChartCommands(view, io.commands);
    commands = commandIndex(tree);
    const format = view.format ?? {};
    const editable = commands.get("chart.legend.none")?.enabled !== false;
    const reason = commands.get("chart.legend.none")?.disabledReason ?? "";
    parts.body.replaceChildren();

    const notice = !editable ? reason : view.replacesWorkbook ? t("chart.replacesWorkbook") : "";
    if (notice) {
      const p = document.createElement("p");
      p.className = `chart-data-reason${editable ? "" : " is-refusal"}`;
      p.textContent = notice;
      parts.body.append(p);
    }

    // ---- Type ----
    const type = section(t("chart.typeHeading"));
    const typeSelect = select(
      groupedTypes(view.kinds ?? []).map((group) => ({
        group: t(group.key),
        options: group.kinds.map((kind) => ({ value: kind, label: t(CHART_TYPE_KEYS[kind] ?? "chart.kind.column") })),
      })),
      view.kind,
      (kind) => run(`chart.type.${kind}`),
    );
    typeSelect.disabled = !editable;
    typeSelect.dataset.chartControl = "type";
    type.append(field(t("chart.typeLabel"), typeSelect));

    // ---- Style: palettes as swatches, ONLYOFFICE's style gallery ----
    const style = section(t("chart.style"));
    const swatches = document.createElement("div");
    swatches.className = "chart-palette-grid";
    swatches.setAttribute("role", "radiogroup");
    swatches.setAttribute("aria-label", t("chart.style"));
    (format.palettes ?? []).forEach((palette, index) => {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "chart-palette";
      btn.dataset.palette = palette;
      btn.setAttribute("role", "radio");
      const on = format.palette === palette;
      btn.setAttribute("aria-checked", String(on));
      btn.tabIndex = on || (!format.palette && index === 0) ? 0 : -1;
      btn.setAttribute("aria-label", t(PALETTE_KEYS[palette] ?? "chart.palette.colorful"));
      btn.title = t(PALETTE_KEYS[palette] ?? "chart.palette.colorful");
      btn.disabled = !editable;
      for (const color of format.swatches?.[index] ?? []) {
        const dot = document.createElement("span");
        dot.className = "chart-palette-dot";
        dot.style.background = color;
        btn.append(dot);
      }
      btn.addEventListener("click", () => run(`chart.style.${palette}`));
      swatches.append(btn);
    });
    swatches.addEventListener("keydown", (event) => {
      const step = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[event.key];
      if (!step) return;
      event.preventDefault();
      const all = [...swatches.querySelectorAll(".chart-palette")];
      const at = all.indexOf(document.activeElement);
      all[(at + step + all.length) % all.length]?.focus();
    });
    style.append(swatches);

    // ---- Elements: ONLYOFFICE's Chart Elements, as fields ----
    const elements = section(t("chart.elements"));
    const titleSelect = select(
      [
        { value: "none", label: t("chart.titleState.none") },
        { value: "above", label: t("chart.titleState.above") },
        { value: "overlay", label: t("chart.titleState.overlay") },
      ],
      titleState(view),
      (state) => run(`chart.title.${state}`),
    );
    titleSelect.disabled = !editable;
    titleSelect.dataset.chartControl = "title";
    elements.append(field(t("chart.element.title"), titleSelect));
    if (view.title) {
      const titleText = document.createElement("input");
      titleText.type = "text";
      titleText.value = view.title;
      titleText.maxLength = view.titleLimit || 1024;
      titleText.disabled = !editable;
      titleText.dataset.chartControl = "titleText";
      titleText.setAttribute("aria-label", t("chart.titleField"));
      titleText.addEventListener("change", () => {
        const text = titleText.value;
        void io.bridge.write(node, (patch) => ({ ...patch, title: text })).then(render);
      });
      titleText.addEventListener("keydown", (event) => {
        if (event.key === "Enter") titleText.blur();
      });
      elements.append(field(t("chart.titleField"), titleText));
    }
    const legendSelect = select(
      Object.entries(LEGEND_KEYS).map(([value, key]) => ({ value, label: t(key) })),
      view.legend,
      (position) => run(`chart.legend.${position}`),
    );
    legendSelect.disabled = !editable;
    legendSelect.dataset.chartControl = "legend";
    elements.append(field(t("chart.element.legend"), legendSelect));
    if (view.legend !== "none") {
      elements.append(checkbox(t("chart.legendOverlay"), format.legendOverlay, () => run("chart.legend.overlay"), editable ? "" : reason));
    }
    const positions = format.labelPositions ?? [];
    const labelOptions = [{ value: "none", label: t("chart.labels.none") }];
    if (positions.length === 0) labelOptions.push({ value: "show", label: t("chart.labels.show") });
    for (const position of positions) {
      labelOptions.push({ value: position, label: t(LABEL_POSITION_KEYS[position] ?? "chart.labels.show") });
    }
    const labelValue = !format.dataLabels
      ? "none"
      : positions.length === 0
        ? "show"
        : format.labelPosition || positions[0];
    const labelSelect = select(labelOptions, labelValue, (value) => run(`chart.labels.${value}`));
    labelSelect.disabled = !editable;
    labelSelect.dataset.chartControl = "labels";
    elements.append(field(t("chart.element.labels"), labelSelect));
    if (format.hasAxes) {
      const vertical = format.verticalAxis ?? {};
      const horizontal = format.horizontalAxis ?? {};
      const off = editable ? "" : reason;
      const grid = document.createElement("div");
      grid.className = "chart-panel-checks";
      grid.append(
        checkbox(t("chart.axis.horizontal"), horizontal.visible, () => run("chart.axis.horizontal"), off),
        checkbox(t("chart.axis.vertical"), vertical.visible, () => run("chart.axis.vertical"), off),
        checkbox(t("chart.gridlines.horizontal"), vertical.gridlines, () => run("chart.gridlines.horizontal"), off),
        checkbox(t("chart.gridlines.vertical"), horizontal.gridlines, () => run("chart.gridlines.vertical"), off),
      );
      elements.append(grid);
      // ---- Axes ----
      axisFields(t("chart.axis.verticalHeading"), "vertical", vertical, editable);
      axisFields(t("chart.axis.horizontalHeading"), "horizontal", horizontal, editable);
    }

    // ---- Data ----
    const data = section(t("chart.dataHeading"));
    const summary = document.createElement("p");
    summary.className = "chart-panel-summary";
    summary.textContent = t("chart.dataSummary", { series: view.series.length, rows: view.labels.length });
    const edit = document.createElement("button");
    edit.type = "button";
    edit.className = "dialog-button";
    edit.dataset.chartControl = "editData";
    edit.textContent = t("chart.editDataEllipsis");
    edit.addEventListener("click", () => run("chart.editData"));
    data.append(summary, edit);
    parts.body.append(parts.note);
  }

  function ensure() {
    if (el) return;
    el = document.createElement("aside");
    el.id = "chartPanel";
    el.className = "side-panel properties-panel chart-panel";
    el.hidden = true;
    el.setAttribute("aria-labelledby", "chartPanelTitle");
    const head = document.createElement("header");
    head.className = "panel-head properties-panel-head";
    const heading = document.createElement("div");
    heading.className = "properties-panel-heading";
    const wrap = document.createElement("span");
    const strong = document.createElement("strong");
    strong.className = "panel-title";
    strong.id = "chartPanelTitle";
    strong.textContent = t("chart.panelTitle");
    const small = document.createElement("small");
    small.textContent = t("chart.panelIntro");
    wrap.append(strong, small);
    heading.append(wrap);
    const closeBtn = document.createElement("button");
    closeBtn.type = "button";
    closeBtn.className = "panel-close";
    closeBtn.setAttribute("aria-label", t("chart.close"));
    closeBtn.title = t("chart.close");
    closeBtn.innerHTML = '<span class="ms" aria-hidden="true">close</span>';
    closeBtn.addEventListener("click", () => close());
    head.append(heading, closeBtn);
    parts.body = document.createElement("div");
    parts.body.className = "panel-body properties-panel-body chart-panel-body";
    parts.note = document.createElement("p");
    parts.note.className = "chart-data-note";
    parts.note.setAttribute("role", "status");
    parts.note.setAttribute("aria-live", "polite");
    el.append(head, parts.body);
    el.addEventListener("keydown", (event) => {
      if (event.key === "Escape" && !event.defaultPrevented) {
        event.preventDefault();
        close();
      }
    });
    (document.querySelector(".workarea") ?? document.body).append(el);
  }

  /** Opens the panel on the chart at `target` (default: the selected chart). */
  function open(target = io.selection()?.node) {
    if (!io.bridge.view(target)) return false;
    ensure();
    node = target;
    el.hidden = false;
    note("");
    render();
    el.querySelector('[data-chart-control="type"]')?.focus({ preventScroll: true });
    return true;
  }

  function close() {
    if (!el || el.hidden) return;
    const hadFocus = el.contains(document.activeElement);
    el.hidden = true;
    node = null;
    lastJson = "";
    if (hadFocus) io.returnFocus?.();
  }

  /** Follows the selection to another chart, closes when its chart is gone,
   *  and redraws after any change to it — but never under a field being
   *  typed in. */
  function sync() {
    if (!el || el.hidden) return;
    const selected = io.selection();
    if (selected?.kind === "chart" && selected.node !== node && io.bridge.view(selected.node)) {
      node = selected.node;
      render();
      return;
    }
    const current = io.bridge.view(node);
    if (!current) {
      close();
      return;
    }
    if (current.json === lastJson) return;
    const typing = el.contains(document.activeElement) && document.activeElement instanceof HTMLInputElement && document.activeElement.type === "text";
    if (typing) return;
    const focusedControl = document.activeElement?.dataset?.chartControl;
    render();
    if (focusedControl) el.querySelector(`[data-chart-control="${focusedControl}"]`)?.focus({ preventScroll: true });
  }

  return { open, close, sync, isOpen: () => !!el && !el.hidden, node: () => node, render };
}
