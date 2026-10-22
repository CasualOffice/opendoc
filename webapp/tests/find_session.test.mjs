// The Find session (docs/174 TA-03, TA-04): typing the query refines from a
// fixed anchor and never advances; Next from a caret goes to the next match
// after it; every match is painted, the current one distinct, from a cache
// that is dropped when the document changes.
import assert from "node:assert/strict";
import test from "node:test";
import {
  FIND_CURRENT_CLASS,
  FIND_MATCH_CLASS,
  createFindSession,
  firstMatchAtOrAfter,
  lastMatchBefore,
  stepIndex,
} from "../src/find_session.mjs";

// One paragraph "fox1 fox2 fox3 fox4": matches at byte offsets 0, 5, 10, 15.
const ORDER = ["p1", "p2"];
const posLE = (aNode, aOff, bNode, bOff) =>
  aNode === bNode ? aOff <= bOff : ORDER.indexOf(aNode) < ORDER.indexOf(bNode);
const fox = [0, 5, 10, 15].map((o) => ({ startNode: "p1", startOffset: o, endNode: "p1", endOffset: o + 3 }));

function fakeDoc() {
  const calls = { rects: 0 };
  return {
    calls,
    selectionEdge(aNode, aOff, bNode, bOff) {
      const aFirst = posLE(aNode, aOff, bNode, bOff);
      return { node: aFirst ? aNode : bNode, offset: aFirst ? aOff : bOff, free() {} };
    },
    selectionRects(node, start, _endNode, end) {
      calls.rects++;
      return [1, start * 100, 0, (end - start) * 100, 200];
    },
  };
}

function harness({ selection = null, implicit = false } = {}) {
  const doc = fakeDoc();
  const state = { selection, implicit, key: "0:1", active: true, placed: [], repaints: 0, rescans: 0, timers: [] };
  const session = createFindSession({
    getDoc: () => doc,
    getSelection: () => state.selection,
    isImplicit: () => state.implicit,
    place: (flat, kind) => state.placed.push([flat[1], kind]),
    revisionKey: () => state.key,
    active: () => state.active,
    repaint: () => state.repaints++,
    rescan: () => state.rescans++,
    schedule: (fn) => state.timers.push(fn),
  });
  return { doc, state, session };
}

const caret = (node, offset) => ({ anchor: { node, offset }, focus: { node, offset } });
const select = (m) => ({
  anchor: { node: m.startNode, offset: m.startOffset },
  focus: { node: m.endNode, offset: m.endOffset },
});

test("firstMatchAtOrAfter: at, between, past the end (wraps) and from the top", () => {
  assert.equal(firstMatchAtOrAfter(fox, { node: "p1", offset: 5 }, posLE), 1);
  assert.equal(firstMatchAtOrAfter(fox, { node: "p1", offset: 6 }, posLE), 2);
  assert.equal(firstMatchAtOrAfter(fox, { node: "p2", offset: 0 }, posLE), 0);
  assert.equal(firstMatchAtOrAfter(fox, null, posLE), 0);
});

test("lastMatchBefore: strictly before, and wraps to the last", () => {
  assert.equal(lastMatchBefore(fox, { node: "p1", offset: 10 }, posLE), 1);
  assert.equal(lastMatchBefore(fox, { node: "p1", offset: 0 }, posLE), 3);
  assert.equal(lastMatchBefore(fox, null, posLE), 3);
});

test("stepIndex: an incremental step ignores the current match and stays on the anchor's", () => {
  // The defect: on "fo" the selection is fox1 (current 0), and typing "x" stepped to fox2.
  assert.equal(stepIndex(fox, { incremental: true, current: 0, anchor: { node: "p1", offset: 0 } }, posLE), 0);
  assert.equal(stepIndex(fox, { current: 0, forward: true }, posLE), 1);
  assert.equal(stepIndex(fox, { current: 0, forward: false }, posLE), 3);
  assert.equal(stepIndex([], { incremental: true }, posLE), -1);
});

