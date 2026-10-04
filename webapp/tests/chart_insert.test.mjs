import assert from "node:assert/strict";
import test from "node:test";
import {
  DEFAULT_CHART_KIND,
  chartInsertFailure,
  insertChartAtCaret,
} from "../src/chart_insert.mjs";

// The whole insert PATH, refusals included — which is the point of injecting
// `main.js`'s state rather than reaching for it. `insertChart` existed in the
// engine since #761 with no surface, so these assert the surface, not the engine.
const harness = (over = {}) => {
  const calls = { insert: [], status: [], applied: [] };
  const doc = {
    insertChart(node, offset, kind) {
      calls.insert.push([node, offset, kind]);
      return { node, offset, kind };
    },
  };
  return {
    calls,
    deps: {
      doc,
      caret: { node: "n1", offset: 3 },
      blocked: () => false,
      suggesting: () => false,
      status: (text, kind) => calls.status.push([text, kind]),
      apply: async (r) => calls.applied.push(r),
      ...over,
    },
  };
};

test("Word's default is what a first Insert ▸ Chart gives you", () => {
  assert.equal(DEFAULT_CHART_KIND, "column", "Word's Insert ▸ Chart default is a clustered column");
});

test("the caret's node and offset reach the engine, with the default family", async () => {
  const h = harness();
  await insertChartAtCaret(h.deps);
  assert.deepEqual(h.calls.insert, [["n1", 3, "column"]]);
  assert.equal(h.calls.applied.length, 1, "the result must be applied, not dropped");
});

test("viewing mode refuses, and does so SILENTLY — the control is already disabled there", async () => {
  const h = harness({ blocked: () => true });
  await insertChartAtCaret(h.deps);
  assert.deepEqual(h.calls.insert, []);
  assert.deepEqual(h.calls.status, [], "a mode that disables the control must not also shout");
});

test("suggesting mode refuses and NAMES the mode to switch to", async () => {
  const h = harness({ suggesting: () => true });
  await insertChartAtCaret(h.deps);
  assert.deepEqual(h.calls.insert, [], "nothing may reach the engine");
  assert.match(h.calls.status[0][0], /cannot be tracked yet; switch to Editing/);
  assert.equal(h.calls.status[0][1], "error");
});

test("no caret and no document are both refusals, not crashes", async () => {
  for (const over of [{ caret: null }, { caret: undefined }, { doc: null }]) {
    const h = harness(over);
    await insertChartAtCaret(h.deps);
    assert.deepEqual(h.calls.insert, []);
  }
});

test("an engine refusal reaches the reader as the engine's OWN sentence", async () => {
  const h = harness();
  h.deps.doc.insertChart = () => {
    throw new Error("This build draws bar, column, line, area, scatter, pie and doughnut charts");
  };
  await insertChartAtCaret(h.deps);
  assert.match(h.calls.status[0][0], /bar, column, line, area, scatter, pie and doughnut/,
    "the engine already phrases this refusal; the host must not replace it with its own");
});

test("a failure with no message still says something", () => {
  assert.equal(chartInsertFailure(new Error("")), "Could not insert the chart.");
  assert.equal(chartInsertFailure(undefined), "Could not insert the chart.");
  assert.equal(chartInsertFailure("boom"), "boom");
});
