//! A table style's cell margins and indent reach the page, and the styles part a
//! Google Docs export writes stops reporting losses it does not have
//! (`docs/109` HF-401..HF-404).
//!
//! # What was wrong
//!
//! An owner's Google Docs export (kept local, never committed) reported
//! `style` ×32, `tblCellMar` ×20 and `tblInd` ×1 "kept in the file, not shown
//! or editable". Three separate defects produced them:
//!
//! - **HF-401** Google writes `Normal`, `Heading1`…`Heading6`, `Title`,
//!   `Subtitle` and `TableNormal` up to four times. Every later copy was
//!   dropped and reported, and the last `TableNormal` copy was the only one that
//!   carried the cell margins.
//! - **HF-402** it writes twips with a decimal tail (`w:w="108.0"`), which a
//!   strict integer parse refused: 5 table styles × 4 margin edges = the 20
//!   `tblCellMar` findings, and the one `tblInd`. And even a table style whose
//!   margins and indent *did* parse never reached layout: nothing cascaded a
//!   table style's `w:tblPr` into the table, so every styled table was laid out
//!   with Word's built-in margins and no indent.
//! - **HF-403** a `w:style` with no `@w:type` (a paragraph style, by ECMA-376
//!   §17.7.4.17) was read as "no kind" and reported lost.
//! - **HF-404** a fractional paragraph indent (`w:right="49.27"`) was dropped
//!   with no finding at all.
//!
//! # Why this paginates
//!
//! "The style's margins are in the model" was true of the integer case before
//! the fix, and the page still ignored them. So the geometry guards lay the
//! document out through the real paginator and read the cell fragments: the
//! table's x offset and each cell's content insets.

use std::io::{Cursor, Write};

use casual_doc_import::{Import, ImportConfig, ImportMode, import_package};
use casual_doc_layout::block::{BlockFragment, CellFragment};
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::units::Twip;
use casual_doc_model::v1::{BlockNode, Style, StyleKind};
use casual_doc_ooxml::{DocxPackage, PackageLimits};

const CONTENT_TYPES: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#;
const ROOT_RELS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
const DOCUMENT_RELS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;

/// The styles part, in the shapes the owner's file uses:
///
/// - `TableNormal` twice: a bare copy first, then the copy with the margins,
///   written with Google's decimal tail;
/// - `Padded`, a table style with an indent, all four cell margins and a
///   `firstRow` conditional fill;
/// - `Heading1` twice, the later copy adding `w:outlineLvl` and `w:qFormat`
///   (and the thin copy carrying a `w:lineRule` with no `w:line`, as Google's
///   do, which means nothing and so disagrees with nothing);
/// - `Quote`, a paragraph style with no `@w:type`;
/// - `Clash` twice, the two copies disagreeing on the font size;
/// - `Union` twice, each copy adding a different property.
const STYLES: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:styleId="Normal" w:default="1"><w:name w:val="normal"/></w:style>
  <w:style w:type="table" w:styleId="TableNormal" w:default="1"><w:name w:val="Normal Table"/></w:style>
  <w:style w:type="paragraph" w:styleId="Normal" w:default="1"><w:name w:val="Normal"/><w:qFormat w:val="1"/></w:style>
  <w:style w:type="table" w:styleId="TableNormal" w:default="1"><w:name w:val="Normal Table"/><w:uiPriority w:val="99"/><w:semiHidden w:val="1"/><w:tblPr><w:tblInd w:w="0.0" w:type="dxa"/><w:tblCellMar><w:top w:w="0.0" w:type="dxa"/><w:left w:w="108.0" w:type="dxa"/><w:bottom w:w="0.0" w:type="dxa"/><w:right w:w="108.0" w:type="dxa"/></w:tblCellMar></w:tblPr></w:style>
  <w:style w:type="table" w:styleId="Padded">
    <w:name w:val="Padded"/>
    <w:basedOn w:val="TableNormal"/>
    <w:tblPr>
      <w:tblInd w:w="360.0" w:type="dxa"/>
      <w:tblCellMar><w:top w:w="144.0" w:type="dxa"/><w:left w:w="216" w:type="dxa"/><w:bottom w:w="72.0" w:type="dxa"/><w:right w:w="230.4" w:type="dxa"/></w:tblCellMar>
    </w:tblPr>
    <w:tblStylePr w:type="firstRow"><w:rPr><w:b/></w:rPr><w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="1F4E79"/></w:tcPr></w:tblStylePr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:before="600" w:lineRule="auto"/></w:pPr></w:style>
  <w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:qFormat w:val="1"/><w:pPr><w:spacing w:before="600"/><w:outlineLvl w:val="0"/></w:pPr></w:style>
  <w:style w:styleId="Quote"><w:name w:val="Quote"/><w:pPr><w:ind w:left="720.0" w:right="49.26666666666695"/></w:pPr></w:style>
  <w:style w:type="character" w:styleId="Clash"><w:name w:val="Clash"/><w:rPr><w:sz w:val="20"/></w:rPr></w:style>
  <w:style w:type="character" w:styleId="Clash"><w:name w:val="Clash"/><w:rPr><w:sz w:val="28"/></w:rPr></w:style>
  <w:style w:type="character" w:styleId="Union"><w:name w:val="Union"/><w:rPr><w:b/></w:rPr></w:style>
  <w:style w:type="character" w:styleId="Union"><w:name w:val="Union"/><w:rPr><w:i/></w:rPr></w:style>
