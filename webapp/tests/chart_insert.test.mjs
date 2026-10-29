import assert from "node:assert/strict";
import test from "node:test";
import {
  DEFAULT_CHART_KIND,
  chartInsertFailure,
  insertChartAtCaret,
  insertChartTypeCommands,
} from "../src/chart_insert.mjs";
import { CHART_TYPE_GROUPS } from "../src/chart_commands.mjs";

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

// HF-258: the Insert ▸ Chart type picker. Mutation: dropping the `kind` default
// parameter's use (inserting DEFAULT_CHART_KIND whatever was picked) fails the
// first test; building the picker from a hand list that omits a family fails
// the second.
test("a picked type reaches the engine as that type, not as the default", async () => {
  const h = harness();
  await insertChartAtCaret({ ...h.deps, kind: "pie" });
  assert.deepEqual(h.calls.insert, [["n1", 3, "pie"]]);
});

test("the insert picker offers every type the Change-type menu offers, grouped by family, and each row inserts its own type", () => {
  const picked = [];
  const rows = insertChartTypeCommands((key) => `<${key}>`, (kind) => picked.push(kind));
  const expected = CHART_TYPE_GROUPS.flatMap((group) => group.kinds);
  assert.deepEqual(rows.map((row) => row.id), expected.map((kind) => `insert.chart.${kind}`));
  assert.ok(rows.every((row) => row.enabled && row.label.startsWith("<chart.kind.")), "every row is labelled from the catalogue");
  assert.equal(new Set(rows.map((row) => row.group)).size, CHART_TYPE_GROUPS.length, "one menu group per family");
  for (const row of rows) row.run();
  assert.deepEqual(picked, expected);
});
