// Durable version history: the store, the capture trigger, naming, retention
// and restore. `104` HF-068 / `105` OO-004, designed in `docs/139` (product) and
// `docs/140` (architecture), with the decisions those two left open recorded in
// **ADR-038**.
//
// What this closes: "get back the version from before I restructured chapter 3"
// is unanswerable today. Undo dies with the session, and the doc 112 draft is
// one slot per tab, overwritten in place — deliberately not a timeline.
//
// Five rules shape every line here.
//
//  1. **A version is not a JSON blob.** `docs/112` §3.2 MEASURED that the
//     normalized-JSON snapshot reports `binary_resources: omitted` and
//     `source_envelope: not_retained` — a document restored from one comes back
//     with every picture gone. So a checkpoint is the same fidelity-complete
//     source-format artifact the draft path already produces, through the same
//     export ladder as Save, and this module never chooses a format: it stores
//     the bytes it is handed and records which format and mode produced them.
//  2. **Same store as the draft.** `104` HF-068's decision cell: *same store as
//     HF-011 — not a second store*. The four object stores are added to
//     `opendoc-drafts` as schema v3 (see `drafts.mjs`), so there is one thing to
//     migrate, one quota, one clear control and one explanation.
//  3. **Autosave must not become O(versions).** The capture DECISION
//     (`VersionCapturePolicy.shouldCapture`) is O(1) in both document size and
//     stored-version count, touches no storage at all, and is the only thing the
//     autosave tick may call. Everything that walks the store runs from a
//     capture, which is rate-limited to one per `versionIntervalMinutes`.
//  4. **No silent loss, in either direction.** A version that cannot be written
//     is REPORTED (`historyStatusKind` maps every refusal to the status kind
//     `status_channel.mjs` escalates to a toast and the assertive region); a
//     named version is never deleted to make room; and pruning is planned from
//     an explicit policy rather than falling out of a quota error.
//  5. **Append, never rewind.** Restore writes a new head whose content is an
//     older checkpoint (`docs/140` §1.2). Nothing is ever removed to make a
//     restore possible, and the pre-restore state becomes a version BEFORE the
//     head moves — the invariant `docs/112`'s own history calls "never let a
//     restore leave the work in neither place".
//
// This module carries no user-facing English. It returns status CODES, and the
// wiring lane turns them into sentences through the localisation catalogue —
// which is also why `historyStatusKind` lives here: the decision about whether a
// message is a refusal belongs with the code that refuses, not with the DOM.

import {
  CHECKPOINT_STORE,
  DOCUMENTS_KEY_INDEX,
  DOCUMENTS_STORE,
  HISTORY_OPS_STORE,
  VERSION_CHECKPOINT_INDEX,
  VERSION_LINEAGE_INDEX,
  VERSION_META_STORE,
  idbCommitted,
  idbRequest,
  openDraftDatabase,
} from "./drafts.mjs";

/** Engine hard ceilings. A host — or a corrupted preference — may ask for less;
 *  it may never ask for more (`docs/140` §16). */
export const HISTORY_LIMITS = Object.freeze({
  MAX_VERSIONS: 200,
  MAX_PINNED: 100,
  MAX_BYTES: 512 * 1024 * 1024,
  MAX_NAME_CHARS: 120,
  MIN_INTERVAL_MS: 60_000,
  MAX_INTERVAL_MS: 24 * 60 * 60 * 1000,
  MAX_AGE_MS: 365 * 24 * 60 * 60 * 1000,
  /** A prepared restore older than this is abandoned rather than resumed: the
   *  session that planned it is long gone and its expectations about the head
   *  cannot be trusted. */
  PREPARED_OP_TTL_MS: 24 * 60 * 60 * 1000,
});

/** Why a version exists. A closed set, because the panel groups on it and a
 *  free-form string would make grouping undefined (`docs/140` §4.3). */
export const VERSION_KIND = Object.freeze({
  IMPORT: "import",
  SAVED: "saved",
  NAMED: "named",
  AUTO: "auto",
  MANUAL: "manual",
  PRE_RESTORE: "pre_restore",
  RESTORE: "restore",
  RECOVERY: "recovery",
});

/** Every outcome this module reports. Codes, not sentences: the catalogue owns
 *  the wording (`docs/124`), and a storage layer that hard-codes English is a
 *  storage layer that cannot be localised. */
export const HISTORY_STATUS = Object.freeze({
  RECORDED: "history.recorded",
  NOT_DUE: "history.notDue",
  UNCHANGED: "history.unchanged",
  PRUNED: "history.pruned",
  RESTORE_PREPARED: "history.restorePrepared",
  RESTORE_COMMITTED: "history.restoreCommitted",
  /** Refusals from here down. */
  FULL_PINNED: "history.fullPinned",
  OVER_BUDGET: "history.overBudget",
  QUOTA_EXHAUSTED: "history.quotaExhausted",
  STORE_UNAVAILABLE: "history.storeUnavailable",
  EVICTED: "history.evicted",
  STALE_HEAD: "history.staleHead",
  MISSING_CHECKPOINT: "history.missingCheckpoint",
  CORRUPT_CHECKPOINT: "history.corruptCheckpoint",
  NAME_REJECTED: "history.nameRejected",
  PIN_LIMIT: "history.pinLimit",
  UNKNOWN_VERSION: "history.unknownVersion",
  UNKNOWN_OPERATION: "history.unknownOperation",
});

/** The refusals: something the user asked for, or was promised, did not happen.
 *
 *  `status_channel.mjs` routes the kind `"error"` to the assertive live region
 *  AND to the viewport toast even when the footer is visible, which is the
 *  behaviour a broken promise about somebody's work needs — the status pill is
 *  the first thing a narrow window sheds. */
const REFUSAL_CODES = new Set([
  HISTORY_STATUS.FULL_PINNED,
  HISTORY_STATUS.OVER_BUDGET,
  HISTORY_STATUS.QUOTA_EXHAUSTED,
  HISTORY_STATUS.STORE_UNAVAILABLE,
  HISTORY_STATUS.EVICTED,
  HISTORY_STATUS.STALE_HEAD,
  HISTORY_STATUS.MISSING_CHECKPOINT,
  HISTORY_STATUS.CORRUPT_CHECKPOINT,
  HISTORY_STATUS.NAME_REJECTED,
  HISTORY_STATUS.PIN_LIMIT,
  HISTORY_STATUS.UNKNOWN_VERSION,
  HISTORY_STATUS.UNKNOWN_OPERATION,
]);

/** Outcomes that are not refusals. Listed rather than inferred, so that a new
 *  code has to be classified deliberately: `version_history.test.mjs` asserts
 *  every code in `HISTORY_STATUS` appears in exactly one of the two sets, which
 *  is what stops a future refusal defaulting to a polite whisper. */
const INFORMATIONAL_CODES = new Set([
  HISTORY_STATUS.RECORDED,
  HISTORY_STATUS.NOT_DUE,
  HISTORY_STATUS.UNCHANGED,
  HISTORY_STATUS.PRUNED,
  HISTORY_STATUS.RESTORE_PREPARED,
  HISTORY_STATUS.RESTORE_COMMITTED,
]);

/** Whether a code is one of the refusals. O(1). */
export function isHistoryRefusal(code) {
  return REFUSAL_CODES.has(code);
}

/** The `status_channel.mjs` kind a code must be published with. O(1).
 *
 *  An unknown code is treated as a refusal on purpose: the failure mode of
 *  guessing "error" is a toast nobody needed, and the failure mode of guessing
 *  "" is silence about lost work. */
export function historyStatusKind(code) {
  return INFORMATIONAL_CODES.has(code) ? "" : "error";
}

/** Every status code, for the completeness guard. O(codes). */
export function historyStatusCodes() {
  return Object.values(HISTORY_STATUS);
}

// ── Retention ────────────────────────────────────────────────────────────────

const MINUTE = 60_000;
const DAY = 24 * 60 * 60 * 1000;

function clamp(value, low, high, fallback) {
  const n = Number(value);
  if (!Number.isFinite(n)) return fallback;
  return Math.min(high, Math.max(low, Math.trunc(n)));
}

/**
 * The effective retention policy, from settings, clamped to the hard limits.
 *
 * O(1). Every value is read defensively because `settings` is
 * `{...DEFAULT_SETTINGS, ...JSON.parse(localStorage)}` — a stored preference is
 * untrusted input, and `docs/140` §14 says so of every persisted string and
 * number in this subsystem.
 *
 * The shape of the policy is ADR-038's answer to the question the owner's
 * ruling ("around 20-30 or retain for 7 days") deliberately left open:
 *
 *   * `maxCount` and `maxBytes` are **ceilings**. They exist to bound storage,
 *     they always apply, and they prune the oldest eligible version first.
 *   * `maxAgeMs` is a **window**, not a deadline. It makes a version eligible
 *     for pruning; it never empties a timeline, because `keepFloor` recent
 *     versions survive it regardless of age.
 */
