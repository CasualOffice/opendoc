// Compare this document with another one: the chrome over `casual-doc-diff`.
//
// `docs/153` files this as `review.compare-documents`, rank 2, and it is the
// third of the three capabilities that were fully built and completely
// unreachable. `crates/casual-doc-diff` is a whole crate — Merkle projection,
// patience anchoring plus Myers, move detection by hash join, grapheme-cluster
// word diff, property comparison by serde reflection — and
// `crates/casual-doc-wasm/src/diff.rs` exposes `beginVersionDiff`, `step`,
// `result`, `cancel`, `blocksProjected`, `blocksTotal` and `defaultDiffSlice`.
// `grep -r diffVersions webapp/` returned nothing. `version_panel.mjs` disabled
// Show changes with a comment saying the structural diff "is not built", which
// was wrong about which half was missing: the diff was built and the panel was
// not. `docs/140` §8-13 already carries the accurate wording.
//
// ## WHAT "THE OTHER DOCUMENT" IS, decided and recorded
//
// The two references answer this differently and both answers are right, so both
// ship, each where its own reference puts it:
//
//   * **Word's Review ▸ Compare** takes two documents off the disk. That is
//     `review.compare` on the Review band and in the Review menu: pick a file,
//     and it is compared against the document on screen.
//   * **Google Docs' version history** compares a stored version with the
//     current document — their "Show changes" checkbox. That is the version
//     panel's own `version.changes` row, which hands this module the checkpoint's
//     bytes.
//
// One engine call serves both, because a comparison is two byte arrays and
// nothing else (`diff.rs`: the facade "references nothing in the live session").
//
// ## THE DIFF IS ON THE CANVAS — ADR-061 (`docs/158`)
//
// A comparison is **applied to the open document as tracked changes**, through the
// revision model that already existed, and read with the review surface that
// already existed. `docs/158` measured all three references and found that none of
// them presents a comparison as a count — ONLYOFFICE mutates the open document,
// setting `reviewtype_Add`/`reviewtype_Remove` on runs with an author and a date
// (`Comparison.js:3864`, `:179`), and its Compare button sits on the Review band
// beside Accept, Reject, Previous and Next, which is the admission that a diff IS
// review markup.
//
// So `compareWith` hands the sidecar to `applyDiffAsRevisions(sidecar, author,
// date)` and turns `setShowChanges` on. Every downstream surface then inherits the
// comparison with no further chrome: the author-coloured underline and
// strikethrough on the canvas, `listRevisions`, the review gutter, accept/reject
// per change and in bulk, next/previous, and `w:ins`/`w:del` on export. ONE
// `Operation::UpdateReviewState` under `HistoryKind::Review`, so the whole
// comparison is one undo step.
//
// The panel is the INDEX of that, in Google Docs' role — not the report. It says
// how many differences are now tracked changes, names each one's object, and says
// how to walk them; the differences themselves are read in the document.
//
// ## THREE THINGS THE PANEL MUST SAY, and why each is not optional
//
//   1. **A REFUSAL, in its own words.** Four coded refusals can come back
//      (`REFUSAL_KEY`), and `compare.document-has-revisions` is the one a reader
//      will actually meet. It is a DELIBERATE refusal, not a shortfall: one
//      `reviewType` field cannot carry both "a person suggested this" and "a
//      comparison computed this" without the two deciding each other, and
//      ONLYOFFICE resolves that by accepting every existing change first
//      (`Comparison.js:3910-3921`). Destroying a reviewer's suggestions to run a
//      comparison is the loss `AGENTS.md` puts first, so we refuse — and the
//      sentence says so rather than apologising.
//   2. **WHAT COULD NOT BE MARKED** (`UNMARKED_KEY`). `EditResult.pasteLoss` names
//      every difference the comparison found and tracked changes cannot express: a
//      whole block only the other document has, a move's far half, a story outside
//      the body, removed text the record carries only as an excerpt, and a
//      comparison that stopped short of exhaustive. A comparison that silently
//      applied nine of twelve changes and reported success is the worst outcome
//      available, so the report is rendered, unknown keys included.
//   3. **HOW TO WALK THEM.** Review ▸ Next / Previous, Accept and Reject. Which is
//      the honest answer to the one part of ADR-061 that is still blocked below.
//
// ## WHAT IS STILL BLOCKED: clicking an entry cannot scroll to its change
//
// Not on effort. `diff.rs` imports BOTH sides freshly and this module's own
// right-hand side is `comparableBytes(doc, …)`, a re-export of the live document,
// so `right.node` belongs to a throwaway parse whose id counter restarted at 1.
// Only `right.path` survives, and the facade exposes no `path → NodeId` resolver:
// `blockIndexOf` is the inverse, `documentOutline` covers only headings.
// `navigateToReviewAnchor` takes `{node, start, end}` — a byte-for-byte match for
// `DiffAnchor` — so handing it one would silently scroll to an unrelated paragraph
// holding the same ordinal id, which is worse than not navigating.
//
// The resolver itself EXISTS in Rust and is what makes this feature work:
// `casual_doc_diff::projection::block_at_path(document, story, path)` reuses the
// walk that produced the path, so the producer and the resolver cannot disagree
// about `Sdt` wrappers or `AltChunk`. `applyDiffAsRevisions` calls it. What is
// missing is only a wasm-exposed `nodeAtStoryPath(story, path) -> String` over it,
// which is a `crates/` change. Until then the panel points at Review's own
// next/previous, which navigates the revisions the comparison just wrote — the
// same destination by a route that cannot land on the wrong paragraph.
//
// ## WHERE IT RUNS
//
// On the main thread, in slices. `diff.rs` says why at length: there is not one
// `new Worker` in this tree, the wasm module is instantiated once by `main.js`
// and holds the live document, and sharing that memory needs
// `SharedArrayBuffer`, which needs COOP/COEP headers GitHub Pages cannot send. A
// separate instance in a worker needs no shared memory and is the right home —
// which is exactly why the facade takes bytes and returns JSON. Until then the
// host drives it between frames, as `background_measure.mjs` already measures a
// long document, and `cancel()` stops it at the next slice boundary.
//
// The budget is BLOCKS, not milliseconds, and the engine owns the starting
// number (`defaultDiffSlice`) so a figure typed into this file cannot drift from
// what a unit of its work costs.
//
// Pure except where it cannot be: the comparison DRIVER takes its engine, its
// scheduler and its clock as arguments, so the whole of "parse two documents in
// slices, report progress, cancel at a boundary" is drivable from node without a
// browser. The DOM half is `bindComparePanel` at the bottom.
import { editRefusalMessage } from "./edit_errors.mjs";
import { n, t } from "./i18n.mjs";

/** This document, as bytes a comparison can parse.
 *
 *  A checkpoint is a fidelity-complete SOURCE-FORMAT artifact, not normalized
 *  JSON — `docs/112` measured that JSON omits binary resources and the retained
 *  source envelope — so a comparison needs a real DOCX/ODT/RTF/TXT, and this is
 *  the same ladder `takeDraftSnapshot` uses for the same reason: the document's
 *  own format first so an unchanged document compares against its own bytes,
 *  then the two lossier modes.
 *
 *  There is deliberately NO cross-format fallback. Normalized JSON would succeed
 *  where DOCX failed and would drop every image on the way, so a comparison that
 *  silently fell back to it would report "no object changes" about a document
 *  whose objects it had never seen.
 *
 *  Returns `null` when no mode produced bytes, which the panel reports as such.
 *
 *  **Complexity: O(document), once, on an explicit gesture, ON THE MAIN THREAD —
 *  and MEASURED rather than asserted.** Driven through the real panel in Chromium
 *  on 2026-10-04, over plain-text documents of n, 2n and 4n paragraphs:
 *
 *    |  paragraphs | mode               |     bytes | export |
 *    | ----------: | ------------------ | --------: | -----: |
 *    |       5,000 | exact_if_unchanged |   358,889 |  10 ms |
 *    |      10,000 | exact_if_unchanged |   718,889 |  19 ms |
 *    |      20,000 | exact_if_unchanged | 1,448,889 |  34 ms |
 *    |       5,000 | preserve_when_safe |   358,896 |  12 ms |
 *    |      10,000 | preserve_when_safe |   718,896 |  23 ms |
 *    |      20,000 | preserve_when_safe | 1,448,896 |  44 ms |
 *
 *  Linear, confirmed by the doubling SKILL §8 asks for rather than by a timing
 *  threshold: 12 → 23 → 44 ms is 1.9× per doubling on the regenerating ladder,
 *  which is the one a comparison after real editing takes (`exact_if_unchanged`
 *  refuses an edited document).
 *
 *  THE MILLISECONDS ARE ONE MACHINE'S, THE RATIO IS THE CLAIM. The table is a
 *  single-worker run on one laptop and a loaded runner has produced 13 → 27 → 55
 *  on the same build; what did not move is the slope, which stayed at 2.0× per
 *  doubling. So `compare-cost.spec.mjs` holds the RATIO and never a budget — a
 *  millisecond bound here would be flaky under contention and would still not
 *  tell a slow constant from a quadratic, which is the distinction that matters.
 *
 *  SO THE CONSTANT IS SMALL AND THE SHAPE IS NOT. 44 ms at 20,000 paragraphs does
 *  not freeze a tab; the same slope at the viewer's own admission ceiling
 *  (`MAX_VIEWER_BLOCKS`, 1,800,000) is seconds of main-thread work that nothing
 *  can interrupt, because an export is one synchronous call into wasm.
 *
 *  It cannot be moved off the thread from here, and the module header says why at
 *  length: the wasm instance holds the live document, a second instance in a
 *  worker would need `SharedArrayBuffer`, and that needs COOP/COEP headers GitHub
 *  Pages cannot send. What CAN be fixed from here is that the reader used to get
 *  no feedback and no way out before the block — `compareWith` exported first and
 *  rendered the progress bar afterwards — and that is fixed: the progress state
 *  and its Cancel are painted and a frame is yielded BEFORE this is called.
 *
 *  The marks are a real instrument, not test scaffolding: `opendoc.compare.export`
 *  is the one document-sized cost on this path and a host profiling a slow
 *  comparison needs to see it by name. They are also what lets the complexity
 *  guard measure a ratio instead of asserting a millisecond budget.
 */
