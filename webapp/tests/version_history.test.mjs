// Durable version history, tested where it can be tested honestly: in node, over
// plain rows, with an injected clock and an injected IndexedDB (`docs/139`,
// `docs/140`, ADR-038; HF-068 / OO-004).
//
// The house rule for storage guards is stricter than "assert the happy path",
// because a retention test that passes while nothing was ever pruned proves
// nothing at all. So every test here CREATES THE CONDITION first — forty
// versions, rows aged past the window, a store that refuses to accept bytes, a
// transaction killed between its two writes — and only then asserts. Each one was
// driven red by mutating `version_history.mjs`; the mutations and their output are
// in the commit message.
//
// The real-IndexedDB half of this row is `tests/e2e/version-history-store.spec.mjs`,
// which runs the same store against the browser's own implementation so that the
// fake in `fake_indexeddb.mjs` is never the only thing the guards agree with.
import test from "node:test";
import assert from "node:assert/strict";

import {
  CAPTURE_REASON,
  HISTORY_LIMITS,
  HISTORY_STATUS,
  VERSION_KIND,
  VersionCapturePolicy,
  historyStatusCodes,
  historyStatusKind,
  isHistoryRefusal,
  openHistoryStore,
  planRetention,
  resolveRetention,
  sanitiseVersionName,
} from "../src/version_history.mjs";
import { DEFAULT_SETTINGS } from "../src/settings_defaults.mjs";
import { announcementRegion, needsToast } from "../src/status_policy.mjs";
import { fakeIndexedDB, quotaError } from "./fake_indexeddb.mjs";

const DAY = 24 * 60 * 60 * 1000;
const NOW = 1_800_000_000_000;

/** Rows the way the store writes them, newest first by construction. */
function versions(count, { now = NOW, spacingMs = 60_000, bytes = 1000, pinned = [] } = {}) {
  return Array.from({ length: count }, (_, i) => ({
    versionId: `v${i}`,
    lineageId: "lin-1",
    checkpointId: `sha256-${i}`,
    createdAt: now - i * spacingMs,
    kind: VERSION_KIND.AUTO,
    name: pinned.includes(`v${i}`) ? `milestone ${i}` : "",
    pinned: pinned.includes(`v${i}`),
    bytes,
  }));
}

const policy = (overrides = {}) => ({ ...resolveRetention(DEFAULT_SETTINGS), ...overrides });

// ── The owner's ruling, as configuration ─────────────────────────────────────

test("the shipped defaults are the owner's ruling: 20-30 versions, or 7 days", () => {
  const retention = resolveRetention(DEFAULT_SETTINGS);
  assert.ok(
    retention.maxCount >= 20 && retention.maxCount <= 30,
    `the count cap is ${retention.maxCount}; the ruling was "around 20-30"`,
  );
  assert.equal(retention.maxAgeMs, 7 * DAY);
  assert.equal(retention.enabled, true);
  assert.ok(retention.keepFloor >= 1 && retention.keepFloor <= retention.maxCount);
  assert.ok(retention.pinLimit < retention.maxCount, "pins must not be able to fill the store");
});

test("a hostile or stale preference cannot raise a limit past the engine ceiling", () => {
  // `settings` is `{...DEFAULT_SETTINGS, ...JSON.parse(localStorage)}`, so every
  // number here is untrusted input (`docs/140` §14).
  const wild = resolveRetention({
    versionRetentionCount: 10_000_000,
    versionRetentionDays: -4,
    versionRetentionFloor: 999,
    versionRetentionMegabytes: 1e9,
    versionIntervalMinutes: 0.0001,
    versionNamedLimit: 1e6,
  });
  assert.equal(wild.maxCount, HISTORY_LIMITS.MAX_VERSIONS);
  assert.equal(wild.maxAgeMs, 1 * DAY, "a negative window clamps to the smallest legal one");
  assert.equal(wild.keepFloor, wild.maxCount, "the floor can never exceed the ceiling");
  assert.equal(wild.maxBytes, HISTORY_LIMITS.MAX_BYTES);
  assert.equal(wild.intervalMs, HISTORY_LIMITS.MIN_INTERVAL_MS);
  assert.equal(wild.pinLimit, HISTORY_LIMITS.MAX_PINNED);

  const nonsense = resolveRetention({
    versionRetentionCount: "many",
    versionIntervalMinutes: "soon",
  });
  assert.equal(nonsense.maxCount, 25, "an unparseable preference falls back to the default");
  assert.equal(nonsense.intervalMs, 10 * 60_000);
  // A parseable-but-absurd one clamps rather than falling back: zero minutes is
  // a number, and the smallest legal cadence is the honest reading of it.
  assert.equal(resolveRetention({ versionIntervalMinutes: 0 }).intervalMs, 60_000);
});

// ── Retention: what happens when count and age disagree ──────────────────────

