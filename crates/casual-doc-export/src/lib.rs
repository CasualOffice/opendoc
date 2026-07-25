//! DOCX package writer for the no-edit round-trip case.
//!
//! This is the "exact no-op return" that Retention mode enables: given the
//! source parts retained verbatim at import ([`RetainedSource`]), it
//! reconstructs a valid DOCX package with byte-identical part contents. It does
//! NOT regenerate OOXML from the model — that is the Phase-2 semantic writer.
//! Combined with the importer, it makes round-trip end-to-end verifiable:
//! `import (Retention) -> write_package -> reopen -> identical model`.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt;
use std::io::{Cursor, Write};

use casual_doc_import::RetainedSource;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

mod semantic;
pub use semantic::write_document;

/// A package-writing failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExportError {
    /// No parts were retained (source was not imported in Retention mode).
    NoRetainedParts,
    /// The ZIP package could not be assembled.
    Package,
}

impl fmt::Display for ExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoRetainedParts => {
                formatter.write_str("no retained parts; import in Retention mode to reconstruct")
            }
            Self::Package => formatter.write_str("DOCX package could not be assembled"),
        }
    }
}

impl Error for ExportError {}

/// Reconstructs a valid DOCX package from a retained source, byte-identical in
/// part content. Parts are written in deterministic (sorted) order with a fixed
/// timestamp, so the output bytes are reproducible.
pub fn write_package(source: &RetainedSource) -> Result<Vec<u8>, ExportError> {
    if source.parts.is_empty() {
        return Err(ExportError::NoRetainedParts);
    }
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(DateTime::default());
    // `source.parts` is a BTreeMap, so iteration is already sorted by name.
    for (name, bytes) in &source.parts {
        writer
            .start_file(name, options)
            .map_err(|_| ExportError::Package)?;
        writer.write_all(bytes).map_err(|_| ExportError::Package)?;
    }
    Ok(writer
        .finish()
        .map_err(|_| ExportError::Package)?
        .into_inner())
}

#[cfg(test)]
mod semantic_tests {
    use std::collections::BTreeMap;

    use casual_doc_import::{ImportConfig, ImportMode, import_main_document_xml, import_package};
    use casual_doc_ooxml::{DocxPackage, PackageLimits};

    use crate::write_document;

    /// The Phase-1B semantic fixed point: importing a document, writing it, and
    /// reopening yields the identical model. The importer allocates ids in
    /// document order, so a writer that emits the body in that same order
    /// reproduces every id — the strongest correctness gate for the writer.
    #[test]
    fn core_body_survives_the_semantic_round_trip() {
        let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:pPr><w:jc w:val="center"/><w:keepNext/><w:outlineLvl w:val="2"/></w:pPr>
                <w:r><w:rPr><w:b/><w:i w:val="0"/><w:u/><w:color w:val="FF0000"/><w:sz w:val="28"/></w:rPr>
                    <w:t xml:space="preserve">Hello </w:t></w:r>
                <w:r><w:t>world</w:t></w:r><w:r><w:tab/></w:r>
                <w:r><w:t>after</w:t></w:r><w:r><w:br w:type="page"/></w:r></w:p>
            <w:p><w:r><w:t>Second paragraph</w:t></w:r></w:p>
        </w:body></w:document>"#;
        let m1 = import_main_document_xml(xml, ImportConfig::default())
            .unwrap()
            .document;

        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let mut package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();
        let m2 = import_package(
            &mut package,
            ImportConfig {
                mode: ImportMode::Semantic,
                ..ImportConfig::default()
            },
        )
        .unwrap()
        .document;

        assert_eq!(m1, m2, "the model survives write -> reopen unchanged");
    }