export function comparableBytes(doc, sourceFormat) {
  if (!doc) return null;
  for (const mode of ["exact_if_unchanged", "preserve_when_safe", "semantic"]) {
    try {
      mark("opendoc.compare.export.start");
      const artifact = doc.exportAs(sourceFormat, mode);
      const bytes = artifact.bytes;
      artifact.free();
      measure("opendoc.compare.export", "opendoc.compare.export.start");
      return bytes;
    } catch {
      // Try the next mode. The failure is reported only if every mode fails,
      // because an `exact_if_unchanged` refusal on an edited document is the
      // normal case and not an error.
    }
  }
  return null;
}

/** `performance.mark`, guarded. The API is missing in no browser this ships to
 *  and present in none of the node tests, and an instrument that throws is worse
 *  than no instrument. O(1). */
function mark(name) {
  try {
    globalThis.performance?.mark?.(name);
  } catch {
    // An instrument may not be the reason a comparison fails.
  }
}

/** `performance.measure` from a start mark, guarded the same way. O(1). */
function measure(name, from) {
  try {
    globalThis.performance?.measure?.(name, from);
  } catch {
    // Same: a missing start mark must not take the comparison with it.
  }
}

/** The phases `WasmVersionDiff.step` returns. Named here so a typo in a
 *  comparison against one of them is a reference error rather than a branch that
 *  silently never runs. */
export const PARSING = "parsing";
export const WORKING = "working";
export const COMPLETE = "complete";
export const CANCELLED = "cancelled";

/** How many slices to take before giving up.
 *
 *  A bound, not a timeout: a job that is making progress finishes, and a job
 *  that is not must stop rather than spin a tab forever. At the engine's default
 *  slice of 4,000 blocks this admits 40 million blocks of work, which is two
 *  orders of magnitude past the viewer's own document ceiling — so it can only
 *  be reached by a job that has stopped converging, which is the case it is for.
 */
export const MAX_SLICES = 10_000;

/** The reason a non-converging job stops. This module's own vocabulary, never a
 *  sentence — see `NO_ANSWER`. */
export const EXHAUSTED = "budget";

/** Outcomes that are this module's INTERNAL vocabulary rather than something a
 *  reader should be shown.
 *
 *  `budget` means the slice bound was reached; `complete` means the engine said
 *  it had finished and handed back no result. Both are real failures and neither
 *  is English: "The comparison failed: budget" is the raw-token-in-the-status-bar
 *  defect `edit_errors.mjs` exists to prevent. They map to one sentence, because
 *  from the reader's side they are one thing — it stopped without an answer. */
export const NO_ANSWER = new Set([EXHAUSTED, COMPLETE]);

/**
 * Runs one comparison to completion, in slices, reporting progress.
 *
 * @param {object} io
 * @param {() => object} io.begin `beginVersionDiff(left, right)`, already bound
 *   to its two byte arrays — so this driver never holds the bytes and cannot
 *   keep two documents alive after the job is done.
 * @param {() => number} io.slice the block budget per slice (`defaultDiffSlice`).
 * @param {() => Promise<void>} io.yieldToHost resolves when it is this job's turn
 *   again. Injected, so a test drives it synchronously and the editor drives it
 *   between frames — and so nothing here reaches for `requestAnimationFrame`.
 * @param {(progress: {phase: string, done: number, total: number}) => void} [io.onProgress]
 * @param {() => boolean} [io.cancelled] asked once per slice.
 * @returns {Promise<{ok: true, diff: object, sidecar: string} | {ok: false, reason: string}>}
 *   `sidecar` is the engine's own JSON text, carried alongside the parsed object
 *   because `applyDiffAsRevisions` takes the STRING. Re-stringifying the parsed
 *   copy would work and is still wrong: it would hand the engine a document this
 *   module had re-serialised, so a key order or a number format this host's
 *   `JSON` differs on would become the engine's problem at the one boundary
 *   where a mis-parse places a revision in the wrong text.
 */
export async function runComparison(io) {
  const job = io.begin();
  const budget = io.slice();
  let retained = false;
  try {
    for (let slices = 0; slices < MAX_SLICES; slices += 1) {
      if (io.cancelled?.()) {
        job.cancel();
        return { ok: false, reason: CANCELLED };
      }
      let phase;
      try {
        phase = job.step(budget);
      } catch (error) {
        // A corrupt or over-limit checkpoint is REPORTED, not silently compared
        // as if it were empty — which is the one outcome that would make a
        // comparison lie rather than fail.
        return { ok: false, reason: String(error?.message ?? error) };
      }
      io.onProgress?.({
        phase,
        done: job.blocksProjected(),
        total: job.blocksTotal(),
      });
      if (phase === CANCELLED) return { ok: false, reason: CANCELLED };
      if (phase === COMPLETE) {
        const json = job.result();
        if (!json) return { ok: false, reason: COMPLETE };
        // RETAINED ONLY WHEN ASKED. The handle owns the two parsed sides, which
        // is what `blockTextAt` reads a unified diff's context from, so a caller
        // that wants context takes ownership of the handle and must `free()` it
        // — the `finally` below is skipped for exactly that case. A caller that
        // wants only the sidecar passes nothing and pays what it always paid.
        if (io.retain) {
          retained = true;
          return { ok: true, diff: JSON.parse(json), sidecar: json, job };
        }
        return { ok: true, diff: JSON.parse(json), sidecar: json };
      }
      // GIVE THE THREAD BACK between slices. Without this the loop is one long
      // synchronous call wearing an `async` keyword: `await` on an already-done
      // promise still yields a microtask, but a microtask does not let the
      // browser paint, so the progress bar this job renders would never appear
      // and Cancel could never be clicked. The host decides what "later" means.
      await io.yieldToHost();
    }
    job.cancel();
    return { ok: false, reason: EXHAUSTED };
  } finally {
    if (!retained) job.free?.();
  }
}

/** The families, in the order the panel groups them.
 *
 *  Reading order of a document rather than alphabetical: what moved, then what
 *  the words say, then how they look, then the structures around them, then the
 *  things beside them. A reader scanning a comparison is asking "what actually
 *  changed" and the answer they want first is never "metadata". */
export const FAMILY_ORDER = Object.freeze([
  "block",
  "text",
  "formatting",
  "style",
  "table",
  "object",
  "section",
  "definition",
  "resource",
  "comment",
  "review",
  "metadata",
]);

/** Family and kind -> catalogue key, WRITTEN OUT.
 *
 *  `t(`compare.family.${family}`)` would be shorter and is wrong: the extractor
 *  that builds `locales/en.json` finds keys by scanning for `t("literal")`, so a
 *  composed key is collected as the key `compare.family.${family}` and reported
 *  as called-but-undeclared — and the extractor is right to say so, because a key
 *  it cannot read is a key no translator is ever shown. These maps are the keys
 *  spelled out once, and `compare_documents.test.mjs` asserts every family the
 *  engine can report has an entry, so a family added to `casual-doc-diff` cannot
 *  render as a blank row. */
export const FAMILY_KEY = Object.freeze({
  block: "compare.family.block",
  text: "compare.family.text",
  formatting: "compare.family.formatting",
  style: "compare.family.style",
  table: "compare.family.table",
  object: "compare.family.object",
  section: "compare.family.section",
  definition: "compare.family.definition",
  resource: "compare.family.resource",
  comment: "compare.family.comment",
  review: "compare.family.review",
  metadata: "compare.family.metadata",
});

/** Why something could not be fully compared -> catalogue key.
 *
 *  THESE ARE LOSS REPORTS AND THEY ARE NOT OPTIONAL. `record.rs` says what each
 *  one is for: "the reader is told *that* it changed and told that the detail is
 *  missing". A panel that collected them and rendered only the change count would
 *  be claiming a completeness the comparison does not have — which is the silent
 *  loss `SKILL` §12 forbids outright, and the whole reason loss-reporting work is
 *  competitive work here rather than hygiene (§1: verbatim retention is only an
 *  advantage if loss is detected and reported).
 *
 *  They arrive AGGREGATED — one row per construct, with a count — so a document
 *  with forty thousand drawings produces one line and not forty thousand. */
export const FINDING_KEY = Object.freeze({
  not_compared: "compare.finding.notCompared",
  ambiguous_match: "compare.finding.ambiguousMatch",
  missing_resource: "compare.finding.missingResource",
  truncated: "compare.finding.truncated",
});

/** The six kinds, in review's own vocabulary — which `record.rs` says is
 *  deliberate: "these strings are the ones review already uses". */
export const KIND_KEY = Object.freeze({
  insertion: "compare.kind.insertion",
  deletion: "compare.kind.deletion",
  move_from: "compare.kind.move_from",
  move_to: "compare.kind.move_to",
  formatting: "compare.kind.formatting",
  property: "compare.kind.property",
});

/**
 * The sidecar, shaped for the panel.
 *
 * Pure. Takes the parsed JSON and returns counts plus a flat, ordered row list —
 * so what the panel renders is decided here, where a unit test can read it,
 * rather than inside a DOM loop.
 *
 * Complexity: O(changes), one pass plus one sort. A comparison that produced
 * 100,000 changes would cost one pass over them and no document walk, because
 * every anchor the engine returns is already resolved.
 */
export function summariseDiff(diff) {
  const changes = Array.isArray(diff?.changes) ? diff.changes : [];
  const byFamily = new Map();
  for (const change of changes) {
    byFamily.set(change.family, (byFamily.get(change.family) ?? 0) + 1);
  }
  const rank = new Map(FAMILY_ORDER.map((family, index) => [family, index]));
  // A family the engine grows and this list has not heard of sorts LAST rather
  // than being dropped. Silently omitting an unknown family is how a comparison
  // comes to report fewer changes than it found.
  const unknown = FAMILY_ORDER.length;
  return {
    total: changes.length,
    complete: diff?.complete === true,
    leftBlocks: diff?.left?.blocks ?? 0,
    rightBlocks: diff?.right?.blocks ?? 0,
    findings: Array.isArray(diff?.findings) ? diff.findings : [],
    families: [...byFamily.entries()]
      .sort((a, b) => (rank.get(a[0]) ?? unknown) - (rank.get(b[0]) ?? unknown))
      .map(([family, count]) => ({ family, count })),
    categories: wordCategories(diff, byFamily),
    // DOCUMENT ORDER, which is the engine's own order and is now simply kept.
    //
    // This used to be `[...changes].sort(by family)`, and that sort was a
    // regression of a guarantee the engine already makes: `DiffJob::finish`
    // sorts `pending` by `(story, path, family, kind)` before sealing the
    // sidecar, and `record.rs` documents `changes` as "in document order of the
    // right side, then of the left". Re-sorting by family threw the position
    // away and handed the reader a list grouped by what a change IS rather than
    // by where it is — which cannot be walked alongside the document, cannot
    // become a unified diff, and was `docs/164` §4 gap 2.
    //
    // Nothing replaces it: the family grouping a reader wants is the per-family
    // COUNTS, which `families` above still carries.
    rows: changes,
  };
}

