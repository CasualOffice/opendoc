// The chart's surfaces, assembled: the engine bridge, the Chart Data dialog,
// the Chart settings panel and the contextual Chart ribbon tab — and the one
// command tree they and the right-click menu and the palette share.
//
// Lifted out of `main.js` whole, because `main.js` is at its line ceiling and
// because this is one feature with one state: which chart, and what the engine
// says about it. Its whole contact with the application is `io`.
//
// The Chart tab follows ONLYOFFICE's (`common/main/lib/view/ChartTab.js`): it
// is contextual — disabled, with the reason, until a chart is selected; it
// carries Edit Data, the chart type, Chart Elements, the style gallery and the
// settings; and it sits after the Table tab at the right-hand end, where both
// references put contextual tabs.
//
// Cost: `sync` runs on repaint — one `chartData` read for the selected chart,
// and nothing when no chart is selected or open.
import { buildChartCommands, createChartBridge, PALETTE_KEYS } from "./chart_commands.mjs";
import { createChartDataDialog } from "./chart_data.mjs";
import { createChartPanel } from "./chart_panel.mjs";
import { newestObject, placedObjectIds } from "./placed_objects.mjs";

/**
 * @param {object} io
 * @param {() => any} io.doc
 * @param {() => {node: string, kind: string, mode: string} | null} io.selection
 * @param {(result: unknown) => Promise<unknown>} io.apply
 * @param {() => boolean} io.blocked            refuses (and says why) when no edit may apply
 * @param {() => string} io.blockedReason       why no edit may apply, without saying it
 * @param {(text: string, kind?: string) => void} io.setStatus
 * @param {(key: string, params?: object) => string} io.t
 * @param {Function} io.registerModal
 * @param {() => void} io.returnFocus
 * @param {(button: HTMLElement, commands: object[]) => void} io.showMenu
 * @param {(tab: HTMLElement, state: {enabled: boolean, reasonKey: string}) => void} io.reflectTab
 * @param {(name: string) => void} io.selectTab
 */
export function createChartSurface(io) {
  const bridge = createChartBridge(io);
  const commandIo = {
    t: io.t,
    write: (node, mutate) => bridge.write(node, mutate),
    openData: (node) => dialog.open(node),
    openSettings: (node, at) => panel.open(node, at),
    blockedReason: () => io.blockedReason(),
  };
  const dialog = createChartDataDialog({
    bridge,
    t: io.t,
    registerModal: io.registerModal,
    fallbackFocus: io.returnFocus,
  });
  const panel = createChartPanel({
    bridge,
    commands: commandIo,
    selection: io.selection,
    t: io.t,
    returnFocus: io.returnFocus,
  });

  /** The selected chart's node, or null. */
  function selectedChart() {
    const selection = io.selection();
    return selection?.kind === "chart" && selection.mode === "selected" ? selection.node : null;
  }

  /** The command tree for the chart at `node` (default: the selected one), or
   *  an empty list when there is no chart there. */
  function commandsFor(node = selectedChart()) {
    const view = node ? bridge.view(node) : null;
    return view ? buildChartCommands(view, commandIo) : [];
  }

  // ---- the contextual Chart tab ----
  const tab = document.getElementById("tabChart");
  const band = document.getElementById("panelChart");
  const gallery = document.getElementById("chartStyleGallery");
  let galleryJson = "";

  function menuButton(id, commandId) {
    const button = document.getElementById(id);
    button?.addEventListener("click", () => {
      const parent = commandsFor().find((command) => command.id === commandId);
      if (parent?.submenu) io.showMenu(button, parent.submenu);
    });
  }
  document.getElementById("chartEditDataBtn")?.addEventListener("click", () => {
    const node = selectedChart();
    if (node) dialog.open(node);
  });
  document.getElementById("chartSettingsBtn")?.addEventListener("click", () => {
    const node = selectedChart();
    if (node) panel.open(node);
  });
  // Opening the tab paints its gallery for the chart selected now.
  tab?.addEventListener("click", () => queueMicrotask(() => sync()));
  menuButton("chartTypeBtn", "chart.type");
  menuButton("chartElementsBtn", "chart.elements");

  /** The style gallery on the tab: one swatch button per palette, the colours
   *  the engine resolved from the document's theme. */
  function renderGallery(view) {
    if (!gallery) return;
    const format = view?.format ?? {};
    const json = JSON.stringify([format.palettes, format.swatches, format.palette, view?.editable]);
    if (json === galleryJson) return;
    galleryJson = json;
    gallery.replaceChildren();
    // A radio group only once it has radios: an empty `radiogroup` (no chart
    // selected yet) is announced with nothing in it. Until then it is a named
    // `group` — a role that may carry the name and may be empty.
    gallery.setAttribute("role", (format.palettes ?? []).length ? "radiogroup" : "group");
    (format.palettes ?? []).forEach((palette, index) => {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "chart-palette chart-palette-ribbon";
      btn.dataset.command = `chart.style.${palette}`;
      btn.setAttribute("role", "radio");
      const on = format.palette === palette;
      btn.setAttribute("aria-checked", String(on));
      btn.tabIndex = on || (!format.palette && index === 0) ? 0 : -1;
      const label = io.t(PALETTE_KEYS[palette] ?? "chart.palette.colorful");
      btn.setAttribute("aria-label", label);
      btn.title = label;
      btn.disabled = !view?.editable;
      for (const color of format.swatches?.[index] ?? []) {
        const dot = document.createElement("span");
        dot.className = "chart-palette-dot";
        dot.style.background = color;
        btn.append(dot);
      }
      btn.addEventListener("click", () => {
        const node = selectedChart();
        const command = commandsFor(node)
          .find((entry) => entry.id === "chart.style")
          ?.submenu?.find((entry) => entry.id === `chart.style.${palette}`);
        if (command?.enabled !== false) command?.run();
      });
      gallery.append(btn);
    });
  }

  /** Keeps the tab, the panel and the dialog honest after any repaint. */
  function sync() {
    const node = selectedChart();
    if (tab) {
      io.reflectTab(tab, { enabled: !!node, reasonKey: "chart.reason.noChartSelected" });
      if (tab.disabled && tab.getAttribute("aria-selected") === "true") io.selectTab("home");
    }
    if (node && band && !band.hidden) renderGallery(bridge.view(node));
    panel.sync();
    dialog.sync();
  }

  /** Insert ▸ Chart: runs `insert`, finds the chart it created (the engine
   *  reports the caret, not the object), selects it with `select`, and opens
   *  its data — ONLYOFFICE's and Word's sequence. */
  async function insertAndOpen(insert, select) {
    const before = placedObjectIds(io.doc());
    const inserted = await insert();
    const chart = inserted ? newestObject(io.doc(), before, "chart") : null;
    if (!chart) return false;
    select(chart);
    return dialog.open(chart.node);
  }

  return {
    insertAndOpen,
    openData: (node = selectedChart()) => (node ? dialog.open(node) : false),
    openSettings: (node = selectedChart()) => (node ? panel.open(node) : false),
    commandsFor,
    sync,
    dialog,
    panel,
  };
}