test("the age window prunes nothing inside it, however many versions there are", () => {
  // Forty versions inside seven days: age says keep them all, and the count
  // ceiling is what has to act. This is half of the disagreement the owner's
  // ruling left open.
  const rows = versions(40, { spacingMs: 60_000 });
  const plan = planRetention(rows, { retention: policy(), now: NOW, incomingBytes: 1000 });
  assert.equal(plan.wedged, false);
  // 40 rows + 1 incoming must come down to the cap of 25.
  assert.equal(plan.prune.length, 16, `pruned ${plan.prune.length}`);
  const pruned = new Set(plan.prune);
  assert.ok(pruned.has("v39"), "the oldest goes first");
  assert.ok(!pruned.has("v0"), "the newest never goes to a count sweep");
  for (const id of plan.prune) {
    const row = rows.find((r) => r.versionId === id);
    assert.ok(NOW - row.createdAt < 7 * DAY, "these were all inside the age window");
  }
});

test("age alone never empties a timeline: the floor survives the window", () => {
  // Five versions, every one of them older than a week — the other half of the
  // disagreement. If age pruned on its own, a document nobody touched for eight
  // days would lose its entire past, which is not what "retain for 7 days"
  // means to anybody.
  const rows = versions(5, { now: NOW - 30 * DAY, spacingMs: DAY });
  const plan = planRetention(rows, { retention: policy(), now: NOW });
  assert.equal(plan.wedged, false);
  const kept = rows.filter((row) => !plan.prune.includes(row.versionId)).map((r) => r.versionId);
  assert.deepEqual(kept, ["v0", "v1", "v2"], "the newest keepFloor survive any age");
  assert.deepEqual(plan.prune.sort(), ["v3", "v4"]);
});

test("the floor is a floor, not a cap: fresh versions above it are untouched", () => {
  const rows = [
    ...versions(3, { now: NOW, spacingMs: 60_000 }),
    ...versions(3, { now: NOW - 40 * DAY, spacingMs: DAY }).map((row, i) => ({
      ...row,
      versionId: `old${i}`,
    })),
  ];
  const plan = planRetention(rows, { retention: policy(), now: NOW });
  assert.deepEqual(plan.prune.sort(), ["old0", "old1", "old2"], "only the stale ones go");
});

test("a named version is never pruned — not by age, not by the count ceiling", () => {
  // The condition, created: forty versions, ten of them named, every one of them
  // a month old. Both rules fire at once and neither may touch a name.
  const rows = versions(40, {
    now: NOW - 30 * DAY,
    spacingMs: DAY,
    pinned: ["v5", "v6", "v7", "v8", "v9", "v10", "v11", "v12", "v13", "v14"],
  });
  const plan = planRetention(rows, { retention: policy(), now: NOW, incomingBytes: 1000 });
  const pruned = new Set(plan.prune);
  for (const row of rows.filter((r) => r.pinned)) {
    assert.ok(
      !pruned.has(row.versionId),
      `${row.versionId} is named "${row.name}" and was pruned; naming would be a lie`,
    );
  }
  const survivors = rows.filter((r) => !pruned.has(r.versionId)).map((r) => r.versionId);
  assert.deepEqual(
    survivors.sort(),
    ["v0", "v1", "v10", "v11", "v12", "v13", "v14", "v2", "v5", "v6", "v7", "v8", "v9"],
    "what survives is exactly the ten named versions plus the three-version floor — " +
      "the age window took everything else, and the count ceiling then had nothing to do",
  );
  assert.ok(survivors.length + 1 <= policy().maxCount, "and the ceiling is satisfied");
});

test("when only named versions remain, the capture is REFUSED rather than a pin deleted", () => {
  // Every row pinned and the cap already full: there is no legal prune, so the
  // only two options are to break a promise or to refuse. It refuses, and says
  // which wall it hit.
  const pinned = Array.from({ length: 25 }, (_, i) => `v${i}`);
  const rows = versions(25, { pinned });
  const plan = planRetention(rows, { retention: policy(), now: NOW, incomingBytes: 1000 });
  assert.equal(plan.wedged, true);
  assert.equal(plan.reason, HISTORY_STATUS.FULL_PINNED);
  assert.deepEqual(plan.prune, [], "nothing pinned was sacrificed on the way to refusing");
  assert.equal(historyStatusKind(plan.reason), "error", "a refusal must reach the user as one");
});

test("an artifact larger than the whole byte budget is refused as over-budget", () => {
  const retention = policy({ maxBytes: 5_000_000 });
  const plan = planRetention(versions(2), { retention, now: NOW, incomingBytes: 9_000_000 });
  assert.equal(plan.wedged, true);
  assert.equal(
    plan.reason,
    HISTORY_STATUS.OVER_BUDGET,
    "pruning every other version cannot make room for this one, and the way out " +
      "is a bigger budget rather than fewer versions — so it is its own sentence",
  );
});

