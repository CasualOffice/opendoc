//! Semantic WordprocessingML import into the normalized schema v1 model.
//!
//! This slice maps the main document body — paragraphs, runs, text, explicit
//! tabs and breaks, direct run properties (bold, italic, underline, strike,
//! size, RGB color), and direct paragraph formatting (alignment, indentation,
//! spacing) — plus the styles part (paragraph/character style definitions with
//! `basedOn` inheritance, resolved `w:pStyle`/`w:rStyle` references) and the
//! numbering part (abstract/instance definitions with resolved `w:numPr`
//! references), body-level section geometry (`w:sectPr` → page size, margins,
//! columns), media references (image relationships → the media table, no bytes
//! decoded), inline drawings (embedded pictures → media-referencing drawing
//! nodes with their EMU extent), and hyperlinks (external `r:id` resolved
//! through the relationship graph or internal `w:anchor`, wrapping their child
//! runs) into a deterministic `v1::Document`. Every traversed construct that is
//! not modeled is recorded in a bounded, deterministic compatibility report
//! under the dual-axis disposition taxonomy (`35-DISPOSITION-TAXONOMY.md`);
//! nothing is dropped silently. Constructs not yet in the semantic model (tables
//! as structure, fields, headers/footers, per-paragraph section breaks, tracked
//! changes, ...) are still fully round-trippable: in `Retention` mode the source
//! is preserved verbatim and reproduced by `casual-doc-export`. Semantic
//! modeling of every construct is progressive; nothing is excluded.
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
mod media;
mod numbering;
mod properties;
mod report;
mod retain;
mod styles;
mod tables;

pub use config::{ImportConfig, ImportMode};
pub use error::ImportError;
pub use report::{CompatibilityEntry, CompatibilityReport, ModelOutcome, RetentionOutcome};
pub use retain::RetainedSource;

use casual_doc_model::IdGenerator;
use casual_doc_model::v1::{
    BlockNode, DefinitionMap, Definitions, Document, HeaderFooter, HeaderFooterId, MediaId, Note,
    NoteId, Paragraph, ParagraphProperties,
};
use casual_doc_ooxml::DocxPackage;

use crate::media::MediaSource;
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
    let footnotes_part = related_part("/footnotes");
    let endnotes_part = related_part("/endnotes");
    let media_sources: Vec<MediaSource> = package
        .main_document_relationships()
        .iter()
        .filter(|relationship| relationship.relationship_type.ends_with("/image"))
        .filter_map(|relationship| {
            let part = relationship.resolved_part.clone()?;
            let media_type = package
                .content_type(&part)
                .map(str::to_owned)
                .unwrap_or_else(|| "application/octet-stream".to_owned());
            Some(MediaSource {
                relationship_id: relationship.id.clone(),
                media_type,
                part_name: part,
            })
        })
        .collect();

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
    // Each extra part (notes, headers, footers) carries its own image and
    // external-hyperlink relationships, so images and links inside it are modeled.
    let footnotes = match footnotes_part {
        Some(part) => Some(resolve_part_sources(package, &part)?),
        None => None,
    };
    let endnotes = match endnotes_part {
        Some(part) => Some(resolve_part_sources(package, &part)?),
        None => None,
    };
    // Header/footer parts: one per relationship, keyed by the `r:id` a `w:sectPr`
    // reference uses. Collect (r:id, part name) first, then resolve each.
    let header_refs: Vec<(String, String)> = package
        .main_document_relationships()
        .iter()
        .filter(|relationship| relationship.relationship_type.ends_with("/header"))
        .filter_map(|relationship| {
            Some((relationship.id.clone(), relationship.resolved_part.clone()?))
        })
        .collect();
    let footer_refs: Vec<(String, String)> = package
        .main_document_relationships()
        .iter()
        .filter(|relationship| relationship.relationship_type.ends_with("/footer"))
        .filter_map(|relationship| {
            Some((relationship.id.clone(), relationship.resolved_part.clone()?))
        })
        .collect();
    let mut header_parts = Vec::new();
    for (relationship_id, part) in header_refs {
        header_parts.push((relationship_id, resolve_part_sources(package, &part)?));
    }
    let mut footer_parts = Vec::new();
    for (relationship_id, part) in footer_refs {
        footer_parts.push((relationship_id, resolve_part_sources(package, &part)?));
    }
    // External hyperlink targets, resolved through the main-document
    // relationship graph (r:id -> URL), for first-class hyperlink modeling.
    let hyperlink_rels: std::collections::BTreeMap<String, String> = package
        .main_document_relationships()
        .iter()
        .filter(|relationship| relationship.relationship_type.ends_with("/hyperlink"))
        .filter(|relationship| {
            relationship.target_mode == casual_doc_ooxml::TargetMode::External
                && !relationship.id.is_empty()
        })
        .map(|relationship| (relationship.id.clone(), relationship.target.clone()))
        .collect();

    let mut import = import_with_sources(
        &document_bytes,
        styles_bytes.as_deref(),
        numbering_bytes.as_deref(),
        footnotes.as_ref(),
        endnotes.as_ref(),
        &header_parts,
        &footer_parts,
        &media_sources,
        &hyperlink_rels,
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
    import_with_sources(
        xml,
        None,
        None,
        None,
        None,
        &[],
        &[],
        &[],
        &std::collections::BTreeMap::new(),
        config,
    )
}

