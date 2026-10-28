//! The byte-preserving PDF save (`docs/182` §7, P4): what an edited
//! PDF-origin session writes when the original can be kept.
//!
//! # The decision, page by page
//!
//! At import the adapter records a [`Baseline`]: the content digest of every
//! page's section, of each frame on it, and of the rest of the document, plus
//! the [`SourceMap`] (page objects, and the show operators each frame's glyphs
//! came from). At save, the same digests are taken of the document as it is
//! now. Unchanged-ness is **computed**, never recorded by an edit (G-6: no
//! `Operation` is added, ADR-030 I2):
//!
//! - a page whose section digest equals the import's is **carried over**: the
//!   update writes nothing for it, so its objects are the original's, byte for
//!   byte;
//! - an edited page keeps its content stream except the show operators of the
//!   frames that were edited or deleted, which are removed (`remove_shows`),
//!   and gains an overlay Form XObject drawing the edited and added frames'
//!   text with the writer's text layer;
//! - the result is an incremental update appended to the original.
//!
//! Anything this cannot prove safe falls back to the semantic writer, and the
//! fallback is **reported** with its reason ([`Fallback`]): a changed picture,
//! shape or section setting; a frame that is not plain text; an operator that
//! also paints a kept frame; content the two tokenizers disagree about; a
//! changed style table or a new header; a page count that moved.
//!
//! The digests are the diff crate's content projection (`casual-doc-diff`
//! `projection`/`hash`, the machinery behind `identity::content_digest`): the
//! same definition of "same content" the version history uses.
//!
//! Complexity: O(document) at save (one projection, one pagination, the
//! original's prefix copy); never per edit.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use casual_doc_diff::hash::ContentHasher;
use casual_doc_diff::projection::{Projector, StoryKey, ValueHashes};
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::display::{DisplayList, PaintItem};
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{BlockNode, BreakKind, Document, InlineNode, TextBox};
use casual_doc_pdf::{
    DocumentText, EditedPage, IncrementalBase, PdfPage, PdfSeverity, RegistryFontSource,
    write_incremental_update,
};
use casual_doc_pdf_import::SourceMap;
use casual_doc_pdf_read::content::remove_shows;
use casual_doc_pdf_read::revision::PdfRevision;

use crate::pdf::ResourceMedia;
use crate::{
    CompatibilityEntry, CompatibilityReport, DocumentResources, FeatureLocation, ModelOutcome,
    RetentionOutcome,
};

/// The finding a fallback to the semantic writer is reported under; the
/// reason is the suffix (`pdf.export.preserve_fallback.frame_not_text_only`).
pub(crate) const FALLBACK: &str = "pdf.export.preserve_fallback";
/// The update rewrote this many pages.
pub(crate) const INCREMENTAL_UPDATE: &str = "pdf.export.incremental_update";
/// This many pages were carried over as their original objects.
pub(crate) const PAGE_CARRIED_OVER: &str = "pdf.export.page_carried_over";
/// The removed text is still in the file, in the earlier revision.
pub(crate) const EARLIER_REVISION_RETAINED: &str = "pdf.export.earlier_revision_retained";
/// The file is signed: the signatures cover the earlier revision only.
pub(crate) const SIGNATURE_EARLIER_REVISION: &str = "pdf.export.signature_covers_earlier_revision";
/// The file is certified (`/Perms`): a validator reports the update as a
/// change after certification.
pub(crate) const CERTIFICATION_INVALIDATED: &str = "pdf.export.certification_invalidated";
/// The edit was undone: the content equals the import's, so the original is
/// written back exactly.
pub(crate) const ORIGINAL_WRITTEN_BACK: &str = "pdf.export.original_written_back";

/// Why the save fell back to the semantic writer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Fallback {
    /// A stable reason id.
    pub(crate) reason: &'static str,
    /// The page it concerns, when it concerns one.
    pub(crate) page: Option<usize>,
}

impl Fallback {
    fn new(reason: &'static str) -> Self {
        Self { reason, page: None }
    }

