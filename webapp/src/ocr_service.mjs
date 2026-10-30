// The main thread's half of text recognition (`docs/182` §17a.5): one
// dedicated worker (`ocr_worker.js`) that owns the models and the engine, and
// this client, which only posts the PDF's bytes and receives the finished
// package. Nothing about recognition runs here: not the model download or its
// SHA-256 check, not the IndexedDB read, not the engine load, not a page.
//
// The PATTERN is a dedicated worker with a request/response protocol and
// transferable buffers (`ocr_worker_core.mjs` documents the messages): the
// PDF's bytes go over as a transferred copy (the session keeps its own for the
// saved original), the package comes back transferred, so neither direction
// clones megabytes on the main thread.
//
// PRELOAD. `warm(url)` starts the model download and the engine load in the
// worker and returns at once. The PDF import calls it the first time a page it
// reads is a scan (`pdf_import.mjs`, `onScans`), so the models load while the
// rest of the PDF is still being read and recognition starts warm. A later
// session warms from IndexedDB the same way, without the network. Nothing is
// warmed for a document that is not a PDF, or a PDF with no scans: no request
// for anyone who does not need it.
//
// CANCEL terminates the worker. A page's recognition is one wasm call that
// nothing outside it can interrupt; terminating frees the job, the engine and
// their memory at once, and the next use spawns a fresh worker that loads the
// models from IndexedDB.
//
// Pure apart from the injected `spawn`, so the protocol is tested in node with
// a fake worker (`ocr_service.test.mjs`); the faces come from the caller
// (the main module's `bundledFace`), so this file imports no wasm.

/** The faces the PDF module draws the standard 14 with (`casual-doc-pdf-read`
 *  `provision`): Liberation, which the main module ships. */
const FACE_FAMILIES = ["Liberation Sans", "Liberation Serif", "Liberation Mono"];

/** The main module's Liberation faces (`face` is its `bundledFace`), copied
 *  out once per worker. O(faces). */
export function bundledFaces(face) {
  const out = [];
  for (const family of FACE_FAMILIES) {
    for (const bold of [false, true]) {
      for (const italic of [false, true]) {
        const bytes = face(family, bold, italic);
        if (bytes?.length) out.push({ family, bold, italic, bytes });
      }
    }
  }
  return out;
}

/** The browser's worker, at this module's own `?v=` stamp (`spell_check.mjs`
 *  gives the reason: a URL resolved against `import.meta.url` drops it). */
function browserSpawn() {
  const stamp = new URL(import.meta.url).search;
  const worker = new Worker(`${new URL("./ocr_worker.js", import.meta.url)}${stamp}`, { type: "module" });
  return {
    post: (message, transfer) => worker.postMessage(message, transfer ?? []),
    terminate: () => worker.terminate(),
    onMessage: (handler) => worker.addEventListener("message", (event) => handler(event.data)),
    onError: (handler) => worker.addEventListener("error", (event) => handler(event.message || "the text recognition worker failed")),
  };
}

const cancelled = () => Object.assign(new Error("cancelled"), { name: "AbortError" });

/**
 * @param {object} [io]
 * @param {() => object} [io.spawn] a worker: `{post, terminate, onMessage, onError}`.
 * @param {() => object[]} [io.faces] the standard-14 faces to hand the worker.
 */
