// The version store against REAL IndexedDB — HF-068 / OO-004, docs/139-140,
// ADR-038.
//
// Why this exists beside `tests/version_history.test.mjs`: the node suite drives
// the policy through a hand-written fake, and a fake is a model of IndexedDB
// rather than IndexedDB. Every claim in that file that depends on the STORE
// rather than on the policy — an upgrade that keeps existing drafts, a
// transaction that is genuinely atomic when one of its writes fails, a blob
// released only at reference count zero — is therefore re-asserted here against
// the browser's own implementation. If the two ever disagree, the browser is
// right and the fake is the bug.
//
// The page is `docs.html` rather than the editor for three of the four tests:
// the store is a module, it needs an origin and nothing else, and booting a
// 9 MB engine to exercise an IndexedDB transaction would make these tests slow
// and flaky for no gain. The fourth test DOES boot the editor, because the one
// thing only the editor can supply is a real document's real export bytes — and
// the size of those bytes is what the whole retention policy is sized against.
import { test, expect, clickIntoFirstPage, waitForFramedEditor } from "./fixtures.mjs";

/** A unique database per test, so two tests cannot see each other's rows and a
 *  retry cannot inherit the previous run's timeline. */
const dbName = (suffix) => `opendoc-vh-e2e-${suffix}-${Date.now()}`;

/** The wrapper both failure-injection tests use, defined in page scope.
 *
 *  It hands `openHistoryStore` an `indexedDB` whose `put` throws — and aborts
 *  its transaction, as a real quota failure does — for the store and value a
 *  predicate names. Nothing test-shaped is compiled into the production module;
 *  the seam is the `indexedDB` injection it already has for node. */
const WRAPPER_SOURCE = `
  function wrapStore(tx, store, name, fails) {
    return {
      index: (i) => store.index(i),
      get: (k) => store.get(k),
      getAll: (k) => store.getAll(k),
      getAllKeys: (k) => store.getAllKeys(k),
      count: () => store.count(),
      delete: (k) => store.delete(k),
      clear: () => store.clear(),
      put: (value, key) => {
        if (fails(name, value)) {
          tx.abort();
          throw new DOMException("out of room", "QuotaExceededError");
        }
        return key === undefined ? store.put(value) : store.put(value, key);
      },
    };
  }
  function wrapDb(db, fails) {
    return {
      objectStoreNames: db.objectStoreNames,
      createObjectStore: (n, o) => db.createObjectStore(n, o),
      transaction: (names, mode) => {
        const tx = db.transaction(names, mode);
        return {
          get error() { return tx.error; },
          set oncomplete(fn) { tx.oncomplete = fn; },
          set onerror(fn) { tx.onerror = fn; },
          set onabort(fn) { tx.onabort = fn; },
          objectStore: (n) => wrapStore(tx, tx.objectStore(n), n, fails),
        };
      },
      close: () => db.close(),
    };
  }
  function wrappedIndexedDB(fails) {
    return {
      open(name, version) {
        const real = indexedDB.open(name, version);
        const shim = {};
        real.onupgradeneeded = (event) => {
          shim.result = wrapDb(real.result, fails);
          shim.onupgradeneeded?.(event);
        };
        real.onsuccess = () => {
          shim.result = wrapDb(real.result, fails);
          shim.onsuccess?.();
        };
        real.onerror = () => { shim.error = real.error; shim.onerror?.(); };
        real.onblocked = () => shim.onblocked?.();
        return shim;
      },
    };
  }
`;

