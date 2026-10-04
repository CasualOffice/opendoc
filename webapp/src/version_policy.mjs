// What the version timeline SAYS — the words half of version history.
//
// `version_history.mjs` is the store and returns status CODES on purpose: a
// storage layer that hard-codes English is a storage layer that cannot be
// localised, and `docs/140` §7.5 records that "the English lives in the
// catalogue, not in the module". This is the module that keeps that promise.
// It is the same split `status_policy.mjs` / `status_channel.mjs` already
// uses — the decision here, the DOM in `version_panel.mjs` — and it is why
// every rule below is answerable in node without a browser or an engine.
//
// Three jobs, and they are all decisions rather than rendering:
//
//  1. **One sentence per status code**, through `t()` with a LITERAL key, so
//     `build-locale.mjs` can see it and a translator is actually shown it. A
//     raw code must never reach a reader: `history.quotaExhausted` tells
//     somebody whose work was not kept absolutely nothing, and a single
//     generic "could not save a version" is the defect `edit_errors.mjs`
//     exists to avoid — a quota-exhausted store and a refused restore have
//     different ways out, so they get different sentences.
//  2. **Grouping by day**, which is Google Docs' timeline shape (`docs/139`
//     §8.1: "grouped by day, then by meaningful edit session"). Deterministic
//     and pure, so the grouping is a unit test rather than a screenshot.
//  3. **A row's words** — its title, its exact timestamp, its origin, and the
//     accessible name that carries all three to a screen reader, because
//     `docs/139` §14 requires an exact machine-readable timestamp beside the
//     locale one and forbids a relative label REPLACING it.
//
// No DOM, no engine, no store. `t()` is DOM-free (`i18n.mjs`).

import { d, n, t } from "./i18n.mjs";
import {
  HISTORY_STATUS,
  VERSION_KIND,
  historyStatusCodes,
  historyStatusKind,
} from "./version_history.mjs";

/**
 * Status code → the sentence a reader gets, as literal `t()` calls.
 *
 * Thunks in a frozen map, exactly as `file_pane.mjs`'s `PANE_TEXT` does it, and
 * for the same two reasons. `t(KEYS[code])` would be invisible to
 * `build-locale.mjs`, which extracts keys by reading call sites — an
 * interpolated key is a key no translator is ever shown. And reading at CALL
 * time rather than at module load means a locale change is reflected without
 * anything being rebuilt.
 *
 * Every code in `HISTORY_STATUS` appears here, and `version_policy.test.mjs`
 * fails if one does not. That guard is the point: a status code with no
 * sentence would fall through to the generic refusal, which is how a specific
 * refusal becomes an unhelpful one without anybody noticing.
 */
const STATUS_TEXT = Object.freeze({
  [HISTORY_STATUS.RECORDED]: () => t("history.status.recorded"),
  [HISTORY_STATUS.NOT_DUE]: () => t("history.status.notDue"),
  [HISTORY_STATUS.UNCHANGED]: () => t("history.status.unchanged"),
  [HISTORY_STATUS.PRUNED]: () => t("history.status.pruned"),
  [HISTORY_STATUS.RESTORE_PREPARED]: () => t("history.status.restorePrepared"),
  [HISTORY_STATUS.RESTORE_COMMITTED]: () => t("history.status.restoreCommitted"),
  [HISTORY_STATUS.FULL_PINNED]: () => t("history.status.fullPinned"),
  [HISTORY_STATUS.OVER_BUDGET]: () => t("history.status.overBudget"),
  [HISTORY_STATUS.QUOTA_EXHAUSTED]: () => t("history.status.quotaExhausted"),
  [HISTORY_STATUS.STORE_UNAVAILABLE]: () => t("history.status.storeUnavailable"),
  [HISTORY_STATUS.EVICTED]: () => t("history.status.evicted"),
  [HISTORY_STATUS.STALE_HEAD]: () => t("history.status.staleHead"),
  [HISTORY_STATUS.MISSING_CHECKPOINT]: () => t("history.status.missingCheckpoint"),
  [HISTORY_STATUS.CORRUPT_CHECKPOINT]: () => t("history.status.corruptCheckpoint"),
  [HISTORY_STATUS.NAME_REJECTED]: () => t("history.status.nameRejected"),
  [HISTORY_STATUS.PIN_LIMIT]: () => t("history.status.pinLimit"),
  [HISTORY_STATUS.UNKNOWN_VERSION]: () => t("history.status.unknownVersion"),
  [HISTORY_STATUS.UNKNOWN_OPERATION]: () => t("history.status.unknownOperation"),
});

