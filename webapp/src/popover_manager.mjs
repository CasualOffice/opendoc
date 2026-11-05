// THE ANCHORED-POPOVER MANAGER: one implementation of "a small menu hangs off
// a button until you point somewhere else".
//
// It was ninety lines in the middle of `main.js`, which meant every surface
// that wanted this behaviour had to be built in `main.js` too — and the ones
// built in modules got it by having the manager handed down to them a function
// at a time. That is the HF-085 seam problem in miniature: the rule was
// reachable only from the file that happened to hold it, and the footer's
// language picker is the surface that made that cost visible.
//
// What the manager guarantees, and what no call site should re-implement: one
// popover open at a time, anchored under its trigger and flipped above it when
// the viewport is short, dismissed on an outside `pointerdown` and on Escape,
// with focus returned to the trigger it came from. `registerPopover` takes a
// `reflect()` that syncs the menu's controls on every open, so a menu is never
// painted from stale state.
import { popoverAnchor, popoverPosition } from "./popover_position.mjs";

const popovers = [];

/** Whether there is anything for a document-scoped popover to act on. The
 *  toolbar menus act on the caret, so there is no honest thing for them to do
 *  without one; CHROME popovers opt out per-popover with `needsDocument:
 *  false`. Replaced once, at boot, by whoever owns the selection. */
let documentReady = () => true;
/** Where focus goes when a popover closes holding it and neither its invoker nor
 *  its trigger can take it back: the document (installed at boot). */
let focusFallback = null;

/** Installs the host's "is there something to act on" test and focus fallback. */
export function configurePopovers(hooks) {
  documentReady = hooks?.documentReady ?? documentReady;
  focusFallback = hooks?.focusFallback ?? focusFallback;
}

/** Whether `el` can take focus back: attached, enabled, rendered, not `<body>`. */
function canTakeFocus(el) {
  return !!el && el.isConnected && el !== document.body && !el.disabled && el.getClientRects().length > 0;
}

/** Re-syncs every popover that is currently on screen. The caret can move while
 *  one is open — arrow keys, a click through the menu's own gap — and a menu
 *  showing the previous paragraph's spacing is a menu that lies. */
export function reflectOpenPopovers() {
  for (const p of popovers) if (!p.menu.hidden) p.reflect();
}

/** Closes all of them, for a surface that takes the screen over (a task pane
 *  opening beside the document). Through `closePopover`, so focus and
 *  `aria-expanded` are handled rather than left behind. */
export function closeAllPopovers() {
  for (const p of popovers) closePopover(p);
}

/** Activates a chrome button WITHOUT letting the press take the document's
 *  selection: the `mousedown` default is what blurs the editing surface, and a
 *  toolbar that steals the caret cannot act on it. */
export function onButton(el, handler) {
  el.addEventListener("mousedown", (e) => e.preventDefault());
  el.addEventListener("click", (e) => {
    e.preventDefault();
    handler(e);
  });
}

export function openPopover(p, { keyboard = false, anchor = p.btn } = {}) {
  // An object can be selected with no text caret behind it (clicking a float
  // first thing), and its Fill/Outline pickers must still open. A CHROME
  // popover skips the gate entirely: the language picker is not about the
  // document, and going dead while the engine loads would make it a dead
  // control at exactly the moment someone needs out of a language they cannot
  // read.
  if (p.needsDocument && !documentReady()) return;
  for (const q of popovers) if (q !== p) closePopover(q);
  // Measure the control the user actually activated, never a hidden popover owner.
  const visibleAnchor = popoverAnchor(anchor, p.btn);
  const r = visibleAnchor.getBoundingClientRect();
  // Who had focus when it opened: the trigger when opened from the keyboard,
  // the document when opened by pointer (the trigger's mousedown keeps focus
  // there) or from the palette (which hands focus back before it runs).
  const before = document.activeElement;
  p.invoker = before && before !== document.body && !p.menu.contains(before) ? before : null;
  p.menu.hidden = false;
  p.activeTrigger = visibleAnchor;
  p.btn.setAttribute("aria-expanded", String(visibleAnchor === p.btn));
  visibleAnchor.setAttribute("aria-expanded", "true");
  p.reflect();
  const at = popoverPosition(
    r,
    { width: p.menu.offsetWidth, height: p.menu.offsetHeight },
    { width: window.innerWidth, height: window.innerHeight },
  );
  p.menu.style.left = `${at.left}px`;
  p.menu.style.top = `${at.top}px`;
  // Opened from the keyboard, the popover takes focus (docs/104 HF-070: it used
  // to leave focus on the trigger, so reaching a swatch meant tabbing through
  // the rest of the ribbon first). Opened by pointer it deliberately does NOT,
  // because these menus preserve the document selection on mouse interaction —
  // that is what the mousedown preventDefault in registerPopover is for.
  if (keyboard) focusFirstIn(p.menu);
}

