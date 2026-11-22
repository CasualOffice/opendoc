// SPDX-License-Identifier: Apache-2.0
// Selecting by WORD or PARAGRAPH while dragging after a double or triple click
// (`docs/109` UX-059, audit CANVAS-04).
//
// Word, Google Docs and ONLYOFFICE all do it: double-click a word, keep the
// button down and drag, and the selection grows a whole word at a time from the
// first word; triple-click-drag grows a paragraph at a time. Ours selected the
// word on `dblclick`, which fires AFTER the button is released, so a
// double-click-drag selected character by character and then collapsed to the
// word under the release point — the word the gesture started on was lost.
//
// The fix is the established "selection granularity" pattern: the press with
// `detail` 2 or 3 records the unit and the unit range it landed in (the
// ORIGIN), and every drag position is snapped outward to that unit, keeping the
// origin wholly inside the selection whichever way the pointer goes.
//
// Pure apart from the engine calls it is handed, so it is unit-tested in Node.
// Complexity: O(1) engine calls per pointer move (one `wordAt` or
// `paragraphLength`, and one `selectionEdge` only when the two ends are in
// different paragraphs).

/** How long and how far apart two presses may be to count as one multi-click —
 *  the platform defaults Windows and macOS ship (500 ms, a few pixels). */
export const MULTI_CLICK_MS = 500;
export const MULTI_CLICK_PX = 4;

/** Counts clicks on PRESS, as a stand-in for the browser's own count.
 *
 *  The browser's count (`MouseEvent.detail`, which follows the OS: its
 *  interval, its distance and whatever the input device reported) is the truth,
 *  but nothing on the press carries it here. `pointerdown`'s `detail` is 0 in
 *  Chromium, and the `mousedown` that would carry it is never dispatched,
 *  because the editor cancels `pointerdown` (a cancelled `pointerdown`
 *  suppresses the compatibility mouse events; measured in Chromium:
 *  `pointerdown:0 click:1 pointerdown:0 click:2 dblclick:2`). The first event
 *  that carries the browser's count is the `click` after the release. So:
 *
 *  - `press(x, y, time)` guesses 1, 2 or 3 for the press, wrapping back to 1
 *    after a triple, as Word does, so a double-click-drag can grow by word
 *    from the press on;
 *  - `settle(detail)` takes the browser's count from that `click`, so the next
 *    press continues from the browser's count, not from the guess;
 *  - `pressUnitStands` (below) decides whether the press's unit survives once
 *    the browser's count is known.
 *
 *  O(1). */
export function createClickCounter() {
  let last = null;
  return {
    press(x, y, time) {
      const near = last && time - last.time <= MULTI_CLICK_MS && Math.hypot(x - last.x, y - last.y) <= MULTI_CLICK_PX;
      const count = near ? (last.count % 3) + 1 : 1;
      last = { x, y, time, count };
      return count;
    },
    settle(detail) {
      if (last && Number.isInteger(detail) && detail > 0) last.count = ((detail - 1) % 3) + 1;
    },
  };
}

/** Whether the unit a press selected on its guessed count (`pressUnit`) stands
 *  once the `click` brings the browser's count (`detail`).
 *
 *  It stands when the browser agrees, and when a drag has already grown the
 *  selection beyond the press's unit (the gesture was the person's, whatever
 *  the count). Otherwise the browser wins: a press the guess took for the
 *  third of a triple, which the browser counts as a single click, is a caret,
 *  not a paragraph (a click straight after a double-click, with a device or an
 *  OS that does not count it on). A browser count of 2 or 3 is handed to the
 *  `dblclick` / triple-`click` routes, which select the word or paragraph. */
export function pressUnitStands(pressUnit, detail, grown) {
  if (!pressUnit || grown) return true;
  return unitForClickCount(detail) === pressUnit;
}

/** The selection unit a press with this click count starts, or null. */
export function unitForClickCount(detail) {
  if (detail === 2) return "word";
  if (detail === 3) return "paragraph";
  return null;
}

/** The `{start, end}` of the `unit` containing `at`. A point between words (a
 *  space, punctuation) is its own collapsed unit, as in Word. */
export function unitRange(doc, at, unit) {
  if (unit === "paragraph") {
    return {
      start: { node: at.node, offset: 0 },
      end: { node: at.node, offset: doc.paragraphLength(at.node) },
    };
  }
  const bounds = doc.wordAt(at.node, at.offset);
  if (bounds?.length === 2) {
    return {
      start: { node: at.node, offset: bounds[0] },
      end: { node: at.node, offset: bounds[1] },
    };
  }
  return { start: { ...at }, end: { ...at } };
}

/** Whether `a` comes strictly before `b` in the document. Two paragraphs are
 *  ordered by the engine — the host cannot compare node ids. */
export function isBefore(doc, a, b) {
  if (a.node === b.node) return a.offset < b.offset;
  const first = doc.selectionEdge(a.node, a.offset, b.node, b.offset, false);
  const before = first.node === a.node && first.offset === a.offset;
  first.free?.();
  return before;
}

/** The selection a unit drag shows with the pointer at `at`: the origin unit
 *  plus every whole unit up to the one under the pointer, anchored on the far
 *  side of the origin so the origin is never cut. */
export function extendByUnit(doc, origin, at, unit) {
  const here = unitRange(doc, at, unit);
  if (isBefore(doc, here.start, origin.start)) return { anchor: origin.end, focus: here.start };
  return { anchor: origin.start, focus: here.end };
}

/** Whether `selection` is still the range a unit gesture left, so Shift+click
 *  extends THAT selection by its unit (`docs/109` UX-076). A caret the keyboard
 *  has since collapsed onto the origin's edge is not: it extends by character. O(1). */
export function extendsUnitSelection(selection, origin) {
  if (!selection) return false;
  const { anchor, focus } = selection;
  if (anchor.node === focus.node && anchor.offset === focus.offset) return false;
  return [origin.start, origin.end].some((edge) => edge.node === anchor.node && edge.offset === anchor.offset);
}
