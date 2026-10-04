import assert from "node:assert/strict";
import test from "node:test";
import { smallestContaining } from "../src/review_anchor.mjs";

// REVIEW-GAP-019's tie-break: several review ranges can stack on one caret, and
// the card that expands must be the innermost one. Setting that up through the
// chrome is hard, which is why the rule was unasserted.
const index = [
  { node: "n1", start: 0, end: 100, itemId: "outer" },
  { node: "n1", start: 10, end: 20, itemId: "inner" },
  { node: "n1", start: 5, end: 60, itemId: "middle" },
  { node: "n2", start: 10, end: 20, itemId: "other-node" },
];

test("the SMALLEST containing range wins when several stack", () => {
  assert.equal(smallestContaining(index, "n1", 15)?.itemId, "inner");
});

test("a caret outside the inner range falls to the next smallest that contains it", () => {
  assert.equal(smallestContaining(index, "n1", 50)?.itemId, "middle");
  assert.equal(smallestContaining(index, "n1", 80)?.itemId, "outer");
});

test("a range in another node never matches, however well its offsets fit", () => {
  assert.equal(smallestContaining(index, "n3", 15), null);
  assert.equal(smallestContaining(index, "n2", 15)?.itemId, "other-node");
});

test("the bounds are inclusive at both ends", () => {
  assert.equal(smallestContaining(index, "n1", 10)?.itemId, "inner");
  assert.equal(smallestContaining(index, "n1", 20)?.itemId, "inner");
  assert.equal(smallestContaining([index[1]], "n1", 21), null);
});

test("an empty index is null, not a throw", () => {
  assert.equal(smallestContaining([], "n1", 0), null);
});
