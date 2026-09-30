// Touch selection: long press, two handles, a magnifier and the caret drag
// (`docs/105` UX-018, `docs/148` §9 item 2).
//
// ## The problem, stated
//
// A phone could place a caret by tapping and could do nothing else. There was
// no way to MAKE a selection with a finger — no handles to grab, no
// long-press-to-select-a-word — and no way to see what was under the finger,
// which is the half that makes the other half unusable: the text you are
// aiming at is the text your fingertip is covering.
//
// ## The named pattern, before any code
//
// This is not a new problem and none of the numbers below are taste. It is the
// **mobile text-selection machine** every touch platform ships: a press timer,
// a movement slop, two draggable endpoint handles with a hit radius larger than
// their drawn size, and a loupe. The reference implementation read for this
// change is ONLYOFFICE's `common/Scrolls/mobileTouchManagerBase.js` (2,774
// lines, AGPL-3.0 — **behaviour and structure only, no code taken**), whose
// mode machine is `None, Scroll, Zoom, Select, InlineObj, FlowObj, Cursor,
// TableMove, TableRuler, SelectTrack`.
//
// ### What was followed, and where the numbers come from
//
//   * `ReadingGlassTime = 750`ms (`:682`) -> `LONG_PRESS_MS`. A held finger
//     becomes a cursor drag / magnifier after this.
//   * `MoveMinDist = 20`px (`:686`) -> `MOVE_MIN_DIST_PX`. Below this a drag is
//     still a press; above it, it is a scroll and the press is off.
//   * `TrackTargetEps = 20`px (`:699`, used at `:958`) -> `TRACK_TARGET_EPS_PX`.
//     The hit radius around a selection handle, tested per axis.
//   * `MOBILE_SELECT_TRACK_ROUND = 14` (`:136`, drawn at `:1911` as
//     `AddEllipse(x, y, ROUND / 2)`) -> `HANDLE_DOT_PX`, a 14px circle whose
//     radius is 7. Draw small, hit big — the separation this repository already
//     uses in `table_gutter_zones.mjs` (`TOUCH_STRIP_PX`), `table_chrome.mjs`
//     (`TOUCH_PILL_PX`) and twice in `style.css` (a 9px grip in a 24px target).
//   * `CheckGlass` — a 100px circle at 2x drawn 25px above the finger ->
//     `MAGNIFIER_PX`, `MAGNIFIER_SCALE`, `MAGNIFIER_GAP_PX`. They clamp the
//     SOURCE rect against the raster's edges and shift the destination by the
//     clamped amount, which `magnifierSource` reproduces.
//   * Word selection on a double tap is **not** separate touch code there:
//     `onTouchStart` routes into the same click counter the mouse uses
//     (`common/Drawings/WorkEvents.js:432-505`). Ours does the same, and it is
//     why `selectWord` lives HERE and is called by `main.js`'s `dblclick`,
//     triple-click and format-painter paths as well as by the long press. One
//     routine, four gestures.
//
// ### What was deliberately NOT followed
//
//   * **Their handle hit test takes the FIRST endpoint within the radius**
//     (`:958`, `:963`: `if pos1 … else if pos4 …`). On a two-character
//     selection both handles are inside one 40px box and the start always wins,
//     so the end handle is unreachable. `handleAtPoint` takes the NEAREST.
//   * **They place handles at the corners of the selection's bounding boxes**
//     (`RectSelect1.x`, `RectSelect2.x + w`). That is the left edge and the
//     right edge, which in a right-to-left paragraph are the logical END and
//     the logical START. We place each handle on the CARET RECT of the
//     endpoint, ordered by document position (`orderEnds`), so the start handle
//     is the logical start in both directions.
//   * **They suppress pinch-zoom and native scrolling wholesale**
//     (`user-scalable=no`, `stopEvent` on every touch). ADR-044 refuses that:
//     removing magnification with nothing behind it is an accessibility
//     failure. Here the page scrolls natively unless this module owns the
//     gesture, and it only owns it once a handle is grabbed or a press has been
//     held for 750ms.
//   * **They have a `Zoom` mode.** Pinch is still `105` UX-018's open half and
//     is not in this module; the browser's own pinch is left alone.
//
// ## Where it arms — neither `phone-mode` nor a media query
//
// The gesture arms on `event.pointerType === "touch"` and nothing else, and the
// handles are painted only while the last pointer to press the document was a
// touch. A device class is the wrong question. `body.phone-mode` would leave a
// tablet — which has exactly the same problem, at 820px — with no way to select
// text, and `(pointer: coarse)` is false on a touchscreen laptop whose primary
// pointer is a mouse while `(any-pointer: coarse)` is true on that same laptop
// even when the person is using the mouse, which would paint finger handles
// over a mouse selection. The pointer that made the selection is the only
// signal that is right on all four devices.
//
// ## Complexity
//
// Per-interaction work is O(1) in document size (`docs/107` §4), which matters
// more here than anywhere: a phone is the slowest device we support.
//
//   * A pointer-move during a handle drag costs **one** engine hit test
//     (`anchorAt`), exactly what the existing mouse drag costs, plus one
//     `drawImage` from the page's own raster. Never a document walk.
//   * A repaint costs **two** `caretRect` calls (one per endpoint) and appends
//     two elements. Nothing iterates the selection, the page or the document.
//   * The press timer and the slop test are arithmetic on two points.
//
// The magnifier reads the page's EXISTING canvas rather than rendering
// anything: there is one rendering path and the loupe is a `drawImage` of it.
//
// ## Purity
//
// Every browser object arrives through `io` — the window as `view`, the
// document surface as `surface`, an element factory as `makeEl`. The module
// names no browser global, so the whole of the arithmetic above is driven from
// Node in `tests/touch_selection.test.mjs` and the module sits in
// `module_seams.test.mjs`'s `PURE_MODULES`.

