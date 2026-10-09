// SPDX-License-Identifier: Apache-2.0

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
//! nodes with their EMU extent), hyperlinks (external `r:id` resolved
//! through the relationship graph or internal `w:anchor`, wrapping their child
//! runs), and embedded objects (charts, SmartArt diagrams, and OLE objects →
//! first-class reference nodes pointing at their side-table-preserved parts,
//! which the writer re-references so they are no longer orphaned) into a
//! deterministic `v1::Document`. Every traversed construct that is
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
mod chart;
mod comments_ext;
mod config;
mod coverage;
mod error;
mod font_table;
mod math;
mod media;
mod metadata;
mod noop;
mod numbering;
mod opaque;
mod properties;
mod recovery;
mod report;
mod retain;
mod settings;
mod styles;
mod tables;
mod theme;
mod vml;
mod watermark;
mod xml_repair;

pub use config::{ImportConfig, ImportMode};
// Own line, kept out of any sorted block (the repo's parallel-PR rule).
pub use coverage::{MeaningfulMarkup, meaningful_markup};
pub use error::ImportError;
pub use opaque::{
    RelationshipOwner, RetainedPart, RetainedParts, RetainedRelationship, RetainedRels,
};
// Own line, kept out of any sorted block (the repo's parallel-PR rule).
pub use recovery::{
    MAX_DETAIL_BYTES, MAX_REPAIRS, PartRole, RecoveryReport, Repair, RepairKind, Severity,
};
pub use report::{
    CompatibilityEntry, CompatibilityReport, Disposition, DispositionViolation, FeatureLocation,
    LedgerId, LedgerRecord, ModelOutcome, PartDisposition, PreservationKind, PreservationLedger,
    RSID_CLASS_FEATURE, RetentionOutcome, WATERMARK_CLASS_FEATURE,
};
pub use retain::RetainedSource;
pub use vml::{
    VmlColor, VmlDrawing, VmlFill, VmlHorizontalAlign, VmlHr, VmlHrAlign, VmlImageEffects,
    VmlPosition, VmlRelFrame, VmlShapeKind, VmlStroke, VmlTextAnchor, VmlTextPath, VmlTextbox,
    VmlVerticalAlign, VmlWrap, VmlWrapMode, parse_vml_pict,
};

use casual_doc_model::IdGenerator;
use casual_doc_model::v1::{
    BlockNode, Comment, CommentId, DefinitionMap, Definitions, Document, DocumentSettings,
    HeaderFooter, HeaderFooterId, MediaId, Note, NoteId, Paragraph, ParagraphProperties, Person,
};
use casual_doc_ooxml::DocxPackage;

use crate::body::EmbeddedRel;
use crate::media::MediaSource;
use crate::numbering::Numbering;
use crate::report::{Reporter, SourceRetention, WholePartDisposition};
use crate::styles::Styles;

/// Resolves one streamed XML character/general-reference event.
///
/// `quick-xml` emits `&amp;`, `&#x2014;`, and the other XML references as
/// `Event::GeneralRef`, separate from the surrounding `Event::Text` chunks.
/// Keeping the resolver here gives body text, OMML fallbacks, and document
/// properties one strict policy: the five predefined XML entities and numeric
/// character references are accepted; undeclared general entities are rejected.
fn decode_xml_reference(
    reference: &quick_xml::events::BytesRef<'_>,
) -> Result<String, ImportError> {
    let name = reference.decode().map_err(|_| ImportError::MalformedXml)?;
    let mut encoded = String::with_capacity(name.len() + 2);
    encoded.push('&');
    encoded.push_str(&name);
    encoded.push(';');
    Ok(quick_xml::escape::unescape(&encoded)
        .map_err(|_| ImportError::MalformedXml)?
        .into_owned())
}

/// A minimal well-formed main document, used as the last rung of the recovery
/// ladder.
///
/// When nothing at all can be read from a damaged main document, this is what is
/// imported in its place, so the open still produces a document with the file's
/// properties, styles and headers around it. An empty document plus a report
/// saying the text could not be read is a worse document than the original and a
/// far better answer than an error dialog: the reader can see what the file still
/// holds, and is told plainly not to save over it.
const EMPTY_MAIN_DOCUMENT: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body/></w:document>"#;

/// Resolves a definition part's parse outcome, substituting `fallback` and
/// recording a repair when recovering.
///
/// `Err` still propagates on the strict path, and still propagates when the
/// failure is a resource bound rather than damage: a limit is a refusal on
/// purpose, and recovering past one would turn a defence into a suggestion.
fn recover_part<T>(
    outcome: Result<T, ImportError>,
    reporter: &mut Reporter,
    role: PartRole,
    fallback: impl FnOnce() -> T,
) -> Result<T, ImportError> {
    match outcome {
        Ok(value) => Ok(value),
        Err(error) if reporter.may_recover() && is_damage(&error) => {
            reporter.repair(Repair::in_role(RepairKind::PartUnparsable, role));
            Ok(fallback())
        }
        Err(error) => Err(error),
    }
}

/// Whether an import failure describes **damaged input** rather than a bound this
/// engine refuses to cross or an internal invariant it must not violate.
///
/// Only damage is recoverable, and the distinction is the whole safety property
/// of the recovering path:
///
/// - `MalformedXml` and `Package` are facts about the bytes a producer wrote.
///   They are what recovery exists for.
/// - `LimitExceeded` is a resource bound, and `InvalidConfig` is the host's own
///   configuration. Recovering past either would make a defence advisory —
///   a 4 GB expansion is still refused, and still says why.
/// - `Model` and `Disposition` are internal invariants of this engine
///   (`35-DISPOSITION-TAXONOMY.md` requires an illegal disposition to fail the
///   import rather than be reported). A recovery that swallowed one would hide a
///   defect here behind a sentence blaming the file.
const fn is_damage(error: &ImportError) -> bool {
    match error {
        ImportError::MalformedXml | ImportError::Package(_) => true,
        ImportError::InvalidConfig
        | ImportError::LimitExceeded { .. }
        | ImportError::Model(_)
        | ImportError::Disposition(_) => false,
    }
}

/// Repairs one part's bytes in place, stamping every repair with the role so the
/// report can say *which* part of the document was damaged.
fn repair_part(bytes: &mut Vec<u8>, role: PartRole, repairs: &mut Vec<Repair>) {
    let Some((repaired, found)) = xml_repair::repair(bytes) else {
        return;
    };
    repairs.extend(found.into_iter().map(|repair| Repair {
        role: Some(role),
        ..repair
    }));
    *bytes = repaired;
}

/// Reads one package part, or records it missing and yields `None` when
/// recovering.
///
/// A relationship pointing at a part the package does not contain is the single
/// most common shape of real-world damage short of a broken ZIP: a producer wrote
/// the relationship and then failed to write the part, or a repair tool dropped
/// it. Every part reached this way is optional by construction — the importer
/// already handles `None` for each of them, because a document need not have
/// styles, a theme, footnotes or a header — so there was never a reason for an
/// absent one to refuse the document, only an absent branch.
fn recover_read(
    package: &mut DocxPackage<'_>,
    part: &str,
    role: PartRole,
    recover: bool,
    repairs: &mut Vec<Repair>,
) -> Result<Option<Vec<u8>>, ImportError> {
    match package.read_part(part) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if recover => {
            let _ = error;
            repairs.push(Repair::in_part(RepairKind::PartMissing, role, part));
            Ok(None)
        }
        Err(error) => Err(ImportError::Package(error)),
    }
}

