// A small IndexedDB good enough to hold this repository's stores honestly, so
// that storage POLICY can be driven in node — including the failure paths a real
// browser will not produce on request.
//
// Why this exists rather than a dependency: the repo ships no test-only npm
// packages (`package.json` has one devDependency, Playwright), and the three
// behaviours the version store's guarantees rest on are exactly the ones a
// hand-rolled fake can get right and a mock usually gets wrong:
//
//   1. **A transaction is atomic.** Writes go to an overlay and are applied to
//      the backing maps only when the transaction COMPLETES. An abort discards
//      them. Without that, an "atomic restore" test would pass over a store that
//      wrote each put immediately.
//   2. **A failed request aborts its transaction**, with the error surfacing on
//      the transaction — which is how a real `QuotaExceededError` on an
//      unawaited `put` reaches the caller.
//   3. **Reads inside a transaction see that transaction's own writes**, so a
//      sweep that deletes rows and then counts references counts what will
//      actually remain.
//
// It is deliberately NOT the whole API: `openKeyCursor`, versionchange events and
// key ranges are absent because nothing here uses them. The same store code is
// also exercised against real IndexedDB in
// `tests/e2e/version-history-store.spec.mjs`, which is what keeps this fake from
// becoming the only thing the guards agree with.

const TOMBSTONE = Symbol("deleted");

class FakeRequest {
  constructor(tx, run) {
    this.onsuccess = null;
    this.onerror = null;
    this.result = undefined;
    this.error = null;
    tx?.enqueue(this, run);
  }
}

class FakeIndex {
  constructor(store, keyPath) {
    this.store = store;
    this.keyPath = keyPath;
  }

  matching(key) {
    return this.store.entries().filter(([, value]) => value?.[this.keyPath] === key);
  }

  getAll(key) {
    return new FakeRequest(this.store.tx, () => this.matching(key).map(([, value]) => value));
  }

  getAllKeys(key) {
    return new FakeRequest(this.store.tx, () => this.matching(key).map(([recordKey]) => recordKey));
  }
}

class FakeObjectStore {
  constructor(tx, name, definition) {
    this.tx = tx;
    this.name = name;
    this.definition = definition;
  }

  /** Base records with this transaction's own pending writes layered over. */
  entries() {
    const overlay = this.tx.overlay.get(this.name) ?? new Map();
    const merged = new Map(this.definition.records);
    for (const [key, value] of overlay) {
      if (value === TOMBSTONE) merged.delete(key);
      else merged.set(key, value);
    }
    return [...merged.entries()];
  }

  read(key) {
    const overlay = this.tx.overlay.get(this.name);
    if (overlay?.has(key)) {
      const staged = overlay.get(key);
      return staged === TOMBSTONE ? undefined : staged;
    }
    return this.definition.records.get(key);
  }

  stage(key, value) {
    if (!this.tx.overlay.has(this.name)) this.tx.overlay.set(this.name, new Map());
    this.tx.overlay.get(this.name).set(key, value);
  }

  keyFor(value, explicitKey) {
    if (this.definition.keyPath) return value?.[this.definition.keyPath];
    return explicitKey;
  }

  get(key) {
    return new FakeRequest(this.tx, () => this.read(key));
  }

  getAll() {
    return new FakeRequest(this.tx, () => this.entries().map(([, value]) => value));
  }

  getAllKeys() {
    return new FakeRequest(this.tx, () => this.entries().map(([key]) => key));
  }

  count() {
    return new FakeRequest(this.tx, () => this.entries().length);
  }

  put(value, explicitKey) {
    return new FakeRequest(this.tx, () => {
      this.tx.db.failures?.beforePut?.(this.name, value);
      this.stage(this.keyFor(value, explicitKey), value);
      return this.keyFor(value, explicitKey);
    });
  }

  delete(key) {
    return new FakeRequest(this.tx, () => {
      this.stage(key, TOMBSTONE);
      return undefined;
    });
  }

  clear() {
    return new FakeRequest(this.tx, () => {
      for (const [key] of this.entries()) this.stage(key, TOMBSTONE);
      return undefined;
    });
  }

  index(name) {
    const keyPath = this.definition.indexes.get(name);
    if (!keyPath) throw new Error(`no index ${name} on ${this.name}`);
    return new FakeIndex(this, keyPath);
  }
}

