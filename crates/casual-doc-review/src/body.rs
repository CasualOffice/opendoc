// SPDX-License-Identifier: Apache-2.0

//! The review-body primitives: splicing tracked insertions, deletions,
//! formatting changes and comment markers into a paragraph's inlines, deciding
//! them, and projecting them for a review pane. Moved verbatim out of
//! `casual-doc-wasm` (`docs/178` §8 A1); the editor and the Native SDK call the
//! same functions.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use casual_doc_edit::ParagraphIndex;
use casual_doc_edit::refused;
use casual_doc_edit::{
    FormatDelta, Operation, ReviewParagraphState, find_paragraph_any, surface_block_lists,
};
use casual_doc_layout::flow::node_plain_text;
use casual_doc_model::IdGenerator;
use casual_doc_model::NodeId;
use casual_doc_model::v1::*;

#[derive(Clone, Debug)]
pub struct RevisionIdAllocator {
    pub used: BTreeSet<u128>,
    pub next: u128,
}

impl RevisionIdAllocator {
    pub fn from_document(document: &Document) -> Self {
        let mut values = Vec::new();
        collect_review_revision_serial_ids_all(document, &mut values);
        let used = values
            .into_iter()
            .filter_map(|value| value.parse::<u128>().ok())
            .collect::<BTreeSet<_>>();
        let mut next = 0_u128;
        while used.contains(&next) {
            let Some(candidate) = next.checked_add(1) else {
                break;
            };
            next = candidate;
        }
        Self { used, next }
    }

    pub fn allocate(&mut self) -> Result<String, String> {
        let mut candidate = self.next;
        loop {
            if self.used.insert(candidate) {
                self.next = candidate.checked_add(1).unwrap_or(candidate);
                return Ok(candidate.to_string());
            }
            candidate = candidate
                .checked_add(1)
                .ok_or_else(|| "revision id space exhausted".to_owned())?;
        }
    }
}

/// A resolved `[start, end)` UTF-8 byte range anchoring a comment or revision
/// inside one paragraph's final-with-markup projection (docs/133) — the same
/// projection `selectionRects`/`caretRect` already address. `node` is an
/// opaque 32-hex `NodeId` string, passed back unchanged.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewAnchorJson {
    pub node: String,
    pub start: u32,
    pub end: u32,
}

/// One comment or threaded reply — the stable, typed element returned by
/// `WasmDocument::list_comments` and `WasmDocument::comment_thread`.
/// Replaces the untyped `comments` array formerly nested only inside
/// `WasmDocument::review_summary` (docs/81 REVIEW-GAP-022): this exact
/// field set is the public SDK contract and only grows additively within a
/// major version. `anchor` is absent when the comment's range markers could
/// not be resolved in the live document tree (docs/68: "no anchor cache").
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewCommentSummaryJson {
    /// The comment's 32-hex `NodeId` string.
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initials: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// `true` once resolved (`commentsExtended.xml` `w15:done`).
    pub resolved: bool,
    /// This comment's own paragraph join key (`w14:paraId`), if assigned.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub para_id: Option<String>,
    /// The thread root's `para_id`, present only on a reply.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_para_id: Option<String>,
    /// The comment body's plain text (multi-paragraph bodies join with `\n`).
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<ReviewAnchorJson>,
}

/// One tracked-change or formatting revision — the stable, typed element
/// returned by `WasmDocument::list_revisions`. Replaces the untyped
/// `revisions` array formerly nested only inside
/// `WasmDocument::review_summary` (docs/81 REVIEW-GAP-022): this exact field
/// set is the public SDK contract and only grows additively within a major
/// version. `move_pair` is present only on a paired `move_from`/`move_to`
/// revision (docs/81 paired-move review); `formatting_delta` is present only
/// on a `formatting` revision.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewRevisionSummaryJson {
    /// The revision's (or format-changed run's) 32-hex `NodeId` string.
    pub id: String,
    /// `"insertion" | "deletion" | "move_from" | "move_to" | "formatting"`.
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// Opaque OpenDoc editor decision-group id (docs/82); absent when the
    /// revision is not part of an editor-authored group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    /// `"typing" | "replacement" | "formatting"`, present iff `group_id` is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_kind: Option<String>,
    /// The producer's opaque `w:id`, if declared (docs/82).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision_id: Option<String>,
    pub text: String,
    pub anchor: ReviewAnchorJson,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub move_pair: Option<ReviewMovePairJson>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formatting_delta: Option<serde_json::Value>,
}

/// The paired source/destination anchor of one imported `MoveFrom`/`MoveTo`
/// suggestion (docs/81 paired-move review), correlated by their shared
/// move-range `name`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewMovePairJson {
    pub name: String,
    pub from_start: String,
    pub to_start: String,
}

/// Builds one `ReviewCommentSummaryJson`, resolving `id`'s anchor (if any)
/// from a pre-walked `anchors` list (shared across a whole
/// `listComments`/`commentThread` call so the document tree is walked once).
pub fn build_review_comment_summary(
    id: CommentId,
    comment: &Comment,
    anchors: &[(CommentId, NodeId, u32, u32)],
) -> ReviewCommentSummaryJson {
    let mut blocks = Vec::new();
    collect_block_text(&comment.blocks, &mut blocks);
    let anchor = anchors
        .iter()
        .find(|(anchor_id, _, _, _)| *anchor_id == id)
        .map(|(_, node, start, end)| ReviewAnchorJson {
            node: node.to_string(),
            start: *start,
            end: *end,
        });
    ReviewCommentSummaryJson {
        id: id.node_id().to_string(),
        author: comment.author.clone(),
        initials: comment.initials.clone(),
        date: comment.date.clone(),
        resolved: comment.done,
        para_id: comment.para_id.clone(),
        parent_para_id: comment.parent_para_id.clone(),
        text: blocks
            .into_iter()
            .map(|(_, text)| text)
            .collect::<Vec<_>>()
            .join("\n"),
        anchor,
    }
}

/// All comments and threaded replies, ordered by (creation-order) id — the
/// typed data backing `WasmDocument::list_comments`.
pub fn build_review_comment_summaries(document: &Document) -> Vec<ReviewCommentSummaryJson> {
    let mut anchors = Vec::new();
    collect_review_comment_anchors_all(document, &mut anchors);
    document
        .definitions()
        .comments
        .iter()
        .map(|(id, comment)| build_review_comment_summary(*id, comment, &anchors))
        .collect()
}

/// All tracked-change and formatting revisions in document order — the typed
/// data backing `WasmDocument::list_revisions`. `collect_review_revisions`
/// itself still builds each entry as a `serde_json::Value` (shared with
/// `review_summary`'s tree walk, unchanged for that method's back-compat
/// output), because a `formatting`-kind entry (from a `w:rPrChange` run) and
/// a `Revision`-kind entry share the array but carry different optional
/// fields (`formattingDelta` vs. `movePair`); this re-validates every value
/// against the merged typed schema so a schema drift between the two call
/// sites fails loudly instead of silently reaching JS untyped.
pub fn build_review_revision_summaries(document: &Document) -> Vec<ReviewRevisionSummaryJson> {
    let move_pairs = collect_review_move_pairs_all(document);
    let move_links = review_move_links(&move_pairs);
    let mut revisions = Vec::new();
    collect_review_revisions_all(document, &move_links, &mut revisions);
    revisions
        .into_iter()
        .map(|value| {
            serde_json::from_value(value)
                .expect("collect_review_revisions matches ReviewRevisionSummaryJson's schema")
        })
        .collect()
}

/// The set of keys under which `id`/`comment` may be referenced as a parent:
/// the id's own hex string, and its `para_id` join key if assigned. Mirrors
/// `WasmDocument::delete_comment`'s existing cascade-matching rule so
/// thread resolution and thread deletion agree on what counts as a child.
pub fn comment_parent_keys(id: CommentId, comment: &Comment) -> BTreeSet<String> {
    let mut keys = BTreeSet::from([id.node_id().to_string()]);
    if let Some(para_id) = comment.para_id.clone() {
        keys.insert(para_id);
    }
    keys
}

/// Walks `comment`'s `parent_para_id` chain up to its thread root (the
/// nearest ancestor with no resolvable parent), so
/// `WasmDocument::comment_thread` returns the same thread regardless of
/// which member's id the caller passes.
pub fn resolve_comment_thread_root(
    comments: &DefinitionMap<CommentId, Comment>,
    mut current: CommentId,
) -> CommentId {
    loop {
        let Some(comment) = comments.get(&current) else {
            return current;
        };
        let Some(parent_key) = comment.parent_para_id.as_deref() else {
            return current;
        };
        let parent = comments.iter().find(|(candidate, value)| {
            **candidate != current
                && (candidate.node_id().to_string() == parent_key
                    || value.para_id.as_deref() == Some(parent_key))
        });
        match parent {
            Some((candidate, _)) => current = *candidate,
            None => return current,
        }
    }
}

/// One comment thread — `comment`'s thread root plus every descendant reply,
/// ordered root-first by creation order — the typed data backing
/// `WasmDocument::comment_thread`.
pub fn build_review_comment_thread(
    document: &Document,
    comment: CommentId,
) -> Vec<ReviewCommentSummaryJson> {
    let comments = &document.definitions().comments;
    let root = resolve_comment_thread_root(comments, comment);
    let mut members = BTreeSet::from([root]);
    let mut parent_keys = comments
        .get(&root)
        .map(|value| comment_parent_keys(root, value))
        .unwrap_or_default();
    loop {
        let mut changed = false;
        for (id, value) in comments.iter() {
            if !members.contains(id)
                && value
                    .parent_para_id
                    .as_deref()
                    .is_some_and(|parent| parent_keys.contains(parent))
            {
                members.insert(*id);
                parent_keys.extend(comment_parent_keys(*id, value));
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut anchors = Vec::new();
    collect_review_comment_anchors_all(document, &mut anchors);
    members
        .into_iter()
        .filter_map(|id| {
            comments
                .get(&id)
                .map(|value| build_review_comment_summary(id, value, &anchors))
        })
        .collect()
}

/// Whether a comment's anchor range `[comment_start, comment_end)` overlaps a
/// revision's anchor range `[revision_start, revision_end)` in the shared
/// final-with-markup byte space (both ranges are same-paragraph, half-open, and
/// produced by the same offset walk). A revision with positive width uses the
/// standard half-open intersection. A zero-width revision (a pure deletion is
/// zero-width in this projection) threads under a comment whose range strictly
/// contains its point — `WasmDocument::revision_thread`'s overlap rule.
pub fn review_ranges_overlap(
    comment_start: u32,
    comment_end: u32,
    revision_start: u32,
    revision_end: u32,
) -> bool {
    if revision_start == revision_end {
        comment_start <= revision_start && revision_start < comment_end
    } else {
        comment_start < revision_end && revision_start < comment_end
    }
}

/// Allocates the eight-hex-digit paragraph join key required by the DOCX
/// comments companion parts. Imported keys remain untouched; editor-authored
/// comments use the next unused value so replies survive export and reopen.
pub fn next_comment_para_id(
    comments: &DefinitionMap<CommentId, Comment>,
) -> Result<String, String> {
    let used: BTreeSet<u32> = comments
        .iter()
        .filter_map(|(_, comment)| {
            comment
                .para_id
                .as_deref()
                .filter(|value| value.len() == 8)
                .and_then(|value| u32::from_str_radix(value, 16).ok())
        })
        .collect();
    let mut candidate = used
        .iter()
        .next_back()
        .copied()
        .unwrap_or(0)
        .wrapping_add(1);
    if candidate == 0 {
        candidate = 1;
    }
    for _ in 0..=used.len() {
        if !used.contains(&candidate) {
            return Ok(format!("{candidate:08X}"));
        }
        candidate = candidate.wrapping_add(1);
        if candidate == 0 {
            candidate = 1;
        }
    }
    Err("comment paragraph id space exhausted".to_owned())
}

#[derive(Clone, Debug)]
pub struct ReviewMoveRange {
    pub start: NodeId,
    pub end: NodeId,
    pub kind: MoveKind,
    pub name: String,
    pub revisions: Vec<NodeId>,
}

#[derive(Clone, Debug)]
pub struct ReviewMovePair {
    pub from: ReviewMoveRange,
    pub to: ReviewMoveRange,
}

#[derive(Clone, Debug)]
pub struct OpenReviewMoveRange {
    pub start: NodeId,
    pub kind: MoveKind,
    pub move_id: String,
    pub name: String,
    pub revisions: Vec<NodeId>,
}

#[derive(Clone, Debug)]
pub struct ReviewMoveLink {
    pub name: String,
    pub from_start: NodeId,
    pub to_start: NodeId,
}

pub fn collect_review_move_pairs(blocks: &[BlockNode]) -> Vec<ReviewMovePair> {
    let mut ranges = Vec::new();
    collect_review_move_ranges(blocks, &mut ranges);
    let mut by_name: BTreeMap<String, (Vec<ReviewMoveRange>, Vec<ReviewMoveRange>)> =
        BTreeMap::new();
    for range in ranges {
        let entry = by_name.entry(range.name.clone()).or_default();
        match range.kind {
            MoveKind::From => entry.0.push(range),
            MoveKind::To => entry.1.push(range),
        }
    }
    let mut pairs = Vec::new();
    for (_, (mut from, mut to)) in by_name {
        // A move name is producer-owned. Pair only the unambiguous standard
        // shape; guessing between duplicate source/destination ranges risks
        // accepting unrelated document content as one change.
        if from.len() == 1 && to.len() == 1 {
            let from = from.remove(0);
            let to = to.remove(0);
            if !from.revisions.is_empty() && !to.revisions.is_empty() {
                pairs.push(ReviewMovePair { from, to });
            }
        }
    }
    pairs
}

pub fn review_move_links(pairs: &[ReviewMovePair]) -> BTreeMap<NodeId, ReviewMoveLink> {
    let mut links = BTreeMap::new();
    for pair in pairs {
        let link = ReviewMoveLink {
            name: pair.from.name.clone(),
            from_start: pair.from.start,
            to_start: pair.to.start,
        };
        for id in pair.from.revisions.iter().chain(pair.to.revisions.iter()) {
            links.insert(*id, link.clone());
        }
    }
    links
}

pub fn collect_review_move_ranges(blocks: &[BlockNode], out: &mut Vec<ReviewMoveRange>) {
    let mut open = Vec::new();
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                collect_review_inline_move_ranges(&paragraph.inlines, &mut open, out);
            }
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        collect_review_move_ranges(&cell.blocks, out);
                    }
                }
            }
            BlockNode::Sdt(sdt) => collect_review_move_ranges(&sdt.blocks, out),
            BlockNode::AltChunk(_) => {}
        }
    }
}

pub fn collect_review_inline_move_ranges(
    inlines: &[InlineNode],
    open: &mut Vec<OpenReviewMoveRange>,
    out: &mut Vec<ReviewMoveRange>,
) {
    for inline in inlines {
        match inline {
            InlineNode::MoveRangeStart(marker) => open.push(OpenReviewMoveRange {
                start: marker.id,
                kind: marker.kind,
                move_id: marker.move_id.clone(),
                name: marker.name.clone(),
                revisions: Vec::new(),
            }),
            InlineNode::MoveRangeEnd(marker) => {
                if let Some(index) = open
                    .iter()
                    .rposition(|range| range.kind == marker.kind && range.move_id == marker.move_id)
                {
                    let range = open.remove(index);
                    out.push(ReviewMoveRange {
                        start: range.start,
                        end: marker.id,
                        kind: range.kind,
                        name: range.name,
                        revisions: range.revisions,
                    });
                }
            }
            InlineNode::Revision(revision) => {
                let move_kind = match revision.kind {
                    RevisionKind::MoveFrom => Some(MoveKind::From),
                    RevisionKind::MoveTo => Some(MoveKind::To),
                    RevisionKind::Insertion | RevisionKind::Deletion => None,
                };
                if let Some(move_kind) = move_kind
                    && let Some(range) = open.iter_mut().rev().find(|range| range.kind == move_kind)
                {
                    range.revisions.push(revision.id);
                }
                collect_review_inline_move_ranges(&revision.inlines, open, out);
            }
            InlineNode::Hyperlink(link) => {
                collect_review_inline_move_ranges(&link.inlines, open, out);
            }
            InlineNode::Sdt(sdt) => {
                collect_review_inline_move_ranges(&sdt.inlines, open, out);
            }
            _ => {}
        }
    }
}

