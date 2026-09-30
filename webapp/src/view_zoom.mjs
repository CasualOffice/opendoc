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
 *  And there is a FLOOR, because fitting is only an improvement while the result
 *  can be read. A phone viewport fits a Letter page at about 31%, and the first
 *  version of this shipped exactly that: a postage stamp of a document where the
 *  defect had at least left readable text to pan across. Below the floor the
 *  page stays at full size and the reader scrolls — which is what Word for the
 *  web does too: it fits the window, and its zoom does not go under 50%.
 *
 *  Complexity: O(1).
 */
export const FIT_ON_OPEN_FLOOR = 0.5;

export function openingZoomMode(fitFactor, current) {
  const untouched = current.mode === "custom" && Math.abs(current.factor - 1) < 1e-6;
  if (!untouched) return current.mode;
  return fitFactor < 0.995 && fitFactor >= FIT_ON_OPEN_FLOOR ? "fit-width" : "custom";
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

/** The ladder the +/- steppers walk. */
export const ZOOM_STEPS = Object.freeze([0.5, 0.75, 0.9, 1, 1.25, 1.5, 2, 3]);

/** The step either side of `current` on that ladder, or a tenth beyond its ends.
 *
 *  Extracted from `main.js` by the reflow round (`docs/151` §6), which needed
 *  lines in a file at its ratchet: a table and a search need no browser, and
 *  `view_zoom.test.mjs` can now ask what the steppers do at the ends of it.
 *  `clamp` is passed in because `ZOOM_MIN`/`ZOOM_MAX` belong to the shell rather
 *  than to this band.
 *
 *  Complexity: O(steps) — eight comparisons, independent of document size.
 *
 * @param {number} current
 * @param {number} direction above zero steps up, otherwise down
 * @param {(z:number)=>number} clamp
 */
export function nextZoomStep(current, direction, clamp) {
  return direction > 0
    ? ZOOM_STEPS.find((s) => s > current + 1e-6) ?? clamp(current + 0.1)
    : [...ZOOM_STEPS].reverse().find((s) => s < current - 1e-6) ?? clamp(current - 0.1);
}

/** The fit-to-viewport factor for one page box, before clamping.
 *
 *  The gutters (64px across, 48px down) are the breathing room the desk keeps
 *  around the sheet; a fit that ran the page edge to edge would read as a page
 *  that does not fit. The 120px floor keeps a viewport that has not been laid
 *  out yet — or a pane dragged to nothing — from producing a zero or negative
 *  factor that the clamp would then present as a deliberate choice.
 *
 *  Complexity: O(1).
 *
 * @param {"fit-width"|"fit-page"} mode
 * @param {{widthTwip:number, heightTwip:number}} page the page box in twips
 * @param {{width:number, height:number}} viewport the scroller's rect in CSS px
 * @param {{dpi:number, twipsPerInch:number}} units
 */
export function fitZoomFactor(mode, page, viewport, { dpi, twipsPerInch }) {
  const fitW = Math.max(120, viewport.width - 64) / ((page.widthTwip / twipsPerInch) * dpi);
  if (mode !== "fit-page") return fitW;
  const fitH = Math.max(120, viewport.height - 48) / ((page.heightTwip / twipsPerInch) * dpi);
  return Math.min(fitW, fitH);
}

/** What a value typed into the status bar's zoom field asks for: a fit `mode`, a
 *  `factor`, or neither (which the caller answers by restoring the last valid
 *  display rather than by guessing).
 *
 *  Extracted with `nextZoomStep` and for the same reason. The tokens are matched
 *  in English only, which is a known gap rather than an oversight: the field
 *  DISPLAYS "Fit width" from the catalogue, so a reader in another locale can
 *  read what they typed back but cannot type it. `docs/148` §9 has the row.
 *
 *  Complexity: O(1).
 *
 * @param {string} raw
 * @returns {{mode?: "fit-width"|"fit-page", factor?: number}}
 */
export function parseZoomInput(raw) {
  const text = String(raw ?? "").trim().toLowerCase();
  if (text.startsWith("fit w") || text === "width") return { mode: "fit-width" };
  if (text.startsWith("fit p") || text === "page") return { mode: "fit-page" };
  const percent = parseFloat(text.replace("%", ""));
  return Number.isFinite(percent) && percent > 0 ? { factor: percent / 100 } : {};
}

/** Ticks the status-bar zoom menu's presets and fit rows against the live state.
 *
 *  Came out of `main.js`'s `updateZoomDisplay` in the same round and for the same
 *  reason. Two `querySelectorAll` loops over one popover: O(presets), and it
 *  reads nothing about the document.
 *
 * @param {Element} menu the `#zoomMenu` popover
 * @param {{mode:string, factor:number}} state
 */
export function reflectZoomMenu(menu, { mode, factor }) {
  for (const b of menu.querySelectorAll(".zoom-preset")) {
    const on = mode === "custom" && Math.abs(Number(b.dataset.zoom) - factor) < 1e-6;
    b.setAttribute("aria-checked", String(on));
  }
  for (const b of menu.querySelectorAll(".zoom-fit")) {
    b.setAttribute("aria-checked", String(mode === b.dataset.zoomMode));
  }
}