/** The codes this module can describe, for the completeness guard. O(codes). */
export function describedStatusCodes() {
  return Object.keys(STATUS_TEXT);
}

/** Every `history.status.*` key the catalogue must declare, derived from the
 *  codes rather than typed out a second time — a hand-maintained list is a
 *  second place to update and the first one to rot (SKILL §8). O(codes). */
export function historyStatusKeys() {
  return historyStatusCodes().map((code) => `history.status.${code.slice("history.".length)}`);
}

/**
 * The sentence for a store result.
 *
 * A code with no sentence falls back to the generic refusal rather than
 * printing the code: a dotted identifier in the status bar is a bug report, and
 * this is the one place that can guarantee a reader never sees one. It is the
 * generic REFUSAL and not a generic confirmation on purpose — the failure mode
 * of over-reporting a refusal is a toast nobody needed, and the failure mode of
 * under-reporting one is silence about somebody's lost work.
 *
 * Complexity: O(1).
 *
 * @param {string} status a `HISTORY_STATUS` code.
 * @returns {string} a user-facing sentence, never a code.
 */
export function historyMessage(status) {
  return STATUS_TEXT[status]?.() ?? t("history.status.unknownFailure");
}

/** The `status_channel.mjs` kind a store result must be published with.
 *
 *  Delegates to `historyStatusKind`, and that is the whole point: the decision
 *  about whether a message is a refusal belongs with the code that refuses, and
 *  having it in two places is having it wrong in one of them. This function was
 *  first written as `isHistoryRefusal(status) ? "error" : ""`, which is the same
 *  answer for every KNOWN code and the opposite answer for an unknown one — the
 *  store treats an unknown code as a refusal on purpose, because the failure mode
 *  of guessing "error" is a toast nobody needed and the failure mode of guessing
 *  "" is silence about somebody's lost work. `version_policy.test.mjs` caught it.
 *
 *  O(1). */
export function historyMessageKind(status) {
  return historyStatusKind(status);
}

// ── A version's words ────────────────────────────────────────────────────────

/** Why a version exists, in the reader's language. Literal keys, one per
 *  `VERSION_KIND`, for `STATUS_TEXT`'s reasons. */
const KIND_TEXT = Object.freeze({
  [VERSION_KIND.IMPORT]: () => t("versionHistory.kind.import"),
  [VERSION_KIND.SAVED]: () => t("versionHistory.kind.saved"),
  [VERSION_KIND.NAMED]: () => t("versionHistory.kind.named"),
  [VERSION_KIND.AUTO]: () => t("versionHistory.kind.auto"),
  [VERSION_KIND.MANUAL]: () => t("versionHistory.kind.manual"),
  [VERSION_KIND.PRE_RESTORE]: () => t("versionHistory.kind.preRestore"),
  [VERSION_KIND.RESTORE]: () => t("versionHistory.kind.restore"),
  [VERSION_KIND.RECOVERY]: () => t("versionHistory.kind.recovery"),
});

/** The origin word for a version row. An unrecognised kind reads as an ordinary
 *  automatic version rather than as an empty cell — the store's kind set is
 *  closed, but a row is untrusted input on reopen (`docs/140` §14). O(1). */
export function versionKindLabel(kind) {
  return (KIND_TEXT[kind] ?? KIND_TEXT[VERSION_KIND.AUTO])();
}

/** The kinds this module can describe, for the completeness guard. */
export function describedKinds() {
  return Object.keys(KIND_TEXT);
}

const DAY_MS = 24 * 60 * 60 * 1000;

/** Local midnight for `at`, as a day key. Local rather than UTC because the day
 *  a person groups by is the one their clock showed: a version written at
 *  23:30 belongs under today, not under tomorrow (`docs/140` §15 —
 *  localisation affects labels only, never identity or ordering). O(1). */
