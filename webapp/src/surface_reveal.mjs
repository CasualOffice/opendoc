// Revealing a control that lives on a surface the reader is not looking at.
//
// ADR-062 corollary C1 — *a pointer must point*. A control on one surface that
// defers to a control on another has to open or select the owning surface,
// bring the control into view, and put focus on it. Opening the container is
// not revealing the control, and `docs/159` §3 is what happens when only the
// first of the three is done: the Measurement-unit row opened Settings and left
// the reader looking at the Theme radio group, with the measurement parameter
// below the fold.
//
// ---- WHY A FRAME AND NOT A SYNCHRONOUS CALL -------------------------------
//
// This is the whole reason the module exists, and it is an ORDERING GUARANTEE
// rather than a race won by being later. `modal.mjs` defers its own initial
// focus with `queueMicrotask`, so a caller that focuses synchronously is
// overwritten a microtask later by the dialog's `initialFocus`. Microtasks
// always drain before the next animation frame, so a callback scheduled on the
// frame ALWAYS lands after the modal has finished its own focus management —
// for any number of queued microtasks, not just one.
//
// The code this replaced focused synchronously and therefore never had an
// observable effect at all. That is the strongest evidence available that the
// path was never watched working, and it is why the guard for this asserts the
// *guarantee* (the control ends up focused) rather than the mechanism.
//
// ---- WHY IT SCROLLS, AND WHY THE SECTION AND NOT THE CONTROL --------------
//
// `modal.mjs` focuses with `preventScroll: true`, so focus alone moves nothing:
// in a scrolling dialog body the control can be focused and still off screen.
// And the target is the enclosing labelled section rather than the control, so
// the reader sees the control's LABEL too — a `<select>` scrolled to the top
// edge with its caption cut off answers "where is it" with "here, unlabelled".
//
// The frame scheduler is injected so the ordering guarantee above is testable
// in node, against a stand-in element, without a browser: that is the one claim
// here that a DOM-free test can actually check.

/**
 * Brings `el` into view and focuses it, after the owning surface has finished
 * its own focus management.
 *
 * @param {{closest: Function, focus: Function} | null | undefined} el the
 *   control to reveal. Tolerates null so a caller need not guard a lookup.
 * @param {object} [options]
 * @param {string} [options.group] selector for the enclosing labelled group to
 *   scroll into view instead of `el` itself; falls back to `el` when it matches
 *   nothing.
 * @param {(callback: () => void) => unknown} [options.frame] the scheduler.
 *   Defaults to `requestAnimationFrame`; injected by tests.
 * @returns {void}
 */
export function revealControl(el, options = {}) {
  const schedule = options.frame ?? ((callback) => requestAnimationFrame(callback));
  schedule(() => {
    if (!el) return;
    const group = options.group ? el.closest(options.group) : null;
    (group ?? el).scrollIntoView?.({ block: "nearest" });
    el.focus?.({ preventScroll: true });
  });
}
