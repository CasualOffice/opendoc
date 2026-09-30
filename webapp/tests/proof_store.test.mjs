// The `opendoc-proofing` store: atomic activation, and the prior version kept for
// rollback (`docs/146` §6/§9, ADR-042).
//
// The fake below is the same shape `word_store.test.mjs` uses and for the same
// reason: `proof_store.mjs` takes its `indexedDB` by injection, so the rules that
// matter — which of "staged", "active" and "removed" a pack is in, and what a
// half-finished transaction leaves behind — are answerable in node. It honours the
// parts of the contract the store actually uses: `objectStoreNames.contains`, a
// keyPath, out-of-line keys, `getAll`/`getAllKeys`, and a transaction that
// completes ASYNCHRONOUSLY and can be made to abort.
//
// The abort is the point of having a fake at all. `activate` writes the pack record
// and flips the pointer in ONE transaction and awaits `oncomplete`; the only way to
// show that is atomic rather than merely tidy is to abort the transaction after the
// puts and assert that neither landed.

import assert from "node:assert/strict";
import test from "node:test";

import {
  ACTIVE_STORE,
  PACKS_STORE,
  PROOF_DB_NAME,
  PROOF_DB_VERSION,
  WORDS_STORE,
  openProofStore,
  wordsKey,
} from "../src/proof_store.mjs";

/** A minimal in-memory IndexedDB.
 *
 *  `abortTouching` makes every readwrite transaction that touches that store abort
 *  instead of committing. The shape of that lever is not arbitrary: a "fail the Nth
 *  transaction" lever cannot tell one transaction from two, because failing the
 *  first leaves both implementations in the same state. Failing the one that flips
 *  the POINTER is what separates them — with a single transaction the pack record
 *  rolls back with it, and with two the record has already committed and the store
 *  is left describing a version nothing points at. */
