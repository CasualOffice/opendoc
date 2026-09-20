// Caret navigation the host owns: the vertical goal column, the rescue for a
// vertical move the engine could not make, and the position comparisons the
// two share with the rest of the editor.
//
// Pure — every piece of geometry and model access is injected — so all of it
// is unit-testable without a browser or an engine.

/** The directions that are VERTICAL moves — the ones that keep the column.
 *  Page Up/Down move by a viewport rather than a line, and Word and Docs keep
 *  the column across them too. */
const VERTICAL = new Set(["up", "down", "pageUp", "pageDown"]);

/**
 * The column a run of vertical caret moves is aiming at, in page-local twips
 * (`caretRect`'s own units).
 *
 * The rule, which is Word's and Google Docs':
 *
 *   * a vertical move SETS the column when there is not one already and
 *     otherwise KEEPS it — including when the line it lands on is too short to
 *     offer it. That is the whole point: the short line clamps the caret, the
 *     column is untouched, and the next long line is entered at the column.
 *   * anything else clears it — a horizontal move (arrow, Home/End, a word
 *     jump), a click, an edit.
 *   * extending with Shift is still a vertical move: the focus travels and the
 *     column persists.
 *
 * Measured before this existed (HF-164): a caret at x = 1456 on a long line,
 * ArrowDown onto a short line (392), then an empty paragraph (346) — and every
 * line after that, long ones included, was entered at 346, in both directions,
 * for the rest of the document.
 *
 * The engine cannot hold this. `moveCaret` is handed a model position, and
 * after the first clamp that position IS the short line's end, so the column
 * the run started at is not recoverable from it. It is state about the RUN of
 * moves, and only the host sees the gestures that end one.
 *
 * Belt and braces: the column is remembered together with the position the
 * last vertical move landed on, and is offered only to a caret that is still
 * there. A selection set anywhere else in the app — find, a command, a
 * restored draft, a caller nobody has written yet — therefore cannot inherit a
 * stale column whether or not it remembered to clear one. The explicit clears
 * still matter for the gestures that leave the caret exactly where it was (a
 * forward Delete, a click on the caret itself), which Word treats as a clear.
 */
export function createVerticalGoal() {
  let goal = null;
  return {
    /** The column `dir` should aim at from `focus`, or null when `dir` is not a
     *  vertical move — which the caller passes straight to `moveCaret`, where
     *  it reads as "aim at the caret's own x", and which also clears the column.
     *
     *  `measure` is called only when the column has to be established, so a
     *  left/right press costs no geometry. */
    columnFor(dir, focus, measure) {
      if (!VERTICAL.has(dir)) return null;
      const carried = goal !== null && goal.node === focus.node && goal.offset === focus.offset;
      return carried ? goal.x : (measure() ?? null);
    },
    /** Remembers `x` as the column, valid while the caret stays at `landed`. A
     *  null `x` — every non-vertical move — clears it. */
    keep(x, landed) {
      goal = x === null || x === undefined ? null : { x, node: landed.node, offset: landed.offset };
    },
    /** Drops the column: a click, or an edit. */
    clear() {
      goal = null;
    },
  };
}

// ---- Positions ---------------------------------------------------------------

/** True when two model positions are the same place in the document. */
export function sameModelPosition(left, right) {
  return left.node === right.node && left.offset === right.offset;
}

/** True when `current` covers exactly `start`..`end`, in either direction. */
export function selectionMatchesRange(current, start, end) {
  return (
    (sameModelPosition(current.anchor, start) && sameModelPosition(current.focus, end)) ||
    (sameModelPosition(current.anchor, end) && sameModelPosition(current.focus, start))
  );
}

/** A selection's two endpoints in document order.
 *
 *  Ordering across paragraphs is the ENGINE's answer, not the offsets':
 *  offsets are per-paragraph, so a backwards selection across paragraphs
 *  cannot be ordered by comparing them. */
export function orderedSelectionEnds(selection, engine) {
  const { anchor, focus } = selection;
  if (anchor.node === focus.node) {
    const forward = anchor.offset <= focus.offset;
    return forward ? [{ ...anchor }, { ...focus }] : [{ ...focus }, { ...anchor }];
  }
  const s = engine.selectionEdge(anchor.node, anchor.offset, focus.node, focus.offset, false);
  const e = engine.selectionEdge(anchor.node, anchor.offset, focus.node, focus.offset, true);
  const ends = [
    { node: s.node, offset: s.offset },
    { node: e.node, offset: e.offset },
  ];
  s.free();
  e.free();
  return ends;
}