pub fn collect_review_revisions(
    notes: &NoteAnchorLengths,
    blocks: &[BlockNode],
    move_links: &BTreeMap<NodeId, ReviewMoveLink>,
    out: &mut Vec<serde_json::Value>,
) {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                // A paragraph formatting change covers the whole paragraph, so it
                // lists before the paragraph's inline changes; a paragraph-mark
                // revision sits where the mark is — at the end — so it lists after
                // them. Neither has a node id of its own and one paragraph can
                // carry both, so their ids are the paragraph id qualified by kind
                // (docs/108 Decision 3).
                let format_at = out.len();
                let mut offset = 0;
                collect_review_inline(
                    notes,
                    &paragraph.inlines,
                    paragraph.id,
                    &mut offset,
                    true,
                    move_links,
                    out,
                );
                let properties = &paragraph.properties;
                if let Some(change) = &properties.prop_change {
                    out.insert(
                        format_at,
                        serde_json::json!({
                            "id": paragraph_revision_id(paragraph.id, ParagraphRevisionPart::Format),
                            "kind": "paragraph_format",
                            "author": change.author,
                            "date": change.date,
                            "revisionId": change.revision_id,
                            "text": "",
                            "formattingDelta": paragraph_formatting_delta_json(
                                properties,
                                change.prior.as_ref(),
                            ),
                            "anchor": {
                                "node": paragraph.id.to_string(),
                                "start": 0,
                                "end": offset,
                            },
                        }),
                    );
                }
                if let Some(mark) = &properties.mark_revision {
                    out.push(serde_json::json!({
                        "id": paragraph_revision_id(paragraph.id, ParagraphRevisionPart::Mark),
                        "kind": match mark.kind {
                            casual_doc_model::v1::MarkRevisionKind::Insertion => "paragraph_mark_insertion",
                            casual_doc_model::v1::MarkRevisionKind::Deletion => "paragraph_mark_deletion",
                        },
                        "author": mark.author,
                        "date": mark.date,
                        "revisionId": mark.revision_id,
                        "text": "",
                        "anchor": {
                            "node": paragraph.id.to_string(),
                            "start": offset,
                            "end": offset,
                        },
                    }));
                }
            }
            BlockNode::Table(table) => {
                // A whole-table change is ONE card; its cells' text revisions are part of
                // it, not cards of their own (`table_review`).
                if let Some(kind) = crate::table_review::tracked_table_kind(table) {
                    out.push(crate::table_review::table_revision_item(table, kind));
                    continue;
                }
                for row in &table.rows {
                    for cell in &row.cells {
                        collect_review_revisions(notes, &cell.blocks, move_links, out);
                    }
                }
            }
            BlockNode::Sdt(sdt) => collect_review_revisions(notes, &sdt.blocks, move_links, out),
            BlockNode::AltChunk(_) => {}
        }
    }
}

pub fn collect_review_comment_anchors(
    notes: &NoteAnchorLengths,
    blocks: &[BlockNode],
    out: &mut Vec<(casual_doc_model::v1::CommentId, NodeId, u32, u32)>,
) {
    // The pending-start map is threaded across paragraphs, not reset per
    // paragraph, so a comment whose `CommentRangeEnd` lands in a later paragraph
    // than its `CommentRangeStart` still resolves to an anchor (multi-paragraph
    // comments). Each pending start records its node, offset, and its
    // paragraph's projected text length so a cross-paragraph end can anchor to
    // the start marker over the remainder of the start paragraph.
    let mut starts = std::collections::BTreeMap::new();
    collect_review_comment_anchors_inner(notes, blocks, &mut starts, out);
}

pub fn collect_review_comment_anchors_inner(
    notes: &NoteAnchorLengths,
    blocks: &[BlockNode],
    starts: &mut std::collections::BTreeMap<casual_doc_model::v1::CommentId, (NodeId, u32, u32)>,
    out: &mut Vec<(casual_doc_model::v1::CommentId, NodeId, u32, u32)>,
) {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                let para_len = paragraph
                    .inlines
                    .iter()
                    .map(|inline| inline_anchor_len(notes, inline))
                    .sum();
                let mut offset = 0;
                for inline in &paragraph.inlines {
                    collect_review_comment_inline(
                        notes,
                        inline,
                        paragraph.id,
                        para_len,
                        &mut offset,
                        true,
                        starts,
                        out,
                    );
                }
            }
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        collect_review_comment_anchors_inner(notes, &cell.blocks, starts, out);
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                collect_review_comment_anchors_inner(notes, &sdt.blocks, starts, out);
            }
            BlockNode::AltChunk(_) => {}
        }
    }
}

pub fn review_paragraph_body(document: &Document, node: NodeId) -> Result<Vec<BlockNode>, String> {
    let paragraph =
        find_paragraph_any(document, node).ok_or_else(|| "paragraph not found".to_owned())?;
    Ok(vec![BlockNode::Paragraph(paragraph.clone())])
}

pub fn collect_changed_review_paragraphs(
    by_id: &ParagraphIndex<'_>,
    blocks: &[BlockNode],
    out: &mut Vec<ReviewParagraphState>,
) -> Result<(), String> {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                let previous = by_id
                    .paragraph(paragraph.id)
                    .ok_or_else(|| "review command introduced an unknown paragraph".to_owned())?;
                if previous.inlines != paragraph.inlines {
                    out.push(ReviewParagraphState {
                        node: paragraph.id,
                        inlines: paragraph.inlines.clone(),
                    });
                }
            }
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        collect_changed_review_paragraphs(by_id, &cell.blocks, out)?;
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                collect_changed_review_paragraphs(by_id, &sdt.blocks, out)?;
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    Ok(())
}

/// Every surface's blocks, copied so a review command can edit them in place.
///
/// Review commands used to copy `document.body()` alone, mutate that, and diff
/// it back. Collection and anchor resolution were already all-surface, so the
/// asymmetry was invisible until a document carried review content outside the
/// body — which needs no authoring at all: an imported DOCX with `w:ins` in a
/// header is enough. Accept All then left those revisions in place, and deleting
/// a comment anchored in a header stripped the definition while its markers
/// survived, so the edit failed validation and rolled back under an error about
/// a value being too large.
pub fn review_surfaces(document: &Document) -> Vec<Vec<BlockNode>> {
    surface_block_lists(document)
        .into_iter()
        .map(<[BlockNode]>::to_vec)
        .collect()
}

/// [`update_review_operation`] over every surface rather than the body alone.
pub fn update_review_operation_across(
    document: &Document,
    surfaces: &[Vec<BlockNode>],
    comments: Option<DefinitionMap<CommentId, Comment>>,
) -> Result<Operation, String> {
    let mut paragraphs = Vec::new();
    // One index for the whole diff: this walks every paragraph of every surface,
    // and resolving each id against the pre-edit document walked it again.
    let by_id = ParagraphIndex::build(document);
    for blocks in surfaces {
        collect_changed_review_paragraphs(&by_id, blocks, &mut paragraphs)?;
    }
    if paragraphs.is_empty() && comments.is_none() {
        return Err("review command made no change".to_owned());
    }
    Ok(Operation::UpdateReviewState {
        paragraphs,
        comments,
    })
}

pub fn update_review_operation(
    document: &Document,
    body: &[BlockNode],
    comments: Option<DefinitionMap<CommentId, Comment>>,
) -> Result<Operation, String> {
    let mut paragraphs = Vec::new();
    // **A suggested keystroke edits ONE paragraph, and must not index the document to find
    // it.** `review_paragraph_body` hands this exactly one top-level paragraph, and
    // `ParagraphIndex::build` walks every surface AND allocates a hash entry per paragraph —
    // 1.3 million of them per keystroke on the owner's file, to answer one lookup. The index
    // is the right tool for the batch shape below (`update_review_operation_across`, where
    // resolving each id separately would be quadratic) and the wrong one for this
    // (`104` HF-111, `107` §4 B1).
    if let [BlockNode::Paragraph(edited)] = body {
        let previous = casual_doc_edit::find_paragraph_any(document, edited.id)
            .ok_or_else(|| "review command introduced an unknown paragraph".to_owned())?;
        if previous.inlines != edited.inlines {
            paragraphs.push(ReviewParagraphState {
                node: edited.id,
                inlines: edited.inlines.clone(),
            });
        }
    } else {
        let by_id = ParagraphIndex::build(document);
        collect_changed_review_paragraphs(&by_id, body, &mut paragraphs)?;
    }
    if paragraphs.is_empty() && comments.is_none() {
        return Err("review command made no change".to_owned());
    }
    Ok(Operation::UpdateReviewState {
        paragraphs,
        comments,
    })
}

pub fn validate_authored_revision_author(author: Option<&str>) -> Result<(), String> {
    if author.is_some_and(|value| !value.is_empty() && value.len() <= 255) {
        Ok(())
    } else {
        Err(refused!(
            "review.author-required",
            "Set an author name before making suggestions."
        )
        .to_owned())
    }
}

/// Validates a trimmed, non-empty `setActiveAuthor` name against the same
/// 255-byte bound as an authored comment/revision author.
pub fn validate_active_author_name(name: &str) -> Result<(), String> {
    if name.len() <= 255 {
        Ok(())
    } else {
        Err(refused!("review.author-too-long", "That author name is too long.").to_owned())
    }
}

/// Why `addComment` refuses a range it cannot mark up: one of its endpoints does
/// not fall on a boundary between two inlines of the paragraph, so a marker there
/// would have to be placed INSIDE something that cannot be split — the middle of
/// an equation, a field's cached result, or a text box's own story.
///
/// One constant so the message the host shows and the message the guards assert
/// are the same string.
pub const COMMENT_RANGE_REFUSAL: &str =
    "comment range must fall on inline boundaries of editable text";

#[allow(clippy::too_many_arguments)]
pub fn insert_review_comment_markers(
    notes: &NoteAnchorLengths,
    blocks: &mut [BlockNode],
    node: NodeId,
    start: u32,
    end: u32,
    range_start: CommentRangeStart,
    range_end: CommentRangeEnd,
    reference: CommentReference,
    ids: &mut IdGenerator,
) -> bool {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) if paragraph.id == node => {
                if !review_split_top_level_run(notes, &mut paragraph.inlines, start, ids)
                    || !review_split_top_level_run(notes, &mut paragraph.inlines, end, ids)
                {
                    return false;
                }
                let mut offset = 0;
                let mut start_index = None;
                let mut end_index = None;
                for (index, inline) in paragraph.inlines.iter().enumerate() {
                    if offset == start {
                        start_index = Some(index);
                    }
                    if offset == end {
                        end_index = Some(index);
                        break;
                    }
                    // Measure EVERY inline, not just runs. This walk used to
                    // advance only past a `Run` and refuse at anything else, so a
                    // comment whose range lay at or after any non-run inline in the
                    // paragraph was refused outright: a page-number field, a
                    // footnote marker, an equation, a picture, a bookmark marker or
                    // a comment marker from an earlier comment was enough. In a
                    // document numbered "Page 7 of 9" the text after the field
                    // could not be commented at all.
                    //
                    // An endpoint that does not land on a boundary is still
                    // refused, and that falls out of the arithmetic rather than
                    // needing a check: `review_split_top_level_run` above has
                    // already split a run at each endpoint, so an offset inside a
                    // run IS a boundary by the time this runs, while an offset
                    // inside an unsplittable inline is never equal to `offset` and
                    // leaves `start_index`/`end_index` `None`.
                    offset = offset.saturating_add(inline_anchor_len(notes, inline));
                }
                let Some(start_index) = start_index else {
                    return false;
                };
                let Some(mut end_index) =
                    end_index.or_else(|| (offset == end).then_some(paragraph.inlines.len()))
                else {
                    return false;
                };
                paragraph
                    .inlines
                    .insert(start_index, InlineNode::CommentRangeStart(range_start));
                end_index += 1;
                paragraph
                    .inlines
                    .insert(end_index, InlineNode::CommentRangeEnd(range_end));
                paragraph
                    .inlines
                    .insert(end_index + 1, InlineNode::CommentReference(reference));
                return true;
            }
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        if insert_review_comment_markers(
                            notes,
                            &mut cell.blocks,
                            node,
                            start,
                            end,
                            range_start,
                            range_end,
                            reference,
                            ids,
                        ) {
                            return true;
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if insert_review_comment_markers(
                    notes,
                    &mut sdt.blocks,
                    node,
                    start,
                    end,
                    range_start,
                    range_end,
                    reference,
                    ids,
                ) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Inserts `markers` (in order) at `offset` within paragraph `node`, splitting
/// the top-level run at the boundary first, and recursing into tables and
/// structured-document tags so a paragraph in any block container is reachable.
/// This is the cross-node building block for `WasmDocument::add_comment`: a
/// multi-paragraph comment places its start marker in one paragraph and its end
/// marker plus reference in another with two calls. Returns `false` when the
/// node is absent or `offset` does not fall on a boundary between two of its
/// top-level inlines — the same restriction as `insert_review_comment_markers`,
/// and the one [`COMMENT_RANGE_REFUSAL`] describes.
///
/// **O(inlines in the paragraph)** per call, plus the descent into block
/// containers to find the paragraph; no document walk and no by-id lookup.
pub fn insert_comment_markers_at(
    notes: &NoteAnchorLengths,
    blocks: &mut [BlockNode],
    node: NodeId,
    offset: u32,
    markers: &[InlineNode],
    ids: &mut IdGenerator,
) -> bool {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) if paragraph.id == node => {
                if !review_split_top_level_run(notes, &mut paragraph.inlines, offset, ids) {
                    return false;
                }
                let mut cursor = 0;
                let mut insert_at = None;
                for (index, inline) in paragraph.inlines.iter().enumerate() {
                    if cursor == offset {
                        insert_at = Some(index);
                        break;
                    }
                    // Every inline is measured — see the same change in
                    // `insert_review_comment_markers`. A cross-paragraph comment
                    // whose start paragraph held a field could not be created
                    // either, for the same reason and in the same walk shape.
                    cursor = cursor.saturating_add(inline_anchor_len(notes, inline));
                }
                let Some(insert_at) =
                    insert_at.or_else(|| (cursor == offset).then_some(paragraph.inlines.len()))
                else {
                    return false;
                };
                for (k, marker) in markers.iter().enumerate() {
                    paragraph.inlines.insert(insert_at + k, marker.clone());
                }
                return true;
            }
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        if insert_comment_markers_at(
                            notes,
                            &mut cell.blocks,
                            node,
                            offset,
                            markers,
                            ids,
                        ) {
                            return true;
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if insert_comment_markers_at(notes, &mut sdt.blocks, node, offset, markers, ids) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

pub fn insert_review_revision(
    notes: &NoteAnchorLengths,
    blocks: &mut [BlockNode],
    node: NodeId,
    offset: u32,
    revision: Revision,
    ids: &mut IdGenerator,
) -> bool {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) if paragraph.id == node => {
                if !review_split_top_level_run(notes, &mut paragraph.inlines, offset, ids) {
                    return false;
                }
                let mut cursor = 0;
                for index in 0..=paragraph.inlines.len() {
                    if cursor == offset {
                        paragraph
                            .inlines
                            .insert(index, InlineNode::Revision(Box::new(revision)));
                        return true;
                    }
                    if let Some(inline) = paragraph.inlines.get(index) {
                        cursor = cursor.saturating_add(inline_anchor_len(notes, inline));
                    }
                }
                return false;
            }
            BlockNode::Paragraph(_) => continue,
            BlockNode::Table(table) => {
                return table.rows.iter_mut().any(|row| {
                    row.cells.iter_mut().any(|cell| {
                        insert_review_revision(
                            notes,
                            &mut cell.blocks,
                            node,
                            offset,
                            revision.clone(),
                            ids,
                        )
                    })
                });
            }
            BlockNode::Sdt(sdt) => {
                return insert_review_revision(
                    notes,
                    &mut sdt.blocks,
                    node,
                    offset,
                    revision.clone(),
                    ids,
                );
            }
            BlockNode::AltChunk(_) => continue,
        }
    }
    false
}

