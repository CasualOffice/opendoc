// The direct-manipulation crop: the rectangle you keep, the region you cut, and
// the arithmetic that turns a dragged grip into a pair of crop fractions.
//
// Crop used to be a numeric dialog — the canonical example in this repository of
// building what the engine binding offered instead of what the user expects —
// and the chrome below is what replaced it. It lives beside `object_snap.mjs`
// rather than inside `main.js` for the reason `module_seams.test.mjs` states for
// `table_commands.mjs`: the clamping rule ("no crop may keep less than 8% of an
// axis, and no edge may cross its opposite") reached the application only
// through closures over editor state, so the only way to ask it a question was a
// browser and a picture. It is now a node question (`object_crop_chrome.test.mjs`).
//
// Cost: `cropFromDrag` is O(1). `paintCropChrome` builds a fixed 13 elements
// (outline + 4 dim strips + kept rect + readout + 8 grips) regardless of
// document size, which is the `docs/107` §4 budget a per-pointer-move repaint
// has to meet.
//
// ## What a crop session is
//
// Entering crop mode does not change the document. The image keeps its outline,
// the region that will be removed is dimmed, and eight grips on the kept
// rectangle are dragged to adjust it. Enter (or the bar's Apply, or clicking
// away) commits ONE `SetImageCrop`; Esc cancels with no change at all.
//
// `setImageCrop`'s insets are fractions of the SOURCE image, and the engine
// returns the crop the document was authored with — so entering crop mode is a
// refinement of the existing intent, not a blind replacement of it.

/** The smallest keep-fraction of the image per axis, so a crop can't collapse it. */
export const MIN_CROP_KEEP = 0.08;

/** Which edges each grip moves. Index is the engine handle kind, clockwise from
 *  NW: NW, N, NE, E, SE, S, SW, W. */
const MOVES = [
  { left: true, right: false, top: true, bottom: false }, // NW
  { left: false, right: false, top: true, bottom: false }, // N
  { left: false, right: true, top: true, bottom: false }, // NE
  { left: false, right: true, top: false, bottom: false }, // E
  { left: false, right: true, top: false, bottom: true }, // SE
  { left: false, right: false, top: false, bottom: true }, // S
  { left: true, right: false, top: false, bottom: true }, // SW
  { left: true, right: false, top: false, bottom: false }, // W
];

/** @typedef {{l: number, t: number, r: number, b: number}} Crop source-rectangle
 *  insets, each a fraction of the source image. */

/**
 * The crop a grip drag produces.
 *
 * Each moving edge is clamped twice: it may not pass the image bound (a
 * negative inset), and it may not come within `MIN_CROP_KEEP` of its opposite
 * edge. Both clamps are per-edge and independent, which is what lets a corner
 * drag run one edge into its limit while the other keeps moving — the
 * alternative, rejecting the whole sample, makes a corner grip stick.
 *
 * O(1).
 *
 * @param {Crop} startCrop the crop when the grip was pressed
 * @param {number} handleKind 0..7, clockwise from NW
 * @param {number} dxFraction pointer dx as a fraction of the placed box width
 * @param {number} dyFraction pointer dy as a fraction of the placed box height
 * @returns {Crop}
 */
export function cropFromDrag(startCrop, handleKind, dxFraction, dyFraction) {
  const moves = MOVES[handleKind];
  const crop = { ...startCrop };
  if (!moves) return crop;
  if (moves.left) {
    crop.l = Math.min(Math.max(0, startCrop.l + dxFraction), 1 - startCrop.r - MIN_CROP_KEEP);
  }
  if (moves.right) {
    crop.r = Math.min(Math.max(0, startCrop.r - dxFraction), 1 - startCrop.l - MIN_CROP_KEEP);
  }
  if (moves.top) {
    crop.t = Math.min(Math.max(0, startCrop.t + dyFraction), 1 - startCrop.b - MIN_CROP_KEEP);
  }
  if (moves.bottom) {
    crop.b = Math.min(Math.max(0, startCrop.b - dyFraction), 1 - startCrop.t - MIN_CROP_KEEP);
  }
  return crop;
}

