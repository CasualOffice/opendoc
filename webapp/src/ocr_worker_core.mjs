// Text recognition off the main thread (`docs/182` §17a.5): what the OCR
// worker does, as a message handler with every browser API injected, so the
// protocol and its ordering are tested in node (`ocr_worker_core.test.mjs`)
// and `ocr_worker.js` is only the transport.
//
// WHY A WORKER. Recognition is the PDF module's (`casual-doc-wasm-pdf`):
// `PdfOcrEngine.load` compiles the PP-OCRv5 graphs on rten, and a page's
// `PdfImportJob.step` rasterises it, runs detection and runs recognition on
// every line, all synchronously inside one wasm call. Run on the main thread,
// that was one task per page of seconds, and the model load another; the
// import's slicing (`pdf_import_scheduler.mjs`) can only cut BETWEEN pages, so
// the tab froze for the whole run (owner report, 2026-10-31). Here the same
// calls run in a dedicated worker: the model fetch and its SHA-256 check, the
// IndexedDB cache, the engine load, the rasterising, detection, recognition and
// the finish. The main thread posts the PDF's bytes (transferred, never
// copied) and receives the finished package, also transferred.
//
// One thread inside the worker: rten runs single-threaded on
// wasm32-unknown-unknown, and wasm threads would need SharedArrayBuffer, which
// needs the page cross-origin isolated (COOP/COEP), a header an embeddable
// editor cannot demand of its host. The worker is the parallelism: the main
// thread is free for the whole run.
//
// THE PROTOCOL (main → worker):
//   { type: "faces", faces: [{family, bold, italic, bytes}] }   once, at spawn
//   { type: "stored", id, manifestUrl }      → { type: "stored", id, stored }
//   { type: "warm", id, manifestUrl }        → { type: "warm", id } | error
//   { type: "import", id, bytes, manifestUrl, mode, cmapSource }
//        → { type: "pages", id, done, total }* → { type: "read", id }
//   { type: "finish", id, layout }           → { type: "done", id, pkg, ocr, layout } | error
// and, broadcast while the models load for whichever request needed them:
//   { type: "models", done, total }, { type: "loading" }
// Every failure is { type: "error", id, message }. Cancel is the main thread
// terminating the worker: a page's step cannot be interrupted from outside a
// wasm call, and a terminated worker frees the job, the engine and its memory
// at once; the next use spawns a new one and loads the models from IndexedDB.

/**
 * @param {object} io
 * @param {(message: object, transfer?: Transferable[]) => void} io.post
 * @param {() => Promise<object>} io.loadGlue the PDF module's namespace,
 *   initialised.
 * @param {object} io.models `ocr_models.mjs`'s `fetchOcrManifest`,
 *   `openOcrStore`, `obtainModels`, `modelsStored`, `sha256Hex`.
 * @param {(url: string, init?: object) => Promise<Response>} io.fetch
 * @param {object} [io.indexedDB]
 * @param {object} [io.subtle] Web Crypto's `subtle`.
 * @returns {{ handle(message: object): void }}
 */
