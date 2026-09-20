// The autosave policy, tested where it can be tested honestly: in node, over
// plain data, with an injected clock and injected timers (docs/112 §6, HF-011).
//
// The e2e side of this row — that a draft written before a renderer CRASH comes
// back afterwards — is `tests/e2e/draft-recovery.spec.mjs`. These are the rules
// that a browser test cannot drive to their edges without waiting real minutes:
// the 60-second ceiling, the 24-hour age gate, and the cross-tab lease.
import test from "node:test";
import assert from "node:assert/strict";

import {
  CEILING_MS,
  DRAFT_TTL_MS,
  DraftPresence,
  DraftScheduler,
  LIVE_SLOT_MS,
  MAX_DRAFT_SLOTS,
  QUIESCE_MS,
  describeDraftAge,
  describeDraftSize,
  documentKey,
  draftFormatFor,
  draftSlotId,
  evictableSlots,
  offerableDrafts,
  slotIsLive,
} from "../src/drafts.mjs";

/** A clock plus timer table the scheduler can be driven through by hand. */
function fakeTimers() {
  let now = 1_000_000;
  let nextHandle = 1;
  const pending = new Map();
  return {
    now: () => now,
    setTimer(fn, ms) {
      const handle = nextHandle++;
      pending.set(handle, { at: now + ms, fn });
      return handle;
    },
    clearTimer(handle) {
      pending.delete(handle);
    },
    /** Advances the clock, firing every timer whose deadline passes. */
    advance(ms) {
      const target = now + ms;
      for (;;) {
        const due = [...pending.entries()]
          .filter(([, timer]) => timer.at <= target)
          .sort((a, b) => a[1].at - b[1].at)[0];
        if (!due) break;
        const [handle, timer] = due;
        pending.delete(handle);
        now = timer.at;
        timer.fn();
      }
      now = target;
    },
    get outstanding() {
      return pending.size;
    },
  };
}

/** Two ends of a fake `BroadcastChannel`: what either posts, the other hears.
 *  Delivery is synchronous, which is why the presence tests can use a no-op
 *  wait — the protocol is what is under test, not the timing. */
function fakeChannelPair() {
  const ends = [];
  const make = () => {
    const end = {
      onmessage: null,
      postMessage(data) {
        for (const other of ends) {
          if (other !== end) other.onmessage?.({ data });
        }
      },
      close() {
        const at = ends.indexOf(end);
        if (at >= 0) ends.splice(at, 1);
      },
    };
    ends.push(end);
    return end;
  };
  return [make(), make()];
}

function scheduler(timers, writes) {
  return new DraftScheduler({
    write: (reason) => writes.push({ reason, at: timers.now() }),
    now: timers.now,
    setTimer: timers.setTimer,
    clearTimer: timers.clearTimer,
  });
}

test("a keystroke schedules a write instead of performing one", () => {
  const timers = fakeTimers();
  const writes = [];
  const s = scheduler(timers, writes);

  s.noteDirty();
  assert.deepEqual(writes, [], "noteDirty must not snapshot; it must arm a timer");

  timers.advance(QUIESCE_MS - 1);
  assert.deepEqual(writes, [], "the quiesce window had not elapsed");

  timers.advance(1);
  assert.equal(writes.length, 1);
  assert.equal(writes[0].reason, "quiesce");
});

test("continuous typing pushes the quiesce window, but the ceiling still fires", () => {
  const timers = fakeTimers();
  const writes = [];
  const s = scheduler(timers, writes);

  // A keystroke every 2s for 50s: the 5s quiesce timer is re-armed every time
  // and never expires. Without the ceiling, nothing would reach disk at all.
  for (let elapsed = 0; elapsed < CEILING_MS - 2 * QUIESCE_MS; elapsed += 2_000) {
    s.noteDirty();
    timers.advance(2_000);
  }
  assert.deepEqual(
    writes.map((w) => w.reason),
    [],
    "quiesce cannot fire while the typist keeps typing — that is what the ceiling is for",
  );

  // Keep typing straight through the ceiling.
  for (let elapsed = 0; elapsed < 3 * QUIESCE_MS; elapsed += 2_000) {
    s.noteDirty();
    timers.advance(2_000);
  }
  assert.deepEqual(
    writes.map((w) => w.reason),
    ["ceiling"],
    "the ceiling must bound how stale the on-disk draft can get while a typist " +
      "never pauses long enough for quiesce",
  );
});

