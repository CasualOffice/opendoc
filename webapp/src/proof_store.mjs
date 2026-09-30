// Where an installed language pack lives: the `opendoc-proofing` database.
//
// `docs/146` §6, "Storage choices": *"a separate database such as
// `opendoc-proofing` reduces migration coupling"*. That is a deliberate
// departure from the rule `drafts.mjs` records for the personal dictionary and
// version history — **one store, the opencalc shape (owner decision D-1)** — and
// the reason it is a departure rather than an oversight is a difference in
// LIFETIME, not in tidiness:
//
//   * a pack is **disposable and reconstructible**. It can be evicted, thrown
//     away on a schema change, or deleted wholesale to reclaim space, and the
//     only cost is a download;
//   * a draft, a version checkpoint and the user's own words are **valuable user
//     data**. Nothing about a pack may ever be a reason to touch them.
//
// Putting the two in one database means every pack schema bump is a version bump
// on the database holding unsaved work, and a blocked upgrade there reports
// "Autosave unavailable" (see `DRAFT_DB_VERSION`'s note). Trading that risk for a
// second `indexedDB.open` is the right trade, and it is the trade `docs/146`
// asked for.
//
// `indexedDB` is INJECTED, here for the same three reasons it is in `drafts.mjs`
// and one more: ADR-042 §6 says a host supplies storage, so an embed that
// forbids IndexedDB must be able to hand over a store of its own shape rather
// than have this module reach for a global that is not there.

import { idbCommitted, idbRequest } from "./drafts.mjs";

export const PROOF_DB_NAME = "opendoc-proofing";

/** Schema 1. Three stores, and the split between them is the same one drafts
 *  and version history use, for the same reason: listing what is installed must
 *  cost kilobytes, so nothing that lists packs may touch a word list. */
export const PROOF_DB_VERSION = 1;

/** One row per installed pack, keyed by `packId`: identity, version, size,
 *  provenance, and the pointer to its word rows. Small. */
export const PACKS_STORE = "packs";

/** The word lists, keyed by `wordsKey(packId, packVersion)`. Out of line, because the
 *  key names a VERSION: staging a new version writes a new row and cannot
 *  overwrite the one currently in use, which is most of how the rollback below
 *  works without a copy. */
export const WORDS_STORE = "packWords";

/** `locale -> { packId, packVersion, previousVersion }`. THE pointer. One
 *  `put` into this store is what "activate" means, and it is why activation is
 *  atomic rather than careful. */
export const ACTIVE_STORE = "activePack";

/** Joins a two-part key. Written as an escape and never as a raw byte: a
 *  literal NUL makes git classify the file as binary and stop showing its
 *  diffs, and `tests/source_bytes.test.mjs` fails the build if one appears
 *  anywhere under `webapp/src`. */
const KEY_SEP = "\u0000";

/** The row key for one pack version's words. */
export function wordsKey(packId, packVersion) {
  return `${packId}${KEY_SEP}${packVersion}`;
}

function upgrade(database) {
  // Every branch is `if (!contains)`, the shape `drafts.mjs` uses: an upgrade
  // adds what is missing and never recreates what is there. A pack is
  // reconstructible, so this matters less here than it does for drafts — but a
  // schema sweep that silently deleted an installed pack would present as
  // spelling quietly getting worse after a deploy, and "quietly" is the failure
  // mode `docs/146` §1 forbids.
  if (!database.objectStoreNames.contains(PACKS_STORE)) {
    database.createObjectStore(PACKS_STORE, { keyPath: "packId" });
  }
  if (!database.objectStoreNames.contains(WORDS_STORE)) {
    database.createObjectStore(WORDS_STORE);
  }
  if (!database.objectStoreNames.contains(ACTIVE_STORE)) {
    database.createObjectStore(ACTIVE_STORE, { keyPath: "locale" });
  }
}

function openDatabase(indexedDB, name) {
  if (!indexedDB) throw new Error("this browser has no IndexedDB");
  return new Promise((resolve, reject) => {
    const rq = indexedDB.open(name, PROOF_DB_VERSION);
    rq.onupgradeneeded = () => upgrade(rq.result);
    rq.onsuccess = () => resolve(rq.result);
    rq.onerror = () => reject(rq.error);
    rq.onblocked = () => reject(new Error("the proofing database is blocked by another tab"));
  });
}

