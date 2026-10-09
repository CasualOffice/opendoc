// The deck viewer's share of the document editor's shell.
//
// The owner's rule for this page is that a reader moving between a document and a
// deck should not meet a different application: the same header, the same two
// toolbar modes behind the same switch, the same status bar with the same zoom
// control. `editor.html` builds those from markup that `main.js` binds by id, and
// `main.js` has no mount seam (`109` HF-109), so its controllers cannot be called
// from a second page. What CAN be shared already is — the radio group, the
// preference store, the ribbon's roving-tab arithmetic, the zoom ladder and its
// input parser, the popover manager — and this module puts the slide page's
// commands into the editor's own markup vocabulary (`.ribbon-tab`,
// `.ribbon-panel`, `.rgroup`, `.fmt`, `.zoom`, `.zoom-preset`) on top of them. The
// rest of the shell moves here when it is lifted out of `main.js`.
//
// Every control resolves through the SAME command registry the compact toolbar
// and the menu bar read (`slide_commands.mjs`), so a command has one label, one
// enablement, one pressed state and one activation path on every surface
// (`105` UX-004/UX-005).

import { readPref, writePref } from "./prefs.mjs";
import { bindRadioGroup } from "./radio_group.mjs";
import { rovingIndex } from "./ribbon_nav.mjs";
import { ZOOM_STEPS, parseZoomInput } from "./view_zoom.mjs";

/** The editor's own toolbar-mode preference key. ONE preference for both
 *  surfaces: a reader who chose Compact in the editor gets Compact here too, which
 *  is what "the UX does not differ" means for a setting. */
export const CHROME_MODE_PREF = "opendoc.chromeMode";

/** Icons for the theme rows, which the compact bar does not show and so
 *  `slide_commands.mjs`'s icon table does not carry. */
const THEME_ICONS = {
  "view.theme.system": "brightness_auto",
  "view.theme.light": "light_mode",
  "view.theme.dark": "dark_mode",
};

/** The ribbon's tabs, in the editor's order and under the editor's tab names,
 *  laid out in the editor's three band shapes.
 *
 *  File, Home and View are the three of the editor's seven tabs a VIEWER has
 *  commands for; a tab with nothing on it would be the dead chrome `docs/63`
 *  forbids. Their captions are the editor's own catalogue keys, so the tab is
 *  the same word in every language on both pages.
 *
 *  Each group is a list of PARTS, in the editor's vocabulary:
 *
 *  - `{ big: id, labelKey }` — a large button with its caption under the icon
 *    (`.fmt.fmt-big`), the editor's Paste / Table / Picture shape: the group's
 *    primary verb.
 *  - `{ stack: [ids] }` — small buttons stacked in one column (`.stackrows`),
 *    the editor's Cut / Copy beside Paste.
 *  - `{ rows: [[ids], [ids]] }` — small buttons in two lines (`.fmt-rows` /
 *    `.fmt-row-line`), the editor's Font and Paragraph bands.
 */
