// The View band's Zoom group.
//
// The band offered only "−" and "+", so the three zoom operations people
// actually reach for — Fit width, Fit page, and back to 100% — existed only in
// the status-bar menu and the command palette. ONLYOFFICE gives each of them a
// button on the View tab (`ViewTab.js:52-125`: `slot-btn-ftw`, `slot-btn-ftp`,
// `slot-btn-zoom-100`), and Word the same; stepping by 10% to reach "fit the
// page" is not a substitute for asking for it.
//
// It lives in a module rather than in `main.js` because `main.js` is at its line
// ratchet with no slack (`module_seams`), and wiring five buttons there is what
// the ratchet exists to refuse. Moving the two existing steppers in here as well
// makes the whole group answer to one table instead of two hand-written
// listeners, and leaves `main.js` shorter than it was.
//
// The actions are declared in the MARKUP (`data-zoom-action`), so a button added
// to the group without a matching action is inert rather than silently wired to
// the wrong one — `view_zoom.test.mjs` fails on an action this table does not
// know, in either direction.

/** The zoom actions the band can name, and what each one asks the editor for.
 *  Keys are the `data-zoom-action` values in `editor.html`. */
export const ZOOM_ACTIONS = Object.freeze([
  "out",
  "in",
  "fit-width",
  "fit-page",
  "actual",
]);

/** True when `action` is the operation currently in effect, so the button can
 *  say so. The steppers are verbs rather than states and are never pressed;
 *  "actual" is a state, but only while the zoom is a plain 100% — a fit mode
 *  that happens to land on 100% is Fit width, not Actual size, and saying
 *  otherwise would make two buttons claim the same state. */
export function zoomActionActive(action, { mode, factor }) {
  if (action === "fit-width" || action === "fit-page") return mode === action;
  if (action === "actual") return mode === "custom" && Math.abs(factor - 1) < 1e-6;
  return false;
}

/** The zoom a freshly-opened document should be shown at.
 *
 *  An editor mounted in a host's column — the embedding playground's ~750px
 *  frame, a sidebar, a phone — is narrower than a page plus its margins, so at
 *  100% the page is CLIPPED: the playground showed a title cut to "OpenDoc
 *  Feature Test Docur…" and a status bar reading "11,516 character", with the
 *  frame scrolling sideways. Word for the web fits the page to its container
 *  rather than shipping a horizontal scrollbar, and a host that configures
 *  nothing still has to land somewhere readable. `fitFactor` is what Fit width
 *  WOULD pick; anything below 1 means the page does not fit at full size.
 *
 *  It only ever changes a zoom the user has not chosen — anything but a plain
 *  100% is returned untouched — which is what stops an automatic fit from
 *  overriding a deliberate zoom on a later open or relayout.
 *
 *  The 0.5% tolerance keeps a page that fits to within a subpixel at 100%
 *  rather than flipping it into a fit mode nobody asked for.
 *
 *  Complexity: O(1).
 */
export function openingZoomMode(fitFactor, current) {
  const untouched = current.mode === "custom" && Math.abs(current.factor - 1) < 1e-6;
  if (!untouched) return current.mode;
  return fitFactor < 0.995 ? "fit-width" : "custom";
}

/** Wires the View band's zoom controls.
 *
 *  `zoomState()` is read at reflect time rather than captured, because the zoom
 *  can change from the status bar, the palette, a fit-mode recompute on resize
 *  or a keyboard shortcut — this group is a view of that state, never its owner.
 *
 *  Complexity: O(buttons in the group) per reflect, which is O(1) in document
 *  size — it touches five elements and reads no document state.
 */
export function createViewZoom({ stepZoom, setZoom, setZoomMode, zoomState, root = document }) {
  const run = {
    out: () => stepZoom(-1),
    in: () => stepZoom(1),
    "fit-width": () => setZoomMode("fit-width"),
    "fit-page": () => setZoomMode("fit-page"),
    actual: () => setZoom(1),
  };
  const buttons = [...root.querySelectorAll("#panelView [data-zoom-action]")];
  for (const button of buttons) {
    const action = button.dataset.zoomAction;
    // An unknown action is a markup bug, not a no-op to paper over: leaving the
    // button live but inert is exactly the "dead control" the UI floor forbids.
    if (!run[action]) {
      button.disabled = true;
      continue;
    }
    button.addEventListener("click", () => run[action]());
  }

  return {
    setEnabled(on) {
      for (const button of buttons) button.disabled = !on || !run[button.dataset.zoomAction];
    },
    reflect() {
      const state = zoomState();
      for (const button of buttons) {
        const action = button.dataset.zoomAction;
        if (action === "out" || action === "in") continue;
        button.setAttribute("aria-pressed", String(zoomActionActive(action, state)));
      }
    },
  };
}
