// The blank document the editor makes when you ask for a new one.
//
// Extracted from `main.js` (`109` HF-085): it is a self-contained builder with
// one input and one output, and the module it lived in is 93% of the webapp.
// Nothing here touches the DOM or the engine, which is what lets the package it
// produces be checked byte for byte rather than by opening it and looking.

// ---- New blank document (docs/105 UX-011, OO-002) ---------------------------
//
// Until now the editor could only ever edit a file that already existed: there
// was no `file.new` on any surface, so the most basic thing a word processor
// does — start a document — was the one thing this one could not do.
//
// The engine exposes `open(bytes)` and nothing that manufactures a document, and
// the plain-text importer would give us a body with no styles at all (see
// `document_from_text`: `Definitions::default()`), so a "blank document" made
// that way would have no Normal, no headings, and no section geometry. So the
// host hands the engine what it hands every other document: a real, minimal
// DOCX package. It is built here rather than shipped as a binary asset so the
// blank document is readable, reviewable text in this file, needs no fetch (and
// therefore works with the network off, which is the whole local-first point),
// and cannot drift away from what the importer expects without this code
// changing.
//
// Deliberately NOT included: `w:rFonts`. Naming Calibri would make every new
// document start by asking for a face the engine has to substitute and report;
// omitting it lets the engine use the default it is certain to have.
export const UNTITLED_DOCUMENT_NAME = "Untitled document.docx";

const BLANK_DOCX_XMLNS = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"';

/** Page geometry of every document this module writes: US Letter at 1in
 *  margins, Word's en-US default. In points, the unit the OOXML below wants in
 *  twentieths, so the thumbnail and the package cannot describe two pages. */
export const BLANK_PAGE = Object.freeze({ widthPt: 612, heightPt: 792, marginPt: 72 });

/** Every paragraph style the package defines, in points. `styles.xml` is
 *  generated from this table and so is the template thumbnail, so a change to a
 *  heading size moves both together instead of leaving the preview lying. */
export const BLANK_STYLE_METRICS = Object.freeze({
  Normal: Object.freeze({ sizePt: 11, beforePt: 0, afterPt: 8, bold: false, italic: false }),
  Title: Object.freeze({ sizePt: 28, beforePt: 0, afterPt: 4, bold: false, italic: false }),
  Subtitle: Object.freeze({ sizePt: 14, beforePt: 0, afterPt: 8, bold: false, italic: true }),
  Heading1: Object.freeze({ sizePt: 16, beforePt: 18, afterPt: 4, bold: true, italic: false }),
  Heading2: Object.freeze({ sizePt: 13, beforePt: 16, afterPt: 4, bold: true, italic: false }),
  Heading3: Object.freeze({ sizePt: 12, beforePt: 14, afterPt: 4, bold: true, italic: false }),
});

/** Points to half-points, the unit `w:sz` carries. */
const halfPoints = (pt) => Math.round(pt * 2);
/** Points to twentieths of a point, the unit `w:spacing` and `w:pgSz` carry. */
const twips = (pt) => Math.round(pt * 20);
/** `w:sz` and `w:szCs`, the pair Word wants for one run size. */
const runSize = (pt) => `<w:sz w:val="${halfPoints(pt)}"/><w:szCs w:val="${halfPoints(pt)}"/>`;

/** One heading style, neutral: size and weight only. A blank document should
 *  not arrive carrying somebody's brand colours. */
function blankHeadingStyle(id, name, level) {
  const metrics = BLANK_STYLE_METRICS[id];
  return (
    `<w:style w:type="paragraph" w:styleId="${id}"><w:name w:val="${name}"/>` +
    `<w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/>` +
    `<w:pPr><w:keepNext/><w:keepLines/>` +
    `<w:spacing w:before="${twips(metrics.beforePt)}" w:after="${twips(metrics.afterPt)}"/>` +
    `<w:outlineLvl w:val="${level}"/></w:pPr>` +
    `<w:rPr><w:b/>${runSize(metrics.sizePt)}</w:rPr></w:style>`
  );
}

