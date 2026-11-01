//! Synthetic ground truths for the reflow families `sample.docx` does not
//! exercise (`docs/182` §18.12): a watermarked multi-page document, aligned
//! numeric tables, and mixed spacing, indents and sizes. Written from the
//! constants below (no third-party or user content), byte for byte the same
//! on every run, so `synth <dir>` then `loop <dir>/<name>.docx ...` measures
//! the same thing on every machine.

use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:w10="urn:schemas-microsoft-com:office:word" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture""#;

/// A 1 x 1 opaque PNG: a picture whose drawn size is all extent.
const PIXEL_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
    0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E,
    0x44, 0xAE, 0x42, 0x60, 0x82,
];

const SENTENCES: [&str; 6] = [
    "The committee reviewed the quarterly figures and agreed that the forecast should stand until the next meeting.",
    "Each department is asked to confirm its numbers by the end of the month so that the summary can be prepared.",
    "Where a figure has changed since the last report, the reason for the change should be noted in the margin.",
    "The appendix lists every account by its code, its owner and the balance carried forward from the prior period.",
    "Questions about the process may be raised with the finance office, which will answer them in writing.",
    "This draft is circulated for comment only and should not be quoted or relied upon until it is final.",
];

fn prose(seed: usize, sentences: usize) -> String {
    (0..sentences)
        .map(|k| SENTENCES[(seed + k) % SENTENCES.len()])
        .collect::<Vec<_>>()
        .join(" ")
}

fn esc(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn para(ppr: &str, rpr: &str, text: &str) -> String {
    format!(
        r#"<w:p><w:pPr>{ppr}</w:pPr><w:r><w:rPr>{rpr}</w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
        esc(text)
    )
}

const STYLES: &str = r#"<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Liberation Sans" w:hAnsi="Liberation Sans" w:cs="Liberation Sans"/><w:sz w:val="22"/><w:szCs w:val="22"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="160" w:line="259" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style><w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:pPr><w:keepNext/><w:spacing w:before="360" w:after="120"/><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:b/><w:sz w:val="32"/><w:szCs w:val="32"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:pPr><w:keepNext/><w:spacing w:before="240" w:after="80"/><w:outlineLvl w:val="1"/></w:pPr><w:rPr><w:b/><w:sz w:val="26"/><w:szCs w:val="26"/></w:rPr></w:style>"#;

fn styles(extra: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles {NS}>{STYLES}{extra}</w:styles>"#
    )
}

