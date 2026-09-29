// The rotation GESTURE: pointer-down on the rotation grip, live preview, commit
// on release — plus the keyboard half, because a direct-manipulation handle that
// only a mouse can reach is half a control.
//
// The arithmetic is `object_rotate.mjs` and stays pure; this owns the session
// state and the one gated engine call. Same shape as the other object command
// modules: everything the editor supplies arrives through `io`, so nothing here
// reaches for a global.
//
// Cost: ONE `objectFrame` read at pointer-down, memoised on the session. A
// pointer move does arithmetic and writes two style properties — no engine call,
// no document walk (`docs/107` §4).
import {
  SHIFT_SNAP_DEGREES,
  normaliseDegrees,
  pointerAngle,
  snapRotation,
} from "./object_rotate.mjs";

/**
 * Builds the rotation gesture.
 *
 * @param {{
 *   doc: () => any,
 *   selection: () => any,
 *   scaleOf: (page: any) => {rect: DOMRect, sx: number, sy: number},
 *   runEdit: (thunk: () => any, options?: object) => any,
 *   setStatus: (text: string, kind?: string) => void,
 *   t: (key: string, vars?: object) => string,
 *   drawSelection: () => void,
 *   focusEditorSurface: () => void,
 *   reviewMode: () => string,
 *   blockMutationInViewing: () => void,
 * }} io
 */
