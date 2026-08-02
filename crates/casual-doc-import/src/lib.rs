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
mod comments_ext;
mod config;
mod error;
mod font_table;
mod math;
mod media;
mod metadata;
mod numbering;
mod opaque;
mod properties;
mod report;
mod retain;
mod settings;
mod styles;
mod tables;
mod theme;
mod vml;

pub use config::{ImportConfig, ImportMode};
pub use error::ImportError;
pub use opaque::{
    RelationshipOwner, RetainedPart, RetainedParts, RetainedRelationship, RetainedRels,
};
pub use report::{
    CompatibilityEntry, CompatibilityReport, ModelOutcome, PartDisposition, RetentionOutcome,
};
pub use retain::RetainedSource;
pub use vml::{
    VmlColor, VmlDrawing, VmlFill, VmlHorizontalAlign, VmlHr, VmlHrAlign, VmlPosition, VmlRelFrame,
    VmlShapeKind, VmlStroke, VmlTextAnchor, VmlTextbox, VmlVerticalAlign, VmlWrap, VmlWrapMode,
    parse_vml_pict,
};

use casual_doc_model::IdGenerator;
use casual_doc_model::v1::{
    BlockNode, Bookmark, BookmarkId, Comment, CommentId, DefinitionMap, Definitions, Document,
    DocumentSettings, HeaderFooter, HeaderFooterId, MediaId, Note, NoteId, Paragraph,
    ParagraphProperties, Person,
};
use casual_doc_ooxml::DocxPackage;

use crate::body::EmbeddedRel;
use crate::media::MediaSource;
use crate::numbering::Numbering;
use crate::report::Reporter;
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

