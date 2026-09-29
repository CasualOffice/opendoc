// The DOM half of object snapping: the line drawn across the page to say WHY a
// dragged object stopped where it did.
//
// Same split as `pointer_cursor.mjs` / `pointer_hover.mjs`. `object_snap.mjs`
// decides where the object goes; this decides what the user sees, and takes its
// overlay as an argument so nothing here reaches for a global page.
//
// Cost: a gesture creates at most TWO elements, once, and afterwards only moves
// and hides them. A drag fires on every pointer move, so the chrome may not be
// rebuilt per sample — `docs/107` §4.

/** The elements a live gesture keeps, so a pointer move moves a line instead of
 *  building one. Create with `null` and hand the same object back each time. */
/** @typedef {{ vertical: Element|null, horizontal: Element|null } | null} GuideElements */

/**
 * Shows exactly the guides in `guides` and hides the rest, reusing `state`'s
 * elements.
 *
 * O(1) — two axes, one element each.
 *
 * @param {Element} overlay the page overlay to paint into
 * @param {GuideElements} state the caller's per-gesture element cache
 * @param {{axis: "vertical"|"horizontal", at: number, id: string}[]} guides
 * @param {{sx: number, sy: number}} scale twips-to-CSS-pixel factors
 * @returns {{vertical: Element|null, horizontal: Element|null}} the new state
 */
export function paintGuides(overlay, state, guides, scale) {
  const next = state ?? { vertical: null, horizontal: null };
  for (const axis of ["vertical", "horizontal"]) {
    const guide = guides.find((candidate) => candidate.axis === axis) ?? null;
    let el = next[axis];
    if (!guide) {
      if (el) el.hidden = true;
      continue;
    }
    if (!el) {
      el = document.createElement("div");
      el.className = `object-align-guide is-${axis}`;
      overlay.appendChild(el);
      next[axis] = el;
    }
    // The id is on the element so a test can assert WHICH line the object was
    // pulled onto — "it snapped" and "it snapped to the page centre" are
    // different guarantees, and only the second one is the feature.
    el.dataset.guide = guide.id;
    if (axis === "vertical") el.style.left = `${guide.at * scale.sx}px`;
    else el.style.top = `${guide.at * scale.sy}px`;
    el.hidden = false;
  }
  return next;
}

/**
 * Removes a gesture's guides. Called from every exit — commit, cancel and the
 * Escape path — because a guide that outlives its drag is a line across the page
 * explaining nothing.
 *
 * O(1).
 *
 * @param {GuideElements} state
 */
export function clearGuides(state) {
  if (!state) return;
  state.vertical?.remove();
  state.horizontal?.remove();
}

/**
 * Paints an object's resize grips from the engine's own handle geometry.
 *
 * The engine emits only the handles the object's carrier can honour — an inline
 * picture cannot move its character anchor, so it gets no north or west grips —
 * and this draws exactly what it was given rather than eight and hiding some.
 *
 * O(handles), a fixed 8 at most.
 *
 * @param {(pageNumber: number) => {overlay: Element, scale: {sx: number, sy: number}} | null} pageOf
 * @param {ArrayLike<number>} handles flat `[page, cx, cy, kind]` quads
 * @param {(event: PointerEvent, pageNumber: number, kind: number) => void} onGripDown
 */
export function paintResizeHandles(pageOf, handles, onGripDown) {
  for (let i = 0; i + 3 < handles.length; i += 4) {
    const [pageNumber, cx, cy, kind] = Array.prototype.slice.call(handles, i, i + 4);
    const target = pageOf(pageNumber);
    if (!target) continue;
    const el = document.createElement("div");
    el.className = "object-handle";
    el.dataset.handle = String(kind);
    el.style.left = `${cx * target.scale.sx}px`;
    el.style.top = `${cy * target.scale.sy}px`;
    el.addEventListener("pointerdown", (event) => onGripDown(event, pageNumber, kind));
    target.overlay.appendChild(el);
  }
}

/**
 * Where the object bar goes: just above the object, clamped into the scrolling
 * page view rather than the window — an object at the top of the view must not
 * push the bar up behind the ribbon — and off screen entirely when the object
 * it acts on has scrolled away. A bar left parked over unrelated paragraphs
 * with a live Delete button aimed at an object the user can no longer see is
 * `docs/104` HF-058, and it is the reason this is arithmetic that can be
 * checked rather than four `style.top` assignments in a repaint.
 *
 * Returns `null` when the bar should be hidden.
 *
 * O(1).
 *
 * @param {{left: number, top: number, bottom: number}} object viewport pixels
 * @param {{top: number, bottom: number}} view the scrolling page view's rect
 * @param {number} barHeight the bar's measured height
 * @param {number} [gap] the clearance in pixels
 * @returns {{left: number, top: number} | null}
 */
