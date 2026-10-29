import assert from "node:assert/strict";
import test from "node:test";
import {
  ALIGN_CHOICES,
  createObjectAlign,
  defaultAlignTarget,
  effectiveAlignTarget,
} from "../src/object_align.mjs";

// RM-28: the Align submenu's policy. Mutations that drive these red: making
// `defaultAlignTarget` always "margin" fails the first test; sending only the
// primary node to the engine fails the third; consulting the engine once per
// row (eight calls) instead of twice fails the cost assertion.

test("Word's default target: the margin for one object, the selection for several", () => {
  assert.equal(defaultAlignTarget(1), "margin");
  assert.equal(defaultAlignTarget(2), "selected");
  assert.equal(effectiveAlignTarget("page", 1), "page", "a chosen target holds");
  assert.equal(effectiveAlignTarget("selected", 1), "margin", "the selection cannot be the target of one object");
  assert.equal(effectiveAlignTarget(null, 3), "selected");
});

function harness({ nodes = ["a", "b"], can = '{"can":true}', mutationEnabled = true } = {}) {
  const calls = { can: [], align: [], announced: [] };
  const doc = {
    canAlignObjects: (json, how, target) => {
      calls.can.push([JSON.parse(json), how, target]);
      return typeof can === "function" ? can(how) : can;
    },
    alignObjects: (json, how, target) => calls.align.push([JSON.parse(json), how, target]),
  };
  const align = createObjectAlign({
    doc: () => doc,
    nodes: () => nodes,
    runEdit: async (thunk) => {
      thunk();
      return true;
    },
    t: (key, params) => (params ? `${key}:${JSON.stringify(params)}` : key),
    explain: (raw) => `explained(${raw})`,
    announce: (text) => calls.announced.push(text),
    modifier: "Ctrl",
  });
  return { calls, rows: align.rows(mutationEnabled, "closed by mode"), align };
}

test("the menu offers the six alignments, two distributions and three targets, asking the engine twice", () => {
  // Three objects: distribution needs three, so with two it is closed by the
  // count rule before the engine is asked, and only one read would happen.
  const { rows, calls } = harness({ nodes: ["a", "b", "c"] });
  assert.deepEqual(
    rows.map((row) => row.id),
    [
      ...ALIGN_CHOICES.map((choice) => `object.align.${choice.how}`),
      "object.align.to.page",
      "object.align.to.margin",
      "object.align.to.selected",
    ],
  );
  assert.equal(calls.can.length, 2, "one read for the alignments, one for the distributions");
  assert.equal(rows.find((row) => row.id === "object.align.to.selected").shortcut, "✓", "several objects: the selection");
});

test("a row runs the engine over EVERY held object, against the target in force, and says so", async () => {
  const { rows, calls } = harness({ nodes: ["a", "b", "c"] });
  await rows.find((row) => row.id === "object.align.middle").run();
  await rows.find((row) => row.id === "object.align.distributeVertically").run();
  assert.deepEqual(calls.align, [
    [["a", "b", "c"], "middle", "selected"],
    [["a", "b", "c"], "distributeVertically", "selected"],
  ]);
  assert.deepEqual(calls.announced, ["object.align.aligned", "object.align.distributed"]);
});

test("picking a target keeps it for the next alignment", async () => {
  const h = harness();
  h.rows.find((row) => row.id === "object.align.to.page").run();
  const again = h.align.rows(true, "");
  assert.equal(again.find((row) => row.id === "object.align.to.page").shortcut, "✓");
  await again.find((row) => row.id === "object.align.left").run();
  assert.deepEqual(h.calls.align.at(-1), [["a", "b"], "left", "page"]);
});

test("closed rows carry a reason: the count rule says what to do, the engine says the rest", () => {
  const one = harness({ nodes: ["a"] });
  const distribute = one.rows.find((row) => row.id === "object.align.distributeHorizontally");
  assert.equal(distribute.enabled, false);
  assert.match(distribute.disabledReason, /^object\.align\.reason\.selectThree/);
  assert.equal(one.rows.find((row) => row.id === "object.align.to.selected").enabled, false);
  assert.equal(one.rows.find((row) => row.id === "object.align.left").enabled, true, "one object aligns to the margin");

  const engine = harness({ can: '{"can":false,"reason":"refused: not floating\\u001fobject.align-not-floating"}' });
  const left = engine.rows.find((row) => row.id === "object.align.left");
  assert.equal(left.enabled, false);
  assert.match(left.disabledReason, /^explained\(refused: not floating/);

  const viewing = harness({ mutationEnabled: false });
  for (const row of viewing.rows.filter((r) => !r.id.startsWith("object.align.to."))) {
    assert.equal(row.enabled, false, row.id);
    assert.equal(row.disabledReason, "closed by mode", row.id);
  }
});
