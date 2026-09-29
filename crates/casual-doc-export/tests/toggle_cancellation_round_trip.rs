//! An explicit `w:val="0"` survives a save, so the cancelled flag does not come
//! back when the document is opened again.
//!
//! The import and cascade halves of this are guarded on the painted page
//! (`casual-doc-render/tests/toggles_cancelled_by_style.rs`). This is the third
//! half, and it is the one that silently rewrites the user's document: if the
//! writer emits only the "on" case of a `CT_OnOff` toggle, every cancellation
//! the document made is dropped on save, the style's value is inherited again on
//! the next open, and the paint changes without anybody touching the file. It is
//! the same round-trip obligation `w:numId="0"` has.
//!
//! The assertion is on the REOPENED model rather than on a substring of the
//! written XML: a writer that emits the attribute and an importer that ignores
//! it would pass a substring assertion and still lose the user's intent.

use casual_doc_export::export_document;
use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_model::v1::{BlockNode, Document, ParagraphProperties};
use casual_doc_ooxml::{DocxPackage, PackageLimits};
use std::collections::BTreeMap;
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
</Types>"#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

const DOCUMENT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>"#;

/// One style turning on all five toggles, so the paragraph below has something
/// real to cancel.
const STYLES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:style w:type="paragraph" w:styleId="Flagged"><w:name w:val="Flagged"/>
<w:pPr><w:keepNext/><w:keepLines/><w:pageBreakBefore/><w:contextualSpacing/><w:suppressLineNumbers/></w:pPr>
</w:style>
</w:styles>"#;

/// A single paragraph that cancels all five, each with a different spelling of
/// "off" (`0`, `false`, `off`) so the writer cannot be shown correct by one
/// token alone.
const DOCUMENT: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:pPr><w:pStyle w:val="Flagged"/>
<w:keepNext w:val="0"/><w:keepLines w:val="false"/><w:pageBreakBefore w:val="off"/>
<w:contextualSpacing w:val="0"/><w:suppressLineNumbers w:val="false"/>
</w:pPr><w:r><w:t>Cancelled</w:t></w:r></w:p>
<w:sectPr><w:pgSz w:w="12240" w:h="15840"/></w:sectPr>
</w:body></w:document>"#;

fn package() -> Vec<u8> {
    let mut buffer = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut buffer);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, body) in [
        ("[Content_Types].xml", CONTENT_TYPES),
        ("_rels/.rels", ROOT_RELS),
        ("word/_rels/document.xml.rels", DOCUMENT_RELS),
        ("word/document.xml", DOCUMENT),
        ("word/styles.xml", STYLES),
    ] {
        zip.start_file(name, options).expect("a fresh zip entry");
        zip.write_all(body.as_bytes()).expect("writing a zip entry");
    }
    zip.finish().expect("finishing the zip");
    buffer.into_inner()
}

fn open(bytes: &[u8]) -> Document {
    let mut docx = DocxPackage::open(bytes, PackageLimits::default()).expect("the package opens");
    import_package(
        &mut docx,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .expect("the fixture imports")
    .document
}

/// The first body paragraph's direct properties.
fn first_paragraph(document: &Document) -> ParagraphProperties {
    match document.body().first().expect("one block") {
        BlockNode::Paragraph(paragraph) => paragraph.properties.get().clone(),
        other => panic!("expected a paragraph, got {other:?}"),
    }
}

/// Every one of the five must read `Some(false)`: stated, and stated as off.
fn assert_all_cancelled(properties: &ParagraphProperties, when: &str) {
    for (value, name) in [
        (properties.keep_next, "w:keepNext"),
        (properties.keep_lines, "w:keepLines"),
        (properties.page_break_before, "w:pageBreakBefore"),
        (properties.contextual_spacing, "w:contextualSpacing"),
        (properties.suppress_line_numbers, "w:suppressLineNumbers"),
    ] {
        assert_eq!(
            value,
            Some(false),
            "{name} must be an explicit OFF {when}; `None` there means \
             \"absent, inherit\", which hands the paragraph the style's flag back"
        );
    }
}

#[test]
fn an_explicit_off_survives_a_save_and_reopen() {
    let original = open(&package());
    assert_all_cancelled(&first_paragraph(&original), "on import");

    let saved = export_document(&original, &BTreeMap::new()).expect("the document exports");
    let reopened = open(&saved.bytes);
    assert_all_cancelled(
        &first_paragraph(&reopened),
        "after a save and a reopen — a writer that emits only the \"on\" case \
         drops every cancellation and lets the style's value return",
    );
}