import { t } from "./i18n.mjs";
import { MIN_TOUCH_TARGET_PX } from "./phone_chrome.mjs";

/** ONLYOFFICE `ReadingGlassTime` (`mobileTouchManagerBase.js:682`): how long a
 *  finger must be held still before the press becomes a selection. */
export const LONG_PRESS_MS = 750;

/** ONLYOFFICE `MoveMinDist` (`:686`): the slop a press is allowed before it is
 *  a scroll instead. Tested per axis, as they test it (`:2458`). */
export const MOVE_MIN_DIST_PX = 20;

/** ONLYOFFICE `TrackTargetEps` (`:699`): the hit radius around a handle's drawn
 *  dot, per axis — so the grabbable box is `2 * eps` on a side. */
export const TRACK_TARGET_EPS_PX = 20;

/** ONLYOFFICE `MOBILE_SELECT_TRACK_ROUND` (`:136`): the DRAWN diameter of a
 *  handle's dot. Seven pixels of radius inside a target several times its size
 *  — the draw-small/hit-big separation, not a small target. */
export const HANDLE_DOT_PX = 14;

/** The grabbable box around a handle. `MIN_TOUCH_TARGET_PX` is WCAG 2.5.8's
 *  floor and `2 * TRACK_TARGET_EPS_PX` is ONLYOFFICE's measured reach; the
 *  larger of the two is the box, so neither rule is quietly lost. */
export const HANDLE_TARGET_PX = Math.max(MIN_TOUCH_TARGET_PX, 2 * TRACK_TARGET_EPS_PX);

/** ONLYOFFICE `CheckGlass`: a 100px circle, at 2x, drawn 25px clear of the
 *  finger. */
export const MAGNIFIER_PX = 100;
export const MAGNIFIER_SCALE = 2;
export const MAGNIFIER_GAP_PX = 25;

/** Has the finger travelled far enough that this is a scroll and not a press?
 *
 *  Per axis, which is how ONLYOFFICE ask it (`:2458-2459`) — a diagonal drag of
 *  20px in each direction is 28px of travel and is still a press to them. Kept
 *  identical rather than "improved" to a hypotenuse, because the number was
 *  measured against the axis test and means something else without it.
 */
export function movedPastThreshold(from, to, slop = MOVE_MIN_DIST_PX) {
  return Math.abs(to.x - from.x) > slop || Math.abs(to.y - from.y) > slop;
}

