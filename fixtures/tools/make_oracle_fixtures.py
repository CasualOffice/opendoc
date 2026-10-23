#!/usr/bin/env python3
"""Regenerates the self-generated oracle fixtures, fixtures/generated/oracle-*.docx.

These exist for the LibreOffice geometry gate (docs/94 H2, backlog FID-P-01):
documents this repository writes itself, so their rights are not in question,
each isolating ONE layout family so a divergence names its cause instead of
being one of a dozen things a page's text region could be measuring.

Constraints every fixture here must keep (the gate's premise, SKILL §11):

- Latin text only. The pinned metric-compatible faces cover Latin; anything
  else is substituted per environment and moves where the rest of a paragraph
  wraps (docs/94 §H2, "Font-parity scoping").
- Every run names its face through docDefaults or an explicit w:rFonts, and only
  a pinned face: Liberation Serif/Sans/Mono by name, or Calibri/Cambria (which
  both renderers map to Carlito/Caladea). Times New Roman, Arial and Courier New
  are deliberately NOT named: a macOS workstation has the real faces installed,
  LibreOffice embeds those instead of Liberation, and the gate then (correctly)
  refuses the reference as shaped with an unpinned face. No bullets, no symbols:
  LibreOffice's list bullet is a private-use `Symbol` glyph, which is what holds
  three corpus fixtures out.
- No images, fields or shapes: the comparison reduces the TEXT region only.

Output is deterministic (fixed zip timestamps, fixed member order), so the
printed sha256 values must match fixtures/manifest.json. Run from the
repository root:

    python3 fixtures/tools/make_oracle_fixtures.py
"""

import hashlib
import zipfile

W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"

CONTENT_TYPES = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
    '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
    '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
    '<Default Extension="xml" ContentType="application/xml"/>'
    '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
    '<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>'
    "</Types>"
)

ROOT_RELS = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
    '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>'
    "</Relationships>"
)

DOC_RELS = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
    '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>'
    "</Relationships>"
)


def styles(face="Liberation Serif", size=24):
    """docDefaults pinned explicitly: no reliance on either renderer's notion
    of an absent default (Word's and LibreOffice's differ)."""
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
        f'<w:styles xmlns:w="{W}">'
        "<w:docDefaults>"
        f'<w:rPrDefault><w:rPr><w:rFonts w:ascii="{face}" w:hAnsi="{face}" w:cs="{face}"/>'
        f'<w:sz w:val="{size}"/><w:szCs w:val="{size}"/><w:lang w:val="en-US"/></w:rPr></w:rPrDefault>'
        '<w:pPrDefault><w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/>'
        "</w:pPr></w:pPrDefault>"
        "</w:docDefaults>"
        '<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>'
        '<w:style w:type="table" w:default="1" w:styleId="TableNormal"><w:name w:val="Normal Table"/>'
        '<w:tblPr><w:tblCellMar><w:left w:w="108" w:type="dxa"/><w:right w:w="108" w:type="dxa"/>'
        "</w:tblCellMar></w:tblPr></w:style>"
        "</w:styles>"
    )


def esc(text):
    return text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def run(text, rpr=""):
    rpr = f"<w:rPr>{rpr}</w:rPr>" if rpr else ""
    return f'<w:r>{rpr}<w:t xml:space="preserve">{esc(text)}</w:t></w:r>'


def para(content, ppr=""):
    ppr = f"<w:pPr>{ppr}</w:pPr>" if ppr else ""
    if not content.startswith("<w:"):
        content = run(content)
    return f"<w:p>{ppr}{content}</w:p>"


def sect(w, h, top, right, bottom, left, orient=None):
    o = f' w:orient="{orient}"' if orient else ""
    return (
        f'<w:sectPr><w:pgSz w:w="{w}" w:h="{h}"{o}/>'
        f'<w:pgMar w:top="{top}" w:right="{right}" w:bottom="{bottom}" w:left="{left}" '
        'w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>'
    )


def document(body, final_sect):
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
        f'<w:document xmlns:w="{W}" xmlns:r="{R}"><w:body>{body}{final_sect}</w:body></w:document>'
    )


