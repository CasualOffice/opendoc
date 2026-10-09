// Compare this document with another one: the chrome over `casual-doc-diff`.
//
// ## What a reader gets (ADR-065)
//
// Review ▸ Compare (Word's route: pick a file off the disk) shows the two
// documents' differences ON THE PAGE, read-only: the canvas swaps to a REDLINE —
// this document with the other one's text struck back in where it was, what is
// new underlined, moves double-lined at both ends — and the panel becomes a
// plain list of those changes, each one a button that takes the reader to it.
// Nothing in the reader's document changes until they press "Keep as tracked
// changes", which applies the same comparison to it through ADR-061's
// `applyDiffAsRevisions` (one undo step, refused on a document that already
// carries tracked changes, and the refusal says why).
//
// ## Why this replaced what was here
//
// The panel used to be a REPORT: a count, six category totals, an "unmarked"
// list, findings, and a block-level unified diff of one-line rows in a 252px
// column — while the comparison itself had already been written into the
// reader's document as tracked changes, which no reference does on Compare's
// first click and which a document with suggestions in it refused outright.
// Every entry was jargon ("<Block>", "Property changed", `spacing.beforeTwips`)
// and a removed paragraph could not be shown at all, because a tracked change
// cannot add a paragraph. The redline can (`DiffChange::place`), so the page is
// now the report and the panel is its index — Word's "compare into a new
// document" and Google's version history, which are the same picture.
//
// The loss reports are still here, under "What isn't highlighted": a difference
// the page cannot mark (a page-setup change, a header, a picture in a removed
// paragraph) is SAID, never silently dropped (`AGENTS.md`).
//
// ## Which side is which
//
// By default the other document is the older side and this one the newer, so
// "added" means "this document has it": review's orientation, and the only one
// "Keep as tracked changes" can apply to this document. "Swap order" reverses
// it — "what did they change in the copy they sent back?" — and hides Keep,
// because the result then describes the other file, not this one.
//
// ## Where it runs
//
// On the main thread, in slices (`runComparison`), with progress and a Cancel —
// `diff.rs` says at length why there is no Worker yet. The budget is BLOCKS, and
// the engine owns the starting number (`defaultDiffSlice`).
//
// Pure except where it cannot be: the driver takes its engine, scheduler and
// clock as arguments, so "parse two documents in slices, report progress, cancel
// at a boundary" is drivable from node. The DOM half is `bindComparePanel`.
import { createChangeNavigator, entryLabel, entryText, redlineEntries } from "./diff_canvas.mjs";
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

/**
 * Compares `older` with `newer` and paints the result into a throwaway copy of
 * `newer`.
 *
 * The copy is opened from `newer`'s own bytes, so it is the comparison's newer
 * side by construction — the engine checks that rather than trusting it. The
 * reader's document is never read or written: both sides are byte arrays the
 * caller already held.
 *
 * Ownership: on success the caller owns `view` and must `free()` it; on every
 * other path nothing is left allocated.
 *
 * Complexity: O(both documents) to parse and compare, in slices that yield to
 * the host between them (`runComparison`), then one O(newer) open and one
 * O(changes) paint. Never O(document) per interaction.
 *
 * @param {object} io
 * @param {{begin: Function, slice: Function, open: Function}} io.engine
 * @param {Uint8Array} io.older
 * @param {Uint8Array} io.newer
 * @param {string} io.author who the changes are attributed to (their colour).
 * @param {string} io.date ISO stamp; with `author`, what marks THIS comparison.
 * @param {() => Promise<void>} io.yieldToHost
 * @param {(progress: object) => void} [io.onProgress]
 * @param {() => boolean} [io.cancelled]
 * @returns {Promise<{ok: true, view: object, diff: object, sidecar: string, summary: object}
 *   | {ok: false, reason: string}>}
 */