export function createOcrService(io = {}) {
  const spawn = io.spawn ?? browserSpawn;
  const faces = io.faces ?? (() => []);
  let worker = null;
  let nextId = 1;
  /** Requests awaiting an answer, by id: `{resolve, reject, on}`. */
  const pending = new Map();
  /** The manifest whose engine the worker has loaded, or is loading. */
  let warmUrl = null;
  let warmed = false;
  let warming = null;
  /** The last model progress the worker broadcast, replayed to a run that
   *  joins a preload already under way. */
  let modelState = null;
  /** The run's progress callbacks, while one runs. */
  let active = null;

  function reset(reason) {
    worker?.terminate();
    worker = null;
    warmUrl = null;
    warmed = false;
    warming = null;
    modelState = null;
    const waiting = [...pending.values()];
    pending.clear();
    for (const request of waiting) request.reject(reason);
  }

  function onMessage(message) {
    if (message?.type === "models") {
      modelState = { phase: "models", done: message.done, total: message.total };
      active?.onModels?.(message.done, message.total);
      return;
    }
    if (message?.type === "loading") {
      modelState = { phase: "loading" };
      active?.onLoading?.();
      return;
    }
    if (message?.type === "ready") return;
    if (message?.type === "error" && message.id === null) {
      reset(new Error(message.message));
      return;
    }
    const request = pending.get(message?.id);
    if (!request) return;
    if (message.type === "error") {
      pending.delete(message.id);
      request.reject(new Error(message.message));
    } else if (message.type === "pages") {
      request.on?.pages?.(message.done, message.total);
    } else if (message.type === "read") {
      request.on?.read?.(message.id);
    } else {
      pending.delete(message.id);
      request.resolve(message);
    }
  }

  function ensureWorker() {
    if (worker) return worker;
    worker = spawn();
    worker.onMessage(onMessage);
    worker.onError((reason) => reset(new Error(String(reason))));
    const list = faces();
    worker.post({ type: "faces", faces: list }, list.map((face) => face.bytes.buffer));
    return worker;
  }

  function request(message, { transfer, on } = {}) {
    const id = nextId++;
    const target = ensureWorker();
    return new Promise((resolve, reject) => {
      pending.set(id, { resolve, reject, on });
      target.post({ ...message, id }, transfer);
    });
  }

  /** Starts loading `url`'s models and engine in the worker; idempotent. The
   *  promise settles when the engine is loaded or failed; callers that only
   *  want the preload ignore it. */
  function warm(url) {
    if (warmUrl === url && warming) return warming;
    if (warmUrl !== url) {
      // A different manifest: a fresh worker, so one engine is ever held.
      if (worker) reset(new Error("the recognition models changed"));
      warmUrl = url;
    }
    const promise = request({ type: "warm", manifestUrl: url }).then(() => {
      if (warming === promise) warmed = true;
    });
    warming = promise;
    promise.catch(() => {
      if (warming === promise) {
        warming = null;
        modelState = null;
      }
    });
    return promise;
  }

  return {
    warm,
    /** Whether `url`'s engine is loaded in the worker. O(1). */
    isWarm: (url) => warmed && warmUrl === url,
    /** Whether `url`'s models are all in IndexedDB (asked of the worker, which
     *  owns the store: the main thread never deserialises a model). */
    async stored(url) {
      if (warmUrl === url && (warmed || warming)) return true;
      const answer = await request({ type: "stored", manifestUrl: url });
      return Boolean(answer.stored);
    },
    /**
     * Imports `bytes` with recognition, in the worker.
     *
     * @param {Uint8Array} bytes the PDF; a copy is transferred, `bytes` is kept.
     * @param {object} options
     * @param {string} options.manifestUrl
     * @param {"import"|"ocr"|"fixed"|"reflow"} options.mode
     * @param {string|null} [options.cmapSource]
     * @param {() => "flowing"|"fixed"} [options.layout] asked once the pages are read.
     * @param {(done: number, total: number) => void} [options.onModels]
     * @param {() => void} [options.onLoading]
     * @param {(done: number, total: number) => void} [options.onPages]
     * @param {() => void} [options.onRead] the pages are read; the finish runs.
     * @param {AbortSignal} [options.signal] terminates the worker.
     * @returns {Promise<{pkg: Uint8Array, ocr: object|null, layout: string}>}
     *   rejects with an `AbortError` when cancelled.
     */
    async recognise(bytes, options) {
      if (options.signal?.aborted) throw cancelled();
      warm(options.manifestUrl).catch(() => {});
      active = options;
      if (!warmed && modelState?.phase === "models") options.onModels?.(modelState.done, modelState.total);
      if (!warmed && modelState?.phase === "loading") options.onLoading?.();
      const copy = bytes.slice();
      let abort = null;
      try {
        return await new Promise((resolve, reject) => {
          abort = () => {
            reset(cancelled());
            reject(cancelled());
          };
          options.signal?.addEventListener("abort", abort, { once: true });
          request(
            {
              type: "import",
              bytes: copy.buffer,
              manifestUrl: options.manifestUrl,
              mode: options.mode,
              cmapSource: options.cmapSource ?? null,
            },
            {
              transfer: [copy.buffer],
              on: {
                pages: (done, total) => options.onPages?.(done, total),
                read: (id) => {
                  options.onRead?.();
                  const layout = options.layout?.() ?? "flowing";
                  worker?.post({ type: "finish", id, layout });
                },
              },
            },
          ).then(
            (answer) =>
              resolve({
                pkg: answer.pkg instanceof Uint8Array ? answer.pkg : new Uint8Array(answer.pkg),
                ocr: answer.ocr ?? null,
                layout: answer.layout,
              }),
            reject,
          );
        });
      } finally {
        options.signal?.removeEventListener?.("abort", abort);
        if (active === options) active = null;
      }
    },
    /** Ends the worker and forgets the engine. */
    terminate: () => reset(cancelled()),
  };
}
