// SPDX-License-Identifier: Apache-2.0
// Types for @casualoffice/opendoc-viewer/node.

export * from "./index.js";
import type { FontProvider, OpenOptions, ViewerDocument } from "./index.js";

export declare const WASM_PATH: string;
export declare const FONTS_DIR: string;
export declare function initNode(): Promise<unknown>;
export declare function nodeFontProvider(dir?: string): FontProvider;
export declare function openDocument(bytes: BufferSource, options?: OpenOptions): Promise<ViewerDocument>;
