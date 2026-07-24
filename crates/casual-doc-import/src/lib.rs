//! Semantic WordprocessingML import into the normalized schema v1 model.
//!
//! This slice maps the main document body — paragraphs, runs, text, explicit
//! tabs and breaks, direct run properties (bold, italic, underline, strike,
//! size, RGB color), and direct paragraph formatting (alignment, indentation,
//! spacing) — plus the styles part (paragraph/character style definitions with
//! `basedOn` inheritance, resolved `w:pStyle`/`w:rStyle` references) and the
//! numbering part (abstract/instance definitions with resolved `w:numPr`
//! references) into a deterministic `v1::Document`. Every traversed construct
//! that is not modeled is recorded in a bounded, deterministic compatibility
//! report under the dual-axis disposition taxonomy (`35-DISPOSITION-TAXONOMY.md`);
//! nothing is dropped silently. Sections, tables (as structure), media, fields,
//! headers/footers, and tracked changes are reported, not yet modeled.
//!
//! Import runs in `Semantic` mode (report-and-drop) by default. `Retention`
//! mode additionally keeps the original main-document bytes verbatim (the D5
//! tier-1 byte floor), so unmapped constructs are `preserved` and an unedited
//! document round-trips exactly. Edit-tolerant tier-2 per-construct provenance
//! and the Phase-2 writer are the next round-trip milestones.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod body;
mod config;
mod error;
mod numbering;
mod properties;
mod report;
mod retain;
mod styles;

pub use config::{ImportConfig, ImportMode};
pub use error::ImportError;
pub use report::{CompatibilityEntry, CompatibilityReport, ModelOutcome, RetentionOutcome};
pub use retain::RetainedSource;

use casual_doc_model::IdGenerator;
use casual_doc_model::v1::{BlockNode, Definitions, Document, Paragraph, ParagraphProperties};
use casual_doc_ooxml::DocxPackage;

use crate::numbering::Numbering;
use crate::report::Reporter;
use crate::styles::Styles;

/// The result of importing a main document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Import {
    /// The normalized v1 document.
    pub document: Document,
    /// The compatibility report.
    pub report: CompatibilityReport,
    /// Source retained for round-trip; `Some` only in `Retention` mode.
    pub retained_source: Option<RetainedSource>,
}

/// Imports the main document of an admitted DOCX package into a v1 document,
/// resolving the styles part through the main document's relationship graph.
pub fn import_package(
    package: &mut DocxPackage<'_>,
    config: ImportConfig,
) -> Result<Import, ImportError> {
    let main_part = package.main_document_part().to_owned();
    let related_part = |suffix: &str| {
        package
            .main_document_relationships()
            .iter()
            .find(|relationship| relationship.relationship_type.ends_with(suffix))
            .and_then(|relationship| relationship.resolved_part.clone())
    };
    let styles_part = related_part("/styles");
    let numbering_part = related_part("/numbering");

    let document_bytes = package
        .read_part(&main_part)
        .map_err(ImportError::Package)?;
    let styles_bytes = match styles_part {
        Some(part) => Some(package.read_part(&part).map_err(ImportError::Package)?),
        None => None,
    };
    let numbering_bytes = match numbering_part {
        Some(part) => Some(package.read_part(&part).map_err(ImportError::Package)?),
        None => None,
    };
    let mut import = import_with_sources(
        &document_bytes,
        styles_bytes.as_deref(),
        numbering_bytes.as_deref(),
        config,
    )?;

    // In Retention mode, retain every admitted part verbatim (the package-level
    // byte floor) so styles, media, and other parts can be reproduced too.
    if let Some(retained) = import.retained_source.as_mut() {
        let names: Vec<String> = package
            .entries()
            .iter()
            .map(|entry| entry.part_name.clone())
            .collect();
        let mut total = 0_usize;
        for name in names {
            let bytes = package.read_part(&name).map_err(ImportError::Package)?;
            total = total.saturating_add(bytes.len());
            if total > config.max_text_bytes {
                return Err(ImportError::LimitExceeded {
                    limit: "retained_bytes",
                });
            }
            retained.parts.insert(name, bytes);
        }
    }
    Ok(import)
}

/// Imports main-document WordprocessingML bytes (no styles) into a v1 document.
pub fn import_main_document_xml(xml: &[u8], config: ImportConfig) -> Result<Import, ImportError> {
    import_with_sources(xml, None, None, config)
}

pub(crate) fn import_with_sources(
    document_xml: &[u8],
    styles_xml: Option<&[u8]>,
    numbering_xml: Option<&[u8]>,
    config: ImportConfig,
) -> Result<Import, ImportError> {
    config.validate()?;

    // Retention mode retains the original main-document bytes verbatim (the
    // tier-1 byte floor) so unmapped constructs are preserved for round-trip.
    let retained_source = match config.mode {
        ImportMode::Retention => {
            if document_xml.len() > config.max_text_bytes {
                return Err(ImportError::LimitExceeded {
                    limit: "retained_bytes",
                });
            }
            Some(RetainedSource {
                main_document: document_xml.to_vec(),
                parts: std::collections::BTreeMap::new(),
            })
        }
        ImportMode::Semantic => None,
    };
    let retention = match config.mode {
        ImportMode::Retention => RetentionOutcome::Preserved,
        ImportMode::Semantic => RetentionOutcome::NotRetained,
    };

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
    let numbering = match numbering_xml {
        Some(xml) => numbering::parse(xml, &mut ids, &mut reporter, config)?,
        None => Numbering::default(),
    };
    let mut body = body::parse(
        document_xml,
        &mut ids,
        &styles,
        &numbering,
        &mut reporter,
        config,
    )?;
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

    let (abstract_numbering, numbering_instances) = numbering.into_definitions();
    let definitions = Definitions {
        styles: styles.into_definitions(),
        abstract_numbering,
        numbering: numbering_instances,
        ..Definitions::default()
    };
    let document = Document::new(document_id, body, definitions).map_err(ImportError::Model)?;
    Ok(Import {
        document,
        report: reporter.into_report(retention),
        retained_source,
    })
}

#[cfg(test)]
mod tests;
