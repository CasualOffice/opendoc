// The personal spelling dictionary, and the version-2 upgrade that carries it.
//
// `docs/114` §4 puts the store in the EXISTING `opendoc-drafts` database at
// version 2 rather than in a new one, which makes the upgrade path the risky
// part: a `createObjectStore` sweep that recreated `meta` and `bytes` would
// throw away every unsaved draft on the first reload after the deploy, and the
// user would find out by losing work. So the thing under test here is not
// mostly "can a word be stored" — it is "does an existing version-1 database
// come through the upgrade with its drafts intact".
//
// `drafts.mjs` takes its `indexedDB` by injection for exactly this reason, and
// the fake below is the smallest thing that honours the parts of the IndexedDB
// contract the store actually uses: a version, `onupgradeneeded` firing only on
// a version increase, `objectStoreNames.contains`, and a transaction that
// completes asynchronously.

import assert from "node:assert/strict";
import test from "node:test";

import {
  BYTES_STORE,
  DRAFT_DB_VERSION,
  META_STORE,
  WORDS_STORE,
  openDraftStore,
  openWordStore,
} from "../src/drafts.mjs";

/** A minimal in-memory IndexedDB: enough of the shape for the two openers. */
function fakeIndexedDB() {
  const databases = new Map();

  function makeDatabase(name, version) {
    return { name, version, stores: new Map() };
  }

  function storeHandle(state, tx) {
    return {
      put(value, key) {
        state.set(key ?? value.slotId, value);
        tx.touched = true;
      },
      get(key) {
        return fakeRequest(state.get(key));
      },
      getAll() {
        return fakeRequest([...state.values()]);
      },
      getAllKeys() {
        return fakeRequest([...state.keys()]);
      },
      delete(key) {
        state.delete(key);
      },
      clear() {
        state.clear();
      },
    };
  }

  function fakeRequest(result) {
    const rq = { result };
    queueMicrotask(() => rq.onsuccess?.());
    return rq;
  }

  function databaseHandle(db) {
    return {
      get objectStoreNames() {
        return { contains: (name) => db.stores.has(name) };
      },
      createObjectStore(name) {
        db.stores.set(name, new Map());
      },
      transaction(names) {
        const list = Array.isArray(names) ? names : [names];
        const tx = { touched: false };
        queueMicrotask(() => queueMicrotask(() => tx.oncomplete?.()));
        tx.objectStore = (name) => {
          assert.ok(list.includes(name), `${name} is not in this transaction`);
          return storeHandle(db.stores.get(name), tx);
        };
        return tx;
      },
      close() {},
    };
  }

  return {
    databases,
    open(name, version) {
      const rq = {};
      queueMicrotask(() => {
        let db = databases.get(name);
        if (!db) {
          db = makeDatabase(name, 0);
          databases.set(name, db);
        }
        const upgrading = version > db.version;
        db.version = Math.max(db.version, version);
        rq.result = databaseHandle(db);
        if (upgrading) rq.onupgradeneeded?.();
        rq.onsuccess?.();
      });
      return rq;
    },
  };
}

test("a word survives a store round trip, and adding it twice is one entry", async () => {
  const indexedDB = fakeIndexedDB();
  const store = await openWordStore({ indexedDB, name: "t1" });
  assert.deepEqual(await store.list(), [], "a fresh dictionary is empty");
  await store.add("QZXword");
  await store.add("QZXword");
  assert.deepEqual(await store.list(), ["QZXword"], "the word IS the key");
  await store.add("Nagpur");
  assert.deepEqual((await store.list()).sort(), ["Nagpur", "QZXword"]);
  await store.remove("Nagpur");
  assert.deepEqual(await store.list(), ["QZXword"]);
  await store.clear();
  assert.deepEqual(await store.list(), []);
});

test("the words store is created at version 2 without disturbing the draft stores", async () => {
  const indexedDB = fakeIndexedDB();

  // A tab on the OLD build: version 1, two stores, one draft in them.
  await new Promise((resolve) => {
    const rq = indexedDB.open("t2", 1);
    rq.onupgradeneeded = () => {
      rq.result.createObjectStore(META_STORE);
      rq.result.createObjectStore(BYTES_STORE);
    };
    rq.onsuccess = () => resolve();
  });
  const legacy = indexedDB.databases.get("t2");
  legacy.stores.get(META_STORE).set("slot-1", { slotId: "slot-1", name: "work.docx" });
  legacy.stores.get(BYTES_STORE).set("slot-1", new Uint8Array([1, 2, 3]));

  // The new build opens the same database.
  const drafts = await openDraftStore({ indexedDB, name: "t2" });

  assert.equal(legacy.version, DRAFT_DB_VERSION, "the upgrade ran");
  assert.ok(legacy.stores.has(WORDS_STORE), "and created the words store");
  assert.deepEqual(
    await drafts.readMeta("slot-1"),
    { slotId: "slot-1", name: "work.docx" },
    "the pre-upgrade draft's meta row is still there — a createObjectStore " +
      "sweep would have thrown away the user's unsaved work on first reload",
  );
  assert.deepEqual(
    [...(await drafts.readBytes("slot-1"))],
    [1, 2, 3],
    "and so are its bytes",
  );

  // And the dictionary opens against the same database with no second upgrade.
  const words = await openWordStore({ indexedDB, name: "t2" });
  await words.add("QZXword");
  assert.deepEqual(await words.list(), ["QZXword"]);
});

test("clearing drafts does not clear the personal dictionary", async () => {
  const indexedDB = fakeIndexedDB();
  const words = await openWordStore({ indexedDB, name: "t3" });
  const drafts = await openDraftStore({ indexedDB, name: "t3" });
  await words.add("QZXword");
  await drafts.putDraft({ slotId: "s", name: "a.docx" }, new Uint8Array([9]));

  // Turning autosave off deletes what autosave stored. It must not delete the
  // words the user added: they are not a draft, and there is no other copy.
  await drafts.clear();
  assert.deepEqual(await drafts.listMeta(), []);
  assert.deepEqual(
    await words.list(),
    ["QZXword"],
    "the personal dictionary is not autosave's to throw away",
  );
});

test("a browser with no IndexedDB is reported, not silently ignored", async () => {
  await assert.rejects(
    () => openWordStore({ indexedDB: null }),
    /no IndexedDB/,
    "a word the user added that was not stored has to be reportable",
  );
});