/** The parts of the blank package, in the order they are written. Letter at 1in
 *  margins is Word's en-US default; the user can change it in Page setup, and
 *  the choice is recorded here rather than hidden in a binary. */
export const BLANK_DOCX_PARTS = [
  [
    "[Content_Types].xml",
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
      '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">' +
      '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>' +
      '<Default Extension="xml" ContentType="application/xml"/>' +
      '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>' +
      '<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>' +
      "</Types>",
  ],
  [
    "_rels/.rels",
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
      '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">' +
      '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>' +
      "</Relationships>",
  ],
  [
    "word/_rels/document.xml.rels",
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
      '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">' +
      '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>' +
      "</Relationships>",
  ],
  [
    "word/document.xml",
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
      `<w:document ${BLANK_DOCX_XMLNS}><w:body><w:p/>` +
      `<w:sectPr><w:pgSz w:w="${twips(BLANK_PAGE.widthPt)}" w:h="${twips(BLANK_PAGE.heightPt)}"/>` +
      `<w:pgMar w:top="${twips(BLANK_PAGE.marginPt)}" w:right="${twips(BLANK_PAGE.marginPt)}" ` +
      `w:bottom="${twips(BLANK_PAGE.marginPt)}" w:left="${twips(BLANK_PAGE.marginPt)}" ` +
      'w:header="720" w:footer="720" w:gutter="0"/>' +
      '<w:cols w:space="720"/></w:sectPr></w:body></w:document>',
  ],
  [
    "word/styles.xml",
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' +
      `<w:styles ${BLANK_DOCX_XMLNS}>` +
      `<w:docDefaults><w:rPrDefault><w:rPr>${runSize(BLANK_STYLE_METRICS.Normal.sizePt)}` +
      "</w:rPr></w:rPrDefault>" +
      `<w:pPrDefault><w:pPr><w:spacing w:after="${twips(BLANK_STYLE_METRICS.Normal.afterPt)}" ` +
      'w:line="259" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults>' +
      '<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style>' +
      '<w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/>' +
      `<w:pPr><w:spacing w:after="${twips(BLANK_STYLE_METRICS.Title.afterPt)}"/><w:contextualSpacing/></w:pPr>` +
      `<w:rPr>${runSize(BLANK_STYLE_METRICS.Title.sizePt)}</w:rPr></w:style>` +
      '<w:style w:type="paragraph" w:styleId="Subtitle"><w:name w:val="Subtitle"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/>' +
      `<w:pPr><w:spacing w:after="${twips(BLANK_STYLE_METRICS.Subtitle.afterPt)}"/></w:pPr>` +
      `<w:rPr><w:i/>${runSize(BLANK_STYLE_METRICS.Subtitle.sizePt)}</w:rPr></w:style>` +
      blankHeadingStyle("Heading1", "heading 1", 0) +
      blankHeadingStyle("Heading2", "heading 2", 1) +
      blankHeadingStyle("Heading3", "heading 3", 2) +
      "</w:styles>",
  ],
];

/** CRC-32 over `bytes`, the one checksum a ZIP local header needs. Table built
 *  once, lazily — a blank document is not created on the hot path. */
let crcTable = null;
function crc32(bytes) {
  if (!crcTable) {
    crcTable = new Uint32Array(256);
    for (let n = 0; n < 256; n += 1) {
      let c = n;
      for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
      crcTable[n] = c >>> 0;
    }
  }
  let crc = 0xffffffff;
  for (let i = 0; i < bytes.length; i += 1) crc = crcTable[(crc ^ bytes[i]) & 0xff] ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}

