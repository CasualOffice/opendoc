# @casualoffice/opendoc

The OpenDoc headless SDK for JavaScript and TypeScript, in Node and the browser.
It creates, reads, modifies, templates, converts and renders word-processing
documents. It has its own page layout, writes real-text PDF and page PNGs, and
reports anything a save would lose. Licence: Apache-2.0.

It has no DOM dependency and no server, and it never runs Word. The engine is a
WebAssembly build of the same Rust engine the OpenDoc editor runs
([`crates/opendoc`](../../crates/opendoc)), so a page rendered here is the page
the editor shows.

**Not published yet.** Publishing is a separate step the owner authorises. Build
and pack it locally:

```sh
./packages/opendoc/build.sh                      # wasm-pack; needs Rust and wasm-pack
cd packages/opendoc && npm test && npm pack
```

## One package, four familiar APIs

| If you use | Write this instead | What you gain |
| --- | --- | --- |
| [docx](https://docx.js.org) (docx-js) | `import { Document, Packer, Paragraph, TextRun } from "@casualoffice/opendoc/docx"`, with the same classes and options | `Packer.toPdf`, `Packer.toPng`, `Packer.open` (read and edit what you built); an unknown style or a bad colour is an error, not a silent skip |
| [docxtemplater](https://docxtemplater.com) | `new Template(bytes).render(data).toBuffer()`, with the same `{x}`, `{#x}…{/x}`, `{^x}` and `{.}` tags | No zip library; pictures, tables, headers and footnotes in the template are kept; `toPdf()` |
| [mammoth](https://github.com/mwilliamson/mammoth.js) | `convertToHtml({ buffer })`, `extractRawText({ buffer })`, giving `{ value, messages }` | Fonts, sizes, colours and borders are kept; the messages are the save's loss report |
| python-docx-style editing | `doc.addParagraph(text, { style })`, `doc.addRun(p, text, { bold: true })`, `doc.addTable(r, c)`, `doc.paragraphs`, `doc.tables` | Atomic batches with an undo; comments and tracked changes; save to any format |

## Quick start (Node)

```js
import { readFile, writeFile } from "node:fs/promises";
import { Document } from "@casualoffice/opendoc"; // in Node this loads the engine itself

const doc = Document.open(await readFile("in.docx"));
console.log(doc.paragraphs.map((p) => `${p.style ?? ""}: ${p.text}`));
console.log("lost at open:", doc.importReport.findings.filter((f) => f.loss));

const p = doc.addHeading("Appendix", 1);
doc.addRun(doc.addParagraph("Signed: "), "Ada", { bold: true });
doc.replaceAll("ACME", "Contoso");

const { bytes, report } = doc.save("docx");          // "preserve" by default
await writeFile("out.docx", bytes);
console.log(report.losses.length ? report.losses : "nothing lost");
await writeFile("out.pdf", doc.toPdf());
await writeFile("page1.png", doc.renderPagePng(0, { dpi: 150 }));
```

In a browser or a worker, call `await init()` first (or `init({ wasm: url })`),
then use the same API.

## Generate (docx-js style)

```js
import { Document, Packer, Paragraph, TextRun, HeadingLevel } from "@casualoffice/opendoc/docx";

const doc = new Document({
  sections: [{
    children: [
      new Paragraph({ text: "Hello", heading: HeadingLevel.HEADING_1 }),
      new Paragraph({ children: [new TextRun("plain "), new TextRun({ text: "bold", bold: true })] }),
    ],
  }],
});
const docx = await Packer.toBuffer(doc);
const pdf = await Packer.toPdf(doc);   // not in docx-js
```

The builder supports these docx-js constructs:

- `Paragraph` (heading, style, alignment, spacing, indent, bullet, numbering, page break
  before, keep options, shading, thematic break) and `TextRun` (bold, italics, underline,
  strike, colour, size, font, highlight, super/subscript, caps, spacing, shading, style,
  breaks, `PageNumber` children);
- `ImageRun`, `ExternalHyperlink`, `InternalHyperlink`, `Bookmark`, `PageBreak`, `ColumnBreak`
  and `Tab`;
- `Table`, `TableRow` and `TableCell`, with column and row spans, widths, header rows, shading
  and vertical alignment;
- headers and footers (default, first, even), page size, orientation, margins, page numbering
  and columns per section;
- `numbering.config`, and `styles` (defaults, paragraph and character styles);
- `comments`, `footnotes` and `endnotes`;
- `InsertedTextRun` and `DeletedTextRun`, `TableOfContents`, and `features.trackRevisions`
  and `updateFields`.

A `TableOfContents` is written with linked entries for the headings in the
spec. Page numbers appear when Word updates the field.

## Templates (docxtemplater style)

```js
import { Template } from "@casualoffice/opendoc";

const t = new Template(await readFile("invoice-template.docx"));
t.render({ customer: { name: "Contoso" }, lines: [{ item: "Pens", qty: 10 }], paid: false });
await writeFile("invoice.docx", t.toBuffer());
await writeFile("invoice.pdf", t.toPdf());
```

- **Tags:**
  - `{name}` and `{a.b.c}`;
  - `{#list}…{/list}` (or `{/}`) loops over an array, sets an object as the scope, and
    otherwise acts as a condition;
  - `{^x}…{/x}` is inverted;
  - `{.}` is the current item.
- **Paragraph and row loops:** when the opening and closing tags stand alone in their own
  paragraphs, the paragraphs between them repeat. When the opening tag starts a row's first
  cell and the closing tag ends a row's last cell, the rows repeat.
- **Line breaks:** a `\n` in a value becomes a line break.
- **Options:** `delimiters`, `nullGetter` (a constant; the default is `""`), and `strict`
  (a missing value is an error).
- **Errors:** every bad tag is reported at once, in docxtemplater's `multi_error` shape
  (`error.properties.errors[i].properties.id`), and the document stays unchanged.

## Formats

<!-- @generated sdk-formats -->
<!-- Generated by `webapp/tools/build-readme.mjs` from the `FormatDescriptor` each `casual-doc-io` adapter declares and `packages/opendoc/build.sh`. Do not edit. -->

| Format | Open | Save | Notes |
| --- | :---: | :---: | --- |
| Office Open XML Document | yes | yes | `exact` saves an unedited file back byte for byte; `preserve` keeps verbatim-retained parts. |
| Word Template | — | yes | — |
| OpenDocument Text | yes | yes | — |
| Rich Text Format | yes | — | Opens; there is no RTF writer. |
| PDF | — | yes | Saved from the engine's own layout, with real, selectable text. Opening a PDF needs the `pdf-import` feature (the `opendoc` CLI); this package does not compile it. |
| Web Page | yes | yes | — |
| Markdown | yes | yes | — |
| Plain Text | yes | yes | — |
| OpenDoc Normalized JSON | yes | yes | — |
<!-- @end sdk-formats -->

## What it does not do yet

- Edits cannot insert pictures, headers, footers or notes. The builder writes all four.
- Edits cannot change page or section setup.
- A paragraph loop inside a header or footer is reported as an error and not run. Value tags
  work there.
- There is no SVG page output.
- HTML and Markdown cannot be opened.
- The module embeds every bundled face (Liberation, Carlito, Caladea, Roboto, Noto Emoji, Noto Sans Symbols 2),
  so pixels do not depend on the machine. That makes the download large: `npm pack` prints
  its size.

## Licence

Apache-2.0. The embedded fonts' licences are in `pkg/LICENSES/` and `NOTICE`.
