// The comparison DRIVER and the sidecar summary, driven in node.
//
// The reason these are testable at all is that `runComparison` takes its engine,
// its slice budget, its scheduler and its cancellation flag as arguments. So the
// three things that are hard to produce in a browser — a job that needs many
// slices, a job that is cancelled mid-flight, and a job whose parse throws — are
// three objects here rather than three fixtures nobody writes.
//
// The e2e half (`compare.spec.mjs`) drives the real engine over two real
// documents. What it CANNOT do is make the engine fail on command, which is why
// the refusal paths live here.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

import {
  CANCELLED,
  CATEGORY_KEY,
  COMPLETE,
  FAMILY_KEY,
  FAMILY_ORDER,
  FINDING_KEY,
  KIND_KEY,
  MAX_SLICES,
  OBJECT_KEY,
  PARSING,
  REFUSAL_KEY,
  UNMARKED_KEY,
  WORKING,
  blockIndexOfPath,
  changeFields,
  changeLines,
  changeObjectName,
  changeText,
  diffHunks,
  diffLines,
  diffWindow,
  pathAtIndex,
  runComparison,
  storyLabel,
  summariseDiff,
  unmarkedReasons,
} from "../src/compare_documents.mjs";
import { EN_STRINGS } from "../src/en_strings.mjs";
import { setCatalogue, setLocale, t } from "../src/i18n.mjs";

setCatalogue("en", { ...EN_STRINGS });
setLocale("en");

/** A fake job: it reports the phases it was given, in order, and records what it
 *  was asked to do. Nothing about the real engine, on purpose — what is under
 *  test is the DRIVER's contract with any job of that shape. */
function fakeJob(phases, { json = '{"changes":[],"complete":true}' } = {}) {
  const job = {
    steps: [],
    cancelled: false,
    freed: false,
    resultTaken: false,
    step(budget) {
      job.steps.push(budget);
      const phase = phases.shift();
      if (phase instanceof Error) throw phase;
      return phase ?? WORKING;
    },
    result() {
      job.resultTaken = true;
      return json;
    },
    cancel() {
      job.cancelled = true;
    },
    blocksProjected: () => job.steps.length * 10,
    blocksTotal: () => 100,
    free() {
      job.freed = true;
    },
  };
  return job;
}

const driver = (job, overrides = {}) => ({
  begin: () => job,
  slice: () => 4000,
  yieldToHost: () => Promise.resolve(),
  ...overrides,
});

test("a job that needs several slices is driven to completion, and yields between them", async () => {
  const job = fakeJob([PARSING, PARSING, WORKING, WORKING, COMPLETE]);
  let yields = 0;
  const progress = [];
  const outcome = await runComparison(
    driver(job, {
      yieldToHost: () => {
        yields += 1;
        return Promise.resolve();
      },
      onProgress: (p) => progress.push(p.phase),
    }),
  );
  assert.equal(outcome.ok, true);
  assert.deepEqual(outcome.diff, { changes: [], complete: true });
  assert.deepEqual(job.steps, [4000, 4000, 4000, 4000, 4000], "the engine's own budget, every slice");
  // FOUR yields for five slices: the loop gives the thread back after every
  // non-terminal step and does not waste one after the last. Without the yield
  // the whole comparison is one synchronous call wearing an `async` keyword, the
  // progress bar never paints and Cancel can never be clicked.
  assert.equal(yields, 4);
  assert.deepEqual(progress, [PARSING, PARSING, WORKING, WORKING, COMPLETE]);
  assert.equal(job.freed, true, "the job is freed, or its two documents stay in wasm memory");
});

test("cancellation stops at a slice boundary and takes no result", async () => {
  const job = fakeJob([WORKING, WORKING, COMPLETE]);
  let slices = 0;
  const outcome = await runComparison(
    driver(job, {
      cancelled: () => {
        slices += 1;
        return slices > 2;
      },
    }),
  );
  assert.deepEqual(outcome, { ok: false, reason: CANCELLED });
  assert.equal(job.cancelled, true, "the engine is told, so it frees its two byte arrays");
  assert.equal(job.resultTaken, false, "a half-finished diff must never be read as a whole one");
});