export const SLIDE_RIBBON = [
  {
    tab: "file",
    labelKey: "documentChrome.file",
    groups: [
      {
        labelKey: "slides.toolbarGroupFile",
        parts: [
          { big: "file.open", labelKey: "slides.open" },
          { big: "file.save", labelKey: "slides.save" },
        ],
      },
    ],
  },
  {
    tab: "home",
    labelKey: "documentChrome.home",
    groups: [
      {
        labelKey: "slides.toolbarGroupFile",
        parts: [
          { big: "file.open", labelKey: "slides.open" },
          { stack: ["file.save"] },
        ],
      },
      {
        labelKey: "slides.toolbarGroupSlide",
        parts: [
          {
            rows: [
              ["slide.previous", "slide.next"],
              ["slide.first", "slide.last"],
            ],
          },
        ],
      },
      {
        labelKey: "slides.toolbarGroupZoom",
        parts: [
          { big: "view.fitSlide", labelKey: "slides.fitSlide" },
          { stack: ["view.zoomIn", "view.zoomOut"] },
        ],
      },
      {
        labelKey: "slides.toolbarGroupPanels",
        parts: [
          { big: "view.slides", labelKey: "slides.sorter" },
          { big: "view.fidelity", labelKey: "slides.fidelityPanel" },
        ],
      },
    ],
  },
  {
    tab: "view",
    labelKey: "documentChrome.view",
    groups: [
      {
        labelKey: "slides.toolbarGroupPanels",
        parts: [
          { big: "view.slides", labelKey: "slides.sorter" },
          { big: "view.fidelity", labelKey: "slides.fidelityPanel" },
        ],
      },
      {
        labelKey: "slides.toolbarGroupZoom",
        parts: [
          { big: "view.fitSlide", labelKey: "slides.fitSlide" },
          { big: "view.fitWidth", labelKey: "slides.fitWidth" },
          { stack: ["view.zoomIn", "view.zoomOut"] },
        ],
      },
      {
        labelKey: "slides.menuGroupTheme",
        // Two lines, never three: the editor's band is two rows tall on every
        // tab, and a taller group would make the ribbon jump between tabs.
        parts: [{ rows: [["view.theme.system"], ["view.theme.light", "view.theme.dark"]] }],
      },
    ],
  },
];

/** Every command id a ribbon group names, in order, whatever its layout. */
export function groupCommandIds(group) {
  return group.parts.flatMap((part) =>
    part.big ? [part.big] : part.stack ? part.stack : part.rows.flat(),
  );
}

/** The tab a reader lands on, as in the editor. */
const DEFAULT_TAB = "home";

/**
 * The tooltip a ribbon button carries: the command's label and its chord, or the
 * reason it cannot run. The same sentence the compact bar shows, so a disabled
 * control says why on both toolbars.
 */
export function ribbonTitle(command, localizeShortcut = (text) => text) {
  if (command.enabled === false && command.disabledReason) return command.disabledReason;
  return command.shortcut
    ? `${command.label} (${localizeShortcut(command.shortcut)})`
    : command.label;
}

/**
 * Renders the ribbon into the editor's markup and keeps it in step with the
 * registry.
 *
 * `tablist` is the header's `.ribbon-tabs`, `body` is `.ribbon-body`. Buttons are
 * built once and REFLECTED on `render()`, rather than rebuilt, so a keyboard user
 * whose focus is on a ribbon button keeps it when a slide change re-renders.
 *
 * Complexity: O(controls) per render, a few dozen elements, independent of the
 * deck's size.
 */