test("the byte ceiling prunes oldest-first before it gives up", () => {
  const retention = policy({ maxBytes: 5_000, maxCount: 200 });
  const rows = versions(5, { bytes: 1_000 });
  const plan = planRetention(rows, { retention, now: NOW, incomingBytes: 2_000 });
  assert.equal(plan.wedged, false);
  assert.deepEqual(plan.prune.sort(), ["v3", "v4"], "two oldest released to fit 2 KB");
});

test("the head and anything a preview or a restore is holding is never pruned", () => {
  const rows = versions(40, { now: NOW - 30 * DAY, spacingMs: DAY });
  const plan = planRetention(rows, {
    retention: policy(),
    now: NOW,
    protectedIds: new Set(["v20", "v39"]),
  });
  assert.ok(!plan.prune.includes("v20"));
  assert.ok(!plan.prune.includes("v39"), "even the oldest row is safe while something holds it");
});

// ── Naming ───────────────────────────────────────────────────────────────────

test("a version name is trimmed, single-lined and bounded; duplicates are legal", () => {
  assert.equal(sanitiseVersionName("  Before   the   rewrite \n").name, "Before the rewrite");
  // Built with `String.fromCharCode` rather than written literally: a raw
  // control byte in a source file makes git treat it as binary
  // (`tests/source_bytes.test.mjs`).
  const control = String.fromCharCode(0) + String.fromCharCode(7);
  assert.equal(sanitiseVersionName(`a${control}b`).name, "a b");
  assert.equal(sanitiseVersionName("").ok, false);
  assert.equal(sanitiseVersionName("   ").ok, false);
  assert.equal(sanitiseVersionName(null).ok, false);
  assert.equal(sanitiseVersionName("x".repeat(HISTORY_LIMITS.MAX_NAME_CHARS)).ok, true);
  assert.equal(sanitiseVersionName("x".repeat(HISTORY_LIMITS.MAX_NAME_CHARS + 1)).ok, false);
  // Identity is the VersionId, so the same label twice is allowed on purpose
  // (`docs/139` §8.3) — the panel separates them by time and author.
  assert.equal(sanitiseVersionName("v1").name, sanitiseVersionName(" v1 ").name);
});

// ── The capture trigger, and the O(1) claim ──────────────────────────────────

test("an autosave tick does not capture a version until the interval has passed", () => {
  const capture = new VersionCapturePolicy({ intervalMs: 10 * 60_000 });
  const first = capture.shouldCapture({ reason: CAPTURE_REASON.QUIESCE, revision: 1, now: NOW });
  assert.equal(first.capture, true, "the first edit session must be recorded");
  capture.noteCaptured(NOW, 1);

  // Autosave fires every five seconds of quiet. A version every five seconds
  // would spend the whole 25-version budget in two minutes.
  for (let tick = 1; tick <= 100; tick++) {
    const at = NOW + tick * 5_000;
    const decision = capture.shouldCapture({
      reason: CAPTURE_REASON.QUIESCE,
      revision: 1 + tick,
      now: at,
    });
    if (at - NOW < 10 * 60_000) {
      assert.equal(decision.capture, false, `captured at ${(at - NOW) / 1000}s`);
      assert.equal(decision.why, HISTORY_STATUS.NOT_DUE);
    } else {
      assert.equal(decision.capture, true, `still refusing at ${(at - NOW) / 1000}s`);
      assert.equal(decision.kind, VERSION_KIND.AUTO);
      break;
    }
  }
});

test("nothing changed means no version, and an explicit Save always means one", () => {
  const capture = new VersionCapturePolicy({ intervalMs: 60_000 });
  capture.noteCaptured(NOW, 7);
  assert.deepEqual(
    capture.shouldCapture({ reason: CAPTURE_REASON.QUIESCE, revision: 7, now: NOW + DAY }),
    { capture: false, kind: null, why: HISTORY_STATUS.UNCHANGED },
    "the engine revision watermark is the authority on whether anything changed",
  );
  // Save, naming, open, restore: points a person recognises. `docs/139` §18
  // question 3 asked whether Save always makes a version; ADR-038 says yes, and
  // content-addressed checkpoints are why it is cheap.
  for (const [reason, kind] of [
    [CAPTURE_REASON.SAVE, VERSION_KIND.SAVED],
    [CAPTURE_REASON.NAME, VERSION_KIND.NAMED],
    [CAPTURE_REASON.OPEN, VERSION_KIND.IMPORT],
    [CAPTURE_REASON.MANUAL, VERSION_KIND.MANUAL],
    [CAPTURE_REASON.PRE_RESTORE, VERSION_KIND.PRE_RESTORE],
  ]) {
    const decision = capture.shouldCapture({ reason, revision: 7, now: NOW + 1 });
    assert.equal(decision.capture, true, `${reason} must always produce a version`);
    assert.equal(decision.kind, kind);
  }
  assert.equal(
    capture.shouldCapture({ reason: CAPTURE_REASON.QUIESCE, revision: 9, now: NOW, enabled: false })
      .capture,
    false,
    "the switch is honoured below the UI",
  );
});

