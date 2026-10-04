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
    job.free?.();
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
    rows: [...changes].sort(
      (a, b) => (rank.get(a.family) ?? unknown) - (rank.get(b.family) ?? unknown),
    ),
  };
}

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
 *  THE FIRST AND ONLY SUCH TABLE IN THIS HOST, checked before it was written.
 *  `casual-doc-edit/src/refusal.rs` ships a refusal as `refused: <sentence>\u{1f}<code>`
 *  and `to_js` splits the code onto the thrown `Error`; its own documentation says
 *  a host routes `t(code)` "so a non-English reader gets the specific reason in
 *  their own language". The engine has ~130 such codes and `grep` finds **no host
 *  table routing any of them** — `edit_errors.mjs` passes the engine's English
 *  sentence through verbatim, deliberately, because a half-populated general list
 *  would silently fall back to the generic sentence for everything not in it.
 *
 *  So this table is scoped to the four codes THIS surface can produce, exactly as
 *  `FINDING_KEY` is scoped to the finding codes this surface can produce, and
 *  anything else still falls through to the engine's own sentence rather than to a
 *  generic one. A general code->sentence catalogue over all ~130 is a separate
 *  piece of work; when it lands, these four keys are what it reads.
 *
 *  `review.author-required` is defence in depth: `compareWith` never passes an
 *  empty author. It is routed anyway, because a refusal whose only reader-facing
 *  form is English prose is a refusal that will be read in English.
 *
 *  `compare_documents.test.mjs` derives the expected set FROM THE RUST and fails
 *  the build if the engine grows a fifth. O(1). */
export const REFUSAL_KEY = Object.freeze({
  "compare.document-has-revisions": "compare.refused.documentHasRevisions",
  "compare.schema-unsupported": "compare.refused.schemaUnsupported",
  "compare.sidecar-unreadable": "compare.refused.sidecarUnreadable",
  "review.author-required": "compare.refused.authorRequired",
});

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
    // `compareWith: null` is what the version panel's Show changes row tests, so
    // it reports the capability as unavailable rather than throwing on click.
    return { open: () => {}, compareWith: null };
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

  function renderResult(summary, otherName, applied = null) {
    const children = [
      paragraph(t("compare.against", { name: otherName })),
    ];
    // "No differences" is a CLAIM, and it is only honest when the comparison
    // actually compared everything. A document whose drawings this build has no
    // typed comparison for can produce zero changes and a `not_compared`
    // finding — saying "No differences" there would tell a reader the two files
    // agree about something the engine never looked at. So the finding wins, and
    // the sentence becomes "no differences in what could be compared".
    if (summary.total === 0) {
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
      marked.dataset.compareMarked = String(summary.total);
      marked.textContent = t("compare.marked", { count: summary.total });
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
    // WHAT COULD NOT BE COMPARED, before the changes themselves: a reader
    // deciding whether to trust this list needs its limits before they start
    // reading, not after.
    children.push(...findingNotes(summary));
    const rows = document.createElement("ol");
    rows.className = "compare-changes";
    for (const change of summary.rows) {
      const item = document.createElement("li");
      item.dataset.compareKind = change.kind;
      item.dataset.compareChangeFamily = change.family;
      const kind = document.createElement("span");
      kind.className = "compare-kind";
      kind.textContent = KIND_KEY[change.kind] ? t(KIND_KEY[change.kind]) : change.kind;
      item.append(kind);
      // WHAT THE ROW IS ABOUT, and never nothing. Text first, because an excerpt
      // of the words is what a reader recognises; then the typed field paths,
      // which are what a formatting or property change actually is; then the
      // bracketed object name, so a row about an untexted block still names a
      // thing. A row that carried only its kind label is the defect this order
      // closes, and `compare_documents.test.mjs` fails if one can still happen.
      const text = changeText(change);
      const fields = changeFields(change);
      if (text) {
        const quote = document.createElement("q");
        quote.textContent = text;
        item.append(quote);
      } else if (fields.length > 0) {
        const named = document.createElement("span");
        named.className = "compare-object";
        named.dataset.compareFields = fields.join(",");
        named.textContent = fields.join(", ");
        item.append(named);
      } else {
        const named = document.createElement("span");
        named.className = "compare-object";
        named.textContent = changeObjectName(change);
        item.append(named);
      }
      const where = storyLabel(change.left?.story ?? change.right?.story);
      if (where) {
        const story = document.createElement("span");
        story.className = "compare-where";
        story.textContent = where;
        item.append(story);
      }
      rows.append(item);
    }
    children.push(rows);
    render(children);
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
      return {
        ok: false,
        code: String(error?.code ?? ""),
        message: editRefusalMessage(error, { editingUnavailableReason: io.readOnlyReason?.() ?? "" }),
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
          // ROUTED BY CODE, falling back to the engine's own sentence. Both
          // halves matter: the code is what makes the reason translatable, and
          // the engine's sentence is what stops an unrecognised code becoming
          // "something went wrong".
          const message = REFUSAL_KEY[result?.code]
            ? t(REFUSAL_KEY[result.code])
            : result?.message || t("compare.noAnswer");
          const note = paragraph(message);
          note.dataset.compareRefused = String(result?.code ?? "");
          render([paragraph(t("compare.against", { name: otherName })), note]);
          io.setStatus(message, "error");
          return;
        }
        applied = result;
      }
      renderResult(summary, otherName, applied);
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
      if (open && !running) renderChooser();
    });
  }
  document.getElementById("compareClose")?.addEventListener("click", () => setOpen(false));

  return {
    open: () => {
      setOpen(true);
      if (!running) renderChooser();
    },
    compareWith,
  };
}
