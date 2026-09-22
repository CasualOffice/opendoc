// Autosave drafts and crash recovery — the store and all of its policy.
//
// Designed in docs/112; the row is docs/109 #2 / HF-011 / OO-004, and the
// owner decision that unblocked it is D-1 ("browser storage yes, one store,
// the opencalc shape"). The problem it closes is that the open document lives
// in the wasm heap and NOWHERE else: an OOM kill, a wasm trap or a power cut
// fires no `beforeunload` and leaves no trace of the work.
//
// Two rules shape everything here.
//
//   1. **Per-keystroke work is O(1) in document size** (docs/107 §4). Taking a
//      snapshot is O(document), so it can never run on the keystroke path. The
//      keystroke path calls `DraftScheduler.noteDirty()`, which does three
//      constant-time things: store an integer, clear a timer, set a timer.
//   2. **No silent data loss** (AGENTS.md, SKILL.md §12). Measured in docs/112
//      §3.2: a normalized-JSON snapshot drops `binary_resources`, i.e. every
//      embedded image. So a draft is written in the document's own source
//      format, through the same export ladder Save uses, and the two formats
//      that cannot carry the model are promoted to DOCX.
//
// This module is deliberately separate from main.js (which has zero exports and
// binds ~360 DOM ids at import — HF-085). Everything policy-shaped is a pure
// function over plain data, and the store takes its `indexedDB` by injection,
// so `tests/drafts.test.mjs` exercises the rules in node with no browser.

export const DRAFT_DB_NAME = "opendoc-drafts";

/** Version 2 adds the personal spelling dictionary (`docs/114` §4).
 *
 *  The store lives in THIS database rather than a second one because there is
 *  one browser-storage decision here (D-1: one store, the opencalc shape) and
 *  a second database would be a second thing to migrate, quota, clear and
 *  explain. The upgrade creates only what is missing, so an existing tab's
 *  drafts survive it untouched — which is the whole point of doing it as a
 *  version bump instead of a delete-and-recreate. */
export const DRAFT_DB_VERSION = 2;
export const META_STORE = "meta";
export const BYTES_STORE = "bytes";
export const WORDS_STORE = "words";

/** How long a draft is kept before it is pruned. The docs-repo reference uses
 *  the same 24 hours; a draft older than a day is far more likely to be a
 *  forgotten experiment than work somebody still wants back. */
export const DRAFT_TTL_MS = 24 * 60 * 60 * 1000;

/** Heartbeat freshness, used ONLY where a presence ping is impossible (see
 *  `DraftPresence`). A slot whose heartbeat is younger than this is assumed to
 *  belong to a tab that is still open. */
export const LIVE_SLOT_MS = 30_000;

/** How long a boot waits for other tabs to answer "who still owns a slot?".
 *  Paid only when some candidate row looks recent enough to be worth asking
 *  about, so the common case — reopening after a crash with nothing else
 *  running — waits for nothing. */
export const LIVE_PING_MS = 600;

export const DRAFT_CHANNEL_NAME = "opendoc-drafts";

/** Bounds the store: at most this many tab slots keep a draft. */
export const MAX_DRAFT_SLOTS = 8;

/** Snapshot cadence. Quiesce is the sibling's (editor.drafts.js); the ceiling
 *  bounds worst-case loss for continuous typing at one minute. */
export const QUIESCE_MS = 5_000;
export const CEILING_MS = 60_000;

/** Meta-only refresh so other tabs can see this slot is alive. Runs only while
 *  a draft exists, and writes a few hundred bytes. */
export const HEARTBEAT_MS = 10_000;

const SLOT_STORAGE_KEY = "opendoc.draftSlot";

const DOCX_FORMAT = "org.openxmlformats.wordprocessingml.document";

/** Source formats that cannot carry the editor's model, and what a draft of
 *  them is written as instead (docs/112 §4.2):
 *
 *   * `text.plain` carries no formatting at all;
 *   * `org.casualoffice.normalized-json` measurably drops binary resources.
 *
 *  Everything else — DOCX, ODT, RTF — is a full-fidelity container, so its
 *  draft is byte-for-byte what Save would have produced. */
const PROMOTED_SOURCE_FORMATS = new Set(["text.plain", "org.casualoffice.normalized-json"]);

/** The format a draft of a `sourceFormat` document is written in. */
export function draftFormatFor(sourceFormat) {
  if (!sourceFormat) return DOCX_FORMAT;
  return PROMOTED_SOURCE_FORMATS.has(sourceFormat) ? DOCX_FORMAT : sourceFormat;
}

