// The OCR worker: a transport over `ocr_worker_core.mjs`, and nothing else
// (`docs/182` §17a.5). Every decision is in the core, which node tests; this
// file only runs it on a thread that is not the one painting the document.
//
// `.js`, not `.mjs`: an entry script, as `module_seams.test.mjs` defines it.
//
// The two rules `proof_worker.js` learned the hard way apply here unchanged:
//
//   * imports are DYNAMIC and carry this worker's own `?v=` stamp, because a
//     page's import map does not apply inside a worker and an unstamped sibling
//     is served from a fixed URL with a four-hour cache (`stamp-assets.py`);
//   * the listener is registered in the FIRST synchronous statement and early
//     messages are queued, because a module worker that yields at a top-level
//     `await` starts dispatching messages with no listener and drops them.
//
// The PDF module's wasm is fetched here with the same stamp and handed to its
// glue as bytes, exactly as `pdf_module.mjs` does on the main thread, so a
// deploy can never pair a new glue with a cached old binary.

/** Messages that arrived before the core finished loading. */
const queued = [];
let core = null;

self.addEventListener("message", (event) => {
  if (!core) {
    queued.push(event.data);
    return;
  }
  core.handle(event.data);
});

const stamp = new URL(self.location.href).search;
const sibling = (path) => `${new URL(path, import.meta.url)}${stamp}`;

Promise.all([import(sibling("./ocr_worker_core.mjs")), import(sibling("./ocr_models.mjs"))])
  .then(([{ createOcrWorkerCore }, models]) => {
    core = createOcrWorkerCore({
      post: (message, transfer) => self.postMessage(message, transfer ?? []),
      loadGlue: async () => {
        const glue = await import(sibling("../pkg-pdf/casual_doc_wasm_pdf.js"));
        const response = await fetch(sibling("../pkg-pdf/casual_doc_wasm_pdf_bg.wasm"));
        if (!response.ok) throw new Error(`the PDF module returned ${response.status}`);
        await glue.default({ module_or_path: new Uint8Array(await response.arrayBuffer()) });
        return glue;
      },
      models,
      fetch: (url, init) => fetch(url, init),
      indexedDB: self.indexedDB,
      subtle: self.crypto?.subtle,
    });
    for (const message of queued.splice(0)) core.handle(message);
    self.postMessage({ type: "ready" });
  })
  .catch((error) => {
    // Answer every queued request, so the editor says what failed instead of
    // waiting on a worker that will never reply.
    const message = `the text recognition worker did not start: ${String(error?.message ?? error)}`;
    for (const request of queued.splice(0)) {
      if (request?.id !== undefined) self.postMessage({ type: "error", id: request.id, message });
    }
    self.postMessage({ type: "error", id: null, message });
  });