/** May a pointer-move from this pointer EXTEND a text drag-selection?
 *
 *  True for a mouse, a pen and anything else; false for a finger. This is the
 *  boundary between the shell's drag-selection machine (`main.js`'s
 *  `updateDragSelection`: press, then every move extends the range, no slop and
 *  no timer, because a held mouse button means exactly one thing) and this
 *  module (which owns the touch gesture and only calls it a selection after a
 *  750ms press or a grab on a handle — ONLYOFFICE's `Scroll` vs `Select` modes,
 *  `docs/148` §3, and the same split iOS, Android, Word mobile and Docs mobile
 *  all ship).
 *
 *  ## Why this exists, measured
 *
 *  Both machines were reading the same `pointermove` stream, so a finger drove
 *  two selection machines and the slop-free one won every race: **a phone
 *  scroll selected text.** The slop here and the window's `pointercancel` ->
 *  `abortPointerGestures` were both firing correctly and both firing too late.
 *  Chromium serves one or two `pointermove`s before it decides the touch is a
 *  pan; on a 390x844 phone, a 160px upward scroll of `sample.docx` moved the
 *  selection's focus into another paragraph on the SECOND `pointermove` at
 *  t+164ms — 4ms before `pointercancel` and 25ms before the first scroll event.
 *  The 750ms press timer never fired at all, and `touchmove` reached this
 *  module's surface listener 11 times with the press already correctly
 *  cancelled, which is why adding a second cancel path changed nothing.
 *
 *  And the symptom was nearly invisible: `selectionRects` returned nothing for
 *  that range, so `paintSelection` fell through to painting a CARET. No
 *  highlight rectangle existed anywhere. The only trace on the glass was this
 *  module's two handles — painted from the MODEL selection — so "scrolling
 *  selects text" looked like two dots floating over unhighlighted text. That is
 *  why `phone-touch-selection.spec.mjs` asserts the handle count and a
 *  range-requiring command's enabled state rather than trusting the highlight.
 *
 *  ## Keyed on the POINTER, not the device
 *
 *  Same reason as "Where it arms" above: a touchscreen laptop must keep its
 *  mouse drag-selection and a tablet must lose its finger one, and neither
 *  `body.phone-mode` nor `(any-pointer: coarse)` can tell those apart.
 */
export function pointerDragSelects(event) {
  return event?.pointerType !== "touch";
}

/** Which handle a point grabs, or `null`.
 *
 *  NEAREST wins, not first — see the "deliberately not followed" note above.
 *  `handles` is the list the last paint produced, in client coordinates.
 */
export function handleAtPoint(handles, point, eps = TRACK_TARGET_EPS_PX) {
  let best = null;
  let bestDistance = Infinity;
  for (const handle of handles ?? []) {
    const dx = Math.abs(handle.x - point.x);
    const dy = Math.abs(handle.y - point.y);
    if (dx > eps || dy > eps) continue;
    const distance = dx + dy;
    if (distance < bestDistance) {
      bestDistance = distance;
      best = handle.which;
    }
  }
  return best;
}

/** Is engine rect `a` before engine rect `b` in document order?
 *
 *  Both are `[pageNumber, x, y, w, h]` in twips, as `caretRect` reports them.
 *  Page, then line, then position on the line — the reading order, which is
 *  also the order the two endpoints of a range have.
 */
export function rectIsBefore(a, b) {
  if (a[0] !== b[0]) return a[0] < b[0];
  if (a[2] !== b[2]) return a[2] < b[2];
  return a[1] < b[1];
}

/** Orders a selection's two endpoints into `{ start, end, swapped }`.
 *
 *  This is the whole of the handle-crossing rule. Dragging one handle past the
 *  other does not collapse the selection and does not stop the drag: the fixed
 *  endpoint stays the anchor, the finger keeps the focus, and the two handles
 *  simply trade places because this function reads the geometry afresh on every
 *  repaint. `swapped` is what a test can see that happening through.
 */
export function orderEnds(anchor, focus) {
  const forward = rectIsBefore(anchor.rect, focus.rect);
  return {
    start: forward ? anchor : focus,
    end: forward ? focus : anchor,
    swapped: !forward,
  };
}