test("a version-1 database upgrades to schema 3 with its drafts intact", async ({ page }) => {
  await page.goto("/docs.html");
  const outcome = await page.evaluate(async (name) => {
    // A tab on the OLD build: version 1, the two draft stores, one draft in them.
    await new Promise((resolve, reject) => {
      const rq = indexedDB.open(name, 1);
      rq.onupgradeneeded = () => {
        rq.result.createObjectStore("meta", { keyPath: "slotId" });
        rq.result.createObjectStore("bytes");
      };
      rq.onsuccess = () => {
        const db = rq.result;
        const tx = db.transaction(["meta", "bytes"], "readwrite");
        tx.objectStore("meta").put({ slotId: "slot-1", name: "work.docx", bytes: 3 });
        tx.objectStore("bytes").put(new Uint8Array([1, 2, 3]), "slot-1");
        tx.oncomplete = () => {
          db.close();
          resolve();
        };
        tx.onerror = () => reject(tx.error);
      };
      rq.onerror = () => reject(rq.error);
    });

    // The new build opens the same database. Nothing else runs the migration.
    const { openHistoryStore } = await import("/src/version_history.mjs");
    const { openDraftStore } = await import("/src/drafts.mjs");
    const history = await openHistoryStore({ name });
    const drafts = await openDraftStore({ name });
    const meta = await drafts.readMeta("slot-1");
    const bytes = await drafts.readBytes("slot-1");
    const status = await history.storageStatus();
    const names = await new Promise((resolve) => {
      const rq = indexedDB.open(name);
      rq.onsuccess = () => {
        const list = [...rq.result.objectStoreNames];
        rq.result.close();
        resolve(list);
      };
    });
    history.close();
    drafts.close();
    return { meta, bytes: [...(bytes ?? [])], names, versions: status.versions };
  }, dbName("upgrade"));

  expect(outcome.names.sort()).toEqual([
    "bytes",
    "checkpoint_blobs",
    "documents",
    "history_ops",
    "meta",
    "version_meta",
    "words",
  ]);
  // The pre-upgrade draft is the point: a `createObjectStore` sweep would have
  // thrown away the user's unsaved work on the first reload after the deploy.
  expect(outcome.meta).toMatchObject({ slotId: "slot-1", name: "work.docx" });
  expect(outcome.bytes).toEqual([1, 2, 3]);
  expect(outcome.versions).toBe(0);
});

test("forty captures leave the cap, oldest first, and never the named one", async ({ page }) => {
  await page.goto("/docs.html");
  const outcome = await page.evaluate(async (name) => {
    const { openHistoryStore, resolveRetention } = await import("/src/version_history.mjs");
    const { DEFAULT_SETTINGS } = await import("/src/settings_defaults.mjs");
    const retention = resolveRetention(DEFAULT_SETTINGS);
    const store = await openHistoryStore({ name });
    const { lineageId } = await store.openLineage({ docKey: "k", name: "report.docx" });

    const at = Date.UTC(2026, 8, 1);
    let firstVersionId = null;
    for (let i = 0; i < 40; i++) {
      const bytes = new Uint8Array(4096);
      bytes.fill(i % 251);
      const written = await store.captureVersion({
        lineageId,
        bytes,
        formatId: "org.openxmlformats.wordprocessingml.document",
        exportMode: "preserve_when_safe",
        revision: i + 1,
        now: at + i * 60_000,
        retention,
      });
      if (!written.ok) return { failed: written.status, at: i };
      if (i === 0) firstVersionId = written.version.versionId;
      // Name the third version: it is old enough that the count ceiling would
      // reach it, which is the whole point of naming it.
      if (i === 2) {
        const named = await store.nameVersion(written.version.versionId, "Before the rewrite", {
          pinLimit: retention.pinLimit,
        });
        if (!named.ok) return { failed: named.status };
      }
    }

    const rows = await store.listVersions(lineageId);
    const blobs = await new Promise((resolve) => {
      const rq = indexedDB.open(name);
      rq.onsuccess = () => {
        const db = rq.result;
        const tx = db.transaction("checkpoint_blobs", "readonly");
        const count = tx.objectStore("checkpoint_blobs").count();
        count.onsuccess = () => {
          db.close();
          resolve(count.result);
        };
      };
    });
    const status = await store.storageStatus();
    store.close();
    return {
      kept: rows.length,
      cap: retention.maxCount,
      named: rows.filter((row) => row.pinned).map((row) => row.name),
      firstStillThere: rows.some((row) => row.versionId === firstVersionId),
      oldestRevision: Math.min(...rows.map((row) => row.revision)),
      blobs,
      bytes: status.bytes,
      quotaKnown: typeof status.quota === "number",
    };
  }, dbName("retention"));

  expect(outcome.failed).toBeUndefined();
  expect(outcome.kept).toBe(outcome.cap);
  expect(outcome.named).toEqual(["Before the rewrite"]);
  expect(outcome.firstStillThere).toBe(false);
  // The named version is revision 3; everything older than it went, and it
  // stayed — which is the guarantee naming makes.
  expect(outcome.oldestRevision).toBe(3);
  // A pruned version's artifact goes with it. An orphan blob IS the quota
  // problem this policy exists to bound.
  expect(outcome.blobs).toBe(outcome.cap);
  expect(outcome.bytes).toBe(outcome.cap * 4096);
  expect(outcome.quotaKnown).toBe(true);
});

