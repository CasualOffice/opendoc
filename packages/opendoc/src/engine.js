// SPDX-License-Identifier: Apache-2.0
//
// The engine binding: loads the WebAssembly module (crates/opendoc, feature
// `wasm`) and wraps `WasmDocument` in an idiomatic `Document`. No DOM: this
// runs in Node, Deno, Bun, a worker or a page.

import initWasm, {
  WasmBuilder,
  WasmDocument,
  convert as wasmConvert,
  formats as wasmFormats,
  version as wasmVersion,
} from "../pkg/opendoc.js";

let ready = null;
let loader = null;

/**
 * Loads the engine module once. In Node, import from `@casualoffice/opendoc/node`
 * (or call `init({ wasm: bytes })`); in a browser the module is fetched next to
 * the glue unless `wasm` says where it is.
 *
 * @param {{ wasm?: URL | string | Response | BufferSource | WebAssembly.Module }} [options]
 */
export function init(options = {}) {
  if (!ready) {
    const source = options.wasm;
    ready = (source === undefined ? initWasm() : initWasm({ module_or_path: source })).catch((error) => {
      ready = null;
      throw error;
    });
  }
  return ready;
}

/** Lets the Node entry supply how to load the module on first use. */
export function setLoader(load) {
  loader = load;
}

/** Resolves when the engine is loaded, loading it with the registered loader. */
export async function ensureReady() {
  if (ready) return ready;
  if (loader) return init({ wasm: await loader() });
  return init();
}

function assertReady() {
  if (!ready) throw new OpenDocError("not-initialised", "call `await init()` (or import from @casualoffice/opendoc/node) first");
}

/**
 * Every error the SDK throws. `code` is stable (`open`, `unsupported-format`,
 * `save`, `edit`, `invalid-argument`, `build`, `template`, `render`,
 * `exhausted`). A template error also carries docxtemplater's shape:
 * `properties.id === "multi_error"` and `properties.errors[]`, each with
 * `properties.id`, `properties.explanation` and `properties.xtag`.
 */
export class OpenDocError extends Error {
  constructor(code, message, properties) {
    super(message);
    this.name = code === "template" ? "TemplateError" : "OpenDocError";
    this.code = code;
    if (properties) this.properties = properties;
  }
}

function wrap(error) {
  if (error instanceof OpenDocError) return error;
  const text = String(error?.message ?? error);
  const match = /^([a-z-]+): ([\s\S]*)$/.exec(text);
  if (!match) return new OpenDocError("engine", text);
  const [, code, rest] = match;
  if (code === "template") {
    const bar = rest.indexOf(" | ");
    const issues = bar >= 0 ? safeJson(rest.slice(bar + 3), []) : [];
    const errors = issues.map((issue) => ({
      name: "TemplateError",
      message: issue.explanation,
      properties: { id: issue.id, explanation: issue.explanation, xtag: issue.tag, paragraph: issue.paragraph },
    }));
    return new OpenDocError("template", bar >= 0 ? rest.slice(0, bar) : rest, {
      id: "multi_error",
      errors,
    });
  }
  return new OpenDocError(code, rest);
}

function safeJson(text, fallback) {
  try {
    return JSON.parse(text);
  } catch {
    return fallback;
  }
}

function call(fn) {
  try {
    return fn();
  } catch (error) {
    throw wrap(error);
  }
}

function bytesOf(value, what = "bytes") {
  if (value instanceof Uint8Array) return value;
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  if (ArrayBuffer.isView(value)) return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  throw new TypeError(`${what} must be a Uint8Array, an ArrayBuffer or a typed array`);
}

function templateOptions(options = {}) {
  const out = {};
  if (options.delimiters) out.delimiters = options.delimiters;
  if (typeof options.nullGetter === "function") out.nullValue = String(options.nullGetter({}) ?? "");
  if (typeof options.nullValue === "string") out.nullValue = options.nullValue;
  if (options.strict) out.strict = true;
  return JSON.stringify(out);
}

/**
 * An open document. Reads return plain objects (the Rust read view's JSON);
 * edits are atomic batches through the engine's operation set; `free()` releases
 * the engine memory (the object is unusable after).
 */
export class Document {
  #inner;

