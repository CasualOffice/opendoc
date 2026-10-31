// SPDX-License-Identifier: Apache-2.0
//
// @casualoffice/opendoc-viewer: the OpenDoc Viewer SDK for browsers and Node
// (docs/176). Read-only and batch: open a DOCX, then read its pages, text,
// outline and PDF. No editing, selection, caret or collaboration, by
// construction: the engine module is built from `crates/opendoc-viewer`, which
// does not link the editing crates.
//
// Fonts (docs/176 section 10, Q2). The module embeds one core family,
// Liberation Sans. Every other face the layout can use is a plain file in
// `pkg/fonts/` that the HOST serves and hands back through `fontProvider`;
// nothing is fetched from a CDN. Serve the same faces and the pixels are the
// same on every machine.

import initWasm, { WasmViewer } from "../pkg/opendoc_viewer.js";

/** The families embedded in the engine module itself. */
export const CORE_FAMILIES = Object.freeze(["Liberation Sans"]);

/** The faces the package ships as files for the host to serve, by the family
 *  name the engine asks for (`FontNeed.substitute`). Paths are relative to
 *  `pkg/fonts/`. */
export const PACKAGE_FONTS = Object.freeze({
  Carlito: ["Carlito-Regular.ttf", "Carlito-Bold.ttf", "Carlito-Italic.ttf", "Carlito-BoldItalic.ttf"],
  Caladea: ["Caladea-Regular.ttf", "Caladea-Bold.ttf", "Caladea-Italic.ttf", "Caladea-BoldItalic.ttf"],
  "Liberation Serif": [
    "LiberationSerif-Regular.ttf",
    "LiberationSerif-Bold.ttf",
    "LiberationSerif-Italic.ttf",
    "LiberationSerif-BoldItalic.ttf",
  ],
  "Liberation Mono": [
    "LiberationMono-Regular.ttf",
    "LiberationMono-Bold.ttf",
    "LiberationMono-Italic.ttf",
    "LiberationMono-BoldItalic.ttf",
  ],
  Roboto: ["Roboto-Regular.ttf", "Roboto-Bold.ttf", "Roboto-Italic.ttf", "Roboto-BoldItalic.ttf"],
  "Noto Emoji": ["NotoEmoji-Variable.ttf"],
  "Noto Sans Symbols 2": ["NotoSansSymbols2-Subset.ttf"],
});

let ready = null;

/**
 * Loads the engine module once. `openDocument` calls it for you; call it
 * yourself to start the download early or to say where the `.wasm` is.
 *
 * @param {{ wasm?: URL | string | Response | BufferSource | WebAssembly.Module }} [options]
 */
export function init(options = {}) {
  if (!ready) {
    const source = options.wasm;
    ready = (source === undefined ? initWasm() : initWasm({ module_or_path: source })).catch(
      (error) => {
        ready = null;
        throw error;
      },
    );
  }
  return ready;
}

function toBytes(value, what) {
  if (value instanceof Uint8Array) return value;
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  if (ArrayBuffer.isView(value)) return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  throw new TypeError(`${what} must be a Uint8Array, an ArrayBuffer or a typed array`);
}

/** Packs faces into the (buffer, lengths) pair the engine boundary takes. */
function packFaces(faces) {
  const list = faces.map((face, index) => toBytes(face, `font ${index}`));
  const total = list.reduce((sum, face) => sum + face.byteLength, 0);
  const blob = new Uint8Array(total);
  const lengths = new Uint32Array(list.length);
  let at = 0;
  list.forEach((face, index) => {
    blob.set(face, at);
    lengths[index] = face.byteLength;
    at += face.byteLength;
  });
  return [blob, lengths];
}

/** Flattens what a font provider returned into a list of faces. */
function facesFrom(answer) {
  if (answer == null) return [];
  if (Array.isArray(answer)) return answer.filter((face) => face != null);
  return [answer];
}

/**
 * A font provider that serves the package's own face files from `baseUrl`
 * (where the host put `pkg/fonts/`). It answers a need only when the engine's
 * substitute is one the package ships and is not available yet, so it never
 * re-registers a family. Each family is fetched once per provider.
 *
 * @param {URL | string} baseUrl
 * @param {{ fetch?: typeof fetch }} [options]
 */
export function packageFontProvider(baseUrl, options = {}) {
  const get = options.fetch ?? globalThis.fetch;
  const base = new URL(String(baseUrl).endsWith("/") ? String(baseUrl) : `${baseUrl}/`, globalThis.location?.href);
  const cache = new Map();
  return (need) => {
    const family = need.substitute;
    if (!family || need.substituteAvailable || !(family in PACKAGE_FONTS)) return null;
    if (!cache.has(family)) {
      cache.set(
        family,
        Promise.all(
          PACKAGE_FONTS[family].map(async (file) => {
            const response = await get(new URL(file, base));
            if (!response.ok) throw new Error(`font ${file}: HTTP ${response.status}`);
            return new Uint8Array(await response.arrayBuffer());
          }),
        ),
      );
    }
    return cache.get(family);
  };
}

/**
 * Opens a document and lays it out.
 *
 * Fonts: `fonts` are registered before the first layout (one pagination).
 * `fontProvider` is then asked once for every font the layout needed and did
 * not have; everything it returns is registered together and the document is
 * laid out once more. A provider that returns nothing leaves the substitute in
 * place, so a document always opens.
 *
 * @param {BufferSource} bytes  The `.docx` file.
 * @param {import("../types/index.d.ts").OpenOptions} [options]
 * @returns {Promise<ViewerDocument>}
 */