/// docs/86 decision 1: typing *inside* the current author's own pending
/// insertion extends that same suggestion instead of failing or starting a new
/// one. If `offset` is strictly interior to an `Insertion` revision authored by
/// `author` (and editor-grouped, i.e. a suggestion this session can continue),
/// splice `addition` into its runs at the local offset and return the group so a
/// typing session keeps merging into it. Returns `None` when no such suggestion
/// contains the offset (the caller then takes the top-level insert path).
pub fn extend_authored_insertion_interior(
    notes: &NoteAnchorLengths,
    blocks: &mut [BlockNode],
    node: NodeId,
    offset: u32,
    addition: &[InlineNode],
    author: Option<&str>,
    ids: &mut IdGenerator,
) -> Option<RevisionGroup> {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) if paragraph.id == node => {
                let mut cursor = 0_u32;
                for inline in &mut paragraph.inlines {
                    let next = cursor.saturating_add(inline_anchor_len(notes, inline));
                    if offset > cursor && offset < next {
                        let InlineNode::Revision(revision) = inline else {
                            return None;
                        };
                        if revision.kind != RevisionKind::Insertion
                            || revision.author.as_deref() != author
                        {
                            return None;
                        }
                        let group = revision.editor_group?;
                        let local = offset - cursor;
                        if splice_addition_into_runs(
                            notes,
                            &mut revision.inlines,
                            local,
                            addition,
                            ids,
                        ) {
                            coalesce_review_runs(&mut revision.inlines);
                            return Some(group);
                        }
                        return None;
                    }
                    cursor = next;
                }
                return None;
            }
            BlockNode::Paragraph(_) => continue,
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        if let Some(group) = extend_authored_insertion_interior(
                            &NoteAnchorLengths::default(),
                            &mut cell.blocks,
                            node,
                            offset,
                            addition,
                            author,
                            ids,
                        ) {
                            return Some(group);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(group) = extend_authored_insertion_interior(
                    &NoteAnchorLengths::default(),
                    &mut sdt.blocks,
                    node,
                    offset,
                    addition,
                    author,
                    ids,
                ) {
                    return Some(group);
                }
            }
            BlockNode::AltChunk(_) => continue,
        }
    }
    None
}

/// Splices `addition` into a suggestion's runs at byte `local`, splitting the run
/// the offset lands inside (fresh id for the tail). Returns `false` if `local`
/// falls inside a non-run inline (a suggestion normally wraps plain runs) or at a
/// non-char boundary. Callers `coalesce_review_runs` afterwards, so a default-
/// property insert merges back into one run.
pub fn splice_addition_into_runs(
    notes: &NoteAnchorLengths,
    inlines: &mut Vec<InlineNode>,
    local: u32,
    addition: &[InlineNode],
    ids: &mut IdGenerator,
) -> bool {
    let mut cursor = 0_u32;
    for index in 0..inlines.len() {
        let len = inline_anchor_len(notes, &inlines[index]);
        let next = cursor.saturating_add(len);
        if local > cursor && local < next {
            let InlineNode::Run(run) = &inlines[index] else {
                return false;
            };
            let split = (local - cursor) as usize;
            if !run.text.is_char_boundary(split) {
                return false;
            }
            let tail_id = match ids.next_id() {
                Ok(id) => id,
                Err(_) => return false,
            };
            let tail = Run {
                id: tail_id,
                properties: run.properties.clone(),
                text: run.text[split..].to_owned(),
            };
            if let InlineNode::Run(head) = &mut inlines[index] {
                head.text.truncate(split);
            }
            let mut insert_at = index + 1;
            for node in addition.iter().cloned() {
                inlines.insert(insert_at, node);
                insert_at += 1;
            }
            inlines.insert(insert_at, InlineNode::Run(tail));
            return true;
        }
        cursor = next;
    }
    false
}

pub fn extend_review_group_insertion(
    notes: &NoteAnchorLengths,
    blocks: &mut [BlockNode],
    node: NodeId,
    offset: u32,
    group: RevisionGroup,
    addition: Vec<InlineNode>,
) -> bool {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) if paragraph.id == node => {
                let mut cursor = 0_u32;
                for inline in &mut paragraph.inlines {
                    let next = cursor.saturating_add(inline_anchor_len(notes, inline));
                    if next == offset
                        && let InlineNode::Revision(revision) = inline
                        && revision.kind == RevisionKind::Insertion
                        && revision.editor_group == Some(group)
                    {
                        revision.inlines.extend(addition);
                        coalesce_review_runs(&mut revision.inlines);
                        return true;
                    }
                    cursor = next;
                }
                return false;
            }
            BlockNode::Paragraph(_) => continue,
            BlockNode::Table(table) => {
                return table.rows.iter_mut().any(|row| {
                    row.cells.iter_mut().any(|cell| {
                        extend_review_group_insertion(
                            notes,
                            &mut cell.blocks,
                            node,
                            offset,
                            group,
                            addition.clone(),
                        )
                    })
                });
            }
            BlockNode::Sdt(sdt) => {
                return extend_review_group_insertion(
                    notes,
                    &mut sdt.blocks,
                    node,
                    offset,
                    group,
                    addition,
                );
            }
            BlockNode::AltChunk(_) => continue,
        }
    }
    false
}

/// The top-level [`Revision`] wrapper that wholly contains `[start, end)`, with
/// the wrapper's own starting offset — or `None` when the range spans wrappers or
/// covers plain text as well.
///
/// Only a wholly-contained range is claimed. A range that straddles a wrapper
/// boundary is the mixed case of docs/86 decision 4, where each covered segment
/// has to be handled by origin; answering `Some` here would silently apply one
/// rule to all of it.
pub fn enclosing_revision(
    notes: &NoteAnchorLengths,
    inlines: &[InlineNode],
    start: u32,
    end: u32,
) -> Option<(usize, u32)> {
    let mut cursor: u32 = 0;
    for (index, inline) in inlines.iter().enumerate() {
        let next = cursor.saturating_add(inline_anchor_len(notes, inline));
        if matches!(inline, InlineNode::Revision(_))
            && start >= cursor
            && end <= next
            && start < end
        {
            return Some((index, cursor));
        }
        cursor = next;
    }
    None
}

/// [`wrap_review_deletion`]'s inline half, so a deletion can be recorded inside a
/// wrapper as well as beside one. Recurses, so a revision nested in a revision
/// resolves too.
pub fn wrap_review_deletion_in_inlines(
    notes: &NoteAnchorLengths,
    inlines: &mut Vec<InlineNode>,
    start: u32,
    end: u32,
    revision: Revision,
    ids: &mut IdGenerator,
) -> bool {
    if let Some((index, base)) = enclosing_revision(notes, inlines, start, end) {
        let InlineNode::Revision(wrapper) = &mut inlines[index] else {
            return false;
        };
        return wrap_review_deletion_in_inlines(
            notes,
            &mut wrapper.inlines,
            start - base,
            end - base,
            revision,
            ids,
        );
    }
    if !review_split_top_level_run(notes, inlines, start, ids)
        || !review_split_top_level_run(notes, inlines, end, ids)
    {
        return false;
    }
    let mut cursor: u32 = 0;
    let mut first = None;
    let mut last = None;
    for (index, inline) in inlines.iter().enumerate() {
        let len = inline_anchor_len(notes, inline);
        if cursor == start && first.is_none() {
            first = Some(index);
        }
        cursor = cursor.saturating_add(len);
        if cursor == end {
            last = Some(index);
            break;
        }
    }
    let (Some(first), Some(last)) = (first, last) else {
        return false;
    };
    let children = inlines.drain(first..=last).collect();
    inlines.insert(
        first,
        InlineNode::Revision(Box::new(Revision {
            inlines: children,
            ..revision
        })),
    );
    true
}

pub fn wrap_review_deletion(
    notes: &NoteAnchorLengths,
    blocks: &mut [BlockNode],
    node: NodeId,
    start: u32,
    end: u32,
    revision: Revision,
    ids: &mut IdGenerator,
) -> bool {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) if paragraph.id == node => {
                // A range that lies wholly inside another author's tracked range
                // is nested there rather than refused. Word does the same: a
                // `w:del` inside a `w:ins` records "this reviewer wants that
                // reviewer's addition gone" without touching a character of it,
                // which is exactly what docs/86 decision 4 asks for. Refusing
                // instead is why a second reviewer's Backspace vanished with only
                // a status line to show for it.
                //
                // Checked BEFORE splitting at top level: the range is expressed in
                // review coordinates, and once it is known to be interior the
                // split has to happen inside the wrapper, not around it.
                if let Some((index, base)) =
                    enclosing_revision(notes, &paragraph.inlines, start, end)
                {
                    let InlineNode::Revision(wrapper) = &mut paragraph.inlines[index] else {
                        return false;
                    };
                    return wrap_review_deletion_in_inlines(
                        notes,
                        &mut wrapper.inlines,
                        start - base,
                        end - base,
                        revision,
                        ids,
                    );
                }
                if !review_split_top_level_run(notes, &mut paragraph.inlines, start, ids)
                    || !review_split_top_level_run(notes, &mut paragraph.inlines, end, ids)
                {
                    return false;
                }
                let mut cursor = 0_u32;
                let mut first = None;
                let mut last = None;
                for (index, inline) in paragraph.inlines.iter().enumerate() {
                    let InlineNode::Run(run) = inline else {
                        // Advance by the inline's projected width so `start`/`end`
                        // stay in projected coordinates even when a contributing
                        // revision precedes the range — e.g. an insertion the
                        // author kept while suggesting-deletion of adjacent
                        // accepted text (docs/86 decision 2). Previously the
                        // cursor only counted top-level runs, so any such wrapper
                        // shifted the range and the wrap silently failed.
                        cursor = cursor.saturating_add(inline_anchor_len(notes, inline));
                        continue;
                    };
                    if cursor == start {
                        first = Some(index);
                    }
                    cursor = cursor.saturating_add(run.text.len() as u32);
                    if cursor == end {
                        last = Some(index);
                        break;
                    }
                }
                let (Some(first), Some(last)) = (first, last) else {
                    return false;
                };
                let children = paragraph.inlines.drain(first..=last).collect();
                paragraph.inlines.insert(
                    first,
                    InlineNode::Revision(Box::new(Revision {
                        inlines: children,
                        ..revision
                    })),
                );
                return true;
            }
            BlockNode::Paragraph(_) => continue,
            BlockNode::Table(table) => {
                return table.rows.iter_mut().any(|row| {
                    row.cells.iter_mut().any(|cell| {
                        wrap_review_deletion(
                            notes,
                            &mut cell.blocks,
                            node,
                            start,
                            end,
                            revision.clone(),
                            ids,
                        )
                    })
                });
            }
            BlockNode::Sdt(sdt) => {
                return wrap_review_deletion(
                    notes,
                    &mut sdt.blocks,
                    node,
                    start,
                    end,
                    revision.clone(),
                    ids,
                );
            }
            BlockNode::AltChunk(_) => continue,
        }
    }
    false
}

/// Backspace/Delete over the current author's pending insertion edits that
/// insertion in place, matching Word/Docs behavior. It must not create a
/// deletion-of-an-insertion pair for text that has never been accepted.
pub fn remove_authored_review_insertion(
    notes: &NoteAnchorLengths,
    blocks: &mut [BlockNode],
    node: NodeId,
    start: u32,
    end: u32,
    author: Option<&str>,
) -> bool {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) if paragraph.id == node => {
                let mut cursor = 0_u32;
                let mut edits = Vec::new();
                for (index, inline) in paragraph.inlines.iter().enumerate() {
                    let next = cursor.saturating_add(inline_anchor_len(notes, inline));
                    if next > start && cursor < end {
                        let InlineNode::Revision(revision) = inline else {
                            return false;
                        };
                        if revision.kind != RevisionKind::Insertion
                            || revision.author.as_deref() != author
                            || revision.editor_group.is_none()
                        {
                            return false;
                        }
                        let local_start = start.saturating_sub(cursor);
                        let local_end = end.min(next).saturating_sub(cursor);
                        let [InlineNode::Run(run)] = revision.inlines.as_slice() else {
                            return false;
                        };
                        let local_start_usize = local_start as usize;
                        let local_end_usize = local_end as usize;
                        if local_start >= local_end
                            || local_end_usize > run.text.len()
                            || !run.text.is_char_boundary(local_start_usize)
                            || !run.text.is_char_boundary(local_end_usize)
                        {
                            return false;
                        }
                        edits.push((index, local_start_usize, local_end_usize));
                    }
                    cursor = next;
                }
                if edits.is_empty() || cursor < end {
                    return false;
                }
                for (index, local_start, local_end) in edits.into_iter().rev() {
                    let InlineNode::Revision(revision) = &mut paragraph.inlines[index] else {
                        return false;
                    };
                    let [InlineNode::Run(run)] = revision.inlines.as_mut_slice() else {
                        return false;
                    };
                    run.text.replace_range(local_start..local_end, "");
                    if run.text.is_empty() {
                        paragraph.inlines.remove(index);
                    }
                }
                coalesce_review_runs(&mut paragraph.inlines);
                return true;
            }
            BlockNode::Paragraph(_) => continue,
            BlockNode::Table(table) => {
                if table.rows.iter_mut().any(|row| {
                    row.cells.iter_mut().any(|cell| {
                        remove_authored_review_insertion(
                            notes,
                            &mut cell.blocks,
                            node,
                            start,
                            end,
                            author,
                        )
                    })
                }) {
                    return true;
                }
            }
            BlockNode::Sdt(sdt) => {
                if remove_authored_review_insertion(
                    notes,
                    &mut sdt.blocks,
                    node,
                    start,
                    end,
                    author,
                ) {
                    return true;
                }
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    false
}

