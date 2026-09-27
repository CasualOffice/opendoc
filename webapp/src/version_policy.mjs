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
import { HISTORY_STATUS, VERSION_KIND, historyStatusCodes, isHistoryRefusal } from "./version_history.mjs";

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

/** The `status_channel.mjs` kind a store result must be published with, taken
 *  from the store's own classification so the refusal/confirmation split is
 *  decided once (`version_history.mjs`'s `historyStatusKind`). O(1). */
export function historyMessageKind(status) {
  return isHistoryRefusal(status) ? "error" : "";
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
 * timestamp, the origin and the "current version" fact in one string — an
 * option whose name is "14:32" tells a non-sighted reader nothing about which
 * of nine rows it is.
 *
 * O(1). Reads only the row.
 *
 * @param {object} row a `version_meta` row.
 * @param {{isHead?: boolean, sizeText?: string}} [context]
 */
export function versionRowText(row, { isHead = false, sizeText = "" } = {}) {
  const at = row?.createdAt ?? 0;
  const named = Boolean(row?.name);
  const clock = d(at, { timeStyle: "short" });
  const timestamp = d(at, { dateStyle: "full", timeStyle: "medium" });
  const kind = versionKindLabel(row?.kind);
  const parts = [named ? row.name : clock, timestamp, kind];
  if (isHead) parts.push(t("versionHistory.current"));
  if (named) parts.push(t("versionHistory.isNamed"));
  return {
    title: named ? row.name : clock,
    clock,
    timestamp,
    exact: Number.isFinite(at) && at > 0 ? new Date(at).toISOString() : "",
    kind,
    named,
    detail: sizeText ? `${kind} · ${sizeText}` : kind,
    label: parts.join(", "),
  };
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
