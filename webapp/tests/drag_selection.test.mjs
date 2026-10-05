// What a pointer drag selects: granularity, and the cost of asking.
//
// The behaviour under test is a MOUSE gesture, so the browser spec in
// `tests/e2e/drag-selection-granularity.spec.mjs` is what proves a reader can
// feel it. These are the parts Playwright cannot assert without a stopwatch:
// the direction rule at both ends, the fallback when the engine declines to
// bound a word, and — the one that matters most — that the number of
// O(document) engine calls a drag makes is bounded by the WORDS it crosses and
// not by how many times the pointer moved.
//
// The fake engine below is the real call shape, `selectionEdge`'s `free()`
// included, because `createGranularityCache` owns those three calls now and a
// test against an invented shape would not notice if it stopped matching.
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  CHARACTER,
  PARAGRAPH,
  WORD,
  autoScrollDelta,
  autoScrollStep,
  createGranularityCache,
  granularEnds,
  granularityForClickCount,
  paragraphEnds,
} from "../src/drag_selection.mjs";

// "Alpha beta gamma" as one paragraph, and "Delta epsilon" as the next.
const TEXT = { p1: "Alpha beta gamma", p2: "Delta epsilon" };
const ORDER = ["p1", "p2"];

/** The engine's own answers, counted. `wordAt` mirrors the Rust `word_bounds`
 *  contract: `[start, end]` of the word containing `offset`, and `[]` when the
 *  offset is not inside one (a run of spaces, or past the last word). */
function fakeEngine() {
  const calls = { wordAt: 0, paragraphLength: 0, selectionEdge: 0 };
  const doc = {
    wordAt(node, offset) {
      calls.wordAt += 1;
      const text = TEXT[node] ?? "";
      if (offset >= text.length || /\s/.test(text[offset] ?? " ")) return [];
      let start = offset;
      while (start > 0 && !/\s/.test(text[start - 1])) start -= 1;
      let end = offset;
      while (end < text.length && !/\s/.test(text[end])) end += 1;
      return [start, end];
    },
    paragraphLength(node) {
      calls.paragraphLength += 1;
      return (TEXT[node] ?? "").length;
    },
    selectionEdge(aNode, aOffset, bNode, bOffset, towardEnd) {
      calls.selectionEdge += 1;
      const a = [ORDER.indexOf(aNode), aOffset];
      const b = [ORDER.indexOf(bNode), bOffset];
      const aFirst = a[0] < b[0] || (a[0] === b[0] && a[1] <= b[1]);
      const [node, offset] = (aFirst ? towardEnd : !towardEnd)
        ? [bNode, bOffset]
        : [aNode, aOffset];
      let freed = false;
      return {
        node,
        offset,
        free() {
          freed = true;
          doc.freed = freed;
        },
      };
    },
  };
  return { doc, calls, io: createGranularityCache(() => doc) };
}

const at = (node, offset) => ({ node, offset });

// ---- The click-count rule ----------------------------------------------------

test("the click count picks the granularity, ONLYOFFICE's modular rule", () => {
  // Document.js:10489-10494 — one click is Common, an even count is Word, an
  // odd count above one is Paragraph. The alternation is the behaviour, not a
  // ">= 3 means paragraph" approximation: a fourth click returns to words.
  assert.equal(granularityForClickCount(1), CHARACTER);
  assert.equal(granularityForClickCount(2), WORD);
  assert.equal(granularityForClickCount(3), PARAGRAPH);
  assert.equal(granularityForClickCount(4), WORD);
  assert.equal(granularityForClickCount(5), PARAGRAPH);
});

test("a detail the browser never supplies is a character drag, not a crash", () => {
  for (const value of [0, -1, undefined, null, NaN, "2"]) {
    assert.equal(granularityForClickCount(value), CHARACTER, `detail ${String(value)}`);
  }
});

// ---- Word granularity --------------------------------------------------------

test("a character drag is left exactly where the pointer is", () => {
  const { io, calls } = fakeEngine();
  const ends = granularEnds(
    { anchor: at("p1", 2), focus: at("p1", 9), granularity: CHARACTER },
    io,
  );
  assert.deepEqual(ends, { anchor: at("p1", 2), focus: at("p1", 9) });
  assert.deepEqual(calls, { wordAt: 0, paragraphLength: 0, selectionEdge: 0 },
    "a plain drag must not pay for granularity it does not use");
});

