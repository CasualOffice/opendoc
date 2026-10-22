// One gesture, one engine action (`docs/109` OL-04).
//
// A wrap chip pressed on an in-line picture ran TWO engine calls (float it,
// then set the wrap); the host repaints only the pages the LAST result names,
// so the first call's reflow never reached the screen until the picture was
// dragged, and one gesture took two Undos.
import assert from "node:assert/strict";
import test from "node:test";

const { combineEditResults, runWrapPlan, wrapPlan } = await import("../src/object_arrange.mjs");

test("an in-line object goes into a wrap in ONE engine call that carries the wrap", () => {
  const calls = [];
  const doc = {
    setObjectAnchorKind: (...args) => (calls.push(["anchorKind", ...args]), "result"),
    setObjectWrap: (...args) => (calls.push(["wrap", ...args]), "result"),
  };
  const inline = { object: true, floating: false, groupChild: false };
  assert.equal(runWrapPlan(doc, "n1", wrapPlan("tight", inline, null)), "result");
  assert.deepEqual(calls, [["anchorKind", "n1", "floating", "tight"]]);

  calls.length = 0;
  runWrapPlan(doc, "n1", wrapPlan("inline", { object: true, floating: true, groupChild: false }, "square"));
  assert.deepEqual(calls, [["anchorKind", "n1", "inline", undefined]]);

  calls.length = 0;
  runWrapPlan(doc, "n1", wrapPlan("behind", { object: true, floating: true, groupChild: false }, "square"));
  assert.deepEqual(calls, [["wrap", "n1", "behind"]]);

  calls.length = 0;
  assert.equal(runWrapPlan(doc, "n1", []), null);
  assert.deepEqual(calls, [], "an empty plan runs nothing");
});

test("two engine results for one gesture repaint the union of their pages", () => {
  let freed = 0;
  const result = (pages, revision) => ({
    node: `n${revision}`,
    offset: 0,
    dirtyPages: Uint32Array.from(pages),
    pageCount: 4,
    revision,
    free: () => freed++,
  });
  const combined = combineEditResults(result([1, 2], 7), result([2, 3], 8));
  assert.deepEqual(combined.dirtyPages, [1, 2, 3], "the first action's page 1 is not lost");
  assert.equal(combined.revision, 8, "the later action's revision");
  assert.equal(combined.node, "n8");
  assert.equal(freed, 2, "both engine results are freed");
  combined.free();
});
