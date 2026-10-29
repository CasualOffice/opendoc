//! The writing half of the byte-preserving PDF save (`docs/182` §7, P4): what
//! an edited PDF-origin session writes when the original can be kept.
//!
//! # Two halves, two modules (`docs/182` §17a)
//!
//! The **plan** (which pages changed, which show operators to remove, which
//! paragraphs the overlay draws, or why nothing can be preserved) is
//! `casual-doc-pdf-save`'s, behind [`PdfSavePlanner`]: in process when the
//! `pdf-import` feature is on, and in the browser by the lazily loaded PDF
//! module, which also does the hayro half (opening the original's last
//! revision, reading the edited pages' parts, removing the shows). Its answer
//! is a `casual-doc-pdf-wire` [`SaveAnswer`].
//!
//! What is **here** is what needs the layout engine, which only the editor's
//! main module has: the edited pages laid out by the same pagination pass the
//! semantic writer uses, their display lists reduced to the overlay frames'
//! glyph runs, and the incremental update written by `casual-doc-pdf`'s
//! `write_incremental_update`. Everything here is hayro-free and digest-free,
//! so the main module links neither.
//!
//! Anything that cannot be written safely falls back to the semantic writer,
//! and the fallback is **reported** with its reason ([`Fallback`]).
//!
//! Complexity: O(document) at save (one pagination, the original's prefix
//! copy); never per edit.

use std::collections::BTreeSet;
use std::sync::Arc;

use casual_doc_layout::compose::compose_page;
use casual_doc_layout::display::{DisplayList, PaintItem};
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_model::NodeId;
use casual_doc_model::v1::Document;
use casual_doc_pdf::{
    DocumentText, EditedPage, IncrementalBase, PdfPage, PdfSeverity, RegistryFontSource,
    write_incremental_update,
};
use casual_doc_pdf_wire::{PlannedPage, RevisionParts, SaveAnswer};

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

/// The planning half of the save (`casual-doc-pdf-save`, `docs/182` §17a):
/// compares `document` with the import's `baseline` (the encoded
/// `casual-doc-pdf-wire` `Baseline` the session kept) and, for an edit it can
/// preserve, reads the edited pages of `original` with their shows removed.
///
/// In process this is `InProcessSavePlanner` (feature `pdf-import`); in the
/// browser it is the PDF module, called through the host.
pub trait PdfSavePlanner {
    /// The plan: write the original back, refuse with a reason, or the pages
    /// to rewrite with their parts. A planner that fails answers
    /// [`SaveAnswer::Refused`]; the save then falls back and reports it.
    fn plan(&self, original: &Arc<Vec<u8>>, baseline: &[u8], document: &Document) -> SaveAnswer;
}

/// Why the save fell back to the semantic writer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Fallback {
    /// A stable reason id.
    pub(crate) reason: String,
    /// The page it concerns, when it concerns one.
    pub(crate) page: Option<usize>,
}

impl Fallback {
    pub(crate) fn new(reason: &str) -> Self {
        Self {
            reason: reason.to_owned(),
            page: None,
        }
    }

    pub(crate) fn on(reason: &str, page: usize) -> Self {
        Self {
            reason: reason.to_owned(),
            page: Some(page),
        }
    }

