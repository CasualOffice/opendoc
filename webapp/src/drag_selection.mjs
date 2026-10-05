// What a pointer drag over the document selects.
//
// Two pure pieces, both of which were arithmetic buried inside `main.js`'s
// pointer handlers and therefore unreachable from a unit test (`109` HF-085):
//
//   1. **Selection granularity** — whether a drag extends by character, by
//      word, or by paragraph, which is decided by the click count of the press
//      that started it.
//   2. **The auto-scroll delta** — how far the viewport should move while the
//      pointer is held near an edge.
//
// ## 1. Granularity: the gesture every word processor has and this one did not
//
// Double-click a word and keep dragging, and every word processor extends the
// selection a whole word at a time. Triple-click and drag, and it extends a
// whole paragraph at a time. It is how a phrase or a run of paragraphs is
// selected with a mouse, and before this module the editor had neither: a drag
// was always character-granular, and — worse — the browser's `dblclick` fires
// on the SECOND pointer-up, i.e. *after* the drag has finished, so the word
// under the final pointer position overwrote everything the drag had selected.
// Double-click-and-drag therefore selected exactly one word, whatever distance
// the pointer covered.
//
// ### What the three references do
//
// * **Word** documents the gesture as "double-click and drag to select by
//   word"; triple-click selects a paragraph and dragging from it extends by
//   paragraph.
// * **Google Docs** behaves identically, including the detail that the ANCHOR
//   end also grows outward, so the first word stays whole while you drag away
//   from it.
// * **ONLYOFFICE** implements it as a document-level selection TYPE that
//   persists for the whole gesture, which is the shape adopted here:
//     - `AscWord.TEXT_SELECTION_TYPE = { Common: 1, Word: 2, Paragraph: 3 }`
//       (`sdkjs/word/Editor/Document.js:82-86`);
//     - chosen from the click count on mouse-down —
//       `if (1 >= e.ClickCount) … Common; else if (e.ClickCount % 2) …
//       Paragraph; else … Word` (`Document.js:10489-10494`), so an EVEN count
//       is word-wise and an ODD count above one is paragraph-wise, which is
//       what makes a 4th click go back to words and a 5th back to paragraphs;
//     - applied on every selection update, not only at the end:
//       `if (logicDocument.IsWordSelection()) pos =
//       this.getClosestWordPos(pos, …)` and `this.checkWordSelection()`, and
//       `if (logicDocument.IsParagraphSelection()) this.SelectAll(…)`
//       (`sdkjs/word/Editor/Paragraph.js:408-409, 423-427`);
//     - `checkWordSelection` (`Paragraph.js:8606-8626`) is the part that grows
//       the anchor end: it snaps the moving end to the word START when the
//       selection runs forward and to the word END when it runs backward.
//       That is `granularEnds` below, with the direction read once rather than
//       inferred per end.
//
// `granularityForClickCount` keeps ONLYOFFICE's modular rule rather than
// collapsing "3 or more" to paragraph, because the alternating gesture is the
// documented behaviour of all three references and a reader who clicks a fourth
// time expects words back.
//
// ### Deliberate difference: the raw press position is kept
//
// ONLYOFFICE mutates the selection in place and re-derives the anchor from
// whatever the last update left behind. This module is handed the RAW hit-test
// positions — the one the press landed on and the one under the pointer now —
// and computes both ends from them every time. Re-snapping an already-snapped
// anchor is what makes a direction change wrong: an anchor parked on a word
// boundary belongs to two words, and asking which word is "at" that offset
// gives a different answer depending on which way the drag last went.
//
// ### Complexity, and why there is a cache
//
// `wordBounds` and `paragraphLength` reach the engine, and both of those
// engine calls are O(document) today: `paragraph_text` in
// `casual-doc-wasm/src/lib.rs` collects the text of EVERY paragraph on every
// surface and then finds the one it was asked for. A pointer-move handler
// calling that at frame rate would be an O(document) interaction, which
// `docs/107` §4 forbids outright.
//
// So `createGranularityCache` memoises both, and the memo is keyed so that the
// common motion costs nothing: a word lookup is reused while the pointer stays
// anywhere INSIDE the word it already resolved, and a paragraph length is
// reused while the pointer stays in that paragraph. The engine is therefore
// asked once per word (or paragraph) the drag crosses — bounded by the gesture
// rather than by the frame rate — and a drag that never leaves one word is
// O(1) in total. The ordering question (`forward`) is free whenever both ends
// are in the same paragraph, which is every word-granular drag inside one
// paragraph, and is memoised per paragraph pair otherwise.
//
// The engine's O(document) `paragraph_text` is a defect in its own right — it
// is on the Backspace and Delete key paths too — and is reported separately;
// when it becomes a direct lookup this cache gets cheaper and stays correct.

/** A drag that extends one character at a time: an ordinary single-click drag. */
export const CHARACTER = "character";
/** A drag that extends a whole word at a time: started by a double-click. */
export const WORD = "word";
/** A drag that extends a whole paragraph at a time: started by a triple-click. */
export const PARAGRAPH = "paragraph";

