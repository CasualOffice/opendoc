// docxtemplater's tags, filled by the engine, saved as DOCX and PDF.
//
//   node examples/template.mjs out-dir
import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";

import { Template } from "@casualoffice/opendoc";
import { Document, Packer, Paragraph, Table, TableCell, TableRow } from "@casualoffice/opendoc/docx";

const out = process.argv[2] ?? "out";
await mkdir(out, { recursive: true });

// The template: usually a .docx a person wrote in Word. Built here so the
// example has no input file.
const cell = (text) => new TableCell({ children: [new Paragraph(text)] });
const template = await Packer.toBuffer(
  new Document({
    sections: [
      {
        children: [
          new Paragraph("Invoice {number} for {customer.name}"),
          new Paragraph("{#vip}Thank you for being a VIP customer.{/vip}"),
          new Table({
            rows: [
              new TableRow({ children: [cell("Item"), cell("Qty"), cell("Price")] }),
              new TableRow({ children: [cell("{#lines}{item}"), cell("{qty}"), cell("{price}{/lines}")] }),
            ],
          }),
          new Paragraph("{^paid}Payment is due in 30 days.{/paid}"),
        ],
      },
    ],
  }),
);

const doc = new Template(template);
console.log("tags:", doc.getTags().map((t) => `${t.kind}:${t.name}`).join(", "));
doc.render({
  number: "2026-0042",
  customer: { name: "Contoso" },
  vip: true,
  paid: false,
  lines: [
    { item: "Pens", qty: 10, price: "4.00" },
    { item: "Paper", qty: 2, price: "9.50" },
  ],
});
await writeFile(join(out, "invoice.docx"), doc.toBuffer());
await writeFile(join(out, "invoice.pdf"), doc.toPdf());
console.log(doc.document.text);