export async function openDocument(bytes, options = {}) {
  await init({ wasm: options.wasm });
  const data = toBytes(bytes, "the document");
  const preload = (options.fonts ?? []).map((face, index) => toBytes(face, `fonts[${index}]`));
  // The name selects a reader whose bytes carry no evidence of their own
  // (`.md`, `.html`, with the engine built with `formats`); fonts and a name
  // together register the fonts after the open, at the cost of a second layout.
  const named = typeof options.name === "string" && options.name;
  const engine = named
    ? WasmViewer.openNamed(data, options.name)
    : preload.length
      ? WasmViewer.openWithFonts(data, ...packFaces(preload))
      : WasmViewer.open(data);
  const doc = new ViewerDocument(engine);
  if (named && preload.length) doc.registerFonts(preload);
  if (typeof options.fontProvider === "function") {
    const needs = doc.fontNeeds();
    const answers = await Promise.all(needs.map((need) => options.fontProvider(need)));
    const seen = new Set();
    const faces = [];
    for (const face of answers.flatMap(facesFrom)) {
      const bytesOfFace = toBytes(face, "a provided font");
      if (seen.has(bytesOfFace)) continue;
      seen.add(bytesOfFace);
      faces.push(bytesOfFace);
    }
    if (faces.length) doc.registerFonts(faces);
  }
  return doc;
}

/** Twips per typographic point. */
const TWIPS_PER_POINT = 20;

/** An opened, laid-out, read-only document. */
export class ViewerDocument {
  #engine;

  /** @internal Use `openDocument`. */
  constructor(engine) {
    this.#engine = engine;
  }

  #live() {
    if (!this.#engine) throw new Error("this document has been freed");
    return this.#engine;
  }

  /** The number of laid-out pages. */
  get pageCount() {
    return this.#live().pageCount;
  }

  /** Page `index`'s size, in twips and points. Pages can differ. */
  pageSize(index) {
    const [widthTwips, heightTwips] = this.#live().pageSizeTwips(index);
    if (widthTwips === undefined) throw new RangeError(`page ${index} does not exist; the document has ${this.pageCount}`);
    return { widthTwips, heightTwips, widthPt: widthTwips / TWIPS_PER_POINT, heightPt: heightTwips / TWIPS_PER_POINT };
  }

  /** Every page's size, in page order. */
  pageSizes() {
    const flat = this.#live().pageSizesTwips();
    const sizes = [];
    for (let i = 0; i < flat.length; i += 2) {
      sizes.push({
        widthTwips: flat[i],
        heightTwips: flat[i + 1],
        widthPt: flat[i] / TWIPS_PER_POINT,
        heightPt: flat[i + 1] / TWIPS_PER_POINT,
      });
    }
    return sizes;
  }

  /** Page `index` as premultiplied RGBA at `dpi` (default 96), ready for
   *  `new ImageData(rgba, width, height)`. */
  renderPage(index, { dpi = 96 } = {}) {
    const bitmap = this.#live().renderPage(index, dpi);
    try {
      const rgba = bitmap.rgba();
      return {
        width: bitmap.widthPx,
        height: bitmap.heightPx,
        rgba: new Uint8ClampedArray(rgba.buffer, rgba.byteOffset, rgba.byteLength),
      };
    } finally {
      bitmap.free();
    }
  }

  /** Page `index` as PNG bytes at `dpi` (default 96): thumbnails, servers. */
  renderPagePng(index, { dpi = 96 } = {}) {
    return this.#live().renderPagePng(index, dpi);
  }

  /** The body text laid out on page `index`. The pages' texts concatenate to
   *  `text()`. Headers, footers and notes are not included. */
  pageText(index) {
    return this.#live().pageText(index);
  }

  /** The whole body text, one line per paragraph. */
  text() {
    return this.#live().plainText();
  }

  /** The headings, in order: `{ level, text, page }`, `page` 0-based or null. */
  outline() {
    return JSON.parse(this.#live().outlineJson());
  }

  /** The fonts the layout needed and did not have:
   *  `{ requested, substitute, substituteAvailable }`. */
  fontNeeds() {
    return JSON.parse(this.#live().fontNeedsJson());
  }

  /** Code points no registered face could draw (they painted as boxes). */
  missingCodePoints() {
    return Array.from(this.#live().missingCodePoints());
  }

  /** Registers faces under the family names in their `name` tables and lays
   *  the document out once. Returns how many faces registered. */
  registerFonts(faces) {
    if (!faces.length) return 0;
    return this.#live().registerFonts(...packFaces(faces));
  }

  /** Registers a face as the fallback for ISO 15924 `scripts` (`["Hani",
   *  "Hira", "Kana"]`, `["Zyyy"]` for emoji) and lays out once. */
  registerFallbackFont(face, scripts) {
    return this.#live().registerFallbackFont(toBytes(face, "the font"), scripts.join(","));
  }

  /** Whether this build can write PDF. */
  get hasPdf() {
    return WasmViewer.hasPdf();
  }

  /** The whole document as real-text vector PDF bytes. */
  toPdf() {
    const engine = this.#live();
    if (!WasmViewer.hasPdf()) throw new Error("this build of the viewer has no PDF writer (feature `pdf`)");
    return engine.toPdf();
  }

  /** Releases the engine memory. The document is unusable afterwards. */
  free() {
    this.#engine?.free();
    this.#engine = null;
  }
}