test("the capture decision is O(1) in the number of stored versions", async () => {
  // The claim under test: autosave does not become O(versions). Measured as
  // STORE REQUESTS, not as milliseconds — a timing threshold cannot tell a slow
  // constant from a linear walk, and a doubling test can (SKILL §8).
  const { store, db, lineageId } = await seeded(3);
  const capture = new VersionCapturePolicy({ intervalMs: 10 * 60_000 });
  capture.noteCaptured(NOW, 1);

  const before = db.requests;
  for (let tick = 1; tick <= 50; tick++) {
    capture.shouldCapture({ reason: CAPTURE_REASON.QUIESCE, revision: 1 + tick, now: NOW + tick });
  }
  assert.equal(db.requests - before, 0, "a tick that does not capture must not touch the store");

  // A capture at the cap prunes exactly one version, whether the store has seen
  // thirty or three hundred: compare two stores that reached the cap by different
  // routes and the request count must not differ.
  const thirty = await seeded(30);
  const sixty = await seeded(60);
  const small = await measureCapture(thirty.store, thirty.lineageId, thirty.db);
  const large = await measureCapture(sixty.store, sixty.lineageId, sixty.db);
  assert.equal(
    small,
    large,
    `a capture cost ${small} requests after 30 versions and ${large} after 60; the number ` +
      "of store requests must not grow with the length of the timeline",
  );
  assert.ok(small <= 12, `a capture issued ${small} store requests; it must be a small constant`);
  store.close();
  thirty.store.close();
  sixty.store.close();
});

// ── The store ────────────────────────────────────────────────────────────────

/** A store with `count` versions already in it, one lineage. */
async function seeded(count, { retention = policy(), bytesEach = 2048 } = {}) {
  const indexedDB = fakeIndexedDB();
  const store = await openHistoryStore({ indexedDB, name: "opendoc-drafts", subtle: null });
  const { lineageId } = await store.openLineage({ docKey: "k1", name: "report.docx", now: NOW });
  for (let i = 0; i < count; i++) {
    const bytes = new Uint8Array(bytesEach).fill(i % 251);
    const outcome = await store.captureVersion({
      lineageId,
      bytes,
      formatId: "org.openxmlformats.wordprocessingml.document",
      exportMode: "preserve_when_safe",
      revision: i + 1,
      now: NOW + i * 60_000,
      retention,
      kind: VERSION_KIND.AUTO,
    });
    assert.equal(outcome.ok, true, `seeding version ${i}: ${outcome.status}`);
  }
  return { store, indexedDB, db: indexedDB.databases.get("opendoc-drafts"), lineageId, retention };
}

async function measureCapture(store, lineageId, db) {
  const before = db.requests;
  const outcome = await store.captureVersion({
    lineageId,
    bytes: new Uint8Array(1024).fill(9),
    formatId: "docx",
    revision: 9_999,
    now: NOW + 10 * 60_000,
    retention: policy(),
  });
  assert.equal(outcome.ok, true, outcome.status);
  return db.requests - before;
}

test("a version is metadata plus one immutable artifact, and identical bytes are stored once", async () => {
  const { store, db, lineageId } = await seeded(0);
  const bytes = new Uint8Array(4096).fill(3);
  const first = await store.captureVersion({
    lineageId,
    bytes,
    formatId: "docx",
    exportMode: "preserve_when_safe",
    revision: 4,
    now: NOW,
    retention: policy(),
    kind: VERSION_KIND.SAVED,
  });
  assert.equal(first.ok, true);
  assert.equal(first.version.bytes, 4096);
  assert.equal(first.version.kind, VERSION_KIND.SAVED);
  assert.equal(await store.head(lineageId), first.version.versionId);

  // Name this version, immediately after a Save: the same bytes, a second row.
  const second = await store.captureVersion({
    lineageId,
    bytes,
    formatId: "docx",
    revision: 4,
    now: NOW + 1000,
    retention: policy(),
    kind: VERSION_KIND.NAMED,
    name: "Before the rewrite",
  });
  assert.equal(second.ok, true);
  assert.equal(second.version.pinned, true, "naming pins");
  assert.equal(
    second.version.checkpointId,
    first.version.checkpointId,
    "content-addressed: identical artifacts share one key",
  );
  assert.equal(
    db.stores.get("checkpoint_blobs").records.size,
    1,
    "two versions of identical bytes must not store the document twice",
  );
  assert.equal(db.stores.get("version_meta").records.size, 2);
  const listed = await store.listVersions(lineageId);
  assert.deepEqual(
    listed.map((row) => row.versionId),
    [second.version.versionId, first.version.versionId],
    "newest first",
  );
  store.close();
});