/**
 * Wires one grip drag to the window, so the gesture survives the pointer leaving
 * the grip — and calls `onEnd` exactly once, however it ends.
 *
 * The listeners go on `window` rather than the grip because a 9px target that
 * only tracks while the pointer is inside it is a grip you lose halfway through
 * a drag.
 *
 * O(1).
 *
 * @param {PointerEvent} event the pointerdown on the grip
 * @param {{onMove: (event: PointerEvent) => void, onEnd: () => void}} io
 */
export function beginGripDrag(event, io) {
  event.preventDefault();
  event.stopPropagation();
  const move = (e) => io.onMove(e);
  const up = (e) => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    io.onEnd();
    e.preventDefault();
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
}

/**
 * The kept rectangle, in the same page-local twips the placed box is in.
 *
 * O(1).
 *
 * @param {[number, number, number, number]} box the placed box, `[x, y, w, h]`
 * @param {Crop} crop
 * @returns {{x: number, y: number, w: number, h: number}}
 */
export function keptRect(box, crop) {
  const [bx, by, bw, bh] = box;
  return {
    x: bx + crop.l * bw,
    y: by + crop.t * bh,
    w: bw * (1 - crop.l - crop.r),
    h: bh * (1 - crop.t - crop.b),
  };
}

/** Centre points of the eight grips on a kept rectangle, clockwise from NW. */
function gripPoints(kept) {
  const { x, y, w, h } = kept;
  return [
    [x, y], [x + w / 2, y], [x + w, y], [x + w, y + h / 2],
    [x + w, y + h], [x + w / 2, y + h], [x, y + h], [x, y + h / 2],
  ];
}

/**
 * Paints the crop chrome into `overlay`: four dim strips over the region being
 * cut, a border on the region being kept, its live size, and eight grips.
 *
 * The outline itself is the caller's, because it is the same outline a
 * non-cropping selection draws and it comes from the engine's own geometry.
 *
 * O(1) — a fixed 13 elements.
 *
 * @param {Element} overlay the page overlay to paint into
 * @param {[number, number, number, number]} box the placed box in twips
 * @param {Crop} crop
 * @param {{sx: number, sy: number}} scale twips-to-CSS-pixel factors
 * @param {{sizeLabel: string, onGripDown: (event: PointerEvent, kind: number) => void}} io
 */
export function paintCropChrome(overlay, box, crop, scale, io) {
  const [bx, by, bw, bh] = box;
  const kept = keptRect(box, crop);
  const put = (className, x, y, w, h) => {
    if (w <= 0 || h <= 0) return null;
    const el = document.createElement("div");
    el.className = className;
    el.style.left = `${x * scale.sx}px`;
    el.style.top = `${y * scale.sy}px`;
    el.style.width = `${w * scale.sx}px`;
    el.style.height = `${h * scale.sy}px`;
    overlay.appendChild(el);
    return el;
  };
  put("object-crop-dim", bx, by, bw, kept.y - by); // top
  put("object-crop-dim", bx, kept.y + kept.h, bw, by + bh - (kept.y + kept.h)); // bottom
  put("object-crop-dim", bx, kept.y, kept.x - bx, kept.h); // left
  put("object-crop-dim", kept.x + kept.w, kept.y, bx + bw - (kept.x + kept.w), kept.h); // right
  const rect = put("object-crop-rect", kept.x, kept.y, kept.w, kept.h);
  if (rect && io.sizeLabel) {
    // Say what size you are KEEPING, the way a resize drag says what size you
    // are dragging to. Without it a crop was the one geometry gesture with no
    // number attached. Not a live region: it changes on every pointer move.
    const readout = document.createElement("span");
    readout.className = "object-resize-readout";
    readout.setAttribute("aria-hidden", "true");
    readout.textContent = io.sizeLabel;
    rect.appendChild(readout);
  }
  gripPoints(kept).forEach(([cx, cy], kind) => {
    const el = document.createElement("div");
    el.className = "object-crop-handle";
    el.dataset.handle = String(kind);
    el.style.left = `${cx * scale.sx}px`;
    el.style.top = `${cy * scale.sy}px`;
    el.addEventListener("pointerdown", (event) => io.onGripDown(event, kind));
    overlay.appendChild(el);
  });
}
