// SPDX-License-Identifier: Apache-2.0
// The add-in side panel's width: docked on the right at full height and
// resizable by a handle on its inner edge (`docs/183` §21).
//
// DESIGNED FROM THE PRODUCTS PEOPLE KNOW. Gemini in Google Docs and Copilot in
// Word both dock their assistant on the right, from below the toolbar to the
// bottom of the window, and the document column shrinks to make room rather than
// sliding under it. Word's task panes take a drag on their inner edge; VS Code's
// side bar resets on a double-click of its sash. The handle here is the WAI-ARIA
// window splitter: `role="separator"`, focusable, `aria-valuenow`/`-min`/`-max`,
// arrow keys to move it, Home/End for the two ends.
//
// The width is the person's, on this device (one key, every add-in panel), and is
// re-clamped whenever the window changes: at most half the window, at least the
// minimum, so a width chosen on a large monitor never swallows a laptop's page.
//
// Complexity: O(1) per pointer move or key press; nothing here reads the document.

/** The bounds, in CSS pixels. `maxFraction` is of the window's width. */
export const PANEL_WIDTH = Object.freeze({ min: 320, default: 400, maxFraction: 0.5, step: 16, bigStep: 64 });

/** Where the width is remembered on this device. */
export const PANEL_WIDTH_KEY = "opendoc.pluginPanel.width";

/** The widest the panel may be in a window `viewportWidth` wide: half of it, but
 *  never below the minimum (a narrow window keeps a usable panel). Pure. */
export function maxPanelWidth(viewportWidth) {
  const half = Math.floor((Number(viewportWidth) || 0) * PANEL_WIDTH.maxFraction);
  return Math.max(PANEL_WIDTH.min, half);
}

/** `width` held between the minimum and half the window, as a whole pixel.
 *  Anything that is not a finite number is the default. Pure. */
export function clampPanelWidth(width, viewportWidth) {
  const value = Number.isFinite(Number(width)) && width !== null && width !== "" ? Math.round(Number(width)) : PANEL_WIDTH.default;
  return Math.min(maxPanelWidth(viewportWidth), Math.max(PANEL_WIDTH.min, value));
}

/** The width a key press on the handle asks for, or null when the key is not
 *  one the splitter answers. The handle is on the panel's INNER edge, so the
 *  arrow pointing away from the panel widens it: ← in a left-to-right page,
 *  → in a right-to-left one. Pure. */
export function widthForKey(key, { width, viewportWidth, rtl = false, shift = false }) {
  const step = shift ? PANEL_WIDTH.bigStep : PANEL_WIDTH.step;
  const wider = rtl ? "ArrowRight" : "ArrowLeft";
  const narrower = rtl ? "ArrowLeft" : "ArrowRight";
  if (key === wider) return clampPanelWidth(width + step, viewportWidth);
  if (key === narrower) return clampPanelWidth(width - step, viewportWidth);
  if (key === "Home") return PANEL_WIDTH.min;
  if (key === "End") return maxPanelWidth(viewportWidth);
  return null;
}

/** The remembered width, or null. Storage can be refused (a private window, a
 *  sandboxed host): then the panel simply starts at its default. */
export function loadPanelWidth(view = globalThis) {
  try {
    const raw = view.localStorage?.getItem(PANEL_WIDTH_KEY);
    const value = raw === null || raw === undefined ? NaN : Number(raw);
    return Number.isFinite(value) ? value : null;
  } catch {
    return null;
  }
}

export function savePanelWidth(width, view = globalThis) {
  try {
    if (width === null) view.localStorage?.removeItem(PANEL_WIDTH_KEY);
    else view.localStorage?.setItem(PANEL_WIDTH_KEY, String(Math.round(width)));
  } catch {
    /* storage refused: the width lasts as long as the page */
  }
}

/**
 * Wires the splitter. `apply(width)` is called with every new width (the
 * platform sets the panel's width and tells the page overlays to re-anchor);
 * `rtl()` says which way the page runs.
 *
 * @param {object} io
 * @param {HTMLElement} io.handle the `role="separator"` element
 * @param {HTMLElement} io.panel the panel it resizes
 * @param {(width: number) => void} io.apply
 * @param {() => boolean} io.rtl
 * @param {(width: number | null) => number} [io.initial] the starting width
 * @param {Window} [io.view]
 * @returns {{width: () => number, set: (width: number, remember?: boolean) => void, reclamp: () => void}}
 */
export function attachPanelResize({ handle, panel, apply, rtl, view = globalThis }) {
  let width = clampPanelWidth(loadPanelWidth(view) ?? PANEL_WIDTH.default, view.innerWidth);
  const reflect = () => {
    handle.setAttribute("aria-valuemin", String(PANEL_WIDTH.min));
    handle.setAttribute("aria-valuemax", String(maxPanelWidth(view.innerWidth)));
    handle.setAttribute("aria-valuenow", String(width));
  };
  const set = (next, remember = true) => {
    width = clampPanelWidth(next, view.innerWidth);
    reflect();
    apply(width);
    if (remember) savePanelWidth(width, view);
  };

  let drag = null;
  handle.addEventListener("pointerdown", (event) => {
    if (event.button !== 0) return;
    event.preventDefault();
    // The width follows the pointer's travel from where it was grabbed, so the
    // edge never jumps to the pointer on the first move.
    drag = { id: event.pointerId, x: event.clientX, width };
    handle.setPointerCapture?.(event.pointerId);
    handle.classList.add("dragging");
    handle.focus({ preventScroll: true });
  });
  handle.addEventListener("pointermove", (event) => {
    if (!drag || event.pointerId !== drag.id) return;
    const travel = event.clientX - drag.x;
    set(drag.width + (rtl() ? travel : -travel), false);
  });
  const end = (event) => {
    if (!drag || event.pointerId !== drag.id) return;
    drag = null;
    handle.releasePointerCapture?.(event.pointerId);
    handle.classList.remove("dragging");
    savePanelWidth(width, view);
  };
  handle.addEventListener("pointerup", end);
  handle.addEventListener("pointercancel", end);
  handle.addEventListener("dblclick", () => {
    set(PANEL_WIDTH.default, false);
    savePanelWidth(null, view);
  });
  handle.addEventListener("keydown", (event) => {
    const next = widthForKey(event.key, { width, viewportWidth: view.innerWidth, rtl: rtl(), shift: event.shiftKey });
    if (next === null) return;
    event.preventDefault();
    set(next);
  });
  // A smaller window lowers the ceiling; the remembered width is kept, so the
  // panel grows back when the window does.
  view.addEventListener?.("resize", () => {
    const remembered = loadPanelWidth(view);
    set(remembered ?? width, false);
  });
  reflect();
  return {
    width: () => width,
    set,
    reclamp: () => set(loadPanelWidth(view) ?? width, false),
  };
}