/// Resolves one extra part and its own relationships, or records it missing and
/// yields `None` when recovering.
fn recover_sources(
    package: &mut DocxPackage<'_>,
    part: &str,
    role: PartRole,
    recover: bool,
    repairs: &mut Vec<Repair>,
) -> Result<Option<PartSources>, ImportError> {
    match resolve_part_sources(package, part) {
        Ok(sources) => Ok(Some(sources)),
        Err(error) if recover && is_damage(&error) => {
            repairs.push(Repair::in_part(RepairKind::PartMissing, role, part));
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

/// The result of importing a main document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Import {
    /// The normalized v1 document.
    pub document: Document,
    /// The compatibility report.
    pub report: CompatibilityReport,
    /// The preservation ledger licensing every `preserved` retention outcome in
    /// [`Import::report`] (`35-DISPOSITION-TAXONOMY.md`). A caller can audit a
    /// preservation claim through it instead of trusting the word `preserved`;
    /// `CompatibilityReport::validate` has already checked that every claim
    /// resolves, and an import whose claims did not resolve fails rather than
    /// reporting.
    pub ledger: PreservationLedger,
    /// What a best-effort open had to repair to produce this document, in words
    /// a reader shares ([`ImportConfig::recover`]).
    ///
    /// Empty whenever the source was well-formed, and **always** empty when
    /// `recover` is off — a strict import that succeeds repaired nothing, and a
    /// strict import that fails returns [`ImportError`] rather than a document.
    /// A non-empty report is the one fact a reader must see before saving over
    /// the original file.
    pub recovery: RecoveryReport,
    /// Source retained for round-trip; `Some` only in `Retention` mode.
    pub retained_source: Option<RetainedSource>,
    /// Opaque part side-table (P1F-2): admitted parts the semantic model does
    /// not consume, carried verbatim so the semantic writer preserves them.
    /// Populated by [`import_package`] (both modes); empty for the XML-only
    /// [`import_main_document_xml`] entry point (no package available).
    pub retained_parts: RetainedParts,
    /// Package part names referenced by an embedded-object node (chart / diagram
    /// / OLE). These parts are still byte-preserved by the side-table, but their
    /// referencing relationship is emitted by the writer from the node — so the
    /// side-table must NOT re-add it as an orphan (that would double-emit it).
    pub(crate) embedded_part_names: std::collections::BTreeSet<String>,
    /// What reading each referenced chart part produced, in document order.
    ///
    /// Carried out of the semantic pass because the *disposition* of a chart
    /// construct cannot be decided here: its retention outcome is "the opaque
    /// side-table holds the part", and the ledger record that licenses that claim
    /// is minted later, by `build_retained_parts`. `import_package` joins the two.
    pub(crate) chart_parts: Vec<crate::chart::ChartPartOutcome>,
}

/// Imports the main document of an admitted DOCX package into a v1 document,
/// resolving the styles part through the main document's relationship graph.
pub fn import_package(
    package: &mut DocxPackage<'_>,
    config: ImportConfig,
) -> Result<Import, ImportError> {
    // Repairs this driver records itself, as distinct from the ones the parsers
    // record through the reporter. Both end up in one `RecoveryReport`.
    let recover = config.recover;
    let mut repairs: Vec<Repair> = Vec::new();

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
    let font_table_part = related_part("/fontTable");
    let theme_part = related_part("/theme");
    let settings_part = related_part("/settings");
    let footnotes_part = related_part("/footnotes");
    let endnotes_part = related_part("/endnotes");
    let comments_part = related_part("/comments");

    // The set of admitted part names the semantic import consumes. Every OTHER
    // admitted part (docProps, glossary, customXml, embeddings, charts, ...) is
    // regenerated away on a semantic edit→save, so the package-manifest
    // disposition pass below reports it as dropped (F2, `44-COVERAGE-GAP-AUDIT`).
    // The main document plus each part reached through a resolved main-document
    // relationship (and, transitively, the images inside extra parts) is consumed.
    let mut consumed: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    consumed.insert(main_part.clone());
    for part in [
        styles_part.as_ref(),
        numbering_part.as_ref(),
        font_table_part.as_ref(),
        theme_part.as_ref(),
        settings_part.as_ref(),
        footnotes_part.as_ref(),
        endnotes_part.as_ref(),
        comments_part.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        consumed.insert(part.clone());
    }

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
    for source in &media_sources {
        consumed.insert(source.part_name.clone());
    }
    // An image relationship whose target is not in the package. The model keeps
    // the reference (no bytes are decoded at import), so nothing here fails and
    // nothing here used to be said — the drawing simply came out as a frame with
    // no picture in it. `casual-doc-io` reports the part when it tries to read
    // the bytes, which covers the product path but not a caller that imports
    // without resources, so the recovery report names it where it is first
    // observable.
    if recover {
        let admitted: std::collections::BTreeSet<&str> = package
            .entries()
            .iter()
            .map(|entry| entry.part_name.as_str())
            .collect();
        for source in &media_sources {
            if !admitted.contains(source.part_name.as_str()) {
                repairs.push(Repair::in_part(
                    RepairKind::PartMissing,
                    PartRole::Media,
                    &source.part_name,
                ));
            }
        }
    }

    let mut document_bytes = match recover_read(
        package,
        &main_part,
        PartRole::MainDocument,
        recover,
        &mut repairs,
    )? {
        Some(bytes) => bytes,
        None => {
            // The main document itself is unreadable. Everything else in the
            // package still is, so the open continues around an empty body
            // rather than refusing: the reader gets the properties, the styles
            // and the headers, and a sentence saying the text is gone.
            repairs.push(Repair::in_role(
                RepairKind::BodyUnreadable,
                PartRole::MainDocument,
            ));
            EMPTY_MAIN_DOCUMENT.to_vec()
        }
    };
    let styles_bytes = match styles_part {
        Some(part) => recover_read(package, &part, PartRole::Styles, recover, &mut repairs)?,
        None => None,
    };
    let numbering_bytes = match numbering_part {
        Some(part) => recover_read(package, &part, PartRole::Numbering, recover, &mut repairs)?,
        None => None,
    };
    // The font table plus its own relationships (embedded `.odttf` fonts resolve
    // through `fontTable.xml.rels`, not the document's).
    let (font_table_bytes, font_table_rels) = match font_table_part {
        Some(part) => {
            match recover_read(package, &part, PartRole::FontTable, recover, &mut repairs)? {
                None => (None, std::collections::BTreeMap::new()),
                Some(bytes) => {
                    let relationships = match package.part_relationships(&part) {
                        Ok(relationships) => relationships,
                        Err(error) if recover => {
                            let _ = error;
                            repairs.push(Repair::in_part(
                                RepairKind::PartUnparsable,
                                PartRole::FontTable,
                                &part,
                            ));
                            Vec::new()
                        }
                        Err(error) => return Err(ImportError::Package(error)),
                    };
                    let font_rels: std::collections::BTreeMap<String, String> = relationships
                        .iter()
                        .filter(|relationship| relationship.relationship_type.ends_with("/font"))
                        .filter_map(|relationship| {
                            Some((relationship.id.clone(), relationship.resolved_part.clone()?))
                        })
                        .collect();
                    (Some(bytes), font_rels)
                }
            }
        }
        None => (None, std::collections::BTreeMap::new()),
    };
    for part in font_table_rels.values() {
        consumed.insert(part.clone());
    }
    let theme_bytes = match theme_part {
        Some(part) => recover_read(package, &part, PartRole::Theme, recover, &mut repairs)?,
        None => None,
    };
    let settings_bytes = match settings_part {
        Some(part) => recover_read(package, &part, PartRole::Settings, recover, &mut repairs)?,
        None => None,
    };
    // Each extra part (notes, headers, footers) carries its own image and
    // external-hyperlink relationships, so images and links inside it are modeled.
    let mut footnotes = match footnotes_part {
        Some(part) => recover_sources(package, &part, PartRole::Footnotes, recover, &mut repairs)?,
        None => None,
    };
    let mut endnotes = match endnotes_part {
        Some(part) => recover_sources(package, &part, PartRole::Endnotes, recover, &mut repairs)?,
        None => None,
    };
    let mut comments = match comments_part {
        Some(part) => {
            let sources =
                recover_sources(package, &part, PartRole::Comments, recover, &mut repairs)?;
            // Comment companion parts (P1F-10): reply threading
            // (`commentsExtended.xml`), durable ids (`commentsIds.xml`), and
            // collaborator identity (`people.xml`). They hang off the main-
            // document relationships, with a well-known part-name fallback for
            // producers that omit the relationship. Reading them here (and marking
            // them consumed) lets `build_comments` join threading/identity onto the
            // comments and keeps the disposition pass / side-table from double-
            // handling them.
            let extended_part =
                related_or_wellknown(package, "/commentsExtended", "word/commentsExtended.xml");
            let ids_part = related_or_wellknown(package, "/commentsIds", "word/commentsIds.xml");
            let people_part = related_or_wellknown(package, "/people", "word/people.xml");
            for companion in [
                extended_part.as_ref(),
                ids_part.as_ref(),
                people_part.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                consumed.insert(companion.clone());
            }
            let mut sources = sources;
            if let (Some(sources), Some(companion)) = (sources.as_mut(), extended_part) {
                sources.comments_extended = recover_read(
                    package,
                    &companion,
                    PartRole::Comments,
                    recover,
                    &mut repairs,
                )?;
            }
            if let (Some(sources), Some(companion)) = (sources.as_mut(), ids_part) {
                sources.comments_ids = recover_read(
                    package,
                    &companion,
                    PartRole::Comments,
                    recover,
                    &mut repairs,
                )?;
            }
            if let (Some(sources), Some(companion)) = (sources.as_mut(), people_part) {
                sources.people = recover_read(
                    package,
                    &companion,
                    PartRole::Comments,
                    recover,
                    &mut repairs,
                )?;
            }
            sources
        }
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
        consumed.insert(part.clone());
        if let Some(sources) =
            recover_sources(package, &part, PartRole::Header, recover, &mut repairs)?
        {
            header_parts.push((relationship_id, sources));
        }
    }
    let mut footer_parts = Vec::new();
    for (relationship_id, part) in footer_refs {
        consumed.insert(part.clone());
        if let Some(sources) =
            recover_sources(package, &part, PartRole::Footer, recover, &mut repairs)?
        {
            footer_parts.push((relationship_id, sources));
        }
    }
    // Images referenced from inside the extra parts (notes, headers, footers,
    // comments) are consumed transitively while those parts are parsed.
    for part in [footnotes.as_ref(), endnotes.as_ref(), comments.as_ref()]
        .into_iter()
        .flatten()
    {
        for image in &part.images {
            consumed.insert(image.part_name.clone());
        }
    }
    for (_, part) in header_parts.iter().chain(footer_parts.iter()) {
        for image in &part.images {
            consumed.insert(image.part_name.clone());
        }
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

    // Embedded-object relationships (chart / SmartArt diagram / OLE) and alt-chunk
    // relationships (`aFChunk`), resolved through the main-document relationship
    // graph (r:id -> part), so a `c:chart`/`dgm:relIds`/`o:OLEObject`/`w:altChunk`
    // reference resolves to a first-class node instead of being reported-dropped.
    // The referenced parts stay preserved by the side-table but are un-orphaned
    // below (their rel is emitted by the writer from the node, not re-added as an
    // orphan).
    let embedded_index = embedded_relationships(package.main_document_relationships());

    // The chart parts any surface can reference — the body through the document's
    // relationships, a header, footer, note or comment through its own (HF-266) —
    // each with its OWN relationships (a chart's workbook, colour style and chart
    // style hang off `word/charts/_rels/chartN.xml.rels`, not the document's).
    // Read here because only this entry point has a package; the parts stay in the
    // opaque side-table and the embedded workbook is never opened (`docs/155`
    // §5.2).
    let mut chart_part_sources: std::collections::BTreeMap<String, crate::chart::ChartPartSource> =
        std::collections::BTreeMap::new();
    let running_parts = footnotes
        .iter()
        .chain(endnotes.iter())
        .chain(comments.iter())
        .chain(header_parts.iter().map(|(_, part)| part))
        .chain(footer_parts.iter().map(|(_, part)| part));
    let chart_part_names: Vec<String> = embedded_index
        .values()
        .chain(running_parts.flat_map(|part| part.embedded.values()))
        .filter(|rel| rel.relationship_type.ends_with("/chart"))
        .map(|rel| rel.part_name.clone())
        .collect();
    for part_name in chart_part_names {
        if chart_part_sources.contains_key(&part_name) {
            continue;
        }
        let Some(bytes) =
            recover_read(package, &part_name, PartRole::Chart, recover, &mut repairs)?
        else {
            continue;
        };
        let rels = match package.part_relationships(&part_name) {
            Ok(relationships) => relationships,
            Err(error) if recover => {
                let _ = error;
                repairs.push(Repair::in_part(
                    RepairKind::PartUnparsable,
                    PartRole::Chart,
                    &part_name,
                ));
                Vec::new()
            }
            Err(error) => return Err(ImportError::Package(error)),
        }
        .iter()
        .filter(|relationship| !relationship.id.is_empty())
        .filter_map(|relationship| {
            let part = relationship.resolved_part.clone()?;
            Some((
                relationship.id.clone(),
                EmbeddedRel {
                    relationship_type: relationship.relationship_type.clone(),
                    part_name: part,
                },
            ))
        })
        .collect();
        chart_part_sources.insert(part_name, crate::chart::ChartPartSource { bytes, rels });
    }

    // Two shapes of damage no parse error can report, because the parse they
    // produce SUCCEEDS: a main part whose root is not `w:document`, and a
    // `w:document` with no `w:body`. Both used to open as a one-block document
    // with an empty report — a silent acceptance, which is the failure this whole
    // path exists to prevent, because a reader who is told nothing saves over the
    // original. Observed on the bytes rather than inside the parser, for the
    // reason `xml_repair::main_document_shape` records.
    if recover {
        match xml_repair::main_document_shape(&document_bytes) {
            xml_repair::MainDocumentShape::Wordprocessing => {}
            xml_repair::MainDocumentShape::NoBody => repairs.push(Repair::in_role(
                RepairKind::BodyMissing,
                PartRole::MainDocument,
            )),
            xml_repair::MainDocumentShape::NotWordprocessing(root) => repairs.push(
                Repair::in_role(RepairKind::NotWordprocessingMl, PartRole::MainDocument)
                    .with_detail(&format!("its root element is <{root}>")),
            ),
            xml_repair::MainDocumentShape::Empty => repairs.push(Repair::in_role(
                RepairKind::BodyUnreadable,
                PartRole::MainDocument,
            )),
        }
    }

    // The recovery ladder.
    //
    // Rung 1 is the strict read, byte for byte what a non-recovering open does,
    // and it runs first even when recovering. That ordering is the safety
    // property of the whole feature: a healthy document takes exactly the path it
    // took before this existed, repairs nothing, and reports nothing, so no
    // amount of leniency below can change what a well-formed file imports to.
    //
    // Rung 2 repairs the content parts' bytes (`crate::xml_repair`) and reads
    // everything tolerantly. The two halves do different work and both are
    // needed: the byte repair recovers damage a stream reader cannot get past at
    // all — a DTD, a mislabelled encoding, a stray end tag, a file cut off
    // mid-element — while the tolerant read recovers the head of a part whose
    // damage survives repair, and drops a definition part whole.
    //
    // Rung 3 reads an empty main document with the file's other parts around it,
    // for the case where the main document yields nothing at all. An empty
    // document carrying the file's properties, styles and headers, with a report
    // saying the text could not be read, is the floor; below it there is nothing
    // to show, and this engine never gets there with a package in hand.
    let mut import = match import_with_sources(
        &document_bytes,
        styles_bytes.as_deref(),
        numbering_bytes.as_deref(),
        font_table_bytes.as_deref(),
        &font_table_rels,
        theme_bytes.as_deref(),
        settings_bytes.as_deref(),
        footnotes.as_ref(),
        endnotes.as_ref(),
        &header_parts,
        &footer_parts,
        comments.as_ref(),
        &media_sources,
        &hyperlink_rels,
        &embedded_index,
        &chart_part_sources,
        ImportConfig {
            recover: false,
            ..config
        },
    ) {
        Ok(import) => import,
        Err(error) if recover && is_damage(&error) => {
            // Byte-repair the parts that hold **content**, and only those. A
            // damaged content part's surviving text is worth recovering: it is
            // the document, and what the damage took is reported.
            //
            // The five definition parts (styles, numbering, theme, settings, the
            // font table) are deliberately NOT repaired, and are dropped whole by
            // `recover_part` instead. A definition table is not content, it is a
            // function applied to content, and half of one is worse than none: a
            // `w:style` truncated through its property list still resolves, and
            // then silently applies the wrong formatting to text that looks
            // right. Dropping it yields a statement a reader can act on — "the
            // style definitions are damaged, so text is shown with default
            // formatting" — where a partial table yields a document that is
            // subtly wrong and says only that a tag was left open.
            repair_part(&mut document_bytes, PartRole::MainDocument, &mut repairs);
            if let Some(part) = footnotes.as_mut() {
                repair_part(&mut part.xml, PartRole::Footnotes, &mut repairs);
            }
            if let Some(part) = endnotes.as_mut() {
                repair_part(&mut part.xml, PartRole::Endnotes, &mut repairs);
            }
            if let Some(part) = comments.as_mut() {
                repair_part(&mut part.xml, PartRole::Comments, &mut repairs);
            }
            for (_, part) in &mut header_parts {
                repair_part(&mut part.xml, PartRole::Header, &mut repairs);
            }
            for (_, part) in &mut footer_parts {
                repair_part(&mut part.xml, PartRole::Footer, &mut repairs);
            }
            match import_with_sources(
                &document_bytes,
                styles_bytes.as_deref(),
                numbering_bytes.as_deref(),
                font_table_bytes.as_deref(),
                &font_table_rels,
                theme_bytes.as_deref(),
                settings_bytes.as_deref(),
                footnotes.as_ref(),
                endnotes.as_ref(),
                &header_parts,
                &footer_parts,
                comments.as_ref(),
                &media_sources,
                &hyperlink_rels,
                &embedded_index,
                &chart_part_sources,
                config,
            ) {
                Ok(import) => import,
                Err(inner) if is_damage(&inner) => {
                    repairs.push(Repair::in_role(
                        RepairKind::BodyUnreadable,
                        PartRole::MainDocument,
                    ));
                    document_bytes = EMPTY_MAIN_DOCUMENT.to_vec();
                    import_with_sources(
                        &document_bytes,
                        styles_bytes.as_deref(),
                        numbering_bytes.as_deref(),
                        font_table_bytes.as_deref(),
                        &font_table_rels,
                        theme_bytes.as_deref(),
                        settings_bytes.as_deref(),
                        footnotes.as_ref(),
                        endnotes.as_ref(),
                        &header_parts,
                        &footer_parts,
                        comments.as_ref(),
                        &media_sources,
                        &hyperlink_rels,
                        &embedded_index,
                        &chart_part_sources,
                        config,
                    )?
                }
                Err(inner) => return Err(inner),
            }
        }
        Err(error) => return Err(error),
    };

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
            let Some(bytes) = recover_read(
                package,
                &name,
                PartRole::RetainedPart,
                recover,
                &mut repairs,
            )?
            else {
                continue;
            };
            total = total.saturating_add(bytes.len());
            if total > config.max_text_bytes {
                return Err(ImportError::LimitExceeded {
                    limit: "retained_bytes",
                });
            }
            retained.parts.insert(name, bytes);
        }
        // The byte floor just grew from the main document to every admitted part,
        // so the snapshot record now accounts for the whole package. The record is
        // what licenses every `preserved` finding in this mode, and a caller
        // auditing the claim must see the real figure.
        import.ledger.restate_source_snapshot(total);
    }

    // Document properties (`docProps/{core,app,custom}.xml`). Their relationships
    // hang off the PACKAGE root (`_rels/.rels`), not the main document's, so they
    // are discovered and parsed here rather than in `import_with_sources`; the
    // well-known part names are a fallback for producers that omit the
    // relationship. Unmapped property fields fold into the compatibility report.
    // The discovered parts are recorded as consumed so the disposition pass below
    // does not report them as dropped (they are modeled and regenerated on write).
    let (sources, docprop_parts) = discover_docprops(package, recover, &mut repairs)?;
    for part in docprop_parts {
        consumed.insert(part);
    }
    if !sources.is_empty() {
        // The property parts are covered by the same byte floor as every other
        // part, so they share the main pass's retention scope rather than
        // re-deciding it.
        let retention = match config.mode {
            ImportMode::Retention => SourceRetention::Snapshot,
            ImportMode::Semantic => SourceRetention::Regenerated,
        };
        let mut reporter = if recover {
            Reporter::recovering(retention)
        } else {
            Reporter::new(retention)
        };
        let parsed = recover_part(
            metadata::parse(&sources, config, &mut reporter),
            &mut reporter,
            PartRole::DocumentProperties,
            || None,
        )?;
        if let Some(properties) = parsed {
            import.document = import
                .document
                .with_properties(properties)
                .map_err(ImportError::Model)?;
        }
        repairs.extend(reporter.take_repairs());
        let docprops = reporter.into_report(&mut import.ledger);
        import.report.merge(docprops);
    }

    // Package-manifest disposition pass (F2, `44-COVERAGE-GAP-AUDIT`) + opaque
    // part side-table (P1F-2). Every admitted part the semantic model did not
    // consume is enumerated. Pure OPC plumbing (the content-type manifest and
    // `_rels` parts) is regenerated deterministically from the model, so it is
    // not a data-loss disposition and is skipped here (an owned `_rels` is
    // instead carried with its part, below).
    //
    // Each non-plumbing unconsumed part is either:
    //   * preserved verbatim via the side-table (glossary, embeddings, charts,
    //     customXml, webSettings, thumbnail, stylesWithEffects, comment
    //     companions, ...) — reported `preserved` on both paths (the semantic
    //     writer re-emits it; Retention's byte floor already keeps it); or
    //   * a digital signature (`_xmlsignatures/*` or a signature content type) —
    //     deliberately NOT preserved on the semantic path, because editing
    //     invalidates a signature. Retention is refused for a security reason
    //     rather than merely declined, which is `35`'s `blocked` ("refused by
    //     security or resource policy; nothing is trusted or stored") rather than
    //     `not-retained` ("intentionally and reportably dropped"). Retention
    //     mode's byte floor still keeps the bytes verbatim, so it is `preserved`
    //     there.
    let projected_chart_parts: std::collections::BTreeSet<String> = import
        .chart_parts
        .iter()
        .filter(|outcome| outcome.projected)
        .map(|outcome| outcome.part_name.clone())
        .collect();
    let (retained_parts, dispositions) = build_retained_parts(
        package,
        &consumed,
        &import.embedded_part_names,
        &projected_chart_parts,
        config,
        &mut import.ledger,
    )?;
    import.retained_parts = retained_parts;
    // A chart part whose projection succeeded is enumerated by CONSTRUCT rather
    // than as one line about a part: `docs/155` §6.2. A fully-projected chart is
    // `mapped` + `preserved`, which `35` says is never a finding, so it raises
    // nothing at all; a partly-projected one is `degraded` + `preserved` once per
    // construct the projection did not represent, each charged to the part and
    // each licensed by the part's own opaque-part ledger record.
    let chart_constructs = chart_construct_dispositions(&import.chart_parts, &import.ledger);
    import.report.add_part_dispositions(dispositions);
    import.report.add_chart_constructs(chart_constructs);
    import
        .report
        .validate(&import.ledger)
        .map_err(ImportError::Disposition)?;
    // The driver's own repairs join the ones the parsers recorded through the
    // reporter, so one report describes the whole open.
    import
        .recovery
        .absorb(RecoveryReport::from_repairs(repairs));

    Ok(import)
}

/// Enumerates every admitted, non-plumbing part the semantic model did not
/// consume, building the opaque side-table (preserved parts + the root/document
/// relationships that keep them reachable) and the matching whole-part
/// dispositions. Digital signatures are excluded from the side-table and
/// reported dropped on the semantic path.
///
/// The side-table's aggregate byte size is bounded by `max_text_bytes` (the same
/// ceiling the Retention byte floor uses), so a hostile package cannot inflate
/// retained memory without limit.
fn build_retained_parts(
    package: &mut DocxPackage<'_>,
    consumed: &std::collections::BTreeSet<String>,
    embedded_part_names: &std::collections::BTreeSet<String>,
    projected_chart_parts: &std::collections::BTreeSet<String>,
    config: ImportConfig,
    ledger: &mut PreservationLedger,
) -> Result<(RetainedParts, Vec<WholePartDisposition>), ImportError> {
    // The admitted part names (sorted), and the subset the model does not
    // consume and that is not pure OPC plumbing — the candidate opaque parts.
    let admitted: std::collections::BTreeSet<String> = package
        .entries()
        .iter()
        .map(|entry| entry.part_name.clone())
        .collect();
    let unconsumed: Vec<String> = package
        .entries()
        .iter()
        .map(|entry| entry.part_name.clone())
        .filter(|name| !is_package_plumbing(name) && !consumed.contains(name))
        .collect();

    // The set of parts whose referencing relationship the side-table re-adds as
    // an orphan (non-signature, and NOT referenced by a first-class embedded-
    // object node). A part referenced by a node is still byte-preserved (it stays
    // in `unconsumed`), but its relationship is emitted by the writer FROM the
    // node — so re-adding it here too would double-emit the same relationship.
    let preserved_names: std::collections::BTreeSet<String> = unconsumed
        .iter()
        .filter(|name| !opaque::is_signature_part(name, package.content_type(name)))
        .filter(|name| !embedded_part_names.contains(name.as_str()))
        .cloned()
        .collect();

    let mut parts = Vec::new();
    let mut dispositions = Vec::new();
    let mut total = 0_usize;
    for name in &unconsumed {
        let content_type = package.content_type(name).map(str::to_owned);
        let is_signature = opaque::is_signature_part(name, content_type.as_deref());
        let part = PartDisposition {
            part_name: name.clone(),
            content_type: content_type.clone(),
        };
        if is_signature {
            // Retention mode's byte floor keeps the signature bytes verbatim, so
            // the snapshot record licenses a `preserved` claim. On the semantic
            // path retention is REFUSED — a signature over regenerated content
            // would assert an integrity nobody checked — which is `blocked`, not
            // `not-retained`: nothing about it is trusted or stored.
            let (disposition, ledger_id) = match ledger.source_snapshot() {
                Some(snapshot) => (Disposition::OmittedPreserved, Some(snapshot)),
                None => (Disposition::OmittedBlocked, None),
            };
            dispositions.push((part, disposition, ledger_id));
            continue;
        }
        let bytes = package.read_part(name).map_err(ImportError::Package)?;
        // The part's own `_rels` companion (a chart's rels to its embeddings, a
        // customXml item's rels to its itemProps, ...): carried verbatim so the
        // parts it references stay reachable.
        let rels_name = relationship_part_name(name);
        let rels = if admitted.contains(&rels_name) {
            let rels_bytes = package
                .read_part(&rels_name)
                .map_err(ImportError::Package)?;
            Some(RetainedRels {
                part_name: rels_name,
                bytes: rels_bytes,
            })
        } else {
            None
        };
        total = total
            .saturating_add(bytes.len())
            .saturating_add(rels.as_ref().map_or(0, |rels| rels.bytes.len()));
        if total > config.max_text_bytes {
            return Err(ImportError::LimitExceeded {
                limit: "retained_bytes",
            });
        }
        // The side-table carries this part verbatim through the semantic writer,
        // so its `preserved` claim has a record of its own on BOTH paths — a
        // caller can audit the byte count rather than take the word for it.
        let retained_bytes = bytes
            .len()
            .saturating_add(rels.as_ref().map_or(0, |rels| rels.bytes.len()));
        let ledger_id = ledger.record_opaque_part(name, retained_bytes);
        // A chart part the projection READ is no longer an `omitted` part: it is
        // `mapped` or `degraded` (`docs/155` §6.2), and `35` says a `mapped`
        // construct is never enumerated while a `degraded` one is enumerated by
        // the construct that was lost, not by the part. Both are emitted by
        // `chart_construct_dispositions`, so the whole-part row is suppressed here
        // rather than contradicted there. The ledger record is still created, so
        // the preservation claim those findings make still resolves.
        if !projected_chart_parts.contains(name) {
            dispositions.push((part, Disposition::OmittedPreserved, Some(ledger_id)));
        }
        parts.push(RetainedPart {
            part_name: name.clone(),
            content_type,
            bytes,
            rels,
        });
    }

    // Root/document relationships that target a preserved part, re-added on write
    // (with a fresh id) so the part stays reachable. Signature relationships are
    // excluded (their targets are not preserved anyway). Body-referenced parts
    // (charts/embeddings) keep their relationship too, but the regenerated body
    // no longer names the id, so they survive as orphaned bytes (Tier-3
    // re-linking is out of scope).
    let mut relationships = Vec::new();
    // A damaged `_rels/.rels` costs the side-table the relationships that keep
    // preserved parts reachable on write. That is a real loss, reported by the
    // whole-part dispositions below, and it is not a reason to refuse: the
    // document's own text does not travel through this index.
    let root_rels = match package.part_relationships("") {
        Ok(relationships) => relationships,
        Err(error) if config.recover => {
            let _ = error;
            Vec::new()
        }
        Err(error) => return Err(ImportError::Package(error)),
    };
    collect_referencing_rels(
        &root_rels,
        opaque::RelationshipOwner::Root,
        &preserved_names,
        &mut relationships,
    );
    let document_rels: Vec<casual_doc_ooxml::DocumentRelationship> =
        package.main_document_relationships().to_vec();
    collect_referencing_rels(
        &document_rels,
        opaque::RelationshipOwner::Document,
        &preserved_names,
        &mut relationships,
    );
    // Deterministic order independent of source id/enumeration order.
    relationships.sort_by(|left, right| {
        (
            owner_rank(left.owner),
            &left.relationship_type,
            &left.target,
        )
            .cmp(&(
                owner_rank(right.owner),
                &right.relationship_type,
                &right.target,
            ))
    });

    Ok((
        RetainedParts {
            parts,
            relationships,
        },
        dispositions,
    ))
}

/// Builds one `degraded` + `preserved` finding per construct a successful chart
/// projection did not represent (`docs/155` §6.2).
///
/// A chart part whose projection captured everything produces **nothing**: `35`
/// makes `mapped` unreachable in a report, and twenty rows that describe no
/// additional loss are the HF-174 defect the `Reporter` doc comment warns about.
///
/// Every finding claims `preserved`, which is true for a reason worth stating: the
/// part's bytes are in the opaque side-table with their own ledger record, and
/// export copies them. So a construct this projection did not understand is still
/// in the saved file — which is the whole of `docs/155` §6.1 and the thing a
/// convert-through-an-intermediate-model pipeline cannot say.
fn chart_construct_dispositions(
    outcomes: &[crate::chart::ChartPartOutcome],
    ledger: &PreservationLedger,
) -> Vec<(PartDisposition, String, Disposition, Option<LedgerId>)> {
    let mut entries = Vec::new();
    for outcome in outcomes {
        let ledger_id = ledger.opaque_part_record(&outcome.part_name);
        // Without a record the retention half of the claim is not evidenced, and
        // `35` says an unevidenced `preserved` must fail the import rather than be
        // reported. Reporting the weaker, true disposition keeps a bookkeeping
        // mismatch from turning into a document that will not open.
        let (degraded, omitted) = match ledger_id {
            Some(_) => (
                Disposition::DegradedPreserved,
                Disposition::OmittedPreserved,
            ),
            None => (
                Disposition::DegradedNotRetained,
                Disposition::OmittedNotRetained,
            ),
        };
        let part = || PartDisposition {
            part_name: outcome.part_name.clone(),
            content_type: None,
        };
        if !outcome.projected {
            // No projection, so the part keeps today's whole-part `omitted` +
            // `preserved` row. One construct is still worth naming beside it: the
            // chart FAMILY that was out of scope, because "we do not model
            // `bar3DChart`" is information the part row cannot carry and is the
            // actionable half of the finding.
            if let Some(family) = &outcome.out_of_scope_family {
                entries.push((part(), family.clone(), omitted, ledger_id));
            }
            continue;
        }
        for construct in &outcome.unconsumed {
            entries.push((part(), construct.clone(), degraded, ledger_id));
        }
    }
    entries
}

/// Appends every relationship in `relationships` that targets a preserved part
/// (and is not signature machinery) as a [`RetainedRelationship`] owned by
/// `owner`.
fn collect_referencing_rels(
    relationships: &[casual_doc_ooxml::DocumentRelationship],
    owner: opaque::RelationshipOwner,
    preserved_names: &std::collections::BTreeSet<String>,
    out: &mut Vec<RetainedRelationship>,
) {
    for relationship in relationships {
        if opaque::is_signature_relationship(&relationship.relationship_type) {
            continue;
        }
        let Some(resolved) = &relationship.resolved_part else {
            continue;
        };
        if preserved_names.contains(resolved) {
            out.push(RetainedRelationship {
                owner,
                relationship_type: relationship.relationship_type.clone(),
                target: relationship.target.clone(),
                external: relationship.target_mode == casual_doc_ooxml::TargetMode::External,
            });
        }
    }
}

/// Stable sort key for a relationship owner (root before document).
const fn owner_rank(owner: opaque::RelationshipOwner) -> u8 {
    match owner {
        opaque::RelationshipOwner::Root => 0,
        opaque::RelationshipOwner::Document => 1,
    }
}

/// The `_rels` part name carrying a part's relationships, e.g.
/// `word/charts/chart1.xml` -> `word/charts/_rels/chart1.xml.rels`.
fn relationship_part_name(part_name: &str) -> String {
    match part_name.rsplit_once('/') {
        Some((directory, file)) => format!("{directory}/_rels/{file}.rels"),
        None => format!("_rels/{part_name}.rels"),
    }
}

/// Whether a part is pure OPC package plumbing — the content-type manifest or a
/// relationships part. The semantic writer regenerates these deterministically
/// from the model, so they are not whole-part data-loss dispositions.
fn is_package_plumbing(part_name: &str) -> bool {
    part_name == "[Content_Types].xml"
        || part_name.starts_with("_rels/")
        || part_name.contains("/_rels/")
}

/// Whether a main-document relationship type points at a part an embedded-object
/// node can reference: a chart, a SmartArt diagram's data/layout/quick-style/
/// colors, or an OLE embedding (`oleObject`/`package`).
fn is_embedded_object_rel(relationship_type: &str) -> bool {
    matches!(
        relationship_type.rsplit('/').next(),
        Some(
            "chart"
                | "diagramData"
                | "diagramLayout"
                | "diagramQuickStyle"
                | "diagramColors"
                | "oleObject"
                | "package"
        )
    )
}

/// Whether a main-document relationship type points at an `w:altChunk` aggregated
/// external content part (`.../aFChunk`).
fn is_alt_chunk_rel(relationship_type: &str) -> bool {
    matches!(relationship_type.rsplit('/').next(), Some("aFChunk"))
}

/// Discovers the `docProps` property parts through the package root
/// relationships (core / extended / custom), falling back to the well-known part
/// names. Returns the read bytes plus the resolved part names (so the caller can
/// mark them consumed).
fn discover_docprops(
    package: &mut DocxPackage<'_>,
    recover: bool,
    repairs: &mut Vec<Repair>,
) -> Result<(metadata::DocPropsSources, Vec<String>), ImportError> {
    // A damaged `_rels/.rels` is read a second time here, for the property
    // parts. Recovering falls back to the well-known names below rather than
    // refusing: the relationship index is plumbing, and losing it must not cost
    // the document its title and author, let alone its text.
    let root_relationships = match package.part_relationships("") {
        Ok(relationships) => relationships,
        Err(error) if recover => {
            let _ = error;
            repairs.push(Repair::in_part(
                RepairKind::PartUnparsable,
                PartRole::DocumentProperties,
                "_rels/.rels",
            ));
            Vec::new()
        }
        Err(error) => return Err(ImportError::Package(error)),
    };
    let admitted: std::collections::BTreeSet<String> = package
        .entries()
        .iter()
        .map(|entry| entry.part_name.clone())
        .collect();
    let resolve = |suffix: &str, fallback: &str| -> Option<String> {
        root_relationships
            .iter()
            .find(|relationship| relationship.relationship_type.ends_with(suffix))
            .and_then(|relationship| relationship.resolved_part.clone())
            .or_else(|| admitted.contains(fallback).then(|| fallback.to_owned()))
            .filter(|part| admitted.contains(part))
    };
    let core_part = resolve("/core-properties", "docProps/core.xml");
    let app_part = resolve("/extended-properties", "docProps/app.xml");
    let custom_part = resolve("/custom-properties", "docProps/custom.xml");
    let mut consumed = Vec::new();
    let mut sources = metadata::DocPropsSources::default();
    for (part, slot) in [
        (core_part, &mut sources.core),
        (app_part, &mut sources.app),
        (custom_part, &mut sources.custom),
    ] {
        if let Some(part) = part {
            *slot = recover_read(
                package,
                &part,
                PartRole::DocumentProperties,
                recover,
                repairs,
            )?;
            consumed.push(part);
        }
    }
    Ok((sources, consumed))
}

/// Imports main-document WordprocessingML bytes (no styles) into a v1 document.
pub fn import_main_document_xml(xml: &[u8], config: ImportConfig) -> Result<Import, ImportError> {
    import_with_sources(
        xml,
        None,
        None,
        None,
        &std::collections::BTreeMap::new(),
        None,
        None,
        None,
        None,
        &[],
        &[],
        None,
        &[],
        &std::collections::BTreeMap::new(),
        &std::collections::BTreeMap::new(),
        // No package, so no chart parts to read: a chart reference still
        // round-trips, it just has no projection on this path.
        &std::collections::BTreeMap::new(),
        config,
    )
}

/// Resolves an admitted part reached through a main-document relationship whose
/// type ends with `suffix`, falling back to a well-known part name when the
/// relationship is absent. Returns `None` unless the resolved part is admitted.
fn related_or_wellknown(package: &DocxPackage<'_>, suffix: &str, fallback: &str) -> Option<String> {
    let admitted = |name: &str| {
        package
            .entries()
            .iter()
            .any(|entry| entry.part_name == name)
    };
    package
        .main_document_relationships()
        .iter()
        .find(|relationship| relationship.relationship_type.ends_with(suffix))
        .and_then(|relationship| relationship.resolved_part.clone())
        .or_else(|| admitted(fallback).then(|| fallback.to_owned()))
        .filter(|part| admitted(part))
}

/// The embedded-object (chart / SmartArt / OLE) and alt-chunk relationships in one
/// part's relationship list, keyed by `r:id`.
///
/// One function for the main document and every running part, because the two
/// used to be built differently — the main document's by this filter, the running
/// parts' not at all — and that difference is `109` HF-266.
fn embedded_relationships(
    relationships: &[casual_doc_ooxml::DocumentRelationship],
) -> std::collections::BTreeMap<String, EmbeddedRel> {
    relationships
        .iter()
        .filter(|relationship| {
            is_embedded_object_rel(&relationship.relationship_type)
                || is_alt_chunk_rel(&relationship.relationship_type)
        })
        .filter(|relationship| !relationship.id.is_empty())
        .filter_map(|relationship| {
            let part = relationship.resolved_part.clone()?;
            Some((
                relationship.id.clone(),
                EmbeddedRel {
                    relationship_type: relationship.relationship_type.clone(),
                    part_name: part,
                },
            ))
        })
        .collect()
}

/// Reads an extra part and resolves its own image, external-hyperlink and
/// embedded-object relationships (via the part's `_rels`), so content inside it
/// can be modeled.
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
        if relationship.relationship_type.ends_with("/image")
            && let Some(part) = relationship.resolved_part.clone()
        {
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
    let hyperlinks = relationships
        .iter()
        .filter(|relationship| relationship.relationship_type.ends_with("/hyperlink"))
        .filter(|relationship| {
            relationship.target_mode == casual_doc_ooxml::TargetMode::External
                && !relationship.id.is_empty()
        })
        .map(|relationship| (relationship.id.clone(), relationship.target.clone()))
        .collect();
    let embedded = embedded_relationships(&relationships);
    Ok(PartSources {
        xml,
        images,
        hyperlinks,
        embedded,
        ..PartSources::default()
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
    shared: &SharedTables<'_>,
    media: &mut DefinitionMap<MediaId, casual_doc_model::v1::MediaReference>,
    parsed_defs: &mut body::ParsedDefinitions,
    embedded_part_names: &mut std::collections::BTreeSet<String>,
    ids: &mut IdGenerator,
    reporter: &mut Reporter,
    config: ImportConfig,
) -> Result<BuiltNotes, ImportError> {
    let mut map = DefinitionMap::default();
    let mut index = std::collections::BTreeMap::new();
    if let Some(part) = part {
        let media_index = media::build_into(&part.images, media, ids, reporter)?;
        let parsed = body::parse_notes(
            &part.xml,
            ids,
            reporter,
            &shared.running_part(&media_index, part),
            parsed_defs,
            container,
            config,
        )?;
        embedded_part_names.extend(parsed.embedded_part_names);
        for (source_id, note_id, blocks) in parsed.notes {
            index.insert(source_id, note_id);
            map.insert(note_id, Note { blocks });
        }
    }
    Ok((map, index))
}

/// A comment definition map, its source-`w:id` -> id resolution index, and the
/// collaborator identity table (`people.xml`).
type BuiltComments = (
    DefinitionMap<CommentId, Comment>,
    std::collections::BTreeMap<String, CommentId>,
    Vec<Person>,
);

/// Parses the comments part into a `CommentId`-keyed definition map plus a
/// source-`w:id` resolution index for in-body `w:commentReference`s. The part's
/// own image and hyperlink relationships are resolved so images/links inside a
/// comment are modeled. The companion parts (`commentsExtended`/`commentsIds`/
/// `people`) are joined on `paraId` so reply threading, resolved-state, durable
/// ids, and author identity survive. Missing part → empty.
#[allow(clippy::too_many_arguments)]
fn build_comments(
    part: Option<&PartSources>,
    shared: &SharedTables<'_>,
    media: &mut DefinitionMap<MediaId, casual_doc_model::v1::MediaReference>,
    parsed_defs: &mut body::ParsedDefinitions,
    embedded_part_names: &mut std::collections::BTreeSet<String>,
    ids: &mut IdGenerator,
    reporter: &mut Reporter,
    config: ImportConfig,
) -> Result<BuiltComments, ImportError> {
    let mut map = DefinitionMap::default();
    let mut index = std::collections::BTreeMap::new();
    let mut people = Vec::new();
    if let Some(part) = part {
        let media_index = media::build_into(&part.images, media, ids, reporter)?;
        let parsed = body::parse_comments(
            &part.xml,
            ids,
            reporter,
            &shared.running_part(&media_index, part),
            parsed_defs,
            config,
        )?;
        embedded_part_names.extend(parsed.embedded_part_names);
        let comments = parsed.comments;
        // Companion-part joins: the last-paragraph `paraId` per comment (from the
        // base part) is the key into commentsExtended (parent/done) and
        // commentsIds (durable id); people supplies author identity.
        let para_ids = comments_ext::scan_comment_para_ids(&part.xml, config)?;
        let extended = match &part.comments_extended {
            Some(xml) => comments_ext::parse_comments_extended(xml, reporter, config)?,
            None => std::collections::BTreeMap::new(),
        };
        let durable = match &part.comments_ids {
            Some(xml) => comments_ext::parse_comments_ids(xml, reporter, config)?,
            None => std::collections::BTreeMap::new(),
        };
        if let Some(xml) = &part.people {
            people = comments_ext::parse_people(xml, reporter, config)?;
        }
        for (source_id, comment_id, mut comment) in comments {
            if let Some(para_id) = para_ids.get(&source_id) {
                if let Some(entry) = extended.get(para_id) {
                    comment.parent_para_id = entry.parent_para_id.clone();
                    comment.done = entry.done;
                }
                if let Some(durable_id) = durable.get(para_id) {
                    comment.durable_id = Some(durable_id.clone());
                }
                comment.para_id = Some(para_id.clone());
            }
            if let Some(author) = &comment.author
                && people.iter().any(|person| &person.author == author)
            {
                comment.person = Some(author.clone());
            }
            index.insert(source_id, comment_id);
            map.insert(comment_id, comment);
        }
    }
    Ok((map, index, people))
}

/// A header/footer definition map, its relationship-id -> id resolution index, and
/// the watermark lifted out of each part that carried one.
///
/// The watermarks travel separately because they do not belong to the header at
/// all: Word stores a watermark as a float in every header of a section, while the
/// model states it once per section, so import lifts the shape out here and
/// `watermark::lift_header_watermarks` attaches it to the sections that reference
/// the header — which cannot happen until the body has been parsed.
type BuiltHeaderFooters = (
    DefinitionMap<HeaderFooterId, HeaderFooter>,
    std::collections::BTreeMap<String, HeaderFooterId>,
    std::collections::BTreeMap<HeaderFooterId, casual_doc_model::v1::Watermark>,
);

/// Parses each header/footer part into a `HeaderFooterId`-keyed definition map
/// plus a relationship-id resolution index for section references. Each part's id
/// precedes its content ids (document order); parts arrive in relationship-id
/// order for determinism.
#[allow(clippy::too_many_arguments)]
fn build_header_footers(
    parts: &[(String, PartSources)],
    root: &'static [u8],
    shared: &SharedTables<'_>,
    media: &mut DefinitionMap<MediaId, casual_doc_model::v1::MediaReference>,
    parsed_defs: &mut body::ParsedDefinitions,
    embedded_part_names: &mut std::collections::BTreeSet<String>,
    ids: &mut IdGenerator,
    reporter: &mut Reporter,
    config: ImportConfig,
) -> Result<BuiltHeaderFooters, ImportError> {
    let mut map = DefinitionMap::default();
    let mut index = std::collections::BTreeMap::new();
    let mut watermarks = std::collections::BTreeMap::new();
    for (relationship_id, part) in parts {
        // The header/footer id precedes its content ids; its media is added to
        // the shared table just before parsing so its drawings resolve.
        let node = ids
            .next_id()
            .map_err(|_| ImportError::LimitExceeded { limit: "node_ids" })?;
        let hf_id = HeaderFooterId::new(node);
        let media_index = media::build_into(&part.images, media, ids, reporter)?;
        let parsed = body::parse_header_footer(
            &part.xml,
            ids,
            reporter,
            &shared.running_part(&media_index, part),
            parsed_defs,
            root,
            config,
        )?;
        embedded_part_names.extend(parsed.embedded_part_names);
        index.insert(relationship_id.clone(), hf_id);
        if let Some(watermark) = parsed.watermark {
            watermarks.insert(hf_id, watermark);
        }
        map.insert(
            hf_id,
            HeaderFooter {
                blocks: parsed.blocks,
            },
        );
    }
    Ok((map, index, watermarks))
}

/// An extra part's bytes plus its own resolved image, external-hyperlink and
/// embedded-object relationships, so images, links, charts, diagrams and OLE
/// objects inside a note/header/footer/comment are modeled. For the comments
/// part, the companion parts (`commentsExtended`/`commentsIds`/`people`) ride
/// along so `build_comments` can join threading and identity.
#[derive(Default)]
pub(crate) struct PartSources {
    pub xml: Vec<u8>,
    pub images: Vec<MediaSource>,
    pub hyperlinks: std::collections::BTreeMap<String, String>,
    /// This part's embedded-object and alt-chunk relationships
    /// (`embedded_relationships`). Empty before HF-266 for every running part,
    /// which is how a header chart came to be dropped.
    pub embedded: std::collections::BTreeMap<String, EmbeddedRel>,
    /// `word/commentsExtended.xml` bytes (comments part only), when present.
    pub comments_extended: Option<Vec<u8>>,
    /// `word/commentsIds.xml` bytes (comments part only), when present.
    pub comments_ids: Option<Vec<u8>>,
    /// `word/people.xml` bytes (comments part only), when present.
    pub people: Option<Vec<u8>>,
}

/// The document-global tables every running part resolves against, so the three
/// running-part builders take one value rather than three parameters each — and
/// so the colour scheme cannot be left out of one of them again (HF-266).
struct SharedTables<'a> {
    styles: &'a Styles,
    numbering: &'a Numbering,
    color_scheme: Option<&'a casual_doc_model::v1::ColorScheme>,
}

impl<'a> SharedTables<'a> {
    /// The inputs for one running part: these shared tables, plus the part's own
    /// media index (already merged into the shared media table), hyperlinks and
    /// embedded-object relationships.
    fn running_part<'b>(
        &self,
        media_index: &'b std::collections::BTreeMap<String, MediaId>,
        part: &'b PartSources,
    ) -> body::RunningPartInputs<'b>
    where
        'a: 'b,
    {
        body::RunningPartInputs {
            styles: self.styles,
            numbering: self.numbering,
            color_scheme: self.color_scheme,
            media_index,
            hyperlink_rels: &part.hyperlinks,
            embedded_index: &part.embedded,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn import_with_sources(
    document_xml: &[u8],
    styles_xml: Option<&[u8]>,
    numbering_xml: Option<&[u8]>,
    font_table_xml: Option<&[u8]>,
    font_table_rels: &std::collections::BTreeMap<String, String>,
    theme_xml: Option<&[u8]>,
    settings_xml: Option<&[u8]>,
    footnotes: Option<&PartSources>,
    endnotes: Option<&PartSources>,
    header_parts: &[(String, PartSources)],
    footer_parts: &[(String, PartSources)],
    comments: Option<&PartSources>,
    media_sources: &[MediaSource],
    hyperlink_rels: &std::collections::BTreeMap<String, String>,
    embedded_index: &std::collections::BTreeMap<String, EmbeddedRel>,
    chart_parts: &std::collections::BTreeMap<String, crate::chart::ChartPartSource>,
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
    // What retains the unconsumed remainder of everything this pass reads. In
    // Retention mode the tier-1 byte floor reproduces the import input exactly,
    // so a single source-snapshot ledger record licenses every `preserved`
    // finding; in Semantic mode nothing blanket-retains, and a finding is
    // `preserved` only if the finding itself carries an in-model retention
    // (`Reporter::report_retained_in_model`). This is *not* the old per-mode
    // disposition constant: the per-construct half of the pair comes from the
    // call site, so one Semantic import now yields `not-retained`, `rejected`
    // and `preserved` on different constructs (FID-R-02).
    let (source_retention, mut ledger) = match retained_source.as_ref() {
        Some(retained) => (
            SourceRetention::Snapshot,
            PreservationLedger::with_source_snapshot(retained.main_document.len()),
        ),
        None => (SourceRetention::Regenerated, PreservationLedger::default()),
    };

    let mut ids = IdGenerator::new(config.id_namespace);
    // documentId is the first allocated id (deterministic).
    let document_id = ids
        .next_id()
        .map_err(|_| ImportError::LimitExceeded { limit: "node_ids" })?;
    let mut reporter = if config.recover {
        Reporter::recovering(source_retention)
    } else {
        Reporter::new(source_retention)
    };

    // Each definition part below is **independent**: a damaged `styles.xml` costs
    // the document its named styles, not its text. On the strict path a parse
    // failure refuses the whole import, which is the one outcome that is wrong
    // for every one of them — the body is readable, and the reader would rather
    // see it in default formatting than see nothing. Recovering therefore drops
    // the damaged part, names it, and carries on; the body parse below is the
    // only input whose loss is not survivable this way, and it has its own ladder
    // in `import_package`.
    let styles = match styles_xml {
        Some(xml) => recover_part(
            styles::parse(xml, &mut ids, &mut reporter, config),
            &mut reporter,
            PartRole::Styles,
            Styles::default,
        )?,
        None => Styles::default(),
    };
    let numbering = match numbering_xml {
        Some(xml) => recover_part(
            numbering::parse(xml, &mut ids, &mut reporter, config, &styles),
            &mut reporter,
            PartRole::Numbering,
            Numbering::default,
        )?,
        None => Numbering::default(),
    };
    // Resolve style-level `w:numPr` now that both parts are parsed: a paragraph
    // style's list membership (captured raw during styles parsing, since the
    // numbering `numId -> instance` map did not exist yet) becomes the style's
    // `paragraph.numbering`, so a paragraph that inherits its list from its style
    // renders with a marker.
    let mut styles = styles;
    styles.resolve_numbering(&numbering, &mut reporter);
    let font_table = match font_table_xml {
        Some(xml) => recover_part(
            font_table::parse(xml, font_table_rels, config, &mut reporter),
            &mut reporter,
            PartRole::FontTable,
            Vec::new,
        )?,
        None => Vec::new(),
    };
    let theme = match theme_xml {
        Some(xml) => recover_part(
            theme::parse(xml, &mut reporter, config),
            &mut reporter,
            PartRole::Theme,
            theme::ParsedTheme::default,
        )?,
        None => theme::ParsedTheme::default(),
    };
    let settings = match settings_xml {
        Some(xml) => recover_part(
            settings::parse(xml, &mut reporter, config),
            &mut reporter,
            PartRole::Settings,
            DocumentSettings::default,
        )?,
        None => DocumentSettings::default(),
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

    // Bookmarks and paragraph-spanning field ranges are discovered during each
    // part's body parse (not built ahead like media), so they accumulate into one
    // document-global bundle threaded (by `&mut`) into every part parser — body,
    // notes, headers, footers, and comments all land in a single
    // `Definitions::bookmarks` / `Definitions::field_ranges`.
    let mut parsed_defs = body::ParsedDefinitions::new();
    // The package parts an embedded-object node references, from EVERY surface:
    // a chart in a header is as much a node-referenced part as one in the body,
    // and the side-table must not re-add its relationship as an orphan either.
    let mut embedded_part_names = std::collections::BTreeSet::new();
    let shared = SharedTables {
        styles: &styles,
        numbering: &numbering,
        color_scheme: theme.color_scheme.as_ref(),
    };

    let (footnotes_map, footnote_ids) = build_notes(
        footnotes,
        b"footnote",
        &shared,
        &mut media,
        &mut parsed_defs,
        &mut embedded_part_names,
        &mut ids,
        &mut reporter,
        config,
    )?;
    let (endnotes_map, endnote_ids) = build_notes(
        endnotes,
        b"endnote",
        &shared,
        &mut media,
        &mut parsed_defs,
        &mut embedded_part_names,
        &mut ids,
        &mut reporter,
        config,
    )?;
    let (headers, header_ids, header_watermarks) = build_header_footers(
        header_parts,
        b"hdr",
        &shared,
        &mut media,
        &mut parsed_defs,
        &mut embedded_part_names,
        &mut ids,
        &mut reporter,
        config,
    )?;
    // A footer never carries a watermark to lift (`crate::watermark` refuses the
    // container), so the third element is always empty here.
    let (footers, footer_ids, _) = build_header_footers(
        footer_parts,
        b"ftr",
        &shared,
        &mut media,
        &mut parsed_defs,
        &mut embedded_part_names,
        &mut ids,
        &mut reporter,
        config,
    )?;
    let (comments_map, comment_ids, people) = build_comments(
        comments,
        &shared,
        &mut media,
        &mut parsed_defs,
        &mut embedded_part_names,
        &mut ids,
        &mut reporter,
        config,
    )?;

    let body::BodyParse {
        blocks: mut body,
        mut sections,
        embedded_part_names: body_embedded_part_names,
        page_background,
    } = body::parse(
        document_xml,
        &mut ids,
        &mut reporter,
        body::ParseInputs {
            styles: &styles,
            numbering: &numbering,
            media_index: &media_index,
            hyperlink_rels,
            embedded_index,
            footnote_ids: &footnote_ids,
            endnote_ids: &endnote_ids,
            header_ids: &header_ids,
            footer_ids: &footer_ids,
            comment_ids: &comment_ids,
            color_scheme: theme.color_scheme.as_ref(),
        },
        &mut parsed_defs,
        config,
    )?;
    // The second half of the watermark lift: the shapes left their headers during
    // the header parse, and only now — with the body's `w:sectPr` boundaries built
    // and each one's header references resolved — is it known which section each
    // stamp belongs to. This is why `build_section_boundary` cannot set it.
    watermark::lift_header_watermarks(&mut sections, &header_watermarks);
    embedded_part_names.extend(body_embedded_part_names);

    // The typed chart projections (`docs/155` §8): a READ projection of parts that
    // stay byte-preserved, built after the body parse because each one is anchored
    // to the `EmbeddedObject` node a part parse minted. Ids come from the same
    // generator, immediately after the body's, so the sequence stays deterministic.
    // The body is walked first, so a document with charts only in its body
    // projects with exactly the ids it always did; the running surfaces follow in
    // the order `Document::visit_chart_object_ids` walks them (HF-266).
    let chart_containers = std::iter::once(body.as_slice())
        .chain(
            footnotes_map
                .iter()
                .chain(endnotes_map.iter())
                .map(|(_, note)| note.blocks.as_slice()),
        )
        .chain(
            headers
                .iter()
                .chain(footers.iter())
                .map(|(_, running)| running.blocks.as_slice()),
        )
        .chain(
            comments_map
                .iter()
                .map(|(_, comment)| comment.blocks.as_slice()),
        );
    let projected_charts = chart::build_charts(chart_containers, chart_parts, &mut ids, config)?;

    if body.is_empty() {
        // A body with no paragraphs yields a single empty paragraph so the v1
        // document has a non-empty body.
        let id = ids
            .next_id()
            .map_err(|_| ImportError::LimitExceeded { limit: "node_ids" })?;
        body.push(BlockNode::Paragraph(Paragraph {
            id,
            properties: ParagraphProperties::default().into(),
            inlines: Vec::new(),
        }));
    }

    let (abstract_numbering, numbering_instances) = numbering.into_definitions();
    let document_defaults = styles.document_defaults();
    let latent_styles = styles.latent_styles();
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
        comments: comments_map,
        bookmarks: parsed_defs.bookmarks,
        field_ranges: parsed_defs.field_ranges,
        charts: projected_charts.charts,
        document_defaults,
        latent_styles,
        font_table,
        font_scheme: theme.font_scheme,
        color_scheme: theme.color_scheme,
        format_scheme: theme.format_scheme,
        format_scheme_xml: theme.format_scheme_xml,
        shape_styles: parsed_defs.shape_styles,
        settings,
        people,
    };
    let mut document = Document::new(document_id, body, definitions).map_err(ImportError::Model)?;
    if let Some(color) = page_background {
        document = document
            .with_background(color)
            .map_err(ImportError::Model)?;
    }
    // Taken before `into_report` consumes the reporter: the repairs this pass
    // recorded are a separate report from its findings, for the reason
    // `crate::recovery` states.
    let recovery = RecoveryReport::from_repairs(reporter.take_repairs());
    let report = reporter.into_report(&mut ledger);
    // `35` requires an illegal disposition to fail the import rather than be
    // reported. The nine legal axis pairs are unrepresentable by construction, so
    // what this catches is the preservation rule: a `preserved` claim with no
    // validated ledger record behind it.
    report.validate(&ledger).map_err(ImportError::Disposition)?;
    Ok(Import {
        document,
        report,
        ledger,
        recovery,
        retained_source,
        // The XML-only path has no package, so no opaque parts to preserve;
        // `import_package` populates the side-table when a package is available.
        retained_parts: RetainedParts::default(),
        embedded_part_names,
        chart_parts: projected_charts.parts,
    })
}

#[cfg(test)]
mod tests;