test("typing f, fo, fox from a caret at the start stays on 1 of 4", () => {
  const { state, session } = harness({ selection: caret("p1", 0) });
  session.anchorHere();
  for (let keystroke = 0; keystroke < 3; keystroke++) {
    const index = session.pick(fox, true, true);
    assert.equal(index, 0, `keystroke ${keystroke + 1} moved off the first match`);
    state.selection = select(fox[index]); // what selectTextMatch does
  }
});

test("typing refines from where Find opened, not from the top", () => {
  const { state, session } = harness({ selection: caret("p1", 7) });
  session.anchorHere();
  assert.equal(session.pick(fox, true, true), 2);
  state.selection = select(fox[2]);
  assert.equal(session.pick(fox, true, true), 2);
});

test("Next steps on, and moves the anchor so typing after it refines from there", () => {
  const { state, session } = harness({ selection: caret("p1", 0) });
  session.anchorHere();
  state.selection = select(fox[session.pick(fox, true, true)]);
  const next = session.pick(fox, true, false);
  assert.equal(next, 1);
  state.selection = select(fox[next]);
  assert.deepEqual(session.anchor(), { node: "p1", offset: 5 });
  assert.equal(session.pick(fox, true, true), 1);
});

test("Next from a caret goes to the next match after it (Replace leaves the caret after its text)", () => {
  const { session } = harness({ selection: caret("p1", 8) });
  assert.equal(session.pick(fox, true, false), 2);
  const back = harness({ selection: caret("p1", 8) });
  assert.equal(back.session.pick(fox, false, false), 1);
});

test("the untouched load-time caret counts as the top", () => {
  const { session } = harness({ selection: caret("p1", 12), implicit: true });
  session.anchorHere();
  assert.equal(session.anchor(), null);
  assert.equal(session.pick(fox, true, true), 0);
  assert.equal(session.pick(fox, true, false), 0);
});

test("a backwards selection anchors at its document-order start", () => {
  const { state, session } = harness();
  state.selection = { anchor: { node: "p2", offset: 3 }, focus: { node: "p1", offset: 6 } };
  session.anchorHere();
  assert.deepEqual(session.anchor(), { node: "p1", offset: 6 });
});

test("paint draws every match once, the current one distinct", async () => {
  const { state, session } = harness({ selection: select(fox[2]) });
  assert.equal(session.show(fox), 2);
  session.paint();
  assert.deepEqual(state.placed, [
    [0, FIND_MATCH_CLASS],
    [500, FIND_MATCH_CLASS],
    [1000, FIND_CURRENT_CLASS],
    [1500, FIND_MATCH_CLASS],
  ]);
  await Promise.resolve();
  assert.equal(state.repaints, 0, "a paint already drew the shown list; no second repaint");
});

test("show without a paint schedules exactly one repaint", async () => {
  const { state, session } = harness();
  session.show(fox);
  session.show(fox);
  await Promise.resolve();
  assert.equal(state.repaints, 1);
});

test("rects are asked once per list, not once per repaint", () => {
  const { doc, session } = harness();
  session.show(fox);
  session.paint();
  session.paint();
  session.paint();
  assert.equal(doc.calls.rects, fox.length);
});

test("an edit makes the highlight stale: nothing drawn, one deferred rescan", () => {
  const { state, session } = harness();
  session.show(fox);
  state.key = "0:2";
  session.paint();
  session.paint();
  assert.deepEqual(state.placed, []);
  assert.equal(state.timers.length, 1, "one rescan per pause, not per keystroke");
  state.timers[0]();
  assert.equal(state.rescans, 1);
});

test("closed Find paints nothing, and clear drops the list", async () => {
  const { state, session } = harness();
  session.show(fox);
  state.active = false;
  session.paint();
  assert.deepEqual(state.placed, []);
  state.active = true;
  session.clear();
  session.paint();
  assert.deepEqual(state.placed, []);
});
