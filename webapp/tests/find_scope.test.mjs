import assert from "node:assert/strict";
import test from "node:test";
import { matchWithinScope } from "../src/find_scope.mjs";

// Ordering two positions in DIFFERENT blocks is the engine's answer, so the
// comparator is injected. Here it is a total order over a node list, which is
// what `selectionEdge` computes for real.
const order = ["a", "b", "c", "d"];
const posLE = (aNode, aOff, bNode, bOff) =>
  aNode === bNode ? aOff <= bOff : order.indexOf(aNode) < order.indexOf(bNode);

const scope = { startNode: "b", startOffset: 5, endNode: "c", endOffset: 7 };

test("a match starting before the scope's offset in the SAME node is out", () => {
  assert.equal(matchWithinScope({ startNode: "b", startOffset: 4, endNode: "b", endOffset: 6 }, scope, posLE), false);
});

test("a match at or after the scope's start offset is in", () => {
  assert.equal(matchWithinScope({ startNode: "b", startOffset: 5, endNode: "b", endOffset: 9 }, scope, posLE), true);
});

test("a single-node scope caps the UPPER bound too", () => {
  const one = { startNode: "b", startOffset: 2, endNode: "b", endOffset: 6 };
  assert.equal(matchWithinScope({ startNode: "b", startOffset: 3, endNode: "b", endOffset: 6 }, one, posLE), true);
  assert.equal(matchWithinScope({ startNode: "b", startOffset: 3, endNode: "b", endOffset: 7 }, one, posLE), false);
});

test("a match in the scope's END node is bounded by the end offset", () => {
  assert.equal(matchWithinScope({ startNode: "c", startOffset: 0, endNode: "c", endOffset: 7 }, scope, posLE), true);
  assert.equal(matchWithinScope({ startNode: "c", startOffset: 0, endNode: "c", endOffset: 8 }, scope, posLE), false);
});

test("an interior node is in scope, and a node outside the range is not", () => {
  // There is no node between b and c in this fixture, so widen the scope.
  const wide = { startNode: "a", startOffset: 0, endNode: "d", endOffset: 9 };
  assert.equal(matchWithinScope({ startNode: "b", startOffset: 1, endNode: "b", endOffset: 2 }, wide, posLE), true);
  assert.equal(matchWithinScope({ startNode: "b", startOffset: 1, endNode: "b", endOffset: 2 }, scope, posLE), false,
    "node b IS the scope's start node, so the start-offset rule applies, not the interior rule");
});