</w:styles>"#;

/// Three tables: one on `Padded`; one on `Padded` with its own indent and left
/// margin (direct formatting wins); one naming no style (the default table
/// style applies).
const DOCUMENT: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
  <w:tbl>
    <w:tblPr><w:tblStyle w:val="Padded"/><w:tblW w:w="4000" w:type="dxa"/><w:tblLook w:firstRow="1" w:lastRow="0" w:firstColumn="0" w:lastColumn="0" w:noHBand="1" w:noVBand="1"/></w:tblPr>
    <w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid>
    <w:tr><w:tc><w:p><w:r><w:t>Head A</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Head B</w:t></w:r></w:p></w:tc></w:tr>
    <w:tr><w:tc><w:p><w:r><w:t>Body A</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Body B</w:t></w:r></w:p></w:tc></w:tr>
  </w:tbl>
  <w:p><w:pPr><w:pStyle w:val="Quote"/></w:pPr><w:r><w:t>Between</w:t></w:r></w:p>
  <w:tbl>
    <w:tblPr><w:tblStyle w:val="Padded"/><w:tblW w:w="4000" w:type="dxa"/><w:tblInd w:w="0" w:type="dxa"/><w:tblCellMar><w:left w:w="50" w:type="dxa"/></w:tblCellMar><w:tblLook w:firstRow="0" w:lastRow="0" w:firstColumn="0" w:lastColumn="0" w:noHBand="1" w:noVBand="1"/></w:tblPr>
    <w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid>
    <w:tr><w:tc><w:p><w:r><w:t>Direct</w:t></w:r></w:p></w:tc></w:tr>
  </w:tbl>
  <w:p><w:r><w:t>Between again</w:t></w:r></w:p>
  <w:tbl>
    <w:tblPr><w:tblW w:w="4000" w:type="dxa"/></w:tblPr>
    <w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid>
    <w:tr><w:tc><w:p><w:r><w:t>Unstyled</w:t></w:r></w:p></w:tc></w:tr>
  </w:tbl>
  <w:p/>
</w:body></w:document>"#;