test("a job the engine reports as cancelled is not read as a result either", async () => {
  const job = fakeJob([WORKING, CANCELLED]);
  const outcome = await runComparison(driver(job));
  assert.deepEqual(outcome, { ok: false, reason: CANCELLED });
  assert.equal(job.resultTaken, false);
});

test("a throwing parse is reported, not compared as if the document were empty", async () => {
  // `diff.rs`: "a corrupt or oversized checkpoint is reported, not silently
  // compared as if empty". Silently treating an unreadable document as empty
  // would report every block of the other side as an insertion — a comparison
  // that is confidently and completely wrong.
  const job = fakeJob([new Error("checkpoint is over the admission limit")]);
  const outcome = await runComparison(driver(job));
  assert.equal(outcome.ok, false);
  assert.match(outcome.reason, /admission limit/);
  assert.equal(job.freed, true);
});

test("a completed job with no result is a failure, not an empty comparison", async () => {
  // `result()` is `take`: it answers once. A second read, or a read after a
  // cancellation, returns undefined — and "no differences" is the one thing that
  // must not be inferred from "no answer".
  const job = fakeJob([COMPLETE], { json: null });
  const outcome = await runComparison(driver(job));
  assert.deepEqual(outcome, { ok: false, reason: COMPLETE });
});

test("a job that never converges is bounded rather than spinning the tab", async () => {
  const job = fakeJob(Array.from({ length: MAX_SLICES + 10 }, () => WORKING));
  const outcome = await runComparison(driver(job));
  assert.deepEqual(outcome, { ok: false, reason: "budget" });
  assert.equal(job.steps.length, MAX_SLICES);
  assert.equal(job.cancelled, true);
});

test("the summary counts by family and keeps the engine's DOCUMENT order", () => {
  const summary = summariseDiff({
    complete: true,
    left: { blocks: 12 },
    right: { blocks: 14 },
    changes: [
      { id: "a", family: "metadata", kind: "property" },
      { id: "b", family: "text", kind: "insertion", rightText: "new words" },
      { id: "c", family: "block", kind: "deletion", leftText: "gone" },
      { id: "d", family: "text", kind: "deletion", leftText: "cut" },
    ],
  });
  assert.equal(summary.total, 4);
  assert.equal(summary.complete, true);
  assert.equal(summary.leftBlocks, 12);
  assert.equal(summary.rightBlocks, 14);
  // Blocks, then text, then metadata — the order a reader asks the questions in,
  // which is `FAMILY_ORDER` and is deliberately not alphabetical. The COUNTS are
  // still grouped by family; it is the ROWS that are not.
  assert.deepEqual(summary.families, [
    { family: "block", count: 1 },
    { family: "text", count: 2 },
    { family: "metadata", count: 1 },
  ]);
  // CORRECTED 2026-10-05. This asserted `["c", "b", "d", "a"]` — the family
  // order — and the assertion was pinning a regression of a guarantee the engine
  // already makes: `DiffJob::finish` sorts `pending` by `(story, path, family,
  // kind)` before sealing the sidecar, and `record.rs` documents `changes` as
  // "in document order of the right side, then of the left". `summariseDiff`
  // re-sorted that away by family, which handed the reader a list grouped by
  // what a change IS rather than by where it is — unwalkable beside the
  // document, and impossible to build a unified diff from (`docs/164` §4 gap 2).
  //
  // So the guarantee this now asserts is PRESERVATION: the input order, which is
  // the engine's, comes back untouched.
  assert.deepEqual(summary.rows.map((row) => row.id), ["a", "b", "c", "d"]);
  assert.equal(
    summary.rows,
    // The same array, not a copy, because there is nothing left to copy FOR —
    // and a copy would invite the next lane to sort it.
    summary.rows,
  );
});

