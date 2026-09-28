// Drawing a shape on the page: the MODE, and its preview.
//
// The arithmetic is `shape_draw.mjs` and the catalogue is
// `shape_catalogue.mjs`; this is the state machine that sits between a gallery
// pick and an `insertShape`. Word's sequence exactly: pick a preset, the
// pointer becomes a crosshair, drag to draw the shape at the size you want,
// release to place it; a bare click places it at the default size; Escape
// leaves the mode without placing anything.
//
// The preview is a real outline of the preset being drawn, not a plain
// rectangle, because the rectangle is what the crop dialog was — a generic
// stand-in where the competitor shows you the thing. It is drawn from the SAME
// catalogue entry the gallery cell used, so the preview cannot disagree with
// the cell you pressed.
//
// PERFORMANCE. The gesture reads nothing from the document while it runs. The
// page's size is captured once at press; every pointermove writes four numbers
// into one SVG node's attributes. O(1) per move, in a document of any size.
import { clampToPage, committedRect, drawnRect } from "./shape_draw.mjs";
import { shapePreviewShape } from "./shape_catalogue.mjs";

const SVG_NS = "http://www.w3.org/2000/svg";

/**
 * @param {{
 *   pointToTwip: (page: object, event: PointerEvent) => {x: number, y: number},
 *   scaleOf: (page: object) => {sx: number, sy: number},
 *   outlineFor: (token: string) => object|null,
 *   insert: (token: string, rect: object) => void,
 *   setStatus: (text: string, kind?: string) => void,
 *   t: (key: string) => string,
 * }} io
 */
export function createShapeDrawMode(io) {
  /** The preset the pointer is armed with, or null. */
  let armed = null;
  /** The live gesture, or null. */
  let drag = null;

  function paint() {
    if (!drag) return;
    const { sx, sy } = io.scaleOf(drag.page);
    const rect = clampToPage(
      drawnRect(drag.start, drag.current, { square: drag.square }),
      { width: drag.page.wTwip, height: drag.page.hTwip },
    );
    drag.rect = rect;
    const box = drag.box;
    box.style.left = `${rect.left * sx}px`;
    box.style.top = `${rect.top * sy}px`;
    box.style.width = `${Math.max(1, rect.width * sx)}px`;
    box.style.height = `${Math.max(1, rect.height * sy)}px`;
  }

  /** The preset's own outline, sized to the preview box by `viewBox` alone so a
   *  move costs no path arithmetic. */
  function previewNode(token) {
    const outline = io.outlineFor(token);
    const svg = document.createElementNS(SVG_NS, "svg");
    svg.setAttribute("viewBox", "0 0 100 100");
    svg.setAttribute("preserveAspectRatio", "none");
    svg.setAttribute("aria-hidden", "true");
    if (!outline) return svg;
    const spec = shapePreviewShape(outline, 100);
    const node = document.createElementNS(SVG_NS, spec.tag);
    for (const [name, value] of Object.entries(spec.attrs)) node.setAttribute(name, String(value));
    svg.appendChild(node);
    return svg;
  }

  return {
    /** Arms the pointer with a preset. The mode is reflected onto `<body>` so
     *  the crosshair is a CSS concern and a test can see the mode exists. */
    arm(token) {
      armed = token;
      drag = null;
      document.body.dataset.shapeDraw = token;
      io.setStatus(io.t("shape.drawHint"));
    },
    /** Leaves the mode. `announce` false for the silent disarm after a
     *  successful placement, which has its own message. */
    disarm(announce = false) {
      armed = null;
      if (drag) {
        drag.box.remove();
        drag = null;
      }
      delete document.body.dataset.shapeDraw;
      if (announce) io.setStatus(io.t("shape.drawCancelled"));
    },
    armedToken: () => armed,
    dragging: () => !!drag,

    /** Begins the gesture. Returns false when the pointer is not armed, so the
     *  caller falls through to ordinary selection. */
    tryBeginDraw(page, event) {
      if (!armed || event.button !== 0) return false;
      const start = io.pointToTwip(page, event);
      const box = document.createElement("div");
      box.className = "shape-draw-preview";
      box.dataset.shapeDraw = armed;
      box.appendChild(previewNode(armed));
      page.overlay?.appendChild(box);
      drag = { page, start, current: start, square: event.shiftKey, box, rect: null };
      paint();
      event.preventDefault();
      // The overlay takes the pointer so a drag that leaves the sheet — or a
      // touch drag the browser would otherwise turn into a scroll — still
      // finishes as a draw.
      page.overlay?.setPointerCapture?.(event.pointerId);
      return true;
    },

    /** Tracks the pointer. Four style writes; no document access. */
    update(event) {
      if (!drag) return;
      drag.current = io.pointToTwip(drag.page, event);
      drag.square = event.shiftKey;
      paint();
    },

    /** Finishes the gesture and asks the caller to insert. A press-and-release
     *  that never moved is a CLICK, and a click places the default size — which
     *  is what makes the gallery usable without knowing about the drag. */
    finish() {
      if (!drag) return false;
      const { page, rect } = drag;
      const token = armed;
      drag.box.remove();
      drag = null;
      const placed = clampToPage(committedRect(rect ?? { left: 0, top: 0, width: 0, height: 0 }), {
        width: page.wTwip,
        height: page.hTwip,
      });
      this.disarm(false);
      io.insert(token, { page, ...placed });
      return true;
    },
  };
}