fn zip_package(parts: &[(&str, &[u8])]) -> Vec<u8> {
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in parts {
        writer
            .start_file(
                *name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

/// A Google Docs body table: its own indent and every cell margin written with
/// the decimal tail (the shape of a Google Docs quote template).
const DIRECT_DECIMAL_DOCUMENT: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
  <w:tbl>
    <w:tblPr><w:tblW w:w="4000.0" w:type="dxa"/><w:tblInd w:w="-135.0" w:type="dxa"/></w:tblPr>
    <w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid>
    <w:tr><w:tc><w:tcPr><w:tcMar><w:top w:w="100.0" w:type="dxa"/><w:left w:w="100.0" w:type="dxa"/><w:bottom w:w="100.0" w:type="dxa"/><w:right w:w="100.0" w:type="dxa"/></w:tcMar></w:tcPr><w:p><w:r><w:t>Item</w:t></w:r></w:p></w:tc></w:tr>
  </w:tbl>
  <w:p/>
</w:body></w:document>"#;

fn package_with(document: &[u8]) -> Vec<u8> {
    zip_package(&[
        ("[Content_Types].xml", CONTENT_TYPES),
        ("_rels/.rels", ROOT_RELS),
        ("word/document.xml", document),
        ("word/_rels/document.xml.rels", DOCUMENT_RELS),
        ("word/styles.xml", STYLES),
    ])
}

fn import() -> Import {
    import_document(DOCUMENT)
}

fn import_document(document: &[u8]) -> Import {
    let bytes = package_with(document);
    let mut package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();
    import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Retention,
            ..ImportConfig::default()
        },
    )
    .unwrap()
}

fn style<'a>(import: &'a Import, name: &str) -> &'a Style {
    import
        .document
        .definitions()
        .styles
        .iter()
        .map(|(_, style)| style)
        .find(|style| {
            style
                .name
                .as_deref()
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(name))
        })
        .unwrap_or_else(|| panic!("style {name} imported"))
}

/// `(feature, occurrences)` for every finding the report carries.
fn findings(import: &Import) -> Vec<(String, u32)> {
    import
        .report
        .entries
        .iter()
        .map(|entry| (entry.feature.clone(), entry.occurrences))
        .collect()
}

#[test]
fn the_google_docs_styles_part_reports_only_the_one_real_disagreement() {
    let import = import();
    let findings = findings(&import);
    // `Clash` is the only genuine loss: two definitions setting one property to
    // two values. Everything else in the part is now applied.
    assert_eq!(
        findings
            .iter()
            .filter(
                |(feature, _)| matches!(feature.as_str(), "style" | "tblCellMar" | "tblInd")
                    || feature.starts_with("docx.style.")
            )
            .cloned()
            .collect::<Vec<_>>(),
        vec![("docx.style.duplicate.character".to_owned(), 1)],
        "full report: {findings:?}"
    );
}

#[test]
fn a_repeated_style_definition_continues_the_first_instead_of_being_dropped() {
    let import = import();
    let styles = &import.document.definitions().styles;
    let headings = styles
        .iter()
        .filter(|(_, style)| style.name.as_deref() == Some("heading 1"))
        .count();
    assert_eq!(headings, 1, "one Heading1, not one per copy");

    let heading = style(&import, "heading 1");
    let paragraph = heading.paragraph.as_ref().expect("heading pPr");
    assert_eq!(
        paragraph.outline_level,
        Some(0),
        "the later copy's outline level"
    );
    assert_eq!(
        paragraph.spacing.as_ref().and_then(|s| s.before_twips),
        Some(600),
        "both copies' spacing, agreed"
    );
    assert!(heading.q_format, "the later copy's qFormat");

    // The default table style's margins lived only in its LAST copy.
    let table_normal = style(&import, "Normal Table");
    let margins = table_normal.table.as_ref().expect("tblPr").cell_margins;
    assert_eq!(margins.start_twips, Some(108));
    assert_eq!(margins.end_twips, Some(108));

    // A disagreement keeps the first definition's value.
    let clash = style(&import, "Clash");
    assert_eq!(
        clash.run.as_ref().and_then(|run| run.size_half_points),
        Some(20)
    );

    // Two copies that each add something are their union.
    let union = style(&import, "Union").run.as_ref().expect("rPr");
    assert_eq!(union.bold, Some(true), "the first copy's bold");
    assert_eq!(union.italic, Some(true), "the later copy's italic");
}

#[test]
fn a_style_with_no_type_is_a_paragraph_style_and_a_fractional_indent_is_read() {
    let import = import();
    let quote = style(&import, "Quote");
    assert_eq!(quote.kind, StyleKind::Paragraph);
    let indentation = quote
        .paragraph
        .as_ref()
        .and_then(|paragraph| paragraph.indentation)
        .expect("indentation");
    assert_eq!(indentation.start_twips, Some(720));
    assert_eq!(indentation.end_twips, Some(49), "49.27 twips rounds to 49");
    // And the body paragraph naming it resolves to it.
    let named =
        import.document.body().iter().any(
            |block| matches!(block, BlockNode::Paragraph(p) if p.properties.style_ref.is_some()),
        );
    assert!(named, "the untyped style resolves for pStyle");
}

