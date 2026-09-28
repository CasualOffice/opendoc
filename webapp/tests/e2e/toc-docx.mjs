// Builds a minimal DOCX carrying a REAL Word table of contents, in memory.
//
// No shipped sample has one, and a TOC is not something the editor can author
// from nothing yet, so the only honest way to test "does clicking an entry go to
// the heading" is to open a document Word itself would produce:
//
//   - the field is a COMPLEX field range: `fldChar begin` + `instrText TOC \o
//     "1-3" \h \z \u` + `separate`, the entry paragraphs, then `fldChar end`;
//   - each entry is a `w:hyperlink w:anchor="_TocN"` — that is what `\h` means —
//     wrapping the heading text, a tab, and a nested `PAGEREF` field;
//   - each heading carries the matching `w:bookmarkStart w:name="_TocN"`.
//
// Returned as bytes rather than written to the repo so nothing has to be
// regenerated or checked in, and `build.sh --check` has nothing to stale.
import { deflateRawSync } from "node:zlib";

const CRC_TABLE = (() => {
  const table = new Int32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c;
  }
  return table;
})();

function crc32(buf) {
  let c = -1;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ -1) >>> 0;
}

/** A stored-or-deflated ZIP of `{ name: string }` entries, as a Buffer. */
function zip(entries) {
  const locals = [];
  const central = [];
  let offset = 0;
  for (const [name, text] of Object.entries(entries)) {
    const nameBytes = Buffer.from(name, "utf8");
    const raw = Buffer.from(text, "utf8");
    const deflated = deflateRawSync(raw);
    const crc = crc32(raw);
    const local = Buffer.alloc(30 + nameBytes.length);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt16LE(0, 6);
    local.writeUInt16LE(8, 8);
    local.writeUInt32LE(0, 10);
    local.writeUInt32LE(crc, 14);
    local.writeUInt32LE(deflated.length, 18);
    local.writeUInt32LE(raw.length, 22);
    local.writeUInt16LE(nameBytes.length, 26);
    local.writeUInt16LE(0, 28);
    nameBytes.copy(local, 30);
    locals.push(local, deflated);

    const dir = Buffer.alloc(46 + nameBytes.length);
    dir.writeUInt32LE(0x02014b50, 0);
    dir.writeUInt16LE(20, 4);
    dir.writeUInt16LE(20, 6);
    dir.writeUInt16LE(0, 8);
    dir.writeUInt16LE(8, 10);
    dir.writeUInt32LE(0, 12);
    dir.writeUInt32LE(crc, 16);
    dir.writeUInt32LE(deflated.length, 20);
    dir.writeUInt32LE(raw.length, 24);
    dir.writeUInt16LE(nameBytes.length, 28);
    dir.writeUInt32LE(0, 38); // external attributes
    dir.writeUInt32LE(offset, 42); // offset of the local header
    nameBytes.copy(dir, 46);
    central.push(dir);
    offset += local.length + deflated.length;
  }
  const centralBuf = Buffer.concat(central);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(central.length, 8);
  end.writeUInt16LE(central.length, 10);
  end.writeUInt32LE(centralBuf.length, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...locals, centralBuf, end]);
}

// A `\\h` entry: the heading text is wrapped in a real hyperlink to its bookmark.
const entry = (n, text, page, level) => `
  <w:p><w:pPr><w:pStyle w:val="TOC${level}"/></w:pPr>
    <w:hyperlink w:anchor="_Toc${n}" w:history="1">
      <w:r><w:rPr><w:rStyle w:val="Hyperlink"/></w:rPr><w:t xml:space="preserve">${text}</w:t></w:r>
      <w:r><w:tab/></w:r>
      <w:r><w:fldChar w:fldCharType="begin"/></w:r>
      <w:r><w:instrText xml:space="preserve"> PAGEREF _Toc${n} \\h </w:instrText></w:r>
      <w:r><w:fldChar w:fldCharType="separate"/></w:r>
      <w:r><w:t>${page}</w:t></w:r>
      <w:r><w:fldChar w:fldCharType="end"/></w:r>
    </w:hyperlink>
  </w:p>`;

// A TOC written WITHOUT `\\h`: identical text, identical `TOCn` style, but no
// hyperlink at all — which is what Word emits when the field has no `\\h` switch
// and what most pre-2007 and LibreOffice-produced tables of contents look like.
const plainEntry = (n, text, page, level) => `
  <w:p><w:pPr><w:pStyle w:val="TOC${level}"/></w:pPr>
    <w:r><w:t xml:space="preserve">${text}</w:t></w:r>
    <w:r><w:tab/></w:r>
    <w:r><w:fldChar w:fldCharType="begin"/></w:r>
    <w:r><w:instrText xml:space="preserve"> PAGEREF _Toc${n} </w:instrText></w:r>
    <w:r><w:fldChar w:fldCharType="separate"/></w:r>
    <w:r><w:t>${page}</w:t></w:r>
    <w:r><w:fldChar w:fldCharType="end"/></w:r>
  </w:p>`;

