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
  COMPLETE,
  FAMILY_KEY,
  FAMILY_ORDER,
  FINDING_KEY,
  KIND_KEY,
  MAX_SLICES,
  PARSING,
  WORKING,
  changeText,
  runComparison,
  storyLabel,
  summariseDiff,
} from "../src/compare_documents.mjs";
import { EN_STRINGS } from "../src/en_strings.mjs";
import { setCatalogue, setLocale } from "../src/i18n.mjs";

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

test("the summary counts by family and orders rows in reading order", () => {
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
  // which is `FAMILY_ORDER` and is deliberately not alphabetical.
  assert.deepEqual(summary.families, [
    { family: "block", count: 1 },
    { family: "text", count: 2 },
    { family: "metadata", count: 1 },
  ]);
  assert.deepEqual(summary.rows.map((row) => row.id), ["c", "b", "d", "a"]);
});

test("a family this build has never heard of is counted and sorted last, never dropped", () => {
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
  assert.deepEqual(summary.families.map((entry) => entry.family), ["text", "somethingNew"]);
  assert.equal(summary.rows.at(-1).id, "x");
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
  // And the order list is the family list, so a new family cannot be given a key
  // and then left out of the panel's grouping.
  assert.deepEqual([...FAMILY_ORDER].sort(), [...families].sort());
  // Every key really resolves. A `labelKey` pointing at nothing is the defect the
  // format catalogue's own guard exists for, one surface over.
  for (const key of [
    ...Object.values(FAMILY_KEY),
    ...Object.values(KIND_KEY),
    ...Object.values(FINDING_KEY),
  ]) {
    assert.ok(Object.hasOwn(EN_STRINGS, key), `${key} is in no string table`);
  }
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