  constructor(inner) {
    this.#inner = inner;
  }

  /**
   * Opens bytes, detecting the format (DOCX, ODT, RTF, TXT, JSON).
   * @param {BufferSource} bytes
   * @param {{ format?: string, fileName?: string, author?: string }} [options]
   */
  static open(bytes, options = {}) {
    assertReady();
    return new Document(
      call(() => WasmDocument.open(bytesOf(bytes), options.format ?? "", options.fileName ?? "", options.author ?? "")),
    );
  }

  /** A blank document with Word's built-in styles. */
  static create() {
    assertReady();
    return new Document(call(() => WasmDocument.create()));
  }

  /** Builds from a docx-js-shaped spec object (see `@casualoffice/opendoc/docx`). */
  static fromSpec(spec, media = []) {
    assertReady();
    const builder = new WasmBuilder();
    try {
      for (const item of media) builder.addMedia(bytesOf(item, "media"));
      return new Document(call(() => builder.build(JSON.stringify(spec))));
    } finally {
      builder.free();
    }
  }

  get #doc() {
    if (!this.#inner) throw new OpenDocError("invalid-argument", "this document was freed");
    return this.#inner;
  }

  /** Releases the engine memory. */
  free() {
    this.#inner?.free();
    this.#inner = null;
  }