test("a restore killed mid-commit leaves the old head, and resumes once", async ({ page }) => {
  await page.goto("/docs.html");
  const outcome = await page.evaluate(
    async ({ name, wrapper }) => {
      // eslint-disable-next-line no-new-func
      const { wrappedIndexedDB } = new Function(`${wrapper}; return { wrappedIndexedDB };`)();
      const { openHistoryStore, resolveRetention } = await import("/src/version_history.mjs");
      const { DEFAULT_SETTINGS } = await import("/src/settings_defaults.mjs");
      const retention = resolveRetention(DEFAULT_SETTINGS);

      // Phase 1: three ordinary versions, written normally.
      const store = await openHistoryStore({ name });
      const { lineageId } = await store.openLineage({ docKey: "k", name: "report.docx" });
      const at = Date.UTC(2026, 8, 2);
      const written = [];
      for (let i = 0; i < 3; i++) {
        const bytes = new Uint8Array(2048);
        bytes.fill(i + 1);
        const row = await store.captureVersion({
          lineageId,
          bytes,
          formatId: "docx",
          revision: i + 1,
          now: at + i * 60_000,
          retention,
        });
        written.push(row.version);
      }
      const target = written[0];
      store.close();

      // Phase 2: the same store, but the browser will refuse the restore row's
      // write — which is what a tab killed between the two writes looks like
      // from inside the transaction.
      const failing = await openHistoryStore({
        name,
        indexedDB: wrappedIndexedDB(
          (storeName, value) => storeName === "version_meta" && value?.kind === "restore",
        ),
      });
      const current = new Uint8Array(1500);
      current.fill(9);
      const prepared = await failing.prepareRestore({
        lineageId,
        versionId: target.versionId,
        idempotencyKey: "e2e-restore",
        current: { bytes: current, formatId: "docx", revision: 77 },
        retention,
        now: at + 10 * 60_000,
      });
      if (!prepared.ok) return { failed: prepared.status };
      const headAfterPrepare = await failing.head(lineageId);
      const rowsAfterPrepare = (await failing.listVersions(lineageId)).length;
      let thrown = null;
      try {
        await failing.commitRestore({ opId: prepared.operation.opId, retention });
      } catch (err) {
        thrown = String(err?.name ?? err);
      }
      const headAfterFailure = await failing.head(lineageId);
      const rowsAfterFailure = (await failing.listVersions(lineageId)).length;
      const preRestore = await failing.getVersion(prepared.operation.preRestoreVersionId);
      failing.close();

      // Phase 3: a healthy store, as the next boot would open it.
      const healthy = await openHistoryStore({ name });
      const pending = await healthy.resolvePendingRestores({ now: at + 11 * 60_000 });
      const retried = await healthy.commitRestore({
        opId: prepared.operation.opId,
        retention,
        now: at + 12 * 60_000,
      });
      const again = await healthy.commitRestore({ opId: prepared.operation.opId, retention });
      const finalRows = await healthy.listVersions(lineageId);
      const restoredBytes = retried.bytes ? [...retried.bytes.slice(0, 3)] : null;
      healthy.close();

      return {
        thrown,
        headHeld: headAfterFailure === headAfterPrepare,
        rowsAfterPrepare,
        rowsAfterFailure,
        preRestoreKept: Boolean(preRestore) && preRestore.kind === "pre_restore",
        preRestoreBytes: preRestore?.bytes ?? null,
        resumable: pending.resumable.length,
        retriedOk: retried.ok,
        restoredBytes,
        finalHeadMatches: finalRows[0]?.versionId === retried.version?.versionId,
        secondCommitStatus: again.status,
        finalCount: finalRows.length,
        targetStillThere: finalRows.some((row) => row.versionId === target.versionId),
      };
    },
    { name: dbName("restore"), wrapper: WRAPPER_SOURCE },
  );

  expect(outcome.failed).toBeUndefined();
  expect(outcome.thrown).toBe("QuotaExceededError");
  // The two halves of "never let a restore leave the work in neither place":
  // the head did not move, and the pre-restore capture is a real version.
  expect(outcome.headHeld).toBe(true);
  expect(outcome.rowsAfterFailure).toBe(outcome.rowsAfterPrepare);
  expect(outcome.preRestoreKept).toBe(true);
  expect(outcome.preRestoreBytes).toBe(1500);
  // And it is resolvable rather than guessed at.
  expect(outcome.resumable).toBe(1);
  expect(outcome.retriedOk).toBe(true);
  expect(outcome.finalHeadMatches).toBe(true);
  expect(outcome.restoredBytes).toEqual([1, 1, 1]);
  expect(outcome.secondCommitStatus).toBe("history.restoreCommitted");
  // 3 originals + pre-restore + restore = 5, with the target untouched: a
  // restore appends and never rewinds.
  expect(outcome.finalCount).toBe(5);
  expect(outcome.targetStillThere).toBe(true);
});

/**
 * Types into whatever document the editor opened, waits for autosave to put its
 * artifact on disk, then captures a version FROM THOSE BYTES and measures it.
 *
 * The point of driving the real editor for this: the size of a stored version is
 * the size of the document's own export, and that number is what the retention
 * ceilings are sized against. Anything else would be a number about a fixture
 * this test made up.
 */
