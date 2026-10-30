// The headless SDK package, driven the way a host drives it: the built
// WebAssembly module through the JS API, in Node. Needs `./build.sh` first. A
// missing build fails loudly: "nothing to test" must never read as a pass.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  Document,
  OpenDocError,
  Template,
  convert,
  convertToHtml,
  convertToMarkdown,
  extractRawText,
  formats,
  version,
} from "../src/node.js";

const sample = await readFile(new URL("../../../sample.docx", import.meta.url));

test("open reads the body, sections, styles and the import report", () => {
  const doc = Document.open(sample);
  assert.equal(doc.sourceFormat, "docx");
  assert.ok(doc.paragraphs.length > 0);
  assert.ok(doc.paragraphs.every((p) => typeof p.id === "string" && Array.isArray(p.runs)));
  assert.ok(doc.sections[0].pageWidthPt > 0);
  assert.ok(doc.styles.some((s) => s.kind === "paragraph"));
  const report = doc.importReport;
  assert.equal(report.schema, 1);
  assert.equal(report.format, "docx");
  assert.ok(Array.isArray(report.findings));
  assert.equal(doc.isModified, false);
  doc.free();
  assert.throws(() => doc.text, /freed/);
});

test("python-docx-style building, then a round trip through DOCX", () => {
  const doc = Document.create();
  doc.addHeading("Document Title", 0);
  const p = doc.addParagraph("A plain paragraph having some ");
  doc.addRun(p, "bold", { bold: true });
  doc.addRun(p, " and italic.", { italic: true });
  const t = doc.addTable(2, 2, { cells: [["a", "b"], ["c", "d"]] });
  doc.setCellText(t, 1, 1, "D");
  doc.addRow(t, ["e", "f"]);
  const { bytes, report } = doc.save("docx");
  assert.deepEqual(report.losses, []);
  const back = Document.open(bytes);
  const texts = back.paragraphs.map((x) => x.text);
  assert.ok(texts.includes("Document Title"));
  assert.ok(texts.includes("A plain paragraph having some bold and italic."));
  const mixed = back.paragraphs.find((x) => x.text.startsWith("A plain"));
  assert.deepEqual(
    mixed.runs.filter((r) => r.bold).map((r) => r.text),
    ["bold"],
  );
  const table = back.tables[0];
  assert.deepEqual(
    table.rows.map((r) => r.cells.map((c) => c.blocks.map((b) => b.text).join(""))),
    [
      ["a", "b"],
      ["c", "D"],
      ["e", "f"],
    ],
  );
});

test("an edit batch is atomic and its undo restores the text", () => {
  const doc = Document.open(sample);
  const before = doc.text;
  assert.throws(
    () =>
      doc.edit([
        { op: "replaceAll", find: "the", replace: "THE" },
        { op: "insertText", paragraph: "nope", offset: 0, text: "x" },
      ]),
    (error) => error instanceof OpenDocError && error.code === "invalid-argument" && /edit 1/.test(error.message),
  );
  assert.equal(doc.text, before);
  const { results, undo } = doc.edit([{ op: "replaceAll", find: "the", replace: "THE" }]);
  assert.ok(results[0] > 0);
  assert.notEqual(doc.text, before);
  doc.undo(undo);
  assert.equal(doc.text, before);
});

test("comments, suggestions and accept-all go through the engine", () => {
  const doc = Document.create();
  doc.author = "Grace";
  const p = doc.addParagraph("Hello world");
  doc.setRunFormat(p, 6, 11, { bold: true, color: "FF0000" });
  doc.addComment(p, 0, 5, "Greeting?");
  doc.suggestInsert(p, 11, "!");
  const [comment] = doc.comments;
  assert.equal(comment.text, "Greeting?");
  assert.equal(comment.author, "Grace");
  assert.deepEqual([comment.anchor.start, comment.anchor.end], [0, 5]);
  assert.ok(doc.revisions.some((r) => r.kind === "insertion" && r.text === "!" && r.author === "Grace"));
  assert.equal(doc.acceptAllRevisions(), true);
  assert.deepEqual(doc.revisions, []);
  assert.match(doc.text, /Hello world!/);
});