export function createSlideRibbon({
  tablist,
  body,
  editorCommands,
  onButton,
  t,
  localizeShortcut = (text) => text,
  table = SLIDE_RIBBON,
  doc = globalThis.document,
}) {
  const tabs = [];
  const panels = [];
  /** Every rendered control, with the command id it runs. */
  const controls = [];
  /** Every large button's caption, re-labelled on a language change. */
  const captions = [];

  /** One ribbon control bound to one registry command. */
  function control(id, className) {
    const button = doc.createElement("button");
    button.type = "button";
    button.className = className;
    button.dataset.command = id;
    const icon = doc.createElement("span");
    icon.className = "ms";
    icon.setAttribute("aria-hidden", "true");
    button.append(icon);
    onButton(button, () => {
      const command = lookup().get(id);
      if (!command || command.enabled === false) return;
      command.run();
    });
    controls.push({ id, button, icon });
    return button;
  }
  const lookup = () => new Map(editorCommands().map((command) => [command.id, command]));

  for (const entry of table) {
    // The markup's tab when it declares one — its caption is a `data-i18n` key
    // the page's localisation sweep translates — and a built one otherwise.
    let tab = tablist.querySelector(`[data-tab="${entry.tab}"]`);
    if (!tab) {
      tab = doc.createElement("button");
      tab.type = "button";
      tab.className = "ribbon-tab";
      tab.dataset.tab = entry.tab;
      tab.setAttribute("role", "tab");
      tab.textContent = t(entry.labelKey);
      tab.dataset.builtCaption = "true";
      tablist.append(tab);
    }
    tab.id = `slidesTab-${entry.tab}`;
    tab.setAttribute("aria-controls", `slidesPanel-${entry.tab}`);
    tabs.push(tab);

    const panel = doc.createElement("div");
    panel.className = "ribbon-panel";
    panel.id = `slidesPanel-${entry.tab}`;
    panel.dataset.panel = entry.tab;
    panel.setAttribute("role", "tabpanel");
    panel.setAttribute("aria-labelledby", tab.id);
    for (const group of entry.groups) {
      const box = doc.createElement("div");
      box.className = "rgroup";
      const ctl = doc.createElement("div");
      ctl.className = "rgroup-ctl";
      for (const part of group.parts) {
        if (part.big) {
          // The editor's large button: icon over caption.
          const button = control(part.big, "fmt fmt-big");
          const caption = doc.createElement("span");
          caption.className = "fmt-big-label";
          caption.textContent = t(part.labelKey);
          button.append(caption);
          captions.push({ caption, labelKey: part.labelKey });
          ctl.append(button);
        } else if (part.stack) {
          const stack = doc.createElement("div");
          stack.className = "stackrows";
          for (const id of part.stack) stack.append(control(id, "fmt"));
          ctl.append(stack);
        } else {
          const rows = doc.createElement("div");
          rows.className = "fmt-rows";
          for (const line of part.rows) {
            const row = doc.createElement("div");
            row.className = "fmt-row-line";
            for (const id of line) row.append(control(id, "fmt"));
            rows.append(row);
          }
          ctl.append(rows);
        }
      }
      const label = doc.createElement("span");
      label.className = "rgroup-label";
      label.textContent = t(group.labelKey);
      box.append(ctl, label);
      panel.append(box);
    }
    body.append(panel);
    panels.push(panel);
  }

  /** Selects one tab and shows its panel, through the same ARIA contract the
   *  editor's tab strip keeps: one `aria-selected="true"`, one tab stop. */
  function select(name, { focus = false } = {}) {
    for (const [index, tab] of tabs.entries()) {
      const on = tab.dataset.tab === name;
      tab.setAttribute("aria-selected", String(on));
      tab.tabIndex = on ? 0 : -1;
      panels[index].hidden = !on;
      if (on && focus) tab.focus();
    }
  }

  for (const tab of tabs) {
    tab.addEventListener("click", () => select(tab.dataset.tab));
    // The editor's roving arithmetic: arrows move between tabs and wrap, Home and
    // End jump to the ends, and the panel follows the selection.
    tab.addEventListener("keydown", (event) => {
      const at = tabs.indexOf(tab);
      const next = rovingIndex(event.key, at, tabs.length);
      if (next === null || next === undefined || next === at) return;
      event.preventDefault();
      select(tabs[next].dataset.tab, { focus: true });
    });
  }

  /** Reflects every control from the registry: label, tooltip, enablement and,
   *  for a toggle, its pressed state. */
  function render() {
    const commands = lookup();
    for (const { id, button, icon } of controls) {
      const command = commands.get(id);
      if (!command) {
        // A row naming a command the registry does not have is a table bug. It
        // is shown disabled rather than hidden, so the bug is visible.
        button.disabled = true;
        continue;
      }
      icon.textContent = command.icon ?? THEME_ICONS[id] ?? "";
      button.setAttribute("aria-label", command.label);
      button.title = ribbonTitle(command, localizeShortcut);
      button.disabled = command.enabled === false;
      if (command.checked === undefined) button.removeAttribute("aria-pressed");
      else button.setAttribute("aria-pressed", String(Boolean(command.checked)));
    }
  }

  /** Re-labels the tab captions and group labels after a language change. */
  function relabel() {
    for (const [index, entry] of table.entries()) {
      if (tabs[index].dataset.builtCaption) tabs[index].textContent = t(entry.labelKey);
      const labels = panels[index].querySelectorAll(".rgroup-label");
      for (const [g, group] of entry.groups.entries()) {
        if (labels[g]) labels[g].textContent = t(group.labelKey);
      }
    }
    for (const { caption, labelKey } of captions) caption.textContent = t(labelKey);
    render();
  }

  select(DEFAULT_TAB);
  render();
  return { render, relabel, select, tabs, panels };
}

