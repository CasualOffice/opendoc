// The resize GESTURE: pointer-down on one of the eight grips, live preview,
// one commit on release.
//
// Lifted out of `main.js` to pay for the rotation work under the line ratchet
// (`tests/module_seams.test.mjs`), and because a grip drag that now has to
// reason in the OBJECT's space as well as the page's is exactly the kind of
// arithmetic that should not be interleaved with pointer routing. Behaviour is
// unchanged for an upright object; what is new is the rotated case.
//
// The split is the established one: `object_snap.mjs` decides the box,
// `object_rotate.mjs` maps between the page's axes and the object's,
// `object_guides.mjs` draws, and this owns the session and the engine call.
//
// Cost: ONE `objectFrame` read at pointer-down and one snap context, both
// memoised on the session. A pointer move does arithmetic and writes four style
// properties — no engine call and no document walk (`docs/107` §4).
import { paintSizeReadout } from "./object_guides.mjs";
import { resizeCommitOrigin, resizeFromDrag, resizeRulesFor, snapResizedBox } from "./object_snap.mjs";
import { dragInObjectSpace, rotatedResizePlacement } from "./object_rotate.mjs";
import { TWIPS_PER_INCH } from "./units.mjs";

/** EMU per twip (914,400 EMU/inch / 1,440 twip/inch). */
const EMU_PER_TWIP = 635;

/** Minimum object edge in twips (~0.1in) so a drag can't collapse an object. */
export const MIN_OBJECT_TWIP = 144;

/**
 * A placed size as the one sentence every size readout in the editor uses.
 *
 * In the unit the reader CHOSE, through the key Page Setup's own preview label
 * declares — so a drag readout, the crop readout and the preview caption cannot
 * disagree, and "in" is not an untranslated corner in eighteen languages.
 *
 * `measure` is optional and the fallback is inches, for two real cases: a
 * caller that has no preference module, and the window before `init()` when the
 * roster has not been faulted in yet and `unit()` is undefined. The fallback
 * supplies the suffix EXPLICITLY, because the shared string carries a `{unit}`
 * placeholder and an unsubstituted placeholder does not degrade — it renders as
 * the literal text `{unit}` to the reader, which is what shipped.
 *
 * O(1).
 */
export function sizeLabel(t, widthTwip, heightTwip, measure = null) {
  const row = measure?.unit?.();
  if (row) {
    return t("pageSetup.dimensions", {
      width: measure.display(widthTwip),
      height: measure.display(heightTwip),
      unit: row.suffix,
    });
  }
  const inches = (twip) => (twip / TWIPS_PER_INCH).toFixed(2);
  return t("pageSetup.dimensions", {
    width: inches(widthTwip),
    height: inches(heightTwip),
    unit: "in",
  });
}

/**
 * Builds the resize gesture.
 *
 * @param {{
 *   doc: () => any,
 *   selection: () => any,
 *   scaleOf: (page: any) => {rect: DOMRect, sx: number, sy: number},
 *   snapContextFor: (page: any) => any,
 *   paintSnapGuides: (drag: any, guides: any[]) => void,
 *   clearSnapGuides: (drag: any) => void,
 *   runEdit: (thunk: () => any, options?: object) => any,
 *   setStatus: (text: string, kind?: string) => void,
 *   t: (key: string, vars?: object) => string,
 *   drawSelection: () => void,
 *   focusEditorSurface: () => void,
 *   hideLinkChip: () => void,
 *   resetPointerGesture: () => void,
 *   reviewMode: () => string,
 *   blockMutationInViewing: () => void,
 * }} io
 */
