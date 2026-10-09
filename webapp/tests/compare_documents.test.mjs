// The comparison DRIVER, the redline builder and the loss vocabulary, in node.
//
// The reason these are testable at all is that `runComparison` and
// `buildRedline` take their engine, slice budget, scheduler and cancellation
// flag as arguments. So the things that are hard to produce in a browser — a job
// that needs many slices, a job cancelled mid-flight, a parse that throws, a
// paint that fails after the comparison finished — are objects here rather than
// fixtures nobody writes.
//
// The e2e half (`compare.spec.mjs`, `version-diff-canvas.spec.mjs`) drives the
// real engine over real documents. What it CANNOT do is make the engine fail on
// command, which is why the refusal and ownership paths live here.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

import {
  CANCELLED,
  COMPLETE,
  FAMILY_KEY,
  FINDING_KEY,
  MAX_SLICES,
  PARSING,
  REFUSAL_KEY,
  UNMARKED_KEY,
  WORKING,
  buildRedline,
  runComparison,
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

test("every family and finding code the engine can report has a catalogue key", () => {
  // Read from the RUST, not from a list here, so a family added to
  // `casual-doc-diff` fails this test instead of rendering as a bare identifier
  // in eighteen languages. Both enums are `#[serde(rename_all = "snake_case")]`,
  // which is what makes the mapping from variant to key mechanical.
  const record = readRecordSource();
  const families = variants(record, "DiffFamily");
  const codes = variants(record, "FindingCode");
  assert.ok(families.length >= 12, `only ${families.length} families found; the scan has drifted`);
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
  // Every key really resolves. A `labelKey` pointing at nothing is the defect the
  // format catalogue's own guard exists for, one surface over.
  for (const key of [...Object.values(FAMILY_KEY), ...Object.values(FINDING_KEY)]) {
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

/** `crates/casual-doc-wasm/src/compare_view.rs`, where the redline is painted
 *  (`showComparison`, ADR-065) — it reports loss in the same vocabulary. */
function readViewSource() {
  return readFileSync(new URL("../../crates/casual-doc-wasm/src/compare_view.rs", import.meta.url), "utf8");
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

test("every loss key `applyDiffAsRevisions` or the redline can report has a catalogue sentence", () => {
  // The three shapes the engine records one with: `loss.insert("key")`,
  // `unapplied(loss, "key")`, and `family_loss_key`'s twelve — which are the
  // `DiffFamily` variants and are read from `record.rs` so the two scans cannot
  // disagree about how many families there are. The redline (`compare_view.rs`)
  // reports in the same vocabulary, so it is scanned too.
  const source = readApplySource() + readViewSource();
  const keys = new Set([
    ...[...source.matchAll(/loss\.insert\(\s*"([A-Za-z][A-Za-z0-9]*)"/g)].map(([, key]) => key),
    ...[...source.matchAll(/unapplied\(\s*loss,\s*"([A-Za-z][A-Za-z0-9]*)"/g)].map(([, key]) => key),
    ...variants(readRecordSource(), "DiffFamily"),
  ]);
  assert.ok(keys.size >= 22, `only ${keys.size} loss keys found; the scan has drifted`);
  assert.deepEqual(
    [...keys].filter((key) => !UNMARKED_KEY[key] && !FAMILY_KEY[key]),
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
  const rows = unmarkedReasons(["blockDeletion", "formatting", "somethingNewInTheEngine"], {
    familyCounts: [["formatting", 3]],
  });
  assert.deepEqual(
    rows.map((row) => row.key),
    ["blockDeletion", "formatting", "somethingNewInTheEngine"],
    "every key is carried, in the engine's own (BTreeSet-sorted) order",
  );
  // A real sentence, not a key and not a bare identifier.
  assert.match(rows[0].label, /paragraphs/i);
  assert.ok(!rows[0].label.includes("compare.unmarked"));
  // A family key is said as the family's own COUNTED sentence ("Formatting
  // changes: 3"), never as an identifier or a bracketed noun.
  assert.equal(rows[1].label, t("compare.family.formatting", { count: "3" }));
  assert.match(rows[1].label, /3/);
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