/// docs/86 decision 2 (Word semantics): a delete spanning the author's own
/// pending insertions *and* accepted text removes the not-yet-accepted
/// insertions outright while the accepted text is suggested-for-deletion. This
/// does the first half — strip the current author's own `Insertion` bytes that
/// overlap `[start, end)` in place — and returns the total bytes removed, which
/// closes the gap so the remaining accepted text is contiguous and the caller
/// can `wrap_review_deletion(notes, [start, end - removed))`.
///
/// Returns `None` (defer to the plain deletion path) when the range holds no
/// own-author insertion, or holds anything this half can't safely handle: a
/// different author's revision, or an own insertion that is not a single run.
pub fn strip_authored_insertions_in_range(
    notes: &NoteAnchorLengths,
    blocks: &mut [BlockNode],
    node: NodeId,
    start: u32,
    end: u32,
    author: Option<&str>,
) -> Option<u32> {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) if paragraph.id == node => {
                let mut cursor = 0_u32;
                let mut edits: Vec<(usize, usize, usize)> = Vec::new();
                let mut saw_own = false;
                for (index, inline) in paragraph.inlines.iter().enumerate() {
                    let len = inline_anchor_len(notes, inline);
                    let next = cursor.saturating_add(len);
                    if len > 0 && next > start && cursor < end {
                        let overlap_start = start.max(cursor);
                        let overlap_end = end.min(next);
                        match inline {
                            InlineNode::Revision(revision)
                                if revision.kind == RevisionKind::Insertion =>
                            {
                                if revision.author.as_deref() != author
                                    || revision.editor_group.is_none()
                                {
                                    return None; // another author's insertion — defer.
                                }
                                let [InlineNode::Run(run)] = revision.inlines.as_slice() else {
                                    return None; // multi-run suggestion — defer.
                                };
                                let local_start = (overlap_start - cursor) as usize;
                                let local_end = (overlap_end - cursor) as usize;
                                if local_start >= local_end
                                    || local_end > run.text.len()
                                    || !run.text.is_char_boundary(local_start)
                                    || !run.text.is_char_boundary(local_end)
                                {
                                    return None;
                                }
                                edits.push((index, local_start, local_end));
                                saw_own = true;
                            }
                            InlineNode::Run(_) => {} // accepted text — left for the wrap step.
                            _ => return None,        // a deletion/other wrapper in range — defer.
                        }
                    }
                    cursor = next;
                }
                if !saw_own {
                    return None; // nothing of ours to strip — plain deletion path handles it.
                }
                let mut removed = 0_u32;
                for (index, local_start, local_end) in edits.into_iter().rev() {
                    let InlineNode::Revision(revision) = &mut paragraph.inlines[index] else {
                        return None;
                    };
                    let [InlineNode::Run(run)] = revision.inlines.as_mut_slice() else {
                        return None;
                    };
                    removed = removed.saturating_add((local_end - local_start) as u32);
                    run.text.replace_range(local_start..local_end, "");
                    if run.text.is_empty() {
                        paragraph.inlines.remove(index);
                    }
                }
                coalesce_review_runs(&mut paragraph.inlines);
                return Some(removed);
            }
            BlockNode::Paragraph(_) => continue,
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        if let Some(removed) = strip_authored_insertions_in_range(
                            &NoteAnchorLengths::default(),
                            &mut cell.blocks,
                            node,
                            start,
                            end,
                            author,
                        ) {
                            return Some(removed);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(removed) = strip_authored_insertions_in_range(
                    &NoteAnchorLengths::default(),
                    &mut sdt.blocks,
                    node,
                    start,
                    end,
                    author,
                ) {
                    return Some(removed);
                }
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
pub fn apply_review_format_change(
    notes: &NoteAnchorLengths,
    blocks: &mut [BlockNode],
    node: NodeId,
    start: u32,
    end: u32,
    delta: FormatDelta,
    author: Option<String>,
    date: Option<String>,
    group: RevisionGroup,
    revision_ids: &mut RevisionIdAllocator,
    ids: &mut IdGenerator,
) -> Result<bool, String> {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) if paragraph.id == node => {
                if !review_split_top_level_run(notes, &mut paragraph.inlines, start, ids)
                    || !review_split_top_level_run(notes, &mut paragraph.inlines, end, ids)
                {
                    return Ok(false);
                }
                let mut cursor = 0_u32;
                let mut selected = Vec::new();
                for (index, inline) in paragraph.inlines.iter().enumerate() {
                    let next = cursor.saturating_add(inline_anchor_len(notes, inline));
                    if cursor < end && next > start {
                        let InlineNode::Run(run) = inline else {
                            return Ok(false);
                        };
                        if cursor < start || next > end || run.properties.prop_change.is_some() {
                            return Ok(false);
                        }
                        selected.push(index);
                    }
                    cursor = next;
                }
                if selected.is_empty() || cursor < end {
                    return Ok(false);
                }

                let mut changes = Vec::with_capacity(selected.len());
                for index in selected {
                    let InlineNode::Run(run) = &paragraph.inlines[index] else {
                        return Ok(false);
                    };
                    let mut current = run.properties.clone();
                    current.prop_change = None;
                    let prior = current.clone();
                    apply_review_delta_to_properties(&mut current, &delta);
                    if current != prior {
                        changes.push((index, current, prior));
                    }
                }
                if changes.is_empty() {
                    return Ok(false);
                }

                for (index, mut current, prior) in changes {
                    current.prop_change = Some(Box::new(PropChange {
                        author: author.clone(),
                        date: date.clone(),
                        revision_id: Some(revision_ids.allocate()?),
                        editor_group: Some(group),
                        prior: Box::new(prior.get().clone()),
                    }));
                    let InlineNode::Run(run) = &mut paragraph.inlines[index] else {
                        return Ok(false);
                    };
                    run.properties = current;
                }
                return Ok(true);
            }
            BlockNode::Paragraph(_) => continue,
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        if apply_review_format_change(
                            notes,
                            &mut cell.blocks,
                            node,
                            start,
                            end,
                            delta.clone(),
                            author.clone(),
                            date.clone(),
                            group,
                            revision_ids,
                            ids,
                        )? {
                            return Ok(true);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if apply_review_format_change(
                    notes,
                    &mut sdt.blocks,
                    node,
                    start,
                    end,
                    delta.clone(),
                    author.clone(),
                    date.clone(),
                    group,
                    revision_ids,
                    ids,
                )? {
                    return Ok(true);
                }
            }
            BlockNode::AltChunk(_) => continue,
        }
    }
    Ok(false)
}

pub fn apply_review_delta_to_properties(properties: &mut RunProperties, delta: &FormatDelta) {
    delta.apply_to(properties);
}

pub fn apply_review_delta(inlines: &mut [InlineNode], delta: &FormatDelta) {
    for inline in inlines {
        match inline {
            InlineNode::Run(run) => {
                apply_review_delta_to_properties(&mut run.properties, delta);
            }
            InlineNode::Revision(revision) => apply_review_delta(&mut revision.inlines, delta),
            InlineNode::Hyperlink(link) => apply_review_delta(&mut link.inlines, delta),
            InlineNode::Sdt(sdt) => apply_review_delta(&mut sdt.inlines, delta),
            _ => {}
        }
    }
}

pub fn remove_review_comment_markers(blocks: &mut [BlockNode], comment: CommentId) {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                paragraph.inlines.retain(|inline| {
                    !matches!(inline,
                        InlineNode::CommentReference(reference) if reference.comment == comment
                    ) && !matches!(inline,
                        InlineNode::CommentRangeStart(marker) if marker.comment == comment
                    ) && !matches!(inline,
                        InlineNode::CommentRangeEnd(marker) if marker.comment == comment
                    )
                });
                coalesce_review_runs(&mut paragraph.inlines);
            }
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        remove_review_comment_markers(&mut cell.blocks, comment);
                    }
                }
            }
            BlockNode::Sdt(sdt) => remove_review_comment_markers(&mut sdt.blocks, comment),
            BlockNode::AltChunk(_) => {}
        }
    }
}

pub fn remove_review_move_markers(blocks: &mut [BlockNode], markers: &BTreeSet<NodeId>) {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                remove_review_inline_move_markers(&mut paragraph.inlines, markers);
            }
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        remove_review_move_markers(&mut cell.blocks, markers);
                    }
                }
            }
            BlockNode::Sdt(sdt) => remove_review_move_markers(&mut sdt.blocks, markers),
            BlockNode::AltChunk(_) => {}
        }
    }
}

pub fn remove_review_inline_move_markers(
    inlines: &mut Vec<InlineNode>,
    markers: &BTreeSet<NodeId>,
) {
    inlines.retain(|inline| {
        !matches!(inline, InlineNode::MoveRangeStart(marker) if markers.contains(&marker.id))
            && !matches!(inline, InlineNode::MoveRangeEnd(marker) if markers.contains(&marker.id))
    });
    for inline in inlines {
        match inline {
            InlineNode::Revision(revision) => {
                remove_review_inline_move_markers(&mut revision.inlines, markers);
            }
            InlineNode::Hyperlink(link) => {
                remove_review_inline_move_markers(&mut link.inlines, markers);
            }
            InlineNode::Sdt(sdt) => {
                remove_review_inline_move_markers(&mut sdt.inlines, markers);
            }
            _ => {}
        }
    }
}

pub fn decide_review_revision(blocks: &mut [BlockNode], id: NodeId, accept: bool) -> bool {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                if decide_review_inline(&mut paragraph.inlines, id, accept) {
                    return true;
                }
            }
            BlockNode::Table(table) => {
                if table.rows.iter_mut().any(|row| {
                    row.cells
                        .iter_mut()
                        .any(|cell| decide_review_revision(&mut cell.blocks, id, accept))
                }) {
                    return true;
                }
            }
            BlockNode::Sdt(sdt) => {
                if decide_review_revision(&mut sdt.blocks, id, accept) {
                    return true;
                }
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    false
}

pub fn decide_all_review_revisions(blocks: &mut [BlockNode], accept: bool) {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                decide_all_review_inlines(&mut paragraph.inlines, accept);
            }
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        decide_all_review_revisions(&mut cell.blocks, accept);
                    }
                }
            }
            BlockNode::Sdt(sdt) => decide_all_review_revisions(&mut sdt.blocks, accept),
            BlockNode::AltChunk(_) => {}
        }
    }
}

pub fn decide_all_review_inlines(inlines: &mut Vec<InlineNode>, accept: bool) {
    let mut index = 0;
    while index < inlines.len() {
        match &mut inlines[index] {
            InlineNode::Run(run) => {
                if let Some(change) = run.properties.prop_change.take()
                    && !accept
                {
                    let mut prior = *change.prior;
                    prior.prop_change = None;
                    run.properties = prior.into();
                }
                index += 1;
            }
            InlineNode::Revision(revision) => {
                decide_all_review_inlines(&mut revision.inlines, accept);
                let keep = matches!(
                    revision.kind,
                    RevisionKind::Insertion | RevisionKind::MoveTo
                ) == accept;
                let replacement = if keep {
                    revision.inlines.clone()
                } else {
                    Vec::new()
                };
                let replacement_len = replacement.len();
                inlines.splice(index..=index, replacement);
                index = index.saturating_add(replacement_len);
            }
            InlineNode::Hyperlink(link) => {
                decide_all_review_inlines(&mut link.inlines, accept);
                index += 1;
            }
            InlineNode::Sdt(sdt) => {
                decide_all_review_inlines(&mut sdt.inlines, accept);
                index += 1;
            }
            _ => index += 1,
        }
    }
    coalesce_review_runs(inlines);
}

pub fn find_review_revision_anchor(
    notes: &NoteAnchorLengths,
    blocks: &[BlockNode],
    id: NodeId,
) -> Option<(NodeId, u32, u32, RevisionKind)> {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                let mut offset = 0;
                if let Some(anchor) = find_review_inline_anchor(
                    notes,
                    &paragraph.inlines,
                    paragraph.id,
                    id,
                    &mut offset,
                    true,
                ) {
                    return Some(anchor);
                }
            }
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        if let Some(anchor) = find_review_revision_anchor(notes, &cell.blocks, id) {
                            return Some(anchor);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(anchor) = find_review_revision_anchor(notes, &sdt.blocks, id) {
                    return Some(anchor);
                }
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    None
}

pub fn find_review_inline_anchor(
    notes: &NoteAnchorLengths,
    inlines: &[InlineNode],
    node: NodeId,
    id: NodeId,
    offset: &mut u32,
    projected: bool,
) -> Option<(NodeId, u32, u32, RevisionKind)> {
    for inline in inlines {
        match inline {
            InlineNode::Revision(revision) => {
                let start = *offset;
                let visible = projected
                    && revision
                        .kind
                        .contributes_to(ReviewProjection::FinalWithMarkup);
                let end = if visible {
                    start.saturating_add(projected_revision_len(notes, revision))
                } else {
                    start
                };
                if revision.id == id {
                    return Some((node, start, end, revision.kind));
                }
                if visible {
                    if let Some(anchor) =
                        find_review_inline_anchor(notes, &revision.inlines, node, id, offset, true)
                    {
                        return Some(anchor);
                    }
                } else {
                    let mut hidden_offset = start;
                    if let Some(anchor) = find_review_inline_anchor(
                        notes,
                        &revision.inlines,
                        node,
                        id,
                        &mut hidden_offset,
                        false,
                    ) {
                        return Some(anchor);
                    }
                }
                *offset = end;
            }
            InlineNode::Hyperlink(link) => {
                if let Some(anchor) =
                    find_review_inline_anchor(notes, &link.inlines, node, id, offset, projected)
                {
                    return Some(anchor);
                }
            }
            InlineNode::Sdt(sdt) => {
                if let Some(anchor) =
                    find_review_inline_anchor(notes, &sdt.inlines, node, id, offset, projected)
                {
                    return Some(anchor);
                }
            }
            _ if projected => {
                *offset = offset.saturating_add(inline_anchor_len(notes, inline));
            }
            _ => {}
        }
    }
    None
}

pub fn collect_review_revision_ids(blocks: &[BlockNode], out: &mut Vec<NodeId>) {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => collect_review_inline_ids(&paragraph.inlines, out),
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        collect_review_revision_ids(&cell.blocks, out);
                    }
                }
            }
            BlockNode::Sdt(sdt) => collect_review_revision_ids(&sdt.blocks, out),
            BlockNode::AltChunk(_) => {}
        }
    }
}

pub fn collect_review_revision_serial_ids(blocks: &[BlockNode], out: &mut Vec<String>) {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                // Paragraph-level revisions carry a `w:id` too. Seeding from inline
                // revisions alone let a new suggestion reuse an id an imported
                // `w:pPrChange` or paragraph-mark revision already held (docs/108).
                let properties = &paragraph.properties;
                if let Some(id) = properties
                    .mark_revision
                    .as_ref()
                    .and_then(|mark| mark.revision_id.as_ref())
                {
                    out.push(id.clone());
                }
                if let Some(id) = properties
                    .prop_change
                    .as_ref()
                    .and_then(|change| change.revision_id.as_ref())
                {
                    out.push(id.clone());
                }
                collect_review_inline_serial_ids(&paragraph.inlines, out);
            }
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        collect_review_revision_serial_ids(&cell.blocks, out);
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                collect_review_revision_serial_ids(&sdt.blocks, out);
            }
            BlockNode::AltChunk(_) => {}
        }
    }
}

pub fn collect_review_editor_group_ids(blocks: &[BlockNode], out: &mut BTreeSet<NodeId>) {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                collect_review_inline_editor_group_ids(&paragraph.inlines, out);
            }
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        collect_review_editor_group_ids(&cell.blocks, out);
                    }
                }
            }
            BlockNode::Sdt(sdt) => collect_review_editor_group_ids(&sdt.blocks, out),
            BlockNode::AltChunk(_) => {}
        }
    }
}

pub fn collect_review_inline_editor_group_ids(inlines: &[InlineNode], out: &mut BTreeSet<NodeId>) {
    for inline in inlines {
        match inline {
            InlineNode::Run(run) => {
                if let Some(group) = run
                    .properties
                    .prop_change
                    .as_ref()
                    .and_then(|change| change.editor_group)
                {
                    out.insert(group.id);
                }
            }
            InlineNode::Revision(revision) => {
                if let Some(group) = revision.editor_group {
                    out.insert(group.id);
                }
                collect_review_inline_editor_group_ids(&revision.inlines, out);
            }
            InlineNode::Hyperlink(link) => {
                collect_review_inline_editor_group_ids(&link.inlines, out);
            }
            InlineNode::Sdt(sdt) => {
                collect_review_inline_editor_group_ids(&sdt.inlines, out);
            }
            _ => {}
        }
    }
}

pub fn collect_review_inline_serial_ids(inlines: &[InlineNode], out: &mut Vec<String>) {
    for inline in inlines {
        match inline {
            InlineNode::Run(run) => {
                if let Some(id) = run
                    .properties
                    .prop_change
                    .as_ref()
                    .and_then(|change| change.revision_id.as_ref())
                {
                    out.push(id.clone());
                }
            }
            InlineNode::Revision(revision) => {
                if let Some(id) = &revision.revision_id {
                    out.push(id.clone());
                }
                collect_review_inline_serial_ids(&revision.inlines, out);
            }
            InlineNode::Hyperlink(link) => {
                collect_review_inline_serial_ids(&link.inlines, out);
            }
            InlineNode::Sdt(sdt) => {
                collect_review_inline_serial_ids(&sdt.inlines, out);
            }
            _ => {}
        }
    }
}

pub fn find_review_revision(blocks: &[BlockNode], id: NodeId) -> Option<&Revision> {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                if let Some(revision) = find_review_inline_revision(&paragraph.inlines, id) {
                    return Some(revision);
                }
            }
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        if let Some(revision) = find_review_revision(&cell.blocks, id) {
                            return Some(revision);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(revision) = find_review_revision(&sdt.blocks, id) {
                    return Some(revision);
                }
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    None
}