export async function buildRedline(io) {
  const outcome = await runComparison({
    // Older LEFT, newer RIGHT: review's orientation, so an insertion is what the
    // newer side has. Reversed, every addition would read as a removal.
    begin: () => io.engine.begin(io.older, io.newer),
    slice: () => io.engine.slice(),
    yieldToHost: io.yieldToHost,
    onProgress: io.onProgress,
    cancelled: io.cancelled,
    // The painted view reads the older side's removed paragraphs from the job,
    // so the handle outlives the comparison until the paint is done.
    retain: true,
  });
  if (!outcome.ok) return outcome;
  let view = null;
  try {
    if (io.cancelled?.()) return { ok: false, reason: CANCELLED };
    view = io.engine.open(io.newer);
    const summary = JSON.parse(view.showComparison(outcome.job, io.author, io.date));
    const done = { ok: true, view, diff: outcome.diff, sidecar: outcome.sidecar, summary };
    view = null;
    return done;
  } catch (error) {
    return { ok: false, reason: String(error?.message ?? error) };
  } finally {
    outcome.job?.free?.();
    view?.free?.();
  }
}

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
 *  **Twelve more keys are the engine's `DiffFamily` names** (`family_loss_key`),
 *  reported when a family has no inline revision form at all — `RevisionKind` is
 *  Insertion/Deletion/MoveFrom/MoveTo and nothing else, so a formatting, style,
 *  section, definition, resource, comment or metadata difference cannot be
 *  EXPRESSED however faithfully it was detected. Those are said with the
 *  family's own counted sentence (`FAMILY_KEY`: "Formatting changes: 3"), which
 *  is what a reader can act on — the bracketed object nouns this used to print
 *  ("<Formatting>") were the jargon ADR-065 removed.
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
  removedObject: "compare.unmarked.removedObject",
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
 * @param {readonly string[] | undefined | null} keys `EditResult.pasteLoss`, or
 *   a redline summary's `unmarked`.
 * @param {object} [diff] the sidecar, whose `familyCounts` give a family key its
 *   count.
 * @returns {{key: string, label: string}[]}
 */
export function unmarkedReasons(keys, diff = null) {
  if (!Array.isArray(keys)) return [];
  const counts = new Map(Array.isArray(diff?.familyCounts) ? diff.familyCounts : []);
  const rows = [];
  const seen = new Set();
  for (const raw of keys) {
    const key = String(raw ?? "");
    if (!key) continue;
    const family = FAMILY_KEY[key];
    const catalogue = family ?? UNMARKED_KEY[key];
    const dedupe = catalogue ?? `raw:${key}`;
    if (seen.has(dedupe)) continue;
    seen.add(dedupe);
    const label = family
      ? t(family, { count: n(counts.get(key) ?? 0) })
      : catalogue
        ? t(catalogue)
        : key;
    rows.push({ key, label });
  }
  return rows;
}

/**
 * Binds the Compare panel and its two entry points.
 *
 * @param {object} io
 * @param {() => object|null} io.doc the live document.
 * @param {() => Uint8Array|null} io.currentBytes this document, exported.
 * @param {(io: object) => Promise<object>} io.redline `buildRedline`, bound to
 *   the engine: `{older, newer, author, date, onProgress, cancelled}`.
 * @param {(view: object|null) => Promise<void>} io.showView puts a redline on
 *   the canvas read-only, or (with `null`) gives the canvas back to the live
 *   document.
 * @param {() => Promise<void>} [io.beforeView] lets another borrower of the
 *   canvas (a version preview) let go first, and BEFORE the export — or the
 *   export would be of the preview.
 * @param {() => Promise<void>} io.yieldToHost
 * @param {(text: string, kind?: string) => void} io.setStatus
 * @param {() => boolean} io.allowed whether the host granted what this needs.
 * @param {string} io.refusedReason what to say when it did not.
 * @param {(res: object) => Promise<void>} [io.landed] ADR-061's seam for
 *   "Keep as tracked changes": repaint, markup on, gutter. Absent (a composition
 *   without review chrome), Keep is not offered.
 * @param {() => string} [io.blockedReason] the sentence for a mutation the host
 *   blocks before the engine sees it (Viewing mode), or `""`. Keep WRITES
 *   revisions, so it goes through that gate like every other mutation; viewing
 *   the comparison does not.
 * @param {() => string} [io.readOnlyReason] the engine's own
 *   `editingUnavailableReason`, for a document no edit can ever apply to.
 * @param {(anchor: {node: string, start: number, end: number}) => unknown} io.navigate
 *   takes the reader to one change on the redline.
 */
