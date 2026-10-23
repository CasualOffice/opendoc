#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
//
// The Viewer SDK from Node: open a DOCX, write every page as PNG, the PDF, the
// text and the outline.
//
//   ./packages/opendoc-viewer/build.sh
//   node packages/opendoc-viewer/examples/node/render.mjs sample.docx /tmp/out [dpi]
//
// In an installed package the import is "@casualoffice/opendoc-viewer/node".
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

import { openDocument } from "../../src/node.js";

const [input, outDir, dpiArg] = process.argv.slice(2);
if (!input || !outDir) {
  console.error("usage: render.mjs <in.docx> <out-dir> [dpi]");
  process.exit(2);
}
const dpi = Number(dpiArg ?? 96);

await mkdir(outDir, { recursive: true });
const started = performance.now();
// The package's own faces are served from disk by default (nodeFontProvider).
const doc = await openDocument(await readFile(input));
const opened = performance.now();

for (let i = 0; i < doc.pageCount; i++) {
  await writeFile(join(outDir, `page-${i + 1}.png`), doc.renderPagePng(i, { dpi }));
}
await writeFile(join(outDir, "document.pdf"), doc.toPdf());
await writeFile(join(outDir, "document.txt"), doc.text());
await writeFile(join(outDir, "outline.json"), `${JSON.stringify(doc.outline(), null, 2)}\n`);

const unmet = doc.fontNeeds().filter((need) => !need.substituteAvailable);
console.log(
  `${doc.pageCount} pages, opened in ${Math.round(opened - started)} ms, ` +
    `written in ${Math.round(performance.now() - opened)} ms to ${outDir}` +
    (unmet.length ? `; fonts not supplied: ${unmet.map((need) => need.requested).join(", ")}` : ""),
);
doc.free();