pub fn find_review_inline_revision(inlines: &[InlineNode], id: NodeId) -> Option<&Revision> {
    for inline in inlines {
        match inline {
            InlineNode::Revision(revision) => {
                if revision.id == id {
                    return Some(revision);
                }
                if let Some(found) = find_review_inline_revision(&revision.inlines, id) {
                    return Some(found);
                }
            }
            InlineNode::Hyperlink(link) => {
                if let Some(found) = find_review_inline_revision(&link.inlines, id) {
                    return Some(found);
                }
            }
            InlineNode::Sdt(sdt) => {
                if let Some(found) = find_review_inline_revision(&sdt.inlines, id) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

pub fn find_review_format_change(
    blocks: &[BlockNode],
    run_id: NodeId,
) -> Option<&PropChange<RunProperties>> {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                if let Some(change) = find_review_inline_format_change(&paragraph.inlines, run_id) {
                    return Some(change);
                }
            }
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        if let Some(change) = find_review_format_change(&cell.blocks, run_id) {
                            return Some(change);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(change) = find_review_format_change(&sdt.blocks, run_id) {
                    return Some(change);
                }
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    None
}

pub fn find_review_inline_format_change(
    inlines: &[InlineNode],
    run_id: NodeId,
) -> Option<&PropChange<RunProperties>> {
    for inline in inlines {
        match inline {
            InlineNode::Run(run) if run.id == run_id => {
                if let Some(change) = &run.properties.prop_change {
                    return Some(change);
                }
            }
            InlineNode::Revision(revision) => {
                if let Some(change) = find_review_inline_format_change(&revision.inlines, run_id) {
                    return Some(change);
                }
            }
            InlineNode::Hyperlink(link) => {
                if let Some(change) = find_review_inline_format_change(&link.inlines, run_id) {
                    return Some(change);
                }
            }
            InlineNode::Sdt(sdt) => {
                if let Some(change) = find_review_inline_format_change(&sdt.inlines, run_id) {
                    return Some(change);
                }
            }
            _ => {}
        }
    }
    None
}

pub fn find_review_format_anchor(
    notes: &NoteAnchorLengths,
    blocks: &[BlockNode],
    run_id: NodeId,
) -> Option<(NodeId, u32, u32)> {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                let mut offset = 0;
                if let Some(anchor) = find_review_inline_format_anchor(
                    notes,
                    &paragraph.inlines,
                    paragraph.id,
                    run_id,
                    &mut offset,
                    true,
                ) {
                    return Some(anchor);
                }
            }
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        if let Some(anchor) = find_review_format_anchor(notes, &cell.blocks, run_id)
                        {
                            return Some(anchor);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(anchor) = find_review_format_anchor(notes, &sdt.blocks, run_id) {
                    return Some(anchor);
                }
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    None
}

pub fn find_review_inline_format_anchor(
    notes: &NoteAnchorLengths,
    inlines: &[InlineNode],
    node: NodeId,
    run_id: NodeId,
    offset: &mut u32,
    projected: bool,
) -> Option<(NodeId, u32, u32)> {
    for inline in inlines {
        match inline {
            InlineNode::Run(run) => {
                let start = *offset;
                let end = if projected {
                    start.saturating_add(run.text.len() as u32)
                } else {
                    start
                };
                if run.id == run_id && run.properties.prop_change.is_some() {
                    return Some((node, start, end));
                }
                *offset = end;
            }
            InlineNode::Revision(revision) => {
                let visible = projected
                    && revision
                        .kind
                        .contributes_to(ReviewProjection::FinalWithMarkup);
                if visible {
                    if let Some(anchor) = find_review_inline_format_anchor(
                        notes,
                        &revision.inlines,
                        node,
                        run_id,
                        offset,
                        true,
                    ) {
                        return Some(anchor);
                    }
                } else {
                    let mut hidden_offset = *offset;
                    if let Some(anchor) = find_review_inline_format_anchor(
                        notes,
                        &revision.inlines,
                        node,
                        run_id,
                        &mut hidden_offset,
                        false,
                    ) {
                        return Some(anchor);
                    }
                }
            }
            InlineNode::Hyperlink(link) => {
                if let Some(anchor) = find_review_inline_format_anchor(
                    notes,
                    &link.inlines,
                    node,
                    run_id,
                    offset,
                    projected,
                ) {
                    return Some(anchor);
                }
            }
            InlineNode::Sdt(sdt) => {
                if let Some(anchor) = find_review_inline_format_anchor(
                    notes,
                    &sdt.inlines,
                    node,
                    run_id,
                    offset,
                    projected,
                ) {
                    return Some(anchor);
                }
            }
            _ if projected => {
                *offset = offset.saturating_add(inline_anchor_len(notes, inline));
            }
            _ => {}
        }
    }
    None
}

pub fn decide_review_format_change(blocks: &mut [BlockNode], run_id: NodeId, accept: bool) -> bool {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                if decide_review_inline_format_change(&mut paragraph.inlines, run_id, accept) {
                    coalesce_review_runs(&mut paragraph.inlines);
                    return true;
                }
            }
            BlockNode::Table(table) => {
                if table.rows.iter_mut().any(|row| {
                    row.cells
                        .iter_mut()
                        .any(|cell| decide_review_format_change(&mut cell.blocks, run_id, accept))
                }) {
                    return true;
                }
            }
            BlockNode::Sdt(sdt) => {
                if decide_review_format_change(&mut sdt.blocks, run_id, accept) {
                    return true;
                }
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    false
}

pub fn decide_review_inline_format_change(
    inlines: &mut [InlineNode],
    run_id: NodeId,
    accept: bool,
) -> bool {
    for inline in inlines {
        match inline {
            InlineNode::Run(run) if run.id == run_id => {
                let Some(change) = run.properties.prop_change.take() else {
                    return false;
                };
                if !accept {
                    let mut prior = *change.prior;
                    prior.prop_change = None;
                    run.properties = prior.into();
                }
                return true;
            }
            InlineNode::Revision(revision) => {
                if decide_review_inline_format_change(&mut revision.inlines, run_id, accept) {
                    return true;
                }
            }
            InlineNode::Hyperlink(link) => {
                if decide_review_inline_format_change(&mut link.inlines, run_id, accept) {
                    return true;
                }
            }
            InlineNode::Sdt(sdt) => {
                if decide_review_inline_format_change(&mut sdt.inlines, run_id, accept) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

pub fn collect_review_format_ids(blocks: &[BlockNode], out: &mut Vec<NodeId>) {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                collect_review_inline_format_ids(&paragraph.inlines, out);
            }
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        collect_review_format_ids(&cell.blocks, out);
                    }
                }
            }
            BlockNode::Sdt(sdt) => collect_review_format_ids(&sdt.blocks, out),
            BlockNode::AltChunk(_) => {}
        }
    }
}

pub fn collect_review_inline_format_ids(inlines: &[InlineNode], out: &mut Vec<NodeId>) {
    for inline in inlines {
        match inline {
            InlineNode::Run(run) if run.properties.prop_change.is_some() => out.push(run.id),
            InlineNode::Revision(revision) => {
                collect_review_inline_format_ids(&revision.inlines, out);
            }
            InlineNode::Hyperlink(link) => collect_review_inline_format_ids(&link.inlines, out),
            InlineNode::Sdt(sdt) => collect_review_inline_format_ids(&sdt.inlines, out),
            _ => {}
        }
    }
}

#[derive(Clone, Debug)]
pub struct ReviewGroupMember {
    pub target: ReviewDecisionTarget,
    pub group: RevisionGroup,
    pub node: NodeId,
    pub start: u32,
    pub end: u32,
    pub kind: ReviewMemberKind,
    pub author: Option<String>,
    pub date: Option<String>,
    pub revision_id: Option<String>,
    pub text: String,
    pub top_level_index: Option<usize>,
    pub prior_is_nested: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewMemberKind {
    Revision(RevisionKind),
    Formatting,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewDecisionTarget {
    Revision(NodeId),
    RunFormatting(NodeId),
}

#[derive(Clone, Debug)]
pub struct ValidatedReviewGroup {
    pub nodes: Vec<NodeId>,
    pub node: NodeId,
    pub start: u32,
    pub targets: Vec<ReviewDecisionTarget>,
}

pub fn validate_review_group(
    notes: &NoteAnchorLengths,
    blocks: &[BlockNode],
    group_id: NodeId,
) -> Result<ValidatedReviewGroup, String> {
    let mut members = Vec::new();
    collect_review_group_members(notes, blocks, group_id, &mut members);
    if members.is_empty() {
        return Err("revision group not found".to_owned());
    }
    let group = members[0].group;
    let across_paragraphs = group.kind == RevisionGroupKind::Formatting
        && members
            .iter()
            .all(|member| member.kind == ReviewMemberKind::Formatting);
    if members.iter().any(|member| {
        member.group != group
            || (!across_paragraphs && member.node != members[0].node)
            || member.top_level_index.is_none()
            || member.end < member.start
            || (member.end == member.start
                && !matches!(
                    member.kind,
                    ReviewMemberKind::Revision(RevisionKind::Deletion | RevisionKind::MoveFrom)
                ))
            || member.author.as_deref().is_none_or(str::is_empty)
            || member.author != members[0].author
            || member.date != members[0].date
            || member.prior_is_nested
            || !member
                .revision_id
                .as_deref()
                .is_some_and(|id| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()))
    }) {
        return Err("revision group metadata or placement is inconsistent".to_owned());
    }
    if !across_paragraphs {
        members.sort_by_key(|member| member.top_level_index);
    }
    if members.windows(2).any(|pair| {
        pair[0].node == pair[1].node
            && pair[1].top_level_index.expect("checked top-level")
                != pair[0].top_level_index.expect("checked top-level") + 1
    }) {
        return Err("revision group members are not contiguous".to_owned());
    }

    let insertions = members
        .iter()
        .filter(|member| member.kind == ReviewMemberKind::Revision(RevisionKind::Insertion))
        .count();
    let deletions = members
        .iter()
        .filter(|member| member.kind == ReviewMemberKind::Revision(RevisionKind::Deletion))
        .count();
    let formatting = members
        .iter()
        .filter(|member| member.kind == ReviewMemberKind::Formatting)
        .count();
    match group.kind {
        RevisionGroupKind::Typing if members.len() == 1 && insertions == 1 && formatting == 0 => {}
        RevisionGroupKind::Replacement
            if members.len() == 2 && insertions == 1 && deletions == 1 && formatting == 0 => {}
        RevisionGroupKind::Formatting if formatting == members.len() && !members.is_empty() => {}
        // Accept an in-memory pre-doc-83 formatting pair. New authoring uses
        // `RunProperties.prop_change` and never creates this representation.
        RevisionGroupKind::Formatting
            if formatting == 0
                && members.len() == 2
                && insertions == 1
                && deletions == 1
                && members
                    .iter()
                    .find(|member| {
                        member.kind == ReviewMemberKind::Revision(RevisionKind::Insertion)
                    })
                    .map(|member| &member.text)
                    == members
                        .iter()
                        .find(|member| {
                            member.kind == ReviewMemberKind::Revision(RevisionKind::Deletion)
                        })
                        .map(|member| &member.text) => {}
        _ => return Err("revision group has an invalid member composition".to_owned()),
    }

    let mut nodes = Vec::new();
    for member in &members {
        if !nodes.contains(&member.node) {
            nodes.push(member.node);
        }
    }
    Ok(ValidatedReviewGroup {
        nodes,
        node: members[0].node,
        start: members.iter().map(|member| member.start).min().unwrap_or(0),
        targets: members.into_iter().map(|member| member.target).collect(),
    })
}

pub fn collect_review_group_members(
    notes: &NoteAnchorLengths,
    blocks: &[BlockNode],
    group_id: NodeId,
    out: &mut Vec<ReviewGroupMember>,
) {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                let mut offset = 0_u32;
                for (index, inline) in paragraph.inlines.iter().enumerate() {
                    collect_review_group_inline(
                        notes,
                        inline,
                        paragraph.id,
                        &mut offset,
                        true,
                        group_id,
                        Some(index),
                        out,
                    );
                }
            }
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        collect_review_group_members(notes, &cell.blocks, group_id, out);
                    }
                }
            }
            BlockNode::Sdt(sdt) => collect_review_group_members(notes, &sdt.blocks, group_id, out),
            BlockNode::AltChunk(_) => {}
        }
    }
}

// Eight arguments because `NoteAnchorLengths` is PASSED rather than looked up:
// that is the whole point of the index (build once, query O(1)), and the
// alternative to the parameter is the linear scan per reference it replaced.
#[allow(clippy::too_many_arguments)]
pub fn collect_review_group_inline(
    notes: &NoteAnchorLengths,
    inline: &InlineNode,
    node: NodeId,
    offset: &mut u32,
    projected: bool,
    group_id: NodeId,
    top_level_index: Option<usize>,
    out: &mut Vec<ReviewGroupMember>,
) {
    match inline {
        InlineNode::Revision(revision) => {
            let start = *offset;
            let visible = projected
                && revision
                    .kind
                    .contributes_to(ReviewProjection::FinalWithMarkup);
            let end = if visible {
                start.saturating_add(projected_revision_len(notes, revision))
            } else {
                start
            };
            if let Some(group) = revision.editor_group
                && group.id == group_id
            {
                out.push(ReviewGroupMember {
                    target: ReviewDecisionTarget::Revision(revision.id),
                    group,
                    node,
                    start,
                    end,
                    kind: ReviewMemberKind::Revision(revision.kind),
                    author: revision.author.clone(),
                    date: revision.date.clone(),
                    revision_id: revision.revision_id.clone(),
                    text: node_plain_text(&revision.inlines),
                    top_level_index,
                    prior_is_nested: false,
                });
            }
            if visible {
                for child in &revision.inlines {
                    collect_review_group_inline(
                        notes, child, node, offset, true, group_id, None, out,
                    );
                }
            } else {
                let mut hidden_offset = start;
                for child in &revision.inlines {
                    collect_review_group_inline(
                        notes,
                        child,
                        node,
                        &mut hidden_offset,
                        false,
                        group_id,
                        None,
                        out,
                    );
                }
            }
            *offset = end;
        }
        InlineNode::Run(run) => {
            let start = *offset;
            let end = if projected {
                start.saturating_add(run.text.len() as u32)
            } else {
                start
            };
            if let Some(change) = &run.properties.prop_change
                && let Some(group) = change.editor_group
                && group.id == group_id
            {
                out.push(ReviewGroupMember {
                    target: ReviewDecisionTarget::RunFormatting(run.id),
                    group,
                    node,
                    start,
                    end,
                    kind: ReviewMemberKind::Formatting,
                    author: change.author.clone(),
                    date: change.date.clone(),
                    revision_id: change.revision_id.clone(),
                    text: run.text.clone(),
                    top_level_index,
                    prior_is_nested: change.prior.prop_change.is_some(),
                });
            }
            *offset = end;
        }
        InlineNode::Hyperlink(link) => {
            for child in &link.inlines {
                collect_review_group_inline(
                    notes, child, node, offset, projected, group_id, None, out,
                );
            }
        }
        InlineNode::Sdt(sdt) => {
            for child in &sdt.inlines {
                collect_review_group_inline(
                    notes, child, node, offset, projected, group_id, None, out,
                );
            }
        }
        _ if projected => {
            *offset = offset.saturating_add(inline_anchor_len(notes, inline));
        }
        _ => {}
    }
}

pub fn collect_review_inline_ids(inlines: &[InlineNode], out: &mut Vec<NodeId>) {
    for inline in inlines {
        match inline {
            InlineNode::Revision(revision) => {
                out.push(revision.id);
                collect_review_inline_ids(&revision.inlines, out);
            }
            InlineNode::Hyperlink(link) => collect_review_inline_ids(&link.inlines, out),
            InlineNode::Sdt(sdt) => collect_review_inline_ids(&sdt.inlines, out),
            _ => {}
        }
    }
}

pub fn decide_review_inline(inlines: &mut Vec<InlineNode>, id: NodeId, accept: bool) -> bool {
    for index in 0..inlines.len() {
        if let InlineNode::Revision(revision) = &inlines[index]
            && revision.id == id
        {
            let keep = matches!(
                revision.kind,
                RevisionKind::Insertion | RevisionKind::MoveTo
            ) == accept;
            let replacement = if keep {
                revision.inlines.clone()
            } else {
                Vec::new()
            };
            inlines.splice(index..=index, replacement);
            coalesce_review_runs(inlines);
            return true;
        }
        let nested = match &mut inlines[index] {
            InlineNode::Hyperlink(link) => decide_review_inline(&mut link.inlines, id, accept),
            InlineNode::Revision(revision) => {
                decide_review_inline(&mut revision.inlines, id, accept)
            }
            InlineNode::Sdt(sdt) => decide_review_inline(&mut sdt.inlines, id, accept),
            _ => false,
        };
        if nested {
            return true;
        }
    }
    false
}

pub fn coalesce_review_runs(inlines: &mut Vec<InlineNode>) {
    let mut index = 0;
    while index + 1 < inlines.len() {
        let mergeable = matches!(
            (&inlines[index], &inlines[index + 1]),
            (InlineNode::Run(left), InlineNode::Run(right))
                if left.properties == right.properties
        );
        if mergeable {
            let InlineNode::Run(next) = inlines.remove(index + 1) else {
                unreachable!("the adjacent inline was matched as a run");
            };
            if let InlineNode::Run(current) = &mut inlines[index] {
                current.text.push_str(&next.text);
            }
        } else {
            index += 1;
        }
    }
}

