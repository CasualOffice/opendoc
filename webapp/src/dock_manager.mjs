// SPDX-License-Identifier: Apache-2.0
// The dock manager (`docs/184` D4 as amended 2026-11-06, PROPOSAL D-10;
// `docs/109` UX-064, audit DIALOGS-01/02/04/05).
//
// ONE SLOT PER SIDE. Word's task panes share one dock, ONLYOFFICE's RightMenu is
// one panel whose content changes, and Google Docs shows one panel per edge: in
// all three, opening a pane replaces the pane already on that side, and the page
// is never pushed off the screen. Before this module the rule was written pair
// by pair (the outline closed Pages, the add-in panel closed the comments, the
// version panel closed the comments) and four panels had no rule at all, so at
// 1280px Pages + Version history + Paragraph properties + Symbol stood side by
// side and no part of the page was visible (DIALOGS-01, P0).
//
// THE MECHANISM IS A CHOKE POINT, NOT A CONVENTION. Every panel in this editor
// opens by clearing its `hidden` attribute, from a dozen call sites (rail,
// ribbon, menus, palette, shortcuts, host commands, the properties tabs). Rather
// than asking each of them to call a `dock.open()` (and finding the thirteenth
// later), the manager watches `hidden` on every registered panel. When one
// becomes visible it closes whatever else is visible on its side, through that
// panel's OWN close control — so the closing panel's rail button, its state and
// its cleanup run exactly as if the person had pressed its ×. A MutationObserver
// callback is a microtask: the two panels are never painted side by side.
//
// FOCUS. The manager records the invoker when a panel opens (the last focused
// control outside the dock, or the editor when the panel was opened from the
// palette, a menu or a shortcut), closes the panel on Escape from anywhere
// inside it, and returns focus to the invoker — or to the editor when the
// invoker is gone. A panel that closes while it holds focus never leaves the
// focus on `<body>` (DIALOGS-05).
//
// WIDTH. Each side has one width, default 20rem, resizable 17.5–30rem on the
// panel's inner edge, remembered on this device (DIALOGS-04). The width is a
// rem custom property on the root (`--dock-w-left` / `--dock-w-right`), so it
// grows with the fluid root like the rest of the chrome. The add-in panel keeps
// its own splitter and width (`plugin_panel_width.mjs`, `docs/183` §21) but
// takes the right slot like every other right panel.
//
// THE COMMENT COLUMN is in the right slot too: it is displaced by a right panel
// and comes back when that panel closes, as Docs' margin comments do when its
// side panel closes. That also takes the comment gutter out of the page stack
// while a right panel is open, which is what made the page scroll sideways under
// a right panel (DIALOGS-02).
//
// Complexity: O(panels) per `hidden` change (a dozen elements); nothing here
// reads the document.
import { t } from "./i18n.mjs";

/** Width bounds, in rem (chrome is rem, `docs/184` D8). */
export const DOCK_WIDTH = Object.freeze({ min: 17.5, default: 20, max: 30, step: 1, bigStep: 4 });

/** Panels docked LEFT (navigation, `docs/184` D4). Everything else is right. */
export const LEFT_PANELS = Object.freeze(["outlinePanel", "pagesPanel"]);

const KEY = (side) => `opendoc.dock.${side}.width`;

/** `width` (rem) held within the bounds; anything not finite is the default. Pure. */
export function clampDockWidth(width) {
  const n = Number(width);
  if (width === null || width === "" || !Number.isFinite(n)) return DOCK_WIDTH.default;
  return Math.min(DOCK_WIDTH.max, Math.max(DOCK_WIDTH.min, Math.round(n * 8) / 8));
}

/** The width a key press on a side's splitter asks for, or null. The splitter is
 *  on the panel's INNER edge, so the arrow pointing away from the panel widens
 *  it: ← widens a right panel, → widens a left one (mirrored in RTL). Pure. */
export function dockWidthForKey(key, { width, side, rtl = false, shift = false }) {
  const step = shift ? DOCK_WIDTH.bigStep : DOCK_WIDTH.step;
  const outward = (side === "right") !== rtl ? "ArrowLeft" : "ArrowRight";
  const inward = outward === "ArrowLeft" ? "ArrowRight" : "ArrowLeft";
  if (key === outward) return clampDockWidth(width + step);
  if (key === inward) return clampDockWidth(width - step);
  if (key === "Home") return DOCK_WIDTH.min;
  if (key === "End") return DOCK_WIDTH.max;
  return null;
}

