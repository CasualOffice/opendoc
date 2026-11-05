// UX-060 (`docs/174` TA-09, TA-22): a double-click drag extends by whole words and
// a triple-click drag by whole paragraphs, keeping the unit it started on.
//
// Mutation proof: make `extendByUnit` return `{ anchor: origin.start, focus: at }`
// (a character drag) and the word and paragraph cases go red — "quick brown f"
// instead of "quick brown fox", and a paragraph range ending mid-paragraph.
import assert from "node:assert/strict";
import test from "node:test";

import { createClickCounter, extendByUnit, isBefore, unitForPress, unitSpan } from "../src/selection_units.mjs";

// Two paragraphs, in this order: p1 "quick brown fox jumps", p2 "lazy dog".
const TEXT = { p1: "quick brown fox jumps", p2: "lazy dog" };
const ORDER = ["p1", "p2"];
const fakeDoc = {
  paragraphLength: (node) => TEXT[node].length,
  wordAt(node, offset) {
    const text = TEXT[node];
    if (offset >= text.length || text[offset] === " ") return [];
    let start = offset;
    while (start > 0 && text[start - 1] !== " ") start -= 1;
    let end = offset;
    while (end < text.length && text[end] !== " ") end += 1;
    return [start, end];
  },
  selectionEdge(aNode, aOffset, bNode, bOffset, towardEnd) {
    const a = { node: aNode, offset: aOffset };
    const b = { node: bNode, offset: bOffset };
    const aFirst = ORDER.indexOf(aNode) < ORDER.indexOf(bNode) || (aNode === bNode && aOffset <= bOffset);
    return towardEnd ? (aFirst ? b : a) : aFirst ? a : b;
  },
};
const at = (node, offset) => ({ node, offset });
const text = ({ anchor, focus }) => {
  const [s, e] = isBefore(fakeDoc, focus, anchor) ? [focus, anchor] : [anchor, focus];
  if (s.node === e.node) return TEXT[s.node].slice(s.offset, e.offset);
  return `${TEXT[s.node].slice(s.offset)}|${TEXT[e.node].slice(0, e.offset)}`;
};

test("the click count picks the unit, and a touch press never does", () => {
  assert.equal(unitForPress({ pointerType: "mouse" }, 1), "char");
  assert.equal(unitForPress({ pointerType: "mouse" }, 2), "word");
  assert.equal(unitForPress({ pointerType: "mouse" }, 3), "paragraph");
  assert.equal(unitForPress({ pointerType: "pen" }, 4), "paragraph");
  assert.equal(unitForPress({ pointerType: "touch" }, 2), "char");
});

// Chrome's pointerdown reports `detail` 0, so the count is the editor's own. The
// first e2e draft read `event.detail` and every word drag silently stayed a
// character drag. Mutation proof: make `count` return `event.detail || 1` and
// the second and third presses read 1.
test("presses close in time and space count up to a triple click; a far or late one restarts", () => {
  const count = createClickCounter();
  const press = (timeStamp, clientX, clientY = 10) => count({ detail: 0, timeStamp, clientX, clientY });
  assert.equal(press(0, 10), 1);
  assert.equal(press(200, 11), 2);
  assert.equal(press(400, 11), 3);
  assert.equal(press(600, 11), 3, "a fourth quick press stays a triple");
  assert.equal(press(2000, 11), 1, "too late");
  assert.equal(press(2100, 40), 1, "too far");
  assert.equal(count({ detail: 2, timeStamp: 9000, clientX: 0, clientY: 0 }), 2, "a browser's own count wins");
});

test("a double-click drag forward extends to the end of the word under the pointer", () => {
  const origin = unitSpan(fakeDoc, "word", at("p1", 2)); // "quick"
  assert.equal(text({ anchor: origin.start, focus: origin.end }), "quick");
  // Dragged to the middle of "fox": the whole of "fox", and "quick" still in.
  assert.equal(text(extendByUnit(fakeDoc, origin, "word", at("p1", 13))), "quick brown fox");
});

test("a double-click drag backward keeps the origin word and extends to a word start", () => {
  const origin = unitSpan(fakeDoc, "word", at("p1", 17)); // "jumps"
  const selection = extendByUnit(fakeDoc, origin, "word", at("p1", 7)); // in "brown"
  assert.equal(text(selection), "brown fox jumps");
  assert.deepEqual(selection.anchor, at("p1", 21), "the anchor is the far edge of the origin word");
});

test("a triple-click drag into the next paragraph selects both whole paragraphs", () => {
  const origin = unitSpan(fakeDoc, "paragraph", at("p1", 8));
  const selection = extendByUnit(fakeDoc, origin, "paragraph", at("p2", 2));
  assert.deepEqual(selection, { anchor: at("p1", 0), focus: at("p2", 8) });
  assert.equal(text(selection), "quick brown fox jumps|lazy dog");
});

test("a position with no word under it is its own collapsed span", () => {
  const span = unitSpan(fakeDoc, "word", at("p1", 5)); // the space after "quick"
  assert.deepEqual(span, { start: at("p1", 5), end: at("p1", 5) });
});
