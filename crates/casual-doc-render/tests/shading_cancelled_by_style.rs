//! `w:shd w:fill="auto"` cancels an inherited fill — asserted on the PAINTED page.
//!
//! # The defect this guards
//!
//! `w:fill="auto"` on a `w:shd` is how Word writes **No Color**. ECMA-376
//! §17.3.5 makes it the automatic (that is, no) background, and it is the idiom
//! a paragraph or a cell uses to *cancel* a fill inherited from its paragraph
//! style, its table style or the table itself. The import mapped it to
//! `fill: None` — the same value an absent `w:shd` produces — so the
//! cancellation was indistinguishable from silence, the cascade's
//! `if !over.shading.is_empty()` never fired, and the table path went further
//! and *fell through* a cancelled cell to the table style's banded fill.
//!
//! It is `w:numId="0"` and the `CT_OnOff` toggles again, in a third property.
//!
//! # Measured against a reference, not reasoned about
//!
//! LibreOffice was given a fixture whose style fills yellow and whose second
//! paragraph carries `<w:shd w:val="clear" w:color="auto" w:fill="auto"/>`
//! (`soffice --headless --convert-to pdf`, rasterised at 72dpi). Exactly **one**
//! yellow band was painted, at the first paragraph — the cancellation is
//! honoured. The same run confirmed that `<w:pBdr><w:top w:val="none"/>…`
//! cancels an inherited paragraph border, which this engine already got right
//! (`flow::single_edge` rejects `none`/`nil`), so no change was needed there;
//! the control below pins that.
//!
//! # Why this asserts the paint
//!
//! The cancelled paragraph's own `shading.fill` is legitimately `None` either
//! way. Only the composed page knows whether a fill rectangle was drawn, so
//! these count `PaintItem::Rect`s of the style's colour.

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
</Types>"#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

const DOCUMENT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>"#;

/// A paragraph style filling yellow, a paragraph style with a thick red top and
/// bottom border, and a table style filling every cell yellow.
const STYLES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:style w:type="paragraph" w:styleId="Shaded"><w:name w:val="Shaded"/>
<w:pPr><w:shd w:val="clear" w:color="auto" w:fill="FFFF00"/></w:pPr>
</w:style>
<w:style w:type="paragraph" w:styleId="Bordered"><w:name w:val="Bordered"/>
<w:pPr><w:pBdr><w:top w:val="single" w:sz="24" w:color="FF0000"/><w:bottom w:val="single" w:sz="24" w:color="FF0000"/></w:pBdr></w:pPr>
</w:style>
<w:style w:type="table" w:styleId="ShadedTable"><w:name w:val="Shaded Table"/>
<w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="FFFF00"/></w:tcPr>
</w:style>
<w:style w:type="character" w:styleId="ShadedRun"><w:name w:val="Shaded Run"/>
<w:rPr><w:shd w:val="clear" w:color="auto" w:fill="00FFFF"/><w:bdr w:val="single" w:sz="8" w:color="FF0000"/></w:rPr>
</w:style>
</w:styles>"#;

const SECTION: &str = r#"<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/></w:sectPr>"#;

/// The style's fill, as the display list carries it (opaque RGBA).
const YELLOW: [u8; 4] = [0xFF, 0xFF, 0x00, 255];
/// The bordered style's line colour.
const RED: [u8; 4] = [0xFF, 0x00, 0x00, 255];

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
    ] {
        zip.start_file(name, options).expect("a fresh zip entry");
        zip.write_all(body.as_bytes()).expect("writing a zip entry");
    }
    zip.finish().expect("finishing the zip");
    buffer.into_inner()
}

fn document_xml(body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}{SECTION}</w:body></w:document>"#
    )
}

/// How many rectangles of `colour` are painted on page 1 — filled or stroked, so
/// one helper answers both "is the background there" and "is the border there".
///
/// Complexity: linear in the page's paint items, over a handful of paragraphs.
fn painted_rects(document: &str, colour: [u8; 4]) -> usize {
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
        .filter(|item| match item {
            PaintItem::Rect { fill, stroke, .. } => {
                fill.map(|c| [c.r, c.g, c.b, c.a]) == Some(colour)
                    || stroke
                        .as_ref()
                        .map(|s| [s.color.r, s.color.g, s.color.b, s.color.a])
                        == Some(colour)
            }
            _ => false,
        })
        .count()
}

/// One paragraph in `Shaded`, carrying `extra` in its `w:pPr`.
fn shaded(extra: &str) -> String {
    document_xml(&format!(
        r#"<w:p><w:pPr><w:pStyle w:val="Shaded"/>{extra}</w:pPr><w:r><w:t>Body</w:t></w:r></w:p>"#
    ))
}

// ---------------------------------------------------------------------------
// Paragraph shading
// ---------------------------------------------------------------------------

/// The control: the style's fill is still painted when nothing cancels it.
#[test]
fn a_style_fill_is_painted() {
    assert_eq!(
        painted_rects(&shaded(""), YELLOW),
        1,
        "the paragraph style's w:shd fill must reach the page"
    );
}

