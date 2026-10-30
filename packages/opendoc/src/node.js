// SPDX-License-Identifier: Apache-2.0
//
// Node entry for @casualoffice/opendoc: the same module as the browser build,
// loaded from the package directory, plus file conveniences. Kept apart so a
// browser bundle never sees `node:fs`.

import { readFileSync } from "node:fs";
import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { Document, init, setLoader } from "./engine.js";

export * from "./index.js";

/** Where the engine module is on disk. */
export const WASM_PATH = fileURLToPath(new URL("../pkg/opendoc_bg.wasm", import.meta.url));

setLoader(() => readFile(WASM_PATH));

// Top-level await: importing this entry loads the engine, so every call below
// (and every `Document` method) is synchronous, as in python-docx.
await init({ wasm: readFileSync(WASM_PATH) });

/** Opens a file. */
export async function openFile(path, options = {}) {
  return Document.open(await readFile(path), { fileName: String(path), ...options });
}

/** Saves a document to a file; the format comes from the extension unless given. */
export async function saveFile(doc, path, { format, mode } = {}) {
  const name = String(path);
  const ext = format ?? name.slice(name.lastIndexOf(".") + 1).toLowerCase();
  const { bytes, report } = doc.save(ext, { mode });
  await writeFile(path, bytes);
  return report;
}
