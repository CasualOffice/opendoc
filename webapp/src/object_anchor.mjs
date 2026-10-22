// One anchored-popover mechanism for everything that floats beside a thing in
// the document (`docs/172`).
//
// The pattern is the ANCHORED POPOVER (Floating UI / Popper's "anchor + flip +
// shift"): a surface is placed against an anchor rectangle on a preferred side,
// FLIPS to the other side when the preferred one has no room, SHIFTS along the
// cross axis to stay inside the view, and HIDES when its anchor is out of view.
// Each surface used to carry its own copy of that arithmetic: the object bar
// placed itself above the object and pinned to the view top when the object
// scrolled up (so it sat over the paragraph above, covering the text the reader
// was looking at), the link chip placed itself at the POINTER and never moved
// again on a scroll, and the block grip has no placement at all yet. Three
// copies drift three ways, so the rule lives here once.
//
// Two decisions are written down here rather than left to each caller:
//
//   * A surface never overlaps the GRIPS of what it acts on. The caller states
//     how far its grips reach beyond the anchor on each side (`clearance`), and
//     placement keeps that band free — a bar inside a grip's target means
//     `elementFromPoint` answers with the bar and the grip cannot be grabbed.
//   * A surface that fits on neither side is placed INSIDE the view against the
//     edge nearer its preferred side, over the anchor, rather than off screen: a
//     control the user cannot reach is worse than one that covers part of the
//     object, and a large object (a full-page picture) leaves no other choice.
//     Word's Layout Options button and Docs' image toolbar both do the same.
//
// Everything here is O(1) per placement. `placeAnchored` is pure, so the whole
// geometry is a node test (`tests/object_anchor.test.mjs`).

/** The gap between a surface and its anchor (after any grip clearance), and
 *  between a surface and the edge of the view. */
export const ANCHOR_GAP = 8;

/**
 * Where a surface of `size` goes against `anchor`, inside `view`.
 *
 * @param {{left:number, top:number, right:number, bottom:number}} anchor viewport px
 * @param {{width:number, height:number}} size the surface's measured size
 * @param {{left:number, top:number, right:number, bottom:number}} view the visible region
 * @param {object} [options]
 * @param {"below"|"above"} [options.prefer] the side tried first
 * @param {{above?:number, below?:number}} [options.clearance] how far the anchor's
 *   own grips reach beyond it on each side, in px
 * @param {"center"|"start"} [options.align] cross-axis alignment
 * @param {"ltr"|"rtl"} [options.dir] reading direction, for `start` alignment
 * @param {number} [options.gap]
 * @returns {{left:number, top:number, side:"below"|"above"|"inside"} | null}
 *   `null` when the anchor is not in view, and the surface must hide.
 */
export function placeAnchored(anchor, size, view, options = {}) {
  const {
    prefer = "below",
    clearance = {},
    align = "center",
    dir = "ltr",
    gap = ANCHOR_GAP,
  } = options;
  // Out of view on either axis: hide. A surface parked over unrelated text,
  // aimed at an object the reader can no longer see, is `docs/104` HF-058.
  if (
    anchor.bottom <= view.top ||
    anchor.top >= view.bottom ||
    anchor.right <= view.left ||
    anchor.left >= view.right
  ) {
    return null;
  }
  const above = Math.max(gap, (clearance.above ?? 0) + gap);
  const below = Math.max(gap, (clearance.below ?? 0) + gap);
  const fits = {
    below: anchor.bottom + below + size.height <= view.bottom - gap,
    above: anchor.top - above - size.height >= view.top + gap,
  };
  const topFor = {
    below: anchor.bottom + below,
    above: anchor.top - above - size.height,
  };
  const other = prefer === "below" ? "above" : "below";
  let side;
  let top;
  if (fits[prefer]) {
    side = prefer;
    top = topFor[prefer];
  } else if (fits[other]) {
    side = other;
    top = topFor[other];
  } else {
    // Inside the view, against the edge nearer the preferred side.
    side = "inside";
    top = prefer === "below" ? view.bottom - gap - size.height : view.top + gap;
    top = Math.max(view.top + gap, top);
  }

  const minLeft = view.left + gap;
  const maxLeft = Math.max(minLeft, view.right - gap - size.width);
  let left;
  if (align === "start") {
    left = dir === "rtl" ? anchor.right - size.width : anchor.left;
  } else {
    left = (anchor.left + anchor.right) / 2 - size.width / 2;
  }
  left = Math.min(Math.max(left, minLeft), maxLeft);
  return { left: Math.round(left), top: Math.round(top), side };
}