/** The side a panel docks on. Pure. */
export function dockSide(panel) {
  if (!panel) return null;
  if (panel.dataset?.dock === "left" || panel.dataset?.dock === "right") return panel.dataset.dock;
  return LEFT_PANELS.includes(panel.id) ? "left" : "right";
}

/** Of the panels visible on one side, the one to keep: the one just opened.
 *  When several opened in the same tick, the one holding focus, else the last in
 *  document order (the most recently appended). Pure over its inputs. */
export function survivorOf(visible, opened, focused) {
  const fresh = visible.filter((p) => opened.has(p));
  const pool = fresh.length ? fresh : visible;
  return pool.find((p) => focused && p.contains(focused)) ?? pool[pool.length - 1] ?? null;
}

function loadWidth(side, view) {
  try {
    const raw = view.localStorage?.getItem(KEY(side));
    return raw === null || raw === undefined ? null : clampDockWidth(raw);
  } catch {
    return null;
  }
}

function saveWidth(side, width, view) {
  try {
    if (width === null) view.localStorage?.removeItem(KEY(side));
    else view.localStorage?.setItem(KEY(side), String(width));
  } catch {
    /* storage refused: the width lasts as long as the page */
  }
}

/**
 * Starts the manager. `ed` is the editor context (docs/177); the manager needs
 * `reviewSidebar`, `toggleReview`, `reviewSidebarPreference`,
 * `reanchorPageOverlays` and `focusEditorSurface` from it.
 *
 * @returns {{ panels: () => HTMLElement[], visible: (side?: string) => HTMLElement[], close: (panel: HTMLElement) => void, width: (side: string) => number, setWidth: (side: string, rem: number) => void }}
 */