/** The export modes a draft write tries, in order. The first two are exactly
 *  what `exportDocumentAs` tries for Save, so the draft and the save can never
 *  be produced by different ladders; `semantic` is the honest last resort.
 *  There is no cross-format fallback: falling back to normalized JSON would
 *  silently drop images, which is the defect this module exists to avoid. */
export const DRAFT_EXPORT_MODES = Object.freeze([
  "exact_if_unchanged",
  "preserve_when_safe",
  "semantic",
]);

/** This tab's slot id, kept in `sessionStorage` — whose lifetime is exactly a
 *  tab. It survives a reload and a renderer-crash reload, so a tab reclaims its
 *  own slot and is offered its own pre-crash draft; it disappears with the tab.
 *
 *  Storage access is guarded the same way main.js guards preferences: in a
 *  cross-origin embed or with site data blocked, even touching the object
 *  throws. A slot id that cannot be persisted still works for this page load. */
export function draftSlotId(storage, randomId = defaultRandomId) {
  try {
    const existing = storage?.getItem(SLOT_STORAGE_KEY);
    if (existing) return existing;
  } catch {
    return randomId();
  }
  const next = randomId();
  try {
    storage.setItem(SLOT_STORAGE_KEY, next);
  } catch {
    // A slot that lives only for this page load is still a usable slot.
  }
  return next;
}

function defaultRandomId() {
  const cryptoObj = globalThis.crypto;
  if (cryptoObj?.randomUUID) return `slot-${cryptoObj.randomUUID()}`;
  return `slot-${Math.random().toString(36).slice(2)}${Date.now().toString(36)}`;
}

/** A cheap, stable identity for the bytes a document was opened from.
 *
 *  FNV-1a over the name, the byte length and up to 4 KB sampled from each end —
 *  O(1) in document size, so it can run on the open path of a 200 MB file. It
 *  answers "is this draft for the file I just opened?", which only decides how
 *  the offer is WORDED; a collision cannot lose data, because recovery never
 *  overwrites anything without the user pressing Restore. */
export function documentKey(name, bytes) {
  let hash = 0x811c9dc5;
  const mix = (byte) => {
    hash ^= byte & 0xff;
    hash = Math.imul(hash, 0x01000193) >>> 0;
  };
  for (const ch of String(name ?? "")) mix(ch.codePointAt(0));
  const length = bytes?.length ?? 0;
  for (let shift = 0; shift < 32; shift += 8) mix((length >>> shift) & 0xff);
  if (bytes) {
    const window = Math.min(4096, length);
    for (let i = 0; i < window; i++) mix(bytes[i]);
    for (let i = Math.max(window, length - window); i < length; i++) mix(bytes[i]);
  }
  return `${(hash >>> 0).toString(16)}-${length.toString(16)}`;
}

/** "just now" / "2 minutes ago" / "3 hours ago" / "yesterday". Deliberately
 *  coarse: the number a user needs from a recovery bar is "is this the work I
 *  remember doing?", not a timestamp to the second. */
export function describeDraftAge(ageMs) {
  const ms = Math.max(0, Number(ageMs) || 0);
  const minutes = Math.floor(ms / 60_000);
  if (minutes < 1) return "just now";
  if (minutes === 1) return "1 minute ago";
  if (minutes < 60) return `${minutes} minutes ago`;
  const hours = Math.floor(minutes / 60);
  if (hours === 1) return "1 hour ago";
  if (hours < 24) return `${hours} hours ago`;
  return "more than a day ago";
}

