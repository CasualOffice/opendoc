//! A deterministic HEAVY page for the render benchmarks (`docs/architecture/179` §2): dense
//! 9pt justified text with mixed bold, italic, underline and highlight, a shaded
//! 6 × 12 table and a line of colour emoji. Shared by `render_profile` and
//! `casual-doc-render-gpu`'s `gpu_profile` through `#[path]`, so both time the
//! identical document.

use std::io::{Cursor, Write};

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

const WORDS: &[&str] = &[
    "render",
    "pipeline",
    "display",
    "list",
    "glyph",
    "outline",
    "raster",
    "page",
    "zoom",
    "scroll",
    "document",
    "paragraph",
    "justified",
    "table",
    "shading",
    "border",
    "fidelity",
    "backend",
    "texture",
    "atlas",
    "tile",
    "cache",
    "device",
    "pixel",
];

/// A deterministic dense document: the worst page a real report produces —
/// small justified text with mixed formatting, a shaded table, colour emoji.
pub fn heavy_docx() -> Vec<u8> {
    let mut body = String::new();
    let mut word = 0usize;
    let mut next_word = || {
        word += 1;
        WORDS[(word * 7 + word / 5) % WORDS.len()]
    };
    for paragraph in 0..48 {
        body.push_str(r#"<w:p><w:pPr><w:jc w:val="both"/><w:spacing w:after="60"/></w:pPr>"#);
        for run in 0..6 {
            let props = match (paragraph + run) % 5 {
                0 => r#"<w:b/>"#,
                1 => r#"<w:i/>"#,
                2 => r#"<w:u w:val="single"/>"#,
                3 => r#"<w:highlight w:val="yellow"/>"#,
                _ => "",
            };
            let text: Vec<&str> = (0..9).map(|_| next_word()).collect();
            body.push_str(&format!(
                r#"<w:r><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/>{props}<w:sz w:val="18"/></w:rPr><w:t xml:space="preserve">{} </w:t></w:r>"#,
                text.join(" ")
            ));
        }
        body.push_str("</w:p>");
        if paragraph == 6 {
            body.push_str(&table_xml());
        }
        if paragraph == 12 {
            body.push_str(
                r#"<w:p><w:r><w:rPr><w:sz w:val="28"/></w:rPr><w:t>😀 🎉 🚀 ✅ 📄 🔥 🌍 💡 😀 🎉 🚀 ✅ 📄 🔥 🌍 💡</w:t></w:r></w:p>"#,
            );
        }
    }
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720" w:header="360" w:footer="360" w:gutter="0"/></w:sectPr></w:body></w:document>"#
    );
    let mut buffer = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut buffer);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, part) in [
        ("[Content_Types].xml", CONTENT_TYPES.to_owned()),
        ("_rels/.rels", ROOT_RELS.to_owned()),
        ("word/document.xml", document),
    ] {
        zip.start_file(name, options).expect("zip entry");
        zip.write_all(part.as_bytes()).expect("zip write");
    }
    zip.finish().expect("zip finish");
    buffer.into_inner()
}

fn table_xml() -> String {
    let border = r#"<w:tblBorders><w:top w:val="single" w:sz="4"/><w:left w:val="single" w:sz="4"/><w:bottom w:val="single" w:sz="4"/><w:right w:val="single" w:sz="4"/><w:insideH w:val="single" w:sz="4"/><w:insideV w:val="single" w:sz="4"/></w:tblBorders>"#;
    let mut xml = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="10800" w:type="dxa"/>{border}</w:tblPr><w:tblGrid>"#
    );
    for _ in 0..6 {
        xml.push_str(r#"<w:gridCol w:w="1800"/>"#);
    }
    xml.push_str("</w:tblGrid>");
    for row in 0..12 {
        xml.push_str("<w:tr>");
        for col in 0..6 {
            let fill = if (row + col) % 2 == 0 {
                "DDEBF7"
            } else {
                "FCE4D6"
            };
            xml.push_str(&format!(
                r#"<w:tc><w:tcPr><w:tcW w:w="1800" w:type="dxa"/><w:shd w:val="clear" w:color="auto" w:fill="{fill}"/></w:tcPr><w:p><w:r><w:rPr><w:sz w:val="16"/></w:rPr><w:t>R{row} C{col} 1,234.56</w:t></w:r></w:p></w:tc>"#
            ));
        }
        xml.push_str("</w:tr>");
    }
    xml.push_str("</w:tbl>");
    xml
}
