// Reflow (pageless) — the shell half of ADR-046 and `docs/151` §6.
//
// The engine half is one setter: `doc.setLayoutView(contentWidthTwip,
// tileHeightTwip, gutterTwip)`. It lays the body out at the reader's width
// instead of on the document's paper and cuts the result into fixed-height
// tiles, so `renderPage`, `hitTest`, the caret and every overlay keep working
// and the document stays EDITABLE — which is where §3.2 diverges from
// ONLYOFFICE's read-only reader mode on purpose. What the shell owes is: a
// command with more than one face, the WIDTH, drawn tiles with no seam, the
// chrome that has to stand down, and print forced back onto paper.
//
// WHY THIS IS A MODULE AND NOT TEN LINES IN `main.js`. Two reasons, and the
// first is not the line ratchet. The interesting part of this feature is a pair
// of decisions about NUMBERS — which widths count as the same width, and how
// long to wait before believing one — and those decide whether a resize is O(1)
// or O(document) in the size of the reader's file. A decision that expensive
// should be answerable in `node` against a fake clock, not only in a browser
// against a stopwatch. The second reason is the ratchet (`module_seams`), and it
// is the smaller one.
//
// ---- THE COST MODEL, because it is the whole design -------------------------
//
// Entering, leaving, or CHANGING THE WIDTH OF reflow is O(document): the galley
// cache is scoped to the width it was shaped at (`flow.rs`'s
// `build_galley_cached`), so a different width keeps nothing. `docs/107` §4 is
// an owner constraint — per-interaction work is O(1) in document size — and a
// resize is an interaction. A drag fires `resize` per animation frame; a phone
// rotating, a soft keyboard opening and a URL bar retracting each fire a burst.
// So a width feed that passed every pixel straight through would be an
// O(document) re-shape sixty times a second on the slowest device we support.
//
// Two mechanisms answer that, and they are different in kind:
//
//   QUANTISATION decides which widths are the SAME width. It is what makes the
//   common case cost nothing at all: a drag that stays inside one bucket does
//   not re-shape once, however many events it fires, because the shell compares
//   the bucketed width with the one the engine already holds and finds them
//   equal. This is the O(1) guarantee.
//
//   THE DEBOUNCE bounds the WORST case — a drag that crosses many buckets. It
//   coalesces a gesture into its last value. Without quantisation it would be
//   the only defence and a slow drag would still re-shape at every step; with
//   quantisation it is the second line rather than the first.
//
// ---- THE TWO NUMBERS, and why these ----------------------------------------
//
// `docs/151` §6.2 proposed "the nearest 8px". This ships 16px, FLOORED, and
// both halves of that are corrections rather than preferences:
//
//   * FLOORED, not nearest. Rounding to the NEAREST bucket rounds UP half the
//     time, which makes the column wider than the space it was measured
//     against — and a column wider than the viewport is a horizontal scroll on
//     `#viewport`, the exact thing this feature exists to retire (`docs/148`
//     §8). A rounding rule that can reintroduce the defect it is part of the
//     fix for is the wrong rounding rule.
//
//   * 16, not 8. Because it is floored, the quantum IS the safety margin: the
//     column is between 0 and one quantum narrower than the space available.
//     That margin has to absorb a scrollbar that appears mid-gesture (15px on
//     Windows, 17px on a desktop GTK theme), sub-pixel rects from a fractional
//     device pixel ratio, and the twip rounding below. 8px does not cover a
//     scrollbar; 16px does. It costs at most 16px of text width, which is under
//     two characters at 11pt, and it halves the number of distinct widths a
//     390 -> 1280px drag can produce (56 rather than 111).
//
// `REFLOW_DEBOUNCE_MS = 150` is a trailing debounce with no leading call. It is
// under the ~200ms at which an interface stops feeling attached to the gesture,
// and above the frame budget by an order of magnitude, so a burst of `resize`
// events collapses into one pass. There is no leading call BECAUSE the first
// event of a drag is the least likely to be the width the reader wants: acting
// on it guarantees at least two O(document) passes for one gesture.

import { TWIPS_PER_INCH } from "./units.mjs";

/** The width bucket, in CSS px. See the header for why 16 and why floored. */
export const REFLOW_QUANTUM_PX = 16;