test("the retention policy is enforced by the store, not only by the planner", async () => {
  const { store, db, lineageId } = await seeded(40);
  const rows = await store.listVersions(lineageId);
  assert.equal(rows.length, policy().maxCount, `${rows.length} versions survived a 40-version run`);
  assert.equal(
    db.stores.get("checkpoint_blobs").records.size,
    policy().maxCount,
    "a pruned version's artifact is released with it — an orphan blob is the whole quota problem",
  );
  const oldest = rows[rows.length - 1];
  assert.ok(oldest.revision > 1, "the earliest versions are the ones that went");
  store.close();
});

test("a named version survives a sweep that removes everything around it", async () => {
  const { store, lineageId } = await seeded(5);
  const rows = await store.listVersions(lineageId);
  const keeper = rows[rows.length - 1]; // the oldest, which a sweep would take first
  const named = await store.nameVersion(keeper.versionId, "Chapter 3 rewrite", {
    pinLimit: policy().pinLimit,
  });
  assert.equal(named.ok, true);

  // Create the condition: every row is now older than the window.
  const later = NOW + 40 * DAY;
  const swept = await store.sweep({ lineageId, retention: policy(), now: later });
  assert.equal(swept.ok, true);
  const after = await store.listVersions(lineageId);
  const survivor = after.find((row) => row.versionId === keeper.versionId);
  assert.ok(survivor, "the named version was pruned by age; naming must outlast the window");
  assert.equal(survivor.name, "Chapter 3 rewrite");
  assert.equal(
    after.length,
    policy().keepFloor + 1,
    "the floor kept three, and the name kept a fourth",
  );
  store.close();
});

test("the pin limit refuses a new name instead of silently unpinning something", async () => {
  const { store, lineageId } = await seeded(6);
  const rows = await store.listVersions(lineageId);
  for (const row of rows.slice(0, 3)) {
    assert.equal((await store.nameVersion(row.versionId, `m${row.versionId}`, { pinLimit: 3 })).ok, true);
  }
  const refused = await store.nameVersion(rows[4].versionId, "one too many", { pinLimit: 3 });
  assert.equal(refused.ok, false);
  assert.equal(refused.status, HISTORY_STATUS.PIN_LIMIT);
  assert.equal(refused.kind, "error");
  // Renaming an existing name is not a new pin.
  assert.equal((await store.nameVersion(rows[0].versionId, "renamed", { pinLimit: 3 })).ok, true);
  store.close();
});

test("running out of storage is reported, and nothing half-written is left behind", async () => {
  const { store, db, lineageId } = await seeded(2);
  const blobsBefore = db.stores.get("checkpoint_blobs").records.size;
  const metasBefore = db.stores.get("version_meta").records.size;
  const headBefore = await store.head(lineageId);

  // The condition: the browser refuses the artifact. Chromium, Firefox and
  // Safari all surface this as a QuotaExceededError.
  db.failures = {
    beforePut(storeName) {
      if (storeName === "checkpoint_blobs") throw quotaError();
    },
  };
  const refused = await store.captureVersion({
    lineageId,
    bytes: new Uint8Array(8192).fill(1),
    formatId: "docx",
    revision: 77,
    now: NOW + 60_000,
    retention: policy(),
  });
  assert.equal(refused.ok, false);
  assert.equal(refused.status, HISTORY_STATUS.QUOTA_EXHAUSTED);
  assert.equal(refused.kind, "error", "silence about unrecorded work is the defect");
  assert.equal(db.stores.get("version_meta").records.size, metasBefore, "no orphan metadata row");
  assert.equal(db.stores.get("checkpoint_blobs").records.size, blobsBefore);
  assert.equal(await store.head(lineageId), headBefore, "the head did not move");
  store.close();
});

test("a refusal reaches the user through the real status channel, not a whisper", () => {
  // The module returns codes; `status_channel.mjs` decides how a message is
  // delivered from its KIND. So the guard runs the code through the real policy
  // rather than asserting a string.
  for (const code of historyStatusCodes()) {
    const kind = historyStatusKind(code);
    if (isHistoryRefusal(code)) {
      assert.equal(kind, "error", `${code} is a refusal and must be published as an error`);
      assert.equal(announcementRegion(kind), "assertive", `${code} must interrupt`);
      assert.equal(
        needsToast(kind, true),
        true,
        `${code} must show a toast even while the footer is visible — the draft pill is ` +
          "the first thing a narrow window sheds",
      );
    } else {
      assert.equal(kind, "", `${code} is informational`);
      assert.equal(announcementRegion(kind), "polite");
    }
  }
});

