// SPDX-License-Identifier: Apache-2.0

//! The planning half of the byte-preserving PDF save (`docs/182` §7, §19, §17a).
//!
//! # What it decides
//!
//! At import, [`baseline`] records the content digest of every page's section,
//! of each text frame on it and of the rest of the document, beside the
//! [`SourceMap`] (page objects, and the show operators each frame's glyphs came
//! from). At save, [`plan`] takes the same digests of the document as it is now.
//! Unchanged-ness is **computed**, never recorded by an edit (G-6: no
//! `Operation` is added, ADR-030 I2):
//!
//! - the whole document equal to the import's (an edit undone):
//!   [`SaveAnswer::WriteBack`];
//! - a page whose section digest equals the import's is carried over and asked
//!   for nothing;
//! - an edited page's edited and deleted frames name the show operators to
//!   remove, and its edited and added frames name the paragraphs the overlay
//!   draws ([`PlannedPage`]);
//! - anything this cannot prove safe is [`SaveAnswer::Refused`] with a stable
//!   reason, which the writer reports as `pdf.export.preserve_fallback.<reason>`.
//!
//! The hayro half (open the original's last revision, read the edited pages'
//! parts, remove the shows) is the `read` callback: `casual-doc-pdf-read`'s
//! `revision::read_for_save`, in process.
//!
//! # Why a crate of its own
//!
//! The editor's main WebAssembly module links no PDF reader, and since
//! `docs/182` §17a it links none of this either: the PDF module it loads the
//! first time a PDF is opened computes the baseline when it imports and answers
//! the main module's save with this plan, in `casual-doc-pdf-wire` bytes. The
//! main module keeps only what needs its layout engine: the overlay's layout
//! and the update's writer (`casual-doc-io`'s `pdf_preserve`).
//!
//! The digests are the diff crate's content projection (`casual-doc-diff`
//! `projection`/`hash`, the machinery behind `identity::content_digest`): the
//! same definition of "same content" the version history uses.
//!
//! Complexity: O(document) for [`baseline`] and for [`plan`] (one projection
//! pass and one walk of the body), never per edit.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};

use casual_doc_diff::hash::ContentHasher;
use casual_doc_diff::projection::{Projector, StoryKey, ValueHashes};
use casual_doc_model::v1::{BlockNode, BreakKind, Document, InlineNode, Paragraph, TextBox};
use casual_doc_pdf_wire::{
    Baseline, FrameSource, PageRequest, PlannedPage, Refusal, RevisionParts, RevisionRequest,
    SaveAnswer, SectionDigest, SourceMap,
};

/// The import's digests of `document`, with its source map: what the main
/// module keeps beside the original and hands back at save. `None` when the
/// document cannot be digested (its body does not project), so no save of it
/// preserves. O(document).
#[must_use]
pub fn baseline(document: &Document, sources: SourceMap) -> Option<Baseline> {
    let digest = digest(document)?;
    Some(Baseline {
        document: digest.document,
        definitions: digest.definitions,
        stories: digest.stories,
        sections: digest.sections,
        sources,
    })
}

/// The document reduced to what the save compares.
struct Digest {
    document: u128,
    definitions: u128,
    stories: u128,
    sections: Vec<SectionDigest>,
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
                            frames.insert(frame.id.as_u128(), values.hash_value(&**frame));
                        }
                        other => fixed.write_u128(values.hash_value(other)),
                    }
                }
                (Some(fixed.finish()), frames.into_iter().collect())
            }
            _ => (None, Vec::new()),
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
fn paragraphs_of(frame: &TextBox, into: &mut BTreeSet<u128>) {
    for block in &frame.blocks {
        if let BlockNode::Paragraph(paragraph) = block {
            into.insert(paragraph.id.as_u128());
        }
    }
}