test("the ceiling is measured from the first edit after a write, not re-armed per edit", () => {
  const timers = fakeTimers();
  const writes = [];
  const s = scheduler(timers, writes);

  s.noteDirty();
  const firstEditAt = timers.now();
  for (let i = 0; i < 30; i++) {
    timers.advance(2_000);
    s.noteDirty();
  }
  const ceiling = writes.find((w) => w.reason === "ceiling");
  assert.ok(ceiling, "a ceiling write must have happened");
  assert.equal(
    ceiling.at - firstEditAt,
    CEILING_MS,
    "re-arming the ceiling per keystroke would let it recede forever",
  );
});

test("flush writes only when something is pending, and clears the pending state", () => {
  const timers = fakeTimers();
  const writes = [];
  const s = scheduler(timers, writes);

  assert.equal(s.flush("hidden"), null, "nothing dirty: a flush must not write");
  s.noteDirty();
  s.flush("hidden");
  assert.deepEqual(
    writes.map((w) => w.reason),
    ["hidden"],
  );
  assert.equal(timers.outstanding, 0, "a flush must cancel the timers it pre-empted");

  timers.advance(CEILING_MS * 2);
  assert.equal(writes.length, 1, "the pre-empted timers must not fire a second write");
});

test("reset forgets pending work — a saved document has no draft to write", () => {
  const timers = fakeTimers();
  const writes = [];
  const s = scheduler(timers, writes);
  s.noteDirty();
  s.reset();
  timers.advance(CEILING_MS * 2);
  assert.deepEqual(writes, []);
});

test("a draft past the age gate is not offered, and is evicted", () => {
  const now = 10 * DRAFT_TTL_MS;
  const fresh = { slotId: "a", bytes: 10, savedAt: now - 60_000, heartbeatAt: 0 };
  const stale = { slotId: "b", bytes: 10, savedAt: now - DRAFT_TTL_MS - 1, heartbeatAt: 0 };

  assert.deepEqual(
    offerableDrafts([fresh, stale], { now, ownSlotId: "me" }).map((m) => m.slotId),
    ["a"],
  );
  assert.deepEqual(evictableSlots([fresh, stale], { now, keepSlotId: "me" }), ["b"]);
});

test("a draft whose tab answered the presence ping is not offered", async () => {
  const now = Date.now();
  const live = { slotId: "other", bytes: 10, savedAt: now - 1_000, heartbeatAt: now - 1_000 };
  const orphan = { slotId: "gone", bytes: 10, savedAt: now - 1_000, heartbeatAt: now - 1_000 };

  const [mine, theirs] = fakeChannelPair();
  // The other tab is running and answers for itself; nothing answers for "gone".
  new DraftPresence({ slotId: "other", channel: theirs });
  const presence = new DraftPresence({ slotId: "me", channel: mine, wait: async () => {} });
  const liveSlotIds = await presence.liveSlots(["other", "gone"]);

  assert.deepEqual([...liveSlotIds], ["other"]);
  assert.deepEqual(
    offerableDrafts([live, orphan], { now, ownSlotId: "me", liveSlotIds }).map((m) => m.slotId),
    ["gone"],
    "offering a live tab's draft would invite the user to 'recover' the document " +
      "they are editing in their other window",
  );
});