test("every saved format writes, and the mammoth-shaped helpers return { value, messages }", () => {
  const names = formats()
    .filter((f) => f.save)
    .map((f) => f.name);
  assert.ok(names.includes("pdf") && names.includes("odt") && names.includes("md"));
  const doc = Document.open(sample);
  for (const name of names) {
    const { bytes, report } = doc.save(name, { mode: "semantic" });
    assert.ok(bytes.length > 0, name);
    assert.equal(report.format, name);
  }
  const first = doc.paragraphs.find((p) => p.text).text;
  const html = convertToHtml({ buffer: sample });
  assert.match(html.value, /<html|<p|<h/i);
  assert.ok(Array.isArray(html.messages));
  assert.ok(extractRawText(sample).value.includes(first));
  assert.ok(convertToMarkdown(sample).value.length > 0);
  const odt = convert(sample, "odt");
  const back = Document.open(odt);
  assert.equal(back.sourceFormat, "odt");
  assert.ok(back.text.includes(first));
  assert.match(version(), /^\d+\.\d+\.\d+/);
});

test("an exact save returns the original bytes; after an edit it is refused", () => {
  const doc = Document.open(sample);
  const { bytes } = doc.save("docx", { mode: "exact" });
  assert.deepEqual(Buffer.from(bytes), sample);
  doc.addParagraph("x");
  assert.throws(() => doc.save("docx", { mode: "exact" }), (e) => e.code === "save");
});

test("pages render to RGBA and PNG deterministically, and the PDF is real", () => {
  const a = Document.open(sample);
  const b = Document.open(sample);
  assert.ok(a.pageCount > 1);
  assert.equal(a.pageSizes().length, a.pageCount);
  const { width, height, rgba } = a.renderPage(0, { dpi: 24 });
  assert.equal(rgba.length, width * height * 4);
  assert.ok(rgba.some((v, i) => i % 4 !== 3 && v !== 255), "page 0 rendered blank");
  const png = a.renderPagePng(0, { dpi: 24 });
  assert.deepEqual([...png.subarray(0, 4)], [0x89, 0x50, 0x4e, 0x47]);
  assert.deepEqual(png, b.renderPagePng(0, { dpi: 24 }));
  const pdf = a.toPdf();
  assert.equal(new TextDecoder().decode(pdf.subarray(0, 5)), "%PDF-");
  assert.ok(a.pageText(0).length > 0);
  assert.throws(() => a.renderPage(a.pageCount), (e) => e instanceof OpenDocError);
});

test("docxtemplater-style templates fill, and errors carry docxtemplater's shape", async () => {
  const { Document: Spec, Packer, Paragraph } = await import("../src/docx-node.js");
  const bytes = await Packer.toBuffer(
    new Spec({
      sections: [
        {
          children: [
            new Paragraph("Dear {name},"),
            new Paragraph("{#items}"),
            new Paragraph("- {title}"),
            new Paragraph("{/items}"),
            new Paragraph("{^vip}Regular customer{/vip}"),
          ],
        },
      ],
    }),
  );
  const template = new Template(bytes);
  assert.deepEqual(
    template.getTags().map((t) => t.name),
    ["name", "items", "title", "vip"],
  );
  template.render({ name: "Ada", items: [{ title: "Pen" }, { title: "Ink" }], vip: false });
  const filled = Document.open(template.toBuffer());
  assert.deepEqual(
    filled.paragraphs.map((p) => p.text),
    ["Dear Ada,", "- Pen", "- Ink", "Regular customer"],
  );
  const broken = new Template(
    await Packer.toBuffer(new Spec({ sections: [{ children: [new Paragraph("Hi {name"), new Paragraph("{#loop}")] }] })),
  );
  assert.throws(
    () => broken.render({}),
    (error) =>
      error.name === "TemplateError" &&
      error.properties.id === "multi_error" &&
      error.properties.errors.map((e) => e.properties.id).join() === "unclosed_tag,unclosed_loop",
  );
});