/// Reads an extra part and resolves its own image and external-hyperlink
/// relationships (via the part's `_rels`), so content inside it can be modeled.
fn resolve_part_sources(
    package: &mut DocxPackage<'_>,
    part_name: &str,
) -> Result<PartSources, ImportError> {
    let xml = package.read_part(part_name).map_err(ImportError::Package)?;
    let relationships = package
        .part_relationships(part_name)
        .map_err(ImportError::Package)?;
    let mut images = Vec::new();
    for relationship in &relationships {
        if relationship.relationship_type.ends_with("/image") {
            if let Some(part) = relationship.resolved_part.clone() {
                let media_type = package
                    .content_type(&part)
                    .map(str::to_owned)
                    .unwrap_or_else(|| "application/octet-stream".to_owned());
                images.push(MediaSource {
                    relationship_id: relationship.id.clone(),
                    media_type,
                    part_name: part,
                });
            }
        }
    }
    let hyperlinks = relationships
        .iter()
        .filter(|relationship| relationship.relationship_type.ends_with("/hyperlink"))
        .filter(|relationship| {
            relationship.target_mode == casual_doc_ooxml::TargetMode::External
                && !relationship.id.is_empty()
        })
        .map(|relationship| (relationship.id.clone(), relationship.target.clone()))
        .collect();
    Ok(PartSources {
        xml,
        images,
        hyperlinks,
    })
}

/// A note definition map plus its source-`w:id` -> id resolution index.
type BuiltNotes = (
    DefinitionMap<NoteId, Note>,
    std::collections::BTreeMap<String, NoteId>,
);

/// Parses a notes part into a `NoteId`-keyed definition map plus a source-`w:id`
/// resolution index for in-body references. The note part's own image and
/// hyperlink relationships are resolved so images/links inside a note are modeled.
/// Missing part → empty.
#[allow(clippy::too_many_arguments)]
fn build_notes(
    part: Option<&PartSources>,
    container: &'static [u8],
    styles: &Styles,
    numbering: &Numbering,
    media: &mut DefinitionMap<MediaId, casual_doc_model::v1::MediaReference>,
    ids: &mut IdGenerator,
    reporter: &mut Reporter,
    config: ImportConfig,
) -> Result<BuiltNotes, ImportError> {
    let mut map = DefinitionMap::default();
    let mut index = std::collections::BTreeMap::new();
    if let Some(part) = part {
        let media_index = media::build_into(&part.images, media, ids, reporter)?;
        let notes = body::parse_notes(
            &part.xml,
            ids,
            reporter,
            styles,
            numbering,
            &media_index,
            &part.hyperlinks,
            container,
            config,
        )?;
        for (source_id, note_id, blocks) in notes {
            index.insert(source_id, note_id);
            map.insert(note_id, Note { blocks });
        }
    }
    Ok((map, index))
}

/// A header/footer definition map plus its relationship-id -> id resolution index.
type BuiltHeaderFooters = (
    DefinitionMap<HeaderFooterId, HeaderFooter>,
    std::collections::BTreeMap<String, HeaderFooterId>,
);

