// The caret and story contract's host-side rules (`docs/109` UX-057..UX-061),
// asserted on fake engines so each rule is pinned without a page:
//
//   * Ctrl+Home / Ctrl+End / Select All target the BODY unless a header or
//     footer is deliberately open (CRITIC-01);
//   * a double/triple press selects by word/paragraph and a drag grows it a
//     whole unit at a time, keeping the origin (CANVAS-04);
//   * opening a document focuses it only when that takes focus from no one
//     (CANVAS-05);
//   * ArrowLeft/ArrowRight follow the text direction on screen (CANVAS-03);
//   * Enter on an empty list item leaves the list on every input path
//     (CANVAS-02).
import { test } from "node:test";
import assert from "node:assert/strict";

import { storyBoundary } from "../src/editor_selection_commands.mjs";
import { createClickCounter, extendByUnit, isBefore, pressUnitStands, unitForClickCount, unitRange } from "../src/unit_selection.mjs";
import { shouldFocusOnOpen } from "../src/open_focus.mjs";
import {
  caretInRtl,
  charsAroundByteOffset,
  strongDirection,
  visualDirection,
} from "../src/caret_navigation.mjs";
import { exitEmptyListItem } from "../src/list_enter.mjs";

/** A three-story document: body b1..b2, header h1, footer f1..f2. */
function storyDoc() {
  const kinds = { b1: "body", b2: "body", h1: "header", f1: "footer", f2: "footer", n1: "footnote" };
  const edges = {
    body: [{ node: "b1", offset: 0 }, { node: "b2", offset: 9 }],
    header: [{ node: "h1", offset: 0 }, { node: "h1", offset: 4 }],
    footer: [{ node: "f1", offset: 0 }, { node: "f2", offset: 7 }],
    footnote: [{ node: "n1", offset: 0 }, { node: "n1", offset: 3 }],
  };
  return {
    storyKind: (node) => kinds[node] ?? "",
    storyEdge: (node, toEnd) => edges[kinds[node]][toEnd ? 1 : 0],
    firstPosition: () => edges.body[0],
    lastPosition: () => edges.body[1],
  };
}

test("Ctrl+End from the body goes to the end of the BODY, never the footer", () => {
  assert.deepEqual(storyBoundary(storyDoc(), "b1", { runningOpen: false }, true), { node: "b2", offset: 9 });
  assert.deepEqual(storyBoundary(storyDoc(), "b2", { runningOpen: false }, false), { node: "b1", offset: 0 });
});

test("a caret stranded in a footer outside header/footer mode is sent to the body", () => {
  assert.deepEqual(storyBoundary(storyDoc(), "f1", { runningOpen: false }, true), { node: "b2", offset: 9 });
});

test("an OPEN header or footer, or a note the caret is in, has its own edges", () => {
  assert.deepEqual(storyBoundary(storyDoc(), "f1", { runningOpen: true }, true), { node: "f2", offset: 7 });
  assert.deepEqual(storyBoundary(storyDoc(), "h1", { runningOpen: true }, false), { node: "h1", offset: 0 });
  assert.deepEqual(storyBoundary(storyDoc(), "n1", { runningOpen: false }, true), { node: "n1", offset: 3 });
});

/** Words of "alpha beta gamma" in p1 and "delta epsilon" in p2 (p1 first). */
function textDoc() {
  const text = { p1: "alpha beta gamma", p2: "delta epsilon" };
  const order = ["p1", "p2"];
  return {
    paragraphLength: (node) => text[node].length,
    wordAt(node, offset) {
      const s = text[node];
      if (/\s/.test(s[offset] ?? " ") && /\s/.test(s[offset - 1] ?? " ")) return [];
      let a = offset;
      while (a > 0 && !/\s/.test(s[a - 1])) a -= 1;
      let b = offset;
      while (b < s.length && !/\s/.test(s[b])) b += 1;
      return [a, b];
    },
    selectionEdge(an, ao, bn, bo, toEnd) {
      const first = order.indexOf(an) < order.indexOf(bn) ? { node: an, offset: ao } : { node: bn, offset: bo };
      const last = first.node === an ? { node: bn, offset: bo } : { node: an, offset: ao };
      return toEnd ? last : first;
    },
  };
}

test("presses are counted on PRESS, because pointerdown carries no click count", () => {
  const counter = createClickCounter();
  assert.equal(counter.press(10, 10, 0), 1);
  assert.equal(counter.press(11, 10, 200), 2, "a quick second press nearby");
  assert.equal(counter.press(11, 11, 400), 3);
  assert.equal(counter.press(11, 11, 500), 1, "after a triple, Word starts again");
  assert.equal(counter.press(11, 11, 1200), 1, "too slow");
  assert.equal(counter.press(40, 11, 1300), 1, "too far");
});

test("the browser's click count, once the click brings it, is what the next press counts from", () => {
  const counter = createClickCounter();
  counter.press(10, 10, 0);
  counter.settle(1);
  assert.equal(counter.press(10, 10, 150), 2);
  // The browser counted that second press as a new single click (its OS
  // interval is shorter, or the device said so): the next press is a double,
  // not a triple.
  counter.settle(1);
  assert.equal(counter.press(10, 10, 300), 2);
  counter.settle(2);
  assert.equal(counter.press(10, 10, 450), 3);
});

