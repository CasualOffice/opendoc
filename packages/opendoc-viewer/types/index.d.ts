// SPDX-License-Identifier: Apache-2.0
// Types for @casualoffice/opendoc-viewer (docs/176 section 3.1).

/** A font the layout asked for and did not have. */
export interface FontNeed {
  /** The family the document requested (`w:rFonts`), e.g. `"Calibri"`. */
  requested: string;
  /** The metric-compatible family the engine substitutes, e.g. `"Carlito"`. */
  substitute: string | null;
  /** Whether the substitute is available to the layout right now. */
  substituteAvailable: boolean;
}

/** A face, or several (a family's regular/bold/italic/bold-italic). */
export type FontFaces = BufferSource | ReadonlyArray<BufferSource>;

/**
 * The host font hook. Return the faces that answer `need` (the requested family
 * itself, or its substitute), or nothing to keep the substitute in place.
 */
export type FontProvider = (
  need: FontNeed,
) => FontFaces | null | undefined | Promise<FontFaces | null | undefined>;

/** Where the engine module comes from (`init`). */
export type WasmSource = URL | string | Response | BufferSource | WebAssembly.Module;

export interface OpenOptions {
  /** Faces registered before the first layout (one pagination). */
  fonts?: ReadonlyArray<BufferSource>;
  /** Asked once per font need after the first layout; see `FontProvider`. */
  fontProvider?: FontProvider | null;
  /** Where the `.wasm` is, when the default next to the glue is not it. */
  wasm?: WasmSource;
}

export interface PageSize {
  widthTwips: number;
  heightTwips: number;
  widthPt: number;
  heightPt: number;
}

export interface PageBitmap {
  width: number;
  height: number;
  /** Premultiplied RGBA8888, `width * height * 4` bytes. */
  rgba: Uint8ClampedArray;
}

export interface OutlineEntry {
  /** 1-based heading level. */
  level: number;
  text: string;
  /** 0-based page the heading starts on, or null. */
  page: number | null;
}

export interface RenderOptions {
  /** Dots per inch; default 96. */
  dpi?: number;
}

/** An opened, laid-out, read-only document. */
export declare class ViewerDocument {
  private constructor();
  readonly pageCount: number;
  pageSize(index: number): PageSize;
  pageSizes(): PageSize[];
  renderPage(index: number, options?: RenderOptions): PageBitmap;
  renderPagePng(index: number, options?: RenderOptions): Uint8Array;
  pageText(index: number): string;
  text(): string;
  outline(): OutlineEntry[];
  fontNeeds(): FontNeed[];
  missingCodePoints(): number[];
  registerFonts(faces: ReadonlyArray<BufferSource>): number;
  registerFallbackFont(face: BufferSource, scripts: ReadonlyArray<string>): number;
  readonly hasPdf: boolean;
  toPdf(): Uint8Array;
  free(): void;
}

/** The families embedded in the engine module. */
export declare const CORE_FAMILIES: ReadonlyArray<string>;
/** Family name → the face files the package ships under `pkg/fonts/`. */
export declare const PACKAGE_FONTS: Readonly<Record<string, ReadonlyArray<string>>>;

export declare function init(options?: { wasm?: WasmSource }): Promise<unknown>;
export declare function openDocument(bytes: BufferSource, options?: OpenOptions): Promise<ViewerDocument>;
export declare function packageFontProvider(
  baseUrl: URL | string,
  options?: { fetch?: typeof fetch },
): FontProvider;