/** Trailing debounce for the width feed, in ms. See the header. */
export const REFLOW_DEBOUNCE_MS = 150;

/** Breathing room between the text column and the edge of the window, per side,
 *  in CSS px. It is passed to the engine as the synthetic page's left and right
 *  margin, so the text never runs to the glass and a tile is still a box with a
 *  content area — which is what keeps indents, float wrap and table widths
 *  resolving against a measure rather than against the window. */
export const REFLOW_GUTTER_PX = 16;

/** The engine's own floor on a reading column: `LayoutView::reflow` refuses
 *  anything under an inch, at the seam, because an unconverted CSS pixel value
 *  (390 twips is a quarter of an inch) should fail where it can be read rather
 *  than inside a layout nobody can see.
 *
 *  It matters here because the twip width is a function of the ZOOM as well as
 *  the window: at 400% a 390px window is 1,404 twips of text, under the floor.
 *  The column then stops shrinking, and the tile is wider than the window — the
 *  same arbitration §6.3 gives a table too wide to fit, and honest for the same
 *  reason: what the reader is being shown is genuinely wider than their screen.
 *  The no-horizontal-scroll guarantee is stated at the zoom a phone actually
 *  opens at, which `FIT_ON_OPEN_FLOOR` pins at 100%. */
export const REFLOW_MIN_CONTENT_TWIP = TWIPS_PER_INCH;

/** Where the per-viewer preference lives (ADR-046: a viewer's choice, never a
 *  document property — one person's phone must not reformat another person's
 *  monitor). `null` from `readPref` means "never chosen", which is what lets the
 *  phone rung supply the default without overriding anybody. */
export const REFLOW_PREF_KEY = "docReflow";

/** The bucket `px` falls in: the largest multiple of `quantum` that is not
 *  wider than it. Floored, never nearest — see the header.
 *
 *  Complexity: O(1).
 *
 * @param {number} px
 * @param {number} [quantum]
 * @returns {number} 0 for a width that is not a usable positive number.
 */
export function quantiseReflowWidth(px, quantum = REFLOW_QUANTUM_PX) {
  if (!Number.isFinite(px) || px <= 0 || !(quantum > 0)) return 0;
  return Math.floor(px / quantum) * quantum;
}

/**
 * The engine's three arguments, from the space the shell actually has.
 *
 * The tile's TOTAL width — `content + 2 * gutter` in the engine's synthetic
 * page — is what gets painted, so it is the quantised bucket that has to be the
 * total, not the content. Getting that the other way round is how a gutter
 * becomes an overflow.
 *
 * Complexity: O(1). Called once per render and once per settled resize.
 *
 * @param {number} clientWidthPx the scroller's content box, excluding its own
 *        scrollbar — `clientWidth`, not `getBoundingClientRect().width`.
 * @param {number} cssPerTwip CSS px per twip at the current zoom, exactly as
 *        `renderAll` computes it. The zoom belongs in here: at 150% the reader
 *        wants bigger text in the same window, which is fewer twips of measure,
 *        and that falls out of this conversion rather than needing a rule.
 * @param {{quantum?:number, gutterPx?:number, minContentTwip?:number}} [options]
 * @returns {{totalPx:number, contentWidthTwip:number, gutterTwip:number}|null}
 *          `null` when there is no usable width yet (a viewport that has not
 *          been laid out reports 0), so the caller leaves the view alone rather
 *          than asking the engine for a column of nothing.
 */
export function reflowMeasure(clientWidthPx, cssPerTwip, options = {}) {
  const quantum = options.quantum ?? REFLOW_QUANTUM_PX;
  const gutterPx = options.gutterPx ?? REFLOW_GUTTER_PX;
  const minContentTwip = options.minContentTwip ?? REFLOW_MIN_CONTENT_TWIP;
  const totalPx = quantiseReflowWidth(clientWidthPx, quantum);
  if (totalPx <= 0 || !(cssPerTwip > 0)) return null;
  const gutterTwip = Math.max(0, Math.round(gutterPx / cssPerTwip));
  // Floored, so the two gutters and the column can never add up to more than
  // the bucket after rounding — a rounded-up content width is a one-twip
  // overflow, which is invisible until it is a scrollbar.
  const contentWidthTwip = Math.floor(totalPx / cssPerTwip) - 2 * gutterTwip;
  if (contentWidthTwip < minContentTwip) {
    // Below the engine's floor the column stops shrinking rather than being
    // refused: see REFLOW_MIN_CONTENT_TWIP. The gutter goes with it, because a
    // gutter wider than the column it pads is the other thing the seam refuses.
    return { totalPx, contentWidthTwip: minContentTwip, gutterTwip: 0 };
  }
  return { totalPx, contentWidthTwip, gutterTwip };
}