/** Where the loupe goes, in client coordinates, so that it never leaves the
 *  window.
 *
 *  ONLYOFFICE draw it `MAGNIFIER_GAP_PX` above the finger and let it run off
 *  the top of the canvas (`CheckGlass` gives `dstY` a negative value and clips).
 *  Nothing here may paint outside the window — the sibling of the
 *  no-horizontal-scroll rule `docs/148` §6 states — so at the top of the screen
 *  it FLIPS below the finger instead, which is also the only placement that
 *  leaves the text visible there.
 */
export function magnifierPlacement({
  x,
  y,
  viewWidth,
  viewHeight,
  size = MAGNIFIER_PX,
  gap = MAGNIFIER_GAP_PX,
}) {
  let top = y - gap - size;
  let flipped = false;
  if (top < 0) {
    top = y + gap;
    flipped = true;
  }
  // A flip at the very bottom of a short window has nowhere to go either; clamp
  // rather than leave it half off-screen.
  if (top + size > viewHeight) top = viewHeight - size;
  top = Math.max(0, top);
  const left = Math.max(0, Math.min(x - size / 2, viewWidth - size));
  return { left, top, flipped };
}

/** The source rectangle to lift out of the page raster, clamped to it.
 *
 *  Coordinates are raster pixels. When the square would reach past the top or
 *  left edge it is clamped and the DESTINATION is shifted by the clamped amount
 *  times the magnification, so the finger stays under the same point of the
 *  image instead of the whole loupe sliding — ONLYOFFICE's `CheckGlass` does
 *  exactly this and it is the only part of their glass that is not obvious.
 */
export function magnifierSource({ x, y, width, height, size, scale = MAGNIFIER_SCALE }) {
  let sx = x - size / 2;
  let sy = y - size / 2;
  let dx = 0;
  let dy = 0;
  if (sx < 0) {
    dx = -sx * scale;
    sx = 0;
  }
  if (sy < 0) {
    dy = -sy * scale;
    sy = 0;
  }
  const sw = Math.max(0, Math.min(size, width - sx));
  const sh = Math.max(0, Math.min(size, height - sy));
  return { sx, sy, sw, sh, dx, dy };
}

const samePoint = (a, b) => !!a && !!b && a.node === b.node && a.offset === b.offset;

/**
 * Wires the touch-selection machine to a document surface.
 *
 * @param {object} io                    every browser and engine dependency
 * @param {Window} io.view               the window: its size, its timers, its pointer stream
 * @param {HTMLElement} io.surface       `#pages` — where a press arms the machine
 * @param {HTMLElement} io.mount         where the loupe is appended (a fixed-position layer)
 * @param {(tag:string)=>HTMLElement} io.makeEl
 * @param {()=>boolean} io.enabled       false while there is no document, or no caret region
 * @param {(x:number,y:number)=>object} io.pageAt   client point -> page record
 * @param {(page:object)=>object} io.scaleOf        page -> `{ rect, sx, sy }`
 * @param {(page:object,event:object)=>object} io.anchorAt  one engine hit test
 * @param {(p:object)=>number[]} io.caretRect       endpoint -> `[page, x, y, w, h]` twips
 * @param {(node:string,offset:number)=>number[]} io.wordAt
 * @param {()=>object} io.selection
 * @param {(next:object)=>void} io.setSelection
 * @param {()=>void} io.draw             repaint (calls back into `paint`)
 * @param {()=>void} io.focus            put focus on the editable proxy
 * @param {()=>void} io.cancelGesture    let the mouse drag machine go
 */