/**
 * The Compact/Ribbon switch, sharing the editor's preference.
 *
 * `body` carries exactly one of `compact-mode`/`ribbon-mode`, which is the class
 * pair the editor's stylesheet keys both toolbars, the menu bar and the tab strip
 * off. `isPhone` forces compact as the editor does — a phone has room for one
 * navigation axis and the ribbon is not it — without touching the preference, so
 * growing back past the rung restores the reader's choice.
 */
export function createChromeMode({ group, body, compactToolbar, isPhone = () => false, onChange }) {
  let mode = readPref(CHROME_MODE_PREF, "ribbon") === "compact" ? "compact" : "ribbon";
  const radios = bindRadioGroup(group, {
    attr: "data-chrome-mode",
    onSelect: (value) => set(value),
  });
  function set(next, { persist = true } = {}) {
    mode = next === "compact" ? "compact" : "ribbon";
    const compact = mode === "compact" || isPhone();
    body.classList.toggle("compact-mode", compact);
    body.classList.toggle("ribbon-mode", !compact);
    if (compactToolbar) compactToolbar.hidden = !compact;
    radios?.reflect?.(mode);
    if (persist) writePref(CHROME_MODE_PREF, mode);
    onChange?.(compact ? "compact" : "ribbon");
  }
  set(mode, { persist: false });
  return {
    mode: () => mode,
    set,
    /** Re-applies the mode when the phone rung is crossed. */
    refresh: () => set(mode, { persist: false }),
  };
}

/**
 * The status bar's zoom control, in the editor's markup: −, a percentage you can
 * type into, a presets caret, +.
 *
 * The percentage is the viewer's multiplier over its FIT, which is what "100%"
 * means on a deck in both reference products: the whole slide, or the desk's
 * width, depending on the fit in force. The presets are the editor's own ladder
 * (`ZOOM_STEPS`), plus the two fits.
 */