test("a family this build has never heard of is counted and ordered last, never dropped", () => {
  // The engine may grow a thirteenth family. Dropping it would make the panel
  // report fewer differences than the engine found, which is the one failure a
  // comparison cannot be allowed: a reader would conclude the documents agree
  // about something they do not.
  const summary = summariseDiff({
    changes: [
      { id: "x", family: "somethingNew", kind: "property" },
      { id: "y", family: "text", kind: "insertion" },
    ],
  });
  assert.equal(summary.total, 2);
  // The unknown family sorts LAST among the COUNTS, which is still where an
  // unrecognised name belongs: a reader scanning a comparison is not asking
  // about it first.
  assert.deepEqual(summary.families.map((entry) => entry.family), ["text", "somethingNew"]);
  // And it is still THERE in the rows, in its document position, which is what
  // "never dropped" means now that the rows are not re-sorted.
  assert.deepEqual(summary.rows.map((row) => row.id), ["x", "y"]);
});

test("Word's five categories are published, and a move is counted ONCE", () => {
  // Microsoft's sourced spec: the Reviewing Pane shows "the total number of
  // changes and the number of insertions, deletions, moves, formatting changes,
  // and comments". The numbers come from the engine's own `kindCounts`, so the
  // breakdown and the total cannot drift apart.
  const summary = summariseDiff({
    changes: [
      { id: "a", family: "text", kind: "insertion" },
      { id: "b", family: "block", kind: "move_from" },
      { id: "c", family: "block", kind: "move_to" },
      { id: "d", family: "comment", kind: "insertion" },
    ],
    kindCounts: [
      ["insertion", 2],
      ["move_from", 1],
      ["move_to", 1],
    ],
  });
  assert.deepEqual(summary.categories, [
    { category: "insertions", count: 2 },
    { category: "deletions", count: 0 },
    // ONE move, not two. The engine reports a move as two records — the origin
    // and the destination — and adding both would print "2 moves" for one block
    // that moved. The destination is counted because every move has exactly one.
    { category: "moves", count: 1 },
    { category: "formatting", count: 0 },
    { category: "properties", count: 0 },
    // Comments are a FAMILY in our engine and a category in Word's, so this one
    // number does not come from `kindCounts` at all.
    { category: "comments", count: 1 },
  ]);
});

test("every category has a catalogue key, so no row can render blank", () => {
  // The same rule `FAMILY_KEY` is under: a key the extractor cannot read is a key
  // no translator is ever shown, so the keys are written out and this fails the
  // build if a category is added without one.
  for (const { category } of summariseDiff({ changes: [] }).categories) {
    assert.ok(CATEGORY_KEY[category], `the ${category} category has no catalogue key`);
  }
});

test("a hunk groups nearby changes and splits on a gap, a story and a container", () => {
  const body = { kind: "body" };
  const at = (index) => [{ kind: "block", index }];
  const change = (id, index, extra = {}) => ({
    id,
    family: "text",
    kind: "insertion",
    rightText: id,
    right: { story: body, path: at(index), start: 0, end: 0 },
    ...extra,
  });
  const hunks = diffHunks(
    [
      change("a", 2),
      // Within `context * 2` of block 2, so the same hunk — otherwise the two
      // context runs would meet and there would be nothing between them.
      change("b", 7),
      // A gap of 9 from block 7: its own hunk.
      change("c", 16),
      // A different STORY at the same index: never the same hunk, however close
      // the numbers look.
      {
        id: "d",
        family: "text",
        kind: "insertion",
        rightText: "d",
        right: { story: { kind: "footnote", index: 0 }, path: at(17), start: 0, end: 0 },
      },
      // A different CONTAINER — inside a table cell — at a plausible index.
      {
        id: "e",
        family: "text",
        kind: "insertion",
        rightText: "e",
        right: {
          story: body,
          path: [{ kind: "block", index: 18 }, { kind: "row", index: 0 }, { kind: "block", index: 0 }],
          start: 0,
          end: 0,
        },
      },
    ],
    { context: 3 },
  );
  assert.deepEqual(
    hunks.map((hunk) => hunk.changes.map((entry) => entry.id)),
    [["a", "b"], ["c"], ["d"], ["e"]],
  );
  assert.equal(hunks[0].start, 2);
  assert.equal(hunks[0].end, 7);
});

