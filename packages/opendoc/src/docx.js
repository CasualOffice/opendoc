// SPDX-License-Identifier: Apache-2.0
//
// A docx-js-familiar builder (docs/176 section 12.2). The class names, option
// names and units are docx-js's (https://docx.js.org): `new Document({ sections })`,
// `Paragraph`, `TextRun({ bold, italics, size, color })`, `HeadingLevel`,
// `Table`/`TableRow`/`TableCell`, `ImageRun`, `ExternalHyperlink`, `Header`,
// `Footer`, `PageNumber`, `Packer.toBuffer`. Moving a docx-js program here is a
// change of import:
//
//   - import { Document, Packer, Paragraph, TextRun } from "docx";
//   + import { Document, Packer, Paragraph, TextRun } from "@casualoffice/opendoc/docx";
//
// What differs: the objects are plain data. `Packer` hands the whole tree to the
// OpenDoc engine as JSON (crates/opendoc `DocumentSpec`), which writes the
// package, opens it, and can then also lay it out: `Packer.toPdf`,
// `Packer.toPng` and `Packer.open` have no docx-js equivalent. An unknown or
// invalid value (a style that does not exist, a colour that is not hex, a
// comment id never defined) is an error naming it, not a silent skip.

import { Document as EngineDocument, ensureReady, OpenDocError } from "./engine.js";

const TYPE = Symbol("opendoc.type");

function plain(value) {
  if (value === undefined) return undefined;
  if (value === null) return null;
  if (Array.isArray(value)) return value.map(plain);
  if (typeof value === "object" && typeof value.toSpec === "function") return value.toSpec();
  if (value instanceof Uint8Array || value instanceof ArrayBuffer) return value;
  if (typeof value === "object") {
    const out = {};
    for (const [key, v] of Object.entries(value)) {
      if (v !== undefined) out[key] = plain(v);
    }
    return out;
  }
  return value;
}

/** docx-js `HeadingLevel`: the built-in style ids. */
export const HeadingLevel = Object.freeze({
  HEADING_1: "Heading1",
  HEADING_2: "Heading2",
  HEADING_3: "Heading3",
  HEADING_4: "Heading4",
  HEADING_5: "Heading5",
  HEADING_6: "Heading6",
  TITLE: "Title",
});

/** docx-js `AlignmentType`. */
export const AlignmentType = Object.freeze({
  START: "start",
  END: "end",
  LEFT: "left",
  RIGHT: "right",
  CENTER: "center",
  BOTH: "both",
  JUSTIFIED: "both",
  DISTRIBUTE: "distribute",
});

/** docx-js `WidthType`. */
export const WidthType = Object.freeze({ DXA: "dxa", PERCENTAGE: "pct", AUTO: "auto", NIL: "auto" });

/** docx-js `LevelFormat`. */
export const LevelFormat = Object.freeze({
  DECIMAL: "decimal",
  BULLET: "bullet",
  UPPER_ROMAN: "upperRoman",
  LOWER_ROMAN: "lowerRoman",
  UPPER_LETTER: "upperLetter",
  LOWER_LETTER: "lowerLetter",
  DECIMAL_ZERO: "decimalZero",
  ORDINAL: "ordinal",
  NONE: "none",
});

/** docx-js `NumberFormat` (page numbering). */
export const NumberFormat = Object.freeze({
  DECIMAL: "decimal",
  UPPER_ROMAN: "upperRoman",
  LOWER_ROMAN: "lowerRoman",
  UPPER_LETTER: "upperLetter",
  LOWER_LETTER: "lowerLetter",
});

/** docx-js `UnderlineType`. */
export const UnderlineType = Object.freeze({
  SINGLE: "single",
  WORDS: "words",
  DOUBLE: "double",
  THICK: "thick",
  DOTTED: "dotted",
  DASH: "dash",
  DOTDASH: "dotDash",
  WAVE: "wave",
  NONE: "none",
});

/** docx-js `PageOrientation`. */
export const PageOrientation = Object.freeze({ PORTRAIT: "portrait", LANDSCAPE: "landscape" });

/** docx-js `SectionType`. */
export const SectionType = Object.freeze({
  NEXT_PAGE: "nextPage",
  NEXT_COLUMN: "nextColumn",
  CONTINUOUS: "continuous",
  EVEN_PAGE: "evenPage",
  ODD_PAGE: "oddPage",
});

/** docx-js `ShadingType`. */
export const ShadingType = Object.freeze({ CLEAR: "clear", SOLID: "solid", PERCENT_10: "pct10", PERCENT_25: "pct25" });

