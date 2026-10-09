// The diff canvas (ADR-065), driven in node: how a redline is built and owned,
// and how its changes become the list a reader steps through.
//
// The browser half (`version-diff-canvas.spec.mjs`, `compare.spec.mjs`) paints a
// real redline. What it cannot do cheaply is enumerate the shapes the engine's
// revision list can take — a replacement's two halves, a move's two ends, a
// version that carries its own suggestions — which is what these do.
import assert from "node:assert/strict";
import test from "node:test";

import { CANCELLED, COMPLETE, buildRedline } from "../src/compare_documents.mjs";
import { ADDED, FORMATTED, MOVED, REMOVED, entryLabel, entryText, legendCounts, redlineEntries } from "../src/diff_canvas.mjs";
import { EN_STRINGS } from "../src/en_strings.mjs";
import { setCatalogue, setLocale, t } from "../src/i18n.mjs";

setCatalogue("en", { ...EN_STRINGS });
setLocale("en");

const STAMP = { author: "Ann", date: "2026-10-09T10:00:00.000Z" };

/** One `listRevisions()` row, stamped as the redline stamps its own. */
function revision(id, kind, node, start, text, extra = {}) {
  return { id, kind, author: STAMP.author, date: STAMP.date, text, anchor: { node, start, end: start + text.length }, ...extra };
}

// ── buildRedline: ownership ─────────────────────────────────────────────────

/** A fake engine that finishes in one slice and records what it was asked. */
function fakeEngine({ paint = () => '{"anchors":{},"placed":{},"order":[],"unmarked":[]}', phases = [COMPLETE] } = {}) {
  const calls = { begin: null, opened: null, painted: null, jobFreed: false, viewFreed: false };
  const job = {
    step: () => phases.shift() ?? COMPLETE,
    result: () => '{"changes":[],"complete":true}',
    cancel() {},
    blocksProjected: () => 0,
    blocksTotal: () => 0,
    free() {
      calls.jobFreed = true;
    },
  };
  const engine = {
    begin(older, newer) {
      calls.begin = [older, newer];
      return job;
    },
    slice: () => 4000,
    open(bytes) {
      calls.opened = bytes;
      return {
        showComparison(handle, author, date) {
          calls.painted = { handle, author, date };
          return paint();
        },
        free() {
          calls.viewFreed = true;
        },
      };
    },
  };
  return { engine, calls, job };
}

const OLDER = new Uint8Array([1]);
const NEWER = new Uint8Array([2]);

test("a redline compares older → newer, paints into a copy of the NEWER side, and frees the job", async () => {
  const { engine, calls, job } = fakeEngine();
  const result = await buildRedline({ engine, older: OLDER, newer: NEWER, ...STAMP, yieldToHost: () => Promise.resolve() });
  assert.equal(result.ok, true);
  assert.deepEqual(calls.begin, [OLDER, NEWER], "older is LEFT: an insertion is what the newer side has");
  assert.equal(calls.opened, NEWER, "the view is the newer side's own bytes");
  assert.equal(calls.painted.handle, job, "painted from the finished comparison's handle");
  assert.equal(calls.painted.author, STAMP.author);
  assert.equal(calls.painted.date, STAMP.date);
  assert.equal(calls.jobFreed, true, "the two parsed sides are released once painted");
  assert.equal(calls.viewFreed, false, "the caller now owns the view");
  assert.deepEqual(result.summary.order, []);
});

test("a paint that fails frees the view it opened and says why", async () => {
  const { engine, calls } = fakeEngine({
    paint: () => {
      throw new Error("refused: This document is not the newer side");
    },
  });
  const result = await buildRedline({ engine, older: OLDER, newer: NEWER, ...STAMP, yieldToHost: () => Promise.resolve() });
  assert.equal(result.ok, false);
  assert.match(result.reason, /not the newer side/);
  assert.equal(calls.viewFreed, true, "a view nobody will own is not leaked");
  assert.equal(calls.jobFreed, true);
});