// A control that reports itself as the current choice. Focus belongs on it
// rather than on the first row: opening the spacing menu should land on the
// spacing this paragraph already has, the way a native menu opens on its checked
// item, so the arrow keys start from where the user is (docs/104 HF-070).
const CHECKED_SELECTOR = '[aria-checked="true"], [aria-pressed="true"], [aria-selected="true"]';

/** Focuses the checked control inside `container`, or the first visible, enabled
 *  one when nothing is checked. */
export function focusFirstIn(container) {
  const focusable = [
    ...container.querySelectorAll(
      "a[href], button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex='-1'])",
    ),
  ].filter((element) => element.getClientRects().length > 0);
  const target = focusable.find((element) => element.matches(CHECKED_SELECTOR)) ?? focusable[0];
  target?.focus({ preventScroll: true });
  return target ?? null;
}

export function closePopover(p) {
  // Closing must not strand the keyboard (audit DIALOGS-05): if focus is inside
  // the menu it goes back to where the person was when it opened — the trigger
  // for a keyboard open, the document for a pointer or palette open. When that
  // is gone, the trigger; when the trigger is not on screen either (a gallery
  // opened from the palette while its ribbon tab is not showing), the document,
  // never `<body>`.
  const holdsFocus = p.menu.contains(document.activeElement);
  const trigger = p.activeTrigger ?? p.btn;
  const invoker = p.invoker;
  p.menu.hidden = true;
  p.btn.setAttribute("aria-expanded", "false");
  if (trigger !== p.btn) trigger.setAttribute("aria-expanded", "false");
  if (holdsFocus) {
    if (canTakeFocus(invoker)) invoker.focus({ preventScroll: true });
    else if (canTakeFocus(trigger)) trigger.focus({ preventScroll: true });
    else if (focusFallback) focusFallback();
    else trigger.focus({ preventScroll: true });
  }
  p.activeTrigger = null;
  p.invoker = null;
}

export function registerPopover(btn, menu, reflect, { needsDocument = true } = {}) {
  const p = { btn, menu, reflect, activeTrigger: null, invoker: null, needsDocument };
  // A handle, so a menu that acts on a click can dismiss itself through the
  // manager rather than hiding the element behind its back and stranding
  // `aria-expanded` and the keyboard's place.
  p.close = () => closePopover(p);
  popovers.push(p);
  onButton(btn, (event) =>
    menu.hidden ? openPopover(p, { keyboard: event?.detail === 0 }) : closePopover(p),
  );
  // Keep clicks inside the menu from stealing the selection focus, but let form
  // controls (inputs, selects) focus, toggle, and open normally.
  menu.addEventListener("mousedown", (e) => {
    if (!["INPUT", "SELECT", "OPTION"].includes(e.target.tagName)) e.preventDefault();
  });
  return p;
}

// `pointerdown` — the phase every other dismissable surface uses (#556). The
// last holdout on `mousedown`, which a pen or a consumed touch never produces.
document.addEventListener("pointerdown", (e) => {
  for (const p of popovers) {
    if (
      !p.menu.hidden &&
      !p.menu.contains(e.target) &&
      e.target !== p.btn &&
      !p.btn.contains(e.target) &&
      e.target !== p.activeTrigger &&
      !p.activeTrigger?.contains(e.target)
    ) {
      closePopover(p);
    }
  }
});
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") for (const p of popovers) if (!p.menu.hidden) closePopover(p);
});
