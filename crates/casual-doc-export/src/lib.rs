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
mod tests {
    use super::*;

    use casual_doc_import::{ImportConfig, ImportMode, import_package};
    use casual_doc_ooxml::{DocxPackage, PackageLimits};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

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
    fn semantic_mode_has_no_retained_parts() {
        let source = casual_doc_import::RetainedSource {
            main_document: Vec::new(),
            parts: std::collections::BTreeMap::new(),
        };
        assert_eq!(write_package(&source), Err(ExportError::NoRetainedParts));
    }
}