class FakeTransaction {
  constructor(db, names, mode) {
    this.db = db;
    this.names = names;
    this.mode = mode;
    this.overlay = new Map();
    this.pending = 0;
    this.finished = false;
    this.error = null;
    this.oncomplete = null;
    this.onerror = null;
    this.onabort = null;
    db.openTransactions += 1;
  }

  objectStore(name) {
    if (!this.names.includes(name)) throw new Error(`${name} is not in this transaction`);
    const definition = this.db.stores.get(name);
    if (!definition) throw new Error(`no object store ${name}`);
    return new FakeObjectStore(this, name, definition);
  }

  enqueue(request, run) {
    this.pending += 1;
    // Counted so a test can assert how much store work an operation costs — the
    // O(1)-per-tick claim is about the NUMBER of requests, and counting them is
    // the only way to assert it that a faster machine cannot fake.
    this.db.requests += 1;
    queueMicrotask(() => {
      if (this.finished) return;
      try {
        request.result = run();
        request.onsuccess?.({ target: request });
      } catch (err) {
        request.error = err;
        this.error = err;
        request.onerror?.({ target: request });
        this.abort(err);
        return;
      } finally {
        this.pending -= 1;
      }
      this.settleLater();
    });
  }

  /** A real transaction commits when the event loop turns with no request
   *  outstanding. Checking on a macrotask reproduces that: a caller awaiting one
   *  request resumes in a microtask and can issue the next one in time, while a
   *  caller that awaits something else — a digest, a fetch — loses the
   *  transaction, exactly as in a browser. */
  settleLater() {
    setTimeout(() => {
      if (this.finished || this.pending > 0) return;
      this.commit();
    }, 0);
  }

  commit() {
    if (this.finished) return;
    this.finished = true;
    this.db.openTransactions -= 1;
    for (const [storeName, overlay] of this.overlay) {
      const definition = this.db.stores.get(storeName);
      for (const [key, value] of overlay) {
        if (value === TOMBSTONE) definition.records.delete(key);
        else definition.records.set(key, value);
      }
    }
    this.oncomplete?.({});
  }

  abort(error = null) {
    if (this.finished) return;
    this.finished = true;
    this.db.openTransactions -= 1;
    this.overlay.clear(); // nothing this transaction wrote survives
    this.error = error ?? this.error;
    this.onerror?.({});
    this.onabort?.({});
  }
}

class FakeDatabase {
  constructor(name) {
    this.name = name;
    this.version = 0;
    this.stores = new Map();
    this.openTransactions = 0;
    /** Every request ever issued against this database. */
    this.requests = 0;
    /** Injected failures: `{beforePut(storeName, value)}` may throw. */
    this.failures = null;
    this.objectStoreNames = {
      contains: (name) => this.stores.has(name),
    };
  }

  createObjectStore(name, options = {}) {
    const definition = {
      keyPath: options.keyPath ?? null,
      records: new Map(),
      indexes: new Map(),
    };
    this.stores.set(name, definition);
    return {
      createIndex: (indexName, keyPath) => definition.indexes.set(indexName, keyPath),
    };
  }

  transaction(names, mode = "readonly") {
    return new FakeTransaction(this, Array.isArray(names) ? names : [names], mode);
  }

  close() {}
}

/**
 * A fake `indexedDB` factory. `databases` is shared across `open` calls, so two
 * opens of one name see one store — which is what makes a schema-upgrade test
 * and a two-connection test possible.
 */
export function fakeIndexedDB() {
  const databases = new Map();
  return {
    databases,
    open(name, version) {
      const request = { onupgradeneeded: null, onsuccess: null, onerror: null, onblocked: null };
      let db = databases.get(name);
      if (!db) {
        db = new FakeDatabase(name);
        databases.set(name, db);
      }
      queueMicrotask(() => {
        request.result = db;
        if (version > db.version) {
          db.version = version;
          request.onupgradeneeded?.({ target: request, oldVersion: db.version });
        }
        request.onsuccess?.({ target: request });
      });
      return request;
    },
  };
}

/** A `DOMException`-shaped quota error: the property the production code reads
 *  is `name`, and it reads it because that is what browsers set. */
export function quotaError() {
  const err = new Error("quota exceeded");
  err.name = "QuotaExceededError";
  return err;
}