/**
 * Opens the proofing store and returns the small API the installer and the SDK
 * use.
 *
 * Every method rejects rather than swallowing, for the reason `docs/112` §4.1
 * gives about the personal dictionary: a pack the user was told was installed
 * and which was not stored is a lie that survives until the next reload.
 */
export async function openProofStore({ indexedDB: idb, name = PROOF_DB_NAME } = {}) {
  const db = await openDatabase(idb, name);

  /** `packId -> record` for every ACTIVE pack, in `packId` order.
   *
   *  "Active" and not "present": a staged version that never got promoted has a
   *  words row and no pointer, and reporting it as installed is exactly the lie
   *  the staging namespace exists to avoid. O(installed packs) — one row per
   *  locale — and nothing here reads a word list. */
  async function activeRecords() {
    const tx = db.transaction([ACTIVE_STORE, PACKS_STORE], "readonly");
    const pointers = await idbRequest(tx.objectStore(ACTIVE_STORE).getAll());
    const packs = await idbRequest(tx.objectStore(PACKS_STORE).getAll());
    const byId = new Map(packs.map((pack) => [pack.packId, pack]));
    const rows = [];
    for (const pointer of pointers) {
      const pack = byId.get(pointer.packId);
      if (!pack) continue;
      if (pack.packVersion !== pointer.packVersion) continue;
      rows.push(pack);
    }
    rows.sort((a, b) => (a.packId < b.packId ? -1 : a.packId > b.packId ? 1 : 0));
    return rows;
  }

  return {
    /** What is installed and active: identity, version, size, provenance and
     *  word COUNT — never the words. The dialog renders from this. */
    async installed() {
      return (await activeRecords()).map((pack) => ({
        packId: pack.packId,
        locale: pack.locale,
        packVersion: pack.packVersion,
        bytes: pack.bytes,
        capabilities: pack.capabilities ?? [],
        provenance: pack.provenance ?? [],
        words: pack.words ?? 0,
        installedAt: pack.installedAt ?? 0,
      }));
    },

    /** The word list of an active pack, as an array, or `[]`. Read once per
     *  session and handed to the worker; nothing on the keystroke path calls it. */
    async words(packId) {
      const tx = db.transaction([ACTIVE_STORE, PACKS_STORE, WORDS_STORE], "readonly");
      const pack = await idbRequest(tx.objectStore(PACKS_STORE).get(packId));
      if (!pack) return [];
      const pointer = await idbRequest(tx.objectStore(ACTIVE_STORE).get(pack.locale));
      if (!pointer || pointer.packId !== packId || pointer.packVersion !== pack.packVersion) {
        return [];
      }
      const row = await idbRequest(
        tx.objectStore(WORDS_STORE).get(wordsKey(packId, pack.packVersion)),
      );
      return row?.words ?? [];
    },

    /**
     * Step 5 of `docs/146` §6 — writes the new version into its OWN namespace.
     *
     * The key carries the version, so this cannot overwrite the version in use.
     * Nothing points at it yet, so nothing reads it yet, so a crash here is
     * invisible to the reader and a retry is idempotent.
     */
    async stage(record) {
      const tx = db.transaction(WORDS_STORE, "readwrite");
      tx.objectStore(WORDS_STORE).put(
        { packId: record.packId, packVersion: record.packVersion, words: [...record.words] },
        wordsKey(record.packId, record.packVersion),
      );
      await idbCommitted(tx);
    },

    /**
     * Step 7 — the atomic switch.
     *
     * ONE readwrite transaction writes the pack record and flips the pointer,
     * awaited on `oncomplete` rather than on the last `put`'s `onsuccess`: a put
     * that succeeds in a transaction that then aborts is not an activation, and
     * the whole atomicity claim rests on awaiting the commit (`idbCommitted`'s
     * own note says the same thing about version history).
     *
     * The PREVIOUS version's words row is deliberately left in place, and the
     * pointer remembers its version. That is the rollback of `docs/146` §9
     * ("update rollback") and it costs one extra word list per updated pack
     * until `forget` sweeps it — which is the right trade, because the failure it
     * covers is "the new version is bad and the reader has no proofing until they
     * find a network".
     */
    async activate(record, now = 0) {
      const tx = db.transaction([PACKS_STORE, ACTIVE_STORE], "readwrite");
      const previous = await idbRequest(tx.objectStore(ACTIVE_STORE).get(record.locale));
      tx.objectStore(PACKS_STORE).put({
        packId: record.packId,
        locale: record.locale,
        packVersion: record.packVersion,
        bytes: record.bytes,
        capabilities: [...(record.capabilities ?? [])],
        provenance: (record.provenance ?? []).map((entry) => ({ ...entry })),
        words: record.words.length,
        installedAt: now,
      });
      tx.objectStore(ACTIVE_STORE).put({
        locale: record.locale,
        packId: record.packId,
        packVersion: record.packVersion,
        previousVersion:
          previous && previous.packId === record.packId ? previous.packVersion : "",
      });
      await idbCommitted(tx);
    },

    /**
     * Rolls one locale back to the version it was on before the last activation.
     *
     * Only possible while the previous version's words row is still there, which
     * is what `activate` retains it for. Returns the version it went back to, or
     * `""` when there was nothing to go back to — a caller that cannot tell those
     * apart would report a rollback that did not happen.
     */
    async rollback(locale) {
      const tx = db.transaction([PACKS_STORE, ACTIVE_STORE, WORDS_STORE], "readwrite");
      const pointer = await idbRequest(tx.objectStore(ACTIVE_STORE).get(locale));
      const target = pointer?.previousVersion ?? "";
      if (!pointer || !target) {
        await idbCommitted(tx);
        return "";
      }
      const row = await idbRequest(
        tx.objectStore(WORDS_STORE).get(wordsKey(pointer.packId, target)),
      );
      if (!row) {
        await idbCommitted(tx);
        return "";
      }
      const pack = await idbRequest(tx.objectStore(PACKS_STORE).get(pointer.packId));
      tx.objectStore(PACKS_STORE).put({ ...pack, packVersion: target, words: row.words.length });
      tx.objectStore(ACTIVE_STORE).put({
        locale,
        packId: pointer.packId,
        packVersion: target,
        previousVersion: "",
      });
      await idbCommitted(tx);
      return target;
    },

    /** Drops a STAGED version that never became active. Called by the installer's
     *  cleanup step on both paths, so a refused install leaves nothing behind. */
    async discard(packId) {
      if (!packId) return;
      const tx = db.transaction([PACKS_STORE, ACTIVE_STORE, WORDS_STORE], "readwrite");
      const pack = await idbRequest(tx.objectStore(PACKS_STORE).get(packId));
      const pointer = pack ? await idbRequest(tx.objectStore(ACTIVE_STORE).get(pack.locale)) : null;
      const keep = new Set(
        [
          pointer?.packVersion ? wordsKey(packId, pointer.packVersion) : "",
          pointer?.previousVersion ? wordsKey(packId, pointer.previousVersion) : "",
        ].filter(Boolean),
      );
      const keys = await idbRequest(tx.objectStore(WORDS_STORE).getAllKeys());
      for (const key of keys) {
        const name = String(key);
        if (!name.startsWith(`${packId}${KEY_SEP}`)) continue;
        if (keep.has(name)) continue;
        tx.objectStore(WORDS_STORE).delete(key);
      }
      await idbCommitted(tx);
    },

    /** Removes a pack completely: its pointer, its record and every version of
     *  its words. The pointer goes FIRST in the same transaction, so a reader
     *  can never see a record with no words behind it. */
    async remove(packId) {
      const tx = db.transaction([PACKS_STORE, ACTIVE_STORE, WORDS_STORE], "readwrite");
      const pack = await idbRequest(tx.objectStore(PACKS_STORE).get(packId));
      if (!pack) {
        await idbCommitted(tx);
        return false;
      }
      tx.objectStore(ACTIVE_STORE).delete(pack.locale);
      tx.objectStore(PACKS_STORE).delete(packId);
      const keys = await idbRequest(tx.objectStore(WORDS_STORE).getAllKeys());
      for (const key of keys) {
        if (String(key).startsWith(`${packId}${KEY_SEP}`)) {
          tx.objectStore(WORDS_STORE).delete(key);
        }
      }
      await idbCommitted(tx);
      return true;
    },

    close() {
      db.close();
    },
  };
}