    #[test]
    fn tables_survive_the_semantic_round_trip() {
        // A table exercising the grid, table/row/cell properties (borders,
        // shading, margins, merges, vAlign, layout, look), and a nested table
        // inside a cell — all must survive write -> reopen unchanged.
        let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:tbl>
                <w:tblPr>
                    <w:jc w:val="center"/><w:tblW w:type="dxa" w:w="9000"/>
                    <w:tblLayout w:type="fixed"/><w:tblLook w:firstRow="1" w:noVBand="1"/>
                    <w:tblBorders><w:top w:val="single" w:sz="8" w:color="112233" w:space="4"/>
                        <w:insideH w:val="dotted"/></w:tblBorders>
                    <w:shd w:val="clear" w:fill="EEEEEE"/>
                    <w:tblCellMar><w:top w:type="dxa" w:w="120"/><w:start w:type="dxa" w:w="60"/></w:tblCellMar>
                </w:tblPr>
                <w:tblGrid><w:gridCol w:w="4500"/><w:gridCol w:w="4500"/></w:tblGrid>
                <w:tr>
                    <w:trPr><w:trHeight w:val="500" w:hRule="atLeast"/><w:tblHeader/></w:trPr>
                    <w:tc>
                        <w:tcPr><w:gridSpan w:val="2"/><w:tcW w:type="dxa" w:w="9000"/>
                            <w:shd w:val="clear" w:fill="FF0000"/><w:vAlign w:val="center"/><w:noWrap/>
                            <w:tcBorders><w:bottom w:val="double" w:sz="16"/></w:tcBorders>
                            <w:tcMar><w:end w:type="dxa" w:w="90"/></w:tcMar></w:tcPr>
                        <w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Header</w:t></w:r></w:p>
                    </w:tc>
                </w:tr>
                <w:tr>
                    <w:tc><w:tcPr><w:vMerge w:val="restart"/></w:tcPr>
                        <w:tbl><w:tr><w:tc><w:p><w:r><w:t>nested</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
                        <w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc>
                    <w:tc><w:p><w:r><w:t>b</w:t></w:r></w:p></w:tc>
                </w:tr>
            </w:tbl>
        </w:body></w:document>"#;
        let m1 = import_main_document_xml(xml, ImportConfig::default())
            .unwrap()
            .document;
        let bytes = write_document(&m1, &BTreeMap::new()).unwrap();
        let mut package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();
        let m2 = import_package(
            &mut package,
            ImportConfig {
                mode: ImportMode::Semantic,
                ..ImportConfig::default()
            },
        )
        .unwrap()
        .document;
        assert_eq!(m1, m2, "the table model survives write -> reopen unchanged");
    }

