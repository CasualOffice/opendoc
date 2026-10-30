// SPDX-License-Identifier: Apache-2.0
export * from "./index.js";
import type { Document, OpenOptions, SaveMode, SaveReport, FormatName } from "./index.js";

/** Where the engine module is on disk. */
export declare const WASM_PATH: string;
/** Opens a file (the engine is loaded when this module is imported). */
export declare function openFile(path: string | URL, options?: OpenOptions): Promise<Document>;
/** Saves to a file; the format comes from the extension unless given. */
export declare function saveFile(doc: Document, path: string | URL, options?: { format?: FormatName; mode?: SaveMode }): Promise<SaveReport>;
