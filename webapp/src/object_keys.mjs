// What the arrow keys mean while an object is selected, and while it is being
// cropped.
//
// Two decisions that used to be inline in `main.js`'s keydown listener, and
// both were wrong in a way a user meets in the first minute:
//
//   * Word nudges a selected object with the arrows, takes a bigger step with
//     nothing held but a smaller one with Ctrl (Windows) or Option (Mac) —
//     "nudge by a pixel". The listener nudged only with NO modifier, so
//     Ctrl+Arrow fell through to word-by-word caret navigation, which dropped
//     the object and moved a caret the reader could not see.
//   * In crop mode the arrows reached the same nudge, so pressing one to adjust
//     the crop MOVED THE PICTURE out from under the crop chrome, which stayed
//     painted where the picture had been (`docs/104` HF-106). The crop session
//     now owns the arrows: they move the kept area over the image, Shift for a
//     bigger step — the keyboard half of dragging the picture inside Word's crop
//     frame.
//
// Pure: nothing here touches the DOM or the engine, so the whole keyboard
// grammar is a node test (`tests/object_keys.test.mjs`).

/** One arrow press, as a unit direction. */
const ARROWS = {
  ArrowLeft: [-1, 0],
  ArrowRight: [1, 0],
  ArrowUp: [0, -1],
  ArrowDown: [0, 1],
};

/** The nudge steps, in page twips. The plain step is ~1/32in (Docs' and Word's
 *  default feel); Shift is ~1/8in; the fine step is one CSS pixel at 100%
 *  (1440 twip/in ÷ 96 px/in), which is what Word's Ctrl+Arrow means. */
export const NUDGE_TWIP = 45;
export const NUDGE_TWIP_LARGE = 180;
export const NUDGE_TWIP_FINE = 15;

/**
 * The nudge a keydown asks for on a selected object, or `null` when the key is
 * not a nudge.
 *
 * Ctrl or Alt/Option takes the FINE step and Shift the large one. Cmd (Meta) is
 * not a nudge: on a Mac it is the line-start/line-end chord, and taking it would
 * break caret movement for anyone whose selection merely touches an object.
 *
 * O(1).
 *
 * @param {{key: string, shiftKey?: boolean, ctrlKey?: boolean, altKey?: boolean, metaKey?: boolean}} event
 * @returns {{dx: number, dy: number, step: number} | null}
 */
export function objectNudge(event) {
  const direction = ARROWS[event.key];
  if (!direction || event.metaKey) return null;
  const step = event.ctrlKey || event.altKey
    ? NUDGE_TWIP_FINE
    : event.shiftKey
      ? NUDGE_TWIP_LARGE
      : NUDGE_TWIP;
  return { dx: direction[0], dy: direction[1], step };
}

/** How far one arrow press moves the kept area in crop mode, as a fraction of
 *  the picture. */
export const CROP_PAN = 0.01;
export const CROP_PAN_LARGE = 0.05;

/**
 * The crop after one arrow press: the kept area moved over the image, keeping
 * its size, never past an edge of the picture.
 *
 * `moved` is false when there is nothing cropped on that side to move into —
 * the case a reader needs to be TOLD about, because otherwise the key does
 * nothing and says nothing.
 *
 * O(1).
 *
 * @param {{l: number, t: number, r: number, b: number}} crop the four insets, 0..1
 * @param {string} key the `KeyboardEvent.key`
 * @param {boolean} large whether Shift is held
 * @returns {{crop: {l: number, t: number, r: number, b: number}, moved: boolean} | null}
 *          `null` when `key` is not an arrow
 */
export function panCrop(crop, key, large) {
  const direction = ARROWS[key];
  if (!direction) return null;
  const step = large ? CROP_PAN_LARGE : CROP_PAN;
  const next = { ...crop };
  // Moving the kept area toward an edge spends the inset on that edge and gives
  // it to the opposite one, so the kept size is exactly preserved.
  const [dx, dy] = direction;
  if (dx !== 0) {
    const room = dx > 0 ? crop.r : crop.l;
    const d = Math.min(step, Math.max(0, room)) * dx;
    next.l = crop.l + d;
    next.r = crop.r - d;
  } else {
    const room = dy > 0 ? crop.b : crop.t;
    const d = Math.min(step, Math.max(0, room)) * dy;
    next.t = crop.t + d;
    next.b = crop.b - d;
  }
  const moved = next.l !== crop.l || next.t !== crop.t;
  return { crop: next, moved };
}

/**
 * The crop session's keyboard: Enter applies, Escape cancels, the arrows move
 * the kept area. Returns whether the key was the session's — a key it does not
 * own falls through to the editor, but an arrow NEVER does, because falling
 * through is how it used to reach the object nudge.
 *
 * O(1).
 *
 * @param {KeyboardEvent} event
 * @param {{session: {crop: object}, commit: () => void, cancel: () => void,
 *          redraw: () => void, nothingToMove: () => void}} io
 * @returns {boolean}
 */
export function handleCropKey(event, io) {
  if (event.key === "Enter") {
    event.preventDefault();
    io.commit();
    return true;
  }
  if (event.key === "Escape") {
    event.preventDefault();
    io.cancel();
    return true;
  }
  // Every arrow is the session's, whatever is held with it: a Ctrl+Arrow let
  // through here would reach the object nudge and move the picture under the
  // crop, which is the defect this exists to close.
  const pan = panCrop(io.session.crop, event.key, event.shiftKey);
  if (!pan) return false;
  event.preventDefault();
  if (pan.moved) {
    io.session.crop = pan.crop;
    io.redraw();
  } else {
    io.nothingToMove();
  }
  return true;
}