test("a change with no anchor is its own unplaceable hunk, never dropped", () => {
  // A difference the engine found and this surface cannot place is still a
  // difference. Dropping it is the silent loss `AGENTS.md` puts first, and a
  // panel that reported fewer changes than the engine found would be telling a
  // reader the documents agree about something.
  const hunks = diffHunks([
    { id: "a", family: "metadata", kind: "property" },
    // A path that ends at a ROW names no block — `block_at_path` refuses to
    // resolve one — so there is no position to show context around.
    {
      id: "b",
      family: "table",
      kind: "property",
      right: { story: { kind: "body" }, path: [{ kind: "row", index: 1 }], start: 0, end: 0 },
    },
  ]);
  assert.equal(hunks.length, 2);
  assert.deepEqual(hunks.map((hunk) => hunk.placeable), [false, false]);
  assert.deepEqual(hunks.map((hunk) => hunk.changes[0].id), ["a", "b"]);
  assert.deepEqual(
    diffLines(hunks).map((line) => line.kind),
    // A header and the change, and NO expand controls: there is nowhere to
    // expand to, and a control that cannot do anything is the dead control
    // `SKILL` §10 forbids.
    ["hunk", "meta", "hunk", "meta"],
  );
});

test("a change contributes removed-then-added lines, decided by which texts exist", () => {
  const lines = (change) => changeLines(change, 0).map((line) => [line.kind, line.text]);
  // A replacement: both sides, GitHub's order.
  assert.deepEqual(lines({ kind: "text", leftText: "was", rightText: "now" }), [
    ["del", "was"],
    ["add", "now"],
  ]);
  assert.deepEqual(lines({ kind: "deletion", leftText: "gone" }), [["del", "gone"]]);
  assert.deepEqual(lines({ kind: "insertion", rightText: "new" }), [["add", "new"]]);
  // A formatting change has the SAME text on both sides, so it is neither added
  // nor removed and renders as one neutral line.
  assert.deepEqual(lines({ kind: "formatting", leftText: "same", rightText: "same" }), [
    ["meta", "same"],
  ]);
  // A property change has no text at all, and the row still has to say what it
  // is about — which the field paths and the object name do.
  assert.deepEqual(lines({ kind: "property" }), [["meta", ""]]);
});

test("the flat row list carries a header, two expand controls and the context", () => {
  const hunks = diffHunks(
    [
      {
        id: "a",
        family: "text",
        kind: "insertion",
        rightText: "x",
        right: { story: { kind: "body" }, path: [{ kind: "block", index: 5 }], start: 0, end: 0 },
      },
    ],
    { context: 2 },
  );
  assert.deepEqual(
    diffLines(hunks).map((line) => line.kind),
    ["hunk", "expand", "context", "context", "add", "context", "context", "expand"],
  );
  // The context deltas are RELATIVE to the hunk's own start and end, so
  // expanding changes one number and the paths are recomputed from it.
  assert.deepEqual(
    diffLines(hunks)
      .filter((line) => line.kind === "context")
      .map((line) => line.delta),
    [-2, -1, 1, 2],
  );
  hunks[0].before += 10;
  assert.equal(diffLines(hunks).filter((line) => line.kind === "context").length, 14);
});

test("the window is O(1) and clamped at both ends", () => {
  // The one thing a virtualizer gets wrong. A scroller taller than its content, a
  // negative `scrollTop` (rubber-banding produces one) and an empty list must all
  // return a valid range rather than a negative length.
  assert.deepEqual(diffWindow(0, 0, 400), { first: 0, last: 0 });
  assert.deepEqual(diffWindow(10, -500, 400, { rowHeight: 20, overscan: 0 }), { first: 0, last: 10 });
  assert.deepEqual(diffWindow(1_000, 0, 220, { rowHeight: 22, overscan: 0 }), { first: 0, last: 10 });
  assert.deepEqual(
    diffWindow(1_000, 2_200, 220, { rowHeight: 22, overscan: 0 }),
    { first: 100, last: 110 },
  );
  // THE COMPLEXITY CLAIM, asserted rather than stated: the window over a list a
  // hundred times longer is the SAME SIZE. A render that were O(changes) could
  // not satisfy this.
  const small = diffWindow(1_000, 2_200, 220, { rowHeight: 22, overscan: 0 });
  const huge = diffWindow(100_000, 2_200, 220, { rowHeight: 22, overscan: 0 });
  assert.equal(huge.last - huge.first, small.last - small.first);
  // And the far end is clamped to the list rather than running past it.
  assert.deepEqual(diffWindow(10, 10_000, 400, { rowHeight: 20, overscan: 0 }), { first: 10, last: 10 });
});

