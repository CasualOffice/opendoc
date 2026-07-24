//! Semantic WordprocessingML import into the normalized schema v1 model.
//!
//! This slice maps the main document body — paragraphs, runs, text, explicit
//! tabs and breaks, direct run properties (bold, italic, underline, strike,
//! size, RGB color), and direct paragraph formatting (alignment, indentation,
//! spacing) — plus the styles part (paragraph/character style definitions with
//! `basedOn` inheritance, resolved `w:pStyle`/`w:rStyle` references) into a
//! deterministic `v1::Document`. Every traversed construct that is not modeled
//! is recorded in a bounded, deterministic compatibility report under the
//! dual-axis disposition taxonomy (`35-DISPOSITION-TAXONOMY.md`); nothing is
//! dropped silently. Numbering, sections, tables (as structure), media, fields,
//! headers/footers, and tracked changes are reported, not yet modeled.
//!
//! Import runs in Semantic mode (report-and-drop). Round-trip fidelity
//! (Retention mode: preserve unmapped constructs) is the next milestone.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod body;
mod config;
mod error;
mod properties;
mod report;
mod styles;

pub use config::ImportConfig;
pub use error::ImportError;
pub use report::{CompatibilityEntry, CompatibilityReport, ModelOutcome, RetentionOutcome};

use casual_doc_model::IdGenerator;
use casual_doc_model::v1::{BlockNode, Definitions, Document, Paragraph, ParagraphProperties};
use casual_doc_ooxml::DocxPackage;

use crate::report::Reporter;
use crate::styles::Styles;

/// The result of importing a main document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Import {
    /// The normalized v1 document.
    pub document: Document,
    /// The compatibility report.
    pub report: CompatibilityReport,
}

/// Imports the main document of an admitted DOCX package into a v1 document,
/// resolving the styles part through the main document's relationship graph.
pub fn import_package(
    package: &mut DocxPackage<'_>,
    config: ImportConfig,
) -> Result<Import, ImportError> {
    let main_part = package.main_document_part().to_owned();
    let styles_part = package
        .main_document_relationships()
        .iter()
        .find(|relationship| relationship.relationship_type.ends_with("/styles"))
        .and_then(|relationship| relationship.resolved_part.clone());

    let document_bytes = package
        .read_part(&main_part)
        .map_err(ImportError::Package)?;
    let styles_bytes = match styles_part {
        Some(part) => Some(package.read_part(&part).map_err(ImportError::Package)?),
        None => None,
    };
    import_with_sources(&document_bytes, styles_bytes.as_deref(), config)
}

/// Imports main-document WordprocessingML bytes (no styles) into a v1 document.
pub fn import_main_document_xml(xml: &[u8], config: ImportConfig) -> Result<Import, ImportError> {
    import_with_sources(xml, None, config)
}

pub(crate) fn import_with_sources(
    document_xml: &[u8],
    styles_xml: Option<&[u8]>,
    config: ImportConfig,
) -> Result<Import, ImportError> {
    config.validate()?;
    let mut ids = IdGenerator::new(config.id_namespace);
    // documentId is the first allocated id (deterministic).
    let document_id = ids
        .next_id()
        .map_err(|_| ImportError::LimitExceeded { limit: "node_ids" })?;
    let mut reporter = Reporter::default();

    let styles = match styles_xml {
        Some(xml) => styles::parse(xml, &mut ids, &mut reporter, config)?,
        None => Styles::default(),
    };
    let mut body = body::parse(document_xml, &mut ids, &styles, &mut reporter, config)?;
    if body.is_empty() {
        // A body with no paragraphs yields a single empty paragraph so the v1
        // document has a non-empty body.
        let id = ids
            .next_id()
            .map_err(|_| ImportError::LimitExceeded { limit: "node_ids" })?;
        body.push(BlockNode::Paragraph(Paragraph {
            id,
            properties: ParagraphProperties::default(),
            inlines: Vec::new(),
        }));
    }

    let definitions = Definitions {
        styles: styles.into_definitions(),
        ..Definitions::default()
    };
    let document = Document::new(document_id, body, definitions).map_err(ImportError::Model)?;
    Ok(Import {
        document,
        report: reporter.into_report(),
    })
}

#[cfg(test)]
mod tests;