/** How far a resize grip's target reaches beyond the object, in pixels.
 *
 *  5px of centring margin plus `--grip-grow` (15px) in `style.css`. Kept beside
 *  the arithmetic that has to clear it; if the grip's target changes there, this
 *  moves with it and `object_guides.test.mjs` is what notices.
 */
export const GRIP_REACH = 20;

export function objectBarPosition(object, view, barHeight, gap = 8, gripReach = GRIP_REACH) {
  if (object.bottom <= view.top || object.top >= view.bottom) return null;
  // The bar must clear the GRIP's target, not the object's edge. A north-facing
  // grip is centred on the corner with a -5px margin and grows `--grip-grow`
  // (15px) upward to make the 24px WCAG 2.5.8 target, so its hit zone starts
  // 20px above the object — and an 8px gap put the bar squarely inside it. The
  // top-left corner of a selected picture could then not be grabbed at all,
  // because `elementFromPoint` answered with the bar.
  const clearance = Math.max(gap, gripReach);
  return {
    left: Math.max(gap, object.left),
    top: Math.round(
      Math.max(
        view.top + gap,
        Math.min(object.top - barHeight - clearance, view.bottom - barHeight - gap),
      ),
    ),
  };
}

/**
 * Puts a transparent move pad over a selected, movable object.
 *
 * The pad exists for ONE reason: `touch-action`. A page sheet is a canvas the
 * browser is free to scroll, and that decision is taken at touch-start from the
 * `touch-action` of the element under the finger — long before any handler can
 * call `preventDefault`. So dragging a floating image with a finger scrolled
 * the document and the image never moved, and no amount of pointer capture in
 * the move handler could change that, because the gesture was already gone.
 *
 * The resize grips do not have this problem: they are real elements carrying
 * `touch-action: none`. The pad gives the object's BODY the same footing, and
 * only while the object is selected and movable — so a finger still scrolls the
 * document everywhere else, including over the same picture when it is not
 * selected.
 *
 * It handles TOUCH ONLY, and is deliberately transparent to every other pointer
 * type: a mouse press falls through to the page's own hit-test, which is what
 * resolves a double-click into a group, a right-click into the context menu,
 * and a click on a picture inside a text box into the right object. Taking mouse
 * presses here broke four nested-object specs and three object-command ones,
 * because "start moving whatever is selected" is a much blunter answer than the
 * hit-test's. Touch has no such path to fall through to — the gesture is lost at
 * touch-start — which is the whole reason the pad exists.
 *
 * O(1).
 *
 * @param {Element} overlay the page overlay to paint into
 * @param {[number, number, number, number]} box the object's box in twips
 * @param {{sx: number, sy: number}} scale
 * @param {(event: PointerEvent) => void} onTouchDown
 */
export function paintMovePad(overlay, box, scale, onTouchDown) {
  const [x, y, w, h] = box;
  const pad = document.createElement("div");
  pad.className = "object-move-pad";
  pad.style.left = `${x * scale.sx}px`;
  pad.style.top = `${y * scale.sy}px`;
  pad.style.width = `${w * scale.sx}px`;
  pad.style.height = `${h * scale.sy}px`;
  pad.addEventListener("pointerdown", (event) => {
    if (event.pointerType === "touch") onTouchDown(event);
  });
  overlay.appendChild(pad);
  return pad;
}

/**
 * Puts (or updates) the live size bubble on a drag preview.
 *
 * The preview was an outline and nothing else, so resizing to a specific size
 * meant releasing, opening the properties panel to read what you got, and
 * correcting it there. Docs shows a W×H bubble during the drag and Word
 * live-updates its Size box; this is the same promise in the place the eye
 * already is.
 *
 * It is deliberately NOT a live region: it changes on every pointer move, and a
 * live region here would flood the screen-reader buffer. The committed size is
 * announced once, by the caller, on release.
 *
 * O(1).
 *
 * @param {Element} preview the drag preview to label
 * @param {string} label the already-localised size sentence
 * @returns {Element} the readout element, to cache on the drag record
 */
export function paintSizeReadout(preview, label) {
  let readout = preview.querySelector(":scope > .object-resize-readout");
  if (!readout) {
    readout = document.createElement("span");
    readout.className = "object-resize-readout";
    readout.setAttribute("aria-hidden", "true");
    preview.appendChild(readout);
  }
  readout.textContent = label;
  return readout;
}