/// The guard: `w:fill="auto"` on the paragraph cancels it.
#[test]
fn a_paragraph_fill_of_auto_cancels_the_style_fill() {
    assert_eq!(
        painted_rects(
            &shaded(r#"<w:shd w:val="clear" w:color="auto" w:fill="auto"/>"#),
            YELLOW
        ),
        0,
        "w:fill=\"auto\" is Word's No Color and cancels the inherited fill — \
         LibreOffice paints one yellow band on this fixture, not two. A \
         rectangle here is the style's fill leaking through a cancellation read \
         as silence"
    );
}

/// `w:val="nil"` — the other spelling of "no shading" — cancels the same way.
#[test]
fn a_paragraph_shading_of_nil_cancels_the_style_fill() {
    assert_eq!(
        painted_rects(&shaded(r#"<w:shd w:val="nil"/>"#), YELLOW),
        0,
        "a nil pattern with no fill paints nothing and cancels the inherited fill"
    );
}

// ---------------------------------------------------------------------------
// Table-cell shading — the fall-through case
// ---------------------------------------------------------------------------

/// A one-cell table in `ShadedTable`, the cell carrying `extra` in its `w:tcPr`.
fn table(extra: &str) -> String {
    document_xml(&format!(
        r#"<w:tbl><w:tblPr><w:tblStyle w:val="ShadedTable"/><w:tblW w:w="5000" w:type="dxa"/></w:tblPr>
<w:tblGrid><w:gridCol w:w="5000"/></w:tblGrid>
<w:tr><w:tc><w:tcPr><w:tcW w:w="5000" w:type="dxa"/>{extra}</w:tcPr><w:p><w:r><w:t>Cell</w:t></w:r></w:p></w:tc></w:tr>
</w:tbl><w:p/>"#
    ))
}

/// The control: the table style's cell fill is painted.
#[test]
fn a_table_style_cell_fill_is_painted() {
    assert_eq!(
        painted_rects(&table(""), YELLOW),
        1,
        "the table style's w:tcPr/w:shd must reach the cell"
    );
}

/// The guard, and the worse half of the defect: the cell cancels the fill, so
/// the resolver must not fall through the cancellation to the table style.
#[test]
fn a_cell_fill_of_auto_cancels_the_table_style_fill() {
    assert_eq!(
        painted_rects(
            &table(r#"<w:shd w:val="clear" w:color="auto" w:fill="auto"/>"#),
            YELLOW
        ),
        0,
        "a cell set to No Color must not keep painting its table style's fill; \
         the cell/style/table fallback chain has to stop at an explicit \
         cancellation rather than read it as an absent w:shd"
    );
}

// ---------------------------------------------------------------------------
// Character-style shading and run border — the other half of the style gap
// ---------------------------------------------------------------------------

/// The style's run fill, as the display list carries it.
const CYAN: [u8; 4] = [0x00, 0xFF, 0xFF, 255];

/// A run carrying the `ShadedRun` character style.
fn styled_run() -> String {
    document_xml(
        r#"<w:p><w:r><w:rPr><w:rStyle w:val="ShadedRun"/></w:rPr><w:t>Boxed</w:t></w:r></w:p>"#,
    )
}

/// `w:rPr > w:shd` in a CHARACTER style was dropped for the same reason the
/// paragraph one was: its value lives in `@w:fill`, not `@w:val`, so the flat
/// `apply_run_property` had no arm and the styles reader never looked.
#[test]
fn a_character_style_fill_is_painted() {
    assert_eq!(
        painted_rects(&styled_run(), CYAN),
        1,
        "a character style's w:shd fill must reach the run it is applied to"
    );
}

/// And its sibling in the same element: `w:rPr > w:bdr`, the boxed-run border.
#[test]
fn a_character_style_run_border_is_painted() {
    assert!(
        painted_rects(&styled_run(), RED) > 0,
        "a character style's w:bdr must reach the run it is applied to"
    );
}

// ---------------------------------------------------------------------------
// Paragraph borders — the sibling that was already correct
// ---------------------------------------------------------------------------

/// One paragraph in `Bordered`, carrying `extra` in its `w:pPr`.
fn bordered(extra: &str) -> String {
    document_xml(&format!(
        r#"<w:p><w:pPr><w:pStyle w:val="Bordered"/>{extra}</w:pPr><w:r><w:t>Body</w:t></w:r></w:p>"#
    ))
}

/// The control: the style's two red edges are painted.
#[test]
fn a_style_border_is_painted() {
    assert_eq!(
        painted_rects(&bordered(""), RED),
        2,
        "the paragraph style's top and bottom w:pBdr edges must reach the page"
    );
}

/// The sibling guard. This class was already handled — `flow::single_edge`
/// rejects an edge whose style token is `none`/`nil`, and the cascade replaces
/// the whole border set when the paragraph declares one — but nothing pinned it,
/// and LibreOffice agrees on the same fixture, so it is pinned here beside the
/// two classes that were broken.
#[test]
fn a_paragraph_border_of_none_cancels_the_style_border() {
    assert_eq!(
        painted_rects(
            &bordered(
                r#"<w:pBdr><w:top w:val="none" w:sz="0" w:color="auto"/><w:bottom w:val="none" w:sz="0" w:color="auto"/></w:pBdr>"#
            ),
            RED
        ),
        0,
        "w:val=\"none\" on an edge cancels the border the style contributes"
    );
}