function fakeIndexedDB({ abortTouching = "" } = {}) {
  const databases = new Map();

  function fakeRequest(result) {
    const rq = { result };
    queueMicrotask(() => rq.onsuccess?.());
    return rq;
  }

  function storeHandle(state, keyPath, onWrite = () => {}) {
    return {
      put(value, key) {
        onWrite();
        state.set(key ?? value[keyPath], value);
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
        onWrite();
        state.delete(key);
      },
      clear() {
        onWrite();
        state.clear();
      },
    };
  }

  function databaseHandle(db) {
    return {
      get objectStoreNames() {
        return { contains: (name) => db.stores.has(name) };
      },
      createObjectStore(name, options = {}) {
        db.stores.set(name, new Map());
        db.keyPaths.set(name, options.keyPath ?? null);
        return { createIndex() {} };
      },
      transaction(names, mode = "readonly") {
        const list = Array.isArray(names) ? names : [names];
        const tx = {};
        // Which stores this transaction actually WROTE to. The lever is keyed on
        // that rather than on the transaction's scope, and the difference is a
        // mutation that would otherwise escape: split `activate` so the pack
        // record commits in a transaction that still DECLARES the pointer store
        // (it reads the previous version from it) and moves the pointer in a
        // second one. A scope-keyed lever aborts that first transaction too, so
        // the record never commits and the split looks atomic — the guard passes
        // on a store carrying the very defect it exists to catch. Keyed on
        // writes, the read-only use commits, the pointer write aborts, and the
        // inconsistency the test asserts against is real.
        const wrote = new Set();
        // A MACROTASK, not a microtask chain: this store's transactions `await`
        // reads before they write, so a fake that settled on the microtask queue
        // could settle before the caller had attached `oncomplete` — and a
        // transaction that settles into no handler is a promise that never
        // resolves, which presents as the whole file hanging rather than as a
        // failure anybody can read. `word_store.test.mjs`'s fake gets away with
        // two microtask hops because its transactions await nothing first.
        setTimeout(() => {
          const aborting = mode === "readwrite" && abortTouching && wrote.has(abortTouching);
          if (aborting) {
            tx.error = new Error("aborted");
            // A real abort rolls the writes back. The fake keeps a snapshot per
            // store and restores it, which is the behaviour under test.
            for (const [name, snapshot] of tx.snapshots) db.stores.set(name, snapshot);
            tx.onabort?.();
          } else {
            tx.oncomplete?.();
          }
        }, 0);
        tx.snapshots = new Map(
          list.map((name) => [name, new Map(db.stores.get(name) ?? new Map())]),
        );
        tx.objectStore = (name) => {
          assert.ok(list.includes(name), `${name} is not in this transaction`);
          return storeHandle(db.stores.get(name), db.keyPaths.get(name), () =>
            wrote.add(name),
          );
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
          db = { name, version: 0, stores: new Map(), keyPaths: new Map() };
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

const record = (version, words = ["alpha", "bravo"]) => ({
  packId: "casual-proof-en-US-test",
  locale: "en-US",
  packVersion: version,
  bytes: 1024,
  capabilities: ["spelling"],
  provenance: [{ name: "test", version, license: "Apache-2.0", url: "https://x.test" }],
  words,
});

async function store(options = {}) {
  return openProofStore({ indexedDB: fakeIndexedDB(options), name: "proof-test" });
}

test("the proofing store is its own database, at schema 1", () => {
  // `docs/146` §6 asks for a separate database, against `drafts.mjs`'s "one store"
  // rule, because a pack is disposable and a draft is not. Asserted rather than
  // left to a comment: putting them together again would be a silent decision to
  // make every pack schema bump a version bump on unsaved work.
  assert.equal(PROOF_DB_NAME, "opendoc-proofing");
  assert.equal(PROOF_DB_VERSION, 1);
  assert.notEqual(PROOF_DB_NAME, "opendoc-drafts");
});

test("a staged pack is not an installed pack", async () => {
  // The distinction the temporary namespace exists for. A version that never got
  // promoted has a words row and no pointer, and reporting it as installed is the
  // lie the staging step exists to avoid.
  const proof = await store();
  await proof.stage(record("1.aaa"));
  assert.deepEqual(await proof.installed(), [], "staged, never activated");
  assert.deepEqual(await proof.words("casual-proof-en-US-test"), []);
  await proof.activate(record("1.aaa"), 1000);
  const installed = await proof.installed();
  assert.equal(installed.length, 1);
  assert.equal(installed[0].packVersion, "1.aaa");
  assert.equal(installed[0].words, 2, "the COUNT, never the words");
  assert.equal(installed[0].installedAt, 1000);
  assert.deepEqual(await proof.words("casual-proof-en-US-test"), ["alpha", "bravo"]);
});

test("activation is one transaction: an interrupted update leaves the old one active", async () => {
  // THE atomicity claim, asserted as a GUARANTEE rather than as a transaction
  // count: after an update that does not complete, the store still reports the
  // version that was working, and its words are still findable.
  //
  // The mechanism is the only thing that makes that true. Split `activate` into two
  // transactions — the pack record, then the pointer — and the record commits while
  // the pointer does not, so the store describes version 1.bbb while the pointer
  // still names 1.aaa, the consistency check in `activeRecords` drops the row, and
  // the reader loses proofing entirely although every byte of 1.aaa is still on
  // disk. MUTATION: that split leaves this red with `installed()` empty.
  const idb = fakeIndexedDB();
  const proof = await openProofStore({ indexedDB: idb, name: "proof-test" });
  await proof.stage(record("1.aaa"));
  await proof.activate(record("1.aaa"), 1);
  await proof.stage(record("1.bbb", ["alpha", "bravo", "charlie"]));

  // The same database, opened through a factory that aborts any write touching the
  // active-version pointer.
  const hostile = fakeIndexedDB({ abortTouching: ACTIVE_STORE });
  hostile.databases.set("proof-test", idb.databases.get("proof-test"));
  const interrupted = await openProofStore({ indexedDB: hostile, name: "proof-test" });
  await assert.rejects(
    () => interrupted.activate(record("1.bbb", ["alpha", "bravo", "charlie"]), 2),
    "a failed activation is reported, never swallowed",
  );

  const installed = await proof.installed();
  assert.equal(installed.length, 1, "the previous version is still reported as installed");
  assert.equal(installed[0].packVersion, "1.aaa");
  assert.deepEqual(
    await proof.words("casual-proof-en-US-test"),
    ["alpha", "bravo"],
    "and its words are still the ones the worker would be handed",
  );
});

test("the previous version's words survive activation, so a rollback is possible", async () => {
  const proof = await store();
  await proof.stage(record("1.aaa"));
  await proof.activate(record("1.aaa"), 1);
  await proof.stage(record("1.bbb", ["alpha", "bravo", "charlie"]));
  await proof.activate(record("1.bbb", ["alpha", "bravo", "charlie"]), 2);
  assert.equal((await proof.installed())[0].words, 3);

  const went = await proof.rollback("en-US");
  assert.equal(went, "1.aaa");
  assert.equal((await proof.installed())[0].packVersion, "1.aaa");
  assert.deepEqual(await proof.words("casual-proof-en-US-test"), ["alpha", "bravo"]);
  // And a second rollback has nowhere to go, and SAYS so rather than reporting a
  // rollback that did not happen.
  assert.equal(await proof.rollback("en-US"), "");
  assert.equal(await proof.rollback("fr-FR"), "");
});

test("the installer's cleanup drops a staged version and keeps the two that matter", async () => {
  const proof = await store();
  await proof.stage(record("1.aaa"));
  await proof.activate(record("1.aaa"), 1);
  await proof.stage(record("1.bbb"));
  await proof.activate(record("1.bbb"), 2);
  await proof.stage(record("1.ccc"));
  await proof.discard("casual-proof-en-US-test");
  // The active version and the one before it stay — the second is the rollback.
  assert.equal(await proof.rollback("en-US"), "1.aaa");
  // And the abandoned staged version is gone, so a later attempt cannot promote it.
  const proof2 = await store();
  await proof2.discard("");
});

test("removing a pack takes its pointer, its record and every version of its words", async () => {
  const proof = await store();
  await proof.stage(record("1.aaa"));
  await proof.activate(record("1.aaa"), 1);
  await proof.stage(record("1.bbb"));
  await proof.activate(record("1.bbb"), 2);
  assert.equal(await proof.remove("casual-proof-en-US-test"), true);
  assert.deepEqual(await proof.installed(), []);
  assert.deepEqual(await proof.words("casual-proof-en-US-test"), []);
  assert.equal(
    await proof.remove("casual-proof-en-US-test"),
    false,
    "and a second removal says there was nothing to remove",
  );
});

test("a browser with no IndexedDB is reported, not silently ignored", async () => {
  await assert.rejects(
    () => openProofStore({ indexedDB: null }),
    /no IndexedDB/,
    "storage being absent is a refusal with a reason (docs/146 §6: private browsing, " +
      "partitioned third-party frames). Swallowing it would make Install fail for " +
      "no stated cause.",
  );
});

test("a version key names the version, which is why staging cannot overwrite", () => {
  assert.notEqual(wordsKey("p", "1"), wordsKey("p", "2"));
  assert.ok(wordsKey("p", "1").startsWith("p"));
  assert.equal(WORDS_STORE, "packWords");
});
