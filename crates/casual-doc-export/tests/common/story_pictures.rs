// SPDX-License-Identifier: Apache-2.0

//! A package with a picture on every surface that writes a story.
//!
//! Built in the test rather than committed under `fixtures/`: it is a probe for one
//! writer property (every story root declares the prefixes the shared body writer can
//! emit), not a corpus document, and a committed binary would need a manifest entry
//! and a generator for what is a few kilobytes of literal XML.

use std::collections::BTreeMap;
use std::io::{Cursor, Write};

use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_ooxml::{DocxPackage, PackageLimits};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const WPS: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingShape";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// The media part every picture in the package points at.
pub const LOGO_PART: &str = "word/media/logo.png";

/// A valid 1x1 PNG, so the package carries a real image rather than a stand-in.
pub const LOGO_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

/// The parts of the package that hold a story: `(part name, root element)`.
pub const STORY_PARTS: [(&str, &str); 6] = [
    ("word/document.xml", "w:document"),
    ("word/header1.xml", "w:hdr"),
    ("word/footer1.xml", "w:ftr"),
    ("word/footnotes.xml", "w:footnotes"),
    ("word/endnotes.xml", "w:endnotes"),
    ("word/comments.xml", "w:comments"),
];

/// One DrawingML picture run, as Word writes it: `wp` on the part root, `a` and `pic`
/// declared on the element that uses them. `anchor` picks `wp:anchor` over
/// `wp:inline`; `id` keeps `wp:docPr/@id` distinct inside a part.
fn picture_run(id: u32, anchor: bool) -> String {
    let graphic = format!(
        r#"<wp:docPr id="{id}" name="Logo {id}"/><a:graphic xmlns:a="{A}"><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:nvPicPr><pic:cNvPr id="0" name="logo.png"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rIdImg"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="685800"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic>"#
    );
    if anchor {
        format!(
            r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="114300" distR="114300" simplePos="0" relativeHeight="2" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="914400" cy="685800"/><wp:wrapNone/>{graphic}</wp:anchor></w:drawing></w:r>"#
        )
    } else {
        format!(
            r#"<w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="914400" cy="685800"/>{graphic}</wp:inline></w:drawing></w:r>"#
        )
    }
}

/// A floating DrawingML text box (`wps:wsp` carrying `wps:txbx`).
fn text_box_run(id: u32) -> String {
    format!(
        r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="3" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="1828800" cy="457200"/><wp:wrapNone/><wp:docPr id="{id}" name="Box {id}"/><a:graphic xmlns:a="{A}"><a:graphicData uri="{WPS}"><wps:wsp><wps:cNvSpPr txBox="1"/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1828800" cy="457200"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>text box in the header</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#
    )
}

