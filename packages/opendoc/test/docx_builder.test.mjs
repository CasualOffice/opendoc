// The docx-js-familiar builder: a docx-js program, with only its import
// changed, builds a document the engine then reads back construct by construct.
import assert from "node:assert/strict";
import test from "node:test";

import {
  AlignmentType,
  CommentRangeEnd,
  CommentRangeStart,
  CommentReference,
  Document,
  ExternalHyperlink,
  Footer,
  FootnoteReferenceRun,
  Header,
  HeadingLevel,
  ImageRun,
  InsertedTextRun,
  LevelFormat,
  PageBreak,
  PageNumber,
  PageOrientation,
  Packer,
  Paragraph,
  Table,
  TableCell,
  TableOfContents,
  TableRow,
  TextRun,
  WidthType,
} from "../src/docx-node.js";
import { Document as Engine } from "../src/node.js";

// The README example of docx-js, verbatim apart from the import.
test("docx-js's README example builds and reads back", async () => {
  const doc = new Document({
    sections: [
      {
        properties: {},
        children: [
          new Paragraph({
            children: [
              new TextRun("Hello World"),
              new TextRun({ text: "Foo Bar", bold: true }),
              new TextRun({ text: "\tGithub is the best", bold: true }),
            ],
          }),
        ],
      },
    ],
  });
  const buffer = await Packer.toBuffer(doc);
  assert.ok(Buffer.isBuffer(buffer));
  const back = Engine.open(buffer);
  const [p] = back.paragraphs;
  assert.equal(p.text, "Hello WorldFoo Bar\tGithub is the best");
  assert.deepEqual(
    p.runs.filter((r) => r.bold).map((r) => r.text),
    ["Foo Bar", "\t", "Github is the best"].filter((t) => t !== "\t"),
  );
});

test("every construct the builder offers reaches the engine's model", async () => {
  // A real PNG: a page the engine rendered.
  const png = Engine.create().renderPagePng(0, { dpi: 8 });
  const doc = new Document({
    creator: "Ada",
    title: "Builder",
    numbering: {
      config: [
        {
          reference: "steps",
          levels: [{ level: 0, format: LevelFormat.DECIMAL, text: "%1.", alignment: AlignmentType.START }],
        },
      ],
    },
    comments: { children: [{ id: 1, author: "Rev", date: new Date(Date.UTC(2026, 0, 2)), children: [new Paragraph("Check")] }] },
    footnotes: { 1: { children: [new Paragraph("A footnote.")] } },
    sections: [
      {
        properties: { page: { size: { orientation: PageOrientation.LANDSCAPE } } },
        headers: { default: new Header({ children: [new Paragraph("Header text")] }) },
        footers: {
          default: new Footer({
            children: [new Paragraph({ alignment: AlignmentType.CENTER, children: [new TextRun({ children: ["Page ", PageNumber.CURRENT] })] })],
          }),
        },
        children: [
          new TableOfContents("Contents", { hyperlink: true, headingStyleRange: "1-2" }),
          new Paragraph({ text: "Intro", heading: HeadingLevel.HEADING_1 }),
          new Paragraph({
            children: [
              new TextRun({ text: "red", color: "FF0000", size: 28, italics: true, font: "Arial" }),
              new FootnoteReferenceRun(1),
              new CommentRangeStart(1),
              new TextRun(" noted"),
              new CommentRangeEnd(1),
              new CommentReference(1),
              new ExternalHyperlink({ link: "https://example.com", children: [new TextRun(" link")] }),
              new InsertedTextRun({ text: " added", id: 7, author: "Bob", date: "2026-01-01T00:00:00Z" }),
              new PageBreak(),
            ],
          }),
          new Paragraph({ text: "One", numbering: { reference: "steps", level: 0 } }),
          new Paragraph({ text: "Bullet", bullet: { level: 0 } }),
          new Paragraph({ children: [new ImageRun({ type: "png", data: png, transformation: { width: 96, height: 48 } })] }),
          new Table({
            width: { size: 100, type: WidthType.PERCENTAGE },
            rows: [
              new TableRow({
                tableHeader: true,
                children: [new TableCell({ rowSpan: 2, children: [new Paragraph("A")] }), new TableCell({ children: [new Paragraph("B")] })],
              }),
              new TableRow({ children: [new TableCell({ children: [new Paragraph("C")] })] }),
            ],
          }),
        ],
      },
    ],
  });
  const engine = await Packer.open(doc);
  assert.equal(engine.properties.creator, "Ada");
  assert.equal(engine.sections[0].orientation, "landscape");
  const ps = engine.paragraphs;
  const intro = ps.find((p) => p.text === "Intro" && p.style === "heading 1");
  assert.equal(intro.headingLevel, 1);
  const toc = ps.find((p) => p.style === "toc 1");
  assert.ok(toc && toc.text === "Intro", "the table of contents lists the heading");
  const rich = ps.find((p) => p.text.startsWith("red"));
  const red = rich.runs.find((r) => r.text === "red");
  assert.deepEqual([red.color, red.sizePt, red.italic, red.font], ["FF0000", 14, true, "Arial"]);
  assert.ok(rich.runs.some((r) => r.hyperlink === "https://example.com"));
  assert.ok(rich.runs.some((r) => r.kind === "pageBreak"));
  assert.ok(rich.runs.some((r) => r.revision?.kind === "insertion" && r.revision.author === "Bob"));
  assert.ok(ps.find((p) => p.text === "One").list);
  assert.ok(ps.find((p) => p.text === "Bullet").list);
  const image = ps.flatMap((p) => p.runs).find((r) => r.kind === "image").image;
  assert.deepEqual([image.widthPt, image.heightPt], [72, 36]);
  assert.deepEqual(engine.mediaBytes(image.part), png);
  assert.equal(engine.comments[0].text, "Check");
  assert.equal(engine.comments[0].date, "2026-01-02T00:00:00Z");
  assert.ok(engine.notes.some((n) => n.text.includes("A footnote.")));
  assert.ok(engine.headersFooters.some((s) => s.part === "header" && s.text === "Header text"));
  const table = engine.tables[0];
  assert.equal(table.rows[0].cells[0].verticalMerge, "restart");
  assert.equal(table.rows[1].cells[0].verticalMerge, "continue");
  assert.ok(engine.pageCount >= 2, "the page break starts a second page");
  engine.free();
  const pdf = await Packer.toPdf(doc);
  assert.equal(new TextDecoder().decode(pdf.subarray(0, 5)), "%PDF-");
});

test("the same builder input writes the same bytes", async () => {
  const make = () => new Document({ sections: [{ children: [new Paragraph("same")] }] });
  assert.deepEqual(await Packer.toUint8Array(make()), await Packer.toUint8Array(make()));
  assert.equal(typeof (await Packer.toBase64String(make())), "string");
});

test("an invalid value is named, not skipped", async () => {
  await assert.rejects(
    Packer.toBuffer(new Document({ sections: [{ children: [new Paragraph({ text: "x", style: "NoSuchStyle" })] }] })),
    (error) => error.code === "build" && /NoSuchStyle/.test(error.message),
  );
  await assert.rejects(
    Packer.toBuffer(new Document({ sections: [{ children: [new Paragraph({ children: [new TextRun({ text: "x", color: "red" })] })] }] })),
    (error) => error.code === "build" && /red/.test(error.message),
  );
});
