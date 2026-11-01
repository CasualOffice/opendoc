// One Properties panel (approved design D5, 2026-11-02; ONLYOFFICE's right-hand
// settings panel is the model): Paragraph, Table and Picture properties read as
// ONE panel whose tab follows the selection, instead of three panels built three
// ways. The three existing panels keep their bodies, ids and engine wiring; this
// module gives each the same tab strip under its header, and a tab click swaps
// which of them is open in the same docked slot.
//
// A tab shows only when it applies: Paragraph whenever there is a caret, Table
// while the caret is in a table (the contextual Table ribbon tab is enabled),
// Picture while an object is selected. A strip with a single tab is hidden.
//
// Complexity: O(1) per refresh (three panels, three tabs); refreshes run when a
// panel's `hidden` changes or the object inspector is created.
import { t } from "./i18n.mjs";

/** Installs the strips. `ed` is the editor context (docs/177). */
export function startPropertiesTabs(ed) {
  const workarea = document.querySelector(".workarea");
  if (!workarea) return;

  const TABS = [
    {
      key: "paragraph",
      text: () => t("propertiesTabs.paragraph"),
      panel: () => ed.paragraphPropertiesPanel,
      available: () => !!ed.doc && !!ed.selection,
      open: () => ed.toggleParagraphProperties?.(true),
      close: () => ed.toggleParagraphProperties?.(false),
    },
    {
      key: "table",
      text: () => t("propertiesTabs.table"),
      panel: () => ed.tablePropertiesPanel,
      available: () => {
        const tab = document.querySelector('.ribbon-tab[data-tab="table"]');
        return !!tab && !tab.disabled && tab.getAttribute("aria-disabled") !== "true";
      },
      open: () => ed.toggleTableProperties?.(true),
      close: () => ed.toggleTableProperties?.(false),
    },
    {
      key: "picture",
      text: () => t("propertiesTabs.picture"),
      panel: () => workarea.querySelector(".side-panel.object-inspector"),
      available: () => !!ed.objectSelection,
      open: () => ed.toggleObjectInspector?.(true),
      close: () => ed.toggleObjectInspector?.(false),
    },
  ];

  function select(target) {
    for (const tab of TABS) if (tab !== target && tab.panel() && !tab.panel().hidden) tab.close();
    target.open();
    queueMicrotask(refresh);
  }

  /** Puts a strip under the panel's header once. */
  function stripFor(panel) {
    let strip = panel.querySelector(":scope > .properties-tabs");
    if (strip) return strip;
    strip = document.createElement("div");
    strip.className = "properties-tabs";
    strip.setAttribute("role", "tablist");
    for (const tab of TABS) {
      const button = document.createElement("button");
      button.type = "button";
      button.setAttribute("role", "tab");
      button.dataset.propertiesTab = tab.key;
      button.addEventListener("click", () => select(tab));
      button.addEventListener("keydown", (e) => {
        const shown = [...strip.querySelectorAll("[role=tab]:not([hidden])")];
        const i = shown.indexOf(button);
        const step = { ArrowRight: 1, ArrowLeft: -1 }[e.key];
        if (!step || shown.length < 2) return;
        e.preventDefault();
        const next = shown[(i + step + shown.length) % shown.length];
        select(TABS.find((x) => x.key === next.dataset.propertiesTab));
        queueMicrotask(() => {
          const panelNow = TABS.find((x) => x.key === next.dataset.propertiesTab).panel();
          panelNow?.querySelector(`[data-properties-tab="${next.dataset.propertiesTab}"]`)?.focus();
        });
      });
      strip.append(button);
    }
    const head = panel.querySelector(":scope > .panel-head, :scope > header");
    if (head) head.after(strip);
    else panel.prepend(strip);
    return strip;
  }

  function refresh() {
    for (const owner of TABS) {
      const panel = owner.panel();
      if (!panel || panel.hidden) continue;
      const strip = stripFor(panel);
      let shown = 0;
      for (const button of strip.querySelectorAll("[role=tab]")) {
        const tab = TABS.find((x) => x.key === button.dataset.propertiesTab);
        const on = tab === owner;
        const visible = on || tab.available();
        // Written only when they change: this runs from a MutationObserver that
        // watches `hidden`, and re-setting an attribute to its own value still
        // queues a record, which would loop.
        if (button.hidden !== !visible) button.hidden = !visible;
        if (visible) shown += 1;
        const text = tab.text();
        if (button.textContent !== text) button.textContent = text;
        if (button.getAttribute("aria-selected") !== String(on)) button.setAttribute("aria-selected", String(on));
        button.tabIndex = on ? 0 : -1;
      }
      if (strip.hidden !== shown < 2) strip.hidden = shown < 2;
    }
  }

  new MutationObserver(refresh).observe(workarea, {
    subtree: true,
    childList: true,
    attributes: true,
    attributeFilter: ["hidden"],
  });
  refresh();
}