    fn on(reason: &'static str, page: usize) -> Self {
        Self {
            reason,
            page: Some(page),
        }
    }

    /// No original was retained (the session was imported without
    /// `retain_source`), so there is nothing to preserve.
    pub(crate) fn no_retained_source() -> Self {
        Self::new("no_retained_source")
    }

    /// The report entry.
    pub(crate) fn entry(&self) -> CompatibilityEntry {
        CompatibilityEntry {
            feature: format!("{FALLBACK}.{}", self.reason),
            occurrences: 1,
            location: FeatureLocation {
                part_name: self.page.map(|page| format!("page {}", page + 1)),
                namespace: None,
                local_name: Some(self.reason.to_owned()),
                attribute_name: None,
            },
            // The save is correct (the semantic writer renders the model), but
            // the original's objects are not kept.
            model_outcome: ModelOutcome::Degraded,
            retention_outcome: RetentionOutcome::NotRetained,
        }
    }
}

/// One section's digests.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SectionDigest {
    /// Everything on the page: its blocks' projection hashes and its section
    /// properties.
    whole: u128,
    /// Everything but the text frames (the anchor paragraph's properties, its
    /// pictures and shapes, the section properties), when the section is the
    /// one anchor paragraph a fixed-layout page is.
    fixed: Option<u128>,
    /// Each text frame's digest, keyed by its node.
    frames: BTreeMap<NodeId, u128>,
}

/// The document reduced to what the save compares.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Digest {
    document: u128,
    definitions: u128,
    stories: u128,
    sections: Vec<SectionDigest>,
}

/// What the import recorded, for the save to compare against.
#[derive(Debug)]
pub(crate) struct Baseline {
    digest: Digest,
    sources: SourceMap,
}

impl Baseline {
    /// Records the import. O(document). `None` when the imported document
    /// cannot be digested (its body does not project), so no save preserves.
    pub(crate) fn new(document: &Document, sources: SourceMap) -> Option<Self> {
        Some(Self {
            digest: digest(document)?,
            sources,
        })
    }

    /// Whether `document` says exactly what the import did.
    pub(crate) fn unchanged(&self, document: &Document) -> bool {
        casual_doc_diff::content_digest(document) == self.digest.document
    }
}

/// O(document): one projection pass and one walk of the body.
fn digest(document: &Document) -> Option<Digest> {
    let mut projector = Projector::new();
    while !projector.step(document, usize::MAX) {}
    let (projection, _, _) = projector.finish();
    let mut values = ValueHashes::default();

    let mut stories = ContentHasher::new();
    let mut body = None;
    for slot in &projection.stories {
        match &slot.key {
            StoryKey::Body => body = Some(slot),
            // Neither PDF writer carries comments, so a comment changes
            // nothing either save writes.
            StoryKey::Comment(_) => {}
            key => {
                stories.write_str(&format!("{key:?}"));
                stories.write_u128(slot.content_hash);
            }
        }
    }
    let body = body?;

    let mut definitions = document.definitions().clone();
    let section_props = std::mem::take(&mut definitions.sections);
    let definitions = values.hash_value(&definitions);

    let blocks = document.body();
    if blocks.len() != body.root_count as usize {
        return None;
    }
    let mut sections = Vec::new();
    let mut start = 0;
    for (at, block) in blocks.iter().enumerate() {
        let ends = match block {
            BlockNode::Paragraph(paragraph) => paragraph.properties.section_break,
            _ => None,
        };
        let last = at + 1 == blocks.len();
        if ends.is_none() && !last {
            continue;
        }
        let properties = match ends {
            Some(id) => section_props.iter().find(|s| s.id == id)?,
            None => section_props.last()?,
        };
        let props_hash = values.hash_value(properties);
        let mut whole = ContentHasher::new();
        whole.write_u128(props_hash);
        for index in start..=at {
            let projected = projection
                .blocks
                .get(body.first_root as usize + index)
                .filter(|b| b.sibling_index as usize == index)?;
            whole.write_u128(projected.subtree_hash);
        }
        let (fixed, frames) = match &blocks[start..=at] {
            [BlockNode::Paragraph(paragraph)] => {
                let mut fixed = ContentHasher::new();
                fixed.write_u128(props_hash);
                fixed.write_u128(values.hash_value(&*paragraph.properties));
                let mut frames = BTreeMap::new();
                for inline in &paragraph.inlines {
                    match inline {
                        InlineNode::TextBox(frame) => {
                            frames.insert(frame.id, values.hash_value(&**frame));
                        }
                        other => fixed.write_u128(values.hash_value(other)),
                    }
                }
                (Some(fixed.finish()), frames)
            }
            _ => (None, BTreeMap::new()),
        };
        sections.push(SectionDigest {
            whole: whole.finish(),
            fixed,
            frames,
        });
        start = at + 1;
    }
    Some(Digest {
        document: casual_doc_diff::content_digest(document),
        definitions,
        stories: stories.finish(),
        sections,
    })
}

