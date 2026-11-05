// Selection by WORDS and PARAGRAPHS for a mouse gesture (`docs/109` UX-060,
// `docs/174` TA-09 and TA-22).
//
// Word, Google Docs and ONLYOFFICE all remember the unit a selection was started
// in. A double-click selects a word, and dragging on from it extends by whole
// words; a triple-click selects a paragraph, and dragging extends by whole
// paragraphs; Shift+click after a double-click extends to a word boundary. In
// every case the unit the gesture STARTED on stays selected, whichever way the
// pointer goes. The editor used to drag by character from a collapsed caret and
// then, on release, let the `dblclick` event replace the whole range with the one
// word under the pointer: double-click "quick", drag to "fox", and "jumps" — or
// whatever the pointer was over — was all that stayed selected.
//
// This module is the rule, with no DOM in it: `editor_pointer.mjs` records the
// unit from the press's click count and calls `extendByUnit` on every move.
//
// Complexity: O(1) engine calls per pointer move — one `wordAt` or one
// `paragraphLength`, and at most one `selectionEdge` to order the two ends.

/** The unit a press selects by, from its click count. A touch press never selects
 *  by unit here: the long press owns that (`touch_selection`). */
export function unitForPress(event, clicks) {
  if (event.pointerType === "touch") return "char";
  if (clicks >= 3) return "paragraph";
  if (clicks === 2) return "word";
  return "char";
}

/** The platform's double-click interval and slop, as browsers apply them. */
const MULTI_CLICK_MS = 500;
const MULTI_CLICK_PX = 4;

/** Counts presses into single, double and triple clicks.
 *
 *  A `pointerdown` carries no click count — Chrome reports `detail` 0 on pointer
 *  events, as the Pointer Events spec says it may; only the `mousedown` that
 *  follows it has one — so the press that STARTS a word drag would read as a
 *  single click. A press continues the count when it comes within the
 *  double-click interval and a few pixels of the previous one; a non-zero
 *  `detail` is believed where a browser does supply it. O(1). */
export function createClickCounter() {
  let last = null;
  return function count(event) {
    const near =
      last &&
      event.timeStamp - last.time <= MULTI_CLICK_MS &&
      Math.abs(event.clientX - last.x) <= MULTI_CLICK_PX &&
      Math.abs(event.clientY - last.y) <= MULTI_CLICK_PX;
    const clicks = event.detail > 0 ? event.detail : near ? Math.min(last.clicks + 1, 3) : 1;
    last = { time: event.timeStamp, x: event.clientX, y: event.clientY, clicks };
    return clicks;
  };
}

/** The word or paragraph around `at`, as `{ start, end }`. A position with no word
 *  under it (a space, an empty paragraph) is its own collapsed span. */
export function unitSpan(doc, unit, at) {
  if (unit === "paragraph") {
    return {
      start: { node: at.node, offset: 0 },
      end: { node: at.node, offset: doc.paragraphLength(at.node) },
    };
  }
  if (unit === "word") {
    const bounds = Array.from(doc.wordAt(at.node, at.offset) ?? []);
    if (bounds.length === 2) {
      return { start: { node: at.node, offset: bounds[0] }, end: { node: at.node, offset: bounds[1] } };
    }
  }
  return { start: { ...at }, end: { ...at } };
}

/** Whether `a` lies strictly before `b` in document order. Positions in two
 *  paragraphs are ordered by the engine, which alone knows document order. */
export function isBefore(doc, a, b) {
  if (a.node === b.node) return a.offset < b.offset;
  const start = doc.selectionEdge(a.node, a.offset, b.node, b.offset, false);
  const first = start.node === a.node && start.offset === a.offset;
  start.free?.();
  return first;
}

/** The selection a unit gesture holds with the pointer at `at`: from the far edge
 *  of the ORIGIN unit to the outer edge of the unit under the pointer, so the
 *  origin stays selected in both directions. */
export function extendByUnit(doc, origin, unit, at) {
  const here = unitSpan(doc, unit, at);
  if (isBefore(doc, here.start, origin.start)) return { anchor: { ...origin.end }, focus: here.start };
  return { anchor: { ...origin.start }, focus: here.end };
}
