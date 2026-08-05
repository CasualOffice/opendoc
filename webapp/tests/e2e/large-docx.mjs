// A deterministic, dependency-free generator for a large multi-page .docx used
// by the memory-budget spec. It emits a minimal-but-valid OOXML package whose
// body forces an exact number of pages with explicit page breaks, so the page
// count does not depend on font metrics. The archive is a "stored" (uncompressed)
// ZIP written by hand — small on the wire (plain text), never committed.

/** Standard CRC-32 (IEEE 802.3), needed for each ZIP entry. */
const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c >>> 0;
  }
  return table;
})();

function crc32(bytes) {
  let crc = 0xffffffff;
  for (let i = 0; i < bytes.length; i++) crc = CRC_TABLE[(crc ^ bytes[i]) & 0xff] ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}

/** Build a ZIP archive from `{ name, data: Uint8Array }` entries, all stored. */
function storedZip(entries) {
  const enc = new TextEncoder();
  const parts = [];
  const central = [];
  let offset = 0;

  const u16 = (v) => Uint8Array.from([v & 0xff, (v >>> 8) & 0xff]);
  const u32 = (v) => Uint8Array.from([v & 0xff, (v >>> 8) & 0xff, (v >>> 16) & 0xff, (v >>> 24) & 0xff]);

  for (const entry of entries) {
    const nameBytes = enc.encode(entry.name);
    const data = entry.data;
    const crc = crc32(data);
    const local = concat([
      u32(0x04034b50), // local file header signature
      u16(20), // version needed
      u16(0), // flags
      u16(0), // method: stored
      u16(0), u16(0), // mod time/date (fixed → deterministic)
      u32(crc),
      u32(data.length), // compressed size
      u32(data.length), // uncompressed size
      u16(nameBytes.length),
      u16(0), // extra length
      nameBytes,
      data,
    ]);
    parts.push(local);
    central.push(concat([
      u32(0x02014b50), // central directory header signature
      u16(20), u16(20), u16(0), u16(0), u16(0), u16(0),
      u32(crc),
      u32(data.length),
      u32(data.length),
      u16(nameBytes.length),
      u16(0), u16(0), u16(0), u16(0),
      u32(0), // external attrs
      u32(offset), // local header offset
      nameBytes,
    ]));
    offset += local.length;
  }

  const centralBytes = concat(central);
  const eocd = concat([
    u32(0x06054b50),
    u16(0), u16(0),
    u16(entries.length), u16(entries.length),
    u32(centralBytes.length),
    u32(offset),
    u16(0),
  ]);
  return concat([...parts, centralBytes, eocd]);
}

function concat(arrays) {
  const total = arrays.reduce((s, a) => s + a.length, 0);
  const out = new Uint8Array(total);
  let o = 0;
  for (const a of arrays) {
    out.set(a, o);
    o += a.length;
  }
  return out;
}

const CONTENT_TYPES = `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>`;

const ROOT_RELS = `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>`;

const DOC_RELS = `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"></Relationships>`;

function documentXml(pageCount) {
  const W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
  const blocks = [];
  for (let i = 0; i < pageCount; i++) {
    blocks.push(`<w:p><w:r><w:t xml:space="preserve">Page ${i + 1} of ${pageCount} — synthetic memory-budget fixture.</w:t></w:r></w:p>`);
    if (i < pageCount - 1) {
      blocks.push(`<w:p><w:r><w:br w:type="page"/></w:r></w:p>`);
    }
  }
  // Letter portrait, 1in margins.
  const sectPr = `<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>`;
  return `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="${W}"><w:body>${blocks.join("")}${sectPr}</w:body></w:document>`;
}

/** Returns a Buffer/Uint8Array of a valid .docx that paginates to `pageCount`
 *  pages (via explicit page breaks — deterministic regardless of fonts). */
export function makeLargeDocx(pageCount = 49) {
  const enc = new TextEncoder();
  return storedZip([
    { name: "[Content_Types].xml", data: enc.encode(CONTENT_TYPES) },
    { name: "_rels/.rels", data: enc.encode(ROOT_RELS) },
    { name: "word/_rels/document.xml.rels", data: enc.encode(DOC_RELS) },
    { name: "word/document.xml", data: enc.encode(documentXml(pageCount)) },
  ]);
}