/** The five categories Word's Reviewing Pane publishes, plus the one kind our
 *  engine has that Word has no name for.
 *
 *  Microsoft's wording is fully sourced and is the competitive spec: the summary
 *  shows "the total number of changes and the number of insertions, deletions,
 *  moves, formatting changes, and comments". So those five ship, in that order,
 *  beside the per-family counts rather than instead of them — a family count
 *  answers "what kind of thing changed" and a category count answers "what
 *  happened to it", and a redline reader counts the second.
 *
 *  THE NUMBERS ARE THE ENGINE'S, not recomputed here. `VersionDiff.kindCounts`
 *  is built in `DiffJob::finish` from the same records this panel lists, so the
 *  breakdown and the total cannot come to disagree — which is the drift a second
 *  count in the host would introduce.
 *
 *  THREE PLACES THE MAPPING IS NOT ONE-TO-ONE, each decided rather than fudged:
 *
 *    * **A move is ONE move.** The engine reports a move as two records,
 *      `move_from` at the origin and `move_to` at the destination, and adding
 *      both would print "2 moves" for one block that moved. The destination is
 *      counted, because every move has exactly one and `move_from` without a
 *      `move_to` is reported through `pasteLoss` rather than as a move.
 *    * **Comments are a FAMILY, not a kind.** Word counts comments among the
 *      five; our engine classifies a comment change by its family and by
 *      whatever happened to it, so the count comes from `familyCounts`.
 *    * **`property` is ours and Word has no word for it.** It is a typed model
 *      field that differs — `alignment`, `spacing.beforeTwips` — and it is
 *      published as a sixth row rather than folded into "formatting", because
 *      folding it would overstate a formatting count, and dropping it would
 *      leave the six rows summing to less than the total with no explanation.
 *      Absence from a published breakdown is an overstatement by omission
 *      (`SKILL` §9.3).
 *
 *  Every row is returned even at zero. "0 deletions" is a fact a reader of a
 *  redline wants, and a surface whose rows come and go cannot be read at a
 *  glance or asserted by a guard.
 *
 *  O(kinds), over a list of six.
 *
 *  @param {object|null|undefined} diff the parsed sidecar.
 *  @param {Map<string, number>} byFamily family -> count, already tallied.
 *  @returns {{category: string, count: number}[]}
 */
export function wordCategories(diff, byFamily) {
  const kinds = new Map();
  for (const entry of Array.isArray(diff?.kindCounts) ? diff.kindCounts : []) {
    if (Array.isArray(entry) && entry.length === 2) kinds.set(entry[0], Number(entry[1]) || 0);
  }
  return [
    { category: "insertions", count: kinds.get("insertion") ?? 0 },
    { category: "deletions", count: kinds.get("deletion") ?? 0 },
    { category: "moves", count: kinds.get("move_to") ?? 0 },
    { category: "formatting", count: kinds.get("formatting") ?? 0 },
    { category: "properties", count: kinds.get("property") ?? 0 },
    { category: "comments", count: byFamily.get("comment") ?? 0 },
  ];
}

/** Word's five categories plus ours -> catalogue key, written out for the reason
 *  `FAMILY_KEY` is: a composed `t()` key is a key no translator is ever shown. */
export const CATEGORY_KEY = Object.freeze({
  insertions: "compare.summary.insertions",
  deletions: "compare.summary.deletions",
  moves: "compare.summary.moves",
  formatting: "compare.summary.formatting",
  properties: "compare.summary.properties",
  comments: "compare.summary.comments",
});

/** The story a change is in, as a sentence. `null` for the body, because saying
 *  "in the body" on every row of a body-only comparison is noise. */
export function storyLabel(story) {
  switch (story?.kind) {
    case "header":
      return t("compare.story.header", { section: n((story.section ?? 0) + 1) });
    case "footer":
      return t("compare.story.footer", { section: n((story.section ?? 0) + 1) });
    case "footnote":
      return t("compare.story.footnote", { number: n((story.index ?? 0) + 1) });
    case "endnote":
      return t("compare.story.endnote", { number: n((story.index ?? 0) + 1) });
    case "comment":
      return t("compare.story.comment");
    case "definitions":
      return t("compare.story.definitions");
    default:
      return null;
  }
}

/** The TEXT a change is about, or `""`.
 *
 *  An insertion has only a right side and a deletion only a left, so this is not
 *  "prefer one": it is "whichever side exists", and a property change has
 *  neither, which is why the field list carries that row instead. */
export function changeText(change) {
  const text = change.kind === "deletion" ? change.leftText : change.rightText;
  return String(text ?? change.leftText ?? change.rightText ?? "");
}

/**
 * WHAT A ROW IS ABOUT, when it is not about text.
 *
 * Measured in Chromium on 2026-10-04: bold one word and compare, and three of
 * the four rows name nothing at all.
 *
 *   <li data-compare-kind="formatting"><span class="compare-kind">Reformatted</span></li>
 *   <li data-compare-kind="property" data-compare-change-family="object">
 *     <span class="compare-kind">Property changed</span></li>
 *   <li data-compare-kind="property" data-compare-change-family="section">
 *     <span class="compare-kind">Property changed</span>
 *     <span class="compare-where">in the document's definitions</span></li>
 *
 * "Reformatted." That is the whole entry. The owner's words on this surface were
 * "what the fuck will i understand from this", and they are literally correct:
 * the row says a change happened and refuses to say what changed.
 *
 * `changeText`'s own doc comment already says where the answer lives — "a
 * property change has neither, which is why the field list carries that row
 * instead" — and `renderResult` never rendered `change.fields`. The intention was
 * written down and not implemented, which is why reading the code made the surface
 * look finished. `fields` is reflected from the model type's own serde names
 * (`record.rs`), so it is exactly the typed path that differs: `alignment`,
 * `spacing.beforeTwips`, `inlineObject`, `revision`, `story`, `runBoundary`.
 *
 * THE PATHS ARE SHOWN VERBATIM, and that is a decision rather than laziness. They
 * are engine identifiers, not prose, and this module has an established rule for
 * exactly that case: an unknown family and an unknown finding code both render
 * with their own name rather than blank, because a name a reader can search for
 * beats a sentence that says nothing. A hand-written English phrase per model
 * field would be a second place to update and the first to rot, and it would
 * silently omit every field added to the model after it was written.
 *
 * O(fields) over a list the engine bounds per change.
 */
export function changeFields(change) {
  const fields = Array.isArray(change?.fields) ? change.fields : [];
  return fields.filter((field) => typeof field === "string" && field.length > 0);
}

/** The bracketed name for a row with no text and no typed fields.
 *
 *  ONLYOFFICE's convention, and a deliberate partial match to it. Theirs reads
 *  `<Image>`, `<Shape>`, `<Chart>` or `<Equation>`; ours can only be as specific
 *  as the sidecar, and the sidecar's `DiffFamily` does not distinguish a picture
 *  from a shape from a chart — `family_of` in `casual-doc-diff/src/job.rs` maps a
 *  row or cell to `table` and EVERYTHING ELSE to `block`. So a deleted
 *  image-only paragraph is `<Block>` here and `<Image>` there.
 *
 *  That gap is the engine's and is reported as such rather than guessed at: the
 *  comparison would have to tag an untexted block with its construct for us to
 *  say "Image". Printing `<Image>` on a `block` row because images are the
 *  commonest untexted block would be the fabrication this repository has
 *  published twice (SKILL §9).
 *
 *  Enumerated over every family rather than defaulted, per SKILL §9.3 — absence
 *  from a matrix is an overstatement by omission — and
 *  `compare_documents.test.mjs` fails if a family the engine can report has no
 *  entry here. O(1). */
export const OBJECT_KEY = Object.freeze({
  block: "compare.object.block",
  text: "compare.object.text",
  formatting: "compare.object.formatting",
  style: "compare.object.style",
  table: "compare.object.table",
  object: "compare.object.object",
  section: "compare.object.section",
  definition: "compare.object.definition",
  resource: "compare.object.resource",
  comment: "compare.object.comment",
  review: "compare.object.review",
  metadata: "compare.object.metadata",
});

/** The bracketed object name for a change, or `""` when the family is unknown to
 *  this build — in which case the raw family name is shown instead, by the same
 *  rule the rest of this module follows. O(1). */
export function changeObjectName(change) {
  const family = String(change?.family ?? "");
  if (!family) return "";
  return OBJECT_KEY[family] ? t(OBJECT_KEY[family]) : `<${family}>`;
}

/** `applyDiffAsRevisions`' coded refusals -> catalogue key.
 *
 *  ONE ROUTING MECHANISM, NOT TWO, and this is the half of it that belongs here.
 *  `casual-doc-edit/src/refusal.rs` ships a refusal as `refused: <sentence>\u{1f}<code>`
 *  and `to_js` splits the code onto the thrown `Error`; `edit_errors.mjs`'s
 *  `editRefusalMessage` is the one function that turns such a throw into a
 *  sentence, and it takes the code->sentence routing as an argument precisely so
 *  the policy stays in one place while each surface supplies its own vocabulary:
 *  *"The caller supplies the routing, not this module."* So these four rows are
 *  fed to that seam (`applyAsRevisions`), never consulted beside it.
 *
 *  It is scoped to the four codes THIS surface can produce, exactly as
 *  `FINDING_KEY` is scoped to the finding codes this surface can produce. The
 *  general table, `session_access.mjs`'s `REFUSAL_KEYS`, covers three families —
 *  `session.*`, `document.protected-*` and `ODC-7xxx` — and its derived guard
 *  reads `access.rs`, `protection.rs` and `docs/20`, none of which mint a
 *  `compare.*` code; the engine has ~130 coded refusals and those three families
 *  are 9 of them. Adding compare's four there would put one surface's nouns in
 *  another surface's module and leave the other ~120 no better off. A code
 *  neither table knows still falls through to the engine's own sentence, which is
 *  more specific than anything a generic replacement could say.
 *
 *  `review.author-required` is defence in depth: `applyAsRevisions` never passes
 *  an empty author. It is routed anyway, because a refusal whose only
 *  reader-facing form is English prose is a refusal that will be read in English.
 *
 *  `compare_documents.test.mjs` derives the expected set FROM THE RUST and fails
 *  the build if the engine grows a fifth. O(1). */