    #[test]
    fn writer_is_deterministic() {
        let xml = br#"<w:document xmlns:w="urn:w"><w:body>
            <w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
        let m = import_main_document_xml(xml, ImportConfig::default())
            .unwrap()
            .document;
        assert_eq!(
            write_document(&m, &BTreeMap::new()).unwrap(),
            write_document(&m, &BTreeMap::new()).unwrap(),
            "the same model writes identical bytes"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use casual_doc_import::{ImportConfig, ImportMode, import_package};
    use casual_doc_model::v1::{BlockNode, Document, InlineNode, Table, VerticalMerge};
    use casual_doc_ooxml::{DocxPackage, PackageLimits};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    /// Whether any inline node (recursing into hyperlinks) matches `predicate`.
    fn any_inline(document: &Document, predicate: impl Fn(&InlineNode) -> bool + Copy) -> bool {
        fn walk(inline: &InlineNode, predicate: impl Fn(&InlineNode) -> bool + Copy) -> bool {
            if predicate(inline) {
                return true;
            }
            match inline {
                InlineNode::Hyperlink(link) => {
                    return link.inlines.iter().any(|child| walk(child, predicate));
                }
                InlineNode::Field(field) => {
                    return field.inlines.iter().any(|child| walk(child, predicate));
                }
                InlineNode::TextBox(text_box) => {
                    return walk_blocks(&text_box.blocks, predicate);
                }
                InlineNode::Sdt(sdt) => {
                    return sdt.inlines.iter().any(|child| walk(child, predicate));
                }
                _ => {}
            }
            false
        }
        fn walk_blocks(
            blocks: &[BlockNode],
            predicate: impl Fn(&InlineNode) -> bool + Copy,
        ) -> bool {
            blocks.iter().any(|block| match block {
                BlockNode::Paragraph(paragraph) => paragraph
                    .inlines
                    .iter()
                    .any(|inline| walk(inline, predicate)),
                BlockNode::Table(table) => table.rows.iter().any(|row| {
                    row.cells
                        .iter()
                        .any(|cell| walk_blocks(&cell.blocks, predicate))
                }),
                BlockNode::Sdt(sdt) => walk_blocks(&sdt.blocks, predicate),
            })
        }
        walk_blocks(document.body(), predicate)
    }

    fn sample_package() -> Vec<u8> {
        let content_types = br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
        let rels = br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document = br#"<?xml version="1.0"?><w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>round trip</w:t></w:r></w:p></w:body></w:document>"#;

        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in [
            ("[Content_Types].xml", content_types.as_slice()),
            ("_rels/.rels", rels.as_slice()),
            ("word/document.xml", document.as_slice()),
        ] {
            writer
                .start_file(
                    name,
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn no_edit_round_trip_reproduces_the_model() {
        let original = sample_package();
        let config = ImportConfig {
            mode: ImportMode::Retention,
            ..ImportConfig::default()
        };

        let first = {
            let mut package = DocxPackage::open(&original, PackageLimits::default()).unwrap();
            import_package(&mut package, config).unwrap()
        };

        // Reconstruct a DOCX from the retained parts and re-import it.
        let rebuilt = write_package(first.retained_source.as_ref().unwrap()).unwrap();
        let second = {
            let mut package = DocxPackage::open(&rebuilt, PackageLimits::default()).unwrap();
            import_package(&mut package, config).unwrap()
        };

        // The reconstructed package imports to an identical model: round-trip.
        assert_eq!(first.document, second.document);
        // Part contents are byte-identical to the source.
        assert_eq!(
            second.retained_source.as_ref().unwrap().parts,
            first.retained_source.as_ref().unwrap().parts
        );
        // Writing is deterministic.
        assert_eq!(
            write_package(first.retained_source.as_ref().unwrap()).unwrap(),
            rebuilt
        );
    }

    fn first_table(document: &Document) -> &Table {
        document
            .body()
            .iter()
            .find_map(|block| match block {
                BlockNode::Table(table) => Some(table),
                BlockNode::Paragraph(_) | BlockNode::Sdt(_) => None,
            })
            .expect("a table in the body")
    }

    #[test]
    fn table_with_merged_cells_round_trips() {
        // A table with a horizontal (gridSpan) and vertical (vMerge) merge. The
        // structure is now modeled AND the source round-trips byte-for-byte via
        // Retention, so the merge geometry survives import -> write -> reopen.
        let original = include_bytes!("../../../fixtures/corpus/real-producer-table-merges.docx");
        let config = ImportConfig {
            mode: ImportMode::Retention,
            ..ImportConfig::default()
        };

        let first = {
            let mut package = DocxPackage::open(original, PackageLimits::default()).unwrap();
            import_package(&mut package, config).unwrap()
        };
        let rebuilt = write_package(first.retained_source.as_ref().unwrap()).unwrap();
        let second = {
            let mut package = DocxPackage::open(&rebuilt, PackageLimits::default()).unwrap();
            import_package(&mut package, config).unwrap()
        };

        assert_eq!(first.document, second.document);
        assert_eq!(
            second.retained_source.as_ref().unwrap().parts,
            first.retained_source.as_ref().unwrap().parts
        );

        // The reopened model still carries the modeled merge geometry.
        let table = first_table(&second.document);
        assert_eq!(table.rows[0].cells[0].properties.grid_span, Some(2));
        assert_eq!(
            table.rows[1].cells[0].properties.vertical_merge,
            Some(VerticalMerge::Restart)
        );
        assert_eq!(
            table.rows[2].cells[0].properties.vertical_merge,
            Some(VerticalMerge::Continue)
        );
    }

    #[test]
    fn real_document_with_tables_and_lists_round_trips() {
        // A real LibreOffice .docx with tables, bullet/numbered lists, styles,
        // and numbering. Retention + reconstruction round-trips ALL of it —
        // every tag, nested element, and part — regardless of what the semantic
        // model captures yet.
        let original = include_bytes!("../../../fixtures/corpus/real-producer-table-list.docx");
        let config = ImportConfig {
            mode: ImportMode::Retention,
            ..ImportConfig::default()
        };

        let first = {
            let mut package = DocxPackage::open(original, PackageLimits::default()).unwrap();
            import_package(&mut package, config).unwrap()
        };
        let rebuilt = write_package(first.retained_source.as_ref().unwrap()).unwrap();
        let second = {
            let mut package = DocxPackage::open(&rebuilt, PackageLimits::default()).unwrap();
            import_package(&mut package, config).unwrap()
        };

        assert_eq!(first.document, second.document);
        // Every retained part (document, styles, numbering, ...) is reproduced
        // byte-identically, so nothing is lost across the round trip.
        assert_eq!(
            second.retained_source.as_ref().unwrap().parts,
            first.retained_source.as_ref().unwrap().parts
        );
    }

    #[test]
    fn rich_document_with_nested_tables_and_image_round_trips() {
        // Nested tables, an embedded image (word/media/image1.png), and styles.
        let original = include_bytes!("../../../fixtures/corpus/real-producer-rich.docx");
        let config = ImportConfig {
            mode: ImportMode::Retention,
            ..ImportConfig::default()
        };

        let first = {
            let mut package = DocxPackage::open(original, PackageLimits::default()).unwrap();
            import_package(&mut package, config).unwrap()
        };
        // The embedded image relationship is mapped into the media table and
        // the picture is modeled as a first-class inline drawing.
        assert!(!first.document.definitions().media.is_empty());
        assert!(any_inline(&first.document, |inline| matches!(
            inline,
            InlineNode::Drawing(_)
        )));

        let rebuilt = write_package(first.retained_source.as_ref().unwrap()).unwrap();
        let second = {
            let mut package = DocxPackage::open(&rebuilt, PackageLimits::default()).unwrap();
            import_package(&mut package, config).unwrap()
        };

        assert_eq!(first.document, second.document);
        // The image binary and every other part round-trip byte-identically.
        let parts = &first.retained_source.as_ref().unwrap().parts;
        assert!(parts.contains_key("word/media/image1.png"));
        assert_eq!(&second.retained_source.as_ref().unwrap().parts, parts);
    }

    #[test]
    fn document_with_external_and_internal_hyperlinks_round_trips() {
        // External (`r:id` -> an External-mode relationship) and internal
        // (`w:anchor`) hyperlinks. They round-trip verbatim today; the semantic
        // model captures them as first-class inlines as modeling lands.
        let original = include_bytes!("../../../fixtures/corpus/real-producer-hyperlinks.docx");
        let config = ImportConfig {
            mode: ImportMode::Retention,
            ..ImportConfig::default()
        };

        let first = {
            let mut package = DocxPackage::open(original, PackageLimits::default()).unwrap();
            import_package(&mut package, config).unwrap()
        };
        // The links are modeled as first-class inline hyperlinks.
        assert!(any_inline(&first.document, |inline| matches!(
            inline,
            InlineNode::Hyperlink(_)
        )));
        let rebuilt = write_package(first.retained_source.as_ref().unwrap()).unwrap();
        let second = {
            let mut package = DocxPackage::open(&rebuilt, PackageLimits::default()).unwrap();
            import_package(&mut package, config).unwrap()
        };

        assert_eq!(first.document, second.document);
        assert_eq!(
            second.retained_source.as_ref().unwrap().parts,
            first.retained_source.as_ref().unwrap().parts
        );
    }

    #[test]
    fn document_with_header_and_footer_parts_round_trips() {
        // Separate word/header1.xml and word/footer1.xml parts referenced from
        // the section. Retention retains every part, so the header/footer parts
        // round-trip byte-identically even before they are modeled.
        let original = include_bytes!("../../../fixtures/corpus/real-producer-header-footer.docx");
        let config = ImportConfig {
            mode: ImportMode::Retention,
            ..ImportConfig::default()
        };

        let first = {
            let mut package = DocxPackage::open(original, PackageLimits::default()).unwrap();
            import_package(&mut package, config).unwrap()
        };
        let parts = &first.retained_source.as_ref().unwrap().parts;
        assert!(parts.contains_key("word/header1.xml"));
        assert!(parts.contains_key("word/footer1.xml"));

        let rebuilt = write_package(first.retained_source.as_ref().unwrap()).unwrap();
        let second = {
            let mut package = DocxPackage::open(&rebuilt, PackageLimits::default()).unwrap();
            import_package(&mut package, config).unwrap()
        };

        assert_eq!(first.document, second.document);
        assert_eq!(&second.retained_source.as_ref().unwrap().parts, parts);
    }

    #[test]
    fn document_with_footnotes_part_round_trips() {
        // A footnote reference in the body plus a separate word/footnotes.xml
        // part. Retention retains the part so it round-trips before modeling.
        let original = include_bytes!("../../../fixtures/corpus/real-producer-footnotes.docx");
        let config = ImportConfig {
            mode: ImportMode::Retention,
            ..ImportConfig::default()
        };

        let first = {
            let mut package = DocxPackage::open(original, PackageLimits::default()).unwrap();
            import_package(&mut package, config).unwrap()
        };
        assert!(
            first
                .retained_source
                .as_ref()
                .unwrap()
                .parts
                .contains_key("word/footnotes.xml")
        );

        let rebuilt = write_package(first.retained_source.as_ref().unwrap()).unwrap();
        let second = {
            let mut package = DocxPackage::open(&rebuilt, PackageLimits::default()).unwrap();
            import_package(&mut package, config).unwrap()
        };

        assert_eq!(first.document, second.document);
        assert_eq!(
            second.retained_source.as_ref().unwrap().parts,
            first.retained_source.as_ref().unwrap().parts
        );
    }

    #[test]
    fn semantic_mode_has_no_retained_parts() {
        let source = casual_doc_import::RetainedSource {
            main_document: Vec::new(),
            parts: std::collections::BTreeMap::new(),
        };
        assert_eq!(write_package(&source), Err(ExportError::NoRetainedParts));
    }
}
