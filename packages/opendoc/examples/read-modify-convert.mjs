// Open any DOCX/ODT/RTF, read it, change it, and save it four ways, with the
// loss report the save returns.
//
//   node examples/read-modify-convert.mjs input.docx out-dir
import { mkdir } from "node:fs/promises";
import { join } from "node:path";

import { openFile, saveFile } from "@casualoffice/opendoc/node";

const [input, out = "out"] = process.argv.slice(2);
if (!input) throw new Error("usage: node read-modify-convert.mjs input.docx [out-dir]");
await mkdir(out, { recursive: true });

const doc = await openFile(input);
console.log(`${doc.sourceFormat}: ${doc.paragraphs.length} paragraphs, ${doc.tables.length} tables, ${doc.pageCount} pages`);
console.log("not carried at open:", doc.importReport.findings.filter((f) => f.loss).map((f) => f.feature));
for (const p of doc.paragraphs.filter((p) => p.headingLevel !== undefined)) {
  console.log(`${"  ".repeat(Math.max(0, p.headingLevel - 1))}${p.text}`);
}

const replaced = doc.replaceAll("Draft", "Final");
const note = doc.addParagraph("Reviewed by the OpenDoc SDK.", { style: "Quote" });
doc.addComment(note, 0, 8, "Automated review");
console.log(`replaced ${replaced}; ${doc.comments.length} comment(s)`);

for (const ext of ["docx", "odt", "pdf", "md"]) {
  const report = await saveFile(doc, join(out, `output.${ext}`));
  console.log(`output.${ext}: ${report.losses.length ? report.losses.map((l) => l.feature).join(", ") : "nothing lost"}`);
}
