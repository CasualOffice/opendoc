// The main thread's half of text recognition (`ocr_service.mjs`, `docs/182`
// §17a.5), against a fake worker: what is posted, what is transferred, when a
// worker is spawned and when it is terminated. The worker's half is
// `ocr_worker_core.test.mjs`; the guarantee both exist for (no long task on
// the main thread while a scan is recognised) is `e2e/ocr-main-thread.spec.mjs`.
import assert from "node:assert/strict";
import test from "node:test";

import { bundledFaces, createOcrService } from "../src/ocr_service.mjs";

const URL_A = "https://host.example/ocr/manifest.json";

/** A fake worker that records every post and answers through `reply`. */
function fakeSpawner(answer) {
  const spawned = [];
  const spawn = () => {
    const worker = {
      posts: [],
      terminated: false,
      handler: null,
      post(message, transfer) {
        worker.posts.push({ message, transfer: transfer ?? [] });
        queueMicrotask(() => answer?.(message, (reply) => worker.handler?.(reply), worker));
      },
      terminate() {
        worker.terminated = true;
      },
      onMessage(handler) {
        worker.handler = handler;
      },
      onError() {},
    };
    spawned.push(worker);
    return worker;
  };
  return { spawn, spawned };
}

/** Answers like the worker core: warm resolves, import reports two pages,
 *  asks for the layout, then returns a package. */
function workerLike(message, reply) {
  if (message.type === "warm") reply({ type: "warm", id: message.id });
  if (message.type === "stored") reply({ type: "stored", id: message.id, stored: false });
  if (message.type === "import") {
    reply({ type: "pages", id: message.id, done: 1, total: 2 });
    reply({ type: "pages", id: message.id, done: 2, total: 2 });
    reply({ type: "read", id: message.id });
  }
  if (message.type === "finish") {
    reply({ type: "done", id: message.id, pkg: new Uint8Array([7, 7]), ocr: { provider: "pp-ocrv5" }, layout: message.layout });
  }
}

test("a recognition run transfers a copy of the PDF, reports its pages and asks the layout once they are read", async () => {
  const { spawn, spawned } = fakeSpawner(workerLike);
  const service = createOcrService({ spawn });
  const bytes = new Uint8Array([1, 2, 3]);
  const pages = [];
  const result = await service.recognise(bytes, {
    manifestUrl: URL_A,
    mode: "ocr",
    layout: () => "fixed",
    onPages: (done, total) => pages.push(`${done}/${total}`),
  });
  assert.equal(spawned.length, 1);
  const post = spawned[0].posts.find((p) => p.message.type === "import");
  assert.equal(post.transfer.length, 1, "the bytes are transferred, not cloned");
  assert.equal(post.transfer[0], post.message.bytes);
  assert.deepEqual([...bytes], [1, 2, 3], "the caller keeps its own bytes (the session's original)");
  assert.deepEqual(pages, ["1/2", "2/2"]);
  assert.equal(spawned[0].posts.find((p) => p.message.type === "finish").message.layout, "fixed");
  assert.deepEqual([...result.pkg], [7, 7]);
  assert.equal(result.layout, "fixed");
  assert.deepEqual(result.ocr, { provider: "pp-ocrv5" });
});

test("a preload is one warm request; the run that follows reuses it, and a warm engine needs no consent", async () => {
  const { spawn, spawned } = fakeSpawner(workerLike);
  const service = createOcrService({ spawn });
  assert.equal(service.isWarm(URL_A), false);
  const first = service.warm(URL_A);
  const second = service.warm(URL_A);
  assert.equal(first, second, "a second preload joins the first");
  await first;
  assert.equal(service.isWarm(URL_A), true);
  await service.recognise(new Uint8Array([1]), { manifestUrl: URL_A, mode: "ocr" });
  const warms = spawned[0].posts.filter((p) => p.message.type === "warm");
  assert.equal(warms.length, 1, "the run did not start a second model load");
  assert.equal(await service.stored(URL_A), true);
  assert.equal(spawned[0].posts.filter((p) => p.message.type === "stored").length, 0, "nothing to ask a warm worker");
});

// Mutation: drop `reset(...)` from the abort handler and the worker is never
// terminated, so the first assertion fails.
test("Cancel terminates the worker, rejects as an abort, and the next run spawns a fresh one", async () => {
  const { spawn, spawned } = fakeSpawner((message, reply) => {
    if (message.type === "warm") reply({ type: "warm", id: message.id });
    // An import that never answers: a page still being recognised.
  });
  const service = createOcrService({ spawn });
  const abort = new AbortController();
  const run = service.recognise(new Uint8Array([1]), { manifestUrl: URL_A, mode: "ocr", signal: abort.signal });
  await new Promise((resolve) => setTimeout(resolve, 0));
  abort.abort();
  await assert.rejects(run, (error) => error.name === "AbortError");
  assert.equal(spawned[0].terminated, true, "the page in flight is stopped by terminating its worker");
  assert.equal(service.isWarm(URL_A), false, "a terminated worker holds no engine");
  service.warm(URL_A).catch(() => {});
  assert.equal(spawned.length, 2);
});

test("the faces are handed over once per worker, transferred", async () => {
  const face = (family, bold, italic) => (family === "Liberation Mono" && italic ? undefined : new Uint8Array([bold ? 1 : 0]));
  const faces = bundledFaces(face);
  assert.equal(faces.length, 10, "every face the main module has, and none it lacks");
  const { spawn, spawned } = fakeSpawner(workerLike);
  const service = createOcrService({ spawn, faces: () => bundledFaces(face) });
  await service.warm(URL_A);
  await service.recognise(new Uint8Array([1]), { manifestUrl: URL_A, mode: "ocr" });
  const sent = spawned[0].posts.filter((p) => p.message.type === "faces");
  assert.equal(sent.length, 1);
  assert.equal(sent[0].transfer.length, 10);
});

test("a manifest the worker cannot load rejects the run and the next preload tries again", async () => {
  let attempts = 0;
  const { spawn } = fakeSpawner((message, reply) => {
    if (message.type === "warm") {
      attempts += 1;
      reply({ type: "error", id: message.id, message: "the engine refused the models" });
    }
  });
  const service = createOcrService({ spawn });
  await assert.rejects(service.warm(URL_A), /refused/);
  assert.equal(service.isWarm(URL_A), false);
  await assert.rejects(service.warm(URL_A), /refused/);
  assert.equal(attempts, 2);
});