/// The anchor paragraph of section `index`, when that section is one
/// paragraph. O(body blocks).
fn section_anchor(document: &Document, index: usize) -> Option<&Paragraph> {
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

fn refuse(reason: &str, page: Option<usize>) -> SaveAnswer {
    SaveAnswer::Refused(Refusal {
        reason: reason.to_owned(),
        page: page.map(|page| u32::try_from(page).unwrap_or(u32::MAX)),
    })
}

/// One edited page's plan, before the reader is asked.
struct PagePlan {
    index: usize,
    remove: BTreeSet<u32>,
    paragraphs: BTreeSet<u128>,
    removed_frames: usize,
}

/// Plans the save of `document` against the import's `baseline`, asking
/// `read` for the edited pages' parts (`casual-doc-pdf-read`'s
/// `revision::read_for_save` over the original). O(document) plus the
/// reader's work.
///
/// The refusals, in the order they are checked: `body_not_projectable`,
/// `definitions_changed`, `stories_changed`, `page_count_changed`,
/// `page_objects_changed`, `page_operators_unnumbered`,
/// `frame_origin_unknown`, `frame_text_not_removable`,
/// `operator_shared_with_kept_frame`, `frame_not_text_only`, then whatever
/// the reader refuses with, then `page_count_changed` or `page_object_moved`
/// when its answer does not match the request.
#[must_use]
pub fn plan(
    document: &Document,
    baseline: &Baseline,
    read: &dyn Fn(&RevisionRequest) -> Result<RevisionParts, Refusal>,
) -> SaveAnswer {
    if casual_doc_diff::content_digest(document) == baseline.document {
        return SaveAnswer::WriteBack;
    }
    let Some(now) = digest(document) else {
        return refuse("body_not_projectable", None);
    };
    if now.definitions != baseline.definitions {
        return refuse("definitions_changed", None);
    }
    if now.stories != baseline.stories {
        return refuse("stories_changed", None);
    }
    let sources = &baseline.sources;
    if now.sections.len() != baseline.sections.len()
        || sources.pages.len() != baseline.sections.len()
    {
        return refuse("page_count_changed", None);
    }

    let mut plans = Vec::new();
    for (index, (now, before)) in now.sections.iter().zip(&baseline.sections).enumerate() {
        if now.whole == before.whole {
            continue;
        }
        if now.fixed.is_none() || now.fixed != before.fixed {
            return refuse("page_objects_changed", Some(index));
        }
        if sources.pages[index].shows.is_none() {
            return refuse("page_operators_unnumbered", Some(index));
        }
        let origins: BTreeMap<u128, &FrameSource> = sources
            .frames
            .iter()
            .filter(|frame| frame.page as usize == index)
            .map(|frame| (frame.frame, frame))
            .collect();
        let now_frames: BTreeMap<u128, u128> = now.frames.iter().copied().collect();
        let before_frames: BTreeMap<u128, u128> = before.frames.iter().copied().collect();
        let mut remove = BTreeSet::new();
        let mut removed = BTreeSet::new();
        for (frame, digest) in &before_frames {
            if now_frames.get(frame) == Some(digest) {
                continue;
            }
            let Some(origin) = origins.get(frame) else {
                return refuse("frame_origin_unknown", Some(index));
            };
            if !origin.complete {
                return refuse("frame_text_not_removable", Some(index));
            }
            remove.extend(origin.shows.iter().copied());
            removed.insert(*frame);
        }
        // An operator that painted a kept frame too cannot be removed.
        for (frame, origin) in &origins {
            if !removed.contains(frame) && origin.shows.iter().any(|s| remove.contains(s)) {
                return refuse("operator_shared_with_kept_frame", Some(index));
            }
        }
        let Some(anchor) = section_anchor(document, index) else {
            return refuse("page_objects_changed", Some(index));
        };
        let mut paragraphs = BTreeSet::new();
        for inline in &anchor.inlines {
            if let InlineNode::TextBox(frame) = inline {
                let id = frame.id.as_u128();
                if before_frames.get(&id) != now_frames.get(&id) {
                    if !text_only(frame) {
                        return refuse("frame_not_text_only", Some(index));
                    }
                    paragraphs_of(frame, &mut paragraphs);
                }
            }
        }
        plans.push(PagePlan {
            index,
            remove,
            paragraphs,
            removed_frames: removed.len(),
        });
    }

    // The hayro half: the file, the page count, each page's object, its
    // parts, the shows removed — refused in that order.
    let request = RevisionRequest {
        expected_pages: u32::try_from(sources.pages.len()).unwrap_or(u32::MAX),
        pages: plans
            .iter()
            .map(|plan| PageRequest {
                index: u32::try_from(plan.index).unwrap_or(u32::MAX),
                object: sources.pages[plan.index].object,
                remove: plan.remove.iter().copied().collect(),
            })
            .collect(),
    };
    let parts = match read(&request) {
        Ok(parts) => parts,
        Err(refusal) => return SaveAnswer::Refused(refusal),
    };
    if parts.page_count as usize != sources.pages.len() || parts.pages.len() != plans.len() {
        return refuse("page_count_changed", None);
    }
    for (plan, page) in plans.iter().zip(&parts.pages) {
        if page.index as usize != plan.index {
            return refuse("page_object_moved", Some(plan.index));
        }
    }
    let pages = plans
        .into_iter()
        .map(|plan| PlannedPage {
            index: u32::try_from(plan.index).unwrap_or(u32::MAX),
            paragraphs: plan.paragraphs.into_iter().collect(),
            removed_frames: u32::try_from(plan.removed_frames).unwrap_or(u32::MAX),
        })
        .collect();
    SaveAnswer::Update { parts, pages }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use casual_doc_pdf_import::{ImportJob, PdfReadLimits, Progress};
    use casual_doc_pdf_wire::PageParts;

    use super::*;

    const PROSE: &[u8] = include_bytes!("../../../fixtures/pdf/synthetic-prose.pdf");

    /// A real import of the two-page prose fixture, and its source map.
    fn imported() -> (Document, SourceMap) {
        let mut job = ImportJob::begin(PROSE.to_vec(), PdfReadLimits::default()).expect("begin");
        while job.step(8) != Progress::Done {}
        let import = job.finish().expect("finish");
        let sources = casual_doc_pdf_import::wire::sources(&import.sources);
        (import.document, sources)
    }

    /// A reader that answers every request with empty parts for the pages
    /// asked, and records what it was asked.
    fn recording(asked: &RefCell<Vec<RevisionRequest>>) -> impl Fn(&RevisionRequest) -> Result<RevisionParts, Refusal> + '_ {
        move |request: &RevisionRequest| {
            asked.borrow_mut().push(request.clone());
            Ok(RevisionParts {
                page_count: request.expected_pages,
                tail: casual_doc_pdf_wire::RevisionTail::default(),
                pages: request
                    .pages
                    .iter()
                    .map(|page| PageParts {
                        index: page.index,
                        object: page.object.unwrap_or((0, 0)),
                        entries: Vec::new(),
                        resource_entries: Vec::new(),
                        xobject_entries: Vec::new(),
                        xobject_names: Vec::new(),
                        kept_content: Vec::new(),
                        user_from_display: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                        display_size: (612.0, 792.0),
                    })
                    .collect(),
            })
        }
    }

    /// Replaces the first run of page `page`'s first frame with `text`.
    fn retype_first_frame(document: &mut Document, page: usize) -> u128 {
        let BlockNode::Paragraph(anchor) = &mut document.body_mut()[page] else {
            panic!("a page is one anchor paragraph");
        };
        let frame = anchor
            .inlines
            .iter_mut()
            .find_map(|inline| match inline {
                InlineNode::TextBox(frame) => Some(&mut **frame),
                _ => None,
            })
            .expect("a page holds a frame");
        let BlockNode::Paragraph(paragraph) = &mut frame.blocks[0] else {
            panic!("a frame holds a paragraph");
        };
        let InlineNode::Run(run) = &mut paragraph.inlines[0] else {
            panic!("a frame starts with a run");
        };
        run.text = format!("Edited {}", run.text);
        paragraph.id.as_u128()
    }

    #[test]
    fn an_unchanged_document_is_written_back_without_asking_the_reader() {
        let (document, sources) = imported();
        let baseline = baseline(&document, sources).expect("the import digests");
        let asked = RefCell::new(Vec::new());
        assert_eq!(
            plan(&document, &baseline, &recording(&asked)),
            SaveAnswer::WriteBack
        );
        assert!(asked.borrow().is_empty(), "nothing to read for a write-back");
    }

    #[test]
    fn an_edited_frame_rewrites_its_page_only_and_names_what_the_overlay_draws() {
        let (mut document, sources) = imported();
        let baseline = baseline(&document, sources).expect("the import digests");
        let edited = retype_first_frame(&mut document, 0);
        let asked = RefCell::new(Vec::new());
        let SaveAnswer::Update { parts, pages } = plan(&document, &baseline, &recording(&asked))
        else {
            panic!("an edited text frame is preserved, not refused");
        };
        assert_eq!(parts.pages.len(), 1);
        assert_eq!(pages.len(), 1, "page 2 is carried over");
        assert_eq!(pages[0].index, 0);
        assert_eq!(pages[0].removed_frames, 1);
        assert!(pages[0].paragraphs.contains(&edited));
        let asked = asked.borrow();
        assert_eq!(asked.len(), 1);
        assert_eq!(asked[0].expected_pages, 2);
        assert_eq!(asked[0].pages.len(), 1);
        assert!(
            !asked[0].pages[0].remove.is_empty(),
            "the edited frame's shows are removed"
        );
    }

    #[test]
    fn the_readers_refusal_is_the_answer() {
        let (mut document, sources) = imported();
        let baseline = baseline(&document, sources).expect("the import digests");
        retype_first_frame(&mut document, 1);
        let answer = plan(&document, &baseline, &|_| {
            Err(Refusal {
                reason: "kept_text_would_move".to_owned(),
                page: Some(1),
            })
        });
        assert_eq!(
            answer,
            SaveAnswer::Refused(Refusal {
                reason: "kept_text_would_move".to_owned(),
                page: Some(1),
            })
        );
    }

    #[test]
    fn a_baseline_of_another_page_count_is_refused_before_the_reader() {
        let (mut document, mut sources) = imported();
        sources.pages.pop();
        let baseline = baseline(&document, sources).expect("the import digests");
        retype_first_frame(&mut document, 0);
        let asked = RefCell::new(Vec::new());
        assert_eq!(
            plan(&document, &baseline, &recording(&asked)),
            refuse("page_count_changed", None)
        );
        assert!(asked.borrow().is_empty());
    }

    #[test]
    fn the_baseline_survives_its_wire_encoding() {
        let (document, sources) = imported();
        let baseline = baseline(&document, sources).expect("the import digests");
        assert_eq!(Baseline::decode(&baseline.encode()).unwrap(), baseline);
    }
}
