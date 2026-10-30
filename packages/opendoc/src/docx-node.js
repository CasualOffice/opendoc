// SPDX-License-Identifier: Apache-2.0
//
// `@casualoffice/opendoc/docx` under Node: loads the engine from disk first, so
// `Packer` works without an `init()` call, exactly as docx-js needs none.

import "./node.js";

export * from "./docx.js";