async function measureVersionOfOpenDocument(page, { url }) {
  await page.goto(url);
  await waitForFramedEditor(page.mainFrame());
  await clickIntoFirstPage(page);
  await page.keyboard.type("VERSIONHISTORYMEASURE");
  // Wait for the product's own autosave indicator rather than for a clock: the
  // draft it writes is the artifact a version is made of.
  await expect(page.locator("#draftStatus")).toContainText(/Draft saved/, { timeout: 30_000 });

  const measured = await page.evaluate(async () => {
    const { openDraftStore, documentKey } = await import("/src/drafts.mjs");
    const { openHistoryStore, resolveRetention, checkpointIdFor } = await import(
      "/src/version_history.mjs"
    );
    const { DEFAULT_SETTINGS } = await import("/src/settings_defaults.mjs");
    const drafts = await openDraftStore({});
    const metas = await drafts.listMeta();
    const meta = metas.sort((a, b) => (b.savedAt ?? 0) - (a.savedAt ?? 0))[0];
    const bytes = await drafts.readBytes(meta.slotId);

    const retention = resolveRetention(DEFAULT_SETTINGS);
    const history = await openHistoryStore({});
    const { lineageId } = await history.openLineage({
      docKey: documentKey(meta.name, bytes),
      name: meta.name,
      fresh: true,
    });
    const started = performance.now();
    const written = await history.captureVersion({
      lineageId,
      bytes,
      formatId: meta.formatId,
      exportMode: meta.exportMode,
      findings: meta.findings,
      revision: meta.revision,
      engine: meta.engine,
      retention,
      kind: "saved",
    });
    const elapsed = performance.now() - started;
    const readBack = await history.readCheckpoint(written.version.checkpointId);
    const hash = await checkpointIdFor(bytes);
    const listed = await history.listVersions(lineageId);
    await history.clearHistory({ lineageId });
    history.close();
    drafts.close();
    return {
      ok: written.ok,
      name: meta.name,
      formatId: written.version.formatId,
      exportMode: written.version.exportMode,
      draftBytes: bytes.length,
      versionBytes: written.version.bytes,
      storedBytes: readBack.bytes?.length ?? 0,
      identical: readBack.bytes?.length === bytes.length && readBack.bytes[0] === bytes[0],
      hashMatches: written.version.checkpointId === hash,
      metadataRow: JSON.stringify(listed[0]).length,
      captureMs: Math.round(elapsed),
    };
  });
  return measured;
}

/** Asserts the physical shape of a stored version, whatever the document. */
function expectVersionIsTheSaveArtifact(measured) {
  expect(measured.ok).toBe(true);
  // What a stored version physically is: the document's own source-format
  // artifact — the same bytes autosave wrote, verified by hash — plus a few
  // hundred bytes of metadata row.
  expect(measured.formatId).toBe("org.openxmlformats.wordprocessingml.document");
  expect(measured.versionBytes).toBe(measured.draftBytes);
  expect(measured.storedBytes).toBe(measured.draftBytes);
  expect(measured.identical).toBe(true);
  expect(measured.hashMatches).toBe(true);
  expect(measured.metadataRow).toBeLessThan(1024);
  // Annotated so the number comes from a run rather than from memory
  // (SKILL §9.1). A version IS this artifact, so docs/112 §3.1's measurements of
  // it — 1,012,199 B for a 14-page real producer file, 2,375,316 B for a
  // 471-page one — are the scale the retention ceilings are sized against.
  test.info().annotations.push({
    type: "measured",
    description:
      `${measured.name}: artifact ${measured.draftBytes} B, metadata row ` +
      `${measured.metadataRow} B, capture ${measured.captureMs} ms ` +
      `(${measured.exportMode})`,
  });
}

test("a version of the demo document is its own Save artifact, byte for byte", async ({ page }) => {
  test.setTimeout(120_000);
  expectVersionIsTheSaveArtifact(
    await measureVersionOfOpenDocument(page, { url: "/editor.html?fixture=rich" }),
  );
});

test("and so is a version of the larger sample document", async ({ page }) => {
  // The plain editor opens `sample.docx`, the real-producer file docs/112
  // measured. Two documents rather than one because the claim is about the
  // artifact, not about a fixture: if capture ever started re-exporting or
  // re-encoding, the larger document is where the two sizes would part company.
  test.setTimeout(120_000);
  expectVersionIsTheSaveArtifact(
    await measureVersionOfOpenDocument(page, { url: "/editor.html" }),
  );
});