test("a CRASHED tab's draft is offered at once, however fresh its heartbeat", async () => {
  // The regression this file exists to pin. A crashed tab stops writing
  // heartbeats, but the last one it wrote is SECONDS old — so a heartbeat-only
  // lease reports it alive and withholds the draft for the next 30 seconds,
  // which is exactly when the user is reopening the editor to get it back. The
  // crash e2e recovered nothing until presence stopped being a timestamp.
  const now = Date.now();
  const justCrashed = {
    slotId: "crashed",
    bytes: 10,
    savedAt: now - 2_000,
    heartbeatAt: now - 2_000, // two seconds ago; a heartbeat lease says "alive"
  };
  assert.equal(slotIsLive(justCrashed, now, "me"), true, "the weak signal is fooled");

  const [mine] = fakeChannelPair(); // nobody else is listening: nothing answers
  const presence = new DraftPresence({ slotId: "me", channel: mine, wait: async () => {} });
  const liveSlotIds = await presence.liveSlots(["crashed"]);

  assert.deepEqual([...liveSlotIds], []);
  assert.deepEqual(
    offerableDrafts([justCrashed], { now, ownSlotId: "me", liveSlotIds }).map((m) => m.slotId),
    ["crashed"],
  );
});

test("with no channel to ask on, the heartbeat rule is the fallback", async () => {
  const now = Date.now();
  const recent = { slotId: "other", bytes: 10, savedAt: now - 1_000, heartbeatAt: now - 1_000 };
  const stale = {
    slotId: "gone",
    bytes: 10,
    savedAt: now - 1_000,
    heartbeatAt: now - LIVE_SLOT_MS - 1,
  };
  const presence = new DraftPresence({ slotId: "me", channelFactory: () => null });
  assert.equal(presence.unavailable, true);
  assert.deepEqual([...(await presence.liveSlots(["other", "gone"]))], []);
  assert.deepEqual(
    offerableDrafts([recent, stale], {
      now,
      ownSlotId: "me",
      liveSlotIds: new Set(),
      heartbeatFallback: true,
    }).map((m) => m.slotId),
    ["gone"],
    "silence is only proof when there was a way to ask",
  );
});

test("this tab's own slot is always reclaimable, however fresh its heartbeat", () => {
  const now = Date.now();
  // After a crash the heartbeat is as recent as the crash — which can be one
  // second ago. Treating our own slot as "live" would hide our own work.
  const mine = { slotId: "me", bytes: 10, savedAt: now - 500, heartbeatAt: now - 500 };
  assert.equal(slotIsLive(mine, now, "me"), false);
  assert.deepEqual(
    offerableDrafts([mine], { now, ownSlotId: "me" }).map((m) => m.slotId),
    ["me"],
  );
});

test("a draft with no bytes recorded is never offered", () => {
  const now = Date.now();
  assert.deepEqual(
    offerableDrafts([{ slotId: "x", bytes: 0, savedAt: now, heartbeatAt: 0 }], {
      now,
      ownSlotId: "me",
    }),
    [],
  );
});

test("offers are newest first", () => {
  const now = Date.now();
  const rows = [
    { slotId: "old", bytes: 1, savedAt: now - 5_000, heartbeatAt: 0 },
    { slotId: "new", bytes: 1, savedAt: now - 1_000, heartbeatAt: 0 },
    { slotId: "mid", bytes: 1, savedAt: now - 3_000, heartbeatAt: 0 },
  ];
  assert.deepEqual(
    offerableDrafts(rows, { now, ownSlotId: "me" }).map((m) => m.slotId),
    ["new", "mid", "old"],
  );
});

test("the slot cap evicts the oldest, and never this tab's own slot", () => {
  const now = Date.now();
  const rows = [];
  for (let i = 0; i < MAX_DRAFT_SLOTS + 3; i++) {
    rows.push({ slotId: `s${i}`, bytes: 1, savedAt: now - i * 1_000, heartbeatAt: 0 });
  }
  // `s0` is the newest; the oldest four (of 11) go, leaving 7 + our own = 8.
  const evicted = evictableSlots(rows, { now, keepSlotId: "s0" });
  assert.equal(evicted.includes("s0"), false, "a write must never evict the slot it is writing");
  assert.deepEqual(evicted, ["s8", "s9", "s10"]);
});