export function createTouchSelection(io) {
  /** Handles the last paint produced: `{ which, x, y }` in client coordinates.
   *  The hit test reads this rather than the DOM, so it is the same list the
   *  unit tests drive. */
  let handles = [];
  /** The press in flight: `{ x, y, timer }`, or null. */
  let press = null;
  /** The drag this module owns: `{ fixed }`, where `fixed` is the endpoint the
   *  finger is NOT holding. Null when the page owns the gesture. */
  let drag = null;
  /** Whether the last press on the document came from a finger. The handles are
   *  a finger's chrome and appear for no other pointer. */
  let touchTier = false;
  let glass = null;

  const clear = () => {
    if (press?.timer) io.view.clearTimeout(press.timer);
    press = null;
  };

  /** Selects the word under a point, the same routine the mouse's double click
   *  uses. ONLYOFFICE route both gestures through one click counter for exactly
   *  this reason; two implementations of "what is a word here" diverge. */
  function selectWord(page, event) {
    const at = io.anchorAt(page, event);
    if (!at) return false;
    io.focus();
    const bounds = io.wordAt(at.node, at.offset); // [start, end] or []
    if (bounds.length !== 2) return false;
    io.setSelection({
      anchor: { node: at.node, offset: bounds[0] },
      focus: { node: at.node, offset: bounds[1] },
    });
    io.draw();
    return true;
  }

  /** Paints the two handles onto the page overlays, and records where they are.
   *
   *  Called from the overlay repaint, beside `tableChrome.paintTouchPills` —
   *  the same shape, for the same reason: a touch affordance is chrome over the
   *  raster and belongs on the layer that already holds the highlight.
   *
   *  Two `caretRect` calls, whatever the selection's size. O(1).
   */
  function paint(pages) {
    handles = [];
    if (!touchTier || !io.enabled()) return;
    const selection = io.selection();
    if (!selection) return;
    // A bare caret gets no handles, on every platform: they are the chrome of a
    // RANGE. A collapsed caret being dragged shows the loupe and nothing else.
    if (samePoint(selection.anchor, selection.focus)) return;
    const anchor = { at: selection.anchor, rect: io.caretRect(selection.anchor) };
    const focus = { at: selection.focus, rect: io.caretRect(selection.focus) };
    if (anchor.rect.length < 5 || focus.rect.length < 5) return;
    const ends = orderEnds(anchor, focus);
    placeHandle(pages, "start", ends.start.rect);
    placeHandle(pages, "end", ends.end.rect);
  }

  function placeHandle(pages, which, rect) {
    const [pageNumber, x, y, w, h] = rect;
    const page = pages[pageNumber - 1];
    if (!page?.overlay) return;
    const { rect: box, sx, sy } = io.scaleOf(page);
    // Clear of the line rather than on top of it: the start dot hangs above the
    // caret's top and the end dot below its bottom, so neither covers the text
    // it marks. ONLYOFFICE offset theirs by 5px for the same reason (`:1911`).
    const cx = x * sx;
    const cy =
      which === "start" ? y * sy - HANDLE_DOT_PX / 2 : (y + h) * sy + HANDLE_DOT_PX / 2;
    const el = io.makeEl("div");
    el.className = `touch-handle is-${which}`;
    el.style.left = `${cx - HANDLE_TARGET_PX / 2}px`;
    el.style.top = `${cy - HANDLE_TARGET_PX / 2}px`;
    el.style.width = `${HANDLE_TARGET_PX}px`;
    el.style.height = `${HANDLE_TARGET_PX}px`;
    el.dataset.handle = which;
    // Labelled and given a role, not hidden: on a touch device a screen reader
    // explores by touch and these are the only visible selection affordances.
    // Deliberately NOT focusable — the keyboard route to a selection is
    // Shift+Arrow and putting two extra stops in the tab order would make the
    // keyboard worse to improve the finger.
    el.setAttribute("role", "button");
    el.setAttribute(
      "aria-label",
      t(which === "start" ? "touchSelection.startHandle" : "touchSelection.endHandle"),
    );
    page.overlay.appendChild(el);
    handles.push({ which, x: box.left + cx, y: box.top + cy });
  }

  /** The loupe: the page's own raster, magnified, above the finger. */
  function showGlass(page, x, y) {
    const canvas = page?.canvas;
    if (!canvas?.width || !canvas?.height) return hideGlass();
    if (!glass) {
      const el = io.makeEl("div");
      el.className = "touch-magnifier";
      el.setAttribute("aria-hidden", "true");
      const face = io.makeEl("canvas");
      el.appendChild(face);
      io.mount.appendChild(el);
      glass = { el, face };
    }
    // The CANVAS's own rect, not the sheet's. They coincide today, and asking
    // the raster where it is means the loupe cannot be one padding change away
    // from magnifying the wrong pixels.
    const rect = canvas.getBoundingClientRect();
    if (!rect.width) return hideGlass();
    // Raster pixels per CSS pixel: the page canvas is drawn at device scale, so
    // the loupe reads its backing store rather than a re-render.
    const ratio = canvas.width / rect.width;
    const source = magnifierSource({
      x: (x - rect.left) * ratio,
      y: (y - rect.top) * ratio,
      width: canvas.width,
      height: canvas.height,
      size: (MAGNIFIER_PX / MAGNIFIER_SCALE) * ratio,
    });
    const face = glass.face;
    const side = Math.round(MAGNIFIER_PX * ratio);
    if (face.width !== side) {
      face.width = side;
      face.height = side;
    }
    const ctx = face.getContext("2d");
    ctx.clearRect(0, 0, side, side);
    if (source.sw > 0 && source.sh > 0) {
      ctx.drawImage(
        canvas,
        source.sx,
        source.sy,
        source.sw,
        source.sh,
        source.dx,
        source.dy,
        source.sw * MAGNIFIER_SCALE,
        source.sh * MAGNIFIER_SCALE,
      );
    }
    const place = magnifierPlacement({
      x,
      y,
      viewWidth: io.view.innerWidth,
      viewHeight: io.view.innerHeight,
    });
    glass.el.style.left = `${place.left}px`;
    glass.el.style.top = `${place.top}px`;
    glass.el.dataset.flipped = place.flipped ? "1" : "0";
    glass.el.hidden = false;
  }

  function hideGlass() {
    if (glass) glass.el.hidden = true;
  }

  /** Moves the endpoint the finger holds to the point it is now over. One
   *  engine hit test, which is what the mouse drag costs too. */
  function dragTo(x, y) {
    const page = io.pageAt(x, y);
    if (!page) return;
    const at = io.anchorAt(page, { clientX: x, clientY: y });
    if (at) {
      // The fixed endpoint stays the anchor. Crossing it therefore reverses the
      // range rather than collapsing it, and `orderEnds` hands the handles over
      // on the next repaint without this code knowing that happened.
      io.setSelection({ anchor: drag.fixed, focus: at });
      io.draw();
    }
    showGlass(page, x, y);
  }

  /** The endpoint the finger is NOT holding, when it grabs `which`. */
  function fixedEndFor(which) {
    const selection = io.selection();
    if (!selection) return null;
    const anchor = { at: selection.anchor, rect: io.caretRect(selection.anchor) };
    const focus = { at: selection.focus, rect: io.caretRect(selection.focus) };
    if (anchor.rect.length < 5 || focus.rect.length < 5) return null;
    const ends = orderEnds(anchor, focus);
    return (which === "start" ? ends.end : ends.start).at;
  }

  function onPointerDown(event) {
    if (event.pointerType !== "touch") {
      touchTier = false;
      return;
    }
    touchTier = true;
    clear();
    drag = null;
    if (!io.enabled()) return;
    const point = { x: event.clientX, y: event.clientY };
    // The ELEMENT first, because it is the same region `handleAtPoint`
    // describes (its box is `2 * TRACK_TARGET_EPS_PX` on a side, centred on the
    // dot) and it is the one that cannot go stale: a scroll moves the handle
    // without repainting, and the recorded client coordinates would then name
    // where it used to be. The arithmetic is the fallback, and is what the unit
    // tests drive, so the rule is stated once and testable.
    const which =
      event.target?.closest?.(".touch-handle")?.dataset?.handle ?? handleAtPoint(handles, point);
    if (which) {
      const fixed = fixedEndFor(which);
      if (fixed) {
        drag = { fixed };
        // The page must not scroll under a handle drag, and nothing later can
        // undo that decision: the browser latches it at touch-start from the
        // hit element's `touch-action`, which is why `.touch-handle` carries
        // `touch-action: none` the way the object move pad already does.
        event.preventDefault();
        event.stopPropagation();
        dragTo(point.x, point.y);
        return;
      }
    }
    // Not a handle: let the press through, so a tap still places a caret and a
    // drag still scrolls the page. All this arms is a timer.
    press = {
      ...point,
      timer: io.view.setTimeout(() => {
        const held = press;
        press = null;
        if (!held || !io.enabled()) return;
        // Take the mouse drag machine's gesture away before selecting, or its
        // pointer-up would treat this as a tap and a later move would drag the
        // caret out of the word this just selected.
        io.cancelGesture();
        const page = io.pageAt(held.x, held.y);
        if (!page) return;
        const at = { clientX: held.x, clientY: held.y };
        // Whether or not there is a word here, the finger now owns an endpoint:
        // on a word, this extends the selection Android-style; in whitespace,
        // `wordAt` answers nothing, the caret stays collapsed, and the finger
        // drags the caret — ONLYOFFICE's `Cursor` mode and iOS's behaviour. One
        // mechanism, both readings.
        selectWord(page, at);
        const selection = io.selection();
        if (!selection) return;
        const anchor = { at: selection.anchor, rect: io.caretRect(selection.anchor) };
        const focus = { at: selection.focus, rect: io.caretRect(selection.focus) };
        drag =
          anchor.rect.length >= 5 && focus.rect.length >= 5
            ? { fixed: orderEnds(anchor, focus).start.at }
            : { fixed: selection.anchor };
        io.draw();
        showGlass(page, held.x, held.y);
      }, LONG_PRESS_MS),
    };
  }

  function onPointerMove(event) {
    if (event.pointerType !== "touch") return;
    if (drag) {
      event.preventDefault();
      dragTo(event.clientX, event.clientY);
      return;
    }
    if (!press) return;
    // A scroll, not a press. Drop the timer: a phone scroll routinely lasts
    // longer than 750ms, so without this the press would fire in the middle of
    // one and select a word out from under a moving finger. That is the whole
    // job of `MoveMinDist` and it is why ONLYOFFICE measured it.
    //
    // Nothing else is done here, and this note used to say why in terms that
    // were WRONG. It read: the mouse drag machine lets go by itself, because
    // Chromium fires `pointercancel` the moment it takes the gesture for a pan
    // and `main.js`'s `abortPointerGestures` listens for it. It does fire, and
    // that listener does run — but both happen AFTER the shell has already
    // served one or two `pointermove`s and moved the selection with them. On a
    // 390x844 phone the focus had crossed into another paragraph 4ms before
    // `pointercancel` arrived. A release here would have been too late for the
    // same reason. What the shell needed was not a second release but never to
    // have extended a range under a finger at all: `pointerDragSelects`.
    if (movedPastThreshold(press, { x: event.clientX, y: event.clientY })) clear();
  }

  function onPointerUp() {
    clear();
    drag = null;
    hideGlass();
  }

  io.surface.addEventListener("pointerdown", onPointerDown, true);
  io.view.addEventListener("pointermove", onPointerMove, true);
  io.view.addEventListener("pointerup", onPointerUp, true);
  // `pointermove.preventDefault()` does not stop a pan — the browser decides
  // scrolling from the TOUCH stream — so while this module owns the gesture the
  // touch move is cancelled here. Non-passive on purpose; it is a no-op on
  // every event this module does not own.
  io.surface.addEventListener(
    "touchmove",
    (event) => {
      if (drag) event.preventDefault();
    },
    { passive: false },
  );

  /** How far other floating chrome must stand off the selection while the
   *  handles are up, in CSS pixels; 0 when they are not.
   *
   *  FOUND BY LOOKING, not by a spec. The floating format bar sits 8px above
   *  the selection and the start handle's dot hangs `HANDLE_DOT_PX` above it,
   *  so the bar cut the dot in half and — because the bar is a fixed element
   *  over the page — took the touch that was aiming at it. The handle was
   *  simply not grabbable whenever the bar was up, which is every time a touch
   *  selection exists, and nothing clickable could fail. Reported here rather
   *  than hard-coded at the bar, so the number cannot drift away from the dot
   *  it is clearing.
   *
   *  It clears the TARGET, not the dot. Standing off by the dot alone still
   *  left the bar across the top five pixels of the 40px box — measured, and
   *  the guard stayed red — and the box is the part a finger has to hit. Half
   *  the box, plus the half-dot the box is offset by, plus the bar's own 8px. */
  const clearance = () => (handles.length ? (HANDLE_TARGET_PX + HANDLE_DOT_PX) / 2 : 0);

  /** `cancel` drops everything in flight: a lost pointer, a blur, a hidden tab. */
  return { paint, selectWord, clearance, cancel: onPointerUp, handles: () => handles };
}