test("a unit the press guessed is undone when the browser counts fewer clicks, unless a drag grew it", () => {
  // A click straight after a double-click: the guess says triple, the browser
  // says 1 (Playwright, and an OS that does not count it on). A caret.
  assert.equal(pressUnitStands("paragraph", 1, false), false);
  assert.equal(pressUnitStands("word", 1, false), false);
  // The browser agrees: the press's unit stands.
  assert.equal(pressUnitStands("word", 2, false), true);
  assert.equal(pressUnitStands("paragraph", 3, false), true);
  // A different count is handed to the dblclick / triple-click routes.
  assert.equal(pressUnitStands("paragraph", 2, false), false);
  assert.equal(pressUnitStands("word", 3, false), false);
  // A drag already grew it by whole units: the gesture stands (UX-059).
  assert.equal(pressUnitStands("word", 1, true), true);
  // No unit, nothing to undo.
  assert.equal(pressUnitStands(null, 1, false), true);
});

test("a double press selects by word and a triple press by paragraph", () => {
  assert.equal(unitForClickCount(1), null);
  assert.equal(unitForClickCount(2), "word");
  assert.equal(unitForClickCount(3), "paragraph");
  const doc = textDoc();
  assert.deepEqual(unitRange(doc, { node: "p1", offset: 7 }, "word"), {
    start: { node: "p1", offset: 6 },
    end: { node: "p1", offset: 10 },
  });
  assert.deepEqual(unitRange(doc, { node: "p2", offset: 3 }, "paragraph"), {
    start: { node: "p2", offset: 0 },
    end: { node: "p2", offset: 13 },
  });
});

test("dragging after a double click grows whole words and keeps the first word", () => {
  const doc = textDoc();
  const origin = unitRange(doc, { node: "p1", offset: 7 }, "word"); // "beta"
  // Forward into "gamma": anchored at the start of "beta", focus at the end of "gamma".
  assert.deepEqual(extendByUnit(doc, origin, { node: "p1", offset: 12 }, "word"), {
    anchor: { node: "p1", offset: 6 },
    focus: { node: "p1", offset: 16 },
  });
  // Backward into "alpha": anchored at the END of "beta", so "beta" stays selected.
  assert.deepEqual(extendByUnit(doc, origin, { node: "p1", offset: 2 }, "word"), {
    anchor: { node: "p1", offset: 10 },
    focus: { node: "p1", offset: 0 },
  });
  // Across a paragraph: whole word in the next paragraph.
  assert.deepEqual(extendByUnit(doc, origin, { node: "p2", offset: 8 }, "word"), {
    anchor: { node: "p1", offset: 6 },
    focus: { node: "p2", offset: 13 },
  });
  assert.equal(isBefore(doc, { node: "p2", offset: 0 }, { node: "p1", offset: 5 }), false);
});

test("opening focuses the document only when that takes focus from no one", () => {
  const body = { parentElement: { tag: "html" } };
  const base = { framed: false, activeElement: body, body, editable: true, coarsePointer: false };
  assert.equal(shouldFocusOnOpen(base), true);
  assert.equal(shouldFocusOnOpen({ ...base, activeElement: null }), true);
  assert.equal(shouldFocusOnOpen({ ...base, framed: true }), false, "an embed never pulls focus from its host page");
  assert.equal(shouldFocusOnOpen({ ...base, activeElement: { id: "findInput" } }), false, "Find already has focus");
  assert.equal(shouldFocusOnOpen({ ...base, editable: false }), false, "nothing to type into");
  assert.equal(shouldFocusOnOpen({ ...base, coarsePointer: true }), false, "no soft keyboard on open");
});

test("arrow keys follow the text direction on screen", () => {
  assert.equal(strongDirection("ש"), "rtl");
  assert.equal(strongDirection("ع"), "rtl");
  assert.equal(strongDirection("a"), "ltr");
  assert.equal(strongDirection("1"), null);
  // Hebrew is two UTF-8 bytes a letter: byte offset 4 sits after the second letter.
  assert.deepEqual(charsAroundByteOffset("שלום", 4), { before: "ל", after: "ו" });
  assert.equal(caretInRtl({ before: "ל", after: "ו" }), true);
  assert.equal(caretInRtl({ before: "a", after: "b", baseRtl: true }), false);
  assert.equal(caretInRtl({ before: " ", after: "", baseRtl: true }), true, "neutral context takes the paragraph's");
  assert.equal(visualDirection("left", true), "right", "ArrowLeft in RTL text moves forward");
  assert.equal(visualDirection("wordRight", true), "wordLeft");
  assert.equal(visualDirection("left", false), "left");
  assert.equal(visualDirection("up", true), "up");
});

test("Enter on an empty list item leaves the list; on text it does not interfere", async () => {
  const calls = [];
  const ed = (length, level, kind = "bullet") => ({
    selection: { anchor: { node: "p", offset: 0 }, focus: { node: "p", offset: 0 } },
    hasRange: () => false,
    doc: {
      listStyleAt: () => kind,
      paragraphLength: () => length,
      listLevelAt: () => level,
      adjustListLevel: (...args) => calls.push(["adjust", args.at(-1)]),
      toggleList: (...args) => calls.push(["toggle", args.at(-1)]),
    },
    runToolbarEdit: async (thunk) => thunk("p", 0, "p", 0),
  });
  assert.equal(await exitEmptyListItem(ed(0, 0)), true);
  assert.deepEqual(calls.pop(), ["toggle", "bullet"], "outermost: the list ends");
  assert.equal(await exitEmptyListItem(ed(0, 2)), true);
  assert.deepEqual(calls.pop(), ["adjust", -1], "nested: one level out");
  assert.equal(await exitEmptyListItem(ed(5, 0)), false, "an item with text splits as usual");
  assert.equal(await exitEmptyListItem(ed(0, 0, "")), false, "not a list item");
  assert.equal(calls.length, 0);
});
