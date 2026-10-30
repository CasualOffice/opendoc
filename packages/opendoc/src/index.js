// SPDX-License-Identifier: Apache-2.0
//
// @casualoffice/opendoc: the OpenDoc headless SDK for JavaScript and TypeScript
// (docs/176 section 12). Open, read, modify, template, convert and render
// DOCX, ODT, RTF, TXT and JSON; save DOCX, ODT, HTML, Markdown, TXT, JSON and
// real-text PDF, with a report of anything a round trip would lose.
//
//   import { init, Document } from "@casualoffice/opendoc";       // browsers, workers
//   import { Document } from "@casualoffice/opendoc/node";        // Node (loads itself)
//   import { Document, Packer, Paragraph } from "@casualoffice/opendoc/docx"; // docx-js-style builder

export {
  Document,
  OpenDocError,
  convert,
  convertToHtml,
  convertToMarkdown,
  extractRawText,
  formats,
  init,
  version,
} from "./engine.js";
export { Template } from "./template.js";
