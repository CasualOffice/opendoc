// Docked side panels: one rule for all of them (`docs/184` D4, `docs/109`
// UX-055, approved by the owner 2026-11-01).
//
// The audit found ten panels in five widths, docked left or right with no rule,
// drawn as floating cards. The stylesheet now docks every `.side-panel` to the
// window edge at full workspace height (`style.css`, "Docked side panels"); this
// module owns the behaviour the stylesheet cannot express, for every panel at
// once rather than per panel:
//
//   * ONE PANEL PER SIDE. A panel opening on a side closes the one already
//     there, through that panel's own close button, so its module's state (the
//     rail's pressed state, a pending edit) is updated the way a person closing
//     it would update it. Left is what precedes the viewport in the workarea
//     (Outline, Pages); right is what follows it.
//   * OPENS AT THE TOP. A panel that was scrolled when it closed, or whose
//     first focusable control sits low, opened mid-way; every `.panel-body` is
//     reset to the top when its panel is shown.
//   * FOCUS RETURNS to whatever opened the panel when it closes and focus was
//     inside it (or nowhere).
//   * RESIZABLE, 17.5-30rem, remembered per side. The separator is keyboard
//     operable (arrow keys, Home/End) and named.
//
// Complexity: every handler is O(number of panels), never O(document).
import { t } from "./i18n.mjs";

const MIN_REM = 17.5;
const MAX_REM = 30;
const DEFAULT_REM = 20;
const STORE = (side) => `opendoc.sidePanelWidth.${side}`;

function readWidth(storage, side) {
  try {
    const value = Number.parseFloat(storage?.getItem(STORE(side)) ?? "");
    return Number.isFinite(value) ? Math.min(MAX_REM, Math.max(MIN_REM, value)) : DEFAULT_REM;
  } catch {
    return DEFAULT_REM;
  }
}

/** Installs the docking behaviour over every `.side-panel` in `doc`'s workarea,
 *  including panels built later (the object inspector, a chart's panel). */
export function installSidePanels(doc = document, { storage = globalThis.localStorage } = {}) {
  const workarea = doc.querySelector(".workarea");
  const viewport = doc.getElementById("viewport");
  if (!workarea || !viewport) return null;
  const root = doc.documentElement;
  const widths = { left: readWidth(storage, "left"), right: readWidth(storage, "right") };
  const apply = (side) => root.style.setProperty(`--panel-w-${side}`, `${widths[side]}rem`);
  apply("left");
  apply("right");

  const sideOf = (panel) =>
    viewport.compareDocumentPosition(panel) & Node.DOCUMENT_POSITION_FOLLOWING ? "right" : "left";
  const panels = () => [...workarea.querySelectorAll(":scope > .side-panel")];
  const shown = (panel) => !panel.hidden && panel.getClientRects().length > 0;
  const invoker = new WeakMap();
  const wasShown = new WeakMap();
  // The last control focused OUTSIDE every panel: by the time the observer
  // runs, a panel's own open path has usually moved focus into it already.
  let lastOutside = null;
  doc.addEventListener(
    "focusin",
    (event) => {
      if (event.target instanceof HTMLElement && !event.target.closest(".side-panel")) lastOutside = event.target;
    },
    true,
  );

  function closeButton(panel) {
    const buttons = panel.querySelectorAll(".panel-head .panel-close");
    return buttons[buttons.length - 1] ?? null;
  }

  function addResizer(panel) {
    if (panel.querySelector(":scope > .panel-resizer")) return;
    const side = sideOf(panel);
    const handle = doc.createElement("div");
    handle.className = `panel-resizer panel-resizer-${side}`;
    handle.setAttribute("role", "separator");
    handle.setAttribute("aria-orientation", "vertical");
    handle.setAttribute("aria-valuemin", String(MIN_REM * 16));
    handle.setAttribute("aria-valuemax", String(MAX_REM * 16));
    handle.tabIndex = 0;
    const label = () => {
      handle.setAttribute("aria-label", t("sidePanel.resize"));
      handle.setAttribute("aria-valuenow", String(Math.round(widths[side] * 16)));
    };
    const set = (rem) => {
      widths[side] = Math.min(MAX_REM, Math.max(MIN_REM, rem));
      apply(side);
      label();
      try {
        storage?.setItem(STORE(side), String(widths[side]));
      } catch {
        // Private mode: the width still applies for this session.
      }
    };
    handle.addEventListener("pointerdown", (event) => {
      event.preventDefault();
      handle.setPointerCapture(event.pointerId);
      const box = panel.getBoundingClientRect();
      const rootPx = Number.parseFloat(getComputedStyle(root).fontSize) || 16;
      const move = (e) => {
        const px = side === "left" ? e.clientX - box.left : box.right - e.clientX;
        set(px / rootPx);
      };
      const up = () => {
        handle.removeEventListener("pointermove", move);
        handle.removeEventListener("pointerup", up);
        handle.removeEventListener("pointercancel", up);
      };
      handle.addEventListener("pointermove", move);
      handle.addEventListener("pointerup", up);
      handle.addEventListener("pointercancel", up);
    });
    handle.addEventListener("keydown", (event) => {
      const grow = side === "left" ? "ArrowRight" : "ArrowLeft";
      const shrink = side === "left" ? "ArrowLeft" : "ArrowRight";
      if (event.key === grow) set(widths[side] + 1);
      else if (event.key === shrink) set(widths[side] - 1);
      else if (event.key === "Home") set(MIN_REM);
      else if (event.key === "End") set(MAX_REM);
      else return;
      event.preventDefault();
    });
    // Named when first shown, not at boot: chrome built at boot must not call
    // t() before the catalogue lands (`chrome-raw-keys.spec.mjs`).
    panel.append(handle);
    label();
  }

  function opened(panel) {
    const active = doc.activeElement;
    invoker.set(panel, active instanceof HTMLElement && !panel.contains(active) && active !== doc.body ? active : lastOutside);
    addResizer(panel);
    const side = sideOf(panel);
    for (const other of panels()) {
      if (other !== panel && shown(other) && sideOf(other) === side) closeButton(other)?.click();
    }
    const top = () => panel.querySelectorAll(".panel-body").forEach((body) => (body.scrollTop = 0));
    top();
    // Again after the panel's own open path has focused a control and filled it.
    requestAnimationFrame(() => requestAnimationFrame(top));
  }

  function closed(panel) {
    const back = invoker.get(panel);
    invoker.delete(panel);
    const active = doc.activeElement;
    const lost = !active || active === doc.body || panel.contains(active);
    if (lost && back?.isConnected && back.getClientRects().length > 0) back.focus({ preventScroll: true });
  }

  function sync() {
    for (const panel of panels()) {
      const now = shown(panel);
      if (now === (wasShown.get(panel) ?? false)) continue;
      wasShown.set(panel, now);
      if (now) opened(panel);
      else closed(panel);
    }
  }

  // Only the workarea's own children and each panel's `hidden` are watched:
  // the page stack beneath the viewport changes on every keystroke, and none of
  // that is this module's business.
  const watched = new WeakSet();
  const panelObserver = new MutationObserver(sync);
  const watch = () => {
    for (const panel of panels()) {
      if (watched.has(panel)) continue;
      watched.add(panel);
      panelObserver.observe(panel, { attributes: true, attributeFilter: ["hidden"] });
    }
    sync();
  };
  const listObserver = new MutationObserver(watch);
  listObserver.observe(workarea, { childList: true });
  watch();
  return {
    sync,
    disconnect: () => {
      listObserver.disconnect();
      panelObserver.disconnect();
    },
  };
}