/** docx-js `VerticalAlign`. */
export const VerticalAlign = Object.freeze({ TOP: "top", CENTER: "center", BOTTOM: "bottom" });

/** docx-js `HeightRule`. */
export const HeightRule = Object.freeze({ AUTO: "auto", ATLEAST: "atLeast", EXACT: "exact" });

/** docx-js `PageNumber`: tokens a `TextRun`'s `children` may hold. */
export const PageNumber = Object.freeze({
  CURRENT: Object.freeze({ [TYPE]: "field", instruction: "PAGE" }),
  TOTAL_PAGES: Object.freeze({ [TYPE]: "field", instruction: "NUMPAGES" }),
  TOTAL_PAGES_IN_SECTION: Object.freeze({ [TYPE]: "field", instruction: "SECTIONPAGES" }),
});

/** Converts docx-js unit helpers' inputs: twips are numbers already. */
export const convertInchesToTwip = (inches) => Math.round(inches * 1440);
/** Millimetres to twips. */
export const convertMillimetersToTwip = (mm) => Math.round((mm / 25.4) * 1440);

function normalizeRunOptions(options) {
  const { font, children, ...rest } = options;
  const out = { ...rest };
  if (font !== undefined) out.font = typeof font === "string" ? font : font?.name;
  if (rest.underline !== undefined && typeof rest.underline === "object") {
    out.underline = { type: rest.underline.type, color: rest.underline.color };
  }
  return { out, children };
}

/** A run of text (docx-js `TextRun`). */
export class TextRun {
  /** @param {string | object} options */
  constructor(options) {
    this.options = typeof options === "string" ? { text: options } : { ...options };
  }

  toSpec() {
    const { out, children } = normalizeRunOptions(this.options);
    if (!children) return [{ type: "text", text: "", ...out }];
    // docx-js lets a run hold strings, page-number tokens and tabs.
    const format = { ...out };
    delete format.text;
    delete format.break;
    const pieces = [];
    if (out.text) pieces.push({ type: "text", ...out });
    else if (out.break) pieces.push({ type: "text", text: "", break: out.break, ...format });
    for (const child of children) {
      if (typeof child === "string") pieces.push({ type: "text", text: child, ...format });
      else if (child?.[TYPE] === "field") pieces.push({ type: "field", instruction: child.instruction });
      else if (child instanceof Tab) pieces.push({ type: "tab" });
      else pieces.push(...[].concat(plain(child)));
    }
    return pieces;
  }
}

/** A tab (docx-js `Tab`, inside a run's children or a paragraph's). */
export class Tab {
  toSpec() {
    return { type: "tab" };
  }
}

/** A page break inside a paragraph (docx-js `PageBreak`). */
export class PageBreak {
  toSpec() {
    return { type: "pageBreak" };
  }
}

/** A column break (docx-js `ColumnBreak`). */
export class ColumnBreak {
  toSpec() {
    return { type: "columnBreak" };
  }
}

function toUint8(data) {
  if (data instanceof Uint8Array) return data;
  if (data instanceof ArrayBuffer) return new Uint8Array(data);
  if (ArrayBuffer.isView(data)) return new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
  return null;
}

/** A picture (docx-js `ImageRun`). `data` is bytes or base64; size in pixels. */
export class ImageRun {
  constructor(options) {
    this.options = { ...options };
  }

  toSpec(media) {
    const { data, type, transformation, altText } = this.options;
    const spec = { type: "image", imageType: type, transformation, altText };
    const bytes = toUint8(data);
    if (bytes && media) {
      spec.media = media.length;
      media.push(bytes);
    } else if (typeof data === "string") {
      spec.data = data;
    } else {
      throw new TypeError("ImageRun data must be a Uint8Array, an ArrayBuffer or a base64 string");
    }
    return spec;
  }
}

/** A link to a URL (docx-js `ExternalHyperlink`). */
export class ExternalHyperlink {
  constructor({ children, link }) {
    this.children = children ?? [];
    this.link = link;
  }

  toSpec(media) {
    return { type: "hyperlink", link: this.link, children: inlines(this.children, media) };
  }
}

/** A link to a bookmark (docx-js `InternalHyperlink`). */
export class InternalHyperlink {
  constructor({ children, anchor }) {
    this.children = children ?? [];
    this.anchor = anchor;
  }

  toSpec(media) {
    return { type: "internalHyperlink", anchor: this.anchor, children: inlines(this.children, media) };
  }
}

/** A bookmark (docx-js `Bookmark`). */
export class Bookmark {
  constructor({ id, children }) {
    this.id = id;
    this.children = children ?? [];
  }