/// The result of importing a main document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Import {
    /// The normalized v1 document.
    pub document: Document,
    /// The compatibility report.
    pub report: CompatibilityReport,
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
    // The font table plus its own relationships (embedded `.odttf` fonts resolve
    // through `fontTable.xml.rels`, not the document's).
    let (font_table_bytes, font_table_rels) = match font_table_part {
        Some(part) => {
            let bytes = package.read_part(&part).map_err(ImportError::Package)?;
            let relationships = package
                .part_relationships(&part)
                .map_err(ImportError::Package)?;
            let font_rels: std::collections::BTreeMap<String, String> = relationships
                .iter()
                .filter(|relationship| relationship.relationship_type.ends_with("/font"))
                .filter_map(|relationship| {
                    Some((relationship.id.clone(), relationship.resolved_part.clone()?))
                })
                .collect();
            (Some(bytes), font_rels)
        }
        None => (None, std::collections::BTreeMap::new()),
    };
    for part in font_table_rels.values() {
        consumed.insert(part.clone());
    }
    let theme_bytes = match theme_part {
        Some(part) => Some(package.read_part(&part).map_err(ImportError::Package)?),
        None => None,
    };
    let settings_bytes = match settings_part {
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
    let comments = match comments_part {
        Some(part) => {
            let mut sources = resolve_part_sources(package, &part)?;
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
            if let Some(companion) = extended_part {
                sources.comments_extended = Some(
                    package
                        .read_part(&companion)
                        .map_err(ImportError::Package)?,
                );
            }
            if let Some(companion) = ids_part {
                sources.comments_ids = Some(
                    package
                        .read_part(&companion)
                        .map_err(ImportError::Package)?,
                );
            }
            if let Some(companion) = people_part {
                sources.people = Some(
                    package
                        .read_part(&companion)
                        .map_err(ImportError::Package)?,
                );
            }
            Some(sources)
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
        header_parts.push((relationship_id, resolve_part_sources(package, &part)?));
    }
    let mut footer_parts = Vec::new();
    for (relationship_id, part) in footer_refs {
        consumed.insert(part.clone());
        footer_parts.push((relationship_id, resolve_part_sources(package, &part)?));
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
    let embedded_index: std::collections::BTreeMap<String, EmbeddedRel> = package
        .main_document_relationships()
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
        .collect();

    let mut import = import_with_sources(
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

    // Document properties (`docProps/{core,app,custom}.xml`). Their relationships
    // hang off the PACKAGE root (`_rels/.rels`), not the main document's, so they
    // are discovered and parsed here rather than in `import_with_sources`; the
    // well-known part names are a fallback for producers that omit the
    // relationship. Unmapped property fields fold into the compatibility report.
    // The discovered parts are recorded as consumed so the disposition pass below
    // does not report them as dropped (they are modeled and regenerated on write).
    let (sources, docprop_parts) = discover_docprops(package)?;
    for part in docprop_parts {
        consumed.insert(part);
    }
    if !sources.is_empty() {
        let mut reporter = Reporter::default();
        if let Some(properties) = metadata::parse(&sources, config, &mut reporter)? {
            import.document = import
                .document
                .with_properties(properties)
                .map_err(ImportError::Model)?;
        }
        let retention = match config.mode {
            ImportMode::Retention => RetentionOutcome::Preserved,
            ImportMode::Semantic => RetentionOutcome::NotRetained,
        };
        import.report.merge(reporter.into_report(retention));
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
    //     invalidates a signature. It is dropped and reported `not-retained` in
    //     Semantic mode (Retention's byte floor still keeps the bytes verbatim,
    //     so it is `preserved` there).
    let (retained_parts, dispositions) =
        build_retained_parts(package, &consumed, &import.embedded_part_names, config)?;
    import.retained_parts = retained_parts;
    import.report.add_part_dispositions(dispositions);

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
    config: ImportConfig,
) -> Result<(RetainedParts, Vec<(PartDisposition, RetentionOutcome)>), ImportError> {
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
        // Retention: the byte floor keeps every part, so both preserved and
        // signature parts are `preserved`. Semantic: only side-table parts are
        // preserved; a signature is dropped (`not-retained`).
        let retention = match config.mode {
            ImportMode::Retention => RetentionOutcome::Preserved,
            ImportMode::Semantic if is_signature => RetentionOutcome::NotRetained,
            ImportMode::Semantic => RetentionOutcome::Preserved,
        };
        dispositions.push((
            PartDisposition {
                part_name: name.clone(),
                content_type: content_type.clone(),
            },
            retention,
        ));
        if is_signature {
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
    let root_rels = package
        .part_relationships("")
        .map_err(ImportError::Package)?;
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
) -> Result<(metadata::DocPropsSources, Vec<String>), ImportError> {
    let root_relationships = package
        .part_relationships("")
        .map_err(ImportError::Package)?;
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
    if let Some(part) = core_part {
        sources.core = Some(package.read_part(&part).map_err(ImportError::Package)?);
        consumed.push(part);
    }
    if let Some(part) = app_part {
        sources.app = Some(package.read_part(&part).map_err(ImportError::Package)?);
        consumed.push(part);
    }
    if let Some(part) = custom_part {
        sources.custom = Some(package.read_part(&part).map_err(ImportError::Package)?);
        consumed.push(part);
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
    Ok(PartSources {
        xml,
        images,
        hyperlinks,
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
    styles: &Styles,
    numbering: &Numbering,
    media: &mut DefinitionMap<MediaId, casual_doc_model::v1::MediaReference>,
    bookmarks: &mut DefinitionMap<BookmarkId, Bookmark>,
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
            bookmarks,
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
    styles: &Styles,
    numbering: &Numbering,
    media: &mut DefinitionMap<MediaId, casual_doc_model::v1::MediaReference>,
    bookmarks: &mut DefinitionMap<BookmarkId, Bookmark>,
    ids: &mut IdGenerator,
    reporter: &mut Reporter,
    config: ImportConfig,
) -> Result<BuiltComments, ImportError> {
    let mut map = DefinitionMap::default();
    let mut index = std::collections::BTreeMap::new();
    let mut people = Vec::new();
    if let Some(part) = part {
        let media_index = media::build_into(&part.images, media, ids, reporter)?;
        let comments = body::parse_comments(
            &part.xml,
            ids,
            reporter,
            styles,
            numbering,
            &media_index,
            &part.hyperlinks,
            bookmarks,
            config,
        )?;
        // Companion-part joins: the last-paragraph `paraId` per comment (from the
        // base part) is the key into commentsExtended (parent/done) and
        // commentsIds (durable id); people supplies author identity.
        let para_ids = comments_ext::scan_comment_para_ids(&part.xml, config)?;
        let extended = match &part.comments_extended {
            Some(xml) => comments_ext::parse_comments_extended(xml, config)?,
            None => std::collections::BTreeMap::new(),
        };
        let durable = match &part.comments_ids {
            Some(xml) => comments_ext::parse_comments_ids(xml, config)?,
            None => std::collections::BTreeMap::new(),
        };
        if let Some(xml) = &part.people {
            people = comments_ext::parse_people(xml, config)?;
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
    bookmarks: &mut DefinitionMap<BookmarkId, Bookmark>,
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
            bookmarks,
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
/// For the comments part, the companion parts (`commentsExtended`/`commentsIds`/
/// `people`) ride along so `build_comments` can join threading and identity.
#[derive(Default)]
pub(crate) struct PartSources {
    pub xml: Vec<u8>,
    pub images: Vec<MediaSource>,
    pub hyperlinks: std::collections::BTreeMap<String, String>,
    /// `word/commentsExtended.xml` bytes (comments part only), when present.
    pub comments_extended: Option<Vec<u8>>,
    /// `word/commentsIds.xml` bytes (comments part only), when present.
    pub comments_ids: Option<Vec<u8>>,
    /// `word/people.xml` bytes (comments part only), when present.
    pub people: Option<Vec<u8>>,
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
    let font_table = match font_table_xml {
        Some(xml) => font_table::parse(xml, font_table_rels, config)?,
        None => Vec::new(),
    };
    let theme = match theme_xml {
        Some(xml) => theme::parse(xml, config)?,
        None => theme::ParsedTheme::default(),
    };
    let settings = match settings_xml {
        Some(xml) => settings::parse(xml, &mut reporter, config)?,
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

    // Bookmarks are discovered during each part's body parse (not built ahead like
    // media), so they accumulate into one document-global map threaded (by `&mut`)
    // into every part parser — body, notes, headers, footers, and comments all land
    // in a single `Definitions::bookmarks`.
    let mut bookmarks = DefinitionMap::default();

    let (footnotes_map, footnote_ids) = build_notes(
        footnotes,
        b"footnote",
        &styles,
        &numbering,
        &mut media,
        &mut bookmarks,
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
        &mut bookmarks,
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
        &mut bookmarks,
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
        &mut bookmarks,
        &mut ids,
        &mut reporter,
        config,
    )?;
    let (comments_map, comment_ids, people) = build_comments(
        comments,
        &styles,
        &numbering,
        &mut media,
        &mut bookmarks,
        &mut ids,
        &mut reporter,
        config,
    )?;

    let body::BodyParse {
        blocks: mut body,
        sections,
        embedded_part_names,
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
        &mut bookmarks,
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
    let document_defaults = styles.document_defaults();
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
        bookmarks,
        document_defaults,
        font_table,
        font_scheme: theme.font_scheme,
        color_scheme: theme.color_scheme,
        format_scheme_xml: theme.format_scheme_xml,
        settings,
        people,
    };
    let mut document = Document::new(document_id, body, definitions).map_err(ImportError::Model)?;
    if let Some(color) = page_background {
        document = document
            .with_background(color)
            .map_err(ImportError::Model)?;
    }
    Ok(Import {
        document,
        report: reporter.into_report(retention),
        retained_source,
        // The XML-only path has no package, so no opaque parts to preserve;
        // `import_package` populates the side-table when a package is available.
        retained_parts: RetainedParts::default(),
        embedded_part_names,
    })
}

#[cfg(test)]
mod tests;