test("every status code is classified as exactly one of refusal or informational", () => {
  // Without this, a code added later defaults to a polite whisper and nobody
  // finds out until a user loses work quietly.
  const codes = historyStatusCodes();
  assert.ok(codes.length >= 18, `only ${codes.length} codes found — the scan is looking wrongly`);
  for (const code of codes) {
    const refusal = isHistoryRefusal(code);
    const informational = historyStatusKind(code) === "";
    assert.notEqual(
      refusal,
      informational,
      `${code} is in both sets or in neither; classify it deliberately`,
    );
  }
});

// ── Restore ──────────────────────────────────────────────────────────────────

/** A store holding three versions, with `target` an older one worth restoring. */
async function restorable() {
  const seed = await seeded(3);
  const rows = await seed.store.listVersions(seed.lineageId);
  return { ...seed, rows, target: rows[rows.length - 1] };
}

test("a restore appends a new head and keeps everything it restored from", async () => {
  const { store, lineageId, target, rows } = await restorable();
  const headBefore = await store.head(lineageId);
  const current = {
    bytes: new Uint8Array(3000).fill(200),
    formatId: "docx",
    revision: 501,
    exportMode: "preserve_when_safe",
  };

  const prepared = await store.prepareRestore({
    lineageId,
    versionId: target.versionId,
    idempotencyKey: "restore-1",
    current,
    retention: policy(),
    now: NOW + 60 * 60_000,
  });
  assert.equal(prepared.ok, true, prepared.status);
  assert.equal(prepared.status, HISTORY_STATUS.RESTORE_PREPARED);
  assert.ok(prepared.bytes, "prepare returns the validated bytes so the caller can open them");
  assert.notEqual(
    await store.head(lineageId),
    headBefore,
    "the current work became a version BEFORE the head moved — that is the invariant",
  );
  const preRestore = await store.getVersion(prepared.operation.preRestoreVersionId);
  assert.equal(preRestore.kind, VERSION_KIND.PRE_RESTORE);
  assert.equal(preRestore.bytes, 3000);

  const committed = await store.commitRestore({
    opId: prepared.operation.opId,
    retention: policy(),
    now: NOW + 61 * 60_000,
  });
  assert.equal(committed.ok, true, committed.status);
  assert.equal(committed.version.kind, VERSION_KIND.RESTORE);
  assert.equal(committed.version.restoredFromVersionId, target.versionId);
  assert.equal(await store.head(lineageId), committed.version.versionId);
  assert.deepEqual(
    [...committed.bytes.slice(0, 4)],
    [...(await store.readCheckpoint(target.checkpointId)).bytes.slice(0, 4)],
    "the bytes handed back are the target's own",
  );

  const after = await store.listVersions(lineageId);
  for (const row of rows) {
    assert.ok(
      after.some((r) => r.versionId === row.versionId),
      `${row.versionId} disappeared; a restore appends and never rewinds`,
    );
  }
  assert.ok(after.some((r) => r.versionId === preRestore.versionId), "the pre-restore state stays");
  store.close();
});

test("a restore killed between its two steps leaves the old head and the work", async () => {
  const { store, db, lineageId, target } = await restorable();
  const prepared = await store.prepareRestore({
    lineageId,
    versionId: target.versionId,
    idempotencyKey: "restore-crash",
    current: { bytes: new Uint8Array(1500).fill(42), formatId: "docx", revision: 900 },
    retention: policy(),
    now: NOW + 60 * 60_000,
  });
  assert.equal(prepared.ok, true);
  const headAfterPrepare = await store.head(lineageId);
  const metasAfterPrepare = db.stores.get("version_meta").records.size;

  // The condition: the commit transaction dies after some of its writes. A
  // browser does this when the tab is killed; here the metadata put throws.
  db.failures = {
    beforePut(storeName, value) {
      if (storeName === "version_meta" && value?.kind === VERSION_KIND.RESTORE) {
        throw new Error("tab killed mid-commit");
      }
    },
  };
  const failed = await store.commitRestore({
    opId: prepared.operation.opId,
    retention: policy(),
    now: NOW + 61 * 60_000,
  }).catch((err) => ({ ok: false, thrown: String(err) }));
  assert.equal(failed.ok, false, "a commit that could not write must not report success");

  assert.equal(await store.head(lineageId), headAfterPrepare, "the head is still the old one");
  assert.equal(
    db.stores.get("version_meta").records.size,
    metasAfterPrepare,
    "no partial restore row survived the aborted transaction",
  );
  const preRestore = await store.getVersion(prepared.operation.preRestoreVersionId);
  assert.ok(preRestore, "the user's pre-restore work is still a version — neither place is empty");

  // And it is recoverable rather than guessed at.
  db.failures = null;
  const pending = await store.resolvePendingRestores({ now: NOW + 62 * 60_000 });
  assert.deepEqual(
    pending.resumable.map((op) => op.opId),
    [prepared.operation.opId],
    "a prepared record whose head still matches is resumable, not abandoned",
  );
  const retried = await store.commitRestore({
    opId: prepared.operation.opId,
    retention: policy(),
    now: NOW + 63 * 60_000,
  });
  assert.equal(retried.ok, true, retried.status);
  assert.equal(await store.head(lineageId), retried.version.versionId);
  store.close();
});

