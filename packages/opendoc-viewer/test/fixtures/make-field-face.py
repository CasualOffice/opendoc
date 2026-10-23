#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Writes field-face.docx: two Calibri paragraphs that must look identical.

Paragraph 1 is the literal text "Page 1". Paragraph 2 is "Page " followed by a
PAGE field whose cached result is "7" and whose value on page 1 is "1". Both
paragraphs therefore paint "Page 1" in Calibri, and a viewer that draws a field
value from the face that shaped it renders the two lines pixel-identically.

    python3 -I make-field-face.py   (writes field-face.docx beside this file)
"""
import os
import zipfile

W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'
RPR = '<w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="48"/></w:rPr>'
PPR = '<w:pPr><w:spacing w:before="0" w:after="240" w:line="240" w:lineRule="auto"/></w:pPr>'
BODY = (
    f'<w:p>{PPR}<w:r>{RPR}<w:t>Page 1</w:t></w:r></w:p>'
    f'<w:p>{PPR}<w:r>{RPR}<w:t xml:space="preserve">Page </w:t></w:r>'
    f'<w:fldSimple w:instr=" PAGE "><w:r>{RPR}<w:t>7</w:t></w:r></w:fldSimple></w:p>'
)
PARTS = {
    "[Content_Types].xml": (
        '<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        '<Override PartName="/word/document.xml" '
        'ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
        "</Types>"
    ),
    "_rels/.rels": (
        '<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" '
        'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" '
        'Target="word/document.xml"/></Relationships>'
    ),
    "word/document.xml": f'<?xml version="1.0"?><w:document {W}><w:body>{BODY}</w:body></w:document>',
}

out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "field-face.docx")
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as package:
    for name, text in PARTS.items():
        info = zipfile.ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
        info.compress_type = zipfile.ZIP_DEFLATED
        package.writestr(info, text)
print(out)
