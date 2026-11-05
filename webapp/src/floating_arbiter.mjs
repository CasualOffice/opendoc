// THE FLOATING-CHROME ARBITER (docs/184 D-12, audit CANVAS-06, CRITIC-10).
//
// Five surfaces float over the page next to what the reader is working on:
// the link card, the object bar, the table insert markers, the selection
// mini-toolbar and the paste-options chip. Each was positioned by its own
// module with no knowledge of the others, so a single click on a link showed
// the link card AND the mini-toolbar, the card covering more than half of the
// toolbar (measured: chip 404x88 over a 254x40 toolbar), and the paste chip sat
// on top of the second pasted paragraph.
//
// The rule, Word's and Docs' (one contextual surface at a time):
//
//   1. PRIORITY. When several want to show, only the highest shows:
//      link card > object bar > table insert markers > mini-toolbar > paste chip.
//      A suppressed surface is not closed: its owner still holds it open, and
//      it comes back the moment the one above it goes away.
//   2. NEVER OVER THE SELECTION. The surface that shows is moved off the
//      selection's rectangle (below it, else above it), inside the viewport.
//
// It owns no surface. Each owner keeps showing and hiding its own element
// (`hidden`) and placing it; the arbiter watches those elements (one
// MutationObserver) and settles after every change, in the same task, before
// paint. That is the choke point: a new code path that shows a surface cannot
// forget to ask, because it is never asked.
//
// Complexity: O(surfaces) per settle, five elements and one rectangle union of
// the painted selection highlights (the visible lines only); no document call.
// A settle runs only when a surface is shown, hidden or re-placed (its `hidden`,
// `class` or `style` changes). The mini-toolbar's pointer fade is therefore NOT
// written to its own `style`: it is a custom property on the root, so a
// pointermove never queues a settle.

/** Higher wins. The order is decision D-12's. */
export const FLOATING_PRIORITY = Object.freeze({
  linkChip: 50,
  objectBar: 40,
  tableInsert: 30,
  selectionToolbar: 20,
  pasteOptions: 10,
});

/** The attribute a suppressed surface carries; the stylesheet hides it. */
export const SUPPRESSED_ATTRIBUTE = "data-float-suppressed";

/** Whether two client rectangles overlap (touching edges do not). */
export function rectsOverlap(a, b) {
  return a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;
}

/**
 * Pure decision: of the surfaces that WANT to show, which one shows.
 * `wanting` is a list of ids; returns the winner id or null.
 */
export function arbitrate(wanting, priority = FLOATING_PRIORITY) {
  let winner = null;
  for (const id of wanting) {
    if (winner === null || (priority[id] ?? 0) > (priority[winner] ?? 0)) winner = id;
  }
  return winner;
}

/**
 * Pure placement: where a `size` surface currently at `at` goes so that it does
 * not cover `avoid` (the selection), inside `bounds` (the viewport less an
 * inset). Returns `at` unchanged when there is no overlap. Prefers just below
 * the selection, then just above it; when neither fits, the side with more room.
 */
export function placeClear(at, size, avoid, bounds, gap = 8) {
  const box = { left: at.left, top: at.top, right: at.left + size.width, bottom: at.top + size.height };
  if (!avoid || !rectsOverlap(box, avoid)) return at;
  const below = avoid.bottom + gap;
  const above = avoid.top - gap - size.height;
  // A selection taller than the window leaves no clear spot; then the owner's
  // own placement stands (it already keeps the surface in view).
  let top;
  if (below + size.height <= bounds.bottom) top = below;
  else if (above >= bounds.top) top = above;
  else return at;
  const left = Math.max(bounds.left, Math.min(at.left, bounds.right - size.width));
  return { left, top };
}

/**
 * Installs the arbiter. `surfaces` maps an id from FLOATING_PRIORITY to a
 * getter for its element (some are created lazily, the object bar among them);
 * `selectionRect()` answers the client rectangle the surfaces must not cover,
 * or null.
 */