test("committing the same restore twice restores once", async () => {
  const { store, lineageId, target } = await restorable();
  const prepared = await store.prepareRestore({
    lineageId,
    versionId: target.versionId,
    idempotencyKey: "restore-idem",
    current: { bytes: new Uint8Array(1200).fill(5), formatId: "docx", revision: 31 },
    retention: policy(),
    now: NOW + DAY,
  });
  const first = await store.commitRestore({ opId: prepared.operation.opId, retention: policy() });
  const before = (await store.listVersions(lineageId)).length;
  const again = await store.commitRestore({ opId: prepared.operation.opId, retention: policy() });
  assert.equal(again.status, HISTORY_STATUS.RESTORE_COMMITTED);
  assert.equal(again.version.versionId, first.version.versionId);
  assert.equal((await store.listVersions(lineageId)).length, before, "no second restore row");

  // And asking to prepare the same restore again returns the finished operation
  // rather than taking a second pre-restore checkpoint.
  const repeat = await store.prepareRestore({
    lineageId,
    versionId: target.versionId,
    idempotencyKey: "restore-idem",
    current: { bytes: new Uint8Array(1200).fill(5), formatId: "docx", revision: 31 },
    retention: policy(),
  });
  assert.equal(repeat.status, HISTORY_STATUS.RESTORE_COMMITTED);
  assert.equal((await store.listVersions(lineageId)).length, before);
  store.close();
});

test("a restore whose head moved underneath it is refused, not applied", async () => {
  const { store, lineageId, target } = await restorable();
  const prepared = await store.prepareRestore({
    lineageId,
    versionId: target.versionId,
    idempotencyKey: "restore-stale",
    current: { bytes: new Uint8Array(900).fill(7), formatId: "docx", revision: 12 },
    retention: policy(),
    now: NOW + DAY,
  });
  // Another tab records a version in the meantime.
  const intruder = await store.captureVersion({
    lineageId,
    bytes: new Uint8Array(950).fill(8),
    formatId: "docx",
    revision: 13,
    now: NOW + DAY + 1000,
    retention: policy(),
  });
  assert.equal(intruder.ok, true);

  const refused = await store.commitRestore({ opId: prepared.operation.opId, retention: policy() });
  assert.equal(refused.ok, false);
  assert.equal(refused.status, HISTORY_STATUS.STALE_HEAD);
  assert.equal(await store.head(lineageId), intruder.version.versionId, "the other tab's head stands");
  store.close();
});

test("a restore is refused outright when the current work cannot be checkpointed", async () => {
  const { store, db, lineageId, target } = await restorable();
  const headBefore = await store.head(lineageId);
  db.failures = {
    beforePut(storeName) {
      if (storeName === "checkpoint_blobs") throw quotaError();
    },
  };
  const refused = await store.prepareRestore({
    lineageId,
    versionId: target.versionId,
    idempotencyKey: "restore-noroom",
    current: { bytes: new Uint8Array(4000).fill(6), formatId: "docx", revision: 88 },
    retention: policy(),
    now: NOW + DAY,
  });
  assert.equal(refused.ok, false);
  assert.equal(refused.status, HISTORY_STATUS.QUOTA_EXHAUSTED);
  assert.equal(await store.head(lineageId), headBefore);
  assert.equal(
    (await store.findOperation("restore-noroom")) ?? null,
    null,
    "no prepared record for a restore that was never safe to start",
  );
  store.close();
});

test("a corrupt or missing checkpoint is reported, never opened", async () => {
  const { store, db, lineageId, target } = await restorable();
  // The condition: the artifact on disk is not what its key claims. A browser
  // does this by evicting or truncating; here the bytes are replaced.
  db.stores.get("checkpoint_blobs").records.set(target.checkpointId, new Uint8Array([1, 2, 3]));
  await assert.rejects(
    () => openHistoryStore({ indexedDB: null }),
    /no IndexedDB/,
    "a store that cannot be opened rejects rather than pretending to have opened",
  );

  const read = await store.readCheckpoint(target.checkpointId);
  assert.equal(read.ok, false);
  assert.equal(read.status, HISTORY_STATUS.CORRUPT_CHECKPOINT);
  const prepared = await store.prepareRestore({
    lineageId,
    versionId: target.versionId,
    idempotencyKey: "restore-corrupt",
    current: { bytes: new Uint8Array(100).fill(1), formatId: "docx", revision: 3 },
    retention: policy(),
  });
  assert.equal(prepared.ok, false);
  assert.equal(prepared.status, HISTORY_STATUS.CORRUPT_CHECKPOINT);
  store.close();
});