/// Splits the [`Revision`] wrapper at `index` into two adjacent wrappers at
/// `local` (a byte offset within the wrapper), so a boundary exists where another
/// author's edit can be placed.
///
/// This is how `docs/86` says a foreign suggestion must be treated: "left intact
/// and… marked with our own deletion at the boundary; we do not silently discard
/// another author's suggestion." Splitting divides the RANGE without touching a
/// single character of its content — the other author's text, authorship and date
/// survive verbatim in both halves.
///
/// The halves are UNGROUPED. `editor_group` marks one authored action, and
/// `validate_review_group` requires a group's members to sit at consecutive
/// top-level indices — which is exactly what a split breaks, because the point of
/// splitting is to put another author's content between the halves. Keeping the
/// group produced a document where "Reject all" failed validation and silently
/// decided nothing, which is a far worse outcome than two cards: the halves are
/// two `w:ins` elements in OOXML terms, and that is now what they are here.
///
/// Authorship, date and `revision_id` are carried across unchanged, so both
/// halves still belong to their original author. Only `id` is refreshed, because
/// two inline nodes cannot share an identity.
pub fn split_revision_wrapper(
    notes: &NoteAnchorLengths,
    inlines: &mut Vec<InlineNode>,
    index: usize,
    local: u32,
    ids: &mut IdGenerator,
) -> bool {
    // A boundary has to exist inside the wrapper before its content can be
    // partitioned; this recurses, so a revision nested in a revision also splits.
    {
        let InlineNode::Revision(revision) = &mut inlines[index] else {
            return false;
        };
        if !review_split_top_level_run(notes, &mut revision.inlines, local, ids) {
            return false;
        }
    }
    let (cut, len) = {
        let InlineNode::Revision(revision) = &inlines[index] else {
            return false;
        };
        let mut cursor: u32 = 0;
        let mut cut = None;
        for i in 0..=revision.inlines.len() {
            if cursor == local {
                cut = Some(i);
                break;
            }
            if let Some(inline) = revision.inlines.get(i) {
                cursor = cursor.saturating_add(inline_anchor_len(notes, inline));
            }
        }
        match cut {
            Some(cut) => (cut, revision.inlines.len()),
            None => return false,
        }
    };
    // Already at an edge: the caller's insertion point is outside this wrapper and
    // no split is needed. Splitting anyway would leave an empty wrapper, which the
    // model rejects.
    if cut == 0 || cut == len {
        return true;
    }
    let Ok(new_id) = ids.next_id() else {
        return false;
    };
    let right = {
        let InlineNode::Revision(revision) = &mut inlines[index] else {
            return false;
        };
        let tail = revision.inlines.split_off(cut);
        let mut right = revision.clone();
        right.id = new_id;
        right.inlines = tail;
        right.editor_group = None;
        revision.editor_group = None;
        right
    };
    inlines.insert(index + 1, InlineNode::Revision(right));
    true
}

pub fn review_split_top_level_run(
    notes: &NoteAnchorLengths,
    inlines: &mut Vec<InlineNode>,
    offset: u32,
    ids: &mut IdGenerator,
) -> bool {
    let mut cursor: u32 = 0;
    for index in 0..inlines.len() {
        let next = cursor.saturating_add(inline_anchor_len(notes, &inlines[index]));
        if offset == cursor || offset == next {
            return true;
        }
        if offset > cursor
            && offset < next
            && let InlineNode::Run(run) = &inlines[index]
        {
            let split = (offset - cursor) as usize;
            if !run.text.is_char_boundary(split) {
                return false;
            }
            let mut right = run.clone();
            right.id = match ids.next_id() {
                Ok(id) => id,
                Err(_) => return false,
            };
            right.text = run.text[split..].to_owned();
            if right.text.is_empty() {
                return false;
            }
            inlines[index] = InlineNode::Run(Run {
                text: run.text[..split].to_owned(),
                ..run.clone()
            });
            inlines.insert(index + 1, InlineNode::Run(right));
            return true;
        }
        if offset > cursor && offset < next {
            // The offset lands inside a tracked range — another author's pending
            // insertion, typically. Split THAT wrapper rather than refusing: a
            // bare `return false` here is why a second reviewer's keystrokes were
            // dropped with nothing but a status line to show for it.
            if matches!(inlines[index], InlineNode::Revision(_)) {
                return split_revision_wrapper(notes, inlines, index, offset - cursor, ids);
            }
            return false;
        }
        cursor = next;
    }
    offset == cursor
}

// Eight arguments because `NoteAnchorLengths` is PASSED rather than looked up:
// that is the whole point of the index (build once, query O(1)), and the
// alternative to the parameter is the linear scan per reference it replaced.
#[allow(clippy::too_many_arguments)]
pub fn collect_review_comment_inline(
    notes: &NoteAnchorLengths,
    inline: &InlineNode,
    node: NodeId,
    para_len: u32,
    offset: &mut u32,
    projected: bool,
    starts: &mut std::collections::BTreeMap<casual_doc_model::v1::CommentId, (NodeId, u32, u32)>,
    out: &mut Vec<(casual_doc_model::v1::CommentId, NodeId, u32, u32)>,
) {
    match inline {
        InlineNode::CommentRangeStart(marker) => {
            starts.insert(marker.comment, (node, *offset, para_len));
        }
        InlineNode::CommentRangeEnd(marker) => {
            if let Some((start_node, start, start_para_len)) = starts.remove(&marker.comment) {
                if start_node == node {
                    // Same paragraph: the anchor is the exact commented span.
                    out.push((marker.comment, node, start, *offset));
                } else {
                    // Cross-paragraph: anchor to the start marker over the
                    // remainder of its own paragraph. The sidebar anchors to the
                    // comment's start and the highlight stays valid (a single-node
                    // range the overlay can rect) rather than breaking.
                    out.push((marker.comment, start_node, start, start_para_len));
                }
            }
        }
        InlineNode::Hyperlink(link) => {
            for child in &link.inlines {
                collect_review_comment_inline(
                    notes, child, node, para_len, offset, projected, starts, out,
                );
            }
        }
        InlineNode::Revision(revision) => {
            let child_projected = projected
                && revision
                    .kind
                    .contributes_to(ReviewProjection::FinalWithMarkup);
            for child in &revision.inlines {
                collect_review_comment_inline(
                    notes,
                    child,
                    node,
                    para_len,
                    offset,
                    child_projected,
                    starts,
                    out,
                );
            }
        }
        InlineNode::Sdt(sdt) => {
            for child in &sdt.inlines {
                collect_review_comment_inline(
                    notes, child, node, para_len, offset, projected, starts, out,
                );
            }
        }
        _ if projected => {
            *offset = offset.saturating_add(inline_anchor_len(notes, inline));
        }
        _ => {}
    }
}

/// One run of paragraph content, as a length and whether it exists in the EDITING
/// byte space. Deleted (and move-from) text is shown by the markup view and is
/// zero-width to the editor, so the two spaces differ by exactly the sum of these
/// non-contributing lengths.
#[derive(Debug)]
pub struct ReviewSegment {
    pub len: u32,
    pub in_editing_space: bool,
}

/// Flattens a paragraph's inlines into [`ReviewSegment`]s in document order.
///
/// The recursion mirrors the layout's own rule (`flow.rs`: a run contributes when
/// its projection says so OR the view is Markup), which is what makes the two
/// spaces relatable at all. Wrappers that merely group content — hyperlinks,
/// fields, content controls — are transparent to both spaces and are descended
/// through.
pub fn review_segments(
    notes: &NoteAnchorLengths,
    inlines: &[InlineNode],
    deleted: bool,
    out: &mut Vec<ReviewSegment>,
) {
    for inline in inlines {
        match inline {
            InlineNode::Revision(revision) => {
                let contributes = revision
                    .kind
                    .contributes_to(ReviewProjection::FinalWithMarkup);
                review_segments(notes, &revision.inlines, !contributes, out);
            }
            InlineNode::Hyperlink(link) => review_segments(notes, &link.inlines, deleted, out),
            InlineNode::Field(field) => review_segments(notes, &field.inlines, deleted, out),
            InlineNode::Sdt(sdt) => review_segments(notes, &sdt.inlines, deleted, out),
            other => {
                let len = inline_anchor_len(notes, other);
                if len > 0 {
                    out.push(ReviewSegment {
                        len,
                        in_editing_space: !deleted,
                    });
                }
            }
        }
    }
}

/// Editing-space offset -> markup-space offset within one paragraph.
///
/// A caret at an editing offset sits AFTER any struck text that precedes it,
/// which is where Word and Docs put it: the deletion has already happened, so the
/// insertion point belongs on its far side.
pub fn editing_to_markup_offset(segments: &[ReviewSegment], editing_offset: u32) -> u32 {
    let mut edit_pos = 0u32;
    let mut markup_pos = 0u32;
    for segment in segments {
        if segment.in_editing_space {
            if edit_pos + segment.len >= editing_offset {
                return markup_pos + (editing_offset - edit_pos);
            }
            edit_pos += segment.len;
        }
        markup_pos += segment.len;
    }
    markup_pos
}

/// Markup-space offset -> editing-space offset within one paragraph.
///
/// A position INSIDE struck text has no editing-space equivalent — the text is
/// not there to put a caret in — so it collapses to the deletion's leading edge.
/// Clamping rather than guessing is what stops a click on struck text from
/// resolving to a character somewhere else entirely.
pub fn markup_to_editing_offset(segments: &[ReviewSegment], markup_offset: u32) -> u32 {
    let mut edit_pos = 0u32;
    let mut markup_pos = 0u32;
    for segment in segments {
        if markup_pos + segment.len >= markup_offset {
            return if segment.in_editing_space {
                edit_pos + (markup_offset - markup_pos)
            } else {
                edit_pos
            };
        }
        markup_pos += segment.len;
        if segment.in_editing_space {
            edit_pos += segment.len;
        }
    }
    edit_pos
}

/// The anchor-stream bytes a revision wrapper contributes under the default
/// (final-with-markup) projection: its children's, or none when the revision is
/// hidden. **O(the revision's inlines)**; the single [`inline_anchor_len`]
/// answers for each child, so this cannot drift from the rest of the engine.
pub fn projected_revision_len(notes: &NoteAnchorLengths, revision: &Revision) -> u32 {
    if revision
        .kind
        .contributes_to(ReviewProjection::FinalWithMarkup)
    {
        inlines_anchor_len(notes, &revision.inlines)
    } else {
        0
    }
}

pub fn collect_review_inline(
    notes: &NoteAnchorLengths,
    inlines: &[InlineNode],
    node: NodeId,
    offset: &mut u32,
    projected: bool,
    move_links: &BTreeMap<NodeId, ReviewMoveLink>,
    out: &mut Vec<serde_json::Value>,
) {
    for inline in inlines {
        match inline {
            InlineNode::Run(run) if run.properties.prop_change.is_some() => {
                let start = *offset;
                let end = if projected {
                    start.saturating_add(run.text.len() as u32)
                } else {
                    start
                };
                let change = run
                    .properties
                    .prop_change
                    .as_ref()
                    .expect("matched property change");
                out.push(serde_json::json!({
                    "id": run.id.to_string(),
                    "kind": "formatting",
                    "author": change.author,
                    "date": change.date,
                    "groupId": change.editor_group.map(|group| group.id.to_string()),
                    "groupKind": change.editor_group.map(|_| "formatting"),
                    "revisionId": change.revision_id,
                    "text": run.text,
                    "formattingDelta": formatting_delta_json(
                        &run.properties,
                        change.prior.as_ref(),
                    ),
                    "anchor": {
                        "node": node.to_string(),
                        "start": start,
                        "end": end,
                    },
                }));
                *offset = end;
            }
            InlineNode::Revision(revision) => {
                let start = *offset;
                let visible = projected
                    && revision
                        .kind
                        .contributes_to(ReviewProjection::FinalWithMarkup);
                let end = if visible {
                    start.saturating_add(projected_revision_len(notes, revision))
                } else {
                    start
                };
                let kind = match revision.kind {
                    RevisionKind::Insertion => "insertion",
                    RevisionKind::Deletion => "deletion",
                    RevisionKind::MoveFrom => "move_from",
                    RevisionKind::MoveTo => "move_to",
                };
                let group_id = revision.editor_group.map(|group| group.id.to_string());
                let group_kind = revision.editor_group.map(|group| match group.kind {
                    RevisionGroupKind::Typing => "typing",
                    RevisionGroupKind::Replacement => "replacement",
                    RevisionGroupKind::Formatting => "formatting",
                });
                let mut item = serde_json::json!({
                    "id": revision.id.to_string(),
                    "kind": kind,
                    "author": revision.author,
                    "date": revision.date,
                    "groupId": group_id,
                    "groupKind": group_kind,
                    "revisionId": revision.revision_id,
                    "text": node_plain_text(&revision.inlines),
                    "anchor": {
                        "node": node.to_string(),
                        "start": start,
                        "end": end,
                    },
                });
                if let Some(link) = move_links.get(&revision.id) {
                    item["movePair"] = serde_json::json!({
                        "name": link.name,
                        "fromStart": link.from_start.to_string(),
                        "toStart": link.to_start.to_string(),
                    });
                }
                out.push(item);
                if visible {
                    collect_review_inline(
                        notes,
                        &revision.inlines,
                        node,
                        offset,
                        true,
                        move_links,
                        out,
                    );
                } else {
                    let mut hidden_offset = start;
                    collect_review_inline(
                        notes,
                        &revision.inlines,
                        node,
                        &mut hidden_offset,
                        false,
                        move_links,
                        out,
                    );
                }
                *offset = end;
            }
            InlineNode::Hyperlink(link) => {
                collect_review_inline(
                    notes,
                    &link.inlines,
                    node,
                    offset,
                    projected,
                    move_links,
                    out,
                );
            }
            InlineNode::Sdt(sdt) => {
                collect_review_inline(
                    notes,
                    &sdt.inlines,
                    node,
                    offset,
                    projected,
                    move_links,
                    out,
                );
            }
            _ if projected => {
                *offset = offset.saturating_add(inline_anchor_len(notes, inline));
            }
            _ => {}
        }
    }
}

/// The operations that decide every paragraph-level revision at once.
///
/// Order matters, and follows Word's own Accept All: formatting and surviving
/// marks resolve first, as property changes that leave the paragraph count
/// alone; merges run last and bottom-up, so each join's following paragraph
/// still exists when it runs. A run of consecutive merging marks chains — each
/// merge carries the properties of whatever paragraph finally survives.
pub fn paragraph_decision_ops(
    document: &Document,
    revisions: &[ParagraphRevisionEntry],
    accept: bool,
) -> Vec<Operation> {
    let mut property_ops = Vec::new();
    let mut merges: Vec<(NodeId, NodeId)> = Vec::new();
    let mut decided: BTreeMap<NodeId, ParagraphProperties> = BTreeMap::new();
    for entry in revisions {
        let (properties, stays_separate) = decide_paragraph_properties(&entry.properties, accept);
        let next = (!stays_separate)
            .then(|| adjacent_next_paragraph(document, entry.node))
            .flatten();
        match next {
            Some(next) => merges.push((entry.node, next)),
            None => {
                let mut properties = properties;
                // A mark that should merge but has nothing to merge into clears.
                properties.mark_revision = None;
                decided.insert(entry.node, properties.clone());
                property_ops.push(Operation::SetParagraphProperties {
                    node: entry.node,
                    properties: Box::new(properties),
                });
            }
        }
    }
    let mut ops = property_ops;
    // One index: the merge loop resolved a paragraph per revision, and each
    // `find_paragraph_any` walks every surface.
    let by_id = ParagraphIndex::build(document);
    for (first, second) in merges.into_iter().rev() {
        let survivor = decided
            .get(&second)
            .cloned()
            .or_else(|| by_id.properties(second));
        let Some(survivor) = survivor else { continue };
        decided.insert(first, survivor.clone());
        ops.push(Operation::JoinParagraphs {
            first,
            second,
            properties: Some(Box::new(survivor)),
        });
    }
    ops
}

/// Which paragraph-level revision a review item refers to (docs/108 Decision 3)./// Which paragraph-level revision a review item refers to (docs/108 Decision 3).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParagraphRevisionPart {
    /// `w:pPr/w:rPr/w:ins|w:del` — the paragraph mark itself.
    Mark,
    /// `w:pPrChange` — a tracked paragraph formatting change.
    Format,
}

