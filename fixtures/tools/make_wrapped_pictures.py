#!/usr/bin/env python3
"""Regenerates fixtures/generated/wrapped-pictures.docx.

Pictures in the containers Word really puts them in, every one of which used to
paint and then refuse the click or the command aimed at it (docs/109 HF-166,
HF-214, HF-252, HF-254):

* a FLOATING picture inside an INCLUDEPICTURE field result — how Word stores an
  image pasted from a web page, and the shape of the owner's partner logos;
* a floating picture inside an inline content control (a template's logo slot);
* an anchored GROUP of two pictures (two logos grouped side by side);
* an INLINE picture inside an INCLUDEPICTURE field result.

Every picture has a known page-relative frame, so a browser spec can click the
middle of one without sweeping. The image is a real 8x4 PNG, built here, so the
pictures paint as pictures rather than as missing-image placeholders: blue on
the left, orange on the right, so a flip is something a spec can see.

Deterministic: fixed member order and timestamps, so the printed sha256 is
stable and must match fixtures/manifest.json. Run from the repository root.
"""

import hashlib
import struct
import zipfile
import zlib

OUT = "fixtures/generated/wrapped-pictures.docx"
EMU_IN = 914400


def png(width, height, left, right):
    """A minimal truecolour PNG: the left half one colour, the right half
    another — asymmetric on purpose, so a horizontal flip is VISIBLE."""

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    half = width // 2
    row = b"\x00" + bytes(left) * half + bytes(right) * (width - half)
    raw = row * height
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


NS = (
    'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" '
    'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" '
    'xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" '
    'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" '
    'xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" '
    'xmlns:wpg="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup"'
)

_ids = iter(range(10, 1000))


def pic(name, cx, cy, off=(0, 0), descr=""):
    i = next(_ids)
    return (
        f'<pic:pic><pic:nvPicPr><pic:cNvPr id="{i}" name="{name}" descr="{descr}"/><pic:cNvPicPr/></pic:nvPicPr>'
        '<pic:blipFill><a:blip r:embed="rIdImg"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>'
        f'<pic:spPr><a:xfrm><a:off x="{off[0]}" y="{off[1]}"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm>'
        '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>'
    )


def anchor(inner, x, y, cx, cy, descr, uri="http://schemas.openxmlformats.org/drawingml/2006/picture"):
    i = next(_ids)
    return (
        '<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="114300" distR="114300" simplePos="0" '
        f'relativeHeight="{i}" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1">'
        '<wp:simplePos x="0" y="0"/>'
        f'<wp:positionH relativeFrom="page"><wp:posOffset>{x}</wp:posOffset></wp:positionH>'
        f'<wp:positionV relativeFrom="page"><wp:posOffset>{y}</wp:posOffset></wp:positionV>'
        f'<wp:extent cx="{cx}" cy="{cy}"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:wrapNone/>'
        f'<wp:docPr id="{i}" name="Picture {i}" descr="{descr}"/><wp:cNvGraphicFramePr/>'
        f'<a:graphic><a:graphicData uri="{uri}">{inner}</a:graphicData></a:graphic>'
        '</wp:anchor></w:drawing></w:r>'
    )


def inline(inner, cx, cy, descr):
    i = next(_ids)
    return (
        '<w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0">'
        f'<wp:extent cx="{cx}" cy="{cy}"/><wp:docPr id="{i}" name="Picture {i}" descr="{descr}"/>'
        '<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">'
        f'{inner}</a:graphicData></a:graphic></wp:inline></w:drawing></w:r>'
    )


def include_picture(result):
    return (
        '<w:r><w:fldChar w:fldCharType="begin"/></w:r>'
        '<w:r><w:instrText xml:space="preserve"> INCLUDEPICTURE "https://example.com/logo.png" \\* MERGEFORMATINET </w:instrText></w:r>'
        '<w:r><w:fldChar w:fldCharType="separate"/></w:r>'
        f"{result}"
        '<w:r><w:fldChar w:fldCharType="end"/></w:r>'
    )


def text(value):
    return f'<w:r><w:t xml:space="preserve">{value}</w:t></w:r>'


W, H = int(1.5 * EMU_IN), int(0.75 * EMU_IN)
field_float = anchor(pic("Field logo", W, H), 1 * EMU_IN, int(1.5 * EMU_IN), W, H, "Field logo")
sdt_float = anchor(pic("Control logo", W, H), int(4.5 * EMU_IN), int(1.5 * EMU_IN), W, H, "Control logo")
group = (
    '<wpg:wgp><wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm>'
    f'<a:off x="0" y="0"/><a:ext cx="{2 * W}" cy="{H}"/><a:chOff x="0" y="0"/><a:chExt cx="{2 * W}" cy="{H}"/>'
    '</a:xfrm></wpg:grpSpPr>'
    + pic("Partner A", W, H, (0, 0), "Partner A")
    + pic("Partner B", W, H, (W, 0), "Partner B")
    + "</wpg:wgp>"
)
group_float = anchor(group, 1 * EMU_IN, 4 * EMU_IN, 2 * W, H, "Partner logos",
                     uri="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup")

filler = "".join(
    f"<w:p>{text(f'Body line {n}: text the floating logos sit over, so a click that misses lands on a caret.')}</w:p>"
    for n in range(14)
)
body = (
    f"<w:p>{text('Wrapped pictures ')}{include_picture(field_float)}"
    f'<w:sdt><w:sdtPr><w:id w:val="7"/><w:alias w:val="Logo"/></w:sdtPr><w:sdtContent>{sdt_float}</w:sdtContent></w:sdt>'
    f"{group_float}</w:p>"
    + filler
    + f"<w:p>{text('Inline field picture: ')}{include_picture(inline(pic('Inline logo', W, H), W, H, 'Inline logo'))}{text(' after it.')}</w:p>"
)
document = (
    f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<w:document {NS}><w:body>{body}'
    '<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" '
    'w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>'
)
content_types = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
    '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
    '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
    '<Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/>'
    '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
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
    '<Relationship Id="rIdImg" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/>'
    "</Relationships>"
)

members = [
    ("[Content_Types].xml", content_types.encode()),
    ("_rels/.rels", root_rels.encode()),
    ("word/document.xml", document.encode()),
    ("word/_rels/document.xml.rels", document_rels.encode()),
    ("word/media/image1.png", png(8, 4, (0x33, 0x55, 0xC4), (0xF0, 0x8C, 0x00))),
]
with zipfile.ZipFile(OUT, "w", zipfile.ZIP_DEFLATED) as package:
    for name, data in members:
        info = zipfile.ZipInfo(name, date_time=(2026, 10, 9, 0, 0, 0))
        info.compress_type = zipfile.ZIP_DEFLATED
        package.writestr(info, data)

with open(OUT, "rb") as handle:
    print(OUT, hashlib.sha256(handle.read()).hexdigest())
