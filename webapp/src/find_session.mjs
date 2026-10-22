/** One Find session: where it searches from, which match is current, and the
 *  all-match highlight (`docs/174` TA-03, TA-04).
 *
 *  Word, Google Docs and ONLYOFFICE agree on two rules this module owns:
 *
 *  1. **Typing the query refines, it never advances.** Each keystroke re-searches
 *     from one fixed ANCHOR — the start of the selection when Find took focus, or
 *     the match the user last stepped to with Next/Previous/Enter — and lands on
 *     the first match at or after it. Typing "f", "fo", "fox" therefore stays on
 *     the first fox after the caret, and Replace changes THAT one. Before this,
 *     every keystroke stepped forward from the previous match, so "fox" ended on
 *     4 of 4 and Replace changed a match the user never navigated to.
 *  2. **Next from a caret goes to the next match after the caret**, not to the
 *     first match in the document; Replace leaves the caret after its
 *     replacement, so the same rule makes Replace move on to the following match.
 *
 *  And it paints every match (Word's "Highlight all", Docs' yellow marks) with
 *  the current one distinct, from the match list already computed for "n of N".
 *
 *  Pure: the engine, the selection and the overlay are injected (`io`), so the
 *  rules run in node. `io` supplies:
 *    `getDoc()`             the engine document, or null
 *    `getSelection()`       `{anchor, focus}` model positions, or null
 *    `isImplicit()`         true while the caret is the untouched load-time seed,
 *                           which is not a place the user searched from
 *    `place(flat, kind)`    paints one `[page, x, y, w, h]` twip rect on its
 *                           page's overlay (main.js's `place`)
 *    `revisionKey()`        identifies the document's current state; a highlight
 *                           computed under another key is stale
 *    `active()`             the Find panel is open
 *    `repaint()`            repaint the overlay layer
 *    `rescan()`             recompute the matches (main.js's `updateFindStatus`)
 *    `schedule(fn, ms)`     a timer (defaults to `setTimeout`), injected for tests
 *
 *  Complexity: `pick` and `show` are O(matches) with at most two engine order
 *  tests per match; `paint` is O(matches) `place` calls and asks the engine for
 *  a match's rectangles once per match list (cached until the list or the
 *  document changes), so scrolling and caret moves never re-query the engine.
 *  An edit while Find is open costs O(1) on the keystroke: the highlight is
 *  dropped as stale and recomputed once the typing pauses.
 */

/** The overlay classes. The current match carries both. */
export const FIND_MATCH_CLASS = "find-match";
export const FIND_CURRENT_CLASS = "find-match find-match-current";

/** How long typing in the DOCUMENT must pause before a stale highlight is
 *  recomputed. A scan is O(document); a keystroke must stay O(1). */
export const FIND_RESCAN_DELAY_MS = 250;

/** The first match whose start is at or after `from`, wrapping to the first
 *  match when none is. `from === null` means "the top".
 *
 *  @param {Array<{startNode: string, startOffset: number}>} matches  document order
 *  @param {{node: string, offset: number} | null} from
 *  @param {(aNode: string, aOff: number, bNode: string, bOff: number) => boolean} posLE
 */
export function firstMatchAtOrAfter(matches, from, posLE) {
  if (!from) return 0;
  const index = matches.findIndex((m) => posLE(from.node, from.offset, m.startNode, m.startOffset));
  return index >= 0 ? index : 0;
}

/** The last match that starts strictly before `from`, wrapping to the last
 *  match when none does. */
export function lastMatchBefore(matches, from, posLE) {
  if (!from) return matches.length - 1;
  for (let i = matches.length - 1; i >= 0; i--) {
    const m = matches[i];
    if (!posLE(from.node, from.offset, m.startNode, m.startOffset)) return i;
  }
  return matches.length - 1;
}

/** The index a Find step lands on.
 *
 *  - `incremental` (the query was typed): the first match at or after `anchor`.
 *  - otherwise, when the selection IS a match (`current >= 0`): the neighbour of
 *    it, cyclically.
 *  - otherwise: the next match after `from` (forward) or the last before it.
 */
export function stepIndex(matches, { current = -1, forward = true, incremental = false, anchor = null, from = null }, posLE) {
  if (!matches.length) return -1;
  if (incremental) return firstMatchAtOrAfter(matches, anchor, posLE);
  if (current >= 0) return (current + (forward ? 1 : matches.length - 1)) % matches.length;
  return forward ? firstMatchAtOrAfter(matches, from, posLE) : lastMatchBefore(matches, from, posLE);
}

/** True when `a` is at or before `b`: the same node compares offsets, otherwise
 *  the engine orders the two (one `selectionEdge`, handle freed). */