/// The review id of a paragraph-level revision: the paragraph id qualified by
/// kind, because the paragraph id alone is shared by both kinds.
pub fn paragraph_revision_id(paragraph: NodeId, part: ParagraphRevisionPart) -> String {
    match part {
        ParagraphRevisionPart::Mark => format!("{paragraph}:mark"),
        ParagraphRevisionPart::Format => format!("{paragraph}:format"),
    }
}

/// Parses a review id produced by [`paragraph_revision_id`].
pub fn parse_paragraph_revision_id(id: &str) -> Option<(&str, ParagraphRevisionPart)> {
    let (paragraph, part) = id.rsplit_once(':')?;
    match part {
        "mark" => Some((paragraph, ParagraphRevisionPart::Mark)),
        "format" => Some((paragraph, ParagraphRevisionPart::Format)),
        _ => None,
    }
}

/// The base paragraph properties a `w:pPrChange` snapshot can hold (`CT_PPrBase`,
/// ISO 29500 §17.13.5.29): everything except the mark's own run properties, a
/// revision of the mark, the section properties and a nested change.
pub fn paragraph_base(properties: &ParagraphProperties) -> ParagraphProperties {
    let mut base = properties.clone();
    base.mark_run = None;
    base.mark_revision = None;
    base.section_break = None;
    base.prop_change = None;
    base
}

/// The paragraph properties Reject restores from a `w:pPrChange` snapshot.
///
/// The snapshot is `CT_PPrBase` (ISO 29500 §17.13.5.29): it never holds the
/// paragraph mark's own run properties, a tracked revision of that mark, or the
/// section properties. Those are separate revisions or not revisions at all, so
/// they keep their CURRENT values rather than being wiped by the restore.
pub fn paragraph_properties_before_change(
    current: &ParagraphProperties,
    prior: &ParagraphProperties,
) -> ParagraphProperties {
    let mut restored = prior.clone();
    restored.mark_run = current.mark_run.clone();
    restored.mark_revision = current.mark_revision.clone();
    restored.section_break = current.section_break;
    restored.prop_change = None;
    restored
}

