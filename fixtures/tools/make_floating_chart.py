#!/usr/bin/env python3
"""Regenerates fixtures/generated/floating-chart.docx (docs/109 HF-260).

A FLOATING chart, the way Word writes one when a chart's Layout Options are
set to Square: the `c:chart` graphic of fixtures/generated/chart.docx inside a
`wp:anchor` rather than a `wp:inline`, positioned 3.5 in from the left margin
and 0.25 in below the top of its paragraph, square-wrapped with Word's default
0.125 in side distances, and with enough body text beside it that the wrap is
visible.

Every other part (the chart, its style and colour parts, the embedded
workbook, the relationships and content types) is copied byte for byte from
chart.docx, so this fixture differs from it in exactly one thing: the frame.

Deterministic: fixed member order and timestamps, so the printed sha256 is
stable and must match fixtures/manifest.json. Run from the repository root.
"""

import hashlib
import zipfile

SOURCE = "fixtures/generated/chart.docx"
OUT = "fixtures/generated/floating-chart.docx"
EMU_IN = 914400

NS = (
    'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" '
    'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" '
    'xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" '
    'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" '
    'xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"'
)

WIDTH, HEIGHT = 3 * EMU_IN, 2 * EMU_IN
ANCHOR = (
    f'<wp:anchor distT="0" distB="0" distL="{EMU_IN // 8}" distR="{EMU_IN // 8}" '
    'simplePos="0" relativeHeight="251659264" behindDoc="0" locked="0" '
    'layoutInCell="1" allowOverlap="1">'
    '<wp:simplePos x="0" y="0"/>'
    f'<wp:positionH relativeFrom="margin"><wp:posOffset>{int(3.5 * EMU_IN)}</wp:posOffset></wp:positionH>'
    f'<wp:positionV relativeFrom="paragraph"><wp:posOffset>{EMU_IN // 4}</wp:posOffset></wp:positionV>'
    f'<wp:extent cx="{WIDTH}" cy="{HEIGHT}"/>'
    '<wp:effectExtent l="0" t="0" r="0" b="0"/>'
    '<wp:wrapSquare wrapText="bothSides"/>'
    '<wp:docPr id="1" name="Chart 1" descr="Revenue by quarter"/>'
    '<wp:cNvGraphicFramePr/>'
    '<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart">'
    '<c:chart r:id="rId4"/></a:graphicData></a:graphic>'
    "</wp:anchor>"
)

WORDS = (
    "Quarterly revenue rose in every quarter but the third, and the target line "
    "shows where the plan expected it to be. "
)


def paragraph(text):
    return f'<w:p><w:r><w:t xml:space="preserve">{text}</w:t></w:r></w:p>'


document = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
    f"<w:document {NS}><w:body>"
    + paragraph("Revenue by quarter, with the target as a line.")
    + f"<w:p><w:r><w:drawing>{ANCHOR}</w:drawing></w:r>"
    + f'<w:r><w:t xml:space="preserve">{WORDS * 6}</w:t></w:r></w:p>'
    + paragraph(WORDS * 4)
    + paragraph("Figure 1.")
    + "</w:body></w:document>"
)

with zipfile.ZipFile(SOURCE) as source:
    members = [
        (info.filename, source.read(info.filename))
        for info in source.infolist()
        if info.filename != "word/document.xml"
    ]
members.insert(0, ("word/document.xml", document.encode()))
members.sort(key=lambda member: (member[0] != "[Content_Types].xml", member[0]))

with zipfile.ZipFile(OUT, "w", zipfile.ZIP_DEFLATED) as package:
    for name, data in members:
        info = zipfile.ZipInfo(name, date_time=(2026, 10, 30, 0, 0, 0))
        info.compress_type = zipfile.ZIP_DEFLATED
        package.writestr(info, data)

with open(OUT, "rb") as handle:
    print(OUT, hashlib.sha256(handle.read()).hexdigest())