export function createSlideZoom({
  elements: { zoomOut, zoomIn, zoomInput, zoomMenuBtn, zoomMenu },
  viewer,
  repaint,
  registerPopover,
  t,
  doc = globalThis.document,
}) {
  const percent = () => `${Math.round(viewer.zoomFactor() * 100)}%`;

  function step(direction) {
    if (!viewer.canZoom(direction)) return;
    viewer.stepZoom(direction);
    repaint();
  }

  function setFactor(factor) {
    viewer.setZoomFactor(factor);
    repaint();
  }

  /** Commits the typed value as the editor's `commitZoomInput` does: a fit word
   *  sets that fit, a percentage sets the factor, anything else restores the last
   *  valid readout. `parseZoomInput` answers `{mode}` or `{factor}` — an object,
   *  so testing the answer itself for truth accepts everything and sets nothing. */
  function commit() {
    const asked = parseZoomInput(zoomInput.value);
    if (asked.mode) {
      viewer.setFit(asked.mode === "fit-width" ? "width" : "slide");
      repaint();
    } else if (asked.factor) {
      setFactor(asked.factor);
    } else {
      zoomInput.value = percent();
    }
  }

  zoomOut.addEventListener("click", () => step(-1));
  zoomIn.addEventListener("click", () => step(1));
  zoomInput.addEventListener("change", commit);
  zoomInput.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      commit();
      // The editor leaves the field on Enter, and the blur below rewrites the
      // readout, which `render` deliberately does not do while it has focus.
      zoomInput.blur();
    } else if (event.key === "Escape") {
      zoomInput.value = percent();
      zoomInput.blur();
    }
  });
  zoomInput.addEventListener("blur", () => {
    zoomInput.value = percent();
  });

  /** One row of the presets menu, in the editor's `.zoom-preset` shape. */
  function row(label, checked, run) {
    const item = doc.createElement("button");
    item.type = "button";
    item.className = "menu-item zoom-preset";
    item.setAttribute("role", "menuitemradio");
    item.setAttribute("aria-checked", String(checked));
    const check = doc.createElement("span");
    check.className = "menu-check";
    check.setAttribute("aria-hidden", "true");
    const text = doc.createElement("span");
    text.className = "menu-item-label";
    text.textContent = label;
    item.append(check, text);
    item.addEventListener("click", () => {
      popover?.close?.();
      run();
    });
    return item;
  }

  function buildMenu() {
    const fit = viewer.currentFit();
    const factor = viewer.zoomFactor();
    const rows = ZOOM_STEPS.map((stepFactor) =>
      row(`${Math.round(stepFactor * 100)}%`, Math.abs(stepFactor - factor) < 1e-6, () =>
        setFactor(stepFactor),
      ),
    );
    const divider = doc.createElement("div");
    divider.className = "menu-divider";
    divider.setAttribute("role", "separator");
    rows.push(
      divider,
      row(t("slides.fitSlide"), fit === "slide" && factor === 1, () => {
        viewer.setFit("slide");
        repaint();
      }),
      row(t("slides.fitWidth"), fit === "width" && factor === 1, () => {
        viewer.setFit("width");
        repaint();
      }),
    );
    zoomMenu.replaceChildren(...rows);
  }

  const popover = registerPopover?.(zoomMenuBtn, zoomMenu, buildMenu, { needsDocument: false });

  /** Reflects the readout and the steppers' enablement. */
  function render() {
    const open = viewer.slideCount() > 0;
    if (doc.activeElement !== zoomInput) zoomInput.value = percent();
    zoomInput.disabled = !open;
    zoomMenuBtn.disabled = !open;
    zoomOut.disabled = !open || !viewer.canZoom(-1);
    zoomIn.disabled = !open || !viewer.canZoom(1);
  }

  render();
  return { render };
}

/**
 * The deck engine's fidelity report, restated in the shape the editor's
 * findings dialog reads (`compat_findings.mjs`), so a deck's findings open in
 * the SAME dialog — grouped into lost / approximated / kept, in words — that a
 * document's do.
 *
 * Field for field: the deck engine names the part, element and attribute flat;
 * the editor's report nests them as a location under the names its findings
 * panel already reads. Nothing is re-decided: the model and retention outcomes
 * are the engine's own.
 *
 * Complexity: O(findings).
 */
export function deckReportJson(findings) {
  // The deck engine spells an outcome as `35-DISPOSITION-TAXONOMY.md` does
  // (`not-retained`); the document engine's JSON, which the editor's dialog
  // reads, spells the same token with an underscore (`not_retained`). Restated
  // here so a deck's losses group as LOST rather than falling through to "other".
  const token = (value) => (typeof value === "string" ? value.replaceAll("-", "_") : value);
  return JSON.stringify({
    entries: findings.map((finding) => ({
      feature: finding.feature,
      occurrences: finding.occurrences ?? 1,
      location: {
        partName: finding.part ?? null,
        namespace: null,
        localName: finding.element ?? null,
        attributeName: finding.attribute ?? null,
      },
      disposition: token(finding.disposition),
      modelOutcome: token(finding.modelOutcome),
      retentionOutcome: token(finding.retentionOutcome),
    })),
    ledger: [],
  });
}
