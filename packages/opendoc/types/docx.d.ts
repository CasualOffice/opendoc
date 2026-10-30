// SPDX-License-Identifier: Apache-2.0
// Types for @casualoffice/opendoc/docx, the docx-js-familiar builder. Option
// names and units are docx-js's: twips for spacing, indents and page sizes;
// half-points for `size`; pixels for image `transformation`.
import type { Bytes, Document as EngineDocument } from "./index.js";

export { OpenDocError } from "./index.js";

export declare const HeadingLevel: {
  readonly HEADING_1: "Heading1";
  readonly HEADING_2: "Heading2";
  readonly HEADING_3: "Heading3";
  readonly HEADING_4: "Heading4";
  readonly HEADING_5: "Heading5";
  readonly HEADING_6: "Heading6";
  readonly TITLE: "Title";
};
export declare const AlignmentType: Record<"START" | "END" | "LEFT" | "RIGHT" | "CENTER" | "BOTH" | "JUSTIFIED" | "DISTRIBUTE", string>;
export declare const WidthType: Record<"DXA" | "PERCENTAGE" | "AUTO" | "NIL", string>;
export declare const LevelFormat: Record<"DECIMAL" | "BULLET" | "UPPER_ROMAN" | "LOWER_ROMAN" | "UPPER_LETTER" | "LOWER_LETTER" | "DECIMAL_ZERO" | "ORDINAL" | "NONE", string>;
export declare const NumberFormat: Record<"DECIMAL" | "UPPER_ROMAN" | "LOWER_ROMAN" | "UPPER_LETTER" | "LOWER_LETTER", string>;
export declare const UnderlineType: Record<"SINGLE" | "WORDS" | "DOUBLE" | "THICK" | "DOTTED" | "DASH" | "DOTDASH" | "WAVE" | "NONE", string>;
export declare const PageOrientation: { readonly PORTRAIT: "portrait"; readonly LANDSCAPE: "landscape" };
export declare const SectionType: Record<"NEXT_PAGE" | "NEXT_COLUMN" | "CONTINUOUS" | "EVEN_PAGE" | "ODD_PAGE", string>;
export declare const ShadingType: Record<"CLEAR" | "SOLID" | "PERCENT_10" | "PERCENT_25", string>;
export declare const VerticalAlign: Record<"TOP" | "CENTER" | "BOTTOM", string>;
export declare const HeightRule: Record<"AUTO" | "ATLEAST" | "EXACT", string>;
export declare const PageNumber: { readonly CURRENT: unknown; readonly TOTAL_PAGES: unknown; readonly TOTAL_PAGES_IN_SECTION: unknown };
export declare function convertInchesToTwip(inches: number): number;
export declare function convertMillimetersToTwip(mm: number): number;

export interface ShadingOptions { fill?: string; color?: string; type?: string }
export interface RunOptions {
  text?: string;
  bold?: boolean;
  italics?: boolean;
  underline?: { type?: string; color?: string } | boolean;
  strike?: boolean;
  doubleStrike?: boolean;
  color?: string;
  /** Half-points: 28 is 14 pt. */
  size?: number;
  font?: string | { name: string };
  highlight?: string;
  superScript?: boolean;
  subScript?: boolean;
  allCaps?: boolean;
  smallCaps?: boolean;
  characterSpacing?: number;
  shading?: ShadingOptions;
  style?: string;
  break?: number;
  children?: (string | Tab | typeof PageNumber[keyof typeof PageNumber])[];
}
export interface ParagraphOptions {
  text?: string;
  children?: ParagraphChild[];
  heading?: (typeof HeadingLevel)[keyof typeof HeadingLevel];
  style?: string;
  alignment?: string;
  spacing?: { before?: number; after?: number; line?: number; lineRule?: "auto" | "exact" | "atLeast" };
  indent?: { left?: number; right?: number; start?: number; end?: number; firstLine?: number; hanging?: number };
  bullet?: { level: number };
  numbering?: { reference: string; level: number; instance?: number };
  keepNext?: boolean;
  keepLines?: boolean;
  pageBreakBefore?: boolean;
  outlineLevel?: number;
  shading?: ShadingOptions;
  thematicBreak?: boolean;
  contextualSpacing?: boolean;
  bookmark?: string;
}
export type ParagraphChild =
  | string
  | TextRun
  | ImageRun
  | ExternalHyperlink
  | InternalHyperlink
  | Bookmark
  | PageBreak
  | ColumnBreak
  | Tab
  | FootnoteReferenceRun
  | EndnoteReferenceRun
  | CommentRangeStart
  | CommentRangeEnd
  | CommentReference
  | InsertedTextRun
  | DeletedTextRun
  | SimpleField;