export function createOcrWorkerCore(io) {
  /** Faces for the standard 14, by `family|bold|italic`. */
  const faces = new Map();
  let glue = null;
  let gluePending = null;
  /** The engine in flight or loaded, keyed by manifest URL. */
  let engine = null; // { url, promise }
  let store = null;
  /** Finish requests the import is waiting on, by id. */
  const finishing = new Map();

  const fail = (id, error) => io.post({ type: "error", id, message: String(error?.message ?? error) });

  async function module() {
    if (glue) return glue;
    gluePending ??= io.loadGlue().then((loaded) => {
      loaded.setFaceHost({
        face: (family, bold, italic) => faces.get(`${family}|${Boolean(bold)}|${Boolean(italic)}`),
      });
      glue = loaded;
      return loaded;
    });
    return gluePending;
  }

  async function modelStore() {
    if (store) return store;
    store = await io.models.openOcrStore({ indexedDB: io.indexedDB }).catch(() => null);
    return store;
  }

  /** The engine for `url`, loading it once; a failed load is forgotten so the
   *  next request tries again. */
  function ensureEngine(url) {
    if (engine?.url === url) return engine.promise;
    const promise = (async () => {
      const manifest = await io.models.fetchOcrManifest(url, { fetch: io.fetch });
      const kept = await modelStore();
      io.post({ type: "models", done: 0, total: manifest.bytes });
      const files = await io.models.obtainModels(manifest, {
        store: kept,
        fetch: io.fetch,
        digest: (bytes) => io.models.sha256Hex(bytes, io.subtle),
        onProgress: (done, total) => io.post({ type: "models", done, total }),
      });
      const pdf = await module();
      io.post({ type: "loading" });
      const models = new pdf.PdfOcrModels(files.detection);
      for (const pack of files.packs) models.addPack(pack.name, pack.model, pack.dictionary);
      return pdf.PdfOcrEngine.load(models);
    })();
    engine = { url, promise };
    promise.catch(() => {
      if (engine?.promise === promise) engine = null;
    });
    return promise;
  }

  async function provideCMaps(pdf, names, source) {
    if (!source || names.length === 0) return;
    const base = source.endsWith("/") ? source : `${source}/`;
    for (const name of names) {
      const response = await io.fetch(new URL(encodeURIComponent(name), base).href);
      if (!response.ok) continue;
      pdf.provideCMap(name, new Uint8Array(await response.arrayBuffer()));
    }
  }

  /** One staged import with the engine, every page in this task: nothing here
   *  holds the thread anyone is waiting on. */
  async function importWith(id, pdf, loaded, bytes, message) {
    let job = null;
    try {
      job = pdf.beginPdfImportWithOcr(bytes, loaded);
      await provideCMaps(pdf, job.missingCMaps, message.cmapSource);
      const total = job.pageCount;
      io.post({ type: "pages", id, done: 0, total });
      let status = "pending";
      while (status === "pending") {
        status = job.step(1);
        io.post({ type: "pages", id, done: job.pagesDone, total });
      }
      if (status !== "done") throw new Error(status === "cancelled" ? "the import was cancelled" : "a page could not be read");
      // The layout is the reader's to choose while the pages were read (the
      // dialog's checkbox): asked now, answered by the "finish" message.
      const layout =
        message.mode === "reflow"
          ? "flowing"
          : message.mode === "fixed"
            ? "fixed"
            : await new Promise((resolve) => {
                finishing.set(id, resolve);
                io.post({ type: "read", id });
              });
      try {
        const pkg = layout === "flowing" ? job.finishReflowed() : job.finish();
        const ocr = job.ocrSummaryJson ? JSON.parse(job.ocrSummaryJson) : null;
        return { pkg, ocr, layout };
      } catch (error) {
        const err = new Error(String(error?.message ?? error));
        err.neededCMaps = job.neededCMaps ?? [];
        throw err;
      }
    } finally {
      job?.free?.();
    }
  }

  async function runImport(message) {
    const { id } = message;
    try {
      const loaded = await ensureEngine(message.manifestUrl);
      const pdf = await module();
      const bytes = new Uint8Array(message.bytes);
      let result;
      try {
        result = await importWith(id, pdf, loaded, bytes, message);
      } catch (error) {
        // A page needed predefined CMaps nobody had: fetch them and read once
        // more, as the main-thread import does.
        if (!message.cmapSource || !error.neededCMaps?.length) throw error;
        await provideCMaps(pdf, error.neededCMaps, message.cmapSource);
        result = await importWith(id, pdf, loaded, bytes, message);
      }
      const pkg = result.pkg instanceof Uint8Array ? result.pkg : new Uint8Array(result.pkg);
      io.post({ type: "done", id, pkg, ocr: result.ocr, layout: result.layout }, [pkg.buffer]);
    } catch (error) {
      fail(id, error);
    }
  }

  return {
    handle(message) {
      switch (message?.type) {
        case "faces":
          for (const face of message.faces ?? []) {
            if (face?.bytes) faces.set(`${face.family}|${Boolean(face.bold)}|${Boolean(face.italic)}`, new Uint8Array(face.bytes));
          }
          return;
        case "stored":
          void (async () => {
            try {
              const manifest = await io.models.fetchOcrManifest(message.manifestUrl, { fetch: io.fetch });
              const kept = await modelStore();
              const stored = kept ? await io.models.modelsStored(manifest, kept) : false;
              io.post({ type: "stored", id: message.id, stored });
            } catch (error) {
              fail(message.id, error);
            }
          })();
          return;
        case "warm":
          ensureEngine(message.manifestUrl).then(
            () => io.post({ type: "warm", id: message.id }),
            (error) => fail(message.id, error),
          );
          return;
        case "import":
          void runImport(message);
          return;
        case "finish": {
          const resolve = finishing.get(message.id);
          finishing.delete(message.id);
          resolve?.(message.layout === "fixed" ? "fixed" : "flowing");
          return;
        }
        default:
      }
    },
  };
}