function enginePosLE(doc) {
  return (aNode, aOff, bNode, bOff) => {
    if (aNode === bNode) return aOff <= bOff;
    const edge = doc.selectionEdge(aNode, aOff, bNode, bOff, false);
    const aIsEarlier = edge.node === aNode;
    edge.free();
    return aIsEarlier;
  };
}

export function createFindSession(io) {
  const schedule = io.schedule ?? ((fn, ms) => setTimeout(fn, ms));
  /** Where typing re-searches from: a model position, or null for the top. */
  let anchor = null;
  /** What `paint` draws, the state it was computed under, and its rects. */
  let shown = [];
  let shownCurrent = -1;
  let shownKey = null;
  let rects = null;
  let repaintQueued = false;
  let rescanQueued = false;

  /** The start of the user's selection, or null for an untouched seed caret. */
  function selectionStart() {
    const doc = io.getDoc();
    const selection = io.getSelection();
    if (!doc || !selection || io.isImplicit()) return null;
    const { anchor: a, focus: f } = selection;
    if (a.node === f.node) return { node: a.node, offset: Math.min(a.offset, f.offset) };
    const edge = doc.selectionEdge(a.node, a.offset, f.node, f.offset, false);
    const start = { node: edge.node, offset: edge.offset };
    edge.free();
    return start;
  }

  /** The match the selection is on (its anchor is the match's start, which is
   *  how `selectTextMatch` selects one), or -1. */
  function currentIndex(matches) {
    const selection = io.getSelection();
    if (!selection || io.isImplicit()) return -1;
    return matches.findIndex(
      (m) => m.startNode === selection.anchor.node && m.startOffset === selection.anchor.offset,
    );
  }

  function queueRepaint() {
    if (repaintQueued) return;
    repaintQueued = true;
    queueMicrotask(() => {
      if (!repaintQueued) return;
      repaintQueued = false;
      io.repaint();
    });
  }

  return {
    /** Fixes the anchor at the current selection. Called when the query field
     *  takes focus, which is also what opening Find does. */
    anchorHere() {
      anchor = selectionStart();
    },

    /** The anchor, for tests and diagnostics. */
    anchor() {
      return anchor;
    },

    /** The index to select for one Find step (see `stepIndex`). A step the user
     *  asked for (Next, Previous, Enter) moves the anchor to where it landed, so
     *  typing afterwards refines from the match they navigated to. */
    pick(matches, forward, incremental) {
      const doc = io.getDoc();
      if (!doc || !matches.length) return -1;
      const posLE = enginePosLE(doc);
      const index = stepIndex(
        matches,
        incremental
          ? { incremental: true, anchor }
          : { current: currentIndex(matches), forward, from: selectionStart() },
        posLE,
      );
      if (!incremental && index >= 0) {
        anchor = { node: matches[index].startNode, offset: matches[index].startOffset };
      }
      return index;
    },

    /** Publishes the match list for the highlight and returns the current
     *  match's index (-1 when the selection is not on one). */
    show(matches) {
      shown = matches;
      shownCurrent = currentIndex(matches);
      shownKey = io.revisionKey();
      rects = null;
      queueRepaint();
      return shownCurrent;
    },

    /** Drops the highlight (Find closed). */
    clear() {
      if (!shown.length) return;
      shown = [];
      shownCurrent = -1;
      rects = null;
      queueRepaint();
    },

    /** Paints every match, the current one distinct. Called from the overlay
     *  repaint, so it never scans: a list computed before the last edit is
     *  stale (its offsets may have moved), is not drawn, and is recomputed
     *  once the typing pauses. */
    paint() {
      repaintQueued = false;
      if (!shown.length || !io.active()) return;
      const doc = io.getDoc();
      if (!doc) return;
      if (shownKey !== io.revisionKey()) {
        if (!rescanQueued) {
          rescanQueued = true;
          schedule(() => {
            rescanQueued = false;
            if (io.active() && shownKey !== io.revisionKey()) io.rescan();
          }, FIND_RESCAN_DELAY_MS);
        }
        return;
      }
      if (!rects) {
        rects = shown.map((m) => {
          try {
            return doc.selectionRects(m.startNode, m.startOffset, m.endNode, m.endOffset);
          } catch {
            return [];
          }
        });
      }
      for (let i = 0; i < shown.length; i++) {
        const flat = rects[i];
        const kind = i === shownCurrent ? FIND_CURRENT_CLASS : FIND_MATCH_CLASS;
        for (let r = 0; r + 4 < flat.length; r += 5) io.place(Array.prototype.slice.call(flat, r, r + 5), kind);
      }
    },
  };
}