/** Byte counts as a person reads them. */
export function describeDraftSize(bytes) {
  const n = Math.max(0, Number(bytes) || 0);
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${Math.round(n / 1024)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

/** Whether a meta row's heartbeat says its tab is still open.
 *
 *  This is the WEAK signal, and knowing why matters. A tab that crashes stops
 *  writing heartbeats, but the last one it wrote is only seconds old — so for
 *  the next `LIVE_SLOT_MS` a heartbeat-only lease reports the crashed tab as
 *  alive and withholds its draft. That is precisely the moment the user is
 *  reopening the editor to get their work back, so it is the one case the
 *  lease must not get wrong. Found by the crash spec, which recovered nothing
 *  until presence stopped being inferred from a timestamp.
 *
 *  `DraftPresence` answers the question properly by asking the other tabs.
 *  This remains the fallback where that is impossible. */
export function slotIsLive(meta, now, ownSlotId) {
  if (!meta) return false;
  if (meta.slotId === ownSlotId) return false; // our own slot is ours to reclaim
  return now - (meta.heartbeatAt ?? 0) < LIVE_SLOT_MS;
}

/**
 * Cross-tab presence over `BroadcastChannel`.
 *
 * Every editor tab answers a ping with its own slot id. A tab that has crashed,
 * been killed or been closed answers nothing — which is the whole difference
 * between "this draft belongs to a window you still have open" and "this draft
 * is all that is left of a session that died", and a heartbeat cannot tell them
 * apart in the seconds right after a crash.
 *
 * Everything is injected so the protocol can be unit-tested against a fake
 * channel pair rather than a real browser.
 */
export class DraftPresence {
  constructor({
    slotId,
    channel = null,
    channelFactory = (channelName) =>
      globalThis.BroadcastChannel ? new globalThis.BroadcastChannel(channelName) : null,
    channelName = DRAFT_CHANNEL_NAME,
    wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
  } = {}) {
    this.slotId = slotId;
    this.wait = wait;
    this.channel = channel ?? channelFactory(channelName);
    this.replies = new Set();
    if (this.channel) {
      this.channel.onmessage = (event) => this.receive(event?.data);
    }
  }

  /** True when this browser gave us no way to ask, so callers know to fall
   *  back to the heartbeat rule rather than treating silence as proof. */
  get unavailable() {
    return !this.channel;
  }

  receive(message) {
    if (!message || message.channel !== DRAFT_CHANNEL_NAME) return;
    if (message.type === "ping" && message.from !== this.slotId) {
      this.channel.postMessage({
        channel: DRAFT_CHANNEL_NAME,
        type: "pong",
        from: this.slotId,
      });
    } else if (message.type === "pong" && message.from !== this.slotId) {
      this.replies.add(message.from);
    }
  }

  /** The subset of `candidateSlotIds` whose tabs answered within the window. */
  async liveSlots(candidateSlotIds, { windowMs = LIVE_PING_MS } = {}) {
    if (this.unavailable || candidateSlotIds.length === 0) return new Set();
    this.replies = new Set();
    this.channel.postMessage({ channel: DRAFT_CHANNEL_NAME, type: "ping", from: this.slotId });
    await this.wait(windowMs);
    const candidates = new Set(candidateSlotIds);
    return new Set([...this.replies].filter((slotId) => candidates.has(slotId)));
  }

  close() {
    this.channel?.close?.();
    this.channel = null;
  }
}

/** Whether a meta row is too old to offer. */
export function draftIsExpired(meta, now, ttlMs = DRAFT_TTL_MS) {
  return now - (meta?.savedAt ?? 0) >= ttlMs;
}

/** The drafts a boot should OFFER, newest first.
 *
 *  Excluded: rows past the age gate, rows with no bytes recorded, and rows
 *  whose owning tab answered the presence ping. `heartbeatFallback` re-enables
 *  the weak timestamp rule for a browser that gave us no channel to ask on.
 *
 *  Note what is NOT a filter — matching the document that happens to be open.
 *  A draft for another file is still the user's unsaved work and still has to
 *  be offered, or the one case where the editor reopens a different document
 *  is the case where the work is lost. */
export function offerableDrafts(
  metas,
  { now, ownSlotId, ttlMs = DRAFT_TTL_MS, liveSlotIds = null, heartbeatFallback = false } = {},
) {
  const live = liveSlotIds ?? new Set();
  return (metas ?? [])
    .filter((meta) => meta && meta.bytes > 0)
    .filter((meta) => !draftIsExpired(meta, now, ttlMs))
    .filter((meta) => !live.has(meta.slotId))
    .filter((meta) => !(heartbeatFallback && slotIsLive(meta, now, ownSlotId)))
    .sort((a, b) => (b.savedAt ?? 0) - (a.savedAt ?? 0));
}

/** Slot ids a write should delete first: everything expired, then the oldest
 *  rows over the slot cap. `keepSlotId` is this tab's own slot, which is about
 *  to be written and must never be evicted by its own write. */
export function evictableSlots(
  metas,
  { now, keepSlotId, ttlMs = DRAFT_TTL_MS, maxSlots = MAX_DRAFT_SLOTS } = {},
) {
  const rows = (metas ?? []).filter((meta) => meta && meta.slotId !== keepSlotId);
  const expired = rows.filter((meta) => draftIsExpired(meta, now, ttlMs));
  const expiredIds = new Set(expired.map((meta) => meta.slotId));
  const survivors = rows
    .filter((meta) => !expiredIds.has(meta.slotId))
    .sort((a, b) => (b.savedAt ?? 0) - (a.savedAt ?? 0));
  // `keepSlotId` occupies one of the slots, hence maxSlots - 1 for the rest.
  const overflow = survivors.slice(Math.max(0, maxSlots - 1));
  return [...expiredIds, ...overflow.map((meta) => meta.slotId)];
}

/**
 * The write cadence: quiesce after the last edit, with a hard ceiling while
 * edits keep arriving.
 *
 * `noteDirty()` is the only method the keystroke path calls, and it is three
 * constant-time operations. Everything expensive happens inside `write`, which
 * this class only ever calls from a timer.
 *
 * Timers and the clock are injected so the cadence itself can be unit-tested
 * without waiting 60 real seconds — and so a test can prove the ceiling fires,
 * which a test that only waits for quiesce never would.
 */
export class DraftScheduler {
  constructor({
    write,
    quiesceMs = QUIESCE_MS,
    ceilingMs = CEILING_MS,
    now = () => Date.now(),
    setTimer = (fn, ms) => setTimeout(fn, ms),
    clearTimer = (handle) => clearTimeout(handle),
  }) {
    this.write = write;
    this.quiesceMs = quiesceMs;
    this.ceilingMs = ceilingMs;
    this.now = now;
    this.setTimer = setTimer;
    this.clearTimer = clearTimer;
    this.quiesceHandle = null;
    this.ceilingHandle = null;
    this.dirty = false;
  }

  /** O(1). Called from the edit choke point, on every keystroke. */
  noteDirty() {
    this.dirty = true;
    if (this.quiesceHandle !== null) this.clearTimer(this.quiesceHandle);
    this.quiesceHandle = this.setTimer(() => {
      this.quiesceHandle = null;
      this.fire("quiesce");
    }, this.quiesceMs);
    // The ceiling is NOT restarted per edit — that is the point of it. It runs
    // from the first edit after a write, so continuous typing cannot push the
    // quiesce timer forward forever and leave the disk copy unboundedly stale.
    if (this.ceilingHandle === null) {
      this.ceilingHandle = this.setTimer(() => {
        this.ceilingHandle = null;
        this.fire("ceiling");
      }, this.ceilingMs);
    }
  }

  /** Writes now if anything is pending (visibilitychange, pagehide, an explicit
   *  flush). Returns whatever `write` returns, or null when nothing was due. */
  flush(reason) {
    if (!this.dirty) return null;
    return this.fire(reason);
  }

  /** The document is saved, or replaced: forget everything pending. */
  reset() {
    this.dirty = false;
    this.cancelTimers();
  }

  stop() {
    this.reset();
  }

  cancelTimers() {
    if (this.quiesceHandle !== null) this.clearTimer(this.quiesceHandle);
    if (this.ceilingHandle !== null) this.clearTimer(this.ceilingHandle);
    this.quiesceHandle = null;
    this.ceilingHandle = null;
  }

  fire(reason) {
    this.dirty = false;
    this.cancelTimers();
    return this.write(reason);
  }
}

/** Promise wrapper for one IDB request. */
function request(rq) {
  return new Promise((resolve, reject) => {
    rq.onsuccess = () => resolve(rq.result);
    rq.onerror = () => reject(rq.error);
  });
}

/** Promise for a transaction actually committing — `oncomplete`, not the last
 *  request's `onsuccess`. A put that succeeds in a transaction that then aborts
 *  is not a draft on disk. */
function committed(tx) {
  return new Promise((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
    tx.onabort = () => reject(tx.error ?? new Error("draft transaction aborted"));
  });
}

/**
 * Creates whatever stores this version needs and nothing else.
 *
 * Every branch is `if (!contains)`, so an upgrade from version 1 adds the words
 * store and leaves `meta` and `bytes` — and every draft in them — exactly as
 * they were. Shared by both openers below so the two can never disagree about
 * the schema; whichever runs first in a session creates all three.
 */
function upgradeDraftDatabase(database) {
  if (!database.objectStoreNames.contains(META_STORE)) {
    database.createObjectStore(META_STORE, { keyPath: "slotId" });
  }
  if (!database.objectStoreNames.contains(BYTES_STORE)) {
    database.createObjectStore(BYTES_STORE);
  }
  if (!database.objectStoreNames.contains(WORDS_STORE)) {
    // Keyed by the word itself: adding the same word twice is idempotent
    // without a read, and "is this word known" is one `get`.
    database.createObjectStore(WORDS_STORE);
  }
}

/** Opens the shared database. `indexedDB` is injected for the same reason
 *  everywhere here: so the rules run in node with no browser. */
function openDatabase(indexedDB, name) {
  if (!indexedDB) throw new Error("this browser has no IndexedDB");
  return new Promise((resolve, reject) => {
    const rq = indexedDB.open(name, DRAFT_DB_VERSION);
    rq.onupgradeneeded = () => upgradeDraftDatabase(rq.result);
    rq.onsuccess = () => resolve(rq.result);
    rq.onerror = () => reject(rq.error);
    rq.onblocked = () => reject(new Error("the draft database is blocked by another tab"));
  });
}

/**
 * Opens the personal spelling dictionary (`docs/114` §4 / `109` HF-035).
 *
 * Deliberately a SEPARATE opener from `openDraftStore` even though it is the
 * same database: autosave can be off — by preference, or by policy in a
 * cross-origin embed — and a user's own words must not disappear with it.
 *
 * Every method rejects rather than swallowing: a word the user added that was
 * not stored has to be reportable (`docs/112` §4.1, the same rule autosave
 * follows).
 */
export async function openWordStore({
  indexedDB = globalThis.indexedDB,
  name = DRAFT_DB_NAME,
} = {}) {
  const db = await openDatabase(indexedDB, name);
  return {
    /** Every word the user has added, as a plain array. Hundreds of entries at
     *  the very most; read once at boot and kept in memory after that. */
    async list() {
      const tx = db.transaction(WORDS_STORE, "readonly");
      const keys = await request(tx.objectStore(WORDS_STORE).getAllKeys());
      return keys.map(String);
    },

    /** Adds one word. Idempotent: the word IS the key. */
    async add(word, addedAt = Date.now()) {
      const tx = db.transaction(WORDS_STORE, "readwrite");
      tx.objectStore(WORDS_STORE).put({ word, addedAt }, word);
      await committed(tx);
    },

    async remove(word) {
      const tx = db.transaction(WORDS_STORE, "readwrite");
      tx.objectStore(WORDS_STORE).delete(word);
      await committed(tx);
    },

    /** Empties the personal dictionary. Until a management surface exists
     *  (`docs/114` §8) this is the only way to take a word back out, which is
     *  why the command that adds words says so. */
    async clear() {
      const tx = db.transaction(WORDS_STORE, "readwrite");
      tx.objectStore(WORDS_STORE).clear();
      await committed(tx);
    },

    close() {
      db.close();
    },
  };
}

/**
 * Opens (or creates) the draft database and returns the small API main.js uses.
 *
 * `indexedDB` is injected so tests can pass a fake, and so the caller — not
 * this module — decides what "storage is unavailable" means. Rejecting is
 * deliberate: autosave that cannot write must SAY so (docs/112 §4.1), not
 * pretend.
 */
export async function openDraftStore({
  indexedDB = globalThis.indexedDB,
  name = DRAFT_DB_NAME,
} = {}) {
  const db = await openDatabase(indexedDB, name);

  return {
    /** Every meta row. Kilobytes: the snapshots live in the other store. */
    async listMeta() {
      const tx = db.transaction(META_STORE, "readonly");
      return request(tx.objectStore(META_STORE).getAll());
    },

    /** One meta row. */
    async readMeta(slotId) {
      const tx = db.transaction(META_STORE, "readonly");
      return request(tx.objectStore(META_STORE).get(slotId));
    },

    /** The snapshot for a slot, or undefined. */
    async readBytes(slotId) {
      const tx = db.transaction(BYTES_STORE, "readonly");
      return request(tx.objectStore(BYTES_STORE).get(slotId));
    },

    /** Writes a draft: meta and bytes in ONE transaction across both stores, so
     *  a meta row can never point at bytes that were not written. */
    async putDraft(meta, bytes) {
      const tx = db.transaction([META_STORE, BYTES_STORE], "readwrite");
      tx.objectStore(META_STORE).put(meta);
      tx.objectStore(BYTES_STORE).put(bytes, meta.slotId);
      await committed(tx);
    },

    /** Refreshes only the heartbeat, so other tabs can see this slot is alive.
     *  A few hundred bytes; never touches the snapshot. */
    async touch(slotId, heartbeatAt) {
      const tx = db.transaction(META_STORE, "readwrite");
      const store = tx.objectStore(META_STORE);
      const meta = await request(store.get(slotId));
      if (meta) store.put({ ...meta, heartbeatAt });
      await committed(tx);
    },

    async deleteSlot(slotId) {
      const tx = db.transaction([META_STORE, BYTES_STORE], "readwrite");
      tx.objectStore(META_STORE).delete(slotId);
      tx.objectStore(BYTES_STORE).delete(slotId);
      await committed(tx);
    },

    async clear() {
      const tx = db.transaction([META_STORE, BYTES_STORE], "readwrite");
      tx.objectStore(META_STORE).clear();
      tx.objectStore(BYTES_STORE).clear();
      await committed(tx);
    },

    close() {
      db.close();
    },
  };
}