    pub(crate) fn refused(refusal: casual_doc_pdf_wire::Refusal) -> Self {
        Self {
            reason: refusal.reason,
            page: refusal.page.map(|page| page as usize),
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
                local_name: Some(self.reason.clone()),
                attribute_name: None,
            },
            // The save is correct (the semantic writer renders the model), but
            // the original's objects are not kept.
            model_outcome: ModelOutcome::Degraded,
            retention_outcome: RetentionOutcome::NotRetained,
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

/// Writes the incremental update the planner's [`SaveAnswer::Update`]
/// describes, or says why it cannot: the edited pages laid out, their
/// overlays drawn, the update appended to `original`. O(document).
fn write_update(
    document: &Document,
    resources: &DocumentResources,
    original: &Arc<Vec<u8>>,
    revision: RevisionParts,
    plans: &[PlannedPage],
) -> Result<(Vec<u8>, CompatibilityReport), Fallback> {
    if revision.pages.len() != plans.len() {
        return Err(Fallback::new("page_count_changed"));
    }
    let page_count = revision.page_count as usize;
    let tail = revision.tail;
    let shaper = ParleyShaper::new();
    let laid_out = paginate_document(document, &shaper);
    if laid_out.pages.len() != page_count {
        return Err(Fallback::new("pagination_changed"));
    }
    // Two passes: `PdfPage` borrows its display list, so every page's list is
    // built and owned first and the pages borrow them afterwards.
    let mut parts = Vec::with_capacity(plans.len());
    let mut lists = Vec::with_capacity(plans.len());
    for (plan, page_parts) in plans.iter().zip(revision.pages) {
        let index = plan.index as usize;
        if page_parts.index != plan.index {
            return Err(Fallback::on("page_object_moved", index));
        }
        let laid = laid_out
            .pages
            .get(index)
            .ok_or(Fallback::on("pagination_changed", index))?;
        let size = laid.page_size;
        let (width, height) = page_parts.display_size;
        if (f64::from(size.width.raw()) / 20.0 - width).abs() > 1.0
            || (f64::from(size.height.raw()) / 20.0 - height).abs() > 1.0
        {
            return Err(Fallback::on("page_size_changed", index));
        }
        let paragraphs: BTreeSet<NodeId> = plan
            .paragraphs
            .iter()
            .filter_map(|id| NodeId::new(*id).ok())
            .collect();
        lists.push((size, overlay_list(&compose_page(laid), &paragraphs)));
        parts.push(page_parts);
    }
    let mut pages = Vec::with_capacity(parts.len());
    for (page_parts, (size, list)) in parts.into_iter().zip(&lists) {
        let mut form_name = "OpenDocEdit".to_owned();
        let mut n = 0;
        while page_parts
            .xobject_names
            .iter()
            .any(|name| name.as_slice() == form_name.as_bytes())
        {
            n += 1;
            form_name = format!("OpenDocEdit{n}");
        }
        pages.push(EditedPage {
            object: page_parts.object,
            entries: page_parts.entries,
            resource_entries: page_parts.resource_entries,
            xobject_entries: page_parts.xobject_entries,
            form_name,
            kept_content: page_parts.kept_content,
            user_from_display: page_parts.user_from_display,
            overlay: PdfPage {
                width: size.width,
                height: size.height,
                list,
            },
        });
    }

    let base = IncrementalBase {
        prev_xref: usize::try_from(tail.startxref).map_err(|_| Fallback::new("xref_unreadable"))?,
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
        page_count.saturating_sub(plans.len()),
        ModelOutcome::Mapped,
        RetentionOutcome::Preserved,
    ));
    let removed: usize = plans.iter().map(|p| p.removed_frames as usize).sum();
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

/// What a planner's answer becomes at save.
#[derive(Debug)]
pub(crate) enum Planned {
    /// The original, unchanged (an edit undone).
    WriteBack,
    /// The update's bytes and report.
    Written(Vec<u8>, CompatibilityReport),
    /// Fall back to the semantic writer, reporting why.
    Fallback(Fallback),
}

/// Runs `planner` and, for an update, writes it. O(document).
pub(crate) fn plan_and_write(
    planner: &dyn PdfSavePlanner,
    document: &Document,
    resources: &DocumentResources,
    original: &Arc<Vec<u8>>,
    baseline: &[u8],
) -> Planned {
    match planner.plan(original, baseline, document) {
        SaveAnswer::WriteBack => Planned::WriteBack,
        SaveAnswer::Refused(refusal) => Planned::Fallback(Fallback::refused(refusal)),
        SaveAnswer::Update { parts, pages } => {
            match write_update(document, resources, original, parts, &pages) {
                Ok((bytes, report)) => Planned::Written(bytes, report),
                Err(fallback) => Planned::Fallback(fallback),
            }
        }
    }
}
