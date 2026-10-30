// A docx-js program, unchanged apart from its import, that also renders.
//
//   node examples/build-report.mjs out-dir
import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";

import {
  AlignmentType,
  Document,
  Footer,
  HeadingLevel,
  Packer,
  PageNumber,
  Paragraph,
  Table,
  TableCell,
  TableOfContents,
  TableRow,
  TextRun,
  WidthType,
} from "@casualoffice/opendoc/docx";

const out = process.argv[2] ?? "out";
await mkdir(out, { recursive: true });

const rows = [
  ["Region", "Q1", "Q2"],
  ["North", "1,200", "1,350"],
  ["South", "980", "1,105"],
];

const doc = new Document({
  creator: "Finance",
  title: "Quarterly report",
  features: { updateFields: true },
  sections: [
    {
      footers: {
        default: new Footer({
          children: [
            new Paragraph({
              alignment: AlignmentType.CENTER,
              children: [new TextRun({ children: ["Page ", PageNumber.CURRENT, " of ", PageNumber.TOTAL_PAGES] })],
            }),
          ],
        }),
      },
      children: [
        new Paragraph({ text: "Quarterly report", heading: HeadingLevel.TITLE }),
        new TableOfContents("Contents", { hyperlink: true, headingStyleRange: "1-2" }),
        new Paragraph({ text: "Summary", heading: HeadingLevel.HEADING_1 }),
        new Paragraph({
          children: [
            new TextRun("Revenue grew "),
            new TextRun({ text: "12%", bold: true, color: "2E7D32" }),
            new TextRun(" quarter on quarter."),
          ],
        }),
        new Paragraph({ text: "By region", heading: HeadingLevel.HEADING_1 }),
        new Table({
          width: { size: 100, type: WidthType.PERCENTAGE },
          rows: rows.map(
            (cells, index) =>
              new TableRow({
                tableHeader: index === 0,
                children: cells.map(
                  (text) =>
                    new TableCell({ children: [new Paragraph({ children: [new TextRun({ text, bold: index === 0 })] })] }),
                ),
              }),
          ),
        }),
      ],
    },
  ],
});

await writeFile(join(out, "report.docx"), await Packer.toBuffer(doc));
// What docx-js cannot do: the same document, laid out.
await writeFile(join(out, "report.pdf"), await Packer.toPdf(doc));
await writeFile(join(out, "report-page-1.png"), await Packer.toPng(doc, { page: 0, dpi: 96 }));
console.log(`wrote report.docx, report.pdf and report-page-1.png to ${out}`);
