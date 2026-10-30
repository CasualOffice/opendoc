// The typed surface: `tsc --noEmit -p examples/typescript` type-checks this
// file against the package's declarations.
import { Document as Engine, type SaveReport } from "@casualoffice/opendoc";
import { Document, HeadingLevel, Packer, Paragraph, TextRun } from "@casualoffice/opendoc/docx";

export async function build(): Promise<{ docx: Uint8Array; pdf: Uint8Array; report: SaveReport }> {
  const doc = new Document({
    sections: [
      {
        children: [
          new Paragraph({ text: "Typed", heading: HeadingLevel.HEADING_1 }),
          new Paragraph({ children: [new TextRun({ text: "bold", bold: true, size: 24 })] }),
        ],
      },
    ],
  });
  const engine: Engine = await Packer.open(doc);
  const id: string = engine.addParagraph("added", { style: "Quote" });
  engine.setRunFormat(id, 0, 5, { italic: true, color: "1F4E79" });
  const { bytes, report } = engine.save("docx", { mode: "preserve" });
  const pdf = engine.toPdf();
  engine.free();
  return { docx: bytes, pdf, report };
}