  /** The detected source format (`"docx"`, `"odt"`, ...), or `""` for a new document. */
  get sourceFormat() {
    return this.#doc.sourceFormat();
  }
  /** Whether it changed since it was opened. */
  get isModified() {
    return this.#doc.isModified();
  }
  /** What the open did not carry into the model, and what it repaired. */
  get importReport() {
    return JSON.parse(this.#doc.importReport());
  }
  /** The body's blocks: `{ type: "paragraph" | "table" | "contentControl" | "other", ... }`. */
  get body() {
    return JSON.parse(this.#doc.body());
  }
  /** The body's top-level paragraphs (python-docx `document.paragraphs`). */
  get paragraphs() {
    return this.body.filter((b) => b.type === "paragraph");
  }
  /** The body's top-level tables. */
  get tables() {
    return this.body.filter((b) => b.type === "table");
  }
  /** Plain text, one line per paragraph. */
  get text() {
    return this.#doc.text();
  }
  /** Page setup per section. */
  get sections() {
    return JSON.parse(this.#doc.sections());
  }
  /** Style definitions. */
  get styles() {
    return JSON.parse(this.#doc.styles());
  }
  /** Comments with their anchors. */
  get comments() {
    return JSON.parse(this.#doc.comments());
  }
  /** Tracked changes. */
  get revisions() {
    return JSON.parse(this.#doc.revisions());
  }
  /** Headers and footers. */
  get headersFooters() {
    return JSON.parse(this.#doc.headersFooters());
  }
  /** Footnotes and endnotes. */
  get notes() {
    return JSON.parse(this.#doc.notes());
  }
  /** Media parts. */
  get media() {
    return JSON.parse(this.#doc.media());
  }
  /** A media part's bytes, or `null`. */
  mediaBytes(part) {
    const bytes = this.#doc.mediaBytes(part);
    return bytes.length ? bytes : null;
  }
  /** Core and custom properties. */
  get properties() {
    return JSON.parse(this.#doc.properties());
  }

  /** Sets the author comments and suggestions are attributed to. */
  set author(name) {
    this.#doc.setAuthor(String(name));
  }

  /**
   * Runs edit commands as one atomic batch: `[{ op: "insertText", paragraph, offset, text }, ...]`.
   * Offsets are Unicode code points (`Array.from(text)` indices).
   * @returns {{ results: unknown[], undo: string }}
   */
  edit(commands) {
    return JSON.parse(call(() => this.#doc.edit(JSON.stringify(commands))));
  }

  /** Applies an `undo` from `edit`; returns the redo. */
  undo(undo) {
    return call(() => this.#doc.undo(undo));
  }

  #one(command) {
    return this.edit([command]).results[0];
  }

  /** Appends a paragraph; returns its id. python-docx `add_paragraph`. */
  addParagraph(text = "", { style } = {}) {
    return this.#one({ op: "insertParagraph", text, style });
  }
  /** Appends a heading (0 = Title, 1-9); returns its id. python-docx `add_heading`. */
  addHeading(text, level = 1) {
    return this.#one({ op: "insertParagraph", text, style: level === 0 ? "Title" : `Heading ${level}` });
  }
  /** Inserts a paragraph before or after a block. */
  insertParagraph(text, { before, after, style } = {}) {
    return this.#one({ op: "insertParagraph", text, before, after, style });
  }
  /** Appends a run with formatting to a paragraph. python-docx `add_run`. */
  addRun(paragraph, text, format = {}) {
    this.#one({ op: "appendRun", paragraph, text, format });
  }
  /** Inserts text at a code-point offset. */
  insertText(paragraph, offset, text) {
    this.#one({ op: "insertText", paragraph, offset, text });
  }
  /** Deletes `[start, end)`. */
  deleteText(paragraph, start, end) {
    this.#one({ op: "deleteText", paragraph, start, end });
  }
  /** Replaces every occurrence everywhere; returns the count. Aspose `Range.Replace`. */
  replaceAll(find, replace) {
    return this.#one({ op: "replaceAll", find, replace });
  }
  /** Sets character formatting over `[start, end)`. */
  setRunFormat(paragraph, start, end, format) {
    this.#one({ op: "setRunFormat", paragraph, start, end, format });
  }
  /** Links `[start, end)` to a URL or `#bookmark`. */
  setHyperlink(paragraph, start, end, target) {
    this.#one({ op: "setHyperlink", paragraph, start, end, target });
  }
  /** Applies a paragraph style by name. */
  setParagraphStyle(paragraph, style) {
    this.#one({ op: "setParagraphStyle", paragraph, style });
  }
  /** Sets alignment: left, center, right, justify. */
  setAlignment(paragraph, alignment) {
    this.#one({ op: "setAlignment", paragraph, alignment });
  }
  /** Deletes a paragraph or table. */
  deleteBlock(id) {
    this.#one({ op: "deleteBlock", id });
  }
  /** Appends (or inserts) a table filled from `cells`; returns its id. python-docx `add_table`. */
  addTable(rows, columns, { cells = [], before, after } = {}) {
    return this.#one({ op: "insertTable", rows, columns, cells, before, after });
  }
  /** Sets a cell's text. */
  setCellText(table, row, column, text) {
    this.#one({ op: "setCellText", table, row, column, text });
  }
  /** Appends a row; returns its id. */
  addRow(table, cells = []) {
    return this.#one({ op: "addRow", table, cells });
  }
  /** Deletes a row. */
  deleteRow(table, index) {
    this.#one({ op: "deleteRow", table, index });
  }
  /** Comments on `[start, end)`; returns the comment id. */
  addComment(paragraph, start, end, text, { date } = {}) {
    return this.#one({ op: "addComment", paragraph, start, end, text, date });
  }
  /** A tracked insertion. */
  suggestInsert(paragraph, offset, text, { date } = {}) {
    this.#one({ op: "suggestInsert", paragraph, offset, text, date });
  }
  /** A tracked deletion. */
  suggestDelete(paragraph, start, end, { date } = {}) {
    this.#one({ op: "suggestDelete", paragraph, start, end, date });
  }
  /** A tracked replacement. */
  suggestReplace(paragraph, start, end, text, { date } = {}) {
    this.#one({ op: "suggestReplace", paragraph, start, end, text, date });
  }
  /** Accepts every tracked change; returns whether there was any. */
  acceptAllRevisions() {
    return this.#one({ op: "acceptAllRevisions" });
  }
  /** Rejects every tracked change. */
  rejectAllRevisions() {
    return this.#one({ op: "rejectAllRevisions" });
  }
  /** Sets core properties (`title`, `subject`, `creator`, `keywords`, `description`, `lastModifiedBy`, `category`). */
  setProperties(properties) {
    this.#one({ op: "setProperties", properties });
  }

  /**
   * Fills docxtemplater tags (`{x}`, `{#items}…{/items}`, `{^x}`, `{.}`) with
   * `data`, through the engine. Throws a `TemplateError` listing every issue
   * (docxtemplater's `multi_error` shape); the document is then unchanged.
   * @param {object} data
   * @param {{ delimiters?: { start: string, end: string }, nullGetter?: () => string, strict?: boolean }} [options]
   */
  render(data, options = {}) {
    call(() => this.#doc.renderTemplate(JSON.stringify(data ?? {}), templateOptions(options)));
    return this;
  }

  /** The tags a template uses: `[{ name, kind: "value" | "loop" | "inverted" }]`. */
  templateTags(options = {}) {
    return JSON.parse(call(() => this.#doc.templateTags(templateOptions(options))));
  }

  /**
   * Saves. `format`: docx, dotx, odt, txt, json, html, md, pdf. `mode` (same
   * format only): `"exact"` (original bytes, unchanged documents only),
   * `"preserve"` (default), `"semantic"`.
   * @returns {{ bytes: Uint8Array, report: object }}
   */
  save(format = "docx", { mode = "" } = {}) {
    const bytes = call(() => this.#doc.save(format, mode));
    return { bytes, report: JSON.parse(this.#doc.lastSaveReport()) };
  }

  /** DOCX bytes (docxtemplater `toBuffer`, docx-js `Packer.toBuffer`). */
  toBuffer() {
    const { bytes } = this.save("docx");
    return typeof Buffer !== "undefined" ? Buffer.from(bytes) : bytes;
  }

  /** The number of laid-out pages. */
  get pageCount() {
    return this.#doc.pageCount();
  }
  /** Page sizes in points. */
  pageSizes() {
    return JSON.parse(this.#doc.pageSizes());
  }
  /** The text on one page. */
  pageText(index) {
    return call(() => this.#doc.pageText(index));
  }
  /** One page as RGBA. */
  renderPage(index, { dpi = 96 } = {}) {
    const rgba = call(() => this.#doc.renderPageRgba(index, dpi));
    return { width: this.#doc.lastRenderWidth(), height: this.#doc.lastRenderHeight(), rgba };
  }
  /** One page as PNG bytes. */
  renderPagePng(index, { dpi = 96 } = {}) {
    return call(() => this.#doc.renderPagePng(index, dpi));
  }
  /** A real-text PDF from the same layout. */
  toPdf() {
    return call(() => this.#doc.toPdf());
  }
  /** Headings with their pages. */
  outline() {
    return JSON.parse(this.#doc.outline());
  }
  /** Families layout had to substitute. */
  fontNeeds() {
    return JSON.parse(this.#doc.fontNeeds());
  }
  /** Adds a font file (TTF/OTF) to lay out with. */
  registerFont(bytes) {
    this.#doc.registerFont(bytesOf(bytes, "font"));
  }
}

/**
 * Converts between formats in one call.
 * @param {BufferSource} bytes
 * @param {string} to docx, odt, txt, json, html, md, pdf
 * @returns {Uint8Array}
 */
export function convert(bytes, to) {
  assertReady();
  return call(() => wasmConvert(bytesOf(bytes), to));
}

/** The formats and their directions. */
export function formats() {
  assertReady();
  return JSON.parse(wasmFormats());
}

/** The engine version. */
export function version() {
  assertReady();
  return wasmVersion();
}

const decoder = new TextDecoder();

function messages(report) {
  return [
    ...report.losses.map((f) => ({ type: "warning", message: `${f.feature} was not kept (${f.occurrences}×)` })),
  ];
}

/**
 * mammoth.js's `convertToHtml`: `{ value, messages }`. Unlike mammoth the HTML
 * keeps fonts, sizes, colours and table borders. Accepts `{ buffer }`,
 * `{ arrayBuffer }` or raw bytes.
 */
export function convertToHtml(input) {
  return renditionOf(input, "html");
}

/** mammoth.js's `extractRawText`: `{ value, messages }`. */
export function extractRawText(input) {
  return renditionOf(input, "txt");
}

/** Markdown, in the same result shape. */
export function convertToMarkdown(input) {
  return renditionOf(input, "md");
}

function renditionOf(input, format) {
  const bytes = input?.buffer && !(input instanceof Uint8Array) ? input.buffer : input?.arrayBuffer ?? input;
  const doc = Document.open(bytes);
  try {
    const { bytes: out, report } = doc.save(format);
    return { value: decoder.decode(out), messages: messages(report) };
  } finally {
    doc.free();
  }
}