export const REFUSAL_KEY = Object.freeze({
  "compare.document-has-revisions": "compare.refused.documentHasRevisions",
  "compare.schema-unsupported": "compare.refused.schemaUnsupported",
  "compare.sidecar-unreadable": "compare.refused.sidecarUnreadable",
  "review.author-required": "compare.refused.authorRequired",
});

/** This surface's half of `editRefusalMessage`'s routing: a coded refusal's
 *  localised sentence, or `""` for a code it does not know — which is the seam's
 *  contract for "fall back to the engine's own sentence". O(1). */
function routeCompareRefusal(code) {
  return REFUSAL_KEY[code] ? t(REFUSAL_KEY[code]) : "";
}

/** What a comparison found and a tracked change cannot say -> catalogue key.
 *
 *  `EditResult.pasteLoss`, whose name reads oddly for a comparison and is used
 *  anyway: the engine's own contract is that the next path to degrade something
 *  must not invent a second channel, and a second channel is what this host would
 *  then have to grow a second renderer for.
 *
 *  `pasteLossMessage` is NOT the renderer for these. That function filters its
 *  input against the five paste families and returns `null` when none match — so
 *  routing a comparison through it would drop every key below except
 *  `trackedMove`, and drop it silently, which is the exact failure this report
 *  exists to prevent. Two vocabularies, two mappings; the mechanism they share is
 *  "stable key in, localised noun out, unknown keys still shown".
 *
 *  **Twelve of the twenty-two keys are the engine's `DiffFamily` names**
 *  (`family_loss_key`), reported when a family has no inline revision form at all
 *  — `RevisionKind` is Insertion/Deletion/MoveFrom/MoveTo and nothing else, so a
 *  formatting, style, section, definition, resource, comment or metadata
 *  difference cannot be EXPRESSED however faithfully it was detected. Those route
 *  through `OBJECT_KEY`, which already names all twelve in all nineteen
 *  catalogues: a thirteenth noun for `formatting` would be a second spelling of
 *  one thing in one panel.
 *
 *  `otherStory` is here because the engine applies a comparison to the BODY only,
 *  deliberately: the right-hand side is a re-export of this document, so a body
 *  path maps back by identity, while a header, footer, note or comment story is
 *  paired by semantic position or by ordinal and depends on the export writing
 *  the same section structure back — unmeasured, and an unmeasured mapping would
 *  place a revision in the wrong header rather than refuse to.
 *
 *  The six below are the reasons that are not a family. Five engine keys share
 *  `notMarkable` on purpose — `unresolvedAnchor`, `offsetSpace`,
 *  `nonParagraphBlock`, `notParagraphText` and `insertionNotText` are five
 *  internal distinctions about offsets and block kinds, and one reader-facing
 *  fact: the difference is real and there is no run of text here to mark it on.
 *  Five near-identical sentences about anchor spaces would be noise presented as
 *  precision. O(1). */
export const UNMARKED_KEY = Object.freeze({
  blockDeletion: "compare.unmarked.blockDeletion",
  trackedMove: "compare.unmarked.trackedMove",
  otherStory: "compare.unmarked.otherStory",
  truncatedText: "compare.unmarked.truncatedText",
  incompleteComparison: "compare.unmarked.incompleteComparison",
  unresolvedAnchor: "compare.unmarked.notMarkable",
  offsetSpace: "compare.unmarked.notMarkable",
  nonParagraphBlock: "compare.unmarked.notMarkable",
  notParagraphText: "compare.unmarked.notMarkable",
  insertionNotText: "compare.unmarked.notMarkable",
  ...OBJECT_KEY,
});

/**
 * The loss report, as `{key, label}` rows: one per distinct SENTENCE, with
 * nothing dropped.
 *
 * The order is the ENGINE's, not one invented here: `apply_diff_as_revisions`
 * collects into a `BTreeSet<&'static str>`, so the keys arrive sorted and a
 * second comparison of the same two documents reports them in the same order.
 * Re-sorting by this host's own idea of importance would be a second ordering of
 * one list, and a reader comparing two runs would see it move.
 *
 * Separate from the DOM so the filtering, the de-duplication and the
 * never-silent rule are testable without a document — and so an unknown key is
 * provably carried rather than provably convenient to drop.
 *
 * De-duplicated by catalogue key rather than by engine key, because the five
 * `notMarkable` aliases would otherwise print one identical sentence five times.
 * An engine key this build has no sentence for keeps its own name and still
 * shows: a loss report dropped because the host has no wording for it is the
 * defect twice over.
 *
 * O(keys).
 *
 * @param {readonly string[] | undefined | null} keys `EditResult.pasteLoss`.
 * @returns {{key: string, label: string}[]}
 */
export function unmarkedReasons(keys) {
  if (!Array.isArray(keys)) return [];
  const rows = [];
  const seen = new Set();
  for (const raw of keys) {
    const key = String(raw ?? "");
    if (!key) continue;
    const catalogue = UNMARKED_KEY[key];
    const dedupe = catalogue ?? `raw:${key}`;
    if (seen.has(dedupe)) continue;
    seen.add(dedupe);
    rows.push({ key, label: catalogue ? t(catalogue) : key });
  }
  return rows;
}

// ---------------------------------------------------------------------------
// THE UNIFIED DIFF — GitHub's shape, over blocks instead of lines
// ---------------------------------------------------------------------------
//
// The owner's words: "diff should be like how GitHub diff appears on a PR —
// things added or removed, on that changes, while you can expect to see more."
//
// ## Why unified and not side-by-side
//
// Side-by-side needs two synchronised document renders and our body is ONE
// canvas — a second render surface is a second paginator, a second scroll
// coupling and a second place every fidelity fix has to land. Word's own
// tri-pane is a desktop app's answer and its own documentation admits the
// reading surface it produces "is not the best tool for making changes". A
// unified diff needs one column, which is what we have.
//
// ## The known pattern, named before the code (`SKILL` §8)
//
// Three of them, and none is new:
//
//   1. **A unified diff with hunks.** Changed regions, each with a few lines of
//      unchanged context, separated by elisions that can be expanded. Our unit
//      is a BLOCK rather than a text line, because the engine's projection is a
//      block forest — which is the honest unit, not an approximation of one.
//   2. **Windowed (virtual) scrolling.** A flat row array of FIXED height, a
//      sizer of `rows × height`, and only the visible slice in the DOM. This is
//      what makes a scroll tick O(window) rather than O(changes), which `docs/107`
//      §4 requires of every interaction.
//   3. **Lazy pull for context.** The unchanged blocks are not in the sidecar —
//      a change record names only what changed — so they are fetched from the
//      engine per hunk, O(depth) each, exactly as GitHub fetches the lines it
//      elided. `WasmVersionDiff.blockTextAt` is that read.
//
// The three pure functions below produce the hunks and the flat rows, so the
// grouping and the ordering are testable in node without a DOM.

/** Unchanged blocks shown on each side of a hunk before anything is expanded.
 *  GitHub's own default, and it is a default rather than a setting because a
 *  figure nobody has asked to change is one more thing to persist and migrate. */
export const DIFF_CONTEXT = 3;

/** How many more blocks one press of an expand control reveals on that side. */
export const DIFF_EXPAND_STEP = 10;

/** One diff row's height in CSS pixels.
 *
 *  JS OWNS THIS NUMBER and writes it onto the container as `--diff-row-h`, so
 *  the stylesheet cannot drift from the arithmetic the window is computed with.
 *  A virtualizer whose row height disagrees with the CSS scrolls to the wrong
 *  place and there is no symptom except that. Rows are therefore `nowrap` with
 *  their own horizontal overflow — which is also what a GitHub diff line does,
 *  and is the one exception `SKILL`'s no-horizontal-scroll rule allows for a
 *  code-shaped block in its own `overflow-x` container. */
export const DIFF_ROW_HEIGHT = 22;

/** Rows rendered beyond each edge of the viewport, so a fast scroll does not
 *  show a blank band before the next frame lands. */
export const DIFF_OVERSCAN = 6;

/** The sibling index of the block a `DiffAnchor.path` names, or `null` when the
 *  path ends at a row or a cell — which `block_at_path` refuses to resolve, so
 *  neither does this. O(1). */
export function blockIndexOfPath(path) {
  const last = Array.isArray(path) && path.length > 0 ? path[path.length - 1] : null;
  return last && last.kind === "block" && Number.isInteger(last.index) ? last.index : null;
}

/** The same path with its final block index set to `index`, or `null` when the
 *  path does not end at a block or `index` is before the first sibling.
 *
 *  This is how a caller asks the engine for a NEIGHBOUR: there is no "next
 *  sibling" call and there should not be one, because the end of a sibling list
 *  is exactly what `blockTextAt` returning `null` already tells you. O(depth). */
export function pathAtIndex(path, index) {
  if (blockIndexOfPath(path) === null || !Number.isInteger(index) || index < 0) return null;
  return [...path.slice(0, -1), { kind: "block", index }];
}

/** The side of a change a unified diff reads its coordinates from: the right
 *  (newer) when it has one, else the left. An insertion has only a right and a
 *  deletion only a left, so this is "whichever exists" rather than a preference,
 *  and a change with neither is unplaceable and says so. O(1). */
export function anchorOf(change) {
  if (change?.right) return { side: "right", anchor: change.right };
  if (change?.left) return { side: "left", anchor: change.left };
  return null;
}

/**
 * Groups ordered changes into hunks.
 *
 * Two changes share a hunk when they sit on the same side, in the same story,
 * under the same parent container, and within `context * 2` blocks of each
 * other — the standard rule, and the reason it is `context * 2` is that any
 * wider gap would render as two context runs with nothing between them, which
 * is two hunks wearing one header.
 *
 * `rows` must already be in document order. It is: `DiffJob::finish` sorts by
 * `(story, path, family, kind)` and `summariseDiff` now keeps that order. A
 * caller that hands this a family-sorted list gets hunks that interleave
 * positions, which is the bug this ordering was restored to prevent.
 *
 * A change with no anchor at all becomes its own unplaceable hunk — rendered
 * with no context and no expand controls, because there is no position to expand
 * around. It is NOT dropped: a difference the engine found and this surface
 * could not place is still a difference, and dropping it is the silent loss
 * `AGENTS.md` puts first.
 *
 * Complexity: **O(changes)**, one pass, no document access.
 *
 * @param {readonly object[]} rows ordered change records.
 * @param {{context?: number}} [options]
 */
