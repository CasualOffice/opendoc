//! `w:numId="0"` cancels an inherited list — asserted on the PAINTED page.
//!
//! # The defect this guards
//!
//! ECMA-376 §17.9.18 reserves numbering id `0` for "this paragraph is not
//! numbered". It is not a reference to a list; it is the idiom a style or a
//! paragraph uses to *cancel* a numbering reference inherited from its
//! `w:basedOn` chain. Word's own `TOC Heading` style is the canonical instance:
//! it is based on `Heading 1`, which carries `numId="1"`, and it cancels that
//! list this way.
//!
//! Both import paths resolved `numId` through the numbering table, so `0` — which
//! names no instance — simply missed. The miss was reported as a lost `w:numPr`
//! and the paragraph's own `numbering` was left unset, which in a cascade means
//! "inherit". So the cancellation was read as silence and `Heading 1`'s marker
//! survived onto a heading Word leaves unnumbered.
//!
//! On the owner's NDA this painted a stray `1.` on the **cover page**: the real
//! `TOC Heading` paragraph begins with `<w:br w:type="page"/>`, so the orphaned
//! marker was drawn at the cursor on page 1 while its text went to page 2.
//! LibreOffice paints neither. The fixture below is synthetic and reproduces the
//! construct in a few hundred bytes; the customer file is not in this repository.
//!
//! # Why this asserts the paint, not the model
//!
//! A model-level assertion (`properties.numbering.is_none()`) would have passed
//! throughout, because the paragraph's *own* numbering was always `None` — the
//! marker came from the resolved style chain, and only the composed page knows
//! what was actually drawn. So these read `GlyphRun::is_marker` off the display
//! list, which is the glyph the user sees.
//!
//! [`a_numbered_heading_still_paints_its_marker`] is the control: it fails if the
//! fix over-reaches and suppresses real list markers, and it proves these tests
//! can see a marker at all — without it, "zero markers" would be satisfied by a
//! harness that never finds any.

use casual_doc_import::ImportConfig;
use casual_doc_import::ImportMode;
use casual_doc_import::import_package;
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::display::PaintItem;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_ooxml::DocxPackage;
use casual_doc_ooxml::PackageLimits;
use std::io::Cursor;
use std::io::Write;
use zip::CompressionMethod;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
<Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/>
</Types>"#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

const DOCUMENT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/>
</Relationships>"#;

/// One decimal list, `numId="1"`, whose level 0 marker is `%1.` — the shape
/// `Heading 1` references and `TOC Heading` cancels.
const NUMBERING: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:abstractNum w:abstractNumId="0">
<w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:lvlJc w:val="left"/></w:lvl>
</w:abstractNum>
<w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>
</w:numbering>"#;

/// `Heading1` carries the list; `TOCHeading` is based on it and cancels the list
/// with `w:numId="0"`, exactly as Word's own `TOC Heading` does.
const STYLES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/>
<w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr><w:outlineLvl w:val="0"/></w:pPr>
</w:style>
<w:style w:type="paragraph" w:styleId="TOCHeading"><w:name w:val="TOC Heading"/><w:basedOn w:val="Heading1"/>
<w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="0"/></w:numPr><w:outlineLvl w:val="9"/></w:pPr>
</w:style>
</w:styles>"#;

/// A one-paragraph document body using `style`, optionally with a direct
/// `w:numPr` of its own in the paragraph's `w:pPr`.
fn document_xml(style: &str, direct_num_id: Option<&str>) -> String {
    let direct = match direct_num_id {
        Some(id) => format!(r#"<w:numPr><w:ilvl w:val="0"/><w:numId w:val="{id}"/></w:numPr>"#),
        None => String::new(),
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:pPr><w:pStyle w:val="{style}"/>{direct}</w:pPr><w:r><w:t>Table of Contents</w:t></w:r></w:p>
<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/></w:sectPr>
</w:body></w:document>"#
    )
}

/// Zips the six parts into a `.docx` in memory.
fn package(document: &str) -> Vec<u8> {
    let mut buffer = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut buffer);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, body) in [
        ("[Content_Types].xml", CONTENT_TYPES),
        ("_rels/.rels", ROOT_RELS),
        ("word/_rels/document.xml.rels", DOCUMENT_RELS),
        ("word/document.xml", document),
        ("word/styles.xml", STYLES),
        ("word/numbering.xml", NUMBERING),
    ] {
        zip.start_file(name, options).expect("a fresh zip entry");
        zip.write_all(body.as_bytes()).expect("writing a zip entry");
    }
    zip.finish().expect("finishing the zip");
    buffer.into_inner()
}

/// Imports, paginates and composes `document`, returning how many list-marker
/// glyph runs are PAINTED on page 1.
///
/// Complexity: linear in the page's paint items, over a one-paragraph document.
fn painted_markers(document: &str) -> usize {
    let bytes = package(document);
    let mut docx = DocxPackage::open(&bytes, PackageLimits::default()).expect("a valid package");
    let imported = import_package(
        &mut docx,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .expect("the fixture imports");
    let shaper = ParleyShaper::new();
    let pages = paginate_document(&imported.document, &shaper);
    let page = pages.pages.first().expect("one page");
    compose_page(page)
        .items
        .iter()
        .filter(|item| matches!(item, PaintItem::Glyphs { run } if run.is_marker))
        .count()
}

/// The control. `Heading 1` references `numId="1"`, so its marker must still be
/// drawn — and this proves the harness can see a marker at all.
#[test]
fn a_numbered_heading_still_paints_its_marker() {
    assert_eq!(
        painted_markers(&document_xml("Heading1", None)),
        1,
        "a paragraph whose style references a real list must still paint its marker"
    );
}

/// The guard. `TOC Heading` is based on `Heading 1` and cancels its list with
/// `w:numId="0"`, so nothing may be drawn ahead of the text.
#[test]
fn a_style_cancelling_with_num_id_zero_paints_no_marker() {
    assert_eq!(
        painted_markers(&document_xml("TOCHeading", None)),
        0,
        "w:numId=\"0\" in a style cancels the list inherited from w:basedOn \
         (ECMA-376 §17.9.18); a marker painted here is the inherited one leaking \
         through a cancellation read as silence"
    );
}

/// The same cancellation written directly on the paragraph rather than on its
/// style — the body-parser half of the same rule.
#[test]
fn a_paragraph_cancelling_with_num_id_zero_paints_no_marker() {
    assert_eq!(
        painted_markers(&document_xml("Heading1", Some("0"))),
        0,
        "a direct w:numPr/w:numId=\"0\" cancels the list the paragraph's style \
         contributes"
    );
}