/**
 * Whether reflow can be entered at all, and the sentence to say when it cannot.
 *
 * ONE condition today, and it is the engine's, not a guess: `setLayoutView`
 * refuses a body laid out one page-window at a time, because reflow's defining
 * promise is that the document stays editable in it and a windowed body is
 * already read-only. The shell cannot ask the engine that question without
 * calling the setter — there is no `reflowUnavailableReason` getter — but
 * `editingUnavailableReason` is non-empty under EXACTLY the same predicate
 * (`self.layout.is_windowed()` in both), so it is a faithful read rather than a
 * shadow copy. If the two ever diverge, the call site still catches the throw
 * and shows the engine's own words, so the failure mode is a late honest
 * message rather than a dead control.
 *
 * Complexity: O(1).
 *
 * @param {string} editingUnavailableReason the engine getter, "" when editable.
 * @param {string} withheldSentence the localised reason to show when it is not.
 * @returns {{available: boolean, reason: string}}
 */
export function reflowAvailability(editingUnavailableReason, withheldSentence) {
  const windowed = String(editingUnavailableReason ?? "") !== "";
  return { available: !windowed, reason: windowed ? withheldSentence : "" };
}

/**
 * The width feed: quantise, then debounce, then only if the bucket moved.
 *
 * The order is the design. Quantising FIRST is what makes the common case free;
 * debouncing first would still wake up once per gesture to discover nothing
 * changed, and would put a 150ms lag on the answer "nothing to do".
 *
 * Injected timers rather than `setTimeout` directly, so the coalescing is
 * answerable in `node` against a fake clock — the behaviour this module exists
 * to make cheap is precisely the behaviour a browser test cannot assert without
 * waiting on a wall clock, which `SKILL.md` §6 names as its own failure mode.
 *
 * @param {{
 *   onSettled: (bucketPx: number) => void,
 *   debounceMs?: number,
 *   quantum?: number,
 *   setTimer?: (fn: () => void, ms: number) => any,
 *   clearTimer?: (handle: any) => void,
 * }} deps
 * @returns {{observe: (px:number) => boolean, adopt: (px:number) => void,
 *            cancel: () => void, current: () => number, pending: () => boolean}}
 */
export function createWidthFeed({
  onSettled,
  debounceMs = REFLOW_DEBOUNCE_MS,
  quantum = REFLOW_QUANTUM_PX,
  setTimer = (fn, ms) => setTimeout(fn, ms),
  clearTimer = (handle) => clearTimeout(handle),
}) {
  /** The bucket the ENGINE is currently laid out at. -1 until the first pass. */
  let applied = -1;
  let timer = null;

  function cancel() {
    if (timer !== null) clearTimer(timer);
    timer = null;
  }

  return {
    /** Record that the engine now holds `px`. Called by the render pass, which
     *  is the only thing that actually talks to the setter. */
    adopt(px) {
      cancel();
      applied = quantiseReflowWidth(px, quantum);
    },
    /** Feed a live width. Returns true when a settled pass has been scheduled —
     *  which is exactly "this event was NOT free". */
    observe(px) {
      const bucket = quantiseReflowWidth(px, quantum);
      if (bucket === applied) {
        // The whole point: a resize inside the bucket the engine already holds
        // costs one division and one comparison, and nothing in the document is
        // touched. A pass scheduled by an earlier width is CANCELLED rather than
        // left standing — a reader who drags out and back has changed nothing,
        // and a timer still holding the furthest width would relayout to a size
        // the window no longer is. The LAST width wins, including when the last
        // width is the one already in effect.
        cancel();
        return false;
      }
      cancel();
      timer = setTimer(() => {
        timer = null;
        onSettled(bucket);
      }, debounceMs);
      return true;
    },
    cancel,
    current: () => applied,
    pending: () => timer !== null,
  };
}
