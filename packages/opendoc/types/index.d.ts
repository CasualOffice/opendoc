// SPDX-License-Identifier: Apache-2.0
// Types for @casualoffice/opendoc. The read-view shapes mirror crates/opendoc
// `view.rs` field for field (camelCase JSON).

/** Bytes the SDK accepts. */
export type Bytes = Uint8Array | ArrayBuffer | ArrayBufferView;

/** A format's short name. */
export type FormatName = "docx" | "dotx" | "odt" | "rtf" | "txt" | "json" | "html" | "md" | "pdf";

/** How a same-format save treats retained source detail. */
export type SaveMode = "exact" | "preserve" | "semantic";

/** Stable error codes. */
export type ErrorCode =
  | "open"
  | "unsupported-format"
  | "save"
  | "edit"
  | "invalid-argument"
  | "build"
  | "template"
  | "render"
  | "exhausted"
  | "not-initialised"
  | "engine";

/** One template issue, in docxtemplater's shape. */
export interface TemplateIssue {
  name: "TemplateError";
  message: string;
  properties: {
    id:
      | "unclosed_tag"
      | "unopened_tag"
      | "unclosed_loop"
      | "unopened_loop"
      | "closing_tag_does_not_match_opening_tag"
      | "undefined_value";
    explanation: string;
    xtag: string;
    paragraph: string;
  };
}

export declare class OpenDocError extends Error {
  readonly code: ErrorCode;
  /** Present on a template error: docxtemplater's `multi_error`. */
  readonly properties?: { id: "multi_error"; errors: TemplateIssue[] };
}

export interface Finding {
  feature: string;
  occurrences: number;
  part?: string;
  element?: string;
  attribute?: string;
  representation: "mapped" | "degraded" | "omitted";
  retention: "preserved" | "not-retained" | "blocked" | "rejected" | "not-applicable";
  /** Not mapped and not preserved: a save makes it permanent. */
  loss: boolean;
}

export interface ImportReport {
  schema: 1;
  format: string;
  findings: Finding[];
  repairs: { token: string; summary: string; severity: "structural" | "content-dropped" | "body-lost"; part?: string; occurrences: number }[];
}

export interface SaveReport {
  schema: 1;
  format: FormatName;
  mode: SaveMode;
  /** TXT, HTML, Markdown and PDF are renditions, not containers. */
  lossyByNature: boolean;
  findings: Finding[];
  /** What the saved file will not hold. Empty means nothing was lost. */
  losses: Finding[];
}

export interface RunView {
  id: string;
  kind:
    | "text"
    | "tab"
    | "break"
    | "pageBreak"
    | "columnBreak"
    | "image"
    | "noteReference"
    | "commentReference"
    | "symbol"
    | "math"
    | "textBox"
    | "object";
  text: string;
  bold?: boolean;
  italic?: boolean;
  underline?: boolean;
  strike?: boolean;
  font?: string;
  sizePt?: number;
  color?: string;
  highlight?: string;
  verticalAlign?: "superscript" | "subscript";
  style?: string;
  hyperlink?: string;
  revision?: { kind: "insertion" | "deletion" | "moveFrom" | "moveTo"; author?: string; date?: string };
  field?: string;
  image?: { part: string; widthPt?: number; heightPt?: number; description?: string; floating: boolean };
  reference?: string;
}

export interface ParagraphView {
  type: "paragraph";
  id: string;
  style?: string;
  text: string;
  headingLevel?: number;
  alignment?: "start" | "center" | "end" | "justify";
  list?: { numId: string; level: number };
  pageBreakBefore?: boolean;
  runs: RunView[];
}

export interface CellView {
  id: string;
  columnSpan: number;
  verticalMerge?: "restart" | "continue";
  blocks: Block[];
}

export interface TableView {
  type: "table";
  id: string;
  style?: string;
  columnWidthsPt: number[];
  rows: { id: string; header?: boolean; cells: CellView[] }[];
}

export interface ContentControlView {
  type: "contentControl";
  id: string;
  tag?: string;
  alias?: string;
  blocks: Block[];
}

export type Block = ParagraphView | TableView | ContentControlView | { type: "other"; id: string; kind: string };