fn content_types(header: bool, png: bool) -> String {
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>"#,
    );
    if png {
        out.push_str(r#"<Default Extension="png" ContentType="image/png"/>"#);
    }
    out.push_str(r#"<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>"#);
    if header {
        out.push_str(r#"<Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/>"#);
    }
    out.push_str("</Types>");
    out
}

fn document_rels(header: bool, png: bool) -> String {
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdStyles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#,
    );
    if header {
        out.push_str(r#"<Relationship Id="rIdHeader" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/>"#);
    }
    if png {
        out.push_str(r#"<Relationship Id="rIdPixel" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/pixel.png"/>"#);
    }
    out.push_str("</Relationships>");
    out
}

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;

/// Letter, one-inch margins.
const SECTION: &str = r#"<w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>"#;

fn document(body: &str, header: bool) -> String {
    let reference = if header {
        r#"<w:headerReference w:type="default" r:id="rIdHeader"/>"#
    } else {
        ""
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {NS}><w:body>{body}<w:sectPr>{reference}{SECTION}</w:sectPr></w:body></w:document>"#
    )
}

fn package(parts: &[(&str, Vec<u8>)]) -> Result<Vec<u8>, String> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in parts {
        writer
            .start_file(
                *name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .map_err(|e| e.to_string())?;
        writer.write_all(bytes).map_err(|e| e.to_string())?;
    }
    writer
        .finish()
        .map(Cursor::into_inner)
        .map_err(|e| e.to_string())
}

/// Three pages of prose under a text watermark exactly as Word's Design ▸
/// Watermark writes one ("CONFIDENTIAL", diagonal, semitransparent silver).
fn watermarked() -> Result<Vec<u8>, String> {
    let mut body = String::new();
    for page in 0..3 {
        body.push_str(&para(
            r#"<w:pStyle w:val="Heading1"/>"#,
            "",
            &format!("Section {}", page + 1),
        ));
        for k in 0..5 {
            body.push_str(&para("", "", &prose(page * 5 + k, 4)));
        }
        if page < 2 {
            body.push_str(r#"<w:p><w:r><w:br w:type="page"/></w:r></w:p>"#);
        }
    }
    let header = format!(
        r##"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:hdr {NS}><w:p><w:pPr><w:spacing w:after="0"/></w:pPr><w:r><w:pict><v:shapetype id="_x0000_t136" coordsize="21600,21600" o:spt="136" adj="10800" path="m@7,l@8,m@5,21600l@11,21600e"><v:path textpathok="t"/><v:textpath on="t" fitshape="t"/></v:shapetype><v:shape id="PowerPlusWaterMarkObject100" o:spid="_x0000_s2049" type="#_x0000_t136" style="position:absolute;margin-left:0;margin-top:0;width:527.85pt;height:131.95pt;rotation:315;z-index:-251658752;mso-position-horizontal:center;mso-position-horizontal-relative:margin;mso-position-vertical:center;mso-position-vertical-relative:margin" o:allowincell="f" fillcolor="silver" stroked="f"><v:fill opacity=".5"/><v:textpath style="font-family:&quot;Liberation Sans&quot;;font-size:1pt" string="CONFIDENTIAL"/></v:shape></w:pict></w:r></w:p></w:hdr>"##
    );
    package(&[
        (
            "[Content_Types].xml",
            content_types(true, false).into_bytes(),
        ),
        ("_rels/.rels", ROOT_RELS.as_bytes().to_vec()),
        (
            "word/_rels/document.xml.rels",
            document_rels(true, false).into_bytes(),
        ),
        ("word/document.xml", document(&body, true).into_bytes()),
        ("word/styles.xml", styles("").into_bytes()),
        ("word/header1.xml", header.into_bytes()),
    ])
}

fn cell(width: u32, jc: &str, bold: bool, text: &str) -> String {
    format!(
        r#"<w:tc><w:tcPr><w:tcW w:w="{width}" w:type="dxa"/></w:tcPr>{}</w:tc>"#,
        para(
            &format!(r#"<w:spacing w:after="0"/><w:jc w:val="{jc}"/>"#),
            if bold { "<w:b/>" } else { "" },
            text
        )
    )
}

/// A ruled and a borderless table of figures: a left-aligned label
/// column, right-aligned amounts, a centred status, a centred header row,
/// and columns of different widths.
fn numeric_tables() -> Result<Vec<u8>, String> {
    let widths = [2880u32, 1872, 1872, 1440, 1296];
    let header = ["Account", "Budget", "Actual", "Status", "Code"];
    let rows = [
        ["Salaries", "1,250,000.00", "1,198,442.50", "Under", "A-101"],
        ["Travel", "84,500.00", "91,207.25", "Over", "A-205"],
        ["Equipment", "310,000.00", "7,880.00", "Under", "B-310"],
        ["Training", "45,000.00", "44,999.99", "Met", "B-412"],
        ["Consulting", "2,000.00", "118,600.10", "Over", "C-007"],
        ["Rent", "960,000.00", "960,000.00", "Met", "C-120"],
    ];
    let aligns = ["left", "right", "right", "center", "left"];
    let mut body = String::new();
    for (k, ruled) in [true, false].into_iter().enumerate() {
        body.push_str(&para(
            r#"<w:pStyle w:val="Heading1"/>"#,
            "",
            if ruled {
                "Ruled figures"
            } else {
                "Borderless figures"
            },
        ));
        body.push_str(&para("", "", &prose(k, 2)));
        let borders = if ruled {
            r#"<w:tblBorders><w:top w:val="single" w:sz="4" w:space="0" w:color="000000"/><w:left w:val="single" w:sz="4" w:space="0" w:color="000000"/><w:bottom w:val="single" w:sz="4" w:space="0" w:color="000000"/><w:right w:val="single" w:sz="4" w:space="0" w:color="000000"/><w:insideH w:val="single" w:sz="4" w:space="0" w:color="000000"/><w:insideV w:val="single" w:sz="4" w:space="0" w:color="000000"/></w:tblBorders>"#
        } else {
            ""
        };
        body.push_str(&format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="9360" w:type="dxa"/>{borders}<w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid>{}</w:tblGrid>"#,
            widths
                .iter()
                .map(|w| format!(r#"<w:gridCol w:w="{w}"/>"#))
                .collect::<String>()
        ));
        body.push_str("<w:tr>");
        for (c, h) in header.iter().enumerate() {
            body.push_str(&cell(widths[c], "center", true, h));
        }
        body.push_str("</w:tr>");
        for row in &rows {
            body.push_str("<w:tr>");
            for (c, text) in row.iter().enumerate() {
                body.push_str(&cell(widths[c], aligns[c], false, text));
            }
            body.push_str("</w:tr>");
        }
        body.push_str("</w:tbl>");
        body.push_str(&para("", "", &prose(k + 2, 2)));
    }
    package(&[
        (
            "[Content_Types].xml",
            content_types(false, false).into_bytes(),
        ),
        ("_rels/.rels", ROOT_RELS.as_bytes().to_vec()),
        (
            "word/_rels/document.xml.rels",
            document_rels(false, false).into_bytes(),
        ),
        ("word/document.xml", document(&body, false).into_bytes()),
        ("word/styles.xml", styles("").into_bytes()),
    ])
}

fn picture(width_pt: u32, height_pt: u32) -> String {
    let (cx, cy) = (width_pt * 12_700, height_pt * 12_700);
    format!(
        r#"<w:p><w:pPr><w:spacing w:before="120" w:after="120"/></w:pPr><w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/><wp:docPr id="1" name="Pixel"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="1" name="pixel.png"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rIdPixel"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#
    )
}

/// Body text at 10.5 pt, one and a half lines, a half-inch first-line
/// indent and 6 pt after; block quotes indented half an inch both sides at
/// 9.5 pt, single, 3 pt after; exact 14 pt notes; headings at 16 and 13 pt
/// with their own space; a picture of an exact size.
fn mixed_spacing() -> Result<Vec<u8>, String> {
    let body_ppr = r#"<w:pStyle w:val="BodyText"/>"#;
    let quote_ppr = r#"<w:pStyle w:val="Quote"/>"#;
    let note_ppr = r#"<w:pStyle w:val="Note"/>"#;
    let mut body = String::new();
    for section in 0..3 {
        body.push_str(&para(
            r#"<w:pStyle w:val="Heading1"/>"#,
            "",
            &format!("Part {}", section + 1),
        ));
        for k in 0..3 {
            body.push_str(&para(body_ppr, "", &prose(section * 3 + k, 3)));
        }
        body.push_str(&para(
            r#"<w:pStyle w:val="Heading2"/>"#,
            "",
            &format!("Details {}", section + 1),
        ));
        body.push_str(&para(quote_ppr, "", &prose(section + 1, 2)));
        body.push_str(&para(quote_ppr, "", &prose(section + 4, 2)));
        body.push_str(&para(body_ppr, "", &prose(section + 2, 3)));
        if section == 1 {
            body.push_str(&picture(144, 96));
        }
        body.push_str(&para(note_ppr, "", &prose(section + 3, 2)));
    }
    let extra = r#"<w:style w:type="paragraph" w:customStyle="1" w:styleId="BodyText"><w:name w:val="Body Text"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:after="120" w:line="360" w:lineRule="auto"/><w:ind w:firstLine="720"/></w:pPr><w:rPr><w:sz w:val="21"/><w:szCs w:val="21"/></w:rPr></w:style><w:style w:type="paragraph" w:customStyle="1" w:styleId="Quote"><w:name w:val="Quote"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:after="60" w:line="240" w:lineRule="auto"/><w:ind w:left="720" w:right="720"/></w:pPr><w:rPr><w:sz w:val="19"/><w:szCs w:val="19"/></w:rPr></w:style><w:style w:type="paragraph" w:customStyle="1" w:styleId="Note"><w:name w:val="Note"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:before="120" w:after="0" w:line="280" w:lineRule="exact"/></w:pPr><w:rPr><w:sz w:val="18"/><w:szCs w:val="18"/></w:rPr></w:style>"#;
    package(&[
        (
            "[Content_Types].xml",
            content_types(false, true).into_bytes(),
        ),
        ("_rels/.rels", ROOT_RELS.as_bytes().to_vec()),
        (
            "word/_rels/document.xml.rels",
            document_rels(false, true).into_bytes(),
        ),
        ("word/document.xml", document(&body, false).into_bytes()),
        ("word/styles.xml", styles(extra).into_bytes()),
        ("word/media/pixel.png", PIXEL_PNG.to_vec()),
    ])
}

/// Every synthetic ground truth, by file name.
pub fn all() -> Result<Vec<(&'static str, Vec<u8>)>, String> {
    Ok(vec![
        ("watermarked.docx", watermarked()?),
        ("numeric-tables.docx", numeric_tables()?),
        ("mixed-spacing.docx", mixed_spacing()?),
    ])
}