/// Whether a frame is text a glyph-run overlay draws completely: paragraphs of
/// runs, tabs and line breaks with no numbering (a list marker is not a run of
/// the paragraph), no fill, border or link on the box, and nothing else.
fn text_only(frame: &TextBox) -> bool {
    frame.fill.is_none()
        && frame.border.is_none()
        && frame.hyperlink.is_none()
        && frame.blocks.iter().all(|block| match block {
            BlockNode::Paragraph(paragraph) => {
                let p = &paragraph.properties;
                p.numbering.is_none()
                    && p.style_ref.is_none()
                    && p.drop_cap_frame.is_none()
                    && p.section_break.is_none()
                    && paragraph.inlines.iter().all(|inline| match inline {
                        InlineNode::Run(_)
                        | InlineNode::Tab(_)
                        | InlineNode::BookmarkStart(_)
                        | InlineNode::BookmarkEnd(_)
                        | InlineNode::CommentRangeStart(_)
                        | InlineNode::CommentRangeEnd(_)
                        | InlineNode::CommentReference(_) => true,
                        InlineNode::Break(b) => b.kind == BreakKind::Line,
                        _ => false,
                    })
            }
            _ => false,
        })
}

/// The paragraph nodes of a text-only frame.
fn paragraphs_of(frame: &TextBox, into: &mut BTreeSet<NodeId>) {
    for block in &frame.blocks {
        if let BlockNode::Paragraph(paragraph) = block {
            into.insert(paragraph.id);
        }
    }
}

/// The page's paint reduced to the overlay frames' glyph runs. Clip and layer
/// brackets are kept so a run inside one stays inside it. O(paint items).
fn overlay_list(list: &DisplayList, paragraphs: &BTreeSet<NodeId>) -> DisplayList {
    DisplayList {
        items: list
            .items
            .iter()
            .filter(|item| match item {
                PaintItem::Glyphs { run } => run.node.is_some_and(|n| paragraphs.contains(&n)),
                PaintItem::PushClip(_)
                | PaintItem::PopClip
                | PaintItem::PushLayer { .. }
                | PaintItem::PopLayer => true,
                _ => false,
            })
            .cloned()
            .collect(),
    }
}

/// One edited page's plan.
struct PagePlan {
    index: usize,
    remove: BTreeSet<u32>,
    paragraphs: BTreeSet<NodeId>,
    removed_frames: usize,
}