/// The paragraph that immediately follows `id` in the same block list, when that
/// next block is a paragraph. `None` when `id` is last in its container or is
/// followed by a table — there is then nothing a paragraph mark can merge into.
pub fn adjacent_next_paragraph(document: &Document, id: NodeId) -> Option<NodeId> {
    fn walk(blocks: &[BlockNode], id: NodeId) -> Option<Option<NodeId>> {
        for (index, block) in blocks.iter().enumerate() {
            match block {
                BlockNode::Paragraph(paragraph) if paragraph.id == id => {
                    return Some(match blocks.get(index + 1) {
                        Some(BlockNode::Paragraph(next)) => Some(next.id),
                        _ => None,
                    });
                }
                BlockNode::Table(table) => {
                    for row in &table.rows {
                        for cell in &row.cells {
                            if let Some(found) = walk(&cell.blocks, id) {
                                return Some(found);
                            }
                        }
                    }
                }
                BlockNode::Sdt(sdt) => {
                    if let Some(found) = walk(&sdt.blocks, id) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }
    surface_block_lists(document)
        .into_iter()
        .find_map(|blocks| walk(blocks, id))
        .flatten()
}

/// One paragraph carrying a paragraph-level revision, in document order.
#[derive(Debug)]
pub struct ParagraphRevisionEntry {
    pub node: NodeId,
    pub properties: ParagraphProperties,
}

/// Every paragraph with a mark revision or a formatting change, on every surface,
/// in document order within each surface.
pub fn collect_paragraph_revisions_all(document: &Document) -> Vec<ParagraphRevisionEntry> {
    fn walk(blocks: &[BlockNode], out: &mut Vec<ParagraphRevisionEntry>) {
        for block in blocks {
            match block {
                BlockNode::Paragraph(paragraph) => {
                    let properties = &paragraph.properties;
                    if properties.mark_revision.is_some() || properties.prop_change.is_some() {
                        out.push(ParagraphRevisionEntry {
                            node: paragraph.id,
                            properties: properties.get().clone(),
                        });
                    }
                }
                BlockNode::Table(table) => {
                    for row in &table.rows {
                        for cell in &row.cells {
                            walk(&cell.blocks, out);
                        }
                    }
                }
                BlockNode::Sdt(sdt) => walk(&sdt.blocks, out),
                BlockNode::AltChunk(_) => {}
            }
        }
    }
    let mut out = Vec::new();
    for blocks in surface_block_lists(document) {
        walk(blocks, &mut out);
    }
    out
}

/// A paragraph's properties after deciding its formatting change, and whether its
/// mark revision survives as a separate paragraph (`true`) or is merged into the
/// following paragraph (`false`). Merging is decided by the caller, which knows
/// whether there is a paragraph to merge into.
pub fn decide_paragraph_properties(
    properties: &ParagraphProperties,
    accept: bool,
) -> (ParagraphProperties, bool) {
    let mut decided = match &properties.prop_change {
        Some(change) if !accept => {
            paragraph_properties_before_change(properties, change.prior.as_ref())
        }
        _ => {
            let mut kept = properties.clone();
            kept.prop_change = None;
            kept
        }
    };
    let stays_separate = match &properties.mark_revision {
        // Accepting an inserted mark keeps the split; rejecting a deleted mark keeps
        // the paragraphs apart. Either way the revision itself is resolved.
        Some(mark) => {
            matches!(mark.kind, casual_doc_model::v1::MarkRevisionKind::Insertion) == accept
        }
        None => true,
    };
    if stays_separate {
        decided.mark_revision = None;
    }
    (decided, stays_separate)
}

/// A tracked paragraph formatting change as a list of changed properties, the
/// paragraph counterpart of [`formatting_delta_json`]. Only the base properties a
/// `w:pPrChange` snapshot can hold are compared.
pub fn paragraph_formatting_delta_json(
    current: &ParagraphProperties,
    prior: &ParagraphProperties,
) -> serde_json::Value {
    let mut changes = Vec::new();
    let mut push = |property: &str, before: serde_json::Value, after: serde_json::Value| {
        if before != after {
            changes.push(serde_json::json!({
                "property": property,
                "before": before,
                "after": after,
            }));
        }
    };
    let value = |v: serde_json::Result<serde_json::Value>| v.unwrap_or(serde_json::Value::Null);
    push(
        "style",
        value(serde_json::to_value(prior.style_ref)),
        value(serde_json::to_value(current.style_ref)),
    );
    push(
        "alignment",
        value(serde_json::to_value(prior.alignment)),
        value(serde_json::to_value(current.alignment)),
    );
    push(
        "numbering",
        value(serde_json::to_value(prior.numbering)),
        value(serde_json::to_value(current.numbering)),
    );
    push(
        "indentation",
        value(serde_json::to_value(prior.indentation)),
        value(serde_json::to_value(current.indentation)),
    );
    push(
        "spacing",
        value(serde_json::to_value(prior.spacing)),
        value(serde_json::to_value(current.spacing)),
    );
    push(
        "keepNext",
        serde_json::json!(prior.keep_next),
        serde_json::json!(current.keep_next),
    );
    push(
        "keepLines",
        serde_json::json!(prior.keep_lines),
        serde_json::json!(current.keep_lines),
    );
    push(
        "pageBreakBefore",
        serde_json::json!(prior.page_break_before),
        serde_json::json!(current.page_break_before),
    );
    push(
        "widowControl",
        value(serde_json::to_value(prior.widow_control)),
        value(serde_json::to_value(current.widow_control)),
    );
    push(
        "outlineLevel",
        value(serde_json::to_value(prior.outline_level)),
        value(serde_json::to_value(current.outline_level)),
    );
    push(
        "contextualSpacing",
        serde_json::json!(prior.contextual_spacing),
        serde_json::json!(current.contextual_spacing),
    );
    push(
        "borders",
        value(serde_json::to_value(&prior.borders)),
        value(serde_json::to_value(&current.borders)),
    );
    push(
        "shading",
        value(serde_json::to_value(prior.shading)),
        value(serde_json::to_value(current.shading)),
    );
    push(
        "tabs",
        value(serde_json::to_value(&prior.tabs)),
        value(serde_json::to_value(&current.tabs)),
    );
    push(
        "bidi",
        value(serde_json::to_value(prior.bidi)),
        value(serde_json::to_value(current.bidi)),
    );
    serde_json::Value::Array(changes)
}

pub fn formatting_delta_json(current: &RunProperties, prior: &RunProperties) -> serde_json::Value {
    let mut changes = Vec::new();
    let mut push = |property: &str, before: serde_json::Value, after: serde_json::Value| {
        if before != after {
            changes.push(serde_json::json!({
                "property": property,
                "before": before,
                "after": after,
            }));
        }
    };
    push(
        "bold",
        serde_json::json!(prior.bold),
        serde_json::json!(current.bold),
    );
    push(
        "rtl",
        serde_json::json!(prior.rtl),
        serde_json::json!(current.rtl),
    );
    push(
        "italic",
        serde_json::json!(prior.italic),
        serde_json::json!(current.italic),
    );
    push(
        "underline",
        serde_json::json!(prior.underline),
        serde_json::json!(current.underline),
    );
    push(
        "strike",
        serde_json::json!(prior.strike),
        serde_json::json!(current.strike),
    );
    push(
        "font",
        serde_json::to_value(&prior.font_ref).unwrap_or(serde_json::Value::Null),
        serde_json::to_value(&current.font_ref).unwrap_or(serde_json::Value::Null),
    );
    push(
        "sizeHalfPoints",
        serde_json::json!(prior.size_half_points),
        serde_json::json!(current.size_half_points),
    );
    push(
        "color",
        serde_json::to_value(prior.color).unwrap_or(serde_json::Value::Null),
        serde_json::to_value(current.color).unwrap_or(serde_json::Value::Null),
    );
    push(
        "highlight",
        serde_json::to_value(prior.highlight).unwrap_or(serde_json::Value::Null),
        serde_json::to_value(current.highlight).unwrap_or(serde_json::Value::Null),
    );
    push(
        "verticalAlignment",
        serde_json::to_value(prior.vertical_alignment).unwrap_or(serde_json::Value::Null),
        serde_json::to_value(current.vertical_alignment).unwrap_or(serde_json::Value::Null),
    );
    append_review_changes(&mut changes, current, prior);
    serde_json::Value::Array(changes)
}

/// How many anchor-stream bytes each of the document's note references
/// contributes, by note id — the one piece of "how long is this inline" that
/// cannot be read off the inline node.
///
/// A note reference is painted as its NUMBER, and that number is the note's
/// 1-based position among the document's note definitions, so answering for one
/// reference used to mean holding the whole `Document`. That is precisely what
/// the review splices cannot do: they are handed a `&mut [BlockNode]` taken *out
/// of* the document, so a `&Document` beside it does not borrow-check. A second,
/// document-free copy of the same question was written instead
/// (`inline_anchor_len_for_review`), and it answered **0** for every field, note
/// reference, equation and embedded object — so every comment marker and every
/// accepted/rejected revision in a paragraph holding one of those was placed
/// short by exactly those bytes. That is the third time two functions in this
/// engine have disagreed about a paragraph's length (`inline_text_len` and
/// `append_node_plain_text` carry the comments from the first two).
///
/// The established fix for "a pure function needs one derived fact from a big
/// structure" is not a second function: it is to **precompute an index** and
/// pass that. So this is built once per command — one pass over the two note
/// maps, O(notes) — queried in O(1), and owned, so it outlives the immutable
/// borrow and every caller can use the single [`inline_anchor_len`].
///
/// Footnotes and endnotes are kept apart because their ordinals restart
/// independently and nothing guarantees the two id spaces are disjoint in an
/// imported document.
#[derive(Clone, Debug, Default)]
pub struct NoteAnchorLengths {
    /// Each footnote's painted ordinal label, by note id.
    pub footnotes: HashMap<NoteId, String>,
    /// Each endnote's painted ordinal label, by note id.
    pub endnotes: HashMap<NoteId, String>,
}

/// What a reference to a note the document does not define is painted as, and
/// therefore how wide it is: one byte. An orphaned reference still occupies the
/// width it is drawn at rather than collapsing and dragging every later offset in
/// its paragraph backwards.
pub const ORPHANED_NOTE_LABEL: &str = "?";

impl NoteAnchorLengths {
    /// Indexes `document`'s note definitions. **O(notes)**, once.
    pub fn of(document: &Document) -> Self {
        fn index(
            notes: &casual_doc_model::v1::DefinitionMap<NoteId, casual_doc_model::v1::Note>,
        ) -> HashMap<NoteId, String> {
            notes
                .iter()
                .enumerate()
                .map(|(index, (id, _))| (*id, (index + 1).to_string()))
                .collect()
        }
        Self {
            footnotes: index(&document.definitions().footnotes),
            endnotes: index(&document.definitions().endnotes),
        }
    }

    /// The label `reference` paints — its 1-based ordinal, or
    /// [`ORPHANED_NOTE_LABEL`] when the document does not define the note.
    /// **O(1)**.
    ///
    /// The index carries the label rather than only its length so that the
    /// accessibility projection can announce *which* note a marker points at
    /// without a second index and without a `position()` scan per reference. One
    /// derived fact, one table: the shape the deleted `inline_anchor_len_for_review`
    /// copy failed to use.
    pub fn label_of(&self, reference: &casual_doc_model::v1::NoteReference) -> &str {
        let table = match reference.kind {
            NoteKind::Footnote => &self.footnotes,
            NoteKind::Endnote => &self.endnotes,
        };
        table
            .get(&reference.note)
            .map_or(ORPHANED_NOTE_LABEL, String::as_str)
    }

    /// The bytes `reference` contributes. **O(1)**.
    pub fn len_of(&self, reference: &casual_doc_model::v1::NoteReference) -> u32 {
        self.label_of(reference).len() as u32
    }
}

/// Number of UTF-8 bytes an inline contributes to the layout anchor stream.
/// This mirrors `tabs::split_blocks`: an ordinary tab and a hard break are one
/// position each, a positional tab is geometry only, and synthetic display
/// values contribute the bytes the shaper assigns them.
///
/// **This is the only answer to "how long is this inline" on the wasm side.**
/// There is no review-specific variant and there must never be one again: the
/// review paths splice comment markers and accepted/rejected revisions against
/// offsets that come from this same space, so a second opinion is a marker in
/// the wrong place. `NoteAnchorLengths` exists so the mutating splices can call
/// this one function; `there_is_exactly_one_anchor_length_family` and
/// `every_review_length_answer_agrees_on_one_paragraph` pin it.
///
/// # Known disagreement with the model-offset space, recorded rather than hidden
///
/// This function is **not** the space the host addresses paragraphs in. That space
/// is `casual_doc_layout::flow::node_plain_text` (what `paragraphLength` reports)
/// and `casual_doc_layout::tabs::FlowItem::model_bytes` (what a painted line
/// range is measured in). The three agree on `Run`, `Symbol`, `Field`, and the
/// four inline-in-inline containers, and they differ on:
///
/// | kind | here | `node_plain_text` | `model_bytes` |
/// | --- | --- | --- | --- |
/// | `Math` | painted label | 0 | 0, or the label when unshaped |
/// | `NoteReference` | the ordinal | 0 | 0 |
/// | `EmbeddedObject` without a preview | the label | 0 | the label |
/// | `NoBreakHyphen` / `SoftHyphen` | the glyph | 0 | 0 |
///
/// So a review splice in a paragraph holding an equation is measured in a space
/// the host's offsets do not come from. (`Tab` was in this table, charged zero
/// here; that one is fixed, HF-297.) The rest is **not** fixed here: the
/// fix is for this crate to stop having a length function at all and call
/// `casual-doc-edit`'s, which needs that function made `pub` in another lane's
/// crate. Reported as a row rather than guessed at, because changing this
/// function's semantics moves every review offset in the crate at once.
///
/// The match is **exhaustive on purpose** — no wildcard arm. A wildcard is how a
/// new inline kind gets silently charged zero bytes, which is the same defect as
/// the review copy, arriving by omission instead of by duplication.
///
/// **O(inlines in this subtree)**, O(1) for a leaf; no document walk.
pub fn inline_anchor_len(notes: &NoteAnchorLengths, inline: &InlineNode) -> u32 {
    match inline {
        InlineNode::Run(run) => run.text.len() as u32,
        // A `w:tab` is one position, as `casual_doc_edit::inline_text_len`,
        // `node_plain_text` and `FlowItem::model_bytes` all count it. It was zero
        // here, so every Suggesting-mode splice and comment marker after a tab
        // landed one position early per tab (`docs/109` HF-297). A positional
        // tab and a rule are geometry, not positions, in every space.
        InlineNode::Tab(_) => 1,
        InlineNode::PositionalTab(_) | InlineNode::HorizontalRule(_) => 0,
        // A hard break is one position, as `casual_doc_edit::inline_text_len`
        // counts it (`docs/109` HF-296).
        InlineNode::Break(_) => 1,
        InlineNode::Symbol(symbol) => {
            char::from_u32(symbol.char).map_or(0, |ch| ch.len_utf8() as u32)
        }
        InlineNode::Hyperlink(link) => inlines_anchor_len(notes, &link.inlines),
        InlineNode::Revision(revision)
            if revision
                .kind
                .contributes_to(ReviewProjection::FinalWithMarkup) =>
        {
            inlines_anchor_len(notes, &revision.inlines)
        }
        InlineNode::Revision(_) => 0,
        InlineNode::Sdt(sdt) => inlines_anchor_len(notes, &sdt.inlines),
        InlineNode::Field(field) => field_anchor_len(field),
        InlineNode::NoteReference(reference) => notes.len_of(reference),
        InlineNode::Math(math) => {
            if math.text.is_empty() {
                "[equation]".len() as u32
            } else {
                math.text.len().saturating_add(2) as u32
            }
        }
        InlineNode::NoBreakHyphen(_) => '\u{2011}'.len_utf8() as u32,
        InlineNode::SoftHyphen(_) => '\u{00ad}'.len_utf8() as u32,
        // An embedded object with a cached preview paints as that picture, which
        // occupies no offsets; without one, layout paints its `[chart]`-style
        // label as real text, so the label's bytes ARE the offsets.
        InlineNode::EmbeddedObject(object) => {
            if object.preview.is_some() {
                0
            } else {
                match &object.kind {
                    casual_doc_model::v1::EmbeddedKind::Chart => "[chart]".len() as u32,
                    casual_doc_model::v1::EmbeddedKind::Diagram => "[diagram]".len() as u32,
                    casual_doc_model::v1::EmbeddedKind::OleObject
                    | casual_doc_model::v1::EmbeddedKind::Other(_) => "[object]".len() as u32,
                }
            }
        }
        // A note's OWN number mark (`w:footnoteRef`) paints the enclosing note's
        // label as real text (`flow.rs` `note_number_run`), so this is a known
        // understatement inside a note body — but the label depends on WHICH
        // note body is being flowed, which is context this function does not
        // have and must not guess at. Recorded rather than papered over: a wrong
        // width is worse than a documented zero, because it would move every
        // offset in the note instead of only those after the mark.
        InlineNode::NoteNumberMark(_) => 0,
        // Objects and markers that occupy no offsets in the model-offset space.
        // A drawing is placed, not shaped; a marker is a boundary, not content;
        // a text box or group carries its own block stories, whose paragraphs
        // have offsets of their own and none in this paragraph.
        InlineNode::Drawing(_)
        | InlineNode::AnchoredDrawing(_)
        | InlineNode::TextBox(_)
        | InlineNode::Group(_)
        | InlineNode::CommentReference(_)
        | InlineNode::CommentRangeStart(_)
        | InlineNode::CommentRangeEnd(_)
        | InlineNode::BookmarkStart(_)
        | InlineNode::BookmarkEnd(_)
        | InlineNode::FieldRangeStart(_)
        | InlineNode::FieldRangeEnd(_)
        | InlineNode::MoveRangeStart(_)
        | InlineNode::MoveRangeEnd(_) => 0,
    }
}

/// [`inline_anchor_len`] summed over `inlines`. **O(inlines in the subtree).**
pub fn inlines_anchor_len(notes: &NoteAnchorLengths, inlines: &[InlineNode]) -> u32 {
    inlines.iter().fold(0u32, |total, inline| {
        total.saturating_add(inline_anchor_len(notes, inline))
    })
}

pub fn field_anchor_len(field: &casual_doc_model::v1::Field) -> u32 {
    // A legacy `FORMCHECKBOX` is one position wide, because that is what the
    // reader sees: layout synthesises a single box character from the field's
    // state, and `node_plain_text` now reports that same character. All three
    // have to agree or the caret lands somewhere the box is not - the offsets
    // the checkbox gesture is resolved against come from the plain text, and
    // the positions an edit is applied at come from here.
    if let Some(form) = field.form.as_ref()
        && let FormFieldKind::CheckBox(checkbox) = &form.kind
    {
        return checkbox.glyph().len_utf8() as u32;
    }
    let cached = field.inlines.iter().fold(0u32, |total, inline| {
        let len = match inline {
            InlineNode::Run(run) => run.text.len() as u32,
            InlineNode::Tab(_) => 1,
            _ => 0,
        };
        total.saturating_add(len)
    });
    if cached > 0 {
        return cached;
    }
    matches!(
        field
            .instruction
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_uppercase()
            .as_str(),
        "PAGE" | "NUMPAGES"
    ) as u32
}

// ---- Review projections across every surface ---------------------------------
// A comment or tracked change on header, footer or note text exists in the model
// but was invisible: these projections walked `document.body()` only, so the
// sidebar had nothing to render and a change there could not be decided. Same
// seam, review layer.
pub fn collect_review_comment_anchors_all(
    document: &Document,
    out: &mut Vec<(casual_doc_model::v1::CommentId, NodeId, u32, u32)>,
) {
    let notes = NoteAnchorLengths::of(document);
    for blocks in surface_block_lists(document) {
        collect_review_comment_anchors(&notes, blocks, out);
    }
}

pub fn collect_review_revisions_all(
    document: &Document,
    move_links: &BTreeMap<NodeId, ReviewMoveLink>,
    out: &mut Vec<serde_json::Value>,
) {
    let notes = NoteAnchorLengths::of(document);
    for blocks in surface_block_lists(document) {
        collect_review_revisions(&notes, blocks, move_links, out);
    }
}

pub fn collect_review_move_pairs_all(document: &Document) -> Vec<ReviewMovePair> {
    let mut out = Vec::new();
    for blocks in surface_block_lists(document) {
        out.extend(collect_review_move_pairs(blocks));
    }
    out
}

pub fn find_review_revision_anchor_all(
    document: &Document,
    id: NodeId,
) -> Option<(NodeId, u32, u32, RevisionKind)> {
    let notes = NoteAnchorLengths::of(document);
    surface_block_lists(document)
        .into_iter()
        .find_map(|blocks| find_review_revision_anchor(&notes, blocks, id))
}

pub fn find_review_format_anchor_all(
    document: &Document,
    run_id: NodeId,
) -> Option<(NodeId, u32, u32)> {
    let notes = NoteAnchorLengths::of(document);
    surface_block_lists(document)
        .into_iter()
        .find_map(|blocks| find_review_format_anchor(&notes, blocks, run_id))
}

pub fn collect_review_revision_serial_ids_all(document: &Document, out: &mut Vec<String>) {
    for blocks in surface_block_lists(document) {
        collect_review_revision_serial_ids(blocks, out);
    }
}

pub fn collect_review_revision_ids_all(document: &Document, out: &mut Vec<NodeId>) {
    for blocks in surface_block_lists(document) {
        collect_review_revision_ids(blocks, out);
    }
}

pub fn collect_review_format_ids_all(document: &Document, out: &mut Vec<NodeId>) {
    for blocks in surface_block_lists(document) {
        collect_review_format_ids(blocks, out);
    }
}

/// What an [`InlineNode`] CONTAINS — the one place in this crate that knows the
/// inline container set.
///
/// An inline can contain other inlines, or block content of its own, and every
/// walk over a paragraph has to descend both. Written out ad hoc, each walk
/// implemented three or four of the six containers and stopped; the pair left out
/// was almost always `Sdt`+`Revision` or `TextBox`+`Group`, and the symptom was
/// always the same — content plainly on the page that one subsystem cannot see.
/// Eight such walks were found in this crate at once, which is the evidence that
/// the missing thing was an abstraction and not eight fixes.
///
/// The match in [`inline_descent`] is **exhaustive with no wildcard arm**: a
/// wildcard is how the next inline kind gets silently treated as a leaf, which is
/// this same defect arriving by omission.
#[derive(Debug)]
pub enum InlineDescent<'a> {
    /// Inline children in the SAME paragraph and the same model-offset space:
    /// `Hyperlink`, `Field`, `Revision`, `Sdt`.
    Inlines(&'a [InlineNode]),
    /// A block story of its own — `TextBox`. Its paragraphs have ids and offsets
    /// in the same id space as the body but a DIFFERENT offset space from the
    /// paragraph that anchors the box, so a walk that measures offsets must not
    /// follow this axis (see [`inline_anchor_len`]).
    Blocks(&'a [BlockNode]),
    /// A DrawingML group's children — `Group`. Pictures and shapes are leaves;
    /// its text boxes and nested groups carry block stories.
    Group(&'a [GroupChild]),
    /// Everything else: a leaf, with nothing inside it to visit.
    Leaf,
}

/// The children of one inline node, by descent axis. **O(1)** — it returns
/// borrowed slices and visits nothing.
///
/// The complete container set is `Hyperlink`, `Field`, `Revision`, `Sdt`
/// (inline-in-inline) and `TextBox`, `Group` (block-in-inline). `Revision` is a
/// container whatever its kind: a walk that is looking for content decides for
/// itself whether a non-contributing revision counts, and a walk that is looking
/// for an id must descend one regardless, because a tracked deletion's nodes are
/// still in the tree.
pub fn inline_descent(inline: &InlineNode) -> InlineDescent<'_> {
    match inline {
        InlineNode::Hyperlink(link) => InlineDescent::Inlines(&link.inlines),
        InlineNode::Field(field) => InlineDescent::Inlines(&field.inlines),
        InlineNode::Revision(revision) => InlineDescent::Inlines(&revision.inlines),
        InlineNode::Sdt(sdt) => InlineDescent::Inlines(&sdt.inlines),
        InlineNode::TextBox(text_box) => InlineDescent::Blocks(&text_box.blocks),
        InlineNode::Group(group) => InlineDescent::Group(&group.children),
        InlineNode::Run(_)
        | InlineNode::Tab(_)
        | InlineNode::PositionalTab(_)
        | InlineNode::Break(_)
        | InlineNode::Symbol(_)
        | InlineNode::Drawing(_)
        | InlineNode::AnchoredDrawing(_)
        | InlineNode::EmbeddedObject(_)
        | InlineNode::NoteReference(_)
        | InlineNode::NoteNumberMark(_)
        | InlineNode::Math(_)
        | InlineNode::HorizontalRule(_)
        | InlineNode::NoBreakHyphen(_)
        | InlineNode::SoftHyphen(_)
        | InlineNode::CommentReference(_)
        | InlineNode::CommentRangeStart(_)
        | InlineNode::CommentRangeEnd(_)
        | InlineNode::BookmarkStart(_)
        | InlineNode::BookmarkEnd(_)
        | InlineNode::FieldRangeStart(_)
        | InlineNode::FieldRangeEnd(_)
        | InlineNode::MoveRangeStart(_)
        | InlineNode::MoveRangeEnd(_) => InlineDescent::Leaf,
    }
}

/// The inline children an inline contains IN THE SAME PARAGRAPH, or `None` when it
/// has none. **O(1)**.
///
/// This is [`InlineDescent::Inlines`] on its own, for the walks that must follow
/// only that axis: anything measuring or addressing offsets within one paragraph.
/// A `TextBox`'s or a `Group`'s paragraphs are separate stories with their own
/// offset spaces, so following them would produce an offset in one paragraph for a
/// node that lives in another.
pub fn contained_inlines(inline: &InlineNode) -> Option<&[InlineNode]> {
    match inline_descent(inline) {
        InlineDescent::Inlines(inlines) => Some(inlines),
        InlineDescent::Blocks(_) | InlineDescent::Group(_) | InlineDescent::Leaf => None,
    }
}

/// [`contained_inlines`] for a walk that MUTATES what it finds. **O(1)**.
///
/// Rust cannot share one match across mutability, so this is the one deliberate
/// second spelling of the container set, kept immediately beside the first so the
/// two cannot drift apart unnoticed — and
/// `the_two_faces_of_the_container_set_agree` fails the build if they ever do.
/// Adding a third spelling anywhere else is the defect this module is about.
pub fn contained_inlines_mut(inline: &mut InlineNode) -> Option<&mut Vec<InlineNode>> {
    match inline {
        InlineNode::Hyperlink(link) => Some(&mut link.inlines),
        InlineNode::Field(field) => Some(&mut field.inlines),
        InlineNode::Revision(revision) => Some(&mut revision.inlines),
        InlineNode::Sdt(sdt) => Some(&mut sdt.inlines),
        InlineNode::TextBox(_)
        | InlineNode::Group(_)
        | InlineNode::Run(_)
        | InlineNode::Tab(_)
        | InlineNode::PositionalTab(_)
        | InlineNode::Break(_)
        | InlineNode::Symbol(_)
        | InlineNode::Drawing(_)
        | InlineNode::AnchoredDrawing(_)
        | InlineNode::EmbeddedObject(_)
        | InlineNode::NoteReference(_)
        | InlineNode::NoteNumberMark(_)
        | InlineNode::Math(_)
        | InlineNode::HorizontalRule(_)
        | InlineNode::NoBreakHyphen(_)
        | InlineNode::SoftHyphen(_)
        | InlineNode::CommentReference(_)
        | InlineNode::CommentRangeStart(_)
        | InlineNode::CommentRangeEnd(_)
        | InlineNode::BookmarkStart(_)
        | InlineNode::BookmarkEnd(_)
        | InlineNode::FieldRangeStart(_)
        | InlineNode::FieldRangeEnd(_)
        | InlineNode::MoveRangeStart(_)
        | InlineNode::MoveRangeEnd(_) => None,
    }
}

// `inline_block_stories`, `group_block_stories`, `visit_paragraphs` and
// `visit_text_box_paragraphs` live in `casual_doc_model::v1` (containers), and
// `visit_paragraphs_all_surfaces` in `casual_doc_edit`, beside the
// `surface_block_lists` it walks: moved down so the capability registry
// (`casual-doc-command`, no layout dependency) walks paragraphs the same way
// (`docs/181` §12, lane references). Re-exported so every caller keeps its path.
pub use casual_doc_edit::visit_paragraphs_all_surfaces;
pub use casual_doc_model::v1::{
    group_block_stories, inline_block_stories, visit_paragraphs, visit_text_box_paragraphs,
};

pub fn collect_block_text(blocks: &[BlockNode], out: &mut Vec<(NodeId, String)>) {
    visit_paragraphs(blocks, &mut |paragraph| {
        out.push((paragraph.id, node_plain_text(&paragraph.inlines)));
    });
}

pub fn character_values(props: &RunProperties) -> serde_json::Map<String, serde_json::Value> {
    serde_json::json!({
        "allCaps":props.all_caps,"smallCaps":props.small_caps,"doubleStrike":props.double_strike,
        "outline":props.outline,"shadow":props.shadow,"emboss":props.emboss,"imprint":props.imprint,
        "emphasis":props.emphasis,"border":props.border,"characterSpacingTwips":props.character_spacing_twips,
        "characterScalePercent":props.character_scale_percent,"positionHalfPoints":props.position_half_points,
        "kerningHalfPoints":props.kerning_half_points,"ligatures":props.ligatures,
        // The Latin `w:lang` tag: what Set Language writes, and so what a tracked
        // language change reports (`docs/153` `text.language-of-selection`).
        "language":props.language.as_ref().and_then(|language| language.value.as_deref()),
    }).as_object().expect("character projection is an object").clone()
}

pub fn append_review_changes(
    changes: &mut Vec<serde_json::Value>,
    current: &RunProperties,
    prior: &RunProperties,
) {
    let before = character_values(prior);
    for (property, after) in character_values(current) {
        if before[&property] != after {
            changes.push(
                serde_json::json!({"property":property,"before":before[&property],"after":after}),
            );
        }
    }
}