/** Writes `parts` ([name, utf-8 text]) as a STORED (uncompressed) ZIP.
 *
 *  Stored, not deflated, because the package is ~3 KB of XML that is read once
 *  and thrown away; adding a compressor to the editor to save two kilobytes
 *  would be the wrong trade. Timestamps are fixed at zero so the same blank
 *  document produces the same bytes every time — determinism is a property this
 *  repo holds everywhere else and there is no reason to break it here. */
export function zipStore(parts) {
  const encoder = new TextEncoder();
  const chunks = [];
  const central = [];
  let offset = 0;
  const u16 = (v) => [v & 0xff, (v >>> 8) & 0xff];
  const u32 = (v) => [v & 0xff, (v >>> 8) & 0xff, (v >>> 16) & 0xff, (v >>> 24) & 0xff];

  for (const [name, text] of parts) {
    const nameBytes = encoder.encode(name);
    const data = encoder.encode(text);
    const crc = crc32(data);
    const header = [
      ...u32(0x04034b50), ...u16(20), ...u16(0), ...u16(0), // signature, version, flags, STORED
      ...u16(0), ...u16(0), // dos time, dos date — fixed for determinism
      ...u32(crc), ...u32(data.length), ...u32(data.length),
      ...u16(nameBytes.length), ...u16(0),
    ];
    chunks.push(Uint8Array.from(header), nameBytes, data);
    central.push({ nameBytes, crc, size: data.length, offset });
    offset += header.length + nameBytes.length + data.length;
  }

  const directoryStart = offset;
  for (const entry of central) {
    const record = [
      ...u32(0x02014b50), ...u16(20), ...u16(20), ...u16(0), ...u16(0),
      ...u16(0), ...u16(0),
      ...u32(entry.crc), ...u32(entry.size), ...u32(entry.size),
      ...u16(entry.nameBytes.length), ...u16(0), ...u16(0),
      ...u16(0), ...u16(0), ...u32(0),
      ...u32(entry.offset),
    ];
    chunks.push(Uint8Array.from(record), entry.nameBytes);
    offset += record.length + entry.nameBytes.length;
  }
  chunks.push(
    Uint8Array.from([
      ...u32(0x06054b50), ...u16(0), ...u16(0),
      ...u16(central.length), ...u16(central.length),
      ...u32(offset - directoryStart), ...u32(directoryStart), ...u16(0),
    ]),
  );

  const total = chunks.reduce((sum, chunk) => sum + chunk.length, 0);
  const out = new Uint8Array(total);
  let at = 0;
  for (const chunk of chunks) {
    out.set(chunk, at);
    at += chunk.length;
  }
  return out;
}


/** XML-escapes text going into a `w:t`. Template text is authored here, not
 *  typed by a user, but a stray `&` in a placeholder would still produce a
 *  package Word refuses to open. */