/// Parses each header/footer part into a `HeaderFooterId`-keyed definition map
/// plus a relationship-id resolution index for section references. Each part's id
/// precedes its content ids (document order); parts arrive in relationship-id
/// order for determinism.
#[allow(clippy::too_many_arguments)]
fn build_header_footers(
    parts: &[(String, PartSources)],
    root: &'static [u8],
    styles: &Styles,
    numbering: &Numbering,
    media: &mut DefinitionMap<MediaId, casual_doc_model::v1::MediaReference>,
    ids: &mut IdGenerator,
    reporter: &mut Reporter,
    config: ImportConfig,
) -> Result<BuiltHeaderFooters, ImportError> {
    let mut map = DefinitionMap::default();
    let mut index = std::collections::BTreeMap::new();
    for (relationship_id, part) in parts {
        // The header/footer id precedes its content ids; its media is added to
        // the shared table just before parsing so its drawings resolve.
        let node = ids
            .next_id()
            .map_err(|_| ImportError::LimitExceeded { limit: "node_ids" })?;
        let hf_id = HeaderFooterId::new(node);
        let media_index = media::build_into(&part.images, media, ids, reporter)?;
        let blocks = body::parse_header_footer(
            &part.xml,
            ids,
            reporter,
            styles,
            numbering,
            &media_index,
            &part.hyperlinks,
            root,
            config,
        )?;
        index.insert(relationship_id.clone(), hf_id);
        map.insert(hf_id, HeaderFooter { blocks });
    }
    Ok((map, index))
}

/// An extra part's bytes plus its own resolved image and external-hyperlink
/// relationships, so images and links inside a note/header/footer are modeled.
#[derive(Default)]
pub(crate) struct PartSources {
    pub xml: Vec<u8>,
    pub images: Vec<MediaSource>,
    pub hyperlinks: std::collections::BTreeMap<String, String>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn import_with_sources(
    document_xml: &[u8],
    styles_xml: Option<&[u8]>,
    numbering_xml: Option<&[u8]>,
    footnotes: Option<&PartSources>,
    endnotes: Option<&PartSources>,
    header_parts: &[(String, PartSources)],
    footer_parts: &[(String, PartSources)],
    media_sources: &[MediaSource],
    hyperlink_rels: &std::collections::BTreeMap<String, String>,
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
    // Media is built into one shared table BEFORE any body so drawings resolve
    // their `r:embed`/`r:id` to a `MediaId` while parsing. The main document's
    // images come first (identical to before), then each extra part's images are
    // added and that part is parsed with its own relationship index — so an image
    // inside a note/header/footer is modeled, and per-part relationship ids (which
    // collide across parts) resolve independently. Deterministic id order:
    // document -> styles -> numbering -> main media -> [footnotes media, content]
    // -> [endnotes ...] -> [headers ...] -> [footers ...] -> body.
    let mut media = DefinitionMap::default();
    let media_index = media::build_into(media_sources, &mut media, &mut ids, &mut reporter)?;

    let (footnotes_map, footnote_ids) = build_notes(
        footnotes,
        b"footnote",
        &styles,
        &numbering,
        &mut media,
        &mut ids,
        &mut reporter,
        config,
    )?;
    let (endnotes_map, endnote_ids) = build_notes(
        endnotes,
        b"endnote",
        &styles,
        &numbering,
        &mut media,
        &mut ids,
        &mut reporter,
        config,
    )?;
    let (headers, header_ids) = build_header_footers(
        header_parts,
        b"hdr",
        &styles,
        &numbering,
        &mut media,
        &mut ids,
        &mut reporter,
        config,
    )?;
    let (footers, footer_ids) = build_header_footers(
        footer_parts,
        b"ftr",
        &styles,
        &numbering,
        &mut media,
        &mut ids,
        &mut reporter,
        config,
    )?;

    let (mut body, sections) = body::parse(
        document_xml,
        &mut ids,
        &mut reporter,
        body::ParseInputs {
            styles: &styles,
            numbering: &numbering,
            media_index: &media_index,
            hyperlink_rels,
            footnote_ids: &footnote_ids,
            endnote_ids: &endnote_ids,
            header_ids: &header_ids,
            footer_ids: &footer_ids,
        },
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
        sections,
        media,
        footnotes: footnotes_map,
        endnotes: endnotes_map,
        headers,
        footers,
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
