// SPDX-License-Identifier: Apache-2.0
//
// Node conveniences for @casualoffice/opendoc-viewer. The same WebAssembly
// module as the browser build; this only reads files that a browser would
// fetch. Kept in its own entry point so a browser bundle never sees `node:fs`.

import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { PACKAGE_FONTS, init, openDocument as openInMemory } from "./index.js";

export * from "./index.js";

/** Where the package's engine module and face files are on disk. */
export const WASM_PATH = fileURLToPath(new URL("../pkg/opendoc_viewer_bg.wasm", import.meta.url));
export const FONTS_DIR = fileURLToPath(new URL("../pkg/fonts/", import.meta.url));

/** Loads the engine module from the package directory. */
export async function initNode() {
  return init({ wasm: await readFile(WASM_PATH) });
}

/**
 * A font provider that reads the package's face files from `dir` (default: the
 * package's own `pkg/fonts/`). Same rule as `packageFontProvider`: it answers
 * only a need whose substitute the package ships and is not available yet.
 *
 * @param {string} [dir]
 */
export function nodeFontProvider(dir = FONTS_DIR) {
  const cache = new Map();
  return (need) => {
    const family = need.substitute;
    if (!family || need.substituteAvailable || !(family in PACKAGE_FONTS)) return null;
    if (!cache.has(family)) {
      cache.set(
        family,
        Promise.all(PACKAGE_FONTS[family].map(async (file) => new Uint8Array(await readFile(`${dir}/${file}`)))),
      );
    }
    return cache.get(family);
  };
}

/**
 * `openDocument` for Node: loads the module from disk on first use, and by
 * default serves the package's own faces through `nodeFontProvider`. Pass
 * `fontProvider: null` to open with the core set only.
 *
 * @param {BufferSource} bytes
 * @param {import("../types/index.d.ts").OpenOptions} [options]
 */
export async function openDocument(bytes, options = {}) {
  await initNode();
  const fontProvider = options.fontProvider === undefined ? nodeFontProvider() : options.fontProvider;
  return openInMemory(bytes, { ...options, fontProvider });
}