export function resolveRetention(settings = {}) {
  const maxCount = clamp(settings.versionRetentionCount, 1, HISTORY_LIMITS.MAX_VERSIONS, 25);
  return Object.freeze({
    enabled: settings.versionHistory !== false,
    maxCount,
    maxAgeMs: clamp(settings.versionRetentionDays, 1, HISTORY_LIMITS.MAX_AGE_MS / DAY, 7) * DAY,
    // The floor cannot exceed the ceiling, or the two rules would contradict
    // each other on the first sweep.
    keepFloor: Math.min(maxCount, clamp(settings.versionRetentionFloor, 1, maxCount, 3)),
    maxBytes:
      clamp(
        settings.versionRetentionMegabytes,
        1,
        HISTORY_LIMITS.MAX_BYTES / (1024 * 1024),
        120,
      ) *
      1024 *
      1024,
    intervalMs: clamp(
      Number(settings.versionIntervalMinutes) * MINUTE,
      HISTORY_LIMITS.MIN_INTERVAL_MS,
      HISTORY_LIMITS.MAX_INTERVAL_MS,
      10 * MINUTE,
    ),
    pinLimit: Math.min(
      HISTORY_LIMITS.MAX_PINNED,
      clamp(settings.versionNamedLimit, 1, HISTORY_LIMITS.MAX_PINNED, 15),
    ),
  });
}

/**
 * Which versions a sweep should delete, and why a sweep cannot help.
 *
 * O(v log v) in the number of versions **in one lineage**, which is bounded by
 * `maxCount` (hard ceiling 200) and runs once per capture — never per autosave
 * tick. Pure: it reads plain rows and returns ids, so every branch below is
 * driven from `version_history.test.mjs` without a store.
 *
 * The order is the argument, and ADR-038 records it:
 *
 *  1. **Nothing pinned, nothing protected, and nothing that is the head is ever
 *     eligible.** A named version that vanishes on day eight makes naming a lie
 *     (`docs/139` §8.7, VH-006), and Google Docs prunes unnamed history while
 *     keeping named versions.
 *  2. **The age window prunes only above the floor.** The newest `keepFloor`
 *     versions of the lineage are exempt whatever their age, so a document
 *     nobody has touched for a fortnight still has a past.
 *  3. **The count and byte ceilings then apply unconditionally** to whatever is
 *     still eligible, oldest first, because a ceiling that can be talked out of
 *     applying is not a bound.
 *  4. **If a ceiling cannot be met, the caller is REFUSED and told which wall it
 *     hit** — `FULL_PINNED` when only pins remain, `OVER_BUDGET` when the
 *     incoming artifact cannot fit inside the budget on its own. Deleting a pin
 *     to make room is the one thing this function will not do.
 *
 * @param {Array<object>} versions rows for ONE lineage
 * @param {{retention: object, now: number, protectedIds?: Set<string>,
 *          incomingBytes?: number}} options
 * @returns {{prune: string[], wedged: boolean, reason: string}}
 */
export function planRetention(
  versions,
  { retention, now, protectedIds, incomingBytes = 0, incoming = incomingBytes > 0 } = {},
) {
  const guarded = protectedIds ?? new Set();
  const rows = (versions ?? []).filter(Boolean);
  const eligible = rows
    .filter((row) => !row.pinned && !guarded.has(row.versionId))
    .sort((a, b) => (b.createdAt ?? 0) - (a.createdAt ?? 0));

  const prune = new Set();
  // 2 — the age window, with the floor carved out first.
  //
  // The floor counts the newest versions of the LINEAGE, not the newest eligible
  // ones: "the three most recent versions always survive" is a sentence a user
  // can hold, whereas "the three most recent versions that are not the head and
  // are not named" is arithmetic nobody can predict — and it silently made the
  // floor deeper every time something above it was pinned.
  const floorExempt = new Set(
    [...rows]
      .sort((a, b) => (b.createdAt ?? 0) - (a.createdAt ?? 0))
      .slice(0, retention.keepFloor)
      .map((row) => row.versionId),
  );
  for (const row of eligible) {
    if (floorExempt.has(row.versionId)) continue;
    if (now - (row.createdAt ?? 0) >= retention.maxAgeMs) prune.add(row.versionId);
  }

  // Oldest first for the ceilings: the two rules disagree about which end of the
  // list they read, which is exactly why they are separate passes.
  const sacrificial = [...eligible].reverse().filter((row) => !prune.has(row.versionId));

  // `incoming` counts the row about to be written; `incomingBytes` is what it
  // will cost. They are separate because a RESTORE adds a row and no bytes — it
  // points at an artifact already stored — and deriving the count from the bytes
  // let a restore push a lineage one version past its ceiling. The default keeps
  // the ordinary capture's meaning; `commitRestore` overrides it.
  const liveCount = () =>
    rows.filter((row) => !prune.has(row.versionId)).length + (incoming ? 1 : 0);
  const liveBytes = () =>
    rows.reduce((sum, row) => (prune.has(row.versionId) ? sum : sum + (row.bytes ?? 0)), 0) +
    incomingBytes;

  // 3 — the count ceiling.
  while (liveCount() > retention.maxCount) {
    const next = sacrificial.find((row) => !prune.has(row.versionId));
    if (!next) return { prune: [...prune], wedged: true, reason: HISTORY_STATUS.FULL_PINNED };
    prune.add(next.versionId);
  }

  // 3 — the byte ceiling.
  while (liveBytes() > retention.maxBytes) {
    const next = sacrificial.find((row) => !prune.has(row.versionId));
    if (!next) {
      // Nothing left to give. Either the pins fill the budget, or the incoming
      // artifact is bigger than the whole budget — different sentences, because
      // the way out is different (unpin something vs raise the budget).
      const reason =
        incomingBytes > retention.maxBytes
          ? HISTORY_STATUS.OVER_BUDGET
          : HISTORY_STATUS.FULL_PINNED;
      return { prune: [...prune], wedged: true, reason };
    }
    prune.add(next.versionId);
  }

  return { prune: [...prune], wedged: false, reason: HISTORY_STATUS.RECORDED };
}

/**
 * A version name the store will accept, or the reason it will not.
 *
 * O(n) in the name's length, which is bounded before the work is done. Trimmed,
 * control characters removed, bounded — and **not** deduplicated: identity is
 * the `versionId`, so two versions may share a label and the panel tells them
 * apart by time and author (`docs/139` §8.3). The name is untrusted presentation
 * data (`docs/140` §14): escaping is the renderer's job, and this function's job
 * is to make sure what is stored is a bounded single-line string.
 */
export function sanitiseVersionName(name) {
  if (typeof name !== "string") return { ok: false, reason: HISTORY_STATUS.NAME_REJECTED };
  // `\p{Cc}\p{Cf}` rather than an escaped range: control and format characters
  // are what has to go (a bidi override in a version label is a spoofing vector,
  // and a newline makes one row look like two), and the property escapes say that
  // without putting a raw control byte in this file — which would make git treat
  // it as binary and stop showing its diffs (`tests/source_bytes.test.mjs`).
  const cleaned = name
    .replace(/[\p{Cc}\p{Cf}]/gu, " ")
    .trim()
    .replace(/\s+/g, " ");
  if (cleaned.length === 0) return { ok: false, reason: HISTORY_STATUS.NAME_REJECTED };
  if (cleaned.length > HISTORY_LIMITS.MAX_NAME_CHARS) {
    return { ok: false, reason: HISTORY_STATUS.NAME_REJECTED };
  }
  return { ok: true, name: cleaned };
}

// ── The capture trigger ──────────────────────────────────────────────────────

/** The reasons a capture can be asked for. The first six are the autosave
 *  path's own triggers (`drafts.mjs`'s `DRAFT_WRITE_REASONS`, which
 *  `version_history.test.mjs` asserts is a subset of this object); the rest are
 *  explicit.
 *
 *  EVERY STRING THE DRAFT PATH CAN FIRE HAS TO BE IN HERE. `pagehide` and
 *  `restored` were not, and `main.js` fires both — `draftScheduler.flush("pagehide")`
 *  on the `pagehide` event and `writeDraft("restored")` after a draft is
 *  recovered. An unregistered reason is not inert: it reaches
 *  `VersionCapturePolicy.shouldCapture`, falls past `ALWAYS_CAPTURE` to the
 *  watermark and interval checks, and then reaches `captureVersion` with
 *  `suppressesUnchanged(reason)` answering for a reason it has never heard of.
 *  Measured in Chromium on 2026-10-04 (`version-history-noise.spec.mjs`):
 *
 *      [VH] capture(pagehide) accepted kind=auto
 *      [VH] captureNow reason=pagehide kind=auto suppress=false bytes=16384 ...
 *      [VH] -> status=history.recorded ok=true
 *
 *  — a version row whose content hash was already in the timeline, written
 *  because the reason was unknown. `suppressesUnchanged` now closes that by
 *  defaulting to suppression; registering the reasons is the other half, so the
 *  classification is a decision rather than a fallback. */