export interface SectionView {
  id: string;
  pageWidthPt: number;
  pageHeightPt: number;
  orientation: "portrait" | "landscape";
  marginsPt: { top: number; bottom: number; left: number; right: number; header?: number; footer?: number };
  columns: number;
  titlePage: boolean;
  /** How the section starts. */
  start: SectionStart;
  /** Even pages use their own header and footer variant (document-wide). */
  evenAndOddHeaders: boolean;
  headers: { kind: "default" | "first" | "even"; id: string }[];
  footers: { kind: "default" | "first" | "even"; id: string }[];
}

/** How a section starts. */
export type SectionStart = "nextPage" | "continuous" | "evenPage" | "oddPage" | "nextColumn";
/** Which page a header or footer applies to. */
export type PageKind = "default" | "first" | "even";
/** The fields a header or footer (or any paragraph) can hold. */
export type FieldKind = "page" | "numpages" | "date" | "time" | "filename" | "author";
/** A picture: bytes or base64. PNG, JPEG, GIF, BMP, TIFF or WebP. */
export type PictureData = Uint8Array | ArrayBuffer | string;
/** A section's page setup; omitted values stay. Points throughout. */
export interface PageSetup {
  widthPt?: number;
  heightPt?: number;
  orientation?: "portrait" | "landscape";
  topPt?: number;
  bottomPt?: number;
  leftPt?: number;
  rightPt?: number;
  headerPt?: number;
  footerPt?: number;
  columns?: number;
  columnSpacingPt?: number;
}

export interface StyleView {
  id: string;
  name?: string;
  kind: "paragraph" | "character" | "table" | "numbering";
  basedOn?: string;
  isDefault: boolean;
  custom: boolean;
}

export interface Anchor {
  paragraph: string;
  start: number;
  end: number;
}

export interface CommentView {
  id: string;
  author?: string;
  initials?: string;
  date?: string;
  resolved: boolean;
  text: string;
  anchor?: Anchor;
  replyTo?: string;
}

export interface RevisionView {
  id: string;
  kind: "insertion" | "deletion" | "move_from" | "move_to" | "formatting";
  author?: string;
  date?: string;
  text: string;
  anchor: Anchor;
}

export interface Properties {
  title?: string;
  subject?: string;
  creator?: string;
  keywords?: string;
  description?: string;
  lastModifiedBy?: string;
  revision?: string;
  created?: string;
  modified?: string;
  category?: string;
  custom: Record<string, string>;
}

export interface RunFormat {
  bold?: boolean;
  italic?: boolean;
  underline?: boolean;
  strike?: boolean;
  /** `RRGGBB`. */
  color?: string;
  sizePt?: number;
  font?: string;
  highlight?: string;
  verticalAlign?: "superscript" | "subscript" | "baseline";
}

/** One edit as data. Offsets are Unicode code points (`Array.from(text)` indices). */
export type EditCommand =
  | { op: "insertText"; paragraph: string; offset: number; text: string }
  | { op: "deleteText"; paragraph: string; start: number; end: number }
  | { op: "replaceText"; paragraph: string; start: number; end: number; text: string }
  | { op: "replaceAll"; find: string; replace: string }
  | { op: "setRunFormat"; paragraph: string; start: number; end: number; format: RunFormat }
  | { op: "clearFormatting"; paragraph: string; start: number; end: number }
  | { op: "setHyperlink"; paragraph: string; start: number; end: number; target: string }
  | { op: "setParagraphStyle"; paragraph: string; style: string }
  | { op: "setAlignment"; paragraph: string; alignment: "left" | "center" | "right" | "justify" | "start" | "end" }
  | { op: "setPageBreakBefore"; paragraph: string; on: boolean }
  | { op: "insertParagraph"; at?: "start" | "end"; before?: string; after?: string; text?: string; style?: string }
  | { op: "appendRun"; paragraph: string; text: string; format?: RunFormat }
  | { op: "deleteBlock"; id: string }
  | { op: "insertTable"; at?: "start" | "end"; before?: string; after?: string; rows: number; columns: number; cells?: string[][] }
  | { op: "setCellText"; table: string; row: number; column: number; text: string }
  | { op: "addRow"; table: string; cells?: string[] }
  | { op: "deleteRow"; table: string; index: number }
  | { op: "addComment"; paragraph: string; start: number; end: number; text: string; date?: string }
  | { op: "suggestInsert"; paragraph: string; offset: number; text: string; date?: string }
  | { op: "suggestDelete"; paragraph: string; start: number; end: number; date?: string }
  | { op: "suggestReplace"; paragraph: string; start: number; end: number; text: string; date?: string }
  | { op: "acceptAllRevisions" }
  | { op: "rejectAllRevisions" }
  | { op: "setProperties"; properties: Partial<Pick<Properties, "title" | "subject" | "creator" | "keywords" | "description" | "lastModifiedBy" | "category">> }
  | { op: "insertPicture"; paragraph: string; offset?: number; image: string; widthPt?: number; heightPt?: number }
  | { op: "replacePicture"; picture: string; image: string }
  | { op: "resizePicture"; picture: string; widthPt: number; heightPt: number }
  | { op: "setAltText"; picture: string; text?: string }
  | { op: "deletePicture"; picture: string }
  | { op: "headerFooter"; section: number; region: "header" | "footer"; kind?: PageKind }
  | { op: "removeHeaderFooter"; section: number; region: "header" | "footer"; kind?: PageKind }
  | { op: "setLinkedToPrevious"; section: number; region: "header" | "footer"; kind?: PageKind; linked: boolean }
  | { op: "setDifferentFirstPage"; section: number; on: boolean }
  | { op: "setDifferentOddEven"; on: boolean }
  | { op: "insertField"; paragraph: string; offset?: number; kind: FieldKind; result?: string }
  | { op: "insertNote"; kind: "footnote" | "endnote"; paragraph: string; offset: number; text?: string }
  | { op: "setNoteText"; note: string; text: string }
  | { op: "deleteNote"; note: string }
  | { op: "addSection"; start?: SectionStart }
  | { op: "insertSectionBreak"; paragraph: string; offset: number; start?: SectionStart }
  | { op: "setSectionStart"; section: number; start: SectionStart }
  | { op: "setPageSetup"; section: number; setup: PageSetup };