test("a restore at the cap leaves no artifact that nothing points at", async () => {
  // The condition: a full store, so both the pre-restore capture and the restore
  // itself have to prune to fit. An artifact left behind by a prune is invisible
  // — it costs quota, no row mentions it, and nothing would ever free it.
  const { store, db, lineageId } = await seeded(policy().maxCount);
  const rows = await store.listVersions(lineageId);
  const target = rows[rows.length - 1];
  const prepared = await store.prepareRestore({
    lineageId,
    versionId: target.versionId,
    idempotencyKey: "restore-at-cap",
    current: { bytes: new Uint8Array(2048).fill(200), formatId: "docx", revision: 900 },
    retention: policy(),
    now: NOW + 60 * 60_000,
  });
  assert.equal(prepared.ok, true, prepared.status);
  const committed = await store.commitRestore({
    opId: prepared.operation.opId,
    retention: policy(),
    now: NOW + 61 * 60_000,
  });
  assert.equal(committed.ok, true, committed.status);

  const after = await store.listVersions(lineageId);
  assert.equal(after.length, policy().maxCount, "the ceiling still holds after a restore");
  const referenced = new Set(after.map((row) => row.checkpointId));
  const stored = new Set(db.stores.get("checkpoint_blobs").records.keys());
  assert.deepEqual(
    [...stored].filter((id) => !referenced.has(id)),
    [],
    "an artifact no version references is quota nobody can account for",
  );
  assert.deepEqual(
    [...referenced].filter((id) => !stored.has(id)),
    [],
    "and a version whose artifact is gone is a row the restore path cannot load",
  );
  store.close();
});

// ── Housekeeping the user can see ────────────────────────────────────────────

test("the head cannot be deleted, and an artifact goes only at reference count zero", async () => {
  const { store, db, lineageId } = await seeded(3);
  const rows = await store.listVersions(lineageId);
  const head = rows[0];
  const refused = await store.deleteVersion(head.versionId);
  assert.equal(refused.ok, false, "deleting the head would leave the document with no past at all");

  // Two versions sharing one artifact: deleting one must not take the bytes the
  // other still needs.
  const shared = await store.captureVersion({
    lineageId,
    bytes: new Uint8Array(2048).fill(1 % 251),
    formatId: "docx",
    revision: 4242,
    now: NOW + 5 * 60_000,
    retention: policy(),
  });
  assert.equal(shared.ok, true);
  const sharing = (await store.listVersions(lineageId)).filter(
    (row) => row.checkpointId === shared.version.checkpointId,
  );
  assert.equal(sharing.length, 2, "the seed wrote these bytes once already");
  const blobs = db.stores.get("checkpoint_blobs").records.size;
  const gone = await store.deleteVersion(sharing[1].versionId);
  assert.equal(gone.ok, true);
  assert.equal(gone.freedCheckpoint, false);
  assert.equal(db.stores.get("checkpoint_blobs").records.size, blobs, "still referenced");
  store.close();
});

test("clearing history says how much it freed, and reports an eviction as an eviction", async () => {
  const { store, lineageId } = await seeded(4);
  const status = await store.storageStatus();
  assert.equal(status.versions, 4);
  assert.ok(status.bytes >= 4 * 2048);

  const cleared = await store.clearHistory({ lineageId });
  assert.equal(cleared.versions, 4);
  assert.ok(cleared.freed >= 4 * 2048, "the user is told what clearing bought them");
  assert.equal(await store.head(lineageId), null);

  // The browser dropping the store is not an empty timeline; it is a loss, and
  // `docs/139` §13 requires it be disclosed.
  const evicted = await store.storageStatus({ expectVersions: 4 });
  assert.equal(evicted.status, HISTORY_STATUS.EVICTED);
  assert.equal(evicted.kind, "error");
  store.close();
});

test("reopening the same document rejoins its lineage; a copy gets its own", async () => {
  const { store, lineageId } = await seeded(2);
  const again = await store.openLineage({ docKey: "k1", name: "report.docx", now: NOW + DAY });
  assert.equal(again.lineageId, lineageId, "the same file must find its own history back");
  assert.equal(again.versions.length, 2);
  const renamed = await store.openLineage({ docKey: "k1", name: "report-copy.docx", now: NOW + DAY });
  assert.notEqual(renamed.lineageId, lineageId, "a different name is a different lineage");
  const copy = await store.openLineage({ docKey: "k1", name: "report.docx", fresh: true });
  assert.notEqual(copy.lineageId, lineageId, "Make a copy mints a new lineage explicitly");
  store.close();
});