export const CAPTURE_REASON = Object.freeze({
  QUIESCE: "quiesce",
  CEILING: "ceiling",
  HIDDEN: "hidden",
  PAGEHIDE: "pagehide",
  RESTORED: "restored",
  RENAME: "rename",
  OPEN: "open",
  SAVE: "save",
  NAME: "name",
  MANUAL: "manual",
  PRE_RESTORE: "pre_restore",
  RESTORE: "restore",
  RECOVERY: "recovery",
});

/** Reasons that always ASK for a version, and the kind each one records.
 *
 *  A user pressed something, or the document changed identity: those are the
 *  points a person recognises in a timeline, and none of them is subject to the
 *  revision watermark or the interval. "Asks for" and not "produces": whether the
 *  version is really written also depends on whether its CONTENT is new, which is
 *  a question about bytes and is answered at capture time — see
 *  `SUPPRESS_UNCHANGED` and `captureVersion`'s `skipIfUnchanged`. */
const ALWAYS_CAPTURE = new Map([
  [CAPTURE_REASON.OPEN, VERSION_KIND.IMPORT],
  [CAPTURE_REASON.SAVE, VERSION_KIND.SAVED],
  [CAPTURE_REASON.NAME, VERSION_KIND.NAMED],
  [CAPTURE_REASON.MANUAL, VERSION_KIND.MANUAL],
  [CAPTURE_REASON.PRE_RESTORE, VERSION_KIND.PRE_RESTORE],
  [CAPTURE_REASON.RESTORE, VERSION_KIND.RESTORE],
  [CAPTURE_REASON.RECOVERY, VERSION_KIND.RECOVERY],
]);

/** Reasons whose capture is SUPPRESSED when the artifact is byte-identical to the
 *  lineage head.
 *
 *  The owner reversed `docs/139` §18 question 3 on 2026-09-28: "version should not
 *  be logged if nothing has changed". Opening a document logged an `import` row
 *  identical to the head, and saving an unmodified one logged a `saved` row beside
 *  it, so a timeline could fill with entries a reader cannot tell apart and cannot
 *  act on. ADR-038 decision 5 said the opposite and has been corrected rather than
 *  left contradicting the code.
 *
 *  It is a per-REASON decision because the reasons are not the same kind of thing:
 *
 *    OPEN, SAVE                     the owner's target. Both are IMPLICIT as far
 *                                   as version history goes — nobody pressed
 *                                   "keep a version" — so a row that says nothing
 *                                   new is noise. The one capture that carries
 *                                   real information, the import baseline of a
 *                                   document with no timeline yet, has no twin to
 *                                   be identical to and is never suppressed.
 *    QUIESCE, CEILING, HIDDEN,      autosave's own triggers. `shouldCapture`
 *    PAGEHIDE, RESTORED, RENAME     already refuses these on an unmoved revision
 *                                   watermark, but a watermark is not content: type
 *                                   a character and delete it and the revision has
 *                                   moved while the document has not. PAGEHIDE and
 *                                   RESTORED are here because `main.js` fires them
 *                                   and they were never registered at all; see
 *                                   `CAPTURE_REASON`.
 *    MANUAL                         moved here on 2026-10-04. The argument below
 *                                   is about somebody asking for a version BY
 *                                   NAME, and MANUAL's only caller is not that:
 *                                   it is `version_panel.mjs`'s pre-copy safety
 *                                   capture, fired because the user pressed "Make
 *                                   a copy". From the reader's side that is as
 *                                   implicit as a Save, the copy's own message is
 *                                   the sentence about what happened, and the row
 *                                   it used to leave behind is one of the rows the
 *                                   owner reported twice. The integrity argument
 *                                   for PRE_RESTORE does not transfer: no
 *                                   invariant is stated over a MANUAL record
 *                                   existing, and `makeCopy` already treats
 *                                   `UNCHANGED` as success.
 *
 *  And these are NOT suppressed, each for its own reason:
 *
 *    NAME                           an explicit user act. Somebody naming the
 *                                   current state expects the named entry to
 *                                   appear, and suppressing it would make the
 *                                   command look broken — the worse failure by
 *                                   far, and content addressing makes the
 *                                   duplicate cost one ~300-byte row. NOTE: this
 *                                   reason currently has no caller —
 *                                   `nameVersion` names a row that already
 *                                   exists rather than capturing a new one — so
 *                                   it is a classification kept correct for the
 *                                   day a "Name the current state" command wants
 *                                   it, not a live code path.
 *    PRE_RESTORE, RESTORE,          INTEGRITY captures, and the careful case.
 *    RECOVERY                       `docs/140` §9 makes the pre-restore capture a
 *                                   PRECONDITION of restore: the current state
 *                                   becomes a version before the head moves, and
 *                                   if that capture is refused the restore is
 *                                   refused with it, so that there is no state in
 *                                   which the work is in neither place. A
 *                                   suppression is not a refusal and the head
 *                                   would in fact already hold those bytes — but
 *                                   the invariant is stated over a RECORD existing
 *                                   and is relied on by the prepare/commit
 *                                   compare-and-set and by `resolvePendingRestores`
 *                                   at the next boot. Trading a documented
 *                                   integrity invariant for one row in the rare
 *                                   case of restoring without having edited is a
 *                                   bad trade, so these keep capturing. The path
 *                                   does not even opt in: `prepareRestore` calls
 *                                   `captureVersion` without `skipIfUnchanged`,
 *                                   whose default is off.
 *
 *  Every `CAPTURE_REASON` is in this set or in `KEEP_UNCHANGED`, and
 *  `version_history.test.mjs` fails if one is in both or neither — the same
 *  "classify it deliberately" guard `REFUSAL_CODES` has, for the same reason. */
export const SUPPRESS_UNCHANGED = Object.freeze([
  CAPTURE_REASON.QUIESCE,
  CAPTURE_REASON.CEILING,
  CAPTURE_REASON.HIDDEN,
  CAPTURE_REASON.PAGEHIDE,
  CAPTURE_REASON.RESTORED,
  CAPTURE_REASON.RENAME,
  CAPTURE_REASON.OPEN,
  CAPTURE_REASON.SAVE,
  CAPTURE_REASON.MANUAL,
]);

/** The reasons that capture even when nothing changed. See `SUPPRESS_UNCHANGED`
 *  for the argument on each.
 *
 *  THIS IS THE LIST THAT DECIDES. `suppressesUnchanged` asks whether a reason is
 *  in here, not whether it is in `SUPPRESS_UNCHANGED`, so the four integrity
 *  captures and `NAME` are the only things that can write a duplicate — and a
 *  reason nobody classified suppresses instead of keeping. `SUPPRESS_UNCHANGED`
 *  stays as the written argument and as the other half of the both-or-neither
 *  guard; it is not the predicate. */
export const KEEP_UNCHANGED = Object.freeze([
  CAPTURE_REASON.NAME,
  CAPTURE_REASON.PRE_RESTORE,
  CAPTURE_REASON.RESTORE,
  CAPTURE_REASON.RECOVERY,
]);

/** Whether a capture for `reason` is skipped when its bytes are already somewhere
 *  in this lineage.
 *
 *  O(reasons) over a frozen list of four. **The question is "is this reason one of
 *  the few that must keep a duplicate", not "is this reason on the suppress
 *  list"** — those read the same for every reason either list names, and
 *  differently for a reason neither does, which is the case that shipped the
 *  defect. `SUPPRESS_UNCHANGED.includes(reason)` answered `false` for `"pagehide"`
 *  and `"restored"` — two strings `main.js` fires and `CAPTURE_REASON` had never
 *  heard of — so the capture went through unsuppressed and wrote a byte-identical
 *  row. The dangerous default was "keep"; it is now "suppress", which is the same
 *  deliberate-classification posture `historyStatusKind` takes in the other
 *  direction (an unknown status code is treated as a refusal, because silence
 *  about lost work is the worse failure there and a duplicate row is the worse
 *  failure here).
 *
 *  Deliberately NOT consulted by `shouldCapture`: that one is the editing path's
 *  whole contribution and must stay O(1) with no storage and no bytes, and a
 *  content comparison needs the artifact. The answer is carried to
 *  `captureVersion`, which has already hashed the bytes it is about to write and
 *  has already read the lineage's rows, so the comparison costs one string
 *  equality per row and no extra work at all. */
export function suppressesUnchanged(reason) {
  return !KEEP_UNCHANGED.includes(reason);
}

/**
 * Decides whether an autosave tick should also lay down a version.
 *
 * This class is the whole of version history's presence on the editing path, and
 * it is deliberately three comparisons over two numbers it holds itself:
 * **`shouldCapture` is O(1) in document size AND O(1) in the number of stored
 * versions, and reads no storage at all.** `docs/107` §4 B5 — snapshots are
 * periodic, never per-operation — is what this enforces; walking the store to
 * decide would have made autosave O(versions), which is the defect `docs/116`
 * records in its worst form.
 *
 * The clock is injected for the same reason `DraftScheduler`'s is: the ten
 * minute interval is not something a test should wait for.
 */