/**
 * Measures `el`, places it with `placeAnchored`, and writes the result.
 *
 * The element must be `position: fixed`. It is shown before measuring (a hidden
 * element has no size) and hidden again when its anchor is out of view. The
 * side it landed on is reflected as `data-side`, so a stylesheet can point a
 * caret at the anchor and a test can assert where it went.
 *
 * O(1): one layout read, two style writes.
 *
 * @returns {boolean} whether the surface is showing
 */
export function applyAnchoredPlacement(el, anchor, view, options) {
  el.hidden = false;
  const at = anchor ? placeAnchored(anchor, { width: el.offsetWidth, height: el.offsetHeight }, view, options) : null;
  if (!at) {
    el.hidden = true;
    delete el.dataset.side;
    return false;
  }
  el.style.left = `${at.left}px`;
  el.style.top = `${at.top}px`;
  el.dataset.side = at.side;
  return true;
}

/**
 * Hides an anchored surface while a pointer gesture is in flight on `scope`,
 * and gives it back when the gesture ends.
 *
 * Every product hides its object toolbar while the object is being dragged:
 * the bar would otherwise sit wherever the object USED to be, over the text
 * the user is dropping into, and a release over it would land on a button.
 * Done with a class (`is-suspended`) rather than `hidden`, so the surface's own
 * show/hide decision is not overwritten and survives the gesture.
 *
 * Listens in the capture phase so it sees the press before any handler that
 * stops propagation. A press INSIDE the surface is not a gesture on the scope.
 *
 * @param {HTMLElement} surface
 * @param {() => Element|null} scope the element whose presses count (the pages)
 * @param {() => void} onResume called after the gesture, to re-place the surface
 * @returns {() => void} removes the listeners
 */
export function suspendDuringPointerGesture(surface, scope, onResume) {
  let active = false;
  const down = (event) => {
    const root = scope();
    if (!root || !(event.target instanceof Node) || !root.contains(event.target)) return;
    if (surface.contains(event.target)) return;
    active = true;
    surface.classList.add("is-suspended");
  };
  const up = () => {
    if (!active) return;
    active = false;
    surface.classList.remove("is-suspended");
    onResume();
  };
  window.addEventListener("pointerdown", down, true);
  window.addEventListener("pointerup", up, true);
  window.addEventListener("pointercancel", up, true);
  return () => {
    window.removeEventListener("pointerdown", down, true);
    window.removeEventListener("pointerup", up, true);
    window.removeEventListener("pointercancel", up, true);
  };
}

/**
 * Makes `el` a WAI-ARIA toolbar with one tab stop: the arrow keys move between
 * its enabled controls, Home and End go to the ends, and Escape hands focus
 * back through `onEscape`.
 *
 * The roving tab stop is re-derived on every key, never cached, because the
 * caller rebuilds the controls on each repaint. Arrows follow the reading
 * direction: in a right-to-left document ArrowLeft moves forward.
 *
 * O(controls) per key.
 */
export function bindToolbarKeys(el, { onEscape, dir = () => "ltr" } = {}) {
  el.setAttribute("role", "toolbar");
  el.setAttribute("aria-orientation", "horizontal");
  el.addEventListener("keydown", (event) => {
    const controls = toolbarControls(el);
    if (controls.length === 0) return;
    const at = controls.indexOf(document.activeElement);
    const forward = dir() === "rtl" ? "ArrowLeft" : "ArrowRight";
    const back = dir() === "rtl" ? "ArrowRight" : "ArrowLeft";
    let next = -1;
    if (event.key === forward) next = (at + 1) % controls.length;
    else if (event.key === back) next = (at - 1 + controls.length) % controls.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = controls.length - 1;
    else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onEscape?.();
      return;
    } else return;
    event.preventDefault();
    focusToolbarControl(el, controls[next]);
  });
}

/** The toolbar's reachable controls, in DOM order. A disabled control is still
 *  reachable — WAI-ARIA's toolbar pattern keeps it focusable so its reason can
 *  be read — which is why the wrap chips use `aria-disabled` and not only the
 *  `disabled` property when they explain themselves. */
export function toolbarControls(el) {
  return [...el.querySelectorAll("button, [role=button]")].filter(
    (control) => !control.hidden && control.getClientRects().length > 0,
  );
}

/** Moves the roving tab stop to `control` and focuses it. */
export function focusToolbarControl(el, control) {
  if (!control) return;
  for (const other of toolbarControls(el)) other.tabIndex = other === control ? 0 : -1;
  control.focus();
}