/**
 * The granularity a press with this click count starts a drag at.
 *
 * `count` is `MouseEvent.detail` — the browser's click count, which is
 * ONLYOFFICE's `MouseEvent.ClickCount`. The rule is theirs
 * (`sdkjs/word/Editor/Document.js:10489-10494`) and Word's and Docs': one click
 * is character-wise, an even count is word-wise, an odd count above one is
 * paragraph-wise.
 *
 * O(1).
 *
 * @param {number} count
 * @returns {"character"|"word"|"paragraph"}
 */
export function granularityForClickCount(count) {
  if (!Number.isFinite(count) || count <= 1) return CHARACTER;
  return count % 2 === 0 ? WORD : PARAGRAPH;
}

/**
 * The two selection ends a granular drag should show.
 *
 * Both inputs are RAW hit-test positions: `anchor` is where the press landed
 * and `focus` is what is under the pointer now. Neither is ever read back from
 * the selection this returns, which is what makes a direction change during the
 * drag reversible (see the module comment).
 *
 * A word the engine declines to bound — `wordBounds` answers `[]` for an offset
 * that is not inside a word, which is where a click in a run of spaces or past
 * the end of a line lands — leaves that end exactly where the pointer is. The
 * alternative is to invent a boundary, and a selection that jumps to somewhere
 * the pointer never was is worse than one that is character-granular for one
 * end.
 *
 * O(1) plus at most two `wordBounds`/`paragraphLength` calls and at most one
 * `forward` call; with {@link createGranularityCache} behind `io` those are
 * served from the memo for every move that stays inside the same word.
 *
 * @param {object} args
 * @param {{node: string, offset: number}} args.anchor raw press position
 * @param {{node: string, offset: number}} args.focus raw pointer position
 * @param {"character"|"word"|"paragraph"} args.granularity
 * @param {object} io
 * @param {(a: object, b: object) => boolean} io.forward whether `b` is at or
 *        after `a` in document order — the engine's answer, because offsets are
 *        per-paragraph and cannot order two different paragraphs
 * @param {(node: string, offset: number, end: "anchor"|"focus") => number[]}
 *        io.wordBounds `[start, end]` of the word at `offset`, or `[]`. `end`
 *        says which of the drag's two ends is asking, which is what lets a memo
 *        hold one word per end instead of a map that grows with the drag.
 * @param {(node: string) => number} io.paragraphLength
 * @returns {{anchor: {node: string, offset: number}, focus: {node: string, offset: number}}}
 */
export function granularEnds({ anchor, focus, granularity }, io) {
  if (granularity !== WORD && granularity !== PARAGRAPH) {
    return { anchor: { ...anchor }, focus: { ...focus } };
  }
  const forward = io.forward(anchor, focus);
  if (granularity === PARAGRAPH) {
    return {
      anchor: { node: anchor.node, offset: forward ? 0 : io.paragraphLength(anchor.node) },
      focus: { node: focus.node, offset: forward ? io.paragraphLength(focus.node) : 0 },
    };
  }
  return {
    anchor: { node: anchor.node, offset: wordEdge(io, anchor, "anchor", forward ? 0 : 1) },
    focus: { node: focus.node, offset: wordEdge(io, focus, "focus", forward ? 1 : 0) },
  };
}

/**
 * The two ends of a whole-paragraph selection: what a bare triple-click selects.
 *
 * The same answer `granularEnds` gives for a paragraph-granular drag that has
 * not left its paragraph, named separately because the triple-click handler has
 * no second end to order against. ONLYOFFICE ends an odd click count above one
 * in `this.SelectAll(1)` on the paragraph (`sdkjs/word/Editor/Paragraph.js:8513-8523`).
 *
 * @param {string} node
 * @param {{paragraphLength: (node: string) => number}} io
 */
export function paragraphEnds(node, io) {
  const end = io.paragraphLength(node);
  return { anchor: { node, offset: 0 }, focus: { node, offset: end } };
}

/** One end of the word at `at`, falling back to the raw offset when the engine
 *  does not consider that offset to be inside a word. `edge` is 0 for the start
 *  and 1 for the end. */
function wordEdge(io, at, end, edge) {
  const bounds = io.wordBounds(at.node, at.offset, end);
  return bounds?.length === 2 ? bounds[edge] : at.offset;
}

/**
 * A gesture-scoped memo over the three engine questions {@link granularEnds}
 * asks, so that a pointer-move handler is not O(document) per frame.
 *
 * Correct only while the document does not change, which is exactly the life of
 * one pointer gesture: nothing can edit the document between pointer-down and
 * pointer-up. `reset()` is called when the gesture ends; a caller that forgets
 * is the reason `reset` also clears the ordering memo rather than leaving it to
 * be overwritten.
 *
 * The word memo holds ONE word per end rather than a map, because a drag has
 * exactly two ends and each moves through words one at a time: a map would grow
 * for the length of the drag and buy nothing. A lookup hits while the offset is
 * inside the cached word's span, INCLUSIVE of both boundaries — an offset
 * exactly on a boundary belongs to the word already resolved, so a pointer
 * resting on a word edge does not re-ask the engine on every frame.
 *
 * The three engine calls are named HERE rather than in `main.js` so that one
 * place knows what granularity needs from the engine, and so a unit test
 * exercises the real call shape — `selectionEdge` included, which hands back a
 * wasm object the caller must `free()`.
 *
 * @param {() => object} getDoc the open document, or null
 */
