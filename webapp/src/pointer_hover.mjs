// The DOM half of the hover router (docs/109 HF-179).
//
// `pointer_cursor.mjs` decides WHICH cursor from a plain description of what is
// under the pointer; this module gathers that description from the engine and
// writes the answer onto the page canvases. The split is what makes the whole
// mapping testable in node: the decision has no DOM, and this half has no
// policy.
//
// It is a module rather than another handful of functions in `main.js` for the
// reason HF-085 exists: the editor's behaviour should be reachable from outside
// the 17,000-line file. The host it takes is a small set of accessors — live
// getters, not values, because every one of them changes between frames.

import { resolvePointerCursor } from "./pointer_cursor.mjs";

/** The gesture in flight, named as `pointer_cursor.mjs` names it, or `""`.
 *  Derived here rather than in the host so the precedence between two drags
 *  that could in principle both be live is written down once. */
function dragOf(state) {
  if (state.resizeDrag) return "object-resize";
  if (state.cropDrag) return "object-crop";
  if (state.moveDrag) return "object-move";
  if (state.tableDrag) return "table-column";
  return state.textDrag ? "text" : "";
}

/**
 * Creates the hover router bound to one editor host.
 *
 * @param {object} host
 * @param {() => object|null} host.doc            the live engine wrapper
 * @param {() => Array} host.pages                the page records
 * @param {() => Iterable} host.materializedPages the sheets currently in the DOM
 * @param {(page, event) => {x:number,y:number}} host.pointToTwip
 * @param {(page, event) => object|null} host.linkAt
 * @param {(node, page, x, y) => boolean} host.pointInsideObject
 * @param {(payload) => object} host.objectCapabilities
 * @param {() => object} host.state               the host flags, per frame
 * @returns {{ schedule: Function, clear: Function, dragKind: Function }}
 */
export function createPointerHover(host) {
  let frame = 0;
  let pending = null;

  /** Writes one router answer onto the surface. `page` is the sheet the pointer
   *  is over, or `null` during a drag, when the gesture owns the whole window
   *  and the pointer routinely leaves the sheet it started on. The inline style
   *  is what beats `.page { cursor: text }`; `body` carries a drag past the
   *  edge of the paper. */
  function apply(page, cursor, target) {
    document.body.style.cursor = !page && cursor ? cursor : "";
    for (const sheet of host.materializedPages()) {
      if (!sheet.canvas) continue;
      const mine = !page || sheet === page;
      // `cursor === null` is the format-painter mode: its brush is a CSS rule
      // over the whole sheet, and an inline style would silently beat it.
      sheet.canvas.style.cursor = mine && cursor ? cursor : "";
      if (mine && cursor) sheet.canvas.dataset.pointerTarget = target;
      else delete sheet.canvas.dataset.pointerTarget;
    }
  }

  /** What is under the pointer, as the plain facts `resolvePointerCursor` reads.
   *
   *  The engine queries are ordered to match what `onPointerDown` actually does
   *  with the press — object, then form checkbox, then caret — and each is
   *  skipped once something above has claimed the point, so the common case
   *  (body text) asks the fewest questions. They are page-local or bounded
   *  document reads; `objectAt` measures 0.7 ms at 40,000 blocks since
   *  `docs/116`, and `main-thread-budget.spec.mjs` carries a hover row so that
   *  cannot regress unnoticed. */
  function probeAt(page, event, state) {
    const doc = host.doc();
    const { x, y } = host.pointToTwip(page, event);
    const probe = {
      onPage: true,
      formatPainting: state.formatPainting,
      editsBlocked: state.editsBlocked,
      geometryBlocked: state.geometryBlocked,
      inRunningStory: state.inRunningStory,
      band: doc.bandAt(page.pageNumber, x, y) || "",
      // An object you are INSIDE is a text surface, not a target — the same
      // rule pointer-down applies, so the pointer must not offer to move it.
      insideObject:
        !!state.insideObjectNode && host.pointInsideObject(state.insideObjectNode, page, x, y),
    };
    if (!probe.insideObject) {
      const object = doc.objectAt(page.pageNumber, x, y);
      if (object) {
        probe.object = { canMove: host.objectCapabilities(object).canMove === true };
        object.free?.();
        return probe;
      }
    }
    const hit = doc.hitTest(page.pageNumber, x, y);
    if (hit) {
      probe.formCheckbox = !!doc.formCheckboxAt(hit.node, hit.offset);
      hit.free?.();
    }
    if (!probe.formCheckbox) probe.link = !!host.linkAt(page, event);
    return probe;
  }

  return {
    /** The gesture in flight, as `pointer_cursor.mjs` names it, or `""`. */
    dragKind: () => dragOf(host.state()),

    /** Drops the router's cursor everywhere, back to the CSS default. */
    clear() {
      pending = null;
      if (frame) cancelAnimationFrame(frame);
      frame = 0;
      apply(null, "", "");
    },

    /** Routes one pointer position. Throttled to one animation frame, because a
     *  pointer emits far more moves than the screen has frames and each probe
     *  asks the engine real questions. `page` may be null during a drag. */
    schedule(page, event) {
      pending = { page, clientX: event.clientX, clientY: event.clientY };
      if (frame) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        const at = pending;
        pending = null;
        if (!host.doc() || !at) return;
        const state = host.state();
        const drag = dragOf(state);
        if (!drag && !host.pages().includes(at.page)) return;
        // A gesture in flight outranks position and needs no engine query: the
        // handle it grabbed already says which way the drag goes. `rotation` is
        // 0 until the engine models object rotation (see `pointer_cursor.mjs`).
        const probe = drag
          ? {
              drag,
              handle: state.resizeDrag?.handleKind ?? state.cropDrag?.handleKind ?? 0,
              rotation: 0,
            }
          : probeAt(at.page, at, state);
        const { target, cursor } = resolvePointerCursor(probe);
        apply(drag ? null : at.page, cursor, target);
      });
    },
  };
}