export function diffHunks(rows, { context = DIFF_CONTEXT } = {}) {
  const hunks = [];
  for (const change of Array.isArray(rows) ? rows : []) {
    const placed = anchorOf(change);
    const index = placed ? blockIndexOfPath(placed.anchor.path) : null;
    if (!placed || index === null) {
      hunks.push({
        placeable: false,
        side: null,
        story: null,
        path: null,
        start: null,
        end: null,
        changes: [change],
        before: 0,
        after: 0,
      });
      continue;
    }
    const storyKey = JSON.stringify(placed.anchor.story ?? null);
    const parentKey = JSON.stringify(placed.anchor.path.slice(0, -1));
    const last = hunks[hunks.length - 1];
    if (
      last?.placeable &&
      last.side === placed.side &&
      last.storyKey === storyKey &&
      last.parentKey === parentKey &&
      index - last.end <= context * 2
    ) {
      last.changes.push(change);
      last.end = Math.max(last.end, index);
      continue;
    }
    hunks.push({
      placeable: true,
      side: placed.side,
      story: placed.anchor.story ?? null,
      storyKey,
      parentKey,
      path: placed.anchor.path,
      start: index,
      end: index,
      changes: [change],
      before: context,
      after: context,
    });
  }
  return hunks;
}

/**
 * The lines one change contributes, in GitHub's order: removed, then added.
 *
 * Decided by WHICH TEXTS EXIST rather than by the kind label, because the kinds
 * do not partition the cases: a `formatting` change has the same text on both
 * sides, a `property` change has neither, and the engine's weaker-key second
 * pass pairs an edited block against the block it replaced, which has both and
 * is a `text` change. The kind still rides on every line, because it is what the
 * row is LABELLED with.
 *
 * A change with no text on either side renders one `meta` line — the typed field
 * paths or the bracketed object name, which `changeFields` and `changeObjectName`
 * already produce. That is the row that used to read only "Reformatted".
 *
 * O(1).
 */
export function changeLines(change, hunk) {
  const left = String(change?.leftText ?? "");
  const right = String(change?.rightText ?? "");
  const line = (kind, text) => ({ kind, hunk, change, text });
  if (left && right && left !== right) return [line("del", left), line("add", right)];
  if (left && !right) return [line("del", left)];
  if (right && !left) return [line("add", right)];
  if (left && right) return [line("meta", right)];
  return [line("meta", "")];
}

/**
 * The flat, fixed-height row array the window is cut from.
 *
 * Every visual element is a row — the hunk header, each expand control, each
 * context block, each removed and added line — because a virtualizer can only
 * be exact over a uniform array. Variable heights would mean measuring, and
 * measuring every row is the O(changes) work the window exists to avoid.
 *
 * Context rows carry a `delta` relative to the hunk's `start` (negative, above)
 * or `end` (positive, below) rather than an absolute index, so expanding a hunk
 * changes one number and the paths are recomputed from it.
 *
 * Complexity: **O(changes + total context shown)**, which is bounded by what the
 * reader has expanded. Called when a hunk is expanded, not per scroll tick.
 */
export function diffLines(hunks) {
  const lines = [];
  for (const [index, hunk] of (Array.isArray(hunks) ? hunks : []).entries()) {
    lines.push({ kind: "hunk", hunk: index });
    if (hunk.placeable) lines.push({ kind: "expand", hunk: index, side: "before" });
    for (let step = hunk.before; step >= 1; step -= 1) {
      lines.push({ kind: "context", hunk: index, delta: -step });
    }
    for (const change of hunk.changes) lines.push(...changeLines(change, index));
    for (let step = 1; step <= hunk.after; step += 1) {
      lines.push({ kind: "context", hunk: index, delta: step });
    }
    if (hunk.placeable) lines.push({ kind: "expand", hunk: index, side: "after" });
  }
  return lines;
}

/**
 * The slice of a flat row array a scroller is showing.
 *
 * Pure arithmetic, separated so the one thing a virtualizer gets wrong — the
 * window — is testable without a browser. Clamped at both ends, so a scroller
 * taller than its content, a negative `scrollTop` (which rubber-banding
 * produces) and an empty list all return a valid range rather than a negative
 * length.
 *
 * O(1), which is the whole point: this runs on every scroll tick.
 */
export function diffWindow(count, scrollTop, viewport, { rowHeight = DIFF_ROW_HEIGHT, overscan = DIFF_OVERSCAN } = {}) {
  const total = Math.max(0, Math.trunc(count));
  if (total === 0) return { first: 0, last: 0 };
  const top = Math.max(0, Number(scrollTop) || 0);
  const height = Math.max(rowHeight, Number(viewport) || rowHeight);
  // BOTH ends are clamped to the list. Clamping only `last` leaves `first` past
  // the end on an over-scrolled container, and although the window is then empty
  // so nothing renders, `translateY(first × rowHeight)` would be written for a
  // position that does not exist — a latent wrong answer waiting for the first
  // row to be rendered into it.
  const first = Math.min(total, Math.max(0, Math.floor(top / rowHeight) - overscan));
  const last = Math.min(total, Math.ceil((top + height) / rowHeight) + overscan);
  return { first, last: Math.max(first, last) };
}

/**
 * Binds the Compare panel and its two entry points.
 *
 * @param {object} io
 * @param {() => object|null} io.doc the live document.
 * @param {() => Uint8Array|null} io.currentBytes this document, exported.
 * @param {object} io.engine `{ begin, slice }` — the two wasm free functions.
 * @param {() => Promise<void>} io.yieldToHost
 * @param {(text: string, kind?: string) => void} io.setStatus
 * @param {() => boolean} io.allowed whether the host granted what this needs.
 * @param {string} io.refusedReason what to say when it did not.
 * @param {(res: object) => Promise<void>} [io.landed] ADR-061's one new seam:
 *   repaint, turn the markup view on, re-render the review gutter — in that
 *   order, for an `EditResult` this module has already read `pasteLoss` off.
 *   OPTIONAL, and absence is not a second code path: a composition that withheld
 *   it (no review chrome) gets the index with no "now tracked changes" claim on
 *   it, which is the one sentence that would be false there.
 * @param {() => string} [io.blockedReason] the sentence for a mutation the host
 *   blocks before the engine sees it (Viewing mode), or `""`. A comparison WRITES
 *   revisions, so it is a mutation and goes through that gate like every other.
 * @param {() => string} [io.readOnlyReason] the engine's own
 *   `editingUnavailableReason`, for a document no edit can ever apply to.
 * @param {(anchor: object) => unknown} [io.navigate] scrolls the live document to
 *   a change, given that change's RIGHT-hand `DiffAnchor`. It must resolve the
 *   anchor's `story` + `path` through `nodeAtStoryPath` and must never use the
 *   anchor's own `node`, which addresses a throwaway parse. Absent, change rows
 *   render as text rather than as buttons — the Review ▸ Compare route supplies
 *   it, and version history's read-only route has nowhere to scroll to.
 */