export interface TemplateOptions {
  delimiters?: { start: string; end: string };
  /** docxtemplater's nullGetter; its return value (a constant) is what a missing value prints. Default `""`. */
  nullGetter?: () => string;
  /** Fail on a missing value (`undefined_value`). */
  strict?: boolean;
}

export interface OpenOptions {
  format?: FormatName;
  fileName?: string;
  /** The author comments and suggestions are attributed to. Default "OpenDoc". */
  author?: string;
}

export declare class Document {
  static open(bytes: Bytes, options?: OpenOptions): Document;
  static create(): Document;
  static fromSpec(spec: object, media?: Bytes[]): Document;

  free(): void;
  readonly sourceFormat: FormatName | "";
  readonly isModified: boolean;
  readonly importReport: ImportReport;
  readonly body: Block[];
  readonly paragraphs: ParagraphView[];
  readonly tables: TableView[];
  readonly text: string;
  readonly sections: SectionView[];
  readonly styles: StyleView[];
  readonly comments: CommentView[];
  readonly revisions: RevisionView[];
  readonly headersFooters: { id: string; part: "header" | "footer"; blocks: Block[]; text: string }[];
  readonly notes: { id: string; kind: "footnote" | "endnote"; blocks: Block[]; text: string }[];
  readonly media: { id: string; part: string; contentType: string; bytes: number }[];
  mediaBytes(part: string): Uint8Array | null;
  readonly properties: Properties;
  author: string;

  edit(commands: EditCommand[]): { results: unknown[]; undo: string };
  undo(undo: string): string;
  addParagraph(text?: string, options?: { style?: string }): string;
  addHeading(text: string, level?: number): string;
  insertParagraph(text: string, options?: { before?: string; after?: string; style?: string }): string;
  addRun(paragraph: string, text: string, format?: RunFormat): void;
  insertText(paragraph: string, offset: number, text: string): void;
  deleteText(paragraph: string, start: number, end: number): void;
  replaceAll(find: string, replace: string): number;
  setRunFormat(paragraph: string, start: number, end: number, format: RunFormat): void;
  setHyperlink(paragraph: string, start: number, end: number, target: string): void;
  setParagraphStyle(paragraph: string, style: string): void;
  setAlignment(paragraph: string, alignment: string): void;
  deleteBlock(id: string): void;
  addTable(rows: number, columns: number, options?: { cells?: string[][]; before?: string; after?: string }): string;
  setCellText(table: string, row: number, column: number, text: string): void;
  addRow(table: string, cells?: string[]): string;
  deleteRow(table: string, index: number): void;
  addComment(paragraph: string, start: number, end: number, text: string, options?: { date?: string }): string;
  suggestInsert(paragraph: string, offset: number, text: string, options?: { date?: string }): void;
  suggestDelete(paragraph: string, start: number, end: number, options?: { date?: string }): void;
  suggestReplace(paragraph: string, start: number, end: number, text: string, options?: { date?: string }): void;
  acceptAllRevisions(): boolean;
  rejectAllRevisions(): boolean;
  setProperties(properties: Partial<Pick<Properties, "title" | "subject" | "creator" | "keywords" | "description" | "lastModifiedBy" | "category">>): void;