export class VersionCapturePolicy {
  constructor({ intervalMs = 10 * MINUTE } = {}) {
    this.intervalMs = intervalMs;
    this.lastAt = 0;
    this.lastRevision = null;
    this.captured = 0;
  }

  /** O(1). Called after a capture actually committed — never before, so a
   *  refused write does not reset the interval and quietly skip the next one. */
  noteCaptured(at, revision) {
    this.lastAt = at;
    this.lastRevision = revision;
    this.captured += 1;
  }

  /**
   * O(1). `{capture, kind, why}` — `why` is a status code, so a decision not to
   * capture is reportable rather than invisible.
   *
   * `revision` is the engine revision watermark the autosave path already reads
   * (`documentIsDirty`'s `currentRevision`), which is what makes "has anything
   * changed since the last version" engine-authoritative instead of inferred
   * from the UI.
   */
  shouldCapture({ reason, revision, now, enabled = true } = {}) {
    if (!enabled) return { capture: false, kind: null, why: HISTORY_STATUS.NOT_DUE };
    const forced = ALWAYS_CAPTURE.get(reason);
    if (forced) {
      // Past the watermark and the interval, both of which are about WHEN rather
      // than about what: an explicit reason is a point the user recognises and is
      // never withheld for being early. Whether the version is actually written
      // is a question about CONTENT, and a content comparison needs the bytes —
      // which is why it happens at capture time (`suppressesUnchanged`,
      // `captureVersion`'s `skipIfUnchanged`) and not here. This function stays
      // O(1) in document size and O(1) in stored versions, reading no storage.
      //
      // `docs/139` §18 question 3 originally settled the opposite — an explicit
      // Save always lays down a version, identical bytes or not — and the owner
      // reversed it on 2026-09-28; ADR-038 decision 5 records the reversal.
      return { capture: true, kind: forced, why: HISTORY_STATUS.RECORDED };
    }
    if (revision === this.lastRevision) {
      return { capture: false, kind: null, why: HISTORY_STATUS.UNCHANGED };
    }
    // THE TAB IS GOING AWAY, so the interval no longer protects anything: it
    // exists to stop a pause in typing spending the count ceiling, and there is
    // no next pause after `pagehide`. Waiting for it lost up to ten minutes of
    // the last session's edits — the ones a reader is likeliest to want back.
    // A tab merely switched away from (`hidden`) still waits, because that
    // happens many times an hour.
    if (reason === CAPTURE_REASON.PAGEHIDE) {
      return { capture: true, kind: VERSION_KIND.AUTO, why: HISTORY_STATUS.RECORDED };
    }
    if (this.lastAt !== 0 && now - this.lastAt < this.intervalMs) {
      return { capture: false, kind: null, why: HISTORY_STATUS.NOT_DUE };
    }
    return { capture: true, kind: VERSION_KIND.AUTO, why: HISTORY_STATUS.RECORDED };
  }
}

// ── Identity and integrity ───────────────────────────────────────────────────

function randomId(prefix) {
  const cryptoObj = globalThis.crypto;
  const tail = cryptoObj?.randomUUID
    ? cryptoObj.randomUUID()
    : `${Math.random().toString(36).slice(2)}${Date.now().toString(36)}`;
  return `${prefix}-${tail}`;
}

/** Minted, never derived. A lineage is not its filename, its bytes or its
 *  timestamp (`docs/140` §4.1): Save As, a restored copy and two identical files
 *  all need identities of their own. O(1). */
export const newLineageId = () => randomId("lin");
export const newVersionId = () => randomId("ver");
export const newOperationId = () => randomId("op");

/** FNV-1a over every byte — the fallback content hash, used only where
 *  `crypto.subtle` is missing (a non-secure context). O(bytes) and on the
 *  calling thread, which is why it is the fallback and not the default. */
