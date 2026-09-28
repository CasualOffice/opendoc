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