/// Writes the incremental update, or says why it cannot. O(document).
pub(crate) fn preserve(
    document: &Document,
    resources: &DocumentResources,
    original: &Arc<Vec<u8>>,
    baseline: &Baseline,
) -> Result<(Vec<u8>, CompatibilityReport), Fallback> {
    let now = digest(document).ok_or(Fallback::new("body_not_projectable"))?;
    let before = &baseline.digest;
    if now.definitions != before.definitions {
        return Err(Fallback::new("definitions_changed"));
    }
    if now.stories != before.stories {
        return Err(Fallback::new("stories_changed"));
    }
    if now.sections.len() != before.sections.len()
        || baseline.sources.pages.len() != before.sections.len()
    {
        return Err(Fallback::new("page_count_changed"));
    }

    let mut plans = Vec::new();
    for (index, (now, before)) in now.sections.iter().zip(&before.sections).enumerate() {
        if now.whole == before.whole {
            continue;
        }
        if now.fixed.is_none() || now.fixed != before.fixed {
            return Err(Fallback::on("page_objects_changed", index));
        }
        let source = &baseline.sources.pages[index];
        if source.shows.is_none() {
            return Err(Fallback::on("page_operators_unnumbered", index));
        }
        let origins: BTreeMap<NodeId, &casual_doc_pdf_import::FrameSource> = baseline
            .sources
            .frames
            .iter()
            .filter(|frame| frame.page == index)
            .map(|frame| (frame.frame, frame))
            .collect();
        let mut remove = BTreeSet::new();
        let mut removed = BTreeSet::new();
        for (frame, digest) in &before.frames {
            if now.frames.get(frame) == Some(digest) {
                continue;
            }
            let origin = origins
                .get(frame)
                .ok_or(Fallback::on("frame_origin_unknown", index))?;
            if !origin.complete {
                return Err(Fallback::on("frame_text_not_removable", index));
            }
            remove.extend(origin.shows.iter().copied());
            removed.insert(*frame);
        }
        // An operator that painted a kept frame too cannot be removed.
        for (frame, origin) in &origins {
            if !removed.contains(frame) && origin.shows.iter().any(|s| remove.contains(s)) {
                return Err(Fallback::on("operator_shared_with_kept_frame", index));
            }
        }
        let mut paragraphs = BTreeSet::new();
        let anchor =
            section_anchor(document, index).ok_or(Fallback::on("page_objects_changed", index))?;
        for inline in &anchor.inlines {
            if let InlineNode::TextBox(frame) = inline
                && before.frames.get(&frame.id) != now.frames.get(&frame.id)
            {
                if !text_only(frame) {
                    return Err(Fallback::on("frame_not_text_only", index));
                }
                paragraphs_of(frame, &mut paragraphs);
            }
        }
        plans.push(PagePlan {
            index,
            remove,
            paragraphs,
            removed_frames: removed.len(),
        });
    }

    let revision = PdfRevision::open(original.clone()).map_err(|r| Fallback::new(r.id()))?;
    if revision.page_count() != baseline.sources.pages.len() {
        return Err(Fallback::new("page_count_changed"));
    }
    let tail = revision.tail().clone();
    let shaper = ParleyShaper::new();
    let laid_out = paginate_document(document, &shaper);
    if laid_out.pages.len() != baseline.sources.pages.len() {
        return Err(Fallback::new("pagination_changed"));
    }
    let mut parts = Vec::with_capacity(plans.len());
    let mut lists = Vec::with_capacity(plans.len());
    for plan in &plans {
        let index = plan.index;
        if revision.page_object(index) != baseline.sources.pages[index].object {
            return Err(Fallback::on("page_object_moved", index));
        }
        let page_parts = revision
            .page_parts(index)
            .map_err(|r| Fallback::on(r.id(), index))?;
        let kept = remove_shows(&page_parts.content, &plan.remove)
            .map_err(|r| Fallback::on(r.id(), index))?;
        let laid = &laid_out.pages[index];
        let size = laid.page_size;
        let (width, height) = page_parts.display_size;
        if (f64::from(size.width.raw()) / 20.0 - width).abs() > 1.0
            || (f64::from(size.height.raw()) / 20.0 - height).abs() > 1.0
        {
            return Err(Fallback::on("page_size_changed", index));
        }
        lists.push((size, overlay_list(&compose_page(laid), &plan.paragraphs)));
        parts.push((page_parts, kept));
    }
    let mut pages = Vec::with_capacity(plans.len());
    for ((page_parts, kept), (size, list)) in parts.into_iter().zip(&lists) {
        let mut form_name = "OpenDocEdit".to_owned();
        let mut n = 0;
        while page_parts.xobject_names.contains(form_name.as_bytes()) {
            n += 1;
            form_name = format!("OpenDocEdit{n}");
        }
        pages.push(EditedPage {
            object: page_parts.object,
            entries: page_parts.entries,
            resource_entries: page_parts.resource_entries,
            xobject_entries: page_parts.xobject_entries,
            form_name,
            kept_content: kept,
            user_from_display: page_parts.user_from_display,
            overlay: PdfPage {
                width: size.width,
                height: size.height,
                list,
            },
        });
    }

    let base = IncrementalBase {
        prev_xref: tail.startxref,
        size: tail.size,
        root: String::from_utf8_lossy(&tail.root).into_owned(),
        info: tail
            .info
            .as_ref()
            .map(|info| String::from_utf8_lossy(info).into_owned()),
        id_first: tail
            .id_first
            .as_ref()
            .map(|id| String::from_utf8_lossy(id).into_owned()),
        xref_stream: tail.xref_stream,
    };
    let fonts = RegistryFontSource::new(&shaper.registry());
    let media = ResourceMedia(resources);
    let text = DocumentText::new(document);
    let export = write_incremental_update(original, &base, &pages, &fonts, &media, &text)
        .map_err(|_| Fallback::new("overlay_font_unembeddable"))?;

    let mut report = CompatibilityReport::default();
    let entry = |feature: &str, occurrences: usize, model, retention| CompatibilityEntry {
        feature: feature.to_owned(),
        occurrences: u32::try_from(occurrences).unwrap_or(u32::MAX),
        location: FeatureLocation::default(),
        model_outcome: model,
        retention_outcome: retention,
    };
    report.entries.push(entry(
        INCREMENTAL_UPDATE,
        plans.len(),
        ModelOutcome::Mapped,
        RetentionOutcome::NotApplicable,
    ));
    report.entries.push(entry(
        PAGE_CARRIED_OVER,
        baseline.sources.pages.len() - plans.len(),
        ModelOutcome::Mapped,
        RetentionOutcome::Preserved,
    ));
    let removed: usize = plans.iter().map(|p| p.removed_frames).sum();
    if removed > 0 {
        // Privacy, stated rather than implied: an incremental update keeps
        // the earlier revision, so the replaced text is still in the file.
        report.entries.push(entry(
            EARLIER_REVISION_RETAINED,
            removed,
            ModelOutcome::Mapped,
            RetentionOutcome::Preserved,
        ));
    }
    if tail.signed {
        report.entries.push(entry(
            SIGNATURE_EARLIER_REVISION,
            1,
            ModelOutcome::Mapped,
            RetentionOutcome::Preserved,
        ));
    }
    if tail.certified {
        report.entries.push(entry(
            CERTIFICATION_INVALIDATED,
            1,
            ModelOutcome::Degraded,
            RetentionOutcome::NotRetained,
        ));
    }
    for finding in export.findings {
        report.entries.push(CompatibilityEntry {
            feature: finding.code,
            occurrences: u32::try_from(finding.occurrences).unwrap_or(u32::MAX),
            location: FeatureLocation::default(),
            model_outcome: match finding.severity {
                PdfSeverity::Note => ModelOutcome::Mapped,
                PdfSeverity::Degraded => ModelOutcome::Degraded,
            },
            retention_outcome: match finding.severity {
                PdfSeverity::Note => RetentionOutcome::NotApplicable,
                PdfSeverity::Degraded => RetentionOutcome::NotRetained,
            },
        });
    }
    report.sort();
    Ok((export.bytes, report))
}

/// The anchor paragraph of section `index`, when that section is one
/// paragraph. O(body blocks).
fn section_anchor(document: &Document, index: usize) -> Option<&casual_doc_model::v1::Paragraph> {
    let mut section = 0;
    let mut start = 0;
    let blocks = document.body();
    for (at, block) in blocks.iter().enumerate() {
        let ends = matches!(block, BlockNode::Paragraph(p) if p.properties.section_break.is_some());
        if !ends && at + 1 != blocks.len() {
            continue;
        }
        if section == index {
            return match &blocks[start..=at] {
                [BlockNode::Paragraph(paragraph)] => Some(paragraph),
                _ => None,
            };
        }
        section += 1;
        start = at + 1;
    }
    None
}
