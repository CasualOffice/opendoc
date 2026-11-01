// The OCR worker's protocol (`ocr_worker_core.mjs`, `docs/182` §17a.5) with a
// fake PDF module and fake model store: the engine is loaded once and reused
// (the preload's whole point), a failed load is forgotten, every page is
// reported, the layout is asked only once the pages are read, and the package
// goes back transferred.
import assert from "node:assert/strict";
import test from "node:test";

import { createOcrWorkerCore } from "../src/ocr_worker_core.mjs";

const URL_A = "https://host.example/ocr/manifest.json";

function harness({ engineFails = 0 } = {}) {
  const posts = [];
  const calls = { obtain: 0, load: 0, begin: 0, plain: 0, finish: [] };
  let failures = engineFails;
  const glue = {
    setFaceHost(host) {
      glue.host = host;
    },
    PdfOcrModels: class {
      addPack() {}
    },
    PdfOcrEngine: {
      load() {
        calls.load += 1;
        if (failures > 0) {
          failures -= 1;
          throw new Error("the engine refused the models");
        }
        return { engine: true };
      },
    },
    beginPdfImportWithOcr(bytes, engine) {
      calls.begin += 1;
      assert.ok(engine.engine);
      return glue.job();
    },
    beginPdfImport() {
      calls.plain += 1;
      return glue.job();
    },
    job() {
      let done = 0;
      return {
        scannedPages: 2,
        missingCMaps: [],
        pageCount: 3,
        get pagesDone() {
          return done;
        },
        step() {
          done += 1;
          return done < 3 ? "pending" : "done";
        },
        finish() {
          calls.finish.push("fixed");
          return new Uint8Array([1]);
        },
        finishReflowed() {
          calls.finish.push("flowing");
          return new Uint8Array([2]);
        },
        ocrSummaryJson: JSON.stringify({ provider: "pp-ocrv5" }),
        free() {},
      };
    },
  };
  const models = {
    fetchOcrManifest: async () => ({ bytes: 10, detection: {}, packs: [] }),
    openOcrStore: async () => ({ get: async () => null, put: async () => {} }),
    modelsStored: async () => true,
    obtainModels: async (manifest, io) => {
      calls.obtain += 1;
      io.onProgress(10, 10);
      return { detection: new Uint8Array(1), packs: [] };
    },
    sha256Hex: async () => "",
  };
  const core = createOcrWorkerCore({
    post: (message, transfer) => posts.push({ message, transfer: transfer ?? [] }),
    loadGlue: async () => glue,
    models,
    fetch: async () => ({ ok: true }),
  });
  const next = async (type, id) => {
    for (let i = 0; i < 200; i += 1) {
      const found = posts.find((p) => p.message.type === type && (id === undefined || p.message.id === id));
      if (found) return found;
      await new Promise((resolve) => setTimeout(resolve, 0));
    }
    throw new Error(`no ${type} message; got ${posts.map((p) => p.message.type).join(", ")}`);
  };
  return { core, posts, calls, glue, next };
}

test("a preload and the run after it load the models once, and the run reports every page", async () => {
  const { core, posts, calls, next } = harness();
  core.handle({ type: "warm", id: 1, manifestUrl: URL_A });
  core.handle({ type: "import", id: 2, manifestUrl: URL_A, mode: "ocr", bytes: new ArrayBuffer(4) });
  await next("warm", 1);
  await next("read", 2);
  assert.equal(calls.obtain, 1, "the run joined the preload's download");
  assert.equal(calls.load, 1, "and its engine");
  const pages = posts.filter((p) => p.message.type === "pages" && p.message.id === 2).map((p) => p.message.done);
  assert.deepEqual(pages, [0, 1, 2, 3]);
  assert.ok(posts.some((p) => p.message.type === "models" && p.message.done === 10), "the download's progress is broadcast");
  assert.deepEqual(calls.finish, [], "nothing is finished before the layout is chosen");
  core.handle({ type: "finish", id: 2, layout: "fixed" });
  const done = await next("done", 2);
  assert.deepEqual(calls.finish, ["fixed"]);
  assert.equal(done.message.layout, "fixed");
  assert.deepEqual(done.message.ocr, { provider: "pp-ocrv5" });
  assert.deepEqual(done.transfer, [done.message.pkg.buffer], "the package is transferred back");

  // A second run reuses the engine: no download, no load.
  core.handle({ type: "import", id: 3, manifestUrl: URL_A, mode: "reflow", bytes: new ArrayBuffer(4) });
  await next("done", 3);
  assert.equal(calls.obtain, 1);
  assert.equal(calls.load, 1);
  assert.deepEqual(calls.finish, ["fixed", "flowing"], "a conversion finishes flowing without asking");
});

test("a plain import (no manifest) reads the pages in the worker with no models, and reports the scans it met", async () => {
  const { core, posts, calls, next } = harness();
  core.handle({ type: "import", id: 4, manifestUrl: null, mode: "import", bytes: new ArrayBuffer(4) });
  await next("read", 4);
  assert.equal(calls.plain, 1, "the plain entry point read it");
  assert.equal(calls.begin, 0, "not the recognising one");
  assert.equal(calls.obtain, 0, "no model was downloaded");
  assert.equal(calls.load, 0, "no engine was loaded");
  const scanned = posts.filter((p) => p.message.type === "pages" && p.message.id === 4).map((p) => p.message.scanned);
  assert.ok(scanned.every((n) => n === 2), "each progress message carries the scanned-page count");
  core.handle({ type: "finish", id: 4, layout: "flowing" });
  const done = await next("done", 4);
  assert.deepEqual(calls.finish, ["flowing"]);
  assert.deepEqual(done.transfer, [done.message.pkg.buffer]);
});

test("an engine that fails to load is reported and forgotten, so the next request loads again", async () => {
  const { core, calls, next } = harness({ engineFails: 1 });
  core.handle({ type: "warm", id: 1, manifestUrl: URL_A });
  const failed = await next("error", 1);
  assert.match(failed.message.message, /refused/);
  core.handle({ type: "warm", id: 2, manifestUrl: URL_A });
  await next("warm", 2);
  assert.equal(calls.load, 2);
});

test("the faces handed over answer the PDF module's face host", async () => {
  const { core, glue, next } = harness();
  core.handle({ type: "faces", faces: [{ family: "Liberation Sans", bold: true, italic: false, bytes: new Uint8Array([9]).buffer }] });
  core.handle({ type: "warm", id: 1, manifestUrl: URL_A });
  await next("warm", 1);
  assert.deepEqual([...glue.host.face("Liberation Sans", true, false)], [9]);
  assert.equal(glue.host.face("Liberation Serif", false, false), undefined);
});

test("whether the models are kept is answered from the worker's store", async () => {
  const { core, next } = harness();
  core.handle({ type: "stored", id: 5, manifestUrl: URL_A });
  const answer = await next("stored", 5);
  assert.equal(answer.message.stored, true);
});
