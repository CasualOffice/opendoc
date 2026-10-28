// Text-recognition models for scanned PDF pages: where they come from, how
// they are checked, and where they are kept (`docs/182` §17a, §20).
//
// THE RULE: the editor bundles no model and fetches none on its own. A HOST
// points it at a manifest (`?pdfOcrModels=<url>`, or the embed element's
// `pdf-ocr-models` attribute) listing each file with its size and SHA-256; with
// none, the import summary says text recognition is unavailable and why. With
// one, the summary offers "Recognise text in scanned pages", and the first use
// asks first, naming the download size: nothing is fetched before that click.
//
// Downloaded files are verified against the manifest's SHA-256 before use and
// kept in IndexedDB (`opendoc-ocr-models`, keyed by digest), the pattern the
// proofing packs use (`proof_store.mjs`): disposable, reconstructible bytes in
// their own database, never beside drafts. A later use reads them offline.
//
// Pure apart from the injected `fetch`, `indexedDB` and `digest`, so the
// manifest rules, the digest refusal and the cache are tested in node.

/** The database. Its own, for `proof_store.mjs`'s reason: a model can be
 *  evicted or deleted wholesale, and nothing about it may ever touch a draft. */
export const OCR_DB_NAME = "opendoc-ocr-models";
const OCR_DB_VERSION = 1;
const FILES = "files";

/** Largest manifest the editor accepts: a few hundred megabytes of models is
 *  plausible (a multilingual pack is ~17 MB), a gigabyte is a mistake. */
export const MAX_MODEL_BYTES = 512 * 1024 * 1024;

const HEX64 = /^[0-9a-f]{64}$/;

function file(entry, base, what) {
  if (!entry || typeof entry !== "object") throw new Error(`${what}: missing`);
  const { url, sha256, bytes } = entry;
  if (typeof url !== "string" || url.length === 0) throw new Error(`${what}: no url`);
  if (typeof sha256 !== "string" || !HEX64.test(sha256)) throw new Error(`${what}: sha256 must be 64 lower-case hex digits`);
  if (!Number.isSafeInteger(bytes) || bytes <= 0) throw new Error(`${what}: bytes must be a positive integer`);
  return Object.freeze({ url: new URL(url, base).href, sha256, bytes });
}

/**
 * Validates a manifest: a detection model and one or more recognition packs,
 * each file with `url`, `sha256` and `bytes`. Relative URLs resolve against
 * the manifest's own.
 *
 * @returns {{name: string, detection: object, packs: object[], bytes: number}}
 * @throws an `Error` naming the first problem.
 */
export function parseOcrManifest(json, base) {
  if (!json || typeof json !== "object") throw new Error("the OCR manifest is not an object");
  const detection = file(json.detection, base, "detection");
  if (!Array.isArray(json.packs) || json.packs.length === 0) throw new Error("the OCR manifest names no recognition pack");
  const packs = json.packs.map((pack, i) =>
    Object.freeze({
      name: typeof pack?.name === "string" && pack.name ? pack.name : `pack${i + 1}`,
      model: file(pack?.model, base, `pack ${i + 1} model`),
      dictionary: file(pack?.dictionary, base, `pack ${i + 1} dictionary`),
    }),
  );
  const bytes = detection.bytes + packs.reduce((sum, p) => sum + p.model.bytes + p.dictionary.bytes, 0);
  if (bytes > MAX_MODEL_BYTES) throw new Error(`the OCR models total ${bytes} bytes, over the ${MAX_MODEL_BYTES}-byte limit`);
  return Object.freeze({ name: typeof json.name === "string" ? json.name : "", detection, packs, bytes });
}

/** Every file of a manifest, in load order. */
export function manifestFiles(manifest) {
  return [manifest.detection, ...manifest.packs.flatMap((p) => [p.model, p.dictionary])];
}

/** The configured manifest URL, from the page's query (`pdfOcrModels`), or null. */
export function ocrManifestUrl(search) {
  try {
    const value = new URLSearchParams(search ?? "").get("pdfOcrModels");
    return value ? new URL(value, globalThis.location?.href ?? "http://localhost/").href : null;
  } catch {
    return null;
  }
}

/** The model store: `get(sha)`, `put(sha, bytes)`. Rejects rather than
 *  swallows, as the proofing store does. */
export async function openOcrStore({ indexedDB: idb } = {}) {
  if (!idb) throw new Error("this browser has no IndexedDB");
  const db = await new Promise((resolve, reject) => {
    const rq = idb.open(OCR_DB_NAME, OCR_DB_VERSION);
    rq.onupgradeneeded = () => {
      if (!rq.result.objectStoreNames.contains(FILES)) rq.result.createObjectStore(FILES);
    };
    rq.onsuccess = () => resolve(rq.result);
    rq.onerror = () => reject(rq.error);
    rq.onblocked = () => reject(new Error("the OCR model store is blocked by another tab"));
  });
  const request = (mode, run) =>
    new Promise((resolve, reject) => {
      const tx = db.transaction(FILES, mode);
      const rq = run(tx.objectStore(FILES));
      rq.onsuccess = () => resolve(rq.result);
      rq.onerror = () => reject(rq.error);
    });
  return {
    get: async (sha) => {
      const value = await request("readonly", (store) => store.get(sha));
      return value ? new Uint8Array(value) : null;
    },
    put: (sha, bytes) => request("readwrite", (store) => store.put(bytes, sha)),
  };
}

/** Whether every file of `manifest` is already stored (no consent needed). */
export async function modelsStored(manifest, store) {
  for (const entry of manifestFiles(manifest)) {
    if (!(await store.get(entry.sha256))) return false;
  }
  return true;
}

/**
 * Every file of `manifest`, from the store or downloaded, verified against its
 * SHA-256 before it is stored or used.
 *
 * @param {object} io `{ store, fetch, digest(bytes) -> hex, onProgress(done,total), signal }`
 * @returns {Promise<{detection: Uint8Array, packs: {name: string, model: Uint8Array, dictionary: string}[]}>}
 */
export async function obtainModels(manifest, io) {
  let done = 0;
  const total = manifest.bytes;
  const one = async (entry) => {
    let bytes = io.store ? await io.store.get(entry.sha256) : null;
    if (!bytes) {
      const response = await io.fetch(entry.url, { signal: io.signal });
      if (!response.ok) throw new Error(`${entry.url}: ${response.status}`);
      bytes = new Uint8Array(await response.arrayBuffer());
      const actual = await io.digest(bytes);
      if (actual !== entry.sha256) throw new Error(`${entry.url} does not match its SHA-256 (got ${actual})`);
      await io.store?.put(entry.sha256, bytes);
    }
    done += entry.bytes;
    io.onProgress?.(done, total);
    return bytes;
  };
  const detection = await one(manifest.detection);
  const packs = [];
  for (const pack of manifest.packs) {
    const model = await one(pack.model);
    const dictionary = new TextDecoder().decode(await one(pack.dictionary));
    packs.push({ name: pack.name, model, dictionary });
  }
  return { detection, packs };
}