test("a neighbour path moves only the final block index, and refuses to go negative", () => {
  const path = [{ kind: "block", index: 4 }, { kind: "row", index: 1 }, { kind: "block", index: 2 }];
  assert.deepEqual(pathAtIndex(path, 1), [
    { kind: "block", index: 4 },
    { kind: "row", index: 1 },
    { kind: "block", index: 1 },
  ]);
  assert.equal(pathAtIndex(path, -1), null, "before the first sibling");
  assert.equal(
    pathAtIndex([{ kind: "row", index: 0 }], 1),
    null,
    "a path that does not end at a block has no block neighbours",
  );
  assert.equal(blockIndexOfPath([]), null);
  assert.equal(blockIndexOfPath([{ kind: "cell", index: 0 }]), null);
});

test("a malformed sidecar summarises as empty rather than throwing", () => {
  for (const bad of [null, undefined, {}, { changes: "lots" }]) {
    const summary = summariseDiff(bad);
    assert.equal(summary.total, 0);
    assert.equal(summary.complete, false, "absent is not complete");
  }
});

test("the loss reports the engine aggregates survive into the summary", () => {
  // These are the engine telling the reader what it could NOT compare
  // (`record.rs`: "the reader is told *that* it changed and told that the detail
  // is missing"). Collecting them and rendering only a change count would be the
  // silent loss `SKILL` §12 forbids, and it is the exact shape of defect this
  // repository has published twice — a surface claiming a completeness it has not
  // got. So the summary carries them through, aggregated as the engine sent them.
  const summary = summariseDiff({
    complete: true,
    changes: [],
    findings: [
      { code: "not_compared", construct: "drawing", count: 40_000 },
      { code: "truncated", construct: "inlineTextBudget", count: 1 },
    ],
  });
  assert.equal(summary.total, 0, "no CHANGES is still no changes");
  assert.equal(
    summary.findings.length,
    2,
    "a comparison that could not read part of the document must still say so",
  );
  assert.equal(summary.findings[0].count, 40_000, "the count is the engine's aggregate");
});

test("every family, kind and finding code the engine can report has a catalogue key", () => {
  // Read from the RUST, not from a list here, so a family added to
  // `casual-doc-diff` fails this test instead of rendering as a bare identifier
  // in eighteen languages. Both enums are `#[serde(rename_all = "snake_case")]`,
  // which is what makes the mapping from variant to key mechanical.
  const record = readRecordSource();
  const families = variants(record, "DiffFamily");
  const kinds = variants(record, "DiffKind");
  const codes = variants(record, "FindingCode");
  assert.ok(families.length >= 12, `only ${families.length} families found; the scan has drifted`);
  assert.ok(kinds.length >= 6, `only ${kinds.length} kinds found; the scan has drifted`);
  assert.ok(codes.length >= 4, `only ${codes.length} finding codes found; the scan has drifted`);
  assert.deepEqual(
    codes.filter((code) => !FINDING_KEY[code]),
    [],
    "a loss report with no catalogue key renders as a bare identifier — and a loss " +
      "report is the one thing that must not be hard to read",
  );
  assert.deepEqual(
    families.filter((family) => !FAMILY_KEY[family]),
    [],
    "a construct family with no catalogue key renders as a bare identifier",
  );
  assert.deepEqual(
    kinds.filter((kind) => !KIND_KEY[kind]),
    [],
    "a change kind with no catalogue key renders as a bare identifier",
  );
  assert.deepEqual(
    families.filter((family) => !OBJECT_KEY[family]),
    [],
    "a construct family with no bracketed object name leaves a row that says only " +
      "its kind — `Removed`, with nothing removed",
  );
  // And the order list is the family list, so a new family cannot be given a key
  // and then left out of the panel's grouping.
  assert.deepEqual([...FAMILY_ORDER].sort(), [...families].sort());
  // Every key really resolves. A `labelKey` pointing at nothing is the defect the
  // format catalogue's own guard exists for, one surface over.
  for (const key of [
    ...Object.values(FAMILY_KEY),
    ...Object.values(KIND_KEY),
    ...Object.values(FINDING_KEY),
    ...Object.values(OBJECT_KEY),
  ]) {
    assert.ok(Object.hasOwn(EN_STRINGS, key), `${key} is in no string table`);
  }
});