// ---- Rescuing a vertical move the engine could not make -----------------------
//
// `move_vertical` is geometric now (#556) — it ranks every line beyond the
// caret's and accepts a candidate only after re-resolving it — but going UP out
// of a table it can still answer with the position it was given, and then the
// key does nothing visible. Measured on the owner's loan agreement: after
// Ctrl+End, sixty ArrowUps moved the caret zero pixels. It still reproduces on
// `pagination-fidelity.docx`, which is why this is still here: removing it
// turned `vertical-navigation-dead-ends.spec.mjs` red on the document that ends
// in a table.
//
// It is deliberately a FALLBACK — it runs only when the engine reported no
// movement at all — and it finds a LINE, not a column: the caller puts the
// rescued position back on the run's goal column afterwards.

/** How far to probe, in multiples of the caret's own height.
 *
 *  Dense near the caret and sparse further out, because the NEAREST accepted
 *  candidate wins: the close steps keep "up" meaning the line immediately
 *  above, and the long tail is what carries the probe over a cell border, a
 *  page's bottom margin and the gap between two sheets — at the foot of a page
 *  the next line up is a whole margin away, which a short ladder never
 *  reaches. */
export const VERTICAL_PROBE_STEPS = [0.75, 1.25, 1.75, 2.5, 3.5, 5, 7, 10, 14, 20];

/** Client-space points to hit-test, nearest first. */
export function verticalProbePoints(rect, direction, steps = VERTICAL_PROBE_STEPS) {
  const height = Math.max(8, rect.bottom - rect.top);
  return steps.map((step) => ({
    x: rect.left,
    y: direction === "up" ? rect.top - height * step : rect.bottom + height * step,
  }));
}

/** True when `target` really is in `direction` from `origin`. */
export function movedVertically(origin, target, direction) {
  if (!origin || !target) return false;
  if (target.page !== origin.page) {
    return direction === "up" ? target.page < origin.page : target.page > origin.page;
  }
  return direction === "up" ? target.y < origin.y : target.y > origin.y;
}

/**
 * The position a vertical arrow key should land on.
 *
 * Takes the engine's own answer and accepts it when it really did move in the
 * asked-for direction. It often does not: at the end of the owner's document
 * `moveCaret(up)` returns the START OF THE SAME LINE — a different model
 * position, so "did the position change?" says yes, while on screen the caret
 * slides sideways and never leaves the last line. Only geometry can tell those
 * apart, which is why this compares the engine's rects rather than its ids.
 *
 * Falls back to the engine's answer when there is nothing in that direction,
 * so the first and last lines stay the no-ops they should be.
 */
export function recoverVerticalMove(direction, from, engineResult, io) {
  const origin = io.positionRect(from);
  if (movedVertically(origin, io.positionRect(engineResult), direction)) return engineResult;
  return probeVerticalNeighbour(direction, from, io) ?? engineResult;
}

/**
 * The model position one line above/below the caret, found geometrically.
 *
 * @param {"up"|"down"} direction
 * @param {{node: string, offset: number}} from the position the engine could
 *   not move away from
 * @param {object} io
 * @param {() => ({left:number,top:number,bottom:number}|null)} io.caretRect
 *   the painted caret's client rect
 * @param {(x:number, y:number) => ({node:string,offset:number}|null)} io.resolveAt
 *   hit-test a client point to a model position
 * @param {(pos:object) => ({page:number,y:number}|null)} io.positionRect
 *   the engine's own geometry for a model position
 * @returns {{node:string,offset:number}|null} null when there is genuinely
 *   nothing in that direction — the top and bottom of the document must stay
 *   no-ops.
 */
export function probeVerticalNeighbour(direction, from, io) {
  const rect = io.caretRect();
  if (!rect) return null;
  const origin = io.positionRect(from);
  if (!origin) return null;
  for (const point of verticalProbePoints(rect, direction)) {
    const candidate = io.resolveAt(point.x, point.y);
    if (!candidate) continue;
    if (candidate.node === from.node && candidate.offset === from.offset) continue;
    // The hit test clamps a point outside a page into that page's box, so a
    // probe past the first or last line can resolve to a position that is not
    // in the direction asked for. Only the engine's own geometry decides.
    if (movedVertically(origin, io.positionRect(candidate), direction)) return candidate;
  }
  return null;
}