export function startDockManager(ed, { doc = document, view = window } = {}) {
  const workarea = doc.querySelector(".workarea");
  if (!workarea) return null;
  const root = doc.documentElement;

  /** Every dock panel in the product: the work area's side panels (including the
   *  object inspector and the chart panel, built at runtime) and the comment
   *  column. Read live, so a panel added later is managed without registering. */
  const panels = () => {
    const list = [...workarea.querySelectorAll(":scope > .side-panel")];
    if (ed.reviewSidebar) list.push(ed.reviewSidebar);
    return list;
  };
  const isShown = (p) => !p.hidden && p.isConnected;
  const visible = (side) => panels().filter((p) => isShown(p) && (!side || dockSide(p) === side));

  // ---- Closing a panel through its own control ------------------------------
  /** The panel's own close button: the last `.panel-close` that belongs to it
   *  and is not a menu button (the add-in panel's ⋯ shares the class). */
  const closeButtonOf = (panel) =>
    [...panel.querySelectorAll(".panel-close")]
      .filter((b) => !b.hasAttribute("aria-haspopup") && b.closest(".side-panel") === panel)
      .pop() ?? null;

  /** Review displaced by a right panel, with the preference it had, so it comes
   *  back as it was when that panel closes. `undefined` = not displaced. */
  let displacedReview;

  function close(panel, { displaced = false } = {}) {
    if (!isShown(panel)) return;
    if (panel === ed.reviewSidebar) {
      if (displaced) displacedReview = ed.reviewSidebarPreference ?? null;
      ed.toggleReview?.(false);
      // `toggleReview` hides the column on its next render; hide it now so the
      // slot is free in this tick, not one frame later.
      panel.hidden = true;
      ed.viewportEl?.classList.remove("has-review-sidebar");
      return;
    }
    const button = closeButtonOf(panel);
    if (button && !button.disabled) button.click();
    if (isShown(panel)) panel.hidden = true;
  }

  // ---- Invoker tracking -----------------------------------------------------
  const inDock = (el) => !!el && panels().some((p) => p.contains(el));
  /** Surfaces that are transient: a control inside one is not an invoker to
   *  return to, because it is gone by the time the panel closes. */
  const TRANSIENT = ".cmd-palette, [role='menu'], .app-menu-popover, .dialog-overlay, .context-menu, .menu-sheet";
  let lastOutside = null;
  doc.addEventListener(
    "focusin",
    (event) => {
      const el = event.target;
      if (!(el instanceof view.HTMLElement) || inDock(el)) return;
      lastOutside = el.closest(TRANSIENT) ? null : el;
    },
    true,
  );
  /** panel → the element to return focus to. */
  const invokers = new WeakMap();
  const usable = (el) =>
    !!el && el.isConnected && !el.disabled && el.getClientRects().length > 0 && !inDock(el) && el !== doc.body;
  function returnFocus(panel) {
    const back = invokers.get(panel);
    if (usable(back)) back.focus({ preventScroll: true });
    else ed.focusEditorSurface?.();
  }

  // ---- The slot rule --------------------------------------------------------
  let known = new Set(panels().filter(isShown));
  let reconciling = false;
  function reconcile() {
    if (reconciling) return;
    reconciling = true;
    try {
      const now = panels().filter(isShown);
      const opened = new Set(now.filter((p) => !known.has(p)));
      const closed = [...known].filter((p) => !now.includes(p));
      for (const panel of opened) {
        // Who to give focus back to. A panel that moved focus inside on open has
        // already taken it, so the last control OUTSIDE the dock is the invoker.
        invokers.set(panel, lastOutside);
      }
      for (const side of ["left", "right"]) {
        const onSide = now.filter((p) => dockSide(p) === side);
        if (onSide.length < 2) continue;
        // The comment column shown on its own (a comment arrived, Suggesting
        // began) yields to a panel the person opened; asked for, it takes the
        // slot like any other panel.
        const review = ed.reviewSidebar;
        const autoReview = review && onSide.includes(review) && ed.reviewSidebarPreference !== true;
        const contenders = autoReview ? onSide.filter((p) => p !== review) : onSide;
        const keep = survivorOf(contenders, opened, doc.activeElement);
        const focusBefore = doc.activeElement;
        for (const other of onSide) if (other !== keep) close(other, { displaced: true });
        // The closed panel's own close handler may hand focus to ITS invoker;
        // the panel that just opened keeps the focus it took.
        if (keep.contains(focusBefore) && !keep.contains(doc.activeElement)) focusBefore.focus({ preventScroll: true });
      }
      for (const panel of closed) {
        // Never leave focus on <body> (or inside a now-hidden panel).
        const active = doc.activeElement;
        if (!active || active === doc.body || panel.contains(active)) returnFocus(panel);
      }
      // The comment column comes back when the right panel that displaced it goes.
      if (displacedReview !== undefined && closed.some((p) => dockSide(p) === "right") && visible("right").length === 0) {
        const preference = displacedReview;
        displacedReview = undefined;
        ed.reviewSidebarPreference = preference;
        ed.reanchorPageOverlays?.();
      } else if (opened.has(ed.reviewSidebar) && isShown(ed.reviewSidebar)) {
        // The column took the slot itself; nothing is waiting to come back.
        displacedReview = undefined;
      }
      for (const side of ["left", "right"]) root.toggleAttribute(`data-dock-${side}`, visible(side).length > 0);
      known = new Set(panels().filter(isShown));
      if (opened.size || closed.length) ed.reanchorPageOverlays?.();
    } finally {
      reconciling = false;
    }
  }
  const observer = new view.MutationObserver(reconcile);
  observer.observe(workarea, { subtree: true, childList: true, attributes: true, attributeFilter: ["hidden"] });

  // ---- Escape from anywhere inside a panel ----------------------------------
  // Bubble phase and only when nothing inside handled it: a menu, a combobox or
  // a composer inside the panel that answers Escape keeps that Escape. The
  // comment column runs its own two-step Escape (card, then column), which
  // `editor_review_margin.mjs` owns.
  doc.addEventListener("keydown", (event) => {
    if (event.key !== "Escape" || event.defaultPrevented || event.isComposing) return;
    const target = event.target;
    if (!(target instanceof view.HTMLElement)) return;
    const panel = panels().find((p) => p !== ed.reviewSidebar && isShown(p) && p.contains(target));
    if (!panel) return;
    if (target.closest("[role='menu'], [role='listbox']")) return;
    event.preventDefault();
    close(panel);
    queueMicrotask(() => {
      const active = doc.activeElement;
      if (!active || active === doc.body || panel.contains(active) || active === closeButtonOf(panel)) returnFocus(panel);
      else if (!invokers.has(panel)) ed.focusEditorSurface?.();
    });
  });

  // ---- Width ----------------------------------------------------------------
  const widths = { left: loadWidth("left", view) ?? DOCK_WIDTH.default, right: loadWidth("right", view) ?? DOCK_WIDTH.default };
  const applyWidth = (side) => {
    root.style.setProperty(`--dock-w-${side}`, `${widths[side]}rem`);
    for (const handle of doc.querySelectorAll(`.dock-resize[data-dock-side="${side}"]`)) {
      handle.setAttribute("aria-valuenow", String(widths[side]));
    }
  };
  function setWidth(side, rem, remember = true) {
    widths[side] = clampDockWidth(rem);
    applyWidth(side);
    if (remember) saveWidth(side, widths[side], view);
    ed.reanchorPageOverlays?.();
  }
  applyWidth("left");
  applyWidth("right");

  const rtl = () => doc.documentElement.dir === "rtl";
  const remPx = () => parseFloat(view.getComputedStyle(root).fontSize) || 16;
  /** One splitter per side panel (the add-in panel brings its own). */
  function attachHandle(panel) {
    if (panel === ed.reviewSidebar || panel.classList.contains("plugin-panel")) return;
    if (panel.querySelector(":scope > .dock-resize")) return;
    const side = dockSide(panel);
    const handle = doc.createElement("div");
    handle.className = "dock-resize";
    handle.dataset.dockSide = side;
    handle.setAttribute("role", "separator");
    handle.setAttribute("aria-orientation", "vertical");
    handle.setAttribute("tabindex", "0");
    handle.setAttribute("aria-valuemin", String(DOCK_WIDTH.min));
    handle.setAttribute("aria-valuemax", String(DOCK_WIDTH.max));
    handle.setAttribute("aria-valuenow", String(widths[side]));
    if (panel.id) handle.setAttribute("aria-controls", panel.id);
    // The label is set when the panel is first shown (chrome built at boot must
    // not call `t()` before the catalogue lands; SKILL §11), from the same
    // strings the add-in panel's splitter uses.
    let drag = null;
    handle.addEventListener("pointerdown", (event) => {
      if (event.button !== 0) return;
      event.preventDefault();
      drag = { id: event.pointerId, x: event.clientX, width: widths[side] };
      handle.setPointerCapture?.(event.pointerId);
      handle.classList.add("dragging");
      handle.focus({ preventScroll: true });
    });
    handle.addEventListener("pointermove", (event) => {
      if (!drag || event.pointerId !== drag.id) return;
      const travel = (event.clientX - drag.x) / remPx();
      const outwardIsLeft = (side === "right") !== rtl();
      setWidth(side, drag.width + (outwardIsLeft ? -travel : travel), false);
    });
    const end = (event) => {
      if (!drag || event.pointerId !== drag.id) return;
      drag = null;
      handle.releasePointerCapture?.(event.pointerId);
      handle.classList.remove("dragging");
      saveWidth(side, widths[side], view);
    };
    handle.addEventListener("pointerup", end);
    handle.addEventListener("pointercancel", end);
    handle.addEventListener("dblclick", () => {
      setWidth(side, DOCK_WIDTH.default, false);
      saveWidth(side, null, view);
    });
    handle.addEventListener("keydown", (event) => {
      const next = dockWidthForKey(event.key, { width: widths[side], side, rtl: rtl(), shift: event.shiftKey });
      if (next === null) return;
      event.preventDefault();
      setWidth(side, next);
    });
    panel.prepend(handle);
  }
  const labelHandles = () => {
    const label = t("pluginPanel.resize.label");
    const title = t("pluginPanel.resize.title");
    for (const handle of doc.querySelectorAll(".dock-resize")) {
      const panel = handle.parentElement;
      if (!panel || panel.hidden) continue;
      if (label && handle.getAttribute("aria-label") !== label) handle.setAttribute("aria-label", label);
      if (title && handle.title !== title) handle.title = title;
    }
  };
  const handleObserver = new view.MutationObserver(() => {
    for (const panel of panels()) attachHandle(panel);
    labelHandles();
  });
  handleObserver.observe(workarea, { childList: true, subtree: true, attributes: true, attributeFilter: ["hidden"] });
  for (const panel of panels()) attachHandle(panel);
  reconcile();

  return {
    panels,
    visible,
    close,
    width: (side) => widths[side],
    setWidth,
  };
}

/** Whether the caret was in a table at the last toolbar pass. */
let followInTable = false;

/**
 * The Properties panel's tab follows the selection (docs/184 D5, audit
 * DIALOGS-07; ONLYOFFICE's RightMenu): the caret entering a table moves an open
 * panel to Table, leaving it moves it back to Paragraph — without taking the
 * caret out of the document. Only on that change, so a person who picks the
 * Paragraph tab inside a table keeps it. Called from the toolbar pass; O(1).
 */
export function followPropertiesTab(ed, inTable, hasSelection) {
  if (inTable === followInTable) return;
  followInTable = inTable;
  if (inTable && !ed.paragraphPropertiesPanel.hidden) ed.toggleTableProperties(true, { follow: true });
  else if (!inTable && !ed.tablePropertiesPanel.hidden && hasSelection) ed.toggleParagraphProperties(true, null, { follow: true });
}