PROSE = [
    "The quick brown fox jumps over the lazy dog while the committee reviews the quarterly figures.",
    "Measured layout is a matter of advances, line heights and the boxes they accumulate into.",
    "A paragraph that wraps across several lines exercises the line breaker as well as the leading.",
    "Every renderer agrees on the words; the question this fixture asks is where each one lands.",
    "Margins, indents and spacing compose into one rectangle, and that rectangle is what we compare.",
]


def prose(n, start=0):
    return " ".join(PROSE[(start + i) % len(PROSE)] for i in range(n))


LETTER = (12240, 15840)
A4 = (11906, 16838)


def alignment_indent():
    """Alignment and indentation: every x-edge of the text region is owned by
    one of these paragraphs (left indent, right indent, hanging, first line)."""
    body = "".join(
        [
            para(prose(2), '<w:jc w:val="left"/><w:spacing w:after="160"/>'),
            para(prose(1, 1), '<w:jc w:val="center"/><w:spacing w:after="160"/>'),
            para(prose(1, 2), '<w:jc w:val="right"/><w:spacing w:after="160"/>'),
            para(prose(3, 3), '<w:jc w:val="both"/><w:spacing w:after="160"/>'),
            para(prose(2, 4), '<w:ind w:left="1440" w:right="1440"/><w:spacing w:after="160"/>'),
            para(prose(2), '<w:ind w:firstLine="720"/><w:spacing w:after="160"/>'),
            para(prose(2, 1), '<w:ind w:left="1440" w:hanging="720"/><w:spacing w:after="160"/>'),
            para(prose(2, 2), '<w:ind w:left="-360" w:right="-360"/>'),
        ]
    )
    return styles(), document(body, sect(*A4, 1440, 1440, 1440, 1440))


def line_spacing():
    """The w:spacing@line box model, one rule per paragraph, each wrapping onto
    at least two lines so the inter-line distance is visible in the extent."""
    rules = [
        '<w:spacing w:after="200" w:line="240" w:lineRule="auto"/>',
        '<w:spacing w:after="200" w:line="276" w:lineRule="auto"/>',
        '<w:spacing w:after="200" w:line="360" w:lineRule="auto"/>',
        '<w:spacing w:after="200" w:line="480" w:lineRule="auto"/>',
        '<w:spacing w:before="240" w:after="200" w:line="400" w:lineRule="atLeast"/>',
        '<w:spacing w:before="120" w:after="0" w:line="240" w:lineRule="auto"/>',
    ]
    body = "".join(para(prose(2, i), rule) for i, rule in enumerate(rules))
    return styles(), document(body, sect(*LETTER, 1440, 1440, 1440, 1440))


def font_families():
    """Each bundled metric-compatible family at several sizes and styles. The
    region's right edge and bottom are sums of these faces' advances and
    ascent/descent, so a wrong metric for any one face moves them."""
    faces = ["Liberation Serif", "Liberation Sans", "Liberation Mono", "Calibri", "Cambria"]
    sizes = [18, 22, 28, 40]
    parts = []
    for i, face in enumerate(faces):
        fonts = f'<w:rFonts w:ascii="{face}" w:hAnsi="{face}" w:cs="{face}"/>'
        for j, size in enumerate(sizes):
            style = ["", "<w:b/>", "<w:i/>", "<w:b/><w:i/>"][j]
            rpr = f"{fonts}{style}<w:sz w:val=\"{size}\"/><w:szCs w:val=\"{size}\"/>"
            text = f"{face} {size // 2}pt: " + PROSE[(i + j) % len(PROSE)]
            parts.append(para(run(text, rpr), '<w:spacing w:after="80"/>'))
    return styles(), document("".join(parts), sect(*LETTER, 1080, 1080, 1080, 1080))


def multipage_flow():
    """Body text flowing over several pages, plus an explicit page break, so
    the comparison covers page count and every page's own region rather than
    the first page alone."""
    parts = []
    for i in range(14):
        parts.append(para(prose(4, i), '<w:spacing w:after="160"/><w:jc w:val="both"/>'))
    parts.append(para('<w:r><w:br w:type="page"/></w:r>'))
    for i in range(4):
        parts.append(para(prose(3, i + 2), '<w:spacing w:after="160"/>'))
    return styles("Liberation Sans", 22), document("".join(parts), sect(*A4, 1134, 1134, 1134, 1134))


