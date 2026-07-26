#!/usr/bin/env python3
"""Regenerates fixtures/corpus/synthetic-rich-metadata.docx.

A minimal but valid DOCX whose docProps/{core,app,custom}.xml parts carry rich
metadata (title/author/dates, company/counts/heading pairs/titles-of-parts, and
typed custom properties). Run from the repository root; the printed sha256 must
match fixtures/manifest.json.
"""

import hashlib
import os
import zipfile

OUT = "fixtures/corpus/synthetic-rich-metadata.docx"

content_types = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
    '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
    '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
    '<Default Extension="xml" ContentType="application/xml"/>'
    '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
    '<Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>'
    '<Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/>'
    '<Override PartName="/docProps/custom.xml" ContentType="application/vnd.openxmlformats-officedocument.custom-properties+xml"/>'
    '</Types>'
)

root_rels = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
    '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>'
    '<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>'
    '<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/>'
    '<Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties" Target="docProps/custom.xml"/>'
    '</Relationships>'
)

document = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
    '<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">'
    '<w:body><w:p><w:r><w:t>Rich document-properties fixture body.</w:t></w:r></w:p>'
    '<w:sectPr><w:pgSz w:w="12240" w:h="15840"/>'
    '<w:pgMar w:top="1440" w:bottom="1440" w:left="1440" w:right="1440"/></w:sectPr>'
    '</w:body></w:document>'
)

doc_rels = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>'
)

core = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
    '<cp:coreProperties '
    'xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" '
    'xmlns:dc="http://purl.org/dc/elements/1.1/" '
    'xmlns:dcterms="http://purl.org/dc/terms/" '
    'xmlns:dcmitype="http://purl.org/dc/dcmitype/" '
    'xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">'
    '<dc:title>Annual Metadata Report</dc:title>'
    '<dc:subject>Metadata round-trip</dc:subject>'
    '<dc:creator>Ada Lovelace</dc:creator>'
    '<cp:keywords>metadata, docprops, roundtrip</cp:keywords>'
    '<dc:description>A fixture exercising rich document properties.</dc:description>'
    '<cp:lastModifiedBy>Grace Hopper</cp:lastModifiedBy>'
    '<cp:revision>3</cp:revision>'
    '<dcterms:created xsi:type="dcterms:W3CDTF">2026-01-15T08:30:00Z</dcterms:created>'
    '<dcterms:modified xsi:type="dcterms:W3CDTF">2026-07-20T14:45:00Z</dcterms:modified>'
    '<cp:lastPrinted>2026-07-01T00:00:00Z</cp:lastPrinted>'
    '<cp:category>Reports</cp:category>'
    '<cp:contentStatus>Final</cp:contentStatus>'
    '<dc:language>en-US</dc:language>'
    '<cp:version>1.2</cp:version>'
    '</cp:coreProperties>'
)

app = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
    '<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties" '
    'xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes">'
    '<Template>Normal.dotm</Template>'
    '<Manager>Charles Babbage</Manager>'
    '<Company>Analytical Engines Ltd</Company>'
    '<Pages>4</Pages><Words>3200</Words><Characters>18000</Characters>'
    '<Lines>260</Lines><Paragraphs>70</Paragraphs><TotalTime>128</TotalTime>'
    '<DocSecurity>0</DocSecurity><ScaleCrop>false</ScaleCrop>'
    '<HeadingPairs><vt:vector size="4" baseType="variant">'
    '<vt:variant><vt:lpstr>Title</vt:lpstr></vt:variant><vt:variant><vt:i4>1</vt:i4></vt:variant>'
    '<vt:variant><vt:lpstr>Sections</vt:lpstr></vt:variant><vt:variant><vt:i4>3</vt:i4></vt:variant>'
    '</vt:vector></HeadingPairs>'
    '<TitlesOfParts><vt:vector size="2" baseType="lpstr">'
    '<vt:lpstr>Annual Metadata Report</vt:lpstr><vt:lpstr>Appendix A</vt:lpstr>'
    '</vt:vector></TitlesOfParts>'
    '<LinksUpToDate>true</LinksUpToDate>'
    '<CharactersWithSpaces>21000</CharactersWithSpaces>'
    '<SharedDoc>false</SharedDoc>'
    '<HyperlinkBase>https://example.com</HyperlinkBase>'
    '<Application>OpenDoc Test Harness</Application>'
    '<AppVersion>1.0000</AppVersion>'
    '</Properties>'
)

custom = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
    '<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/custom-properties" '
    'xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes">'
    '<property fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="2" name="Editor"><vt:lpwstr>Grace Hopper</vt:lpwstr></property>'
    '<property fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="3" name="Revision Number"><vt:i4>7</vt:i4></property>'
    '<property fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="4" name="Ratio"><vt:r8>2.5</vt:r8></property>'
    '<property fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="5" name="Approved"><vt:bool>true</vt:bool></property>'
    '<property fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="6" name="Received"><vt:filetime>2026-03-01T09:00:00Z</vt:filetime></property>'
    '</Properties>'
)

parts = [
    ("[Content_Types].xml", content_types),
    ("_rels/.rels", root_rels),
    ("docProps/app.xml", app),
    ("docProps/core.xml", core),
    ("docProps/custom.xml", custom),
    ("word/_rels/document.xml.rels", doc_rels),
    ("word/document.xml", document),
]

with zipfile.ZipFile(OUT, "w", zipfile.ZIP_DEFLATED) as z:
    for name, data in parts:
        zi = zipfile.ZipInfo(name)
        zi.compress_type = zipfile.ZIP_DEFLATED
        z.writestr(zi, data.encode("utf-8"))

with open(OUT, "rb") as f:
    print("sha256", hashlib.sha256(f.read()).hexdigest())
print("bytes", os.path.getsize(OUT))
