// SPDX-License-Identifier: Apache-2.0
//
// A docxtemplater-familiar facade (https://docxtemplater.com/docs/):
//
//   const doc = new Docxtemplater(zip, { paragraphLoop: true, linebreaks: true });
//   doc.render(data);
//   const buf = doc.toBuffer();
//
// becomes
//
//   const doc = new Template(bytes, { delimiters, nullGetter });
//   doc.render(data);
//   const buf = doc.toBuffer();
//
// No zip library is needed: the engine opens the package itself. The tags are
// docxtemplater's ({x}, {#x}...{/x}, {^x}, {.}, dotted paths); paragraph loops
// and line breaks are always on. Errors carry docxtemplater's shape
// (`properties.id === "multi_error"`, `properties.errors[]`). Unlike the free
// docxtemplater core, pictures, tables, footnotes, headers and every other part
// of the template are kept because the engine edits the document, not its XML
// text; and the result can be saved to PDF, ODT or HTML or rendered to PNG.

import { Document } from "./engine.js";

export class Template {
  #doc;
  #options;

  /**
   * @param {BufferSource} bytes the template (DOCX, ODT or any format the engine opens)
   * @param {{ delimiters?: { start: string, end: string }, nullGetter?: () => string, strict?: boolean }} [options]
   */
  constructor(bytes, options = {}) {
    this.#doc = Document.open(bytes);
    this.#options = options;
  }

  /** The tags the template uses. docxtemplater's inspect module `getTags`. */
  getTags() {
    return this.#doc.templateTags(this.#options);
  }

  /** Fills the template. Throws a TemplateError listing every issue. */
  render(data) {
    this.#doc.render(data, this.#options);
    return this;
  }

  /** The filled DOCX (a Buffer in Node). */
  toBuffer() {
    return this.#doc.toBuffer();
  }

  /** The filled document in any format: `{ bytes, report }`. */
  save(format = "docx", options) {
    return this.#doc.save(format, options);
  }

  /** The filled document as PDF. */
  toPdf() {
    return this.#doc.toPdf();
  }

  /** The engine document, for reading or further edits. */
  get document() {
    return this.#doc;
  }

  /** Releases the engine memory. */
  free() {
    this.#doc.free();
  }
}
