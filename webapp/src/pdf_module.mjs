// The editor's PDF module, fetched the first time a PDF is opened
// (`docs/182` §17a).
//
// The main engine module links no PDF reader: hayro, the importer and OCR are
// a second WebAssembly module (`pkg-pdf/`, built by `build.sh`), so opening a
// DOCX never downloads them. This file is the one place that loads it:
//
//   * once: the first successful load is kept, so a second PDF open reuses it
//     (the module's own memory stays alive with it);
//   * with progress: the wasm is fetched as a stream and its bytes counted
//     against `Content-Length`, so the dialog can say how much is left;
//   * cancellable: an `AbortSignal` stops the fetch, and a cancelled or failed
//     load leaves nothing behind, so the next PDF open simply tries again;
//   * wired: on load it hands the PDF module the faces the main module ships
//     (the standard 14 resolve to Liberation, `casual-doc-pdf-read`
//     `provision`), and hands the main module the PDF module's half of the
//     byte-preserving save (`setPdfSavePlanner`): the import's digests, the
//     plan and the hayro half all live there, so the main module only lays
//     out the overlay and writes the update.
//
// No service worker exists in this app, so there is no offline cache to join;
// the browser's HTTP cache keeps the module like every other stamped asset.
//
// URLs carry this module's own `?v=` stamp (`stamp-assets.py`): the import
// map stamps the glue, and the wasm fetched here takes the same query, so a
// deploy can never pair a new glue with a cached old binary.

import { bundledFace, setPdfSavePlanner } from "../pkg/casual_doc_wasm.js";

const GLUE = "../pkg-pdf/casual_doc_wasm_pdf.js";
const WASM = "../pkg-pdf/casual_doc_wasm_pdf_bg.wasm";

/** The loaded module's namespace, once a load has succeeded. */
let loaded = null;
/** The load in flight, shared by concurrent callers. */
let pending = null;
/** How many times the wasm was actually fetched (the e2e's reuse check). */
let fetches = 0;

/** Whether the PDF module is loaded. */
export function pdfModuleLoaded() {
  return loaded !== null;
}

/** The loaded module, or null. */
export function loadedPdfModule() {
  return loaded;
}

/** The stamped URL of `path`, relative to this module. */
function stamped(path, base) {
  const url = new URL(path, base);
  url.search = new URL(base).search;
  return url;
}

/**
 * Reads `response`'s body, reporting bytes as they arrive.
 *
 * @returns {Promise<Uint8Array>}
 */
async function readWithProgress(response, onProgress) {
  const total = Number(response.headers.get("content-length")) || 0;
  if (!response.body?.getReader) {
    const bytes = new Uint8Array(await response.arrayBuffer());
    onProgress?.(bytes.length, total || bytes.length);
    return bytes;
  }
  const reader = response.body.getReader();
  const chunks = [];
  let received = 0;
  onProgress?.(0, total);
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    chunks.push(value);
    received += value.length;
    onProgress?.(received, total);
  }
  const bytes = new Uint8Array(received);
  let at = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, at);
    at += chunk.length;
  }
  return bytes;
}

/**
 * Loads the PDF module, or returns the one already loaded.
 *
 * @param {object} [io]
 * @param {(received: number, total: number) => void} [io.onProgress] bytes of
 *   the wasm received so far, and the total when the server said it (else 0).
 * @param {AbortSignal} [io.signal] aborts the fetch; the promise rejects with
 *   an `AbortError` and nothing is kept.
 * @param {string} [io.base] the URL the module paths resolve against
 *   (`import.meta.url`; a test passes its own).
 * @param {(url: URL, init: object) => Promise<Response>} [io.fetch]
 * @returns {Promise<object>} the module's namespace.
 */
export async function loadPdfModule(io = {}) {
  if (loaded) return loaded;
  if (pending) return pending;
  const base = io.base ?? import.meta.url;
  const doFetch = io.fetch ?? ((url, init) => fetch(url, init));
  pending = (async () => {
    try {
      const glue = await import(GLUE);
      fetches += 1;
      const response = await doFetch(stamped(WASM, base), { signal: io.signal });
      if (!response.ok) throw new Error(`the PDF module returned ${response.status}`);
      const bytes = await readWithProgress(response, io.onProgress);
      io.signal?.throwIfAborted?.();
      await glue.default({ module_or_path: bytes });
      // The faces the PDF module draws the standard 14 with are the main
      // module's own: asked for once each, by family and style.
      glue.setFaceHost({ face: (family, bold, italic) => bundledFace(family, bold, italic) });
      // And the main module's save asks this one what changed and for the
      // original's edited pages (`docs/182` §17a).
      setPdfSavePlanner({
        planSave: (original, baseline, documentJson) => glue.planSave(original, baseline, documentJson),
      });
      loaded = glue;
      return glue;
    } finally {
      pending = null;
    }
  })();
  return pending;
}

/** For the e2e and diagnostics: how many times the wasm was fetched. */
export function pdfModuleFetches() {
  return fetches;
}
