// Recovering a vertical caret move the engine could not make.
//
// `move_vertical` in the layout crate picks the next line by FLOW INDEX into a
// flat line list (HF-024). Around a table that ordering is not the reading
// order — a TableRow fragment emits all of cell 1's lines before cell 2's — so
// the candidate one step away is sometimes not the line above, and the search
// dead-ends: `moveCaret(up)` returns the position it was given.
//
// Measured on the owner's loan agreement (5 pages, tables top and bottom):
// after Ctrl+End, sixty ArrowUps moved the caret zero pixels. Walking up from
// mid-document, it stopped dead at the paragraph between the first two tables
// and no further press did anything. Down was unaffected, which is why this
// reads as "arrow keys stopped working" rather than as a table bug.
//
// The engine fix is geometric (HF-024's own prescribed fix, applied to the UP
// direction), and it belongs in `crates/casual-doc-layout`. This is the host
// side of the same idea, and it is deliberately a FALLBACK: it runs only when
// the engine reported no movement at all, so where navigation works today
// nothing here executes. Where it does run, the alternative is a key that does
// nothing.
//
// It is pure — all geometry and model access is injected — so the decision
// logic is unit-testable without a browser or an engine.

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