test("a redline cancelled after the comparison never opens a view", async () => {
  const { engine, calls } = fakeEngine();
  let cancelled = false;
  const result = await buildRedline({
    engine,
    older: OLDER,
    newer: NEWER,
    ...STAMP,
    yieldToHost: () => Promise.resolve(),
    // The comparison's own slices see `false`; the reader clicks another
    // version just as it completes.
    cancelled: () => {
      const answer = cancelled;
      cancelled = true;
      return answer;
    },
  });
  assert.deepEqual(result, { ok: false, reason: CANCELLED });
  assert.equal(calls.opened, null, "no document is parsed for a view nobody will see");
  assert.equal(calls.jobFreed, true);
});

// ── redlineEntries: what a reader steps through ─────────────────────────────

test("only THIS comparison's marks are changes; a version's own suggestions are not", () => {
  const entries = redlineEntries(
    [
      revision("r1", "insertion", "p1", 0, "new words"),
      { ...revision("r2", "deletion", "p1", 10, "a reviewer's suggestion"), author: "Bob" },
    ],
    { changes: [] },
    { order: ["p1"] },
    STAMP,
  );
  assert.deepEqual(
    entries.map((entry) => entry.id),
    ["r1"],
  );
});

test("a replacement is ONE change, reading old → new", () => {
  const entries = redlineEntries(
    [
      revision("d1", "deletion", "p1", 4, "quick", { groupId: "g1" }),
      revision("i1", "insertion", "p1", 4, "slow", { groupId: "g1" }),
    ],
    { changes: [] },
    { order: ["p1"] },
    STAMP,
  );
  assert.equal(entries.length, 1, "two marks, one edit");
  assert.equal(entries[0].replaced, true);
  assert.equal(entryText(entries[0]), "quick → slow");
  assert.equal(entryLabel(entries[0]), t("diffCanvas.replaced"));
  assert.deepEqual(legendCounts(entries), { [ADDED]: 1, [REMOVED]: 1, [MOVED]: 0, [FORMATTED]: 0 });
});

test("a move is one kind with two ends, and each end says which it is", () => {
  const entries = redlineEntries(
    [revision("f", "move_from", "p1", 0, "first"), revision("to", "move_to", "p4", 0, "first")],
    { changes: [] },
    { order: ["p1", "p4"] },
    STAMP,
  );
  assert.deepEqual(
    entries.map((entry) => [entry.kind, entryLabel(entry)]),
    [
      [MOVED, t("compare.kind.move_from")],
      [MOVED, t("compare.kind.move_to")],
    ],
  );
  assert.equal(legendCounts(entries)[MOVED], 2);
});

test("a formatting change has no mark and is still a stop, on its paragraph", () => {
  const entries = redlineEntries(
    [],
    {
      changes: [
        { id: "c1", family: "formatting", kind: "formatting", rightText: "Heading", fields: ["runProperties[0].bold"] },
        // A family the page cannot place (a section) is not a stop: it has no paragraph.
        { id: "c2", family: "section", kind: "property" },
      ],
    },
    { anchors: { c1: "p9" }, order: ["p9"] },
    STAMP,
  );
  assert.deepEqual(
    entries.map((entry) => [entry.kind, entry.anchor.node]),
    [[FORMATTED, "p9"]],
  );
});

test("changes are in DOCUMENT order, from the engine's order, then by offset", () => {
  const entries = redlineEntries(
    [
      revision("late", "insertion", "p3", 0, "c"),
      revision("second", "insertion", "p1", 9, "b"),
      revision("first", "deletion", "p1", 2, "a"),
    ],
    { changes: [{ id: "fmt", family: "formatting", kind: "formatting" }] },
    { anchors: { fmt: "p2" }, order: ["p1", "p2", "p3"] },
    STAMP,
  );
  assert.deepEqual(
    entries.map((entry) => entry.id),
    ["first", "second", "fmt", "late"],
  );
});

test("a long change is clipped for its list row, never for the page", () => {
  const entry = { kind: ADDED, text: "x".repeat(200), removed: "", replaced: false };
  const clipped = entryText(entry, 20);
  assert.equal(clipped.length, 20);
  assert.ok(clipped.endsWith("…"));
  assert.equal(entry.text.length, 200, "the entry itself is untouched");
});