export function bindComparePanel(io) {
  const panel = document.getElementById("comparePanel");
  const body = document.getElementById("compareBody");
  const fileInput = document.getElementById("compareFile");
  // BOTH faces of `review.compare`, owned here: one command, one owner of both
  // clicks and both pressed states. Their DISABLED state is the surface
  // table's, which is why the row declares `ownsClick` and lists both.
  const entryPoints = [
    document.getElementById("reviewCompareBtn"),
    document.getElementById("railCompare"),
  ].filter(Boolean);
  if (!panel || !body || !fileInput || entryPoints.length === 0) {
    // No surface in this composition — an embed built without the Compare panel.
    return { open: () => {}, compareWith: null, closeView: async () => {} };
  }

  let cancelled = false;
  let running = false;
  /** The redline on the canvas, owned here until it is given back. */
  let view = null;
  /** The comparison on screen: both byte arrays, so Swap needs no re-export. */
  let current = null;

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

  function actionButton(text, action, run, primary = false) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = primary ? "dialog-button dialog-button-primary" : "dialog-button";
    button.dataset.compareAction = action;
    button.textContent = text;
    button.addEventListener("click", run);
    return button;
  }

  /** The chooser: what to compare against. Word's Compare dialog in one panel. */
  function renderChooser() {
    const choose = actionButton(t("compare.chooseFile"), "choose-file", () => fileInput.click(), true);
    choose.disabled = !io.allowed();
    if (!io.allowed()) choose.title = io.refusedReason;
    // Said up front: the differences are SHOWN, and nothing is written into this
    // document unless the reader keeps them.
    render([paragraph(t("compare.intro"), "muted"), choose, paragraph(t("compare.showsOnPage"), "muted")]);
  }

  function renderProgress(progress) {
    const cancel = actionButton(t("compare.cancel"), "cancel", () => {
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

  /** WHAT COULD NOT BE COMPARED, as elements — empty when there is nothing.
   *  `record.rs` aggregates these (one row per construct, with a count), so the
   *  engine did that work precisely so a host could SAY it. */
  function findingNotes(diff) {
    const findings = Array.isArray(diff?.findings) ? diff.findings : [];
    if (findings.length === 0) return [];
    const notes = document.createElement("ul");
    notes.className = "compare-list";
    for (const finding of findings) {
      const item = document.createElement("li");
      item.dataset.compareFinding = finding.code;
      const key = FINDING_KEY[finding.code];
      // An unknown code still SHOWS, with its own name and count.
      item.textContent = key
        ? t(key, { construct: finding.construct, count: finding.count })
        : `${finding.construct}: ${finding.code} (${n(finding.count)})`;
      notes.append(item);
    }
    return [paragraph(t("compare.findingsTitle"), "muted"), notes];
  }

  /** WHAT WAS FOUND AND COULD NOT BE MARKED, as elements — empty when nothing.
   *  Collecting the report and not rendering it is the silent loss `AGENTS.md`
   *  puts first, so an unknown key still shows by its own name. */
  function unmarkedNotes(loss, diff) {
    const rows = unmarkedReasons(loss, diff);
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

  /** Gives the canvas back to the live document and frees the redline.
   *  Idempotent: closing the panel, a new comparison, Keep, a version preview
   *  and opening another file all reach it. */
  async function closeView() {
    if (!view) return;
    const closing = view;
    view = null;
    await io.showView(null);
    closing.free?.();
  }

  /** Runs `current` in its current orientation and puts the redline on screen. */
  async function show() {
    const { other, mine, otherName, swapped } = current;
    const older = swapped ? mine : other;
    const newer = swapped ? other : mine;
    // The changes are attributed to the OTHER document either way — ADR-061's
    // author, so Keep paints them in the same colour the view did.
    const author = String(otherName ?? "").trim().slice(0, 80) || t("compare.author");
    const date = new Date().toISOString();
    renderProgress({ phase: PARSING, done: 0, total: 0 });
    const result = await io.redline({
      older,
      newer,
      author,
      date,
      onProgress: renderProgress,
      cancelled: () => cancelled,
    });
    if (!result.ok) {
      if (result.reason === CANCELLED) {
        render([paragraph(t("compare.cancelled"), "muted")]);
        return;
      }
      // `NO_ANSWER` is this module's own vocabulary, not English; anything else
      // is a sentence the engine wrote about one of the documents.
      const message = NO_ANSWER.has(result.reason)
        ? t("compare.noAnswer")
        : t("compare.failed", { reason: result.reason });
      render([paragraph(message, "muted")]);
      io.setStatus(message, "error");
      return;
    }
    const entries = redlineEntries(JSON.parse(result.view.listRevisions()), result.diff, result.summary, {
      author,
      date,
    });
    await closeView();
    view = result.view;
    await io.showView(view);
    current.result = result;
    renderView(entries, author);
  }

  /** The panel while a redline is on the canvas: what is compared with what,
   *  "3 of 12" with previous/next, the list, and the three ways on. */
  function renderView(entries, author) {
    const { otherName, swapped, result } = current;
    const mineName = t("compare.thisDocument");
    const heading = paragraph(
      t("compare.changesFrom", {
        older: swapped ? mineName : otherName,
        newer: swapped ? otherName : mineName,
      }),
      "compare-heading",
    );
    heading.dataset.compareView = "";

    const list = document.createElement("ol");
    list.className = "compare-changes";
    const rows = new Map();
    const navRoot = document.createElement("div");
    const navigator = createChangeNavigator(navRoot, {
      navigate: (entry) => {
        for (const [candidate, row] of rows) row.classList.toggle("is-current", candidate === entry);
        io.navigate(entry.anchor);
      },
    });
    for (const entry of entries) {
      const item = document.createElement("li");
      const button = document.createElement("button");
      button.type = "button";
      button.className = "compare-change";
      button.dataset.diffKind = entry.replaced ? "replaced" : entry.kind;
      const kind = document.createElement("span");
      kind.className = "compare-change-kind";
      kind.textContent = entryLabel(entry);
      const text = document.createElement("span");
      text.className = "compare-change-text";
      text.textContent = entryText(entry);
      button.append(kind, text);
      button.addEventListener("click", () => {
        navigator.select(entry);
        for (const [candidate, row] of rows) row.classList.toggle("is-current", candidate === entry);
        io.navigate(entry.anchor);
      });
      item.append(button);
      list.append(item);
      rows.set(entry, button);
    }
    navigator.show(entries, { author });

    const actions = document.createElement("div");
    actions.className = "compare-actions";
    // Keep applies to THIS document, so it exists only in the orientation in
    // which the redline describes this document — and only where the host can
    // repaint review markup at all.
    if (!swapped && io.landed && entries.length > 0) {
      actions.append(actionButton(t("compare.keep"), "keep", () => void keep(), true));
    }
    actions.append(
      actionButton(t("compare.swap"), "swap", () => void swap()),
      actionButton(t("compare.closeView"), "close", () => void closeAll()),
    );

    const details = [...unmarkedNotes(result.summary?.unmarked, result.diff), ...findingNotes(result.diff)];
    const children = [heading, paragraph(t("compare.viewReadOnly"), "muted"), navRoot];
    children.push(entries.length > 0 ? list : paragraph(t("compare.identical")));
    children.push(actions);
    if (details.length > 0) {
      const more = document.createElement("details");
      more.className = "compare-details";
      const summary = document.createElement("summary");
      summary.textContent = t("diffCanvas.details");
      more.append(summary, ...details);
      children.push(more);
    }
    render(children);
  }

  async function swap() {
    if (running || !current) return;
    running = true;
    cancelled = false;
    try {
      current.swapped = !current.swapped;
      await show();
    } finally {
      running = false;
    }
  }

  /** "Keep as tracked changes": the redline's comparison, applied to THIS
   *  document (ADR-061). The view is closed first, so the edit lands on the
   *  live document and not on the throwaway one. */
  async function keep() {
    if (running || !current?.result) return;
    const { result, otherName } = current;
    await closeView();
    const outcome = await applyAsRevisions(result.sidecar, otherName);
    current = null;
    if (!outcome.ok) {
      const note = paragraph(outcome.message || t("compare.noAnswer"));
      note.dataset.compareRefused = String(outcome.code ?? "");
      render([paragraph(t("compare.against", { name: otherName })), note]);
      io.setStatus(outcome.message || t("compare.noAnswer"), "error");
      return;
    }
    const marked = paragraph(t("compare.marked"));
    marked.dataset.compareMarked = "";
    render([
      paragraph(t("compare.against", { name: otherName })),
      marked,
      paragraph(t("compare.reviewNav"), "muted"),
      ...unmarkedNotes(outcome.loss, result.diff),
    ]);
  }

  async function closeAll() {
    current = null;
    cancelled = true;
    setOpen(false);
    await closeView();
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

  /** Compares the live document against `bytes`, named `otherName`, and puts
   *  the result on the canvas. */
  async function compareWith(bytes, otherName) {
    // NO capability gate here: comparing reads nothing the caller did not
    // already have. The FILE route is what `open` gates, at the chooser.
    if (running) return;
    setOpen(true);
    running = true;
    cancelled = false;
    try {
      // PAINT AND ARM CANCEL BEFORE THE BLOCK. `io.currentBytes()` is one
      // synchronous O(document) export (`comparableBytes`); one yielded frame
      // is what makes progress and Cancel real before it (SKILL §8).
      renderProgress({ phase: PARSING, done: 0, total: 0 });
      mark("opendoc.compare.progress");
      await io.yieldToHost();
      if (cancelled) {
        render([paragraph(t("compare.cancelled"), "muted")]);
        return;
      }
      // The canvas goes home FIRST: an export taken while a preview or an
      // earlier redline is on screen would be of that, not of this document.
      await closeView();
      await io.beforeView?.();
      const mine = io.currentBytes();
      if (!mine) {
        render([paragraph(t("compare.cannotExport"), "muted")]);
        return;
      }
      current = { other: bytes, otherName, mine, swapped: false, result: null };
      await show();
    } finally {
      running = false;
    }
  }

  fileInput.addEventListener("change", async () => {
    const file = fileInput.files?.[0];
    fileInput.value = "";
    if (!file) return;
    // Gated twice on purpose: the chooser is disabled with its reason, and a
    // file that arrives anyway is refused here rather than silently read.
    if (!io.allowed()) {
      io.setStatus(io.refusedReason, "error");
      return;
    }
    await compareWith(new Uint8Array(await file.arrayBuffer()), file.name);
  });

  // Both faces TOGGLE: a surface opened by a control stays closable by it.
  for (const button of entryPoints) {
    button.addEventListener("click", () => {
      if (!panel.hidden) return void closeAll();
      setOpen(true);
      if (!running) renderChooser();
    });
  }
  document.getElementById("compareClose")?.addEventListener("click", () => void closeAll());

  return {
    open: () => {
      setOpen(true);
      if (!running && !view) renderChooser();
    },
    compareWith,
    /** Gives the canvas back (a version preview or an open is taking it). */
    closeView: async () => {
      if (!view) return;
      current = null;
      await closeView();
      if (!panel.hidden && !running) renderChooser();
    },
  };
}