export function bindComparePanel(io) {
  const panel = document.getElementById("comparePanel");
  const body = document.getElementById("compareBody");
  const fileInput = document.getElementById("compareFile");
  // BOTH faces of `review.compare`, owned here. The Review band's button and the
  // rail's entry are one command, so one thing owns both their clicks and both
  // their pressed states — the argument `version_panel.mjs` makes for its own
  // pair: two owners of one button is how a control comes to say one thing and do
  // another, and two buttons for one command with two owners is that twice. Their
  // DISABLED state is the surface table's, which is why the row declares
  // `ownsClick` and lists both.
  const entryPoints = [
    document.getElementById("reviewCompareBtn"),
    document.getElementById("railCompare"),
  ].filter(Boolean);
  if (!panel || !body || !fileInput || entryPoints.length === 0) {
    // No surface in this composition — an embed built without the Compare panel.
    // `compareVersions: null` is what the version panel's Show changes row tests,
    // so it reports the capability as unavailable rather than throwing on click.
    return { open: () => {}, compareWith: null, compareVersions: null };
  }

  let cancelled = false;
  let running = false;

  function setOpen(open) {
    panel.hidden = !open;
    for (const button of entryPoints) button.setAttribute("aria-pressed", String(open));
  }

  function render(children) {
    body.replaceChildren(...children);
  }

  function paragraph(text, className) {
    const element = document.createElement("p");
    if (className) element.className = className;
    element.textContent = text;
    return element;
  }

  /** The chooser: what to compare against. Word's Compare dialog in one panel. */
  function renderChooser() {
    const choose = document.createElement("button");
    choose.type = "button";
    choose.className = "dialog-button dialog-button-primary";
    choose.dataset.compareAction = "choose-file";
    choose.textContent = t("compare.chooseFile");
    choose.disabled = !io.allowed();
    if (!io.allowed()) choose.title = io.refusedReason;
    choose.addEventListener("click", () => fileInput.click());
    render([
      paragraph(t("compare.intro"), "muted"),
      choose,
      // Said up front, not discovered afterwards — and it now says what ADR-061
      // decided rather than what the previous shape could manage. Word and Google
      // Docs build a merged THIRD document; we mutate the open one, which is
      // ONLYOFFICE's answer, and the difference is visible to the reader the
      // moment they pick a file. It is also the warning that matters: this writes
      // tracked changes into the document on screen.
      paragraph(t("compare.writesTrackedChanges"), "muted"),
    ]);
  }

  function renderProgress(progress) {
    const cancel = document.createElement("button");
    cancel.type = "button";
    cancel.className = "dialog-button";
    cancel.dataset.compareAction = "cancel";
    cancel.textContent = t("compare.cancel");
    cancel.addEventListener("click", () => {
      cancelled = true;
    });
    // Indeterminate until the SECOND side has been parsed, because
    // `blocksTotal()` is 0 until then — a determinate bar before that would be
    // pretending to know how far along it is.
    const bar = document.createElement("progress");
    if (progress.total > 0) {
      bar.max = progress.total;
      bar.value = Math.min(progress.done, progress.total);
    }
    bar.dataset.compareProgress = progress.phase;
    render([
      paragraph(progress.phase === PARSING ? t("compare.parsing") : t("compare.comparing")),
      bar,
      cancel,
    ]);
  }

  /** The loss reports, as elements — empty when there are none.
   *
   *  `record.rs` aggregates these on purpose (one row per construct, with a
   *  count), so a document with forty thousand drawings produces one line. The
   *  engine does that work precisely so a host can SAY what it could not compare,
   *  and collecting them without rendering them would be the silent loss `SKILL`
   *  §12 forbids. */
  function findingNotes(summary) {
    if (summary.findings.length === 0) return [];
    const notes = document.createElement("ul");
    notes.className = "compare-list";
    for (const finding of summary.findings) {
      const item = document.createElement("li");
      item.dataset.compareFinding = finding.code;
      const key = FINDING_KEY[finding.code];
      // An unknown code still SHOWS, with its own name and count: a loss report
      // dropped because this build has no sentence for it is the defect twice.
      item.textContent = key
        ? t(key, { construct: finding.construct, count: finding.count })
        : `${finding.construct}: ${finding.code} (${n(finding.count)})`;
      notes.append(item);
    }
    return [paragraph(t("compare.findingsTitle"), "muted"), notes];
  }

  /** WHAT THE COMPARISON FOUND AND COULD NOT MARK, as elements.
   *
   *  Rendered above the index and below the "now tracked changes" sentence,
   *  because that sentence is a claim about the document and this is its
   *  qualification: a reader who is told nine differences are in the document
   *  needs to know in the same breath that three of twelve are not. The engine
   *  computed the report so a host could say it; collecting it and not rendering
   *  it is the silent loss `AGENTS.md` puts first. */
  function unmarkedNotes(loss) {
    const rows = unmarkedReasons(loss);
    if (rows.length === 0) return [];
    const notes = document.createElement("ul");
    notes.className = "compare-list";
    for (const row of rows) {
      const item = document.createElement("li");
      item.dataset.compareUnmarked = row.key;
      item.textContent = row.label;
      notes.append(item);
    }
    return [paragraph(t("compare.unmarkedTitle"), "muted"), notes];
  }

  function renderResult(summary, otherName, applied = null, options = null) {
    const children = [
      paragraph(options?.heading ?? t("compare.against", { name: otherName })),
    ];
    // Said before the numbers, because it is what the reader is owed first on
    // this route: the comparison they just asked for changed nothing. Only the
    // read-only route says it; saying it where ADR-061 applies would be false.
    if (options?.readOnly) {
      const note = paragraph(t("compare.readOnlyProjection"), "muted");
      note.dataset.compareReadOnly = "1";
      children.push(note);
    }
    // "No differences" is a CLAIM, and it is only honest when the comparison
    // actually compared everything. A document whose drawings this build has no
    // typed comparison for can produce zero changes and a `not_compared`
    // finding — saying "No differences" there would tell a reader the two files
    // agree about something the engine never looked at. So the finding wins, and
    // the sentence becomes "no differences in what could be compared".
    if (summary.total === 0) {
      // OWNERSHIP OF THE RETAINED HANDLE ENDS HERE on this branch. There is no
      // diff to render, so nothing takes it, and leaving it alive would hold two
      // parsed documents for a panel that says "No differences". Every other
      // branch hands it to `diffElements`, which owns it until `releaseDiff`.
      releaseDiff();
      options?.job?.free?.();
      children.push(
        paragraph(summary.findings.length > 0 ? t("compare.identicalPartly") : t("compare.identical")),
      );
      if (summary.findings.length > 0) children.push(...findingNotes(summary));
      render(children);
      return;
    }
    // THE ANSWER FIRST, and it is a sentence about the DOCUMENT rather than a
    // number about the panel. ADR-061: the differences are tracked changes now,
    // so the reader's next move is to read the document, and the second sentence
    // says which controls walk them — Review's own next/previous, which navigate
    // the revisions the comparison just wrote.
    //
    // Only when a comparison was actually applied. A refusal renders its own
    // sentence instead and never reaches here, and a composition with no
    // `applyAsRevisions` gets the index alone: claiming a document holds tracked
    // changes it does not hold is the overstatement SKILL §9 exists for.
    if (applied) {
      const marked = document.createElement("p");
      // The count stays an ATTRIBUTE here and a sentence one line down: see
      // `compare.marked`, which lost its `{count}` after rendering "1
      // differences". A guard still reads the number off this.
      marked.dataset.compareMarked = String(summary.total);
      marked.textContent = t("compare.marked");
      children.push(marked, paragraph(t("compare.reviewNav"), "muted"));
      children.push(...unmarkedNotes(applied.loss));
    }
    const total = document.createElement("p");
    total.dataset.compareTotal = String(summary.total);
    total.textContent = t("compare.changeCount", { count: summary.total });
    children.push(total);
    // CORRECTED 2026-10-04, and the comment it replaces was wrong about the
    // engine. It said "`complete: false` only happens on a cancelled job, which
    // this code never renders". `record.rs` says the opposite in as many words:
    // `complete` is "**False whenever `findings` is non-empty**". So every
    // ordinary comparison that met one construct this build has no typed
    // comparison for was being labelled with "This comparison did not finish, so
    // the list below is incomplete" — measured on bolding one word in the demo
    // document, which produces one `not_compared` finding for an inline object.
    //
    // A comparison that finished and skipped something is not a comparison that
    // did not finish, and telling a reader their comparison broke when it did not
    // is the fastest way to make a working surface untrustworthy. The sentence now
    // says what the flag means, and the findings list immediately below says which
    // constructs. A genuinely interrupted job is the `CANCELLED` branch in
    // `compareWith` and has its own sentence.
    if (!summary.complete) children.push(paragraph(t("compare.partial"), "muted"));
    const list = document.createElement("ul");
    list.className = "compare-list";
    for (const { family, count } of summary.families) {
      const item = document.createElement("li");
      item.dataset.compareFamily = family;
      // A family this build has no key for still SHOWS, with its own name and
      // its count, rather than rendering blank. The guard fails the build for it;
      // this is what the reader sees meanwhile, and it is strictly better than a
      // row that says nothing about changes that are really there.
      item.textContent = FAMILY_KEY[family]
        ? t(FAMILY_KEY[family], { count })
        : `${family}: ${n(count)}`;
      list.append(item);
    }
    children.push(list);
    // WORD'S FIVE CATEGORIES, plus the one kind Word has no name for.
    // `wordCategories` says why each mapping is what it is. Rendered after the
    // families and before the findings: "what happened" after "to what", and
    // both before the limits of the answer.
    const summaryList = document.createElement("ul");
    summaryList.className = "compare-list compare-categories";
    for (const { category, count } of summary.categories) {
      const item = document.createElement("li");
      item.dataset.compareCategory = category;
      item.dataset.compareCategoryCount = String(count);
      item.textContent = t(CATEGORY_KEY[category], { count });
      summaryList.append(item);
    }
    children.push(paragraph(t("compare.summary.title"), "muted"), summaryList);
    // WHAT COULD NOT BE COMPARED, before the changes themselves: a reader
    // deciding whether to trust this list needs its limits before they start
    // reading, not after.
    children.push(...findingNotes(summary));
    children.push(...diffElements(summary, options));
    render(children);
  }

  // ---------------------------------------------------------------------------
  // The unified diff view: a windowed row list over `diffLines`
  // ---------------------------------------------------------------------------

  /** Blocks of fetched context text held at once.
   *
   *  A cap rather than a weak map, because the keys are strings. It is generous
   *  — a reader cannot expand two thousand blocks without noticing — and the
   *  overflow behaviour is a clear rather than an eviction policy: an LRU here
   *  would be a cache implementation nobody asked for, and re-fetching a block
   *  is one O(depth) engine call. */
  const CONTEXT_CACHE_MAX = 2_000;

  /** The live diff view, or `null`. Holds the RETAINED engine handle, so
   *  `releaseDiff` is the one place that frees it and every path that replaces
   *  or closes a result goes through it. */
  let diffView = null;

  /** Drops the view and the two parsed documents behind it. Idempotent. O(1). */
  function releaseDiff() {
    if (!diffView) return;
    diffView.scroller?.removeEventListener("scroll", diffView.onScroll);
    if (diffView.frame) cancelAnimationFrame(diffView.frame);
    diffView.job?.free?.();
    diffView = null;
  }

  /** One block of context text on a hunk's side, or `null` at the end of the
   *  sibling list — which is how the expand control learns there is no more.
   *
   *  Complexity: **O(depth)** per uncached block, in the engine. Per
   *  interaction, never per document. */
  function contextText(hunk, delta) {
    if (!diffView?.job || !hunk.placeable) return null;
    const base = delta < 0 ? hunk.start : hunk.end;
    const path = pathAtIndex(hunk.path, base + delta);
    if (!path) return null;
    const serialised = JSON.stringify(path);
    const key = `${hunk.side}|${hunk.storyKey}|${serialised}`;
    if (diffView.cache.has(key)) return diffView.cache.get(key);
    let text = null;
    try {
      text = diffView.job.blockTextAt(hunk.side, hunk.storyKey, serialised) ?? null;
    } catch {
      // A path that does not resolve is an ANSWER — the document stops here —
      // and the engine returns `None` for it rather than throwing. This catch is
      // for a freed handle reached by a stale frame, which must not take the
      // panel down with it.
      text = null;
    }
    if (diffView.cache.size >= CONTEXT_CACHE_MAX) diffView.cache.clear();
    diffView.cache.set(key, text);
    return text;
  }

  /** The hunk header: where in the document this run of changes is. */
  function hunkRow(hunk) {
    const label = document.createElement("span");
    label.className = "compare-diff-at";
    label.textContent = hunk.placeable
      ? t("compare.diff.at", { position: n(hunk.start + 1) })
      : t("compare.diff.unplaced");
    const parts = [label];
    const where = storyLabel(hunk.story);
    if (where) {
      const story = document.createElement("span");
      story.className = "compare-where";
      story.textContent = where;
      parts.push(story);
    }
    return parts;
  }

  /** An expand control, disabled WITH ITS REASON when the document stops here —
   *  never a dead control (`SKILL` §10). */
  function expandRow(hunkIndex, side) {
    const hunk = diffView.hunks[hunkIndex];
    const next = side === "before" ? -(hunk.before + 1) : hunk.after + 1;
    const more = contextText(hunk, next) !== null;
    const button = document.createElement("button");
    button.type = "button";
    button.className = "compare-diff-more";
    button.dataset.compareExpand = side;
    button.textContent = t("compare.diff.more");
    button.setAttribute(
      "aria-label",
      side === "before" ? t("compare.diff.moreAbove") : t("compare.diff.moreBelow"),
    );
    button.disabled = !more;
    if (!more) button.title = t("compare.diff.noMoreContext");
    else button.addEventListener("click", () => expandHunk(hunkIndex, side));
    return [button];
  }

  /** Reveals `DIFF_EXPAND_STEP` more blocks on one side of one hunk.
   *
   *  Complexity: O(rows) to rebuild the flat array plus O(step) engine reads for
   *  the blocks that actually become visible. The array rebuild is the one
   *  non-O(window) step on this surface and it happens on an explicit press, not
   *  on a scroll tick. */
  function expandHunk(hunkIndex, side) {
    const hunk = diffView?.hunks?.[hunkIndex];
    if (!hunk) return;
    if (side === "before") hunk.before += DIFF_EXPAND_STEP;
    else hunk.after += DIFF_EXPAND_STEP;
    diffView.lines = diffLines(diffView.hunks);
    diffView.sizer.style.height = `${diffView.lines.length * DIFF_ROW_HEIGHT}px`;
    diffView.dirty = true;
    paintDiff();
  }

  /** The contents of one change line: its kind label, then what it is about.
   *
   *  The ORDER is the one `docs/164` §4 recorded as fixed and this keeps: the
   *  text excerpt first, because an excerpt of the words is what a reader
   *  recognises; then the typed field paths, which are what a formatting or
   *  property change actually is; then the bracketed object name, so a row about
   *  an untexted block still names a thing. A row carrying only its kind label is
   *  the defect, and `compare_documents.test.mjs` fails if one can still happen. */
  function changeRow(line) {
    const change = line.change;
    const kind = document.createElement("span");
    kind.className = "compare-kind";
    kind.textContent = KIND_KEY[change.kind] ? t(KIND_KEY[change.kind]) : change.kind;
    const parts = [kind];
    const fields = changeFields(change);
    if (line.text) {
      const quote = document.createElement("q");
      quote.className = "compare-diff-text";
      quote.textContent = line.text;
      parts.push(quote);
    } else if (fields.length > 0) {
      const named = document.createElement("span");
      named.className = "compare-object";
      named.dataset.compareFields = fields.join(",");
      named.textContent = fields.join(", ");
      parts.push(named);
    } else {
      const named = document.createElement("span");
      named.className = "compare-object";
      named.textContent = changeObjectName(change);
      parts.push(named);
    }
    return parts;
  }

  /** One row of the window, as an element. O(1). */
  function diffRowElement(line, index, total) {
    const item = document.createElement("li");
    item.className = `compare-diff-row compare-diff-${line.kind}`;
    item.dataset.compareRow = line.kind;
    // A windowed list must publish the real list's shape, or a screen reader
    // announces "1 of 40" over a document of four thousand changes.
    item.setAttribute("aria-posinset", String(index + 1));
    item.setAttribute("aria-setsize", String(total));
    const hunk = diffView.hunks[line.hunk];
    if (line.kind === "hunk") {
      item.append(...hunkRow(hunk));
      return item;
    }
    if (line.kind === "expand") {
      item.append(...expandRow(line.hunk, line.side));
      return item;
    }
    if (line.kind === "context") {
      const text = contextText(hunk, line.delta);
      const span = document.createElement("span");
      span.className = "compare-diff-text";
      // A block that is not a paragraph has no projected text — a table, an
      // alt-chunk — and says so rather than rendering an empty line a reader
      // would read as a blank paragraph.
      span.textContent = text ?? "";
      if (text === null) {
        item.classList.add("is-empty");
        span.textContent = t("compare.diff.notText");
      }
      item.append(span);
      return item;
    }
    // A change line. These are the ENTRIES — the index `docs/164` §4 asked to
    // become a navigation surface — so they carry the kind and family a guard
    // and a stylesheet read off, and they are clickable when there is somewhere
    // to go.
    item.dataset.compareKind = line.change.kind;
    item.dataset.compareChangeFamily = line.change.family;
    const target = navigableAnchor(line.change);
    if (target) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "compare-diff-goto";
      button.dataset.compareGoto = line.change.id ?? "";
      button.append(...changeRow(line));
      button.addEventListener("click", () => void io.navigate(target));
      item.append(button);
    } else {
      item.append(...changeRow(line));
    }
    return item;
  }

  /** The right-hand anchor a click can navigate to, or `null`.
   *
   *  NAVIGATION IS ONLY OFFERED WHERE A PATH CAN RESOLVE, which is the Review ▸
   *  Compare route: there the right-hand side IS the open document, so the
   *  change's `right.path` addresses a block the reader is looking at and
   *  `nodeAtStoryPath` turns it into that paragraph's real NodeId. In version
   *  history's read-only route neither compared state is on screen, so there is
   *  nowhere to scroll to — and the unified diff above IS the reading surface,
   *  which is why no row pretends otherwise. `DiffAnchor.node` is never used for
   *  this: it addresses a throwaway parse whose id counter restarted at 1, so
   *  handing it to `navigateToReviewAnchor` lands on an unrelated paragraph with
   *  the same ordinal and reports nothing wrong. O(1). */
  function navigableAnchor(change) {
    if (typeof io.navigate !== "function" || diffView?.readOnly) return null;
    const right = change?.right;
    if (!right || blockIndexOfPath(right.path) === null) return null;
    return right;
  }

  /** Paints the visible slice. O(window) — the rule this surface exists to keep
   *  (`docs/107` §4: per-interaction work is O(1) in document size, and a scroll
   *  tick is an interaction). */
  function paintDiff() {
    const view = diffView;
    if (!view?.scroller) return;
    const { first, last } = diffWindow(
      view.lines.length,
      view.scroller.scrollTop,
      view.scroller.clientHeight,
    );
    if (!view.dirty && first === view.first && last === view.last) return;
    view.first = first;
    view.last = last;
    view.dirty = false;
    const items = [];
    for (let index = first; index < last; index += 1) {
      items.push(diffRowElement(view.lines[index], index, view.lines.length));
    }
    view.list.replaceChildren(...items);
    view.list.style.transform = `translateY(${first * DIFF_ROW_HEIGHT}px)`;
  }

  /**
   * The unified diff, as elements.
   *
   * Replaces a flat `<ol>` that built one `<li>` per change in one synchronous
   * loop — an O(changes) main-thread render, uncapped and unvirtualized, which is
   * the shape `SKILL` §8 forbids and `docs/164` §4 gap 3 recorded.
   *
   * Complexity: **O(hunks) to group, O(rows) to flatten, O(window) to paint**,
   * and O(window) per scroll tick thereafter. The flatten is O(changes) once per
   * comparison and per expand press, not per tick.
   */
  function diffElements(summary, options) {
    releaseDiff();
    if (summary.rows.length === 0) return [];
    const hunks = diffHunks(summary.rows);
    const scroller = document.createElement("div");
    scroller.className = "compare-diff";
    scroller.dataset.compareDiff = String(summary.rows.length);
    scroller.style.setProperty("--diff-row-h", `${DIFF_ROW_HEIGHT}px`);
    const sizer = document.createElement("div");
    sizer.className = "compare-diff-sizer";
    const list = document.createElement("ol");
    list.className = "compare-changes";
    sizer.append(list);
    scroller.append(sizer);
    const onScroll = () => {
      if (diffView?.frame) return;
      diffView.frame = requestAnimationFrame(() => {
        diffView.frame = 0;
        paintDiff();
      });
    };
    diffView = {
      hunks,
      lines: diffLines(hunks),
      job: options?.job ?? null,
      readOnly: options?.readOnly === true,
      scroller,
      sizer,
      list,
      cache: new Map(),
      first: -1,
      last: -1,
      dirty: true,
      frame: 0,
      onScroll,
    };
    sizer.style.height = `${diffView.lines.length * DIFF_ROW_HEIGHT}px`;
    scroller.addEventListener("scroll", onScroll);
    // Painted once now, with the scroller's height still 0 — `diffWindow`
    // clamps the viewport to one row, so the first paint is one row plus the
    // overscan, and the `requestAnimationFrame` below repaints with the real
    // height as soon as the layout exists. Rendering nothing until a frame had
    // passed would leave an empty diff in a synchronous test.
    paintDiff();
    requestAnimationFrame(() => {
      if (diffView?.scroller === scroller) {
        diffView.dirty = true;
        paintDiff();
      }
    });
    return [paragraph(t("compare.diff.title"), "muted"), scroller];
  }

  /** ADR-061: applies the sidecar to the open document as tracked changes.
   *
   *  THE AUTHOR IS THE COMPARED DOCUMENT — ADR-061's "the name comes from the
   *  compared document" — so the author colour on the canvas distinguishes what a
   *  comparison computed from what a person suggested, and the review card names
   *  the file the change came from. Bounded well inside the engine's 255-BYTE
   *  author limit, and bounded by CODE POINTS: `slice(80)` on an 80-glyph CJK
   *  name is 240 bytes, which is why the figure is not 255. A blank name falls
   *  back to a localised noun rather than to `review.author-required` — that
   *  refusal would be this chrome's bug reported as the reader's.
   *
   *  NOT routed through the host's `runEdit`, and the two reasons are the whole
   *  reason this function exists rather than one more `runEdit` call:
   *
   *    1. `runEdit` swallows a throw into a status sentence and returns `false`,
   *       so the refusal CODE never reaches a caller — and four coded refusals
   *       are exactly what this surface has to tell apart.
   *    2. It hands the `EditResult` straight to `applyEditResult`, which calls
   *       `res.free()`. `pasteLoss` read after that throws "null pointer passed
   *       to rust", which is how the structured-paste loss report was first
   *       written wrong. It is read HERE, before `io.landed` frees anything.
   *
   *  Complexity: O(changes + the paragraphs they touch) in the engine, then one
   *  re-render. Not O(document) per change. */
  async function applyAsRevisions(sidecar, otherName) {
    const live = io.doc();
    if (!live) return { ok: false, code: "", message: t("compare.needsDocument") };
    const blocked = io.blockedReason?.() ?? "";
    if (blocked) return { ok: false, code: "", message: blocked };
    const author = String(otherName ?? "").trim().slice(0, 80) || t("compare.author");
    let res;
    try {
      res = live.applyDiffAsRevisions(sidecar, author, new Date().toISOString());
    } catch (error) {
      // ROUTED THROUGH THE ONE SEAM, with this surface's four rows as its
      // vocabulary: the code becomes a localised sentence when `REFUSAL_KEY`
      // knows it, and the engine's own specific sentence when it does not.
      return {
        ok: false,
        code: String(error?.code ?? ""),
        message: editRefusalMessage(error, {
          editingUnavailableReason: io.readOnlyReason?.() ?? "",
          routeRefusal: routeCompareRefusal,
        }),
      };
    }
    const loss = Array.from(res.pasteLoss ?? []);
    await io.landed(res);
    return { ok: true, loss };
  }

  /** Compares the live document against `bytes`, named `otherName`. */
  async function compareWith(bytes, otherName) {
    // NO capability gate here, deliberately. Comparing reads nothing the caller
    // did not already have: the version-history route arrives with bytes the
    // checkpoint store already held, so gating this on `open` would take the
    // whole capability away from a host that withheld file access while granting
    // review — and the grant it would be enforcing is about READING A FILE FROM
    // THE VISITOR'S DISK, which is the chooser's business and is gated there.
    if (running) return;
    setOpen(true);
    running = true;
    cancelled = false;
    try {
      // PAINT AND ARM CANCEL BEFORE THE BLOCK, which this did not used to do.
      // `io.currentBytes()` is `comparableBytes` — one synchronous, O(document)
      // export into wasm, measured at 44 ms per 20,000 paragraphs and linear in
      // document size — and it ran first, with the chooser (or the previous
      // result) still on screen and no Cancel anywhere. SKILL §8 is explicit:
      // anything O(document) must show real progress and be cancellable. One
      // yielded frame is what makes both true, and it costs a frame on a gesture
      // that is already about to take longer than one.
      //
      // The export itself still cannot be interrupted — it is one call into the
      // engine — so a Cancel pressed during it takes effect at the first slice
      // boundary afterwards, which is where `runComparison` already asks. That is
      // the honest limit of what the chrome can do; moving the export off the
      // thread needs a worker, and the module header says why there is not one.
      renderProgress({ phase: PARSING, done: 0, total: 0 });
      mark("opendoc.compare.progress");
      await io.yieldToHost();
      if (cancelled) {
        render([paragraph(t("compare.cancelled"), "muted")]);
        return;
      }
      const mine = io.currentBytes();
      if (!mine) {
        render([paragraph(t("compare.cannotExport"), "muted")]);
        return;
      }
      const outcome = await runComparison({
        // The ORDER is review's: the other document is the left (older) side and
        // this one is the right, so an insertion is what this document has and
        // the other does not. Getting it the other way round would report every
        // addition as a deletion, which is the kind of mistake a reader cannot
        // detect from the output.
        begin: () => io.engine.begin(bytes, mine),
        slice: () => io.engine.slice(),
        yieldToHost: io.yieldToHost,
        onProgress: renderProgress,
        cancelled: () => cancelled,
        // The handle is kept so the unified diff can pull its context blocks
        // from the two parsed sides. `releaseDiff` frees it.
        retain: true,
      });
      if (!outcome.ok) {
        if (outcome.reason === CANCELLED) {
          render([paragraph(t("compare.cancelled"), "muted")]);
          return;
        }
        // TWO kinds of reason, and only one of them is a sentence.
        //
        // `NO_ANSWER` holds this module's own internal outcomes — it ran out of
        // slices, or the engine said complete and handed back nothing. Those are
        // vocabulary, not English a reader should see: "The comparison failed:
        // budget" is exactly the raw engine token `edit_errors.mjs` exists to keep
        // off the status bar.
        //
        // Anything else is a MESSAGE THE ENGINE WROTE about this document —
        // admission limits, a corrupt package — and it passes through, because
        // "the comparison failed" without naming the cause sends a reader looking
        // for a problem with the wrong file. That is the same split
        // `editRefusalMessage` makes for `refused:`-coded refusals.
        const message = NO_ANSWER.has(outcome.reason)
          ? t("compare.noAnswer")
          : t("compare.failed", { reason: outcome.reason });
        render([paragraph(message, "muted")]);
        io.setStatus(message, "error");
        return;
      }
      const summary = summariseDiff(outcome.diff);
      // ADR-061, and the whole point of this lane: the differences go INTO the
      // document as tracked changes, and the panel becomes their index.
      //
      // Nothing is applied for a comparison that found nothing — an empty
      // `UpdateReviewState` would bump the revision and enable Save for a
      // comparison that changed no text, which the engine refuses to do for the
      // same reason.
      let applied = null;
      if (summary.total > 0 && io.landed) {
        const result = await applyAsRevisions(outcome.sidecar, otherName);
        if (!result?.ok) {
          // The sentence `applyAsRevisions` already routed — NOT a second lookup
          // of the same code. Two places deciding one mapping is the drift this
          // repository keeps fixing, and here it would be two places deciding
          // whether a refusal is read in the reader's language.
          const message = result?.message || t("compare.noAnswer");
          const note = paragraph(message);
          note.dataset.compareRefused = String(result?.code ?? "");
          render([paragraph(t("compare.against", { name: otherName })), note]);
          io.setStatus(message, "error");
          // Nothing renders the diff on a refusal, so nothing takes the handle.
          outcome.job?.free?.();
          return;
        }
        applied = result;
      }
      renderResult(summary, otherName, applied, { job: outcome.job ?? null });
    } finally {
      running = false;
    }
  }

  /**
   * VERSION HISTORY'S ROUTE: one stored version against its PREDECESSOR,
   * read-only (**ADR-062**).
   *
   * ## Why this is not `compareWith`, and why that is a split rather than a
   * second mechanism
   *
   * `compareWith` is Review ▸ Compare: a deliberate, warned, undoable act on the
   * document in front of the reader, which ADR-061 decided and which keeps every
   * word of it. This is version history, and it is a different question: *what
   * changed in this version?* Three facts make routing it through `compareWith`
   * wrong rather than merely untidy, and all three were measured:
   *
   *   1. **It compared a version against ITSELF.** Clicking a row opens a
   *      preview, the preview assigns `doc = previewDoc` in `main.js`, and
   *      `compareWith`'s own right-hand side is `comparableBytes(doc, …)`. A
   *      freshly parsed preview has `revision == 0`, so `ExactIfUnchanged`
   *      returns the retained original bytes VERBATIM — byte-identical to the
   *      checkpoint handed in as the other side. The comparison could not find
   *      anything, and `compare.spec.mjs` asserted `/Compared with|No
   *      differences/` over it, which passes either way.
   *   2. **No competitor routes history through a mutation.** Word and Google
   *      produce a third document; ONLYOFFICE mutates only from Review ▸ Compare
   *      and its history is read-only by construction. Writing tracked changes
   *      into the reader's current document because they asked a question about
   *      the past is a behaviour nobody has, and it is not a weak version of
   *      anyone's feature.
   *   3. **`docs/139` §9.4 already specified this** — "Version diff is derived
   *      data. It does not add tracked changes, comments, nodes, or marks to
   *      either source document" — and ADR-061 reversed it for both routes when
   *      it only needed to reverse it for one.
   *
   * ## What makes it read-only, mechanically
   *
   * Both sides are checkpoint byte arrays. The live document is never exported,
   * `applyDiffAsRevisions` is never called, `io.landed` is never called, so
   * `setShowingChanges` never fires and no `Operation::UpdateReviewState` is
   * built. There is nothing to undo because nothing happened. The panel renders
   * `[data-compare-read-only]` and never `[data-compare-marked]` — which is what
   * `compare.spec.mjs` asserts, against `compare-on-canvas.spec.mjs`'s assertion
   * that Review ▸ Compare still renders `[data-compare-marked]` visible.
   *
   * It is also NOT gated on `blockedReason`. A comparison that writes revisions
   * is a mutation and goes through the Viewing-mode gate; this one writes
   * nothing, and a preview is read-only by definition — so gating it there would
   * refuse the feature in precisely the state a reader reaches it from.
   *
   * Complexity: O(both checkpoints) to parse, sliced with progress and a Cancel,
   * then O(window) to render. No live-document work at all.
   *
   * @param {Uint8Array} olderBytes the predecessor's checkpoint.
   * @param {Uint8Array} newerBytes the selected version's checkpoint.
   * @param {string} olderLabel when the predecessor was.
   * @param {string} newerLabel when the selected version was.
   */
  async function compareVersions(olderBytes, newerBytes, olderLabel, newerLabel) {
    if (running) return;
    releaseDiff();
    setOpen(true);
    running = true;
    cancelled = false;
    try {
      renderProgress({ phase: PARSING, done: 0, total: 0 });
      await io.yieldToHost();
      if (cancelled) {
        render([paragraph(t("compare.cancelled"), "muted")]);
        return;
      }
      const outcome = await runComparison({
        // The older state is LEFT and the newer is RIGHT, which is review's
        // orientation and the engine's: an insertion is what the newer version
        // has and the older did not. Reversed, every addition in a version would
        // read as a removal and the count would still be right — the mistake a
        // reader cannot detect, which is why the spec names the text it expects.
        begin: () => io.engine.begin(olderBytes, newerBytes),
        slice: () => io.engine.slice(),
        yieldToHost: io.yieldToHost,
        onProgress: renderProgress,
        cancelled: () => cancelled,
        retain: true,
      });
      if (!outcome.ok) {
        if (outcome.reason === CANCELLED) {
          render([paragraph(t("compare.cancelled"), "muted")]);
          return;
        }
        const message = NO_ANSWER.has(outcome.reason)
          ? t("compare.noAnswer")
          : t("compare.failed", { reason: outcome.reason });
        render([paragraph(message, "muted")]);
        io.setStatus(message, "error");
        return;
      }
      renderResult(summariseDiff(outcome.diff), newerLabel, null, {
        job: outcome.job ?? null,
        readOnly: true,
        heading: t("compare.betweenVersions", { older: olderLabel, newer: newerLabel }),
      });
    } finally {
      running = false;
    }
  }

  fileInput.addEventListener("change", async () => {
    const file = fileInput.files?.[0];
    fileInput.value = "";
    if (!file) return;
    // THE FILE ROUTE is what `open` gates, and it is gated twice on purpose: the
    // chooser is disabled with its reason, and a file that arrives anyway — a
    // host driving the input directly, a drop handler added later — is refused
    // here rather than silently read. The first is the affordance and the second
    // is the rule.
    if (!io.allowed()) {
      io.setStatus(io.refusedReason, "error");
      return;
    }
    await compareWith(new Uint8Array(await file.arrayBuffer()), file.name);
  });

  // Both faces TOGGLE, and both do the same thing — which is what makes them one
  // command rather than two controls that happen to open the same panel. A
  // surface opened by a control stays closable by it (the rule `modal.mjs`
  // already follows for chords).
  for (const button of entryPoints) {
    button.addEventListener("click", () => {
      const open = panel.hidden;
      setOpen(open);
      // CLOSING RELEASES THE TWO PARSED SIDES. They are the largest thing this
      // surface holds and they exist only to answer "show me more context", so
      // the panel going away is the release point — not a timer, and not a
      // weak reference the engine has no notion of.
      if (!open) releaseDiff();
      if (open && !running) {
        releaseDiff();
        renderChooser();
      }
    });
  }
  document.getElementById("compareClose")?.addEventListener("click", () => {
    setOpen(false);
    releaseDiff();
  });

  return {
    open: () => {
      setOpen(true);
      if (!running) {
        releaseDiff();
        renderChooser();
      }
    },
    compareWith,
    compareVersions,
  };
}