#[test]
fn decimal_twips_in_a_table_style_are_modelled() {
    let import = import();
    let padded = style(&import, "Padded");
    let table = padded.table.as_ref().expect("tblPr");
    assert_eq!(table.indent_twips, Some(360));
    assert_eq!(table.cell_margins.top_twips, Some(144));
    assert_eq!(table.cell_margins.start_twips, Some(216));
    assert_eq!(table.cell_margins.bottom_twips, Some(72));
    assert_eq!(
        table.cell_margins.end_twips,
        Some(230),
        "230.4 rounds to 230"
    );
}

/// Every table row's cells on every page, in document order.
fn laid_out_rows() -> Vec<Vec<CellFragment>> {
    let import = import();
    let shaper = ParleyShaper::new();
    let layout = paginate_document(&import.document, &shaper);
    let mut rows: Vec<Vec<CellFragment>> = Vec::new();
    for page in &layout.pages {
        for placed in &page.placed {
            if let BlockFragment::TableRow { cells, .. } = &placed.fragment {
                rows.push(cells.clone());
            }
        }
    }
    rows
}

#[test]
fn a_table_style_indents_the_table_and_insets_its_cells() {
    let rows = laid_out_rows();
    // Rows in document order: table 1 row 1, table 1 row 2, table 2, table 3.
    assert_eq!(rows.len(), 4, "four rows laid out");
    let styled = &rows[0][0];
    assert_eq!(
        styled.x,
        Twip(360),
        "the style's w:tblInd offsets the table"
    );
    assert_eq!(styled.margins.top, Twip(144), "style top margin");
    assert_eq!(styled.margins.start, Twip(216), "style left margin");
    assert_eq!(styled.margins.bottom, Twip(72), "style bottom margin");
    assert_eq!(styled.margins.end, Twip(230), "style right margin");
}

#[test]
fn direct_table_formatting_beats_the_style_edge_by_edge() {
    let rows = laid_out_rows();
    let direct = &rows[2][0];
    assert_eq!(direct.x, Twip(0), "the table's own w:tblInd wins");
    assert_eq!(
        direct.margins.start,
        Twip(50),
        "the table's own left margin wins"
    );
    assert_eq!(
        direct.margins.top,
        Twip(144),
        "an edge it leaves unset inherits the style"
    );
}

#[test]
fn an_unstyled_table_takes_the_default_table_style() {
    let rows = laid_out_rows();
    let unstyled = &rows[3][0];
    assert_eq!(unstyled.x, Twip(0));
    assert_eq!(unstyled.margins.start, Twip(108));
    assert_eq!(unstyled.margins.top, Twip(0));
}

#[test]
fn the_first_row_conditional_format_applies_under_tbl_look() {
    let rows = laid_out_rows();
    assert_eq!(
        rows[0][0].shading,
        Some([0x1F, 0x4E, 0x79, 255]),
        "header fill"
    );
    assert_eq!(rows[1][0].shading, None, "a body row has no fill");
}

#[test]
fn a_body_tables_decimal_indent_width_and_cell_margins_reach_the_page() {
    let import = import_document(DIRECT_DECIMAL_DOCUMENT);
    let body: Vec<&str> = import
        .report
        .entries
        .iter()
        .filter(|entry| entry.location.part_name.as_deref() == Some("word/document.xml"))
        .map(|entry| entry.feature.as_str())
        .collect();
    assert!(
        !body
            .iter()
            .any(|feature| matches!(*feature, "tblInd" | "tblW" | "tcMar")),
        "nothing on the body table is reported lost: {body:?}"
    );
    let shaper = ParleyShaper::new();
    let layout = paginate_document(&import.document, &shaper);
    let cell = layout
        .pages
        .iter()
        .flat_map(|page| &page.placed)
        .find_map(|placed| match &placed.fragment {
            BlockFragment::TableRow { cells, .. } => cells.first().cloned(),
            _ => None,
        })
        .expect("a laid-out row");
    assert_eq!(
        cell.x,
        Twip(-135),
        "the negative indent pulls the table left"
    );
    assert_eq!(cell.margins.top, Twip(100));
    assert_eq!(cell.margins.start, Twip(100));
}
