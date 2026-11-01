//! `w14:textFill/w14:noFill` (laid-out text painted with no fill) survives a
//! save and a reopen (`docs/182` §20b).
//!
//! An image-text line box over a scan carries its recognised text this way
//! until the line is edited: the page shows the scan's own pixels, the text
//! is there to select, search and edit. A writer that drops the element turns
//! every unedited line into visible substitute-face text drawn over the
//! scan's own glyphs, the doubling the design exists to prevent; an importer
//! that ignores it does the same on the next open. So the assertion is on the
//! REOPENED model, and a solid glyph fill (not modelled) stays reported
//! rather than being read as "no fill".

use casual_doc_export::export_document;
use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_model::v1::{BlockNode, Document, InlineNode, RunProperties};
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
</Types>"#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

/// Three runs: one unpainted (as Word 2010 writes it), one with a SOLID glyph
/// fill (not modelled: must not read as unpainted), one plain.
const DOCUMENT: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" mc:Ignorable="w14"><w:body>
<w:p>
<w:r><w:rPr><w:color w:val="000000"/><w14:textFill><w14:noFill/></w14:textFill></w:rPr><w:t>unpainted</w:t></w:r>
<w:r><w:rPr><w14:textFill><w14:solidFill><w14:srgbClr w14:val="FF0000"/></w14:solidFill></w14:textFill></w:rPr><w:t>solid</w:t></w:r>
<w:r><w:rPr><w:b/></w:rPr><w:t>plain</w:t></w:r>
</w:p>
<w:sectPr><w:pgSz w:w="12240" w:h="15840"/></w:sectPr>
</w:body></w:document>"#;

fn package() -> Vec<u8> {
    let mut buffer = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut buffer);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, body) in [
        ("[Content_Types].xml", CONTENT_TYPES),
        ("_rels/.rels", ROOT_RELS),
        ("word/document.xml", DOCUMENT),
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

/// Each run's text and properties, in order.
fn runs(document: &Document) -> Vec<(String, RunProperties)> {
    let BlockNode::Paragraph(paragraph) = document.body().first().expect("one block") else {
        panic!("a paragraph");
    };
    paragraph
        .inlines
        .iter()
        .filter_map(|inline| match inline {
            InlineNode::Run(run) => Some((run.text.clone(), run.properties.get().clone())),
            _ => None,
        })
        .collect()
}

fn assert_fills(document: &Document, when: &str) {
    let runs = runs(document);
    let fill = |text: &str| {
        runs.iter()
            .find(|(t, _)| t == text)
            .unwrap_or_else(|| panic!("the {text} run {when}"))
            .1
            .no_text_fill
    };
    assert_eq!(fill("unpainted"), Some(true), "noFill is unpainted {when}");
    assert_eq!(
        fill("solid"),
        None,
        "a solid glyph fill is not \"no fill\" {when}"
    );
    assert_eq!(fill("plain"), None, "{when}");
}

#[test]
fn unpainted_text_survives_a_save_and_reopen() {
    let original = open(&package());
    assert_fills(&original, "on import");
    let saved = export_document(&original, &BTreeMap::new()).expect("the document exports");
    let reopened = open(&saved.bytes);
    assert_fills(&reopened, "after a save and a reopen");
}