function xmlText(value) {
  return String(value)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

/** One paragraph: `[styleId, text]`, or `[styleId]` for an empty one. */
function templateParagraph([style, text]) {
  const properties = style ? `<w:pPr><w:pStyle w:val="${style}"/></w:pPr>` : "";
  const run = text ? `<w:r><w:t xml:space="preserve">${xmlText(text)}</w:t></w:r>` : "";
  return `<w:p>${properties}${run}</w:p>`;
}

/**
 * The starter documents offered by File ▸ New.
 *
 * ONLYOFFICE's Create New pane ships exactly one entry — `Blank document` —
 * and takes any others from the host (`FileMenuPanels.js:1350`: `blank`
 * concatenated with a host-supplied `docs`). So the PANE is theirs and this
 * set is ours, and it is deliberately small: four shapes that this editor can
 * actually render well today — headings, a title, body text and lists — rather
 * than a gallery that would need images, tables and theming to look like
 * anything.
 *
 * Each one is the same minimal package the blank document uses, with content.
 * They are built here rather than shipped as binaries for the same reason the
 * blank one is: readable, reviewable, no fetch, and they cannot drift away
 * from what the importer expects without this file changing.
 */
export const DOCUMENT_TEMPLATES = Object.freeze([
  Object.freeze({
    id: "blank",
    label: "Blank document",
    icon: "draft",
    description: "An empty page.",
    body: [[null, ""]],
  }),
  Object.freeze({
    id: "letter",
    label: "Letter",
    icon: "mail",
    description: "Sender, date, salutation and sign-off.",
    body: [
      [null, "Your name"],
      [null, "Street address"],
      [null, "City, postcode"],
      [null, ""],
      [null, "1 January 2026"],
      [null, ""],
      [null, "Dear …,"],
      [null, ""],
      [null, "Write your letter here."],
      [null, ""],
      [null, "Yours sincerely,"],
      [null, ""],
      [null, "Your name"],
    ],
  }),
  Object.freeze({
    id: "report",
    label: "Report",
    icon: "lab_profile",
    description: "Title, summary and numbered sections.",
    body: [
      ["Title", "Report title"],
      ["Subtitle", "Subtitle or author"],
      ["Heading1", "Summary"],
      [null, "One paragraph on what this report concludes."],
      ["Heading1", "Background"],
      [null, "What prompted it."],
      ["Heading1", "Findings"],
      ["Heading2", "First finding"],
      [null, "Evidence."],
      ["Heading1", "Recommendation"],
      [null, "What should happen next."],
    ],
  }),
  Object.freeze({
    id: "notes",
    label: "Meeting notes",
    icon: "event_note",
    description: "Attendees, discussion and actions.",
    body: [
      ["Title", "Meeting notes"],
      ["Subtitle", "Date · attendees"],
      ["Heading1", "Agenda"],
      [null, "What the meeting is for."],
      ["Heading1", "Discussion"],
      [null, "What was said."],
      ["Heading1", "Actions"],
      [null, "Who does what, by when."],
    ],
  }),
]);

/**
 * The template's first page as something the UI can draw at any scale: the same
 * `[styleId, text]` body {@link templateBytes} writes, resolved against
 * {@link BLANK_STYLE_METRICS}. Every measurement is in points, so a caller
 * scales the page once instead of re-deriving a size per paragraph — and a
 * preview built from this cannot show a document the `.docx` would not open as.
 *
 * @param {string} id one of `DOCUMENT_TEMPLATES`; anything else is the blank.
 * @returns {{widthPt: number, heightPt: number, marginPt: number, blocks: object[]}}
 */
export function templateThumbnail(id) {
  const template =
    DOCUMENT_TEMPLATES.find((candidate) => candidate.id === id) ?? DOCUMENT_TEMPLATES[0];
  const blocks = template.body.map(([style, text]) => {
    const metrics = BLANK_STYLE_METRICS[style] ?? BLANK_STYLE_METRICS.Normal;
    return Object.freeze({ style: style ?? "Normal", text: text ?? "", ...metrics });
  });
  return Object.freeze({ ...BLANK_PAGE, blocks: Object.freeze(blocks) });
}

/**
 * The package bytes for one template.
 *
 * @param {string} id one of `DOCUMENT_TEMPLATES`; anything else is the blank.
 * @returns {Uint8Array} a `.docx` the engine can open.
 */
export function templateBytes(id) {
  const template =
    DOCUMENT_TEMPLATES.find((candidate) => candidate.id === id) ?? DOCUMENT_TEMPLATES[0];
  const body = template.body.map(templateParagraph).join("");
  const parts = BLANK_DOCX_PARTS.map(([name, text]) =>
    name === "word/document.xml" ? [name, text.replace("<w:body><w:p/>", `<w:body>${body}`)] : [name, text],
  );
  return zipStore(parts);
}

/** The file name a new document from `id` opens under. */
export function templateName(id) {
  const template = DOCUMENT_TEMPLATES.find((candidate) => candidate.id === id);
  return template && template.id !== "blank"
    ? `${template.label}.docx`
    : UNTITLED_DOCUMENT_NAME;
}
