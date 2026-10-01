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
// ## WHERE WE DELIBERATELY DIFFER FROM WORD, and say so
//
// Word's Compare produces a THIRD DOCUMENT: a merged copy whose differences are
// real tracked changes you can accept and reject. We do not, and claiming
// otherwise would be the overstatement this repository has twice published by
// accident. `casual-doc-diff` returns a typed SIDECAR — a list of changes with
// anchors — not a merged document, and turning one into the other is a document
// construction with its own correctness questions (whose author owns a change?
// what happens to a move?). So this surface is a CHANGE LIST in a side panel,
// which is Google Docs' answer, and the panel says which document each side is.
//
// The honest consequence, stated in the panel rather than hidden: the changes
// can be read and counted, and they cannot be accepted or rejected, because
// there is nothing to accept them INTO.
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
 *  Complexity: O(document), once, on an explicit gesture.
 */
export function comparableBytes(doc, sourceFormat) {
  if (!doc) return null;
  for (const mode of ["exact_if_unchanged", "preserve_when_safe", "semantic"]) {
    try {
      const artifact = doc.exportAs(sourceFormat, mode);
      const bytes = artifact.bytes;
      artifact.free();
      return bytes;
    } catch {
      // Try the next mode. The failure is reported only if every mode fails,
      // because an `exact_if_unchanged` refusal on an edited document is the
      // normal case and not an error.
    }
  }
  return null;
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
 * @returns {Promise<{ok: true, diff: object} | {ok: false, reason: string}>}
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
        return { ok: true, diff: JSON.parse(json) };
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
      // Said up front, not discovered afterwards. Word's Compare makes a third
      // document of tracked changes; this makes a list. A reader who expects to
      // accept a change deserves to know before they pick a file.
      paragraph(t("compare.notTrackedChanges"), "muted"),
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

  function renderResult(summary, otherName) {
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
    const total = document.createElement("p");
    total.dataset.compareTotal = String(summary.total);
    total.textContent = t("compare.changeCount", { count: summary.total });
    children.push(total);
    // A comparison the engine could not finish must not be read as a complete
    // one. `complete: false` only happens on a cancelled job, which this code
    // never renders — but asserting it here means a future partial result cannot
    // arrive looking whole.
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
      const text = changeText(change);
      if (text) {
        const quote = document.createElement("q");
        quote.textContent = text;
        item.append(quote);
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
      const mine = io.currentBytes();
      if (!mine) {
        render([paragraph(t("compare.cannotExport"), "muted")]);
        return;
      }
      renderProgress({ phase: PARSING, done: 0, total: 0 });
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
      renderResult(summariseDiff(outcome.diff), otherName);
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