export function createGranularityCache(getDoc) {
  const engine = {
    /** Whether `b` is at or after `a` in document order. Ordering across
     *  paragraphs is the engine's answer: offsets are per-paragraph. */
    forward(a, b) {
      const start = getDoc().selectionEdge(a.node, a.offset, b.node, b.offset, false);
      const same = start.node === a.node && start.offset === a.offset;
      start.free();
      return same;
    },
    wordBounds: (node, offset) => getDoc().wordAt(node, offset),
    paragraphLength: (node) => getDoc().paragraphLength(node),
  };
  const words = new Map(); // "anchor" | "focus" -> { node, bounds }
  const lengths = new Map(); // node -> paragraph length
  let order = null; // { key, forward }
  let calls = 0; // engine calls served, for the complexity guard

  return {
    forward(a, b) {
      // Same paragraph: the offsets order themselves and the engine is not
      // involved at all, which is every word-granular drag inside one
      // paragraph.
      if (a.node === b.node) return a.offset <= b.offset;
      const key = `${a.node} ${b.node}`;
      if (order && order.key === key) return order.forward;
      calls += 1;
      const forward = engine.forward(a, b);
      order = { key, forward };
      return forward;
    },
    wordBounds(node, offset, end = "focus") {
      const held = words.get(end);
      if (held && held.node === node && offset >= held.bounds[0] && offset <= held.bounds[1]) {
        return held.bounds;
      }
      calls += 1;
      const bounds = engine.wordBounds(node, offset);
      if (bounds?.length === 2) words.set(end, { node, bounds });
      else words.delete(end);
      return bounds;
    },
    paragraphLength(node) {
      const held = lengths.get(node);
      if (held !== undefined) return held;
      calls += 1;
      const length = engine.paragraphLength(node);
      lengths.set(node, length);
      return length;
    },
    /** How many questions actually reached the engine. The complexity guard
     *  reads it: a drag across one word must not grow this with the number of
     *  pointer moves, which is the property that keeps the interaction O(1) in
     *  document size while the engine's own lookups are O(document). */
    engineCalls() {
      return calls;
    },
    reset() {
      words.clear();
      lengths.clear();
      order = null;
      calls = 0;
    },
  };
}

// ---- 2. Auto-scroll while dragging ------------------------------------------
//
// Extracted unchanged in behaviour. It was 30 lines of near-duplicated
// arithmetic inside `startSelectionAutoScroll`, one axis written out twice, and
// the horizontal half did not exist at all for a while — at any zoom where the
// sheet is wider than the window a drag-selection simply stopped at the window
// edge and the end of the line was unreachable by mouse. One function over one
// axis is what stops the next axis being forgotten.

/** How close to an edge the pointer has to be before the viewport follows. */
export const AUTO_SCROLL_EDGE_PX = 56;
/** The most one frame may scroll, so a pointer parked at the edge travels at a
 *  readable speed rather than teleporting. */
export const AUTO_SCROLL_MAX_PX = 24;

/**
 * How far one axis should scroll this frame, given where the pointer is.
 *
 * Negative is toward the `low` edge. Zero anywhere in the middle. The ramp is
 * linear in how far INTO the edge band the pointer is, so the scroll
 * accelerates as the pointer is pushed further against the edge and a pointer
 * resting just inside the band creeps.
 *
 * O(1).
 *
 * @param {number} position the pointer, in client coordinates
 * @param {number} low the viewport's low edge (`rect.top` / `rect.left`)
 * @param {number} high the viewport's high edge (`rect.bottom` / `rect.right`)
 * @param {number} band {@link AUTO_SCROLL_EDGE_PX}
 * @param {number} max {@link AUTO_SCROLL_MAX_PX}
 * @returns {number} whole pixels
 */
export function autoScrollStep(
  position,
  low,
  high,
  band = AUTO_SCROLL_EDGE_PX,
  max = AUTO_SCROLL_MAX_PX,
) {
  if (position < low + band) {
    return -Math.ceil(Math.min(1, (low + band - position) / band) * max);
  }
  if (position > high - band) {
    return Math.ceil(Math.min(1, (position - (high - band)) / band) * max);
  }
  return 0;
}

/**
 * Both axes at once: how far the viewport should scroll this frame.
 *
 * @param {{top: number, bottom: number, left: number, right: number}} rect the
 *        viewport, in client coordinates
 * @param {number} x pointer client x
 * @param {number} y pointer client y
 * @returns {{dx: number, dy: number}}
 */
export function autoScrollDelta(rect, x, y) {
  return {
    dx: autoScrollStep(x, rect.left, rect.right),
    dy: autoScrollStep(y, rect.top, rect.bottom),
  };
}