/// A `Stored` ZIP of `(name, bytes)` entries.
fn stored_zip(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, bytes) in entries {
        writer.start_file(name.as_str(), options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

/// A source `.docx` with a picture on every surface that writes a story: an inline
/// and an anchored picture and a text box in the header, an inline and an anchored
/// picture in the footer, and an inline picture in a footnote, an endnote and a
/// comment. Every part points at [`LOGO_PART`] through its own relationships.
///
/// The source parts declare only `w`, `r`, `wp` and `wps` on the root, the way a
/// producer declares what it uses; the exporter must not depend on that.
pub fn story_pictures_source() -> Vec<u8> {
    let ns = format!(r#"xmlns:w="{W}" xmlns:r="{R}" xmlns:wp="{WP}" xmlns:wps="{WPS}""#);
    let prolog = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#;
    let header = format!(
        r#"{prolog}<w:hdr {ns}><w:p>{}{}{}</w:p></w:hdr>"#,
        picture_run(1, false),
        picture_run(2, true),
        text_box_run(3)
    );
    let footer = format!(
        r#"{prolog}<w:ftr {ns}><w:p>{}{}</w:p></w:ftr>"#,
        picture_run(1, false),
        picture_run(2, true)
    );
    let footnotes = format!(
        r#"{prolog}<w:footnotes {ns}><w:footnote w:id="1"><w:p>{}</w:p></w:footnote></w:footnotes>"#,
        picture_run(1, false)
    );
    let endnotes = format!(
        r#"{prolog}<w:endnotes {ns}><w:endnote w:id="1"><w:p>{}</w:p></w:endnote></w:endnotes>"#,
        picture_run(1, false)
    );
    let comments = format!(
        r#"{prolog}<w:comments {ns}><w:comment w:id="0" w:author="A"><w:p>{}</w:p></w:comment></w:comments>"#,
        picture_run(1, false)
    );
    let document = format!(
        r#"{prolog}<w:document {ns}><w:body><w:p><w:commentRangeStart w:id="0"/><w:r><w:t>body</w:t></w:r><w:r><w:footnoteReference w:id="1"/></w:r><w:r><w:endnoteReference w:id="1"/></w:r><w:commentRangeEnd w:id="0"/><w:r><w:commentReference w:id="0"/></w:r></w:p><w:sectPr><w:headerReference w:type="default" r:id="rIdH"/><w:footerReference w:type="default" r:id="rIdF"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:bottom="1440" w:left="1440" w:right="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#
    );
    let content_types = format!(
        r#"{prolog}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/><Override PartName="/word/footer1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/><Override PartName="/word/footnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"/><Override PartName="/word/endnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml"/><Override PartName="/word/comments.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml"/></Types>"#
    );
    let root_rels = format!(
        r#"{prolog}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/officeDocument" Target="word/document.xml"/></Relationships>"#
    );
    let document_rels = format!(
        r#"{prolog}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdH" Type="{REL}/header" Target="header1.xml"/><Relationship Id="rIdF" Type="{REL}/footer" Target="footer1.xml"/><Relationship Id="rIdFn" Type="{REL}/footnotes" Target="footnotes.xml"/><Relationship Id="rIdEn" Type="{REL}/endnotes" Target="endnotes.xml"/><Relationship Id="rIdCm" Type="{REL}/comments" Target="comments.xml"/></Relationships>"#
    );
    let image_rels = format!(
        r#"{prolog}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdImg" Type="{REL}/image" Target="media/logo.png"/></Relationships>"#
    );
    let mut entries: Vec<(String, Vec<u8>)> = vec![
        ("[Content_Types].xml".into(), content_types.into_bytes()),
        ("_rels/.rels".into(), root_rels.into_bytes()),
        ("word/document.xml".into(), document.into_bytes()),
        (
            "word/_rels/document.xml.rels".into(),
            document_rels.into_bytes(),
        ),
        ("word/header1.xml".into(), header.into_bytes()),
        ("word/footer1.xml".into(), footer.into_bytes()),
        ("word/footnotes.xml".into(), footnotes.into_bytes()),
        ("word/endnotes.xml".into(), endnotes.into_bytes()),
        ("word/comments.xml".into(), comments.into_bytes()),
        (LOGO_PART.into(), LOGO_PNG.to_vec()),
    ];
    for part in ["header1", "footer1", "footnotes", "endnotes", "comments"] {
        entries.push((
            format!("word/_rels/{part}.xml.rels"),
            image_rels.clone().into_bytes(),
        ));
    }
    stored_zip(&entries)
}

/// [`story_pictures_source`] imported semantically and written back: the package the
/// exporter produces, which is what the guards read.
pub fn write_story_pictures_package() -> Vec<u8> {
    let source = story_pictures_source();
    let mut package =
        DocxPackage::open(&source, PackageLimits::default()).expect("the package is admitted");
    let import = import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .expect("the package imports");
    let media = BTreeMap::from([(LOGO_PART.to_owned(), LOGO_PNG.to_vec())]);
    casual_doc_export::export_document(&import.document, &media)
        .expect("the model is writable")
        .bytes
}