const heading = (n, text, level) => `
  <w:p><w:pPr><w:pStyle w:val="Heading${level}"/></w:pPr>
    <w:bookmarkStart w:id="${n}" w:name="_Toc${n}"/>
    <w:r><w:t xml:space="preserve">${text}</w:t></w:r>
    <w:bookmarkEnd w:id="${n}"/>
  </w:p>`;

const filler = (t) => `<w:p><w:r><w:t xml:space="preserve">${t}</w:t></w:r></w:p>`;

/** `TOC 1`..`TOC 3` and `Heading 1`/`Heading 2`, spelled the way Word spells
 *  them: the style NAME is lower-case ("toc 1", "heading 1") while the id is
 *  not, and a heading carries its own `w:outlineLvl`. */
const STYLES = `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  ${[1, 2, 3]
    .map(
      (n) =>
        `<w:style w:type="paragraph" w:styleId="TOC${n}"><w:name w:val="toc ${n}"/><w:basedOn w:val="Normal"/><w:uiPriority w:val="39"/></w:style>`,
    )
    .join("\n  ")}
  ${[1, 2]
    .map(
      (n) =>
        `<w:style w:type="paragraph" w:styleId="Heading${n}"><w:name w:val="heading ${n}"/><w:basedOn w:val="Normal"/><w:pPr><w:outlineLvl w:val="${n - 1}"/></w:pPr><w:rPr><w:b/><w:sz w:val="${32 - n * 4}"/></w:rPr></w:style>`,
    )
    .join("\n  ")}
  <w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
  <w:style w:type="character" w:styleId="Hyperlink"><w:name w:val="Hyperlink"/><w:rPr><w:color w:val="0563C1"/><w:u w:val="single"/></w:rPr></w:style>
</w:styles>`;

/** The three headings this document's TOC points at, in document order. */
export const TOC_HEADINGS = ["Alpha chapter", "Beta chapter", "Gamma section"];

/** A `{ name, mimeType, buffer }` ready for `setInputFiles`. */
export function tocDocx(name = "toc.docx", filler_lines = 30, sdt = false, hyperlinked = true) {
  const body = `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>
  ${sdt ? '<w:sdt><w:sdtPr><w:docPartObj><w:docPartGallery w:val="Table of Contents"/><w:docPartUnique/></w:docPartObj></w:sdtPr><w:sdtContent>' : ''}
  <w:p><w:r><w:t>Contents</w:t></w:r></w:p>
  <w:p>
    <w:r><w:fldChar w:fldCharType="begin"/></w:r>
    <w:r><w:instrText xml:space="preserve">${hyperlinked ? ' TOC \\o "1-3" \\h \\z \\u ' : ' TOC \\o "1-3" \\z \\u '}</w:instrText></w:r>
    <w:r><w:fldChar w:fldCharType="separate"/></w:r>
  </w:p>
  ${(hyperlinked ? entry : plainEntry)(101, TOC_HEADINGS[0], 2, 1)}
  ${(hyperlinked ? entry : plainEntry)(102, TOC_HEADINGS[1], 3, 1)}
  ${(hyperlinked ? entry : plainEntry)(103, TOC_HEADINGS[2], 4, 2)}
  <w:p><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>
  ${sdt ? '</w:sdtContent></w:sdt>' : ''}
  <w:p><w:r><w:br w:type="page"/></w:r></w:p>
  ${heading(101, TOC_HEADINGS[0], 1)}
  ${Array.from({ length: filler_lines }, (_, i) => filler(`Alpha body line ${i + 1}.`)).join("")}
  ${heading(102, TOC_HEADINGS[1], 1)}
  ${Array.from({ length: filler_lines }, (_, i) => filler(`Beta body line ${i + 1}.`)).join("")}
  ${heading(103, TOC_HEADINGS[2], 2)}
  ${Array.from({ length: filler_lines }, (_, i) => filler(`Gamma body line ${i + 1}.`)).join("")}
  <w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>
</w:body>
</w:document>`;

  const buffer = zip({
    "[Content_Types].xml": `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
</Types>`,
    "_rels/.rels": `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>`,
    "word/_rels/document.xml.rels": `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>`,
    // Word's own style definitions for the two families this document uses.
    // Without them the entries carry a `w:pStyle` pointing at nothing and the
    // headings are ordinary paragraphs — which is not what any real table of
    // contents looks like, and would make the fixture prove less than it claims.
    "word/styles.xml": STYLES,
    "word/document.xml": body,
  });

  return {
    name,
    mimeType:
      "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    buffer,
  };
}