export function createObjectRotateDrag(io) {
  /** The live gesture, or `null`. Holds the object's centre in CSS pixels and
   *  the angle the object had at pointer-down, both read ONCE. */
  let session = null;

  /** Whether rotating is offered at all right now, with the refusal SAID rather
   *  than the gesture silently doing nothing. */
  function rotatable() {
    const selection = io.selection();
    if (!io.doc() || !selection || selection.mode !== "selected" || !selection.canRotate) {
      return false;
    }
    if (io.reviewMode() === "viewing") {
      io.blockMutationInViewing();
      return false;
    }
    if (io.reviewMode() === "suggesting") {
      io.setStatus(io.t("object.rotate.notTracked"), "error");
      return false;
    }
    return true;
  }

  /** The object's frame and angle, as `[page, x, y, w, h, milliDegrees]`, or
   *  `null`. One engine call. */
  function frameOf(node) {
    const frame = io.doc().objectFrame?.(node) ?? [];
    return frame.length >= 6 ? frame : null;
  }

  /** Puts the live angle on the preview and on the grip's `aria-valuenow`, so
   *  the number a screen reader reads and the number the eye sees are the same
   *  one. */
  function reflect(degrees) {
    const rounded = Math.round(degrees);
    session.outline.style.transform = `rotate(${degrees}deg)`;
    session.grip?.setAttribute("aria-valuenow", String(rounded));
    session.readout.textContent = io.t("object.rotate.degrees", { degrees: rounded });
  }

  /** Begins a rotation drag from the grip. */
  function start(event, page, node) {
    if (!rotatable() || io.selection().node !== node) return;
    // As on a resize grip: suppress the compatibility mouse events so a
    // double-click that BEGINS on the handle opens no crop and descends into no
    // group, and stop the page's own hit-test from re-resolving the press.
    event.preventDefault();
    event.stopPropagation();
    const frame = frameOf(node);
    if (!frame) return;
    io.focusEditorSurface();
    const { sx, sy } = io.scaleOf(page);
    const [, x, y, w, h, milliDegrees] = frame;
    const outline = document.createElement("div");
    outline.className = "object-rotate-preview";
    outline.style.left = `${x * sx}px`;
    outline.style.top = `${y * sy}px`;
    outline.style.width = `${w * sx}px`;
    outline.style.height = `${h * sy}px`;
    const readout = document.createElement("span");
    readout.className = "object-rotate-readout";
    readout.setAttribute("aria-hidden", "true");
    outline.appendChild(readout);
    page.overlay.appendChild(outline);
    session = {
      node,
      page,
      outline,
      readout,
      grip: event.currentTarget instanceof Element ? event.currentTarget : null,
      // The object's centre in the page overlay's own pixel space, which is the
      // space the pointer is converted into below. Computed once: a rotation
      // drag fires on every pointer move and the centre cannot move during one.
      centre: { x: (x + w / 2) * sx, y: (y + h / 2) * sy },
      overlayRect: page.overlay.getBoundingClientRect(),
      startDegrees: milliDegrees / 1000,
      // The bearing of the grabbed point, so the object follows the pointer from
      // wherever the grip was grabbed instead of jumping to it.
      grabOffset: 0,
      degrees: milliDegrees / 1000,
    };
    session.grabOffset = normaliseDegrees(
      session.startDegrees - pointerAngle(session.centre, pointerIn(event)),
    );
    reflect(session.degrees);
    event.currentTarget?.setPointerCapture?.(event.pointerId);
  }

  /** The pointer in the overlay's pixel space. */
  function pointerIn(event) {
    return {
      x: event.clientX - session.overlayRect.left,
      y: event.clientY - session.overlayRect.top,
    };
  }

  /** Follows the pointer. Snapping is ONLYOFFICE's — the four quarter turns pull
   *  the angle in, then Shift constrains what is left to 15 degrees. */
  function update(event) {
    if (!session) return;
    const raw = pointerAngle(session.centre, pointerIn(event)) + session.grabOffset;
    session.degrees = snapRotation(raw, event.shiftKey === true);
    reflect(session.degrees);
    event.preventDefault();
  }

  /** Commits the rotation as one undoable action, or drops a drag that did not
   *  move. Returns whether a gesture was live. */
  function finish(event) {
    if (!session) return false;
    const { node, degrees, startDegrees, outline } = session;
    session = null;
    outline.remove();
    event?.preventDefault?.();
    if (Math.abs(degrees - startDegrees) < 0.5) {
      io.drawSelection();
      return true;
    }
    apply(node, degrees);
    return true;
  }

  /** Drops a live gesture without writing anything (Escape, or a selection that
   *  went away under it). */
  function cancel() {
    if (!session) return false;
    session.outline.remove();
    session = null;
    io.drawSelection();
    return true;
  }

  /** The one engine call, gated like every other geometry mutation. */
  function apply(node, degrees) {
    io.runEdit(() => io.doc().setObjectRotation(node, normaliseDegrees(degrees)), { gate: true });
    io.setStatus(io.t("object.rotate.applied", { degrees: Math.round(normaliseDegrees(degrees)) }));
  }

  /**
   * The keyboard half of the same handle.
   *
   * A grip that only a pointer can move is not an operable control, and the
   * Rotate menu's four fixed choices are not the same capability — they cannot
   * express 30 degrees. Arrows step one degree, Shift+arrow steps the same 15
   * the drag's Shift snaps to (so the two halves agree by construction), Home
   * straightens the object, and both must land on the SAME engine call the drag
   * makes or the handle and the menu would drift.
   */
  function onKey(event) {
    if (!rotatable()) return;
    const node = io.selection().node;
    const frame = frameOf(node);
    if (!frame) return;
    const current = frame[5] / 1000;
    const step = event.shiftKey ? SHIFT_SNAP_DEGREES : 1;
    let next = null;
    if (event.key === "ArrowRight" || event.key === "ArrowUp") next = current + step;
    else if (event.key === "ArrowLeft" || event.key === "ArrowDown") next = current - step;
    else if (event.key === "Home") next = 0;
    if (next === null) return;
    event.preventDefault();
    event.stopPropagation();
    apply(node, next);
  }

  return {
    start,
    update,
    finish,
    cancel,
    onKey,
    /** Whether a rotation drag is live — what tells the pointer router to send
     *  moves here rather than to the caret. */
    active: () => session !== null,
  };
}