  /** Appends a paragraph holding a picture; returns the picture id (python-docx `add_picture`). */
  addPicture(data: PictureData, options?: { widthPt?: number; heightPt?: number; style?: string }): string;
  insertPicture(paragraph: string, offset: number, data: PictureData, options?: { widthPt?: number; heightPt?: number }): string;
  replacePicture(picture: string, data: PictureData): void;
  resizePicture(picture: string, widthPt: number, heightPt: number): void;
  setAltText(picture: string, text: string | null): void;
  deletePicture(picture: string): void;

  /** The first paragraph of a section's header, created on first use; returns its id. */
  header(section?: number, options?: { kind?: PageKind }): string;
  footer(section?: number, options?: { kind?: PageKind }): string;
  removeHeader(section?: number, options?: { kind?: PageKind }): void;
  removeFooter(section?: number, options?: { kind?: PageKind }): void;
  setLinkedToPrevious(section: number, region: "header" | "footer", linked: boolean, options?: { kind?: PageKind }): void;
  setDifferentFirstPage(section: number, on?: boolean): void;
  setDifferentOddEven(on?: boolean): void;
  insertField(paragraph: string, offset: number, kind: FieldKind, options?: { result?: string }): string;

  addFootnote(paragraph: string, offset: number, text: string): string;
  addEndnote(paragraph: string, offset: number, text: string): string;
  setNoteText(note: string, text: string): void;
  deleteNote(note: string): void;

  addSection(start?: SectionStart): number;
  insertSectionBreak(paragraph: string, offset: number, start?: SectionStart): number;
  setSectionStart(section: number, start: SectionStart): void;
  setPageSetup(section: number, setup: PageSetup): void;

  render(data: object, options?: TemplateOptions): this;
  templateTags(options?: TemplateOptions): { name: string; kind: "value" | "loop" | "inverted" }[];

  save(format?: FormatName, options?: { mode?: SaveMode }): { bytes: Uint8Array; report: SaveReport };
  toBuffer(): Uint8Array;

  readonly pageCount: number;
  pageSizes(): { widthPt: number; heightPt: number }[];
  pageText(index: number): string;
  renderPage(index: number, options?: { dpi?: number }): { width: number; height: number; rgba: Uint8Array };
  renderPagePng(index: number, options?: { dpi?: number }): Uint8Array;
  /** One page as SVG: glyph outlines plus a transparent real-text layer, from the same layout as the PNG and PDF. */
  renderPageSvg(index: number): string;
  toPdf(): Uint8Array;
  outline(): { level: number; text: string; page: number | null }[];
  fontNeeds(): string[];
  registerFont(bytes: Bytes): void;
}

/** docxtemplater-familiar facade. */
export declare class Template {
  constructor(bytes: Bytes, options?: TemplateOptions);
  getTags(): { name: string; kind: "value" | "loop" | "inverted" }[];
  render(data: object): this;
  toBuffer(): Uint8Array;
  save(format?: FormatName, options?: { mode?: SaveMode }): { bytes: Uint8Array; report: SaveReport };
  toPdf(): Uint8Array;
  readonly document: Document;
  free(): void;
}

/** mammoth.js-shaped result. */
export interface Rendition {
  value: string;
  messages: { type: "warning" | "error"; message: string }[];
}
type MammothInput = Bytes | { buffer: Bytes } | { arrayBuffer: Bytes };

export declare function init(options?: { wasm?: URL | string | Response | Bytes | WebAssembly.Module }): Promise<unknown>;
export declare function convert(bytes: Bytes, to: FormatName): Uint8Array;
export declare function convertToHtml(input: MammothInput): Rendition;
export declare function convertToMarkdown(input: MammothInput): Rendition;
export declare function extractRawText(input: MammothInput): Rendition;
export declare function formats(): { name: FormatName; mime: string; open: boolean; save: boolean; carriesDocument: boolean }[];
export declare function version(): string;