export function createObjectResizeDrag(io) {
  let drag = null;

  /** Begins a handle drag-resize (docs/85 §5.3). Records the object's current
   *  placed size and its angle, and shows a live preview outline; the model is
   *  untouched until release. Object geometry is not trackable, so a resize is
   *  blocked in Suggesting/Viewing mode. */
  function start(event, page, node, handleKind) {
    const selection = io.selection();
    if (!io.doc() || !selection || selection.node !== node || !selection.canResize) return;
    // `preventDefault` suppresses the compatibility mouse events for the whole
    // interaction, so a double-click that BEGINS on a grip opens no crop and
    // descends into no group — a handle is a handle, as it is in Word, Docs and
    // ONLYOFFICE. It matters more with eight grips ringing the object than with
    // three, but only the few pixels each grip overhangs by are affected, and
    // crop keeps its button.
    event.preventDefault();
    event.stopPropagation(); // do not let the page pointerdown re-hit-test
    if (io.reviewMode() === "viewing") {
      io.blockMutationInViewing();
      return;
    }
    if (io.reviewMode() === "suggesting") {
      io.setStatus(io.t("object.resize.notTracked"), "error");
      return;
    }
    io.focusEditorSurface();
    io.hideLinkChip();
    io.resetPointerGesture();
    // `objectFrame` rather than `objectRect`: for a rotated object the two agree
    // on the rectangle — it is the object's own unrotated frame either way — but
    // only this one also carries the angle, and the angle is what turns a
    // pointer delta into a change along the object's own axes.
    const frame = io.doc().objectFrame?.(node) ?? [];
    if (frame.length < 6) return;
    const [, x, y, w, h, milliDegrees] = frame;
    const degrees = milliDegrees / 1000;
    const preview = document.createElement("div");
    preview.className = "object-resize-preview";
    const { sx, sy } = io.scaleOf(page);
    preview.style.left = `${x * sx}px`;
    preview.style.top = `${y * sy}px`;
    preview.style.width = `${w * sx}px`;
    preview.style.height = `${h * sy}px`;
    // The preview has to be turned too, or a rotated object previews as an
    // upright box that matches nothing on the page and the user is resizing
    // against a rectangle that is not there.
    if (degrees) preview.style.transform = `rotate(${degrees}deg)`;
    page.overlay.appendChild(preview);
    drag = {
      node,
      root: io.selection().ref.root,
      handleKind,
      page,
      startClientX: event.clientX,
      startClientY: event.clientY,
      startX: x,
      startY: y,
      startW: w,
      startH: h,
      degrees,
      anchored: io.selection().anchored === true,
      kind: io.selection().kind,
      lastX: x,
      lastY: y,
      lastW: w,
      lastH: h,
      aspect: h > 0 ? w / h : 1,
      preview,
      snap: io.snapContextFor(page),
      guideEls: null,
    };
    event.currentTarget.setPointerCapture?.(event.pointerId);
  }

  /** Updates the resize preview from the pointer delta. Per-handle signs decide
   *  which edges grow (corners = both axes, N/S = height, E/W = width); Shift
   *  constrains a corner to the original aspect and Ctrl/Cmd resizes about the
   *  centre. Both rules, and which kinds constrain by default, are
   *  `resizeRulesFor` (docs/85 §10.3). */
  function update(event) {
    if (!drag) return;
    const { sx, sy } = io.scaleOf(drag.page);
    // The delta is measured on the page and then turned into the object's own
    // axes, which is the whole of what makes a turned object resize correctly:
    // the east grip of a shape rotated 90 degrees is dragged DOWNWARD and still
    // changes the shape's width. ONLYOFFICE does the same, from the other side —
    // `hitToHandles` runs the pointer through `getInvertTransform()` before it
    // compares against any marker, so every handle there is tested in object
    // space too.
    const local = dragInObjectSpace(
      (event.clientX - drag.startClientX) / sx,
      (event.clientY - drag.startClientY) / sy,
      drag.degrees,
    );
    const dxTwip = Math.round(local.dx);
    const dyTwip = Math.round(local.dy);
    const { shiftKey, ctrlKey, metaKey } = event;
    const modifiers = { kind: drag.kind, shiftKey, ctrlKey, metaKey };
    const box = resizeFromDrag(
      { x: drag.startX, y: drag.startY, w: drag.startW, h: drag.startH, aspect: drag.aspect },
      drag.handleKind,
      dxTwip,
      dyTwip,
      resizeRulesFor(modifiers, MIN_OBJECT_TWIP),
    );
    // `resizeFromDrag` pins the opposite corner in the OBJECT's space. For an
    // upright object that is also the page's; for a turned one it is not, and
    // leaving it there swings the shape about the page as it grows, because the
    // rotation is about a centre that just moved.
    const placed = rotatedResizePlacement(
      box,
      { x: drag.startX, y: drag.startY, w: drag.startW, h: drag.startH },
      drag.handleKind,
      drag.degrees,
      box.fromCentre,
    );
    // Alt suppresses the pull, as it does on a move. The rest of the rule — and
    // which drags are deliberately left unsnapped — is `snapResizedBox`.
    //
    // A ROTATED object is left unsnapped: the guides are page-axis lines and the
    // object's edges are not on those axes, so pulling its frame onto one would
    // align an edge that is not where the line is. Stated rather than silently
    // snapping the wrong rectangle.
    const { box: snapped, guides } = snapResizedBox(
      { ...box, ...placed },
      event.altKey || drag.degrees ? null : drag.snap,
      MIN_OBJECT_TWIP,
    );
    const { x: newX, y: newY, w: newW, h: newH } = snapped;
    io.paintSnapGuides(drag, guides);
    drag.lastX = newX;
    drag.lastY = newY;
    drag.lastW = newW;
    drag.lastH = newH;
    drag.preview.style.left = `${newX * sx}px`;
    drag.preview.style.top = `${newY * sy}px`;
    drag.preview.style.width = `${newW * sx}px`;
    drag.preview.style.height = `${newH * sy}px`;
    // Say what size you are dragging TO. The preview was an outline and nothing
    // else, so resizing to a specific size meant releasing, opening the properties
    // panel to read what you got, and correcting it there. Docs shows a W×H bubble
    // during the drag and Word live-updates its Size box; this is the same promise
    // in the place the eye already is.
    drag.readout = paintSizeReadout(drag.preview, sizeLabel(io.t, newW, newH, io.measure?.()));
    event.preventDefault();
  }

  /** Commits (or cancels) the resize on release through one engine geometry
   * transaction, converting the final page-local rectangle from twips to EMU.
   * Returns whether a drag was active. */
  function finish(event) {
    if (!drag) return false;
    const held = drag;
    drag = null;
    held.preview.remove();
    io.clearSnapGuides(held);
    event.preventDefault();
    const changed =
      Math.abs(held.lastX - held.startX) >= 8 ||
      Math.abs(held.lastY - held.startY) >= 8 ||
      Math.abs(held.lastW - held.startW) >= 8 ||
      Math.abs(held.lastH - held.startH) >= 8;
    if (changed) {
      // An inline object commits its SIZE at the origin the paragraph gave it;
      // only a float commits the origin it was dragged to. `resizeCommitOrigin`
      // carries why the preview still pins the opposite edge.
      const origin = resizeCommitOrigin(
        { x: held.lastX, y: held.lastY },
        { x: held.startX, y: held.startY },
        held.anchored,
      );
      io.runEdit(
        () =>
          io.doc().resizeObject(
            held.root,
            origin.x * EMU_PER_TWIP,
            origin.y * EMU_PER_TWIP,
            held.lastW * EMU_PER_TWIP,
            held.lastH * EMU_PER_TWIP,
          ),
        { gate: true },
      );
    } else {
      io.drawSelection();
    }
    return true;
  }

  /** Aborts an in-progress resize (pointer cancel / window blur / Escape),
   *  discarding the preview and committing nothing. */
  function cancel() {
    if (!drag) return;
    drag.preview.remove();
    io.clearSnapGuides(drag);
    drag = null;
    io.drawSelection();
  }

  return {
    start,
    update,
    finish,
    cancel,
    /** Whether a resize drag is live — what the pointer router and the hover
     *  cursor both ask. */
    active: () => drag !== null,
    /** The live drag record, for the cursor router's `resizeDrag` probe. */
    record: () => drag,
  };
}
