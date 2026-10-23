// Caret navigation: the vertical goal column and `navCaret`, the arrow-key
// movement every caret key goes through. Moved out of `main.js` unchanged
// (docs/177); it reaches the rest of the editor only through the context `ed`.
import { createVerticalGoal, recoverVerticalMove } from "./caret_navigation.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createCaretNav(ed) {
  const verticalGoal = createVerticalGoal();

  /** Move the caret by arrow key. Shift extends (moves the focus); plain collapses. */
  function navCaret(dir, extend) {
    if (!ed.selection) return;
    if (ed.objectCropSession) ed.cancelCrop(); // arrow-navigating away discards the crop preview
    ed.objectSelection = null; // moving the caret leaves any object selection
    // ...and so does it leave a row/column/table selection. These two were written
    // as one rule and only one of them was applied here, which is how arrowing out
    // of a table left the whole table still painted as selected.
    ed.tableRange.clear();
    ed.clearObjectStatus();
    ed.breakTypingSession();
    ed.pendingFormat = null; // caret moved → disarm typing format
    const collapseToStart = dir === "left" || dir === "wordLeft";
    const collapseToEnd = dir === "right" || dir === "wordRight";
    const goalX = verticalGoal.columnFor(dir, ed.selection.focus, () => caretColumn(ed.selection.focus));
    const c =
      !extend && ed.hasRange() && (collapseToStart || collapseToEnd)
        ? ed.doc.selectionEdge(
            ed.selection.anchor.node,
            ed.selection.anchor.offset,
            ed.selection.focus.node,
            ed.selection.focus.offset,
            collapseToEnd,
          )
        : ed.doc.moveCaret(ed.selection.focus.node, ed.selection.focus.offset, dir, goalX ?? undefined);
    const to = { node: c.node, offset: c.offset };
    c.free();
    // The engine still dead-ends going UP out of a table: it answers with the
    // position it was given and the key does nothing visible. `recoverVerticalMove`
    // takes the engine's answer whenever it really moved and only otherwise finds
    // the neighbouring line by hit-testing — and a rescued move is then put back
    // on the run's goal column, which the probe's own geometry cannot know about.
    const rescued =
      dir === "up" || dir === "down"
        ? recoverVerticalMove(dir, ed.selection.focus, to, caretProbeIO())
        : to;
    const next = rescued === to ? to : atGoalColumn(rescued, goalX);
    verticalGoal.keep(goalX, next);
    ed.selection = extend ? { anchor: ed.selection.anchor, focus: next } : { anchor: next, focus: next };
    ed.drawSelection();
    ed.scrollCaretIntoView();
  }

  /** The caret's own column at `position` — page-local twips, the units the goal
   *  column is held in — or null where the position has no geometry. */
  function caretColumn(position) {
    return ed.doc.caretRect(position.node, position.offset)[1] ?? null;
  }

  /** `position`, moved sideways onto `goalX` on its own line. The rescue probe
   *  finds a LINE by hit-testing below/above the painted caret, and the caret's
   *  x is not the column the run aims at once a short line has clamped it. */
  function atGoalColumn(position, goalX) {
    const flat = goalX === null ? [] : ed.doc.caretRect(position.node, position.offset);
    if (flat.length < 5) return position;
    const hit = ed.doc.hitTest(flat[0], goalX, flat[2] + Math.round(flat[4] / 2));
    if (!hit) return position;
    const at = { node: hit.node, offset: hit.offset };
    hit.free();
    return at;
  }

  /** The geometry and model access `probeVerticalNeighbour` needs, bound to this
   *  editor's DOM and engine. */
  function caretProbeIO() {
    return {
      caretRect: () => ed.pagesEl.querySelector(".overlay .caret")?.getBoundingClientRect() ?? null,
      resolveAt: (x, y) => {
        const page = ed.pageFromClientPoint(x, y);
        return page ? ed.anchorAt(page, ed.clientPointEvent(x, y)) : null;
      },
      positionRect: (position) => {
        const flat = ed.doc.caretRect(position.node, position.offset);
        return flat.length >= 5 ? { page: flat[0], y: flat[2], height: flat[4] } : null;
      },
    };
  }

  return { navCaret, verticalGoal };
}