test("a lossy container is promoted, a full-fidelity one is kept", () => {
  const DOCX = "org.openxmlformats.wordprocessingml.document";
  // Measured in docs/112 §3.2: a normalized-JSON snapshot drops every embedded
  // image, and plain text carries no formatting at all. Drafting either as
  // itself would hand back a document with the user's work missing.
  assert.equal(draftFormatFor("text.plain"), DOCX);
  assert.equal(draftFormatFor("org.casualoffice.normalized-json"), DOCX);
  assert.equal(draftFormatFor(DOCX), DOCX);
  assert.equal(draftFormatFor("org.oasis.opendocument.text"), "org.oasis.opendocument.text");
  assert.equal(draftFormatFor("application.rtf"), "application.rtf");
  assert.equal(draftFormatFor(undefined), DOCX, "an unknown source must not be drafted as JSON");
});

test("the document key is stable, cheap, and separates different documents", () => {
  const a = new Uint8Array(20_000).fill(7);
  const b = new Uint8Array(20_000).fill(7);
  b[19_999] = 8; // a change in the tail window
  assert.equal(documentKey("a.docx", a), documentKey("a.docx", a));
  assert.notEqual(documentKey("a.docx", a), documentKey("b.docx", a), "the name is part of it");
  assert.notEqual(documentKey("a.docx", a), documentKey("a.docx", b));
  assert.notEqual(
    documentKey("a.docx", a),
    documentKey("a.docx", new Uint8Array(20_001).fill(7)),
    "the length is part of it",
  );
});

test("the document key samples a bounded window, so open stays O(1)", () => {
  // 40 MB, the size of the file in docs/111. If this hashed every byte it would
  // be visible in the open path of every large document.
  const huge = new Uint8Array(40 * 1024 * 1024);
  const started = process.hrtime.bigint();
  documentKey("huge.docx", huge);
  const ms = Number(process.hrtime.bigint() - started) / 1e6;
  assert.ok(ms < 50, `documentKey took ${ms.toFixed(1)}ms on 40MB; it must sample, not scan`);
});

test("ages read the way a person would say them", () => {
  assert.equal(describeDraftAge(0), "just now");
  assert.equal(describeDraftAge(59_000), "just now");
  assert.equal(describeDraftAge(60_000), "1 minute ago");
  assert.equal(describeDraftAge(5 * 60_000), "5 minutes ago");
  assert.equal(describeDraftAge(60 * 60_000), "1 hour ago");
  assert.equal(describeDraftAge(5 * 60 * 60_000), "5 hours ago");
  assert.equal(describeDraftAge(30 * 60 * 60_000), "more than a day ago");
});

test("sizes read the way a person would say them", () => {
  assert.equal(describeDraftSize(512), "512 B");
  assert.equal(describeDraftSize(12_003), "12 KB");
  assert.equal(describeDraftSize(3_441_567), "3.3 MB");
});

test("a slot id survives a reload and is created when storage refuses", () => {
  const backing = new Map();
  const storage = {
    getItem: (k) => backing.get(k) ?? null,
    setItem: (k, v) => backing.set(k, v),
  };
  const first = draftSlotId(storage, () => "slot-1");
  assert.equal(first, "slot-1");
  assert.equal(
    draftSlotId(storage, () => "slot-2"),
    "slot-1",
    "the same tab must reclaim its own slot after a reload, or it cannot be " +
      "offered its own pre-crash draft",
  );

  const hostile = {
    getItem() {
      throw new Error("site data blocked");
    },
    setItem() {
      throw new Error("site data blocked");
    },
  };
  assert.equal(draftSlotId(hostile, () => "slot-3"), "slot-3");
});