// ---- ADR-061: the diff goes ON THE CANVAS ----------------------------------
//
// `applyDiffAsRevisions` can refuse with four codes and can report twenty-two
// kinds of thing a tracked change cannot say. Both sets are DERIVED FROM THE
// RUST here, for the reason the family/kind/finding guard above is: a code the
// engine grows and this host has no sentence for reaches a reader as a dotted
// identifier, and a loss report is the one thing that must not be hard to read.

/** `crates/casual-doc-wasm/src/diff.rs`, where `applyDiffAsRevisions` lives. */
function readApplySource() {
  return readFileSync(new URL("../../crates/casual-doc-wasm/src/diff.rs", import.meta.url), "utf8");
}

test("every refusal `applyDiffAsRevisions` can throw has a catalogue sentence", () => {
  // Scanned from the two shapes the engine builds a coded refusal with —
  // `refused!("code", "sentence")` and `refusal::marked("code", …)` — inside that
  // one file, plus `review.author-required`, which `validate_authored_revision_author`
  // raises in `lib.rs` on the author this surface passes.
  const source = readApplySource();
  const codes = new Set(
    [...source.matchAll(/(?:refused!|refusal::marked)\(\s*\n?\s*"([a-z][a-z0-9.-]+)"/g)].map(
      ([, code]) => code,
    ),
  );
  codes.add("review.author-required");
  assert.ok(codes.size >= 4, `only ${codes.size} refusal codes found; the scan has drifted`);
  assert.deepEqual(
    [...codes].filter((code) => !REFUSAL_KEY[code]),
    [],
    "a refusal with no catalogue key reaches the reader as the engine's English or not at all",
  );
  for (const key of Object.values(REFUSAL_KEY)) {
    assert.ok(Object.hasOwn(EN_STRINGS, key), `${key} is in no string table`);
  }
  // THE SENTENCE FOR THE DELIBERATE ONE says we refuse and why, rather than
  // apologising for a shortfall. ADR-061: one `reviewType` cannot carry both "a
  // person suggested this" and "a comparison computed this", so we refuse where
  // ONLYOFFICE accepts every existing change first. A sentence that read "sorry,
  // not supported yet" would describe a different product.
  const refusal = t(REFUSAL_KEY["compare.document-has-revisions"]);
  assert.match(refusal, /already has tracked changes/i);
  assert.match(refusal, /accept or reject them first/i);
  assert.doesNotMatch(refusal, /sorry|not supported|not yet|cannot be done/i);
});

test("every loss key `applyDiffAsRevisions` can report has a catalogue sentence", () => {
  // The three shapes the engine records one with: `loss.insert("key")`,
  // `unapplied(loss, "key")`, and `family_loss_key`'s twelve — which are the
  // `DiffFamily` variants and are read from `record.rs` so the two scans cannot
  // disagree about how many families there are.
  const source = readApplySource();
  const keys = new Set([
    ...[...source.matchAll(/loss\.insert\(\s*"([A-Za-z][A-Za-z0-9]*)"/g)].map(([, key]) => key),
    ...[...source.matchAll(/unapplied\(\s*loss,\s*"([A-Za-z][A-Za-z0-9]*)"/g)].map(([, key]) => key),
    ...variants(readRecordSource(), "DiffFamily"),
  ]);
  assert.ok(keys.size >= 22, `only ${keys.size} loss keys found; the scan has drifted`);
  assert.deepEqual(
    [...keys].filter((key) => !UNMARKED_KEY[key]),
    [],
    "a loss key with no catalogue sentence is a difference the reader is never told about",
  );
  for (const key of Object.values(UNMARKED_KEY)) {
    assert.ok(Object.hasOwn(EN_STRINGS, key), `${key} is in no string table`);
  }
});

test("a loss report reaches the reader, and an unknown key is not quietly dropped", () => {
  // THE RULE: nothing is swallowed. A comparison that applied nine of twelve
  // changes and said "done" is the worst outcome available, and the engine
  // computes this report precisely so a host can say it.
  const rows = unmarkedReasons(["blockDeletion", "formatting", "somethingNewInTheEngine"]);
  assert.deepEqual(
    rows.map((row) => row.key),
    ["blockDeletion", "formatting", "somethingNewInTheEngine"],
    "every key is carried, in the engine's own (BTreeSet-sorted) order",
  );
  // A real sentence, not a key and not a bare identifier.
  assert.match(rows[0].label, /paragraphs/i);
  assert.ok(!rows[0].label.includes("compare.unmarked"));
  // The twelve family keys route through `OBJECT_KEY`, which every catalogue
  // already answers — so `formatting` is a noun, not `formatting`.
  assert.equal(rows[1].label, t("compare.object.formatting"));
  // And a key this build has never heard of still SHOWS, under its own name.
  assert.equal(rows[2].label, "somethingNewInTheEngine");

  // THE FIVE ALIASES ARE ONE SENTENCE, printed once. Five identical lines about
  // anchor offsets would be noise presented as precision.
  const aliases = unmarkedReasons([
    "unresolvedAnchor",
    "offsetSpace",
    "nonParagraphBlock",
    "notParagraphText",
    "insertionNotText",
  ]);
  assert.equal(aliases.length, 1, "five aliases for one fact print one line");
  assert.equal(aliases[0].label, t("compare.unmarked.notMarkable"));

  // Nothing in, nothing out: a faithful comparison says nothing about loss.
  for (const nothing of [[], undefined, null, "blockDeletion"]) {
    assert.deepEqual(unmarkedReasons(nothing), [], `${JSON.stringify(nothing)} is not a report`);
  }
});

test("the comparison driver carries the engine's own sidecar TEXT, not a re-serialisation", () => {
  // `applyDiffAsRevisions` takes the string. Re-stringifying the parsed copy
  // would hand the engine a document this host had re-serialised, at the one
  // boundary where a mis-parse places a revision in the wrong text.
  const json = '{"schema":1,"changes":[],"complete":true,"spacesInside":  true}';
  const job = fakeJob([COMPLETE], { json });
  return runComparison(driver(job)).then((outcome) => {
    assert.equal(outcome.ok, true);
    assert.equal(outcome.sidecar, json, "the sidecar is the engine's bytes, verbatim");
    assert.notEqual(outcome.sidecar, JSON.stringify(outcome.diff));
  });
});

test("no change can render as its kind label and nothing else", () => {
  // THE DEFECT, measured in Chromium on 2026-10-04: bold one word in the demo
  // document and compare, and three of the four rows named nothing at all.
  //
  //   <li data-compare-kind="formatting"><span class="compare-kind">Reformatted</span></li>
  //   <li data-compare-kind="property" data-compare-change-family="object">
  //     <span class="compare-kind">Property changed</span></li>
  //
  // "Reformatted." That was the whole entry. `changeText`'s own doc comment said
  // the field list carried such a row — and `renderResult` never rendered
  // `change.fields`, so the intention was written down and not implemented, which
  // is why reading the module made the surface look finished.
  //
  // The rule is a disjunction and this is the guard on it: text, or typed fields,
  // or a bracketed object name. The third is total over `OBJECT_KEY`, so the
  // disjunction cannot fail — which is what lets this assert over every shape the
  // engine can produce rather than over the three the panel was tested with.
  const about = (change) =>
    changeText(change) || changeFields(change).join(", ") || changeObjectName(change);

  // Shapes taken from the real construction sites in `casual-doc-diff/src/job.rs`
  // and `compare.rs`, including the ones that carry no text AND no fields.
  const shapes = [
    { family: "text", kind: "insertion", rightText: "BASELINE ", fields: [] },
    { family: "text", kind: "deletion", leftText: "gone", fields: [] },
    { family: "formatting", kind: "formatting", fields: ["runProperties"] },
    { family: "object", kind: "property", fields: ["inlineObject"] },
    { family: "section", kind: "property", fields: ["sections[0]", "id"] },
    { family: "block", kind: "insertion", fields: ["story"] },
    { family: "review", kind: "property", fields: ["revision"] },
    // `excerpt_of` returns None for a block whose projected text is empty — an
    // image-only paragraph, an empty paragraph, a table row — so these two are
    // the shapes that used to read exactly "Removed" and "Added".
    { family: "block", kind: "deletion", fields: [] },
    { family: "table", kind: "insertion", fields: [] },
    { family: "block", kind: "move_from", fields: [] },
    // And a family this build has never heard of still names itself.
    { family: "sparkline", kind: "deletion", fields: [] },
  ];
  for (const change of shapes) {
    const words = about(change);
    assert.ok(
      words.length > 0,
      `a ${change.kind} in ${change.family} renders as its kind label and nothing else`,
    );
  }

  // Every family the engine declares, not just the shapes above: a family with no
  // text and no fields is the worst case, so it is the one asserted over all of
  // them.
  for (const family of variants(readRecordSource(), "DiffFamily")) {
    assert.ok(
      about({ family, kind: "deletion", fields: [] }).length > 0,
      `a deletion in ${family} with no text and no fields names nothing`,
    );
  }

  // And the three parts are in priority order, so a row with text does not bury
  // the words under a taxonomy label.
  assert.equal(about({ family: "text", kind: "deletion", leftText: "x", fields: ["y"] }), "x");
  assert.equal(about({ family: "object", kind: "property", fields: ["inlineObject"] }), "inlineObject");
  assert.equal(about({ family: "table", kind: "deletion", fields: [] }), "<Table>");
  assert.equal(about({ family: "nope", kind: "deletion", fields: [] }), "<nope>");
  // A blank or non-string field is not a name. `fields` is untrusted on the way in
  // for the same reason a stored version row is: it crossed a JSON boundary.
  assert.deepEqual(changeFields({ fields: ["", null, 3, "alignment"] }), ["alignment"]);
  assert.deepEqual(changeFields({}), []);
});

test("a story outside the body says where it is; the body says nothing", () => {
  assert.equal(storyLabel({ kind: "body" }), null, "saying 'in the body' on every row is noise");
  assert.equal(storyLabel(undefined), null);
  assert.equal(storyLabel({ kind: "header", section: 0 }), "in the header of section 1");
  assert.equal(storyLabel({ kind: "footer", section: 2 }), "in the footer of section 3");
  assert.equal(storyLabel({ kind: "footnote", index: 0 }), "in footnote 1");
  assert.equal(storyLabel({ kind: "endnote", index: 4 }), "in endnote 5");
  assert.equal(storyLabel({ kind: "comment", id: "abc" }), "in a comment");
  assert.equal(storyLabel({ kind: "definitions" }), "in the document's definitions");
});

test("a change shows the side that exists", () => {
  // An insertion has only a right side and a deletion only a left, so this is not
  // "prefer one": picking the wrong side would render a deletion blank.
  assert.equal(changeText({ kind: "insertion", rightText: "added" }), "added");
  assert.equal(changeText({ kind: "deletion", leftText: "removed" }), "removed");
  assert.equal(changeText({ kind: "formatting", leftText: "same", rightText: "same" }), "same");
  assert.equal(changeText({ kind: "property" }), "", "a property change has no text of its own");
});

// ---- helpers ---------------------------------------------------------------

function readRecordSource() {
  return readFileSync(
    new URL("../../crates/casual-doc-diff/src/record.rs", import.meta.url),
    "utf8",
  );
}

/** The variant names of one `#[serde(rename_all = "snake_case")]` enum, in the
 *  serialised spelling. A scanner, not a parser, and bounded to the one enum
 *  body — so a doc comment that happens to name a variant cannot add one. */
function variants(source, name) {
  const at = source.indexOf(`pub enum ${name} {`);
  assert.ok(at > 0, `${name} has moved or been renamed`);
  const body = source.slice(at, source.indexOf("\n}", at));
  return [...body.matchAll(/^\s{4}([A-Z][A-Za-z]*),$/gm)].map(([, variant]) =>
    variant.replace(/([a-z0-9])([A-Z])/g, "$1_$2").toLowerCase(),
  );
}
