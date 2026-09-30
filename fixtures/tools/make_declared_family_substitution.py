#!/usr/bin/env python3
"""Regenerates fixtures/corpus/synthetic-declared-family-substitution.docx.

# What this fixture is for

A customer document (under NDA, never committed) wrapped its paragraphs
differently from LibreOffice's rendering of the same file, and the working
hypothesis was font substitution: the document declares faces this engine does
not bundle, and the two renderers were suspected of choosing different
replacements, so glyph advances — and therefore where each line breaks — would
differ.

That hypothesis is only testable against a document whose *declared* faces are
controlled. This fixture is that document, reduced to the shape of the real one
and to nothing else:

- every paragraph carries the SAME Latin-only text at the SAME size, so the only
  variable between them is the declared face;
- the text is long enough to wrap several times in the body measure, so a
  difference in advance shows up as a different wrap point rather than as a few
  twips on one line;
- paragraphs are `start`-aligned, NOT justified. The real document's `Normal`
  style is `w:jc="both"`, and justification hides exactly what is being measured:
  both renderers stretch a line to the same right margin, so the natural width is
  erased and only the word COUNT differs. Left-aligned, the line's right edge IS
  the natural width;
- `word/fontTable.xml` declares each face's `w:family`, which is the signal
  `casual_doc_layout::font_substitution` consults when a name is not in its
  known-family table. Three cases, chosen from the real document's own font table:

  | declared face | `w:family` | what the engine resolved BEFORE the fix | why it is here |
  | --- | --- | --- | --- |
  | Calibri | roman | Carlito (metric-compatible) | control: LibreOffice substitutes Carlito too, so the two sides must agree to within a few twips whatever else changes |
  | Calibri Light | roman | Liberation Serif (by declared class) | the suspect: no metric partner is listed for this exact name, so the declared `roman` routed a variant of a SANS family to a SERIF face while LibreOffice picks Carlito |
  | Carlito | roman | Carlito (bundled, by name) | the real document really does declare Carlito as `roman`; a name in the known-family table must beat the declaration, or a metric-compatible match would be thrown away for a worse one |

**Why no Times New Roman row, though the real document has one.** Every face here
must be one that *neither* renderer has installed, or the comparison stops
measuring substitution. Calibri, Calibri Light and Carlito are absent from a
stock macOS and a stock Linux runner alike, so both sides substitute and the
advances are the subject. Times New Roman is installed on macOS: this engine
(with the OS font fallback on, as the native build ships it) correctly uses the
real face, whose lines are then excluded from the comparison for want of font
parity, while LibreOffice keeps them — so the two sides compare different regions
and the report fills with findings about the measuring machine. That is the same
platform-dependence that holds three fixtures out of the H2 gate. A face the host
happens to own is not a control.

Run from the repository root; the printed sha256 must match
fixtures/manifest.json.
"""

import hashlib
import os
import zipfile

OUT = "fixtures/corpus/synthetic-declared-family-substitution.docx"

# Latin-only, no punctuation a pinned face might not cover, and long enough to
# wrap three or four times in a six-inch measure. Deliberately dull: the words
# carry no meaning, only width.
SAMPLE = (
    "The quick brown fox jumps over the lazy dog while the confidential "
    "information described in this agreement remains subject to the obligations "
    "set out above and neither party shall disclose any portion of it to any "
    "third person without the prior written consent of the disclosing party."
)

# (declared face name, w:family value)
FACES = [
    ("Calibri", "roman"),
    ("Calibri Light", "roman"),
    ("Carlito", "roman"),
]

content_types = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
    '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
    '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
    '<Default Extension="xml" ContentType="application/xml"/>'
    '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
    '<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>'
    '<Override PartName="/word/fontTable.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml"/>'
    "</Types>"
)

root_rels = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
    '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>'
    "</Relationships>"
)

document_rels = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
    '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>'
    '<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/fontTable" Target="fontTable.xml"/>'
    "</Relationships>"
)

# 11pt everywhere (22 half-points), matching the real document's body size, and
# no paragraph spacing, so a vertical difference is line pitch and nothing else.
styles = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
    '<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">'
    "<w:docDefaults><w:rPrDefault><w:rPr>"
    '<w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="22"/>'
    "</w:rPr></w:rPrDefault>"
    '<w:pPrDefault><w:pPr><w:spacing w:before="0" w:after="0"/>'
    '<w:jc w:val="start"/></w:pPr></w:pPrDefault></w:docDefaults>'
    '<w:style w:type="paragraph" w:default="1" w:styleId="Normal">'
    '<w:name w:val="Normal"/>'
    "</w:style>"
    "</w:styles>"
)


def font_table():
    entries = "".join(
        f'<w:font w:name="{name}">'
        f'<w:family w:val="{family}"/><w:pitch w:val="variable"/>'
        "</w:font>"
        for name, family in FACES
    )
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
        '<w:fonts xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">'
        f"{entries}</w:fonts>"
    )


def paragraph(face):
    """One labelled paragraph: the face's name, then the sample in that face.

    The label is in the SAME face, so a reader of the rendered page can tell the
    paragraphs apart without a second variable entering the measurement.
    """
    run = (
        f'<w:r><w:rPr><w:rFonts w:ascii="{face}" w:hAnsi="{face}"/>'
        '<w:sz w:val="22"/></w:rPr>'
        f"<w:t xml:space=\"preserve\">{face}: {SAMPLE}</w:t></w:r>"
    )
    return f'<w:p><w:pPr><w:jc w:val="start"/></w:pPr>{run}</w:p>'


def document_xml():
    body = "".join(paragraph(name) for name, _ in FACES)
    # A4 portrait with 1-inch margins: an 8976-twip measure, wide enough that the
    # sample wraps three or four times and narrow enough that a per-line advance
    # difference of a fraction of a percent moves a wrap point.
    section = (
        "<w:sectPr>"
        '<w:pgSz w:w="11906" w:h="16838"/>'
        '<w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" '
        'w:header="720" w:footer="720" w:gutter="0"/>'
        "</w:sectPr>"
    )
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
        '<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">'
        f"<w:body>{body}{section}</w:body></w:document>"
    )


def main():
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    parts = [
        ("[Content_Types].xml", content_types),
        ("_rels/.rels", root_rels),
        ("word/_rels/document.xml.rels", document_rels),
        ("word/document.xml", document_xml()),
        ("word/styles.xml", styles),
        ("word/fontTable.xml", font_table()),
    ]
    # Fixed timestamps and no compression variance, so the package is
    # byte-reproducible and its sha256 can be manifested.
    with zipfile.ZipFile(OUT, "w", zipfile.ZIP_DEFLATED) as package:
        for name, text in parts:
            info = zipfile.ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o600 << 16
            package.writestr(info, text.encode("utf-8"))
    with open(OUT, "rb") as handle:
        print(f"{hashlib.sha256(handle.read()).hexdigest()}  {OUT}")


if __name__ == "__main__":
    main()
