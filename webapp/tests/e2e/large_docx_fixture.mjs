// A large, editable DOCX, generated rather than committed (`docs/180` §2).
//
// The open path has two regimes (`docs/113`): a body at or under the engine's
// whole-layout ceiling is laid out whole and is EDITABLE; past it the body is
// windowed and read-only. Every long-task number that matters to a person
// editing is in the first regime, and no committed fixture is in it at a size
// where the open is felt: `sample.docx` is 75 KB. So the measurement and the
// worker-open guard both build this one in memory: Heading 1 every 25
// paragraphs (so the outline, the style cascade and keep-with-next all have
// work), the rest Normal prose long enough to wrap to three or four lines.
//
// Built with the editor's own `zipStore` and blank package, so the parts the
// importer needs (content types, relationships, styles) are exactly those of a
// document the product itself creates.
import { BLANK_DOCX_PARTS, zipStore } from "../../src/blank_document.mjs";

const W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"';
const PROSE =
  "The quick brown fox jumps over the lazy dog while the committee reviews the " +
  "quarterly figures, and every clause of this sentence exists only so that the " +
  "paragraph wraps across several lines of a Letter page at eleven points. ";

/**
 * A `.docx` of `paragraphs` body paragraphs, as bytes.
 *
 * @param {number} paragraphs
 * @returns {Uint8Array}
 */
export function largeDocx(paragraphs) {
  const body = [];
  for (let i = 0; i < paragraphs; i += 1) {
    if (i % 25 === 0) {
      body.push(
        `<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Section ${i / 25 + 1}</w:t></w:r></w:p>`,
      );
    } else {
      body.push(`<w:p><w:r><w:t xml:space="preserve">${i}. ${PROSE}${PROSE}</w:t></w:r></w:p>`);
    }
  }
  const parts = BLANK_DOCX_PARTS.map(([name, text]) => {
    if (name !== "word/document.xml") return [name, text];
    // Keep the blank package's own section properties, so page geometry is the
    // product's default rather than something this fixture invented.
    const sectPr = text.slice(text.indexOf("<w:sectPr>"), text.indexOf("</w:body>"));
    return [
      name,
      `<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document ${W}><w:body>${body.join("")}${sectPr}</w:body></w:document>`,
    ];
  });
  return zipStore(parts);
}