export declare class TextRun { constructor(options: string | RunOptions); }
export declare class Tab { constructor(); }
export declare class PageBreak { constructor(); }
export declare class ColumnBreak { constructor(); }
export declare class ImageRun {
  constructor(options: {
    type?: "png" | "jpg" | "gif" | "bmp";
    data: Bytes | string;
    transformation?: { width: number; height: number };
    altText?: { name?: string; title?: string; description?: string };
  });
}
export declare class ExternalHyperlink { constructor(options: { link: string; children: ParagraphChild[] }); }
export declare class InternalHyperlink { constructor(options: { anchor: string; children: ParagraphChild[] }); }
export declare class Bookmark { constructor(options: { id: string; children: ParagraphChild[] }); }
export declare class FootnoteReferenceRun { constructor(id: number | string); }
export declare class EndnoteReferenceRun { constructor(id: number | string); }
export declare class CommentRangeStart { constructor(id: number); }
export declare class CommentRangeEnd { constructor(id: number); }
export declare class CommentReference { constructor(id: number); }
export declare class InsertedTextRun { constructor(options: RunOptions & { id?: number; author?: string; date?: string }); }
export declare class DeletedTextRun { constructor(options: RunOptions & { id?: number; author?: string; date?: string }); }
export declare class SimpleField { constructor(instruction: string, cachedValue?: string); }
export declare class Paragraph { constructor(options?: string | ParagraphOptions); }
export interface WidthOptions { size: number | string; type?: string }
export declare class TableCell {
  constructor(options: {
    children: (Paragraph | Table)[];
    columnSpan?: number;
    rowSpan?: number;
    width?: WidthOptions;
    shading?: ShadingOptions;
    verticalAlign?: string;
  });
}
export declare class TableRow {
  constructor(options: { children: TableCell[]; tableHeader?: boolean; cantSplit?: boolean; height?: { value: number; rule?: string } });
}
export declare class Table {
  constructor(options: { rows: TableRow[]; width?: WidthOptions; columnWidths?: number[]; style?: string; alignment?: string; layout?: "fixed" | "autofit" });
}
export declare class TableOfContents { constructor(title?: string, options?: { hyperlink?: boolean; headingStyleRange?: string }); }
export declare class Header { constructor(options?: { children: (Paragraph | Table)[] }); }
export declare class Footer extends Header {}

export interface StyleDefinition {
  id: string;
  name?: string;
  basedOn?: string;
  next?: string;
  quickFormat?: boolean;
  run?: Omit<RunOptions, "text" | "children" | "break" | "style">;
  paragraph?: Omit<ParagraphOptions, "text" | "children" | "heading" | "style" | "bullet" | "numbering" | "bookmark">;
}
export interface SectionOptions {
  properties?: {
    page?: {
      size?: { width?: number; height?: number; orientation?: "portrait" | "landscape" };
      margin?: { top?: number; right?: number; bottom?: number; left?: number; header?: number; footer?: number; gutter?: number };
      pageNumbers?: { start?: number; formatType?: string };
    };
    titlePage?: boolean;
    type?: string;
    column?: { count?: number; space?: number; separate?: boolean };
  };
  headers?: { default?: Header; first?: Header; even?: Header };
  footers?: { default?: Footer; first?: Footer; even?: Footer };
  children: (Paragraph | Table | TableOfContents)[];
}
export interface DocumentOptions {
  sections: SectionOptions[];
  creator?: string;
  title?: string;
  subject?: string;
  description?: string;
  keywords?: string;
  lastModifiedBy?: string;
  styles?: { default?: { document?: { run?: StyleDefinition["run"]; paragraph?: StyleDefinition["paragraph"] } }; paragraphStyles?: StyleDefinition[]; characterStyles?: StyleDefinition[] };
  numbering?: { config: { reference: string; levels: { level: number; format?: string; text?: string; alignment?: string; start?: number; style?: { run?: StyleDefinition["run"]; paragraph?: StyleDefinition["paragraph"] } }[] }[] };
  comments?: { children: { id: number; author?: string; initials?: string; date?: Date | string; children: Paragraph[] }[] };
  footnotes?: Record<number | string, { children: Paragraph[] }>;
  endnotes?: Record<number | string, { children: Paragraph[] }>;
  features?: { trackRevisions?: boolean; updateFields?: boolean };
}
export declare class Document {
  constructor(options: DocumentOptions);
  toSpec(): { spec: object; media: Uint8Array[] };
}

export declare const Packer: {
  toBuffer(doc: Document): Promise<Uint8Array>;
  toUint8Array(doc: Document): Promise<Uint8Array>;
  toBlob(doc: Document): Promise<Blob>;
  toBase64String(doc: Document): Promise<string>;
  /** Not in docx-js: a real-text PDF of the laid-out document. */
  toPdf(doc: Document): Promise<Uint8Array>;
  /** Not in docx-js: one page as PNG. */
  toPng(doc: Document, options?: { page?: number; dpi?: number }): Promise<Uint8Array>;
  /** Not in docx-js: the built document open in the engine. The caller frees it. */
  open(doc: Document): Promise<EngineDocument>;
};