  toSpec(media) {
    return { type: "bookmark", id: this.id, children: inlines(this.children, media) };
  }
}

/** A footnote reference (docx-js `FootnoteReferenceRun`). */
export class FootnoteReferenceRun {
  constructor(id) {
    this.id = String(id);
  }

  toSpec() {
    return { type: "footnoteReference", id: this.id };
  }
}

/** An endnote reference. */
export class EndnoteReferenceRun {
  constructor(id) {
    this.id = String(id);
  }

  toSpec() {
    return { type: "endnoteReference", id: this.id };
  }
}

/** The start of a comment's range (docx-js `CommentRangeStart`). */
export class CommentRangeStart {
  constructor(id) {
    this.id = id;
  }

  toSpec() {
    return { type: "commentRangeStart", id: this.id };
  }
}

/** The end of a comment's range (docx-js `CommentRangeEnd`). */
export class CommentRangeEnd {
  constructor(id) {
    this.id = id;
  }

  toSpec() {
    return { type: "commentRangeEnd", id: this.id };
  }
}

/** A comment's reference mark (docx-js `CommentReference`). */
export class CommentReference {
  constructor(id) {
    this.id = id;
  }

  toSpec() {
    return { type: "commentReference", id: this.id };
  }
}

/** A tracked insertion (docx-js `InsertedTextRun`). */
export class InsertedTextRun {
  constructor(options) {
    this.options = { ...options };
  }

  toSpec() {
    return { type: "insertedText", ...normalizeRunOptions(this.options).out };
  }
}

/** A tracked deletion (docx-js `DeletedTextRun`). */
export class DeletedTextRun {
  constructor(options) {
    this.options = { ...options };
  }

  toSpec() {
    return { type: "deletedText", ...normalizeRunOptions(this.options).out };
  }
}

/** A field with its cached text (`DATE \@ "yyyy-MM-dd"`, `AUTHOR`, ...). */
export class SimpleField {
  constructor(instruction, cachedValue) {
    this.instruction = instruction;
    this.text = cachedValue;
  }

  toSpec() {
    return { type: "field", instruction: this.instruction, text: this.text };
  }
}

function inlines(children, media) {
  const out = [];
  for (const child of children ?? []) {
    if (typeof child === "string") out.push({ type: "text", text: child });
    else if (typeof child?.toSpec === "function") out.push(...[].concat(child.toSpec(media)));
    else if (child?.[TYPE] === "field") out.push({ type: "field", instruction: child.instruction });
    else throw new TypeError(`a paragraph child must be a run, a link, a break or a string, not ${typeof child}`);
  }
  return out;
}

/** A paragraph (docx-js `Paragraph`). */
export class Paragraph {
  /** @param {string | object} options */
  constructor(options = {}) {
    this.options = typeof options === "string" ? { text: options } : { ...options };
  }

  toSpec(media) {
    const { children, ...rest } = this.options;
    return { type: "paragraph", ...plain(rest), children: inlines(children, media) };
  }
}

function width(value) {
  if (!value) return undefined;
  const { size, type } = value;
  // docx-js: `{ size: 50, type: WidthType.PERCENTAGE }` is 50 %.
  if (type === "pct" && typeof size === "number") return { size: `${size}%`, type: "pct" };
  return { size, type };
}

/** A table cell (docx-js `TableCell`). */
export class TableCell {
  constructor(options) {
    this.options = { ...options };
  }

  toSpec(media) {
    const { children, width: w, ...rest } = this.options;
    return { ...plain(rest), width: width(w), children: blocks(children, media) };
  }
}

/** A table row (docx-js `TableRow`). */
export class TableRow {
  constructor(options) {
    this.options = { ...options };
  }

  toSpec(media) {
    const { children, ...rest } = this.options;
    return { ...plain(rest), children: (children ?? []).map((cell) => cell.toSpec(media)) };
  }
}

/** A table (docx-js `Table`). */
export class Table {
  constructor(options) {
    this.options = { ...options };
  }

  toSpec(media) {
    const { rows, width: w, ...rest } = this.options;
    return { type: "table", ...plain(rest), width: width(w), rows: (rows ?? []).map((row) => row.toSpec(media)) };
  }
}

/** A table of contents (docx-js `TableOfContents`). */
export class TableOfContents {
  constructor(title, options = {}) {
    this.title = title;
    this.options = options;
  }

  toSpec() {
    return {
      type: "tableOfContents",
      title: this.title,
      headingStyleRange: this.options.headingStyleRange,
      hyperlink: this.options.hyperlink,
    };
  }
}