function fnv1aHex(bytes) {
  let hash = 0x811c9dc5;
  for (let i = 0; i < bytes.length; i++) {
    hash ^= bytes[i];
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return `fnv1a-${(hash >>> 0).toString(16)}-${bytes.length.toString(16)}`;
}

/**
 * Whether a stored version row holds the same DOCUMENT as the capture in hand.
 *
 * TWO IDENTITIES, AND THE CONTENT ONE DECIDES. `checkpointId` is a SHA-256 of
 * the **source-format bytes**, and bytes are not what "the same document" means:
 * the import row holds the original file verbatim, `source_unchanged` is
 * `revision == 0` — a monotonic watermark — so after any edit at all, even one
 * that is immediately undone, the exact export mode is permanently unavailable
 * and every later checkpoint is a re-export with a different byte layout. One
 * unchanged document, two hashes, and a version row with nothing in it. That is
 * the defect the owner reported three times.
 *
 * `contentId` is the engine's `contentDigest()` — a fold of the semantic
 * projection the Compare pipeline aligns on plus the definition values it
 * compares (`casual_doc_diff::identity`), so two documents that say the same
 * thing share it however they were serialized, and it is the SAME notion of
 * sameness Compare uses rather than a second one that could drift.
 *
 * The byte hash is still consulted, as a fallback, for exactly two cases: a row
 * written before this field existed, and a host whose engine could not supply a
 * digest. Byte-identical artifacts ARE the same document, so keeping it loses
 * nothing — it is a weaker test kept for the rows that have nothing stronger.
 *
 * O(1): two string comparisons, inside the transaction that already read the row.
 */
export function isSameDocument(row, candidate) {
  if (candidate?.contentId && row?.contentId === candidate.contentId) return true;
  return Boolean(row?.checkpointId) && row.checkpointId === candidate?.checkpointId;
}

/**
 * The checkpoint's content hash, which is also its storage key.
 *
 * O(bytes), and deliberately `crypto.subtle.digest`: the digest of a 2.4 MB
 * artifact is computed by the browser off the JavaScript thread, so the one
 * document-sized cost this module adds does not block a frame. SKILL §8's rule
 * — anything O(document) stays off the main thread — is why this is async and
 * why it is awaited BEFORE any IndexedDB transaction opens: awaiting a
 * non-IDB promise inside a live transaction lets it auto-commit underneath you.
 */
export async function checkpointIdFor(bytes, subtle = globalThis.crypto?.subtle) {
  if (!subtle?.digest) return fnv1aHex(bytes);
  const digest = await subtle.digest("SHA-256", bytes);
  const hex = [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
  return `sha256-${hex}`;
}

// ── The store ────────────────────────────────────────────────────────────────

const HISTORY_STORES = [
  DOCUMENTS_STORE,
  VERSION_META_STORE,
  CHECKPOINT_STORE,
  HISTORY_OPS_STORE,
];

/** A rejection worth distinguishing: the browser is out of room. Chromium,
 *  Firefox and Safari all surface it as a `QuotaExceededError` DOMException. */
function isQuotaError(err) {
  return err?.name === "QuotaExceededError" || err?.name === "QuotaExceeded";
}

function result(code, extra = {}) {
  return { ok: !isHistoryRefusal(code), status: code, kind: historyStatusKind(code), ...extra };
}

/**
 * Opens the version store on the draft database's schema v3.
 *
 * Rejects rather than degrading silently, exactly as `openDraftStore` does: a
 * history that is not being recorded is a promise the editor is not keeping, and
 * the caller — which owns the status channel — is the one that must say so.
 *
 * `indexedDB` and `subtle` are injected so the rules run under a fake in node
 * and under the real thing in the browser; the e2e spec passes a wrapped
 * `indexedDB` that fails on command, which is how the atomicity claims below are
 * driven red against REAL IndexedDB rather than against a model of it.
 */
export async function openHistoryStore({
  indexedDB = globalThis.indexedDB,
  name = undefined,
  subtle = globalThis.crypto?.subtle,
  storage = globalThis.navigator?.storage,
} = {}) {
  const db = name ? await openDraftDatabase(indexedDB, name) : await openDraftDatabase(indexedDB);

  /** Every version row of one lineage, newest first.
   *  O(v) in that lineage's versions, bounded by the count ceiling; metadata
   *  only, so this is kilobytes and never touches a checkpoint artifact
   *  (`docs/139` VH-001). */
  async function readLineageVersions(store, lineageId) {
    const rows = await idbRequest(store.index(VERSION_LINEAGE_INDEX).getAll(lineageId));
    return rows.sort((a, b) => (b.createdAt ?? 0) - (a.createdAt ?? 0));
  }

  /** Deletes a checkpoint artifact once no version row references it.
   *  O(refs) — an index lookup on the hash. Called after the meta deletions in
   *  the same transaction, so the count it reads already excludes them
   *  (`docs/140` §13: a blob goes at reference count zero, never before). */
  async function freeCheckpointIfUnreferenced(versions, blobs, checkpointId) {
    if (!checkpointId) return false;
    const refs = await idbRequest(versions.index(VERSION_CHECKPOINT_INDEX).getAllKeys(checkpointId));
    if (refs.length > 0) return false;
    blobs.delete(checkpointId);
    return true;
  }

  async function readDocument(store, lineageId) {
    return idbRequest(store.get(lineageId));
  }

  /**
   * Version ids a prepared operation is holding, which no sweep may prune.
   *
   * `docs/140` §13 lists "artifacts referenced by active previews, drafts,
   * branches, or prepared operations" among the things retention must never
   * touch, and this is the half of that list this layer can enforce. It is not
   * decoration: a restore of the OLDEST version starts by capturing the current
   * state, that capture is what pushes the lineage over its ceiling, and the
   * oldest version is precisely what the sweep would then release — so without
   * this the artifact being restored is deleted between prepare and commit, and
   * the restore fails with a missing checkpoint. Found by the guard "a restore at
   * the cap leaves no artifact that nothing points at", not by reading the code.
   *
   * O(prepared operations), which is bounded because a resolved operation is
   * retired at the next boot.
   */
  async function leasedVersionIds(ops, lineageId) {
    const all = await idbRequest(ops.getAll());
    const leased = new Set();
    for (const op of all) {
      if (op.state !== "prepared" || op.lineageId !== lineageId) continue;
      if (op.targetVersionId) leased.add(op.targetVersionId);
      if (op.preRestoreVersionId) leased.add(op.preRestoreVersionId);
    }
    return leased;
  }

  /** One version row, or null. O(1). */
  async function getVersionRow(versionId) {
    const tx = db.transaction(VERSION_META_STORE, "readonly");
    return (await idbRequest(tx.objectStore(VERSION_META_STORE).get(versionId))) ?? null;
  }

  /** The lineage's current head version id, or null. O(1). */
  async function headOf(lineageId) {
    const tx = db.transaction(DOCUMENTS_STORE, "readonly");
    const row = await readDocument(tx.objectStore(DOCUMENTS_STORE), lineageId);
    return row?.headVersionId ?? null;
  }

  /** The operation carrying this idempotency key, or null. O(operations), which
   *  is bounded because a resolved operation is retired at the next boot. */
  async function operationForKey(idempotencyKey) {
    const tx = db.transaction(HISTORY_OPS_STORE, "readonly");
    const all = await idbRequest(tx.objectStore(HISTORY_OPS_STORE).getAll());
    return all.find((op) => op.idempotencyKey && op.idempotencyKey === idempotencyKey) ?? null;
  }

  /** The oldest unpinned, unprotected version of a lineage, or null.
   *  O(versions in the lineage). */
  async function oldestEligibleIn(lineageId, protectedIds = null) {
    const guarded = new Set(protectedIds ?? []);
    const tx = db.transaction(VERSION_META_STORE, "readonly");
    const rows = await readLineageVersions(tx.objectStore(VERSION_META_STORE), lineageId);
    const candidate = [...rows].reverse().find((row) => !row.pinned && !guarded.has(row.versionId));
    return candidate?.versionId ?? null;
  }

  /** Reads a checkpoint and verifies it against the hash its key claims.
   *
   *  O(bytes) — the read plus the digest. Verified rather than trusted because
   *  "merely writing bytes is not enough" (`docs/140` §4.4): a truncated or
   *  half-evicted artifact has to be reported as corrupt, never opened. */
  async function loadCheckpoint(checkpointId) {
    const tx = db.transaction(CHECKPOINT_STORE, "readonly");
    const bytes = await idbRequest(tx.objectStore(CHECKPOINT_STORE).get(checkpointId));
    if (!bytes) return result(HISTORY_STATUS.MISSING_CHECKPOINT, { checkpointId });
    const actual = await checkpointIdFor(bytes, subtle);
    if (actual !== checkpointId) {
      return result(HISTORY_STATUS.CORRUPT_CHECKPOINT, { checkpointId, actual });
    }
    return result(HISTORY_STATUS.RECORDED, { bytes, checkpointId });
  }

  const api = {
    /**
     * Finds or creates this document's lineage, and returns its timeline.
     *
     * `docKey` is the `documentKey` hint `drafts.mjs` already computes — a
     * bounded sample hash, O(1) in document size. It is a HINT and not the
     * identity: the lineage id is minted and stored, because `docs/140` §4.1
     * forbids deriving identity from a filename or a byte hash. A hint collision
     * can only join two timelines that should have been separate — confusing,
     * never lossy, and the name has to match too. Save As and Make a copy mint a
     * new lineage explicitly by passing `fresh: true`.
     *
     * O(lineages with this key + versions in the lineage); both bounded.
     */
    async openLineage({ docKey, name: docName = "", now = Date.now(), fresh = false } = {}) {
      const tx = db.transaction([DOCUMENTS_STORE, VERSION_META_STORE], "readwrite");
      const documents = tx.objectStore(DOCUMENTS_STORE);
      let row = null;
      if (!fresh && docKey) {
        const candidates = await idbRequest(documents.index(DOCUMENTS_KEY_INDEX).getAll(docKey));
        row =
          candidates
            .filter((c) => (c.name ?? "") === docName)
            .sort((a, b) => (b.updatedAt ?? 0) - (a.updatedAt ?? 0))[0] ?? null;
      }
      if (!row) {
        row = {
          schema: 1,
          lineageId: newLineageId(),
          docKey: docKey ?? "",
          name: docName,
          headVersionId: null,
          createdAt: now,
          updatedAt: now,
        };
      }
      documents.put({ ...row, updatedAt: now });
      const versions = await readLineageVersions(tx.objectStore(VERSION_META_STORE), row.lineageId);
      await idbCommitted(tx);
      return { lineageId: row.lineageId, headVersionId: row.headVersionId, versions };
    },

    /** The timeline, metadata only. O(versions in the lineage). */
    async listVersions(lineageId) {
      const tx = db.transaction(VERSION_META_STORE, "readonly");
      return readLineageVersions(tx.objectStore(VERSION_META_STORE), lineageId);
    },

    /** One checkpoint's bytes, verified against the hash its key claims. */
    readCheckpoint: loadCheckpoint,

    /**
     * Writes one version: the artifact, its metadata, the retention sweep it
     * implies, and the new head — in ONE IndexedDB transaction.
     *
     * Atomicity is the point. A metadata row that references bytes which were
     * never written would be a version the panel offers and the restore path
     * cannot load (`docs/140` §7.2), so both stores and the head pointer move
     * together or not at all.
     *
     * Complexity: O(bytes) for the hash (off-thread, before the transaction) and
     * O(versions in this lineage) for the plan and the sweep — bounded by the
     * count ceiling, and paid once per capture rather than per autosave tick.
     * The autosave path's own contribution is `VersionCapturePolicy`, which
     * touches none of this.
     *
     * `skipIfUnchanged` reports `UNCHANGED` and writes nothing when the capture
     * holds the same DOCUMENT as ANY version already in this lineage — not only
     * as the head — and corrects the head onto the row that holds it. Sameness is
     * `isSameDocument`: the engine's content digest (`contentId`) first, the
     * artifact's byte hash only as the fallback for a row that carries no digest.
     * Dedupe used to be on bytes alone, which is why a save after an edit and its
     * undo kept a version with nothing in it. The caller decides, per reason, and
     * the default is OFF, so the restore path, which calls this without the flag,
     * keeps its pre-restore capture unconditionally (`KEEP_UNCHANGED`).
     *
     * Quota is handled and never guessed at: one extra eligible version is
     * released and the write retried once, and if it still fails the caller gets
     * `QUOTA_EXHAUSTED` — a refusal, which reaches the user as one. Nothing
     * pinned is ever released to make room.
     */
    async captureVersion({
      lineageId,
      bytes,
      formatId,
      exportMode = "",
      findings = 0,
      revision = null,
      engine = "",
      actor = "",
      kind = VERSION_KIND.AUTO,
      name = "",
      now = Date.now(),
      retention,
      protectedIds = null,
      expectedHead = undefined,
      skipIfUnchanged = false,
      contentId = "",
      words = null,
    }) {
      if (!bytes || bytes.length === 0) return result(HISTORY_STATUS.MISSING_CHECKPOINT);
      const named = sanitiseVersionName(name);
      if (name !== "" && !named.ok) return result(HISTORY_STATUS.NAME_REJECTED);

      // Before the transaction, deliberately: this is the one document-sized
      // cost, it is asynchronous, and an `await` inside a live IDB transaction
      // would let the transaction commit underneath it.
      const checkpointId = await checkpointIdFor(bytes, subtle);

      const write = async (extraSacrifice) => {
        const tx = db.transaction(HISTORY_STORES, "readwrite");
        const documents = tx.objectStore(DOCUMENTS_STORE);
        const metas = tx.objectStore(VERSION_META_STORE);
        const blobs = tx.objectStore(CHECKPOINT_STORE);

        const lineage = await readDocument(documents, lineageId);
        if (!lineage) return result(HISTORY_STATUS.UNKNOWN_VERSION, { lineageId });
        if (expectedHead !== undefined && (lineage.headVersionId ?? null) !== expectedHead) {
          // Another tab advanced this lineage. Last-writer-wins is explicitly
          // not an acceptable merge policy here (`docs/140` §7.3).
          return result(HISTORY_STATUS.STALE_HEAD, { head: lineage.headVersionId ?? null });
        }

        const existing = await readLineageVersions(metas, lineageId);
        // NOTHING NEW TO KEEP (`docs/139` §18 q3 as the owner reversed it on
        // 2026-09-28). "Already in this timeline" is `isSameDocument` — the
        // CONTENT identity the engine computed for this capture against the ones
        // the rows already store, with the byte hash as the fallback for a row
        // that has no digest. One or two string comparisons per row, inside the
        // transaction that is already reading them, so nothing is hashed, read or
        // walked twice. Here rather than in `shouldCapture` because that one is
        // the editing path's whole contribution and may not touch bytes or
        // storage.
        //
        // Inside the transaction on purpose: doing it outside would be a
        // check-then-write across a `readwrite` boundary, where a second tab's
        // capture between the two would leave this one writing against a head it
        // no longer saw — the same race `expectedHead` exists for.
        //
        // UNCHANGED is not a refusal (`INFORMATIONAL_CODES`), so `result.ok` is
        // true and nothing is reported as an error: nothing went wrong. The row
        // that holds these bytes is handed back so a caller can SAY which version
        // the document already matches — which `version_panel.mjs` does, because
        // §10 forbids a silent no-op.
        //
        // THE WHOLE LINEAGE, not just the head. A duplicate is a duplicate
        // wherever it already sits: two rows holding one document are two rows
        // a reader cannot tell apart and cannot choose between, and the head is
        // only one of the places the twin can be. Measured in Chromium on
        // 2026-10-04 — open a document, edit it, save, then reload the same file:
        //
        //    after reload 1: import cp=fb07bd2d | saved cp=28c11967 | import cp=fb07bd2d
        //
        // The third row is byte-identical to the first and was written because
        // the comparison only looked at the head, which was `saved` at the time.
        // Every reload after a save added one, forever.
        //
        // And that same recorded line is the OTHER half of the defect, which the
        // byte comparison cannot see: `saved` was captured after an edit that
        // cancelled itself out, so it holds the same document as `import` and a
        // different hash. `contentId` is what tells them apart — see
        // `isSameDocument`.
        //
        // AND THE HEAD MOVES TO THE TWIN. The head is defined one screen down as
        // "the only version that still describes the document" — `deleteVersion`
        // refuses to delete it on exactly that ground — so leaving it on a row
        // whose content the document no longer holds is the lie that made
        // suppressing the duplicate look wrong. Pointing it at the row that does
        // hold these bytes is what makes "no new row" honest: the panel's
        // "current version" marker lands on the version a restore would get you.
        // Nothing is rewritten and nothing is removed — `createdAt`,
        // `parentVersionId` and the rest of the chain are untouched, so this is a
        // pointer correction and not a rewind of the timeline. A concurrent
        // prepared restore notices through `expectedHead` and refuses, which is
        // the behaviour it already has for any other head movement.
        if (skipIfUnchanged) {
          const twins = existing.filter((row) => isSameDocument(row, { checkpointId, contentId }));
          if (twins.length > 0) {
            const atHead = twins.find((row) => row.versionId === lineage.headVersionId);
            // The head when the head is one of them (nothing to correct),
            // otherwise the NEWEST twin: it is the one nearest the reader in the
            // timeline and the one a "current version" marker belongs on.
            const holder =
              atHead ??
              twins.reduce((best, row) => ((row.createdAt ?? 0) >= (best.createdAt ?? 0) ? row : best));
            if (!atHead) {
              documents.put({ ...lineage, headVersionId: holder.versionId, updatedAt: now });
              await idbCommitted(tx);
            }
            return result(HISTORY_STATUS.UNCHANGED, { version: holder });
          }
        }
        const pinned = existing.filter((row) => row.pinned).length;
        const wantsPin = Boolean(named.ok && named.name);
        if (wantsPin && pinned >= retention.pinLimit) {
          return result(HISTORY_STATUS.PIN_LIMIT, { pinned, limit: retention.pinLimit });
        }

        // The head of record is protected only while it is still the head: the
        // version being written becomes the new one, so the old head is ordinary
        // history the moment this transaction commits. What IS protected is
        // whatever a prepared restore is holding.
        const guarded = new Set(protectedIds ?? []);
        for (const id of await leasedVersionIds(tx.objectStore(HISTORY_OPS_STORE), lineageId)) {
          guarded.add(id);
        }
        const plan = planRetention(existing, {
          retention,
          now,
          protectedIds: guarded,
          incomingBytes: bytes.length,
        });
        if (plan.wedged) return result(plan.reason, { pruned: plan.prune });

        const doomed = new Set(plan.prune);
        for (const id of extraSacrifice ?? []) doomed.add(id);
        const freedCheckpoints = new Set();
        for (const id of doomed) {
          const row = existing.find((r) => r.versionId === id);
          metas.delete(id);
          if (row?.checkpointId && row.checkpointId !== checkpointId) {
            freedCheckpoints.add(row.checkpointId);
          }
        }
        for (const id of freedCheckpoints) {
          await freeCheckpointIfUnreferenced(metas, blobs, id);
        }

        const version = {
          schema: 1,
          versionId: newVersionId(),
          lineageId,
          checkpointId,
          // The identity a later capture is actually compared against. Stored
          // beside the byte hash rather than instead of it: the byte hash is also
          // the checkpoint's storage key, and two rows may legitimately share one
          // artifact (Save, then Name this version).
          contentId,
          // The document's word count at capture, or null when the host gave
          // none (an older row, a host without stats): what lets a row say
          // "Words added: 12" without parsing a checkpoint.
          words: Number.isFinite(words) ? words : null,
          parentVersionId: lineage.headVersionId ?? null,
          createdAt: now,
          kind,
          name: named.ok ? named.name : "",
          pinned: wantsPin,
          formatId: formatId ?? "",
          exportMode,
          findings,
          revision,
          bytes: bytes.length,
          engine,
          actor,
        };
        // Content-addressed, so an identical artifact is stored once however
        // many versions point at it — Save followed by Name this version costs
        // one row, not a second copy of the document.
        blobs.put(bytes, checkpointId);
        metas.put(version);
        documents.put({ ...lineage, headVersionId: version.versionId, updatedAt: now });
        await idbCommitted(tx);
        return result(HISTORY_STATUS.RECORDED, { version, pruned: [...doomed] });
      };

      try {
        return await write(null);
      } catch (err) {
        if (!isQuotaError(err)) return result(HISTORY_STATUS.STORE_UNAVAILABLE, { error: String(err) });
        // One eligible version beyond the policy, then one retry. Pins are not
        // candidates, which is why this can legitimately fail.
        const spare = await oldestEligibleIn(lineageId, protectedIds);
        if (!spare) return result(HISTORY_STATUS.QUOTA_EXHAUSTED);
        try {
          return await write([spare]);
        } catch (retryErr) {
          return result(HISTORY_STATUS.QUOTA_EXHAUSTED, { error: String(retryErr) });
        }
      }
    },

    /** The oldest unpinned, unprotected version of a lineage, or null. */
    oldestEligible: oldestEligibleIn,

    /**
     * Names a version, which pins it.
     *
     * O(versions in the lineage) — the pin limit has to be counted. Naming
     * changes metadata only: it never rewrites content or commit identity
     * (`docs/139` §8.3). Renaming an already-named version does not consume a
     * second pin.
     */
    async nameVersion(
      versionId,
      name,
      { now = Date.now(), pinLimit = HISTORY_LIMITS.MAX_PINNED } = {},
    ) {
      const named = sanitiseVersionName(name);
      if (!named.ok) return result(named.reason);
      const tx = db.transaction(VERSION_META_STORE, "readwrite");
      const metas = tx.objectStore(VERSION_META_STORE);
      const row = await idbRequest(metas.get(versionId));
      if (!row) return result(HISTORY_STATUS.UNKNOWN_VERSION, { versionId });
      if (!row.pinned) {
        const siblings = await readLineageVersions(metas, row.lineageId);
        const pinned = siblings.filter((other) => other.pinned).length;
        if (pinned >= pinLimit) return result(HISTORY_STATUS.PIN_LIMIT, { pinned });
      }
      const updated = { ...row, name: named.name, pinned: true, namedAt: now };
      metas.put(updated);
      await idbCommitted(tx);
      return result(HISTORY_STATUS.RECORDED, { version: updated });
    },

    /** Pins or unpins without naming (`docs/139` §18 question 4: yes, a pin is
     *  available without a label — the label is what makes it findable, the pin
     *  is what makes it durable). O(1) plus the pin count when pinning. */
    async setPinned(versionId, pinned, { pinLimit = HISTORY_LIMITS.MAX_PINNED } = {}) {
      const tx = db.transaction(VERSION_META_STORE, "readwrite");
      const metas = tx.objectStore(VERSION_META_STORE);
      const row = await idbRequest(metas.get(versionId));
      if (!row) return result(HISTORY_STATUS.UNKNOWN_VERSION, { versionId });
      if (pinned && !row.pinned) {
        const siblings = await readLineageVersions(metas, row.lineageId);
        if (siblings.filter((other) => other.pinned).length >= pinLimit) {
          return result(HISTORY_STATUS.PIN_LIMIT);
        }
      }
      const updated = { ...row, pinned: Boolean(pinned) };
      metas.put(updated);
      await idbCommitted(tx);
      return result(HISTORY_STATUS.RECORDED, { version: updated });
    },

    /**
     * Deletes one version explicitly — the user asked, which automatic
     * retention never does to a pin.
     *
     * Refuses to delete the head: it is the only version that still describes
     * the document, and `docs/140` §13 lists it first among the things a sweep
     * may not touch. O(versions in the lineage).
     */
    async deleteVersion(versionId) {
      const tx = db.transaction(HISTORY_STORES, "readwrite");
      const metas = tx.objectStore(VERSION_META_STORE);
      const blobs = tx.objectStore(CHECKPOINT_STORE);
      const documents = tx.objectStore(DOCUMENTS_STORE);
      const row = await idbRequest(metas.get(versionId));
      if (!row) return result(HISTORY_STATUS.UNKNOWN_VERSION, { versionId });
      const lineage = await readDocument(documents, row.lineageId);
      if (lineage?.headVersionId === versionId) {
        return result(HISTORY_STATUS.STALE_HEAD, { versionId });
      }
      metas.delete(versionId);
      const freed = await freeCheckpointIfUnreferenced(metas, blobs, row.checkpointId);
      await idbCommitted(tx);
      return result(HISTORY_STATUS.PRUNED, { versionId, freedCheckpoint: freed });
    },

    /**
     * Runs the retention policy without writing a version — the sweep a boot
     * does, so a session that ended eight days ago is not still holding content
     * the policy says to release.
     *
     * The head IS protected here, unlike during a capture: there is no incoming
     * version to take its place. O(versions in the lineage).
     */
    async sweep({ lineageId, retention, now = Date.now(), protectedIds = null }) {
      const tx = db.transaction(HISTORY_STORES, "readwrite");
      const documents = tx.objectStore(DOCUMENTS_STORE);
      const metas = tx.objectStore(VERSION_META_STORE);
      const blobs = tx.objectStore(CHECKPOINT_STORE);
      const lineage = await readDocument(documents, lineageId);
      const rows = await readLineageVersions(metas, lineageId);
      const guarded = new Set(protectedIds ?? []);
      if (lineage?.headVersionId) guarded.add(lineage.headVersionId);
      for (const id of await leasedVersionIds(tx.objectStore(HISTORY_OPS_STORE), lineageId)) {
        guarded.add(id);
      }
      const plan = planRetention(rows, { retention, now, protectedIds: guarded });
      const freedCheckpoints = new Set();
      for (const id of plan.prune) {
        const row = rows.find((r) => r.versionId === id);
        metas.delete(id);
        if (row?.checkpointId) freedCheckpoints.add(row.checkpointId);
      }
      for (const id of freedCheckpoints) await freeCheckpointIfUnreferenced(metas, blobs, id);
      await idbCommitted(tx);
      return result(plan.wedged ? plan.reason : HISTORY_STATUS.PRUNED, { pruned: plan.prune });
    },

    /**
     * Step one of a restore: validate the target and make the current state a
     * version, then record what is about to happen.
     *
     * Nothing about the live document changes here (`docs/140` §9.2). The order
     * is the safety property and it is not negotiable: **the current work
     * becomes a version BEFORE the head moves**, and if that capture is refused
     * — quota, a wedged budget — the restore is refused with it and the document
     * is exactly as it was. That is `docs/112`'s "never let a restore leave the
     * work in neither place", applied one level up.
     *
     * Returns the validated target bytes so the caller can open them in an
     * isolated session and only then commit; a restore whose parse fails must
     * abandon the prepared record rather than commit it.
     *
     * Complexity: O(target bytes) for the read and hash verification, plus
     * O(current bytes) for the pre-restore capture. Both are document-sized and
     * neither is on an interaction path.
     */
    async prepareRestore({
      lineageId,
      versionId,
      idempotencyKey,
      current = null,
      retention,
      now = Date.now(),
      actor = "",
    }) {
      if (idempotencyKey) {
        const done = await operationForKey(idempotencyKey);
        if (done?.state === "committed") {
          return result(HISTORY_STATUS.RESTORE_COMMITTED, { operation: done });
        }
        if (done?.state === "prepared") {
          return result(HISTORY_STATUS.RESTORE_PREPARED, { operation: done });
        }
      }

      const target = await getVersionRow(versionId);
      if (!target) return result(HISTORY_STATUS.UNKNOWN_VERSION, { versionId });
      const loaded = await loadCheckpoint(target.checkpointId);
      if (!loaded.ok) return loaded;

      let preRestore = null;
      if (current?.bytes) {
        const captured = await api.captureVersion({
          ...current,
          lineageId,
          kind: VERSION_KIND.PRE_RESTORE,
          retention,
          now,
          actor,
          // The target has to survive its own restore. The prepared record does
          // not exist yet — it is written below, once there is something to
          // record — so this capture cannot learn the lease from the store and
          // is told explicitly.
          protectedIds: [versionId],
        });
        if (!captured.ok) return captured;
        preRestore = captured.version;
      }

      const head = await headOf(lineageId);
      const operation = {
        opId: newOperationId(),
        kind: "restore",
        state: "prepared",
        lineageId,
        targetVersionId: versionId,
        targetCheckpointId: target.checkpointId,
        preRestoreVersionId: preRestore?.versionId ?? null,
        expectedHead: head,
        idempotencyKey: idempotencyKey ?? "",
        actor,
        createdAt: now,
        resultVersionId: null,
      };
      const tx = db.transaction(HISTORY_OPS_STORE, "readwrite");
      tx.objectStore(HISTORY_OPS_STORE).put(operation);
      await idbCommitted(tx);
      return result(HISTORY_STATUS.RESTORE_PREPARED, { operation, bytes: loaded.bytes, target });
    },

    /**
     * Step two: append the restore as the new head, atomically.
     *
     * One transaction does all of it — compare-and-set the head against the
     * value `prepareRestore` recorded, append the restore version, mark the
     * operation committed. If anything in it fails, IndexedDB aborts the whole
     * transaction and the lineage still points at the pre-restore head, with the
     * prepared record left for `resolvePendingRestores` to finish. There is no
     * state in which the work is in neither place.
     *
     * Nothing is deleted: the target version, every version after it, and the
     * pre-restore capture all remain (`docs/140` §1.2, append never rewind).
     *
     * Idempotent: committing the same operation twice returns the first result
     * instead of restoring twice. O(versions in the lineage).
     */
    async commitRestore({ opId, now = Date.now(), retention }) {
      const tx = db.transaction(HISTORY_STORES, "readwrite");
      const ops = tx.objectStore(HISTORY_OPS_STORE);
      const documents = tx.objectStore(DOCUMENTS_STORE);
      const metas = tx.objectStore(VERSION_META_STORE);
      const blobs = tx.objectStore(CHECKPOINT_STORE);

      const operation = await idbRequest(ops.get(opId));
      if (!operation) return result(HISTORY_STATUS.UNKNOWN_OPERATION, { opId });
      if (operation.state === "committed") {
        const already = await idbRequest(metas.get(operation.resultVersionId));
        return result(HISTORY_STATUS.RESTORE_COMMITTED, { operation, version: already });
      }
      if (operation.state !== "prepared") {
        return result(HISTORY_STATUS.UNKNOWN_OPERATION, { opId, state: operation.state });
      }

      const lineage = await readDocument(documents, operation.lineageId);
      if (!lineage) return result(HISTORY_STATUS.UNKNOWN_VERSION, { lineageId: operation.lineageId });
      if ((lineage.headVersionId ?? null) !== (operation.expectedHead ?? null)) {
        return result(HISTORY_STATUS.STALE_HEAD, { head: lineage.headVersionId ?? null });
      }

      const bytes = await idbRequest(blobs.get(operation.targetCheckpointId));
      if (!bytes) return result(HISTORY_STATUS.MISSING_CHECKPOINT, { operation });
      const source = await idbRequest(metas.get(operation.targetVersionId));

      const rows = await readLineageVersions(metas, operation.lineageId);
      const guarded = new Set([operation.targetVersionId, operation.preRestoreVersionId].filter(Boolean));
      if (lineage.headVersionId) guarded.add(lineage.headVersionId);
      const plan = planRetention(rows, {
        retention,
        now,
        protectedIds: guarded,
        // The restore points at an artifact that is already stored, so it costs
        // one metadata ROW and no new bytes. Counting its bytes again would make
        // the budget refuse a restore it has room for; not counting the row at
        // all let the restore push the lineage one version past its ceiling.
        incoming: true,
        incomingBytes: 0,
      });
      if (plan.wedged) return result(plan.reason);

      const version = {
        schema: 1,
        versionId: newVersionId(),
        lineageId: operation.lineageId,
        checkpointId: operation.targetCheckpointId,
        // Carried from the row being restored, not recomputed: this version
        // points at that version's artifact, so it holds that document by
        // definition. Recomputing would need a parse of bytes this transaction is
        // only moving a pointer to.
        contentId: source?.contentId ?? "",
        words: source?.words ?? null,
        parentVersionId: lineage.headVersionId ?? null,
        restoredFromVersionId: operation.targetVersionId,
        createdAt: now,
        kind: VERSION_KIND.RESTORE,
        name: "",
        pinned: false,
        formatId: source?.formatId ?? "",
        exportMode: source?.exportMode ?? "",
        findings: source?.findings ?? 0,
        revision: null,
        bytes: bytes.length,
        engine: source?.engine ?? "",
        actor: operation.actor ?? "",
      };
      const orphaned = new Set();
      for (const id of plan.prune) {
        metas.delete(id);
        const row = rows.find((r) => r.versionId === id);
        if (row?.checkpointId && row.checkpointId !== operation.targetCheckpointId) {
          orphaned.add(row.checkpointId);
        }
      }
      metas.put(version);
      // The restore's own row is written before the reachability check, so the
      // target's artifact counts as referenced by it. Freeing here matters as
      // much as it does in `captureVersion`: an artifact nothing points at is
      // exactly the quota problem the retention policy exists to bound, and a
      // restore that pruned without freeing would leak one per restore.
      for (const id of orphaned) await freeCheckpointIfUnreferenced(metas, blobs, id);
      documents.put({ ...lineage, headVersionId: version.versionId, updatedAt: now });
      ops.put({ ...operation, state: "committed", completedAt: now, resultVersionId: version.versionId });
      await idbCommitted(tx);
      return result(HISTORY_STATUS.RESTORE_COMMITTED, { version, bytes, operation });
    },

    /** Abandons a prepared restore the caller decided not to commit — a failed
     *  parse, a cancelled confirmation. The pre-restore version it already took
     *  STAYS: it is a real point in the document's past and deleting it to tidy
     *  up would be the one deletion this module never makes. O(1). */
    async abandonRestore(opId, { now = Date.now(), reason = "" } = {}) {
      const tx = db.transaction(HISTORY_OPS_STORE, "readwrite");
      const ops = tx.objectStore(HISTORY_OPS_STORE);
      const operation = await idbRequest(ops.get(opId));
      if (!operation) return result(HISTORY_STATUS.UNKNOWN_OPERATION, { opId });
      ops.put({ ...operation, state: "abandoned", completedAt: now, reason });
      await idbCommitted(tx);
      return result(HISTORY_STATUS.PRUNED, { opId });
    },

    /**
     * Resolves prepared restores left behind by a killed tab.
     *
     * Reads which head actually committed — it never guesses from timestamps
     * (`docs/140` §9.6). Three outcomes: the head is already the restore's own
     * version, so the operation committed and the record is corrected; the head
     * is still what the operation expected, so it is resumable and reported as
     * such; or the head is neither, so somebody else moved on and the operation
     * is abandoned. O(prepared operations).
     *
     * The first outcome cannot arise from `commitRestore` in this store, because
     * that advances the head and marks the operation in ONE transaction. It is
     * kept for a record this schema did not write — a host adapter, or an older
     * version of this code — because resolving such a row by abandoning it would
     * report a completed restore as a failed one.
     */
    async resolvePendingRestores({ now = Date.now() } = {}) {
      const tx = db.transaction([HISTORY_OPS_STORE, DOCUMENTS_STORE], "readwrite");
      const ops = tx.objectStore(HISTORY_OPS_STORE);
      const documents = tx.objectStore(DOCUMENTS_STORE);
      const all = await idbRequest(ops.getAll());
      const resumable = [];
      for (const operation of all.filter((op) => op.state === "prepared")) {
        const lineage = await readDocument(documents, operation.lineageId);
        const head = lineage?.headVersionId ?? null;
        if (operation.resultVersionId && head === operation.resultVersionId) {
          ops.put({ ...operation, state: "committed", completedAt: now });
        } else if (head === (operation.expectedHead ?? null)) {
          if (now - (operation.createdAt ?? 0) >= HISTORY_LIMITS.PREPARED_OP_TTL_MS) {
            ops.put({ ...operation, state: "abandoned", completedAt: now, reason: "expired" });
          } else {
            resumable.push(operation);
          }
        } else {
          ops.put({ ...operation, state: "abandoned", completedAt: now, reason: "head_moved" });
        }
      }
      await idbCommitted(tx);
      return result(HISTORY_STATUS.RECORDED, { resumable });
    },

    /** One version row, or null. */
    getVersion: getVersionRow,

    /** The operation carrying this idempotency key, or null. */
    findOperation: operationForKey,

    /** The lineage's current head version id, or null. */
    head: headOf,

    /**
     * Everything the user needs to be told about where their history lives: how
     * much of it there is, what the browser says about the origin's quota, and
     * whether the store has been emptied underneath us.
     *
     * `evicted` is the disclosure `docs/139` §13 asks for: a browser may drop an
     * origin's IndexedDB, and history that is gone must be described as gone
     * rather than as an empty timeline. O(versions in the store).
     *
     * `lineageId` scopes the counts to ONE document. The panel used to call this
     * unscoped and print the whole browser's totals under one document's list —
     * "47 versions kept" over six rows, and "18 of 15 named" against a limit that
     * is per document — which read as a broken count to anyone who checked.
     */
    async storageStatus({ expectVersions = 0, lineageId = null } = {}) {
      const tx = db.transaction([VERSION_META_STORE, DOCUMENTS_STORE], "readonly");
      const all = await idbRequest(tx.objectStore(VERSION_META_STORE).getAll());
      const rows = lineageId ? all.filter((row) => row.lineageId === lineageId) : all;
      const lineages = await idbRequest(tx.objectStore(DOCUMENTS_STORE).count());
      let usage = null;
      let quota = null;
      try {
        const estimate = await storage?.estimate?.();
        usage = estimate?.usage ?? null;
        quota = estimate?.quota ?? null;
      } catch {
        // An origin that refuses to estimate is not an error; it is one less
        // number to show.
      }
      const bytes = rows.reduce((sum, row) => sum + (row.bytes ?? 0), 0);
      const evicted = expectVersions > 0 && rows.length === 0;
      return {
        ...result(evicted ? HISTORY_STATUS.EVICTED : HISTORY_STATUS.RECORDED),
        versions: rows.length,
        pinned: rows.filter((row) => row.pinned).length,
        lineages,
        bytes,
        usage,
        quota,
      };
    },

    /** Deletes a lineage's history, or all of it, and reports what that freed.
     *  The visible **Clear version history** control this backs is required by
     *  `docs/139` §8.7, and it reports the size for the same reason autosave's
     *  retention posture is written next to its switch. O(versions). */
    async clearHistory({ lineageId = null } = {}) {
      const tx = db.transaction(HISTORY_STORES, "readwrite");
      const metas = tx.objectStore(VERSION_META_STORE);
      const blobs = tx.objectStore(CHECKPOINT_STORE);
      const documents = tx.objectStore(DOCUMENTS_STORE);
      const ops = tx.objectStore(HISTORY_OPS_STORE);
      const rows = lineageId
        ? await readLineageVersions(metas, lineageId)
        : await idbRequest(metas.getAll());
      let freed = 0;
      for (const row of rows) {
        freed += row.bytes ?? 0;
        metas.delete(row.versionId);
      }
      const checkpoints = new Set(rows.map((row) => row.checkpointId).filter(Boolean));
      for (const id of checkpoints) await freeCheckpointIfUnreferenced(metas, blobs, id);
      if (lineageId) {
        const lineage = await readDocument(documents, lineageId);
        if (lineage) documents.put({ ...lineage, headVersionId: null });
        const all = await idbRequest(ops.getAll());
        for (const op of all.filter((o) => o.lineageId === lineageId)) ops.delete(op.opId);
      } else {
        documents.clear();
        ops.clear();
        blobs.clear();
      }
      await idbCommitted(tx);
      return result(HISTORY_STATUS.PRUNED, { versions: rows.length, freed });
    },

    close() {
      db.close();
    },
  };

  return api;
}