test("a forward word drag covers whole words at BOTH ends", () => {
  // Press inside "Alpha" (offset 2), pointer inside "gamma" (offset 13).
  // "Alpha beta gamma" — the selection must be 0..16, not 2..13.
  const { io } = fakeEngine();
  const ends = granularEnds(
    { anchor: at("p1", 2), focus: at("p1", 13), granularity: WORD },
    io,
  );
  assert.deepEqual(ends, { anchor: at("p1", 0), focus: at("p1", 16) });
});

test("a backward word drag grows the anchor the other way", () => {
  // The ONLYOFFICE rule `checkWordSelection` encodes (Paragraph.js:8606-8626):
  // running backward, the anchor snaps to its word's END and the focus to its
  // word's START, so the pair still covers both whole words.
  const { io } = fakeEngine();
  const ends = granularEnds(
    { anchor: at("p1", 13), focus: at("p1", 2), granularity: WORD },
    io,
  );
  assert.deepEqual(ends, { anchor: at("p1", 16), focus: at("p1", 0) });
});

test("a word drag that has not left its word still selects the whole word", () => {
  const { io } = fakeEngine();
  const ends = granularEnds(
    { anchor: at("p1", 2), focus: at("p1", 3), granularity: WORD },
    io,
  );
  assert.deepEqual(ends, { anchor: at("p1", 0), focus: at("p1", 5) }, "Alpha");
});

test("an offset the engine will not bound keeps the raw pointer position", () => {
  // Offset 5 is the space after "Alpha": `word_bounds` answers `[]`. Inventing
  // a boundary would jump the selection somewhere the pointer never was, so
  // that end stays character-granular and the other end is still a whole word.
  const { io } = fakeEngine();
  const ends = granularEnds(
    { anchor: at("p1", 7), focus: at("p1", 5), granularity: WORD },
    io,
  );
  assert.deepEqual(ends, { anchor: at("p1", 10), focus: at("p1", 5) });
});

test("a word drag across paragraphs asks the ENGINE for the direction", () => {
  const { io, calls } = fakeEngine();
  const ends = granularEnds(
    { anchor: at("p1", 7), focus: at("p2", 8), granularity: WORD },
    io,
  );
  // "beta" starts at 6 in p1; "epsilon" ends at 13 in p2.
  assert.deepEqual(ends, { anchor: at("p1", 6), focus: at("p2", 13) });
  assert.equal(calls.selectionEdge, 1, "offsets cannot order two paragraphs");
});

test("the selectionEdge handle is freed, every time", () => {
  const { io, doc } = fakeEngine();
  granularEnds({ anchor: at("p1", 7), focus: at("p2", 8), granularity: WORD }, io);
  assert.equal(doc.freed, true, "a leaked wasm handle is a leak per pointer move");
});

// ---- Paragraph granularity ---------------------------------------------------

test("a forward paragraph drag runs from the first paragraph's start to the last one's end", () => {
  const { io } = fakeEngine();
  const ends = granularEnds(
    { anchor: at("p1", 7), focus: at("p2", 4), granularity: PARAGRAPH },
    io,
  );
  assert.deepEqual(ends, { anchor: at("p1", 0), focus: at("p2", 13) });
});

test("a backward paragraph drag reverses both ends", () => {
  const { io } = fakeEngine();
  const ends = granularEnds(
    { anchor: at("p2", 4), focus: at("p1", 7), granularity: PARAGRAPH },
    io,
  );
  assert.deepEqual(ends, { anchor: at("p2", 13), focus: at("p1", 0) });
});

test("a bare triple-click selects the paragraph it landed in", () => {
  const { io } = fakeEngine();
  assert.deepEqual(paragraphEnds("p1", io), { anchor: at("p1", 0), focus: at("p1", 16) });
});

// ---- The cost of asking: the complexity guard --------------------------------
//
// `docs/107` §4: per-interaction work is O(1) in document size. Every engine
// call here is O(document) today (`paragraph_text` collects every paragraph on
// every surface), so the property that has to hold is that a pointer MOVE does
// not make one. These count calls rather than milliseconds, per SKILL §8: a
// timing threshold cannot tell a slow constant from a quadratic, and a count
// cannot be passed by a fast machine.