def sections_tabs():
    """Two sections with different page sizes, orientation and margins, and
    left/center/right/decimal tab stops. The first section ends with a
    next-page section break, so page 2 must take the landscape geometry."""
    tabs = (
        "<w:tabs>"
        '<w:tab w:val="left" w:pos="1440"/>'
        '<w:tab w:val="center" w:pos="4680"/>'
        '<w:tab w:val="right" w:pos="9360"/>'
        "</w:tabs>"
    )
    decimal = '<w:tabs><w:tab w:val="decimal" w:pos="5040"/></w:tabs>'
    tab = "<w:r><w:tab/></w:r>"
    first = [para(prose(2), '<w:spacing w:after="160"/>')]
    for label in ["Alpha", "Bravo", "Charlie"]:
        first.append(
            para(
                run(label) + tab + run("left") + tab + run("centred") + tab + run("right"),
                tabs,
            )
        )
    for amount in ["1.5", "22.75", "333.125", "4444.0"]:
        first.append(para(run("Amount") + tab + run(amount), decimal))
    # The section break lives in the last paragraph of section one.
    first.append(
        para(
            prose(1, 3),
            sect(*A4, 1440, 1440, 1440, 1440).replace("<w:sectPr>", '<w:sectPr><w:type w:val="nextPage"/>'),
        )
    )
    second = [para(prose(3, i), '<w:spacing w:after="160"/>') for i in range(3)]
    body = "".join(first + second)
    final = sect(LETTER[1], LETTER[0], 720, 1080, 720, 1080, orient="landscape")
    return styles("Cambria", 22), document(body, final)


def plain_table():
    """One bordered, fixed-width table with wrapping cell text, between two
    paragraphs. No nesting, no merges, no lists: only the cell box model
    (margins, borders, row height from content)."""
    widths = [2400, 3600, 3360]
    border = '<w:{0} w:val="single" w:sz="8" w:space="0" w:color="000000"/>'
    borders = "".join(border.format(e) for e in ["top", "left", "bottom", "right", "insideH", "insideV"])
    grid = "".join(f'<w:gridCol w:w="{w}"/>' for w in widths)
    rows = []
    for r in range(5):
        cells = []
        for c, w in enumerate(widths):
            text = prose(1, r + c) if (r + c) % 2 else f"Row {r + 1} column {c + 1}"
            cells.append(
                f'<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/></w:tcPr>{para(text)}</w:tc>'
            )
        rows.append(f"<w:tr>{''.join(cells)}</w:tr>")
    table = (
        f'<w:tbl><w:tblPr><w:tblW w:w="{sum(widths)}" w:type="dxa"/>'
        f'<w:tblLayout w:type="fixed"/><w:tblBorders>{borders}</w:tblBorders></w:tblPr>'
        f"<w:tblGrid>{grid}</w:tblGrid>{''.join(rows)}</w:tbl>"
    )
    body = (
        para(prose(2), '<w:spacing w:after="240"/>')
        + table
        + para(prose(2, 2), '<w:spacing w:before="240"/>')
    )
    return styles("Calibri", 22), document(body, sect(*LETTER, 1440, 1440, 1440, 1440))


FIXTURES = {
    "fixtures/generated/oracle-alignment-indent.docx": alignment_indent,
    "fixtures/generated/oracle-line-spacing.docx": line_spacing,
    "fixtures/generated/oracle-font-families.docx": font_families,
    "fixtures/generated/oracle-multipage-flow.docx": multipage_flow,
    "fixtures/generated/oracle-sections-tabs.docx": sections_tabs,
    "fixtures/generated/oracle-plain-table.docx": plain_table,
}


def write(path, styles_xml, document_xml):
    members = [
        ("[Content_Types].xml", CONTENT_TYPES),
        ("_rels/.rels", ROOT_RELS),
        ("word/_rels/document.xml.rels", DOC_RELS),
        ("word/styles.xml", styles_xml),
        ("word/document.xml", document_xml),
    ]
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        for name, data in members:
            info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o644 << 16
            z.writestr(info, data.encode("utf-8"))
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


if __name__ == "__main__":
    for path, build in FIXTURES.items():
        print(write(path, *build()), path)