export function createFloatingArbiter({ surfaces, selectionRect, viewportRect, priority = FLOATING_PRIORITY }) {
  const observed = new WeakSet();
  let scheduled = false;
  let settling = false;

  const observer = new MutationObserver(() => {
    if (!settling) schedule();
  });

  /** Starts watching any surface element that exists now and was not watched. */
  function watch() {
    for (const get of Object.values(surfaces)) {
      const el = get();
      if (!el || observed.has(el)) continue;
      observed.add(el);
      observer.observe(el, { attributes: true, attributeFilter: ["hidden", "style", "class"] });
    }
  }

  function schedule() {
    if (scheduled) return;
    scheduled = true;
    queueMicrotask(() => {
      scheduled = false;
      settle();
    });
  }

  /** The surfaces whose owner holds them open, by id. */
  function wanting() {
    const out = [];
    for (const [id, get] of Object.entries(surfaces)) {
      const el = get();
      if (el && el.isConnected && !el.hidden) out.push(id);
    }
    return out;
  }

  /** Applies the decision. Runs synchronously; safe to call any time. */
  function settle() {
    watch();
    settling = true;
    let winner = null;
    try {
      const want = wanting();
      winner = arbitrate(want, priority);
      for (const [id, get] of Object.entries(surfaces)) {
        const el = get();
        if (!el) continue;
        if (id === winner || !want.includes(id)) el.removeAttribute(SUPPRESSED_ATTRIBUTE);
        else el.setAttribute(SUPPRESSED_ATTRIBUTE, "true");
      }
      if (winner) keepOffSelection(surfaces[winner]());
    } finally {
      // Our own writes queued records; drop them so settling does not loop.
      observer.takeRecords();
      settling = false;
    }
    return winner;
  }

  function keepOffSelection(el) {
    if (!el || !movable(el)) return;
    const avoid = selectionRect();
    if (!avoid) return;
    const rect = el.getBoundingClientRect();
    if (rect.width === 0 && rect.height === 0) return;
    const view = viewportRect?.() ?? { left: 0, top: 0, right: window.innerWidth, bottom: window.innerHeight };
    const bounds = { left: view.left + 8, top: view.top + 8, right: view.right - 8, bottom: view.bottom - 8 };
    const next = placeClear({ left: rect.left, top: rect.top }, rect, avoid, bounds);
    if (next.left === rect.left && next.top === rect.top) return;
    el.style.left = `${Math.round(next.left)}px`;
    el.style.top = `${Math.round(next.top)}px`;
  }

  /** Only a `position: fixed` surface can be moved: its left/top ARE client
   *  coordinates, the space `placeClear` answers in. The table insert disc is
   *  absolute inside a page overlay (offset by the page, scaled by the zoom),
   *  and it is also its own hit box (HF-341: `gutterAt` answers for the
   *  painted disc), so moving it would separate what is painted from what is
   *  clicked. Such a surface is only ever suppressed or shown where its owner
   *  put it. */
  function movable(el) {
    return (el.ownerDocument?.defaultView ?? window).getComputedStyle(el).position === "fixed";
  }

  watch();
  return { settle, schedule, watch, winner: () => arbitrate(wanting(), priority) };
}

/** The one arbiter the editor installed, so a repaint path that cannot reach
 *  the editor context (the overlay painter, the table gutter) can still ask it
 *  to settle. A module slot rather than a property on `ed`, which `main.js`
 *  defines. */
let installed = null;

export function installFloatingArbiter(arbiter) {
  installed = arbiter;
  return arbiter;
}

/** Something that may change which surface shows was repainted: settle before
 *  paint. O(1) here; the settle is one microtask later, once per task. */
export function noteFloatingChange() {
  installed?.schedule();
}

/**
 * The mini-toolbar's opacity for a pointer at (x, y): 1 within 24px of the
 * bar, fading to 0 by 160px, as Word's mini toolbar fades when the pointer
 * moves away from it (audit CANVAS-06). Pure, for its test.
 */
export function miniToolbarOpacity(rect, x, y, near = 24, far = 160) {
  const dx = Math.max(rect.left - x, 0, x - rect.right);
  const dy = Math.max(rect.top - y, 0, y - rect.bottom);
  const distance = Math.hypot(dx, dy);
  if (distance <= near) return 1;
  if (distance >= far) return 0;
  return Math.round((1 - (distance - near) / (far - near)) * 100) / 100;
}

/** The client rectangle of the painted selection: the union of the visible
 *  highlight rectangles, else the caret. Reads the overlay the editor already
 *  painted, so it costs no engine call. */
export function paintedSelectionRect(root) {
  const marks = root.querySelectorAll(".overlay .highlight, .overlay .table-cell-selection");
  const parts = marks.length ? marks : root.querySelectorAll(".overlay .caret");
  let box = null;
  for (const el of parts) {
    const r = el.getBoundingClientRect();
    if (r.width === 0 && r.height === 0) continue;
    box = box
      ? {
          left: Math.min(box.left, r.left),
          top: Math.min(box.top, r.top),
          right: Math.max(box.right, r.right),
          bottom: Math.max(box.bottom, r.bottom),
        }
      : { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
  }
  return box;
}