function blocks(children, media) {
  return (children ?? []).map((child) => {
    if (typeof child === "string") return { type: "paragraph", text: child };
    if (typeof child?.toSpec !== "function") throw new TypeError("a section child must be a Paragraph, a Table or a TableOfContents");
    return child.toSpec(media);
  });
}

/** A header (docx-js `Header`). */
export class Header {
  constructor({ children } = {}) {
    this.children = children ?? [];
  }

  toSpec(media) {
    return { children: blocks(this.children, media) };
  }
}

/** A footer (docx-js `Footer`). */
export class Footer extends Header {}

function story(group, media) {
  if (!group) return undefined;
  const out = {};
  for (const key of ["default", "first", "even"]) {
    if (group[key]) out[key] = group[key].toSpec(media);
  }
  return out;
}

/** A document under construction (docx-js `Document`). */
export class Document {
  constructor(options = {}) {
    this.options = options;
  }

  /** The engine's spec and the media it indexes. */
  toSpec() {
    const media = [];
    const {
      sections = [],
      comments,
      footnotes,
      endnotes,
      styles,
      numbering,
      features,
      creator,
      title,
      subject,
      description,
      keywords,
      lastModifiedBy,
    } = this.options;
    const notes = (map) => {
      if (!map) return undefined;
      const out = {};
      for (const [id, note] of Object.entries(map)) out[id] = { children: blocks(note.children, media) };
      return out;
    };
    const spec = {
      creator,
      title,
      subject,
      description,
      keywords,
      lastModifiedBy,
      styles: plain(styles),
      numbering: plain(numbering),
      features: plain(features),
      comments: comments && {
        children: comments.children.map((c) => ({
          id: c.id,
          author: c.author,
          initials: c.initials,
          date: c.date instanceof Date ? c.date.toISOString().replace(/\.\d{3}Z$/, "Z") : c.date,
          children: blocks(c.children, media),
        })),
      },
      footnotes: notes(footnotes),
      endnotes: notes(endnotes),
      sections: sections.map((section) => ({
        properties: plain(section.properties),
        headers: story(section.headers, media),
        footers: story(section.footers, media),
        children: blocks(section.children, media),
      })),
    };
    return { spec: JSON.parse(JSON.stringify(spec)), media };
  }
}

async function open(doc) {
  if (!(doc instanceof Document)) throw new TypeError("Packer takes a Document from @casualoffice/opendoc/docx");
  await ensureReady();
  const { spec, media } = doc.toSpec();
  return EngineDocument.fromSpec(spec, media);
}

const isNode = typeof process !== "undefined" && !!process.versions?.node;

function base64(bytes) {
  if (typeof Buffer !== "undefined") return Buffer.from(bytes).toString("base64");
  let binary = "";
  for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(binary);
}

/** docx-js `Packer`, plus what only a layout engine can do. */
export const Packer = Object.freeze({
  /** The DOCX bytes (a `Buffer` in Node, a `Uint8Array` elsewhere). */
  async toBuffer(doc) {
    const engine = await open(doc);
    try {
      const { bytes } = engine.save("docx", { mode: "exact" });
      return isNode && typeof Buffer !== "undefined" ? Buffer.from(bytes) : bytes;
    } finally {
      engine.free();
    }
  },
  /** The DOCX as a `Uint8Array`. */
  async toUint8Array(doc) {
    const engine = await open(doc);
    try {
      return engine.save("docx", { mode: "exact" }).bytes;
    } finally {
      engine.free();
    }
  },
  /** The DOCX as a `Blob` (browsers, and Node 18+). */
  async toBlob(doc) {
    const bytes = await Packer.toUint8Array(doc);
    return new Blob([bytes], { type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document" });
  },
  /** The DOCX as base64. */
  async toBase64String(doc) {
    return base64(await Packer.toUint8Array(doc));
  },
  /** A real-text PDF of the laid-out document. Not in docx-js. */
  async toPdf(doc) {
    const engine = await open(doc);
    try {
      return engine.toPdf();
    } finally {
      engine.free();
    }
  },
  /** One page as PNG. Not in docx-js. */
  async toPng(doc, { page = 0, dpi = 96 } = {}) {
    const engine = await open(doc);
    try {
      return engine.renderPagePng(page, { dpi });
    } finally {
      engine.free();
    }
  },
  /** The built document, open in the engine for reading, editing or saving to
   *  any format. The caller frees it. Not in docx-js. */
  open,
});

export { OpenDocError };