function dayKeyOf(at) {
  const date = new Date(at);
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(
    date.getDate(),
  ).padStart(2, "0")}`;
}

/** "Today" / "Yesterday" / a locale date. The relative words are for the two
 *  days everybody recognises and nothing else — "3 days ago" as a GROUP heading
 *  makes a timeline harder to read, not easier, and `docs/139` §14 forbids a
 *  relative label standing in for the exact date anywhere it matters. O(1). */
export function dayLabel(at, now) {
  const today = dayKeyOf(now);
  const key = dayKeyOf(at);
  if (key === today) return t("versionHistory.day.today");
  if (key === dayKeyOf(now - DAY_MS)) return t("versionHistory.day.yesterday");
  return d(at, { dateStyle: "long" });
}

/**
 * The timeline as day groups, newest first.
 *
 * Grouping is PRESENTATION and never deletes a version (`docs/139` §8.2): every
 * row handed in comes back inside exactly one group. Day boundaries only —
 * within-day session grouping is `docs/139` §8.2's expandable detail and is not
 * built, so a day's versions are all visible rather than silently collapsed
 * behind a count nobody can expand.
 *
 * O(v log v) in the number of versions of ONE lineage, bounded by the retention
 * count ceiling (hard limit 200). It reads metadata rows and never the document,
 * which is what makes opening the panel O(versions) and not O(document)
 * (`docs/139` VH-001).
 *
 * @param {Array<object>} versions metadata rows, any order.
 * @param {{now?: number, namedOnly?: boolean}} [options]
 * @returns {Array<{key: string, label: string, rows: Array<object>}>}
 */
export function groupVersions(versions, { now = Date.now(), namedOnly = false } = {}) {
  const rows = (versions ?? [])
    .filter(Boolean)
    // Docs' "Only show named versions" filter (`docs/139` §8.3). It hides rows;
    // it never changes which rows exist, so a filtered timeline that comes back
    // empty says so with its own sentence rather than reading as no history.
    .filter((row) => !namedOnly || Boolean(row.name))
    .sort((a, b) => (b.createdAt ?? 0) - (a.createdAt ?? 0));
  const groups = [];
  let current = null;
  for (const row of rows) {
    const key = dayKeyOf(row.createdAt ?? 0);
    if (!current || current.key !== key) {
      current = { key, label: dayLabel(row.createdAt ?? 0, now), rows: [] };
      groups.push(current);
    }
    current.rows.push(row);
  }
  return groups;
}

/** How a row is referred to in another row's sentence: its name when it has
 *  one, its clock time otherwise. The same choice `versionRowText` makes for
 *  the title, in one place so the two cannot disagree. O(1). */
export function versionRowRef(row) {
  return row?.name ? row.name : d(row?.createdAt ?? 0, { timeStyle: "short" });
}

/**
 * WHAT CHANGED between each version and the one before it, for every row at once.
 *
 * The owner's second report was that the timeline itself had not improved: "still
 * no improvement in versions". A column of rows that all read `Saved · 16 KB` is
 * why — the only thing distinguishing them is a clock, so the panel answers *when*
 * and never *what*, and a reader cannot tell which row holds the work they want.
 *
 * Three facts are derivable from the stored metadata alone, with no engine call,
 * no checkpoint read and no document parse:
 *
 *   * **Same content.** Two rows holding the same document — by the engine's
 *     content digest, or failing that by the checkpoint's byte hash, which is
 *     `isSameDocument`'s own precedence — are twins, and the later one SAYS so
 *     and names the earlier. This is deliberately kept even though
 *     `captureVersion`'s `skipIfUnchanged` now refuses to create such a row: the
 *     reasons in `KEEP_UNCHANGED` can still make one, and a timeline written by
 *     an older build already has them. A duplicate that labels itself is strictly
 *     better than one the reader has to diff by eye.
 *   * **How many edits.** `revision` is the engine's monotonic edit watermark,
 *     recorded on every row. Its delta is the number of landed edits between two
 *     versions — a real measure of how much happened, not a guess. It is reported
 *     ONLY when both watermarks are finite and the delta is positive: a reopen
 *     resets the watermark to 0 (`resetDirtyTracking`), so across an open boundary
 *     the subtraction is meaningless and saying nothing is the honest answer.
 *   * **How much bigger or smaller.** The byte delta, which is what every file
 *     history shows and what a reader uses to spot the version where a chapter
 *     went missing.
 *
 * What this does NOT claim is what the changed TEXT was: that needs a comparison,
 * and a comparison needs both checkpoints parsed. `version.changes` (Show changes)
 * is the route to that, per row, on demand. Naming the limit here so this is not
 * read as a diff.
 *
 * O(rows), once per repaint, over metadata already in memory — no storage read
 * and nothing document-sized. Computed over the UNFILTERED list on purpose: "what
 * changed since the previous version" must mean the same thing whether or not the
 * reader has ticked *Only named versions*.
 *
 * @param {object[]} rows `version_meta` rows, NEWEST FIRST (`listVersions`' order).
 * @returns {Map<string, {edits: number, byteDelta: number, sameAs: string}>}
 */
export function versionRowDeltas(rows) {
  const list = Array.isArray(rows) ? rows : [];
  const deltas = new Map();
  const firstByContent = new Map();
  const firstByBytes = new Map();
  // Oldest first, so "the earlier row holding this document" is the one already
  // seen. Content identity FIRST and bytes as the fallback — the same precedence
  // `isSameDocument` applies, because a label that disagreed with the
  // suppression would be a second answer to one question. Keying this on bytes
  // alone is what the suppression used to do, and it misses the case the whole
  // fix is about: two rows can hold one document and two different byte layouts,
  // which `KEEP_UNCHANGED`'s reasons can still legitimately produce.
  for (let i = list.length - 1; i >= 0; i -= 1) {
    const row = list[i];
    if (!row?.versionId) continue;
    const previous = list[i + 1] ?? null;
    const twin =
      (row.contentId ? firstByContent.get(row.contentId) : undefined) ??
      (row.checkpointId ? firstByBytes.get(row.checkpointId) : undefined);
    if (row.contentId && !firstByContent.has(row.contentId)) {
      firstByContent.set(row.contentId, row);
    }
    if (row.checkpointId && !firstByBytes.has(row.checkpointId)) {
      firstByBytes.set(row.checkpointId, row);
    }
    const here = Number(row.revision);
    const there = Number(previous?.revision);
    const edits =
      previous && Number.isFinite(here) && Number.isFinite(there) && here > there
        ? here - there
        : 0;
    deltas.set(row.versionId, {
      edits,
      byteDelta: previous ? (row.bytes ?? 0) - (previous.bytes ?? 0) : 0,
      sameAs: twin ? versionRowRef(twin) : "",
    });
  }
  return deltas;
}

/**
 * Everything one row displays, derived once.
 *
 * The title is the version's NAME when it has one and its time otherwise —
 * the day is already the group's heading, so repeating the date on every row
 * would bury the one thing that distinguishes them. `exact` is the ISO instant
 * for the `<time datetime>` attribute and `timestamp` the full locale spelling
 * for the tooltip and the accessible name, because `docs/139` §14 requires the
 * exact timestamp to be AVAILABLE, not merely implied by a friendly one.
 *
 * `label` is what a screen reader reads, and it carries the title, the full
 * timestamp, the origin, what changed and the "current version" fact in one
 * string — an option whose name is "14:32" tells a non-sighted reader nothing
 * about which of nine rows it is. `change` is in there rather than left to the
 * visible line alone, because a sighted reader can now tell two rows apart and a
 * screen-reader reader could not.
 *
 * O(1). Reads only the row and the delta it is handed.
 *
 * @param {object} row a `version_meta` row.
 * @param {{isHead?: boolean, sizeText?: string, delta?: object,
 *          describeSize?: (bytes: number) => string}} [context]
 */
export function versionRowText(
  row,
  { isHead = false, sizeText = "", delta = null, describeSize = null } = {},
) {
  const at = row?.createdAt ?? 0;
  const named = Boolean(row?.name);
  const clock = d(at, { timeStyle: "short" });
  const timestamp = d(at, { dateStyle: "full", timeStyle: "medium" });
  const kind = versionKindLabel(row?.kind);
  const change = versionChangeText(delta, describeSize);
  const author = String(row?.actor ?? "").trim();
  const parts = [named ? row.name : clock, timestamp, kind];
  if (change) parts.push(change);
  if (author) parts.push(t("versionHistory.row.by", { name: author }));
  if (isHead) parts.push(t("versionHistory.current"));
  if (named) parts.push(t("versionHistory.isNamed"));
  // Origin first, then what changed, then who: the kind says which gesture made
  // the row, and the rest is what tells two rows of one kind apart.
  const detailParts = [kind];
  if (change) detailParts.push(change);
  else if (sizeText) detailParts.push(sizeText);
  if (author) detailParts.push(t("versionHistory.row.by", { name: author }));
  return {
    title: named ? row.name : clock,
    clock,
    timestamp,
    exact: Number.isFinite(at) && at > 0 ? new Date(at).toISOString() : "",
    kind,
    named,
    change,
    author,
    detail: detailParts.join(" · "),
    label: parts.join(", "),
  };
}

/** The "what changed" phrase for one row, or `""` when nothing is derivable.
 *
 *  Order matters and is a decision: "same content" OUTRANKS a count of edits,
 *  because a row whose bytes are already in the timeline is the one fact a reader
 *  most needs and the edits that produced it cancelled out. O(1). */
export function versionChangeText(delta, describeSize = null) {
  if (!delta) return "";
  if (delta.sameAs) return t("versionHistory.row.sameAs", { name: delta.sameAs });
  const phrases = [];
  if (delta.edits > 0) phrases.push(t("versionHistory.row.edits", { count: delta.edits }));
  const size = Number(delta.byteDelta);
  if (describeSize && Number.isFinite(size) && size !== 0) {
    phrases.push(
      size > 0
        ? t("versionHistory.row.grew", { size: describeSize(size) })
        : t("versionHistory.row.shrank", { size: describeSize(-size) }),
    );
  }
  return phrases.join(" · ");
}

/**
 * The one-line disclosure under the timeline: how much history there is, and
 * what the policy will do with it.
 *
 * `docs/139` §12 requires the product to DISCLOSE where history lives and what
 * bounds it, and §8.7 requires the local store to report the size a clear would
 * free. Both are here rather than in a help page because the panel is where
 * somebody asks the question.
 *
 * O(1) — the counts are handed in already summed by the store.
 *
 * @param {{versions: number, bytes: number, pinned: number}} status
 * @param {{maxCount: number, maxAgeMs: number, pinLimit: number}} retention
 * @param {(bytes: number) => string} describeSize
 */
export function retentionSummary(status, retention, describeSize) {
  const days = Math.max(1, Math.round((retention?.maxAgeMs ?? 0) / DAY_MS));
  return {
    kept: t("versionHistory.kept", { count: status?.versions ?? 0 }),
    detail: t("versionHistory.footerDetail", {
      size: describeSize(status?.bytes ?? 0),
      named: n(status?.pinned ?? 0),
      limit: n(retention?.pinLimit ?? 0),
    }),
    policy: t("versionHistory.retention", { days: n(days), count: n(retention?.maxCount ?? 0) }),
  };
}

/**
 * Why version history cannot be used right now, or "" when it can.
 *
 * Its whole job is that the entry point is never a dead control: SKILL §10 and
 * `docs/139` §12 both require the editor to SAY when private browsing, a host
 * policy, a preference or a refused store makes durable history unavailable.
 * The order is most-specific-first, because a framed editor that has also
 * turned the preference off should read as the host's decision rather than as
 * the user's — the host's is the one the user cannot change from here.
 *
 * O(1).
 *
 * @param {{hasDocument: boolean, hostAllows: boolean, autosaveOn: boolean,
 *          historyOn: boolean, storeReason?: string}} state
 */
export function historyUnavailableReason(state) {
  if (!state?.hostAllows) return t("versionHistory.disabled.embedded");
  if (state.storeReason) return state.storeReason;
  if (!state.historyOn) return t("versionHistory.disabled.setting");
  if (!state.autosaveOn) return t("versionHistory.disabled.autosave");
  if (!state.hasDocument) return t("versionHistory.disabled.noDocument");
  return "";
}