test("a drag inside one word costs the engine nothing after the first move", () => {
  const { io, calls } = fakeEngine();
  const anchor = at("p1", 2);
  // 60 pointer moves, all still inside "Alpha" (0..5) and "Alpha" again.
  for (let i = 0; i < 60; i += 1) {
    granularEnds({ anchor, focus: at("p1", 1 + (i % 4)), granularity: WORD }, io);
  }
  assert.equal(io.engineCalls(), 2, "one word per end, then the memo answers");
  assert.equal(calls.wordAt, 2);
  assert.equal(calls.selectionEdge, 0, "one paragraph needs no ordering call");
});

test("the engine is asked once per WORD crossed, not once per pointer move", () => {
  // The doubling shape (SKILL §8): the same three words crossed with 3 moves
  // and with 120 moves must cost the engine the same, because the cost is the
  // gesture's, not the frame rate's.
  function cost(moves) {
    const { io } = fakeEngine();
    const anchor = at("p1", 2);
    const stops = [2, 7, 13]; // Alpha, beta, gamma
    for (let i = 0; i < moves; i += 1) {
      granularEnds(
        { anchor, focus: at("p1", stops[Math.floor((i * stops.length) / moves)]), granularity: WORD },
        io,
      );
    }
    return io.engineCalls();
  }
  const few = cost(3);
  const many = cost(120);
  assert.equal(many, few, `3 moves cost ${few} calls, 120 moves cost ${many}`);
  assert.ok(few <= 4, `crossing three words must not cost more than four calls, cost ${few}`);
});

test("a paragraph drag costs the same whether it moves 4 times or 40", () => {
  function cost(moves) {
    const { io, calls } = fakeEngine();
    const anchor = at("p1", 7);
    for (let i = 0; i < moves; i += 1) {
      granularEnds({ anchor, focus: at("p2", i % 13), granularity: PARAGRAPH }, io);
    }
    return calls;
  }
  const few = cost(4);
  const many = cost(40);
  assert.deepEqual(many, few, `4 moves ${JSON.stringify(few)}, 40 moves ${JSON.stringify(many)}`);
  // A forward drag needs only the FOCUS paragraph's length — the anchor end is
  // offset 0 — so one length call and one ordering call is the whole bill.
  assert.equal(many.paragraphLength, 1);
  assert.equal(many.selectionEdge, 1, "the pair's order is settled once");
});

test("reset drops the memo, because the next gesture may follow an edit", () => {
  const { io, calls } = fakeEngine();
  const anchor = at("p1", 2);
  granularEnds({ anchor, focus: at("p1", 3), granularity: WORD }, io);
  const before = calls.wordAt;
  io.reset();
  assert.equal(io.engineCalls(), 0, "the counter is part of the gesture");
  granularEnds({ anchor, focus: at("p1", 3), granularity: WORD }, io);
  assert.ok(calls.wordAt > before, "a stale word boundary would survive an edit");
});

// ---- Auto-scroll -------------------------------------------------------------

test("the auto-scroll band only acts within 56px of an edge", () => {
  assert.equal(autoScrollStep(500, 0, 1000), 0, "the middle does not scroll");
  assert.equal(autoScrollStep(56, 0, 1000), 0, "exactly at the band's inner edge");
  assert.equal(autoScrollStep(944, 0, 1000), 0);
});

test("the auto-scroll ramp accelerates toward the edge and clamps", () => {
  const nearTop = autoScrollStep(40, 0, 1000);
  const atTop = autoScrollStep(0, 0, 1000);
  const pastTop = autoScrollStep(-500, 0, 1000);
  assert.ok(nearTop < 0 && atTop < nearTop, `${atTop} must be faster than ${nearTop}`);
  assert.equal(atTop, -24, "clamped to AUTO_SCROLL_MAX_PX");
  assert.equal(pastTop, -24, "a pointer dragged off the window does not teleport");
  assert.equal(autoScrollStep(1000, 0, 1000), 24, "and symmetrically at the other edge");
});

test("both axes come from the one rule, so neither can be forgotten", () => {
  // The horizontal half genuinely did not exist once, and at a zoom where the
  // sheet is wider than the window the end of a line was unreachable by mouse.
  const rect = { top: 0, bottom: 1000, left: 0, right: 800 };
  assert.deepEqual(autoScrollDelta(rect, 400, 500), { dx: 0, dy: 0 });
  assert.deepEqual(autoScrollDelta(rect, 800, 1000), { dx: 24, dy: 24 });
  assert.deepEqual(autoScrollDelta(rect, 0, 0), { dx: -24, dy: -24 });
});
