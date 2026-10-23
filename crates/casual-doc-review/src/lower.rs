// SPDX-License-Identifier: Apache-2.0

//! The review commands, lowered to operations: every tracked suggestion, every
//! comment action and every accept/reject decision the editor offers, as pure
//! functions of a document and two id sources.
//!
//! This is `docs/178` §8 prerequisite A1. These bodies used to be methods on
//! `casual-doc-wasm`'s `WasmDocument`, so a native caller (the Native SDK, the
//! MCP server) could only make a suggestion by writing a second lowering, which
//! would drift from the editor's. Now the editor's methods are thin wrappers that
//! resolve the host identity, call one of these, and commit the result through
//! their own history; a native caller does the same through `casual_doc_edit`.
//!
//! Each function returns a [`Lowered`]: the operations to apply as ONE undoable
//! step, and where the caret lands. Nothing here mutates the document; the
//! caller applies `operations` (and owns undo, history coalescing and typing
//! sessions). An empty `operations` means "no change, move the caret": the one
//! command that can answer that is [`suggest_delete_paragraph_mark`] over a mark
//! already suggested for deletion.
//!
//! **Id allocation order is part of the contract.** The functions draw node ids
//! and revision serials in exactly the order the editor always did, so the same
//! command on the same document yields the same ids whether a person or a
//! program issued it.
//!
//! Complexity: each command is O(the paragraphs it touches) for the splice, plus
//! O(notes) once to index note-reference widths, plus the O(document) projection
//! [`update_review_operation`] already costs. The `decide_all_revisions` and
//! comment-deletion commands walk every surface, O(document).

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

use casual_doc_edit::{
    FormatDelta, Operation, ParagraphIndex, Pos, SplitProperties, caret_run_properties,
    find_paragraph_any, paragraph_properties, refused, surface_block_lists,
};
use casual_doc_model::v1::{
    BlockNode, Comment, CommentId, CommentRangeEnd, CommentRangeStart, CommentReference,
    DefinitionMap, Document, InlineNode, MarkRevision, MarkRevisionKind, Paragraph,
    ParagraphProperties, PropChange, Revision, RevisionGroup, RevisionGroupKind, RevisionKind, Run,
    RunProperties,
};
use casual_doc_model::{IdGenerator, NodeId};

use crate::body::{
    COMMENT_RANGE_REFUSAL, NoteAnchorLengths, ParagraphRevisionPart, ReviewDecisionTarget,
    RevisionIdAllocator, adjacent_next_paragraph, apply_review_delta, apply_review_format_change,
    collect_block_text, collect_paragraph_revisions_all, collect_review_editor_group_ids,
    collect_review_format_ids_all, collect_review_move_pairs_all, collect_review_revision_ids_all,
    decide_all_review_revisions, decide_review_format_change, decide_review_revision,
    extend_authored_insertion_interior, extend_review_group_insertion,
    find_review_format_anchor_all, find_review_format_change, find_review_revision,
    find_review_revision_anchor_all, insert_comment_markers_at, insert_review_comment_markers,
    insert_review_revision, next_comment_para_id, paragraph_base, paragraph_decision_ops,
    paragraph_properties_before_change, parse_paragraph_revision_id,
    remove_authored_review_insertion, remove_review_comment_markers, remove_review_move_markers,
    review_paragraph_body, review_surfaces, strip_authored_insertions_in_range,
    update_review_operation, update_review_operation_across, validate_authored_revision_author,
    validate_review_group, wrap_review_deletion,
};

/// The two id sources a review command draws from: node ids for every inline,
/// revision and group it creates, and the `w:id` serials Word requires to be
/// unique across a document's revisions.
#[derive(Debug)]
pub struct ReviewIds<'a> {
    /// Node ids, from the session's editing namespace.
    pub nodes: &'a mut IdGenerator,
    /// Revision serials (`w:id`), unique across the document.
    pub revisions: &'a mut RevisionIdAllocator,
}

impl ReviewIds<'_> {
    fn node(&mut self) -> Result<NodeId, String> {
        self.nodes
            .next_id()
            .map_err(|_| "id space exhausted".to_string())
    }
}

/// A review command lowered to operations.
#[derive(Clone, Debug, PartialEq)]
pub struct Lowered {
    /// The operations, applied in order as one undoable step. Empty means the
    /// command changed nothing and only moves the caret.
    pub operations: Vec<Operation>,
    /// Where the caret lands after the step.
    pub caret: Pos,
    /// The revision group a typing suggestion joined or opened, so the caller
    /// can continue it on the next keystroke. `None` for every other command.
    pub group: Option<RevisionGroup>,
}

impl Lowered {
    fn new(operations: Vec<Operation>, caret: Pos) -> Self {
        Self {
            operations,
            caret,
            group: None,
        }
    }
}

fn node_id(node: &str) -> Result<NodeId, String> {
    NodeId::from_str(node).map_err(|_| format!("invalid node id: {node}"))
}

/// The paragraph's text as the editor addresses it, on any surface. Empty when
/// the node is not a paragraph. O(document): it walks every surface.
#[must_use]
pub fn paragraph_text(document: &Document, node: NodeId) -> String {
    let mut nodes: Vec<(NodeId, String)> = Vec::new();
    for blocks in surface_block_lists(document) {
        collect_block_text(blocks, &mut nodes);
    }
    nodes
        .into_iter()
        .find(|(id, _)| *id == node)
        .map(|(_, text)| text)
        .unwrap_or_default()
}

fn comment_body(ids: &mut ReviewIds<'_>, text: &str) -> Result<Vec<BlockNode>, String> {
    comment_body_mentioning(ids, text, &[])
}

/// One `@` mention inside a comment's text (`docs/175` §7.3): the span of the text that names
/// the person, and the link the host's directory gave for them (a `mailto:` or an `https:`
/// profile page), if any.
///
/// Offsets are **UTF-16 code units**, the unit the composer's `selectionStart` reports, so the
/// chrome passes what the browser measured and nothing is re-counted on its side.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Mention {
    /// Where the `@Name` span starts.
    pub start: u32,
    /// Where it ends (exclusive).
    pub end: u32,
    /// The entry's link, or `None` for a mention that is only text.
    #[serde(default)]
    pub link: Option<String>,
}

/// The longest mention link accepted, the model's hyperlink bound.
const MAX_MENTION_LINK_BYTES: usize = 2048;

/// The comment body: one paragraph whose mention spans are hyperlink runs to their links.
///
/// **The shape is the editor's, not a measured Word one.** `docs/175` §12 Q4 asks for Word's
/// own mention shape to be measured from a Word-authored file before the model is extended;
/// none is in the corpus. So the model is **not** extended: a mention is the existing
/// `Hyperlink` inline around the `@Name` run, which every reader already round-trips, and
/// a mention with no link is plain text. Nothing here freezes a shape the measurement could
/// contradict. O(text + mentions).
fn comment_body_mentioning(
    ids: &mut ReviewIds<'_>,
    text: &str,
    mentions: &[Mention],
) -> Result<Vec<BlockNode>, String> {
    let body_id = ids.node()?;
    let inlines = mention_inlines(ids, text, mentions)?;
    Ok(vec![BlockNode::Paragraph(Paragraph {
        id: body_id,
        properties: ParagraphProperties::default().into(),
        inlines,
    })])
}

/// `text` split into runs, each linked mention wrapped in a hyperlink.
///
/// Refused rather than repaired: mentions that overlap, run backwards, fall outside the text,
/// split a character, or carry a link that is not `https:` or `mailto:` (or is over the
/// model's bound). A mention the editor cannot place exactly is one it must not guess at.
fn mention_inlines(
    ids: &mut ReviewIds<'_>,
    text: &str,
    mentions: &[Mention],
) -> Result<Vec<InlineNode>, String> {
    use casual_doc_model::v1::{ExternalTarget, Hyperlink, HyperlinkTarget};
    let refusal = || {
        refused!(
            "comment.mention-unusable",
            "A mention in this comment could not be placed. Type the name again and pick it from the list."
        )
        .to_string()
    };
    // UTF-16 offset -> byte offset, for every character boundary. O(text).
    let mut boundaries = Vec::with_capacity(text.len() + 1);
    let mut units = 0_u32;
    for (byte, character) in text.char_indices() {
        boundaries.push((units, byte));
        units += u32::try_from(character.len_utf16()).unwrap_or(2);
    }
    boundaries.push((units, text.len()));
    let byte_at = |unit: u32| {
        boundaries
            .binary_search_by_key(&unit, |(u, _)| *u)
            .ok()
            .map(|index| boundaries[index].1)
    };
    let mut sorted: Vec<&Mention> = mentions.iter().collect();
    sorted.sort_by_key(|mention| mention.start);
    let mut inlines = Vec::new();
    // `at`: where the text not yet emitted starts. `placed`: where the last mention ended, so
    // overlap is refused even across an unlinked mention that emitted nothing.
    let mut at = 0_usize;
    let mut placed = 0_usize;
    let run = |ids: &mut ReviewIds<'_>, piece: &str| -> Result<InlineNode, String> {
        Ok(InlineNode::Run(Run {
            id: ids.node()?,
            properties: RunProperties::default().into(),
            text: piece.to_owned(),
        }))
    };
    for mention in sorted {
        let (Some(start), Some(end)) = (byte_at(mention.start), byte_at(mention.end)) else {
            return Err(refusal());
        };
        if start >= end || start < placed {
            return Err(refusal());
        }
        placed = end;
        // A mention with no link is text: it stays inside the surrounding run, because two
        // adjacent runs with one formatting are not a shape the model keeps.
        let Some(link) = mention.link.as_deref().filter(|link| !link.is_empty()) else {
            continue;
        };
        if start > at {
            inlines.push(run(ids, &text[at..start])?);
        }
        let named = run(ids, &text[start..end])?;
        let allowed = (link.starts_with("mailto:") || link.starts_with("https://"))
            && link.len() <= MAX_MENTION_LINK_BYTES
            && !link.chars().any(char::is_whitespace);
        if !allowed {
            return Err(refusal());
        }
        inlines.push(InlineNode::Hyperlink(Box::new(Hyperlink {
            id: ids.node()?,
            target: HyperlinkTarget::External(ExternalTarget {
                url: link.to_owned(),
                anchor: None,
            }),
            tooltip: None,
            inlines: vec![named],
            history: true,
        })));
        at = end;
    }
    if at < text.len() || inlines.is_empty() {
        inlines.push(run(ids, &text[at..])?);
    }
    Ok(inlines)
}

fn comments_only(document: &Document, comments: DefinitionMap<CommentId, Comment>) -> Lowered {
    Lowered::new(
        vec![Operation::UpdateReviewState {
            paragraphs: Vec::new(),
            comments: Some(comments),
        }],
        Pos::new(document.id(), 0),
    )
}

fn comment_id(comment: &str) -> Result<CommentId, String> {
    NodeId::from_str(comment)
        .map(CommentId::new)
        .map_err(|_| "invalid comment id".to_string())
}

/// Creates a comment over a selection that may span paragraphs: the range start
/// in `start_node` at `start`, the range end and the reference in `end_node` at
/// `end`. The endpoints are expected in document order.
#[allow(clippy::too_many_arguments)]
pub fn add_comment(
    document: &Document,
    ids: &mut ReviewIds<'_>,
    start_node: &str,
    start: u32,
    end_node: &str,
    end: u32,
    text: &str,
    author: Option<String>,
    initials: Option<String>,
    date: Option<String>,
) -> Result<Lowered, String> {
    add_comment_mentioning(
        document,
        ids,
        start_node,
        start,
        end_node,
        end,
        text,
        &[],
        author,
        initials,
        date,
    )
}

/// [`add_comment`], with `@` mentions in the text linked to their directory entries
/// (`docs/175` §7.3; see [`Mention`]).
#[allow(clippy::too_many_arguments)]
pub fn add_comment_mentioning(
    document: &Document,
    ids: &mut ReviewIds<'_>,
    start_node: &str,
    start: u32,
    end_node: &str,
    end: u32,
    text: &str,
    mentions: &[Mention],
    author: Option<String>,
    initials: Option<String>,
    date: Option<String>,
) -> Result<Lowered, String> {
    if text.is_empty() {
        return Err(refused!(
            "comment.text-required",
            "Select some text and write a comment on it."
        )
        .to_string());
    }
    let start_node = node_id(start_node)?;
    let end_node = node_id(end_node)?;
    if start_node == end_node && start >= end {
        return Err(refused!(
            "comment.text-required",
            "Select some text and write a comment on it."
        )
        .to_string());
    }
    // The note-ordinal index for this one command, built once before the
    // review body is spliced. O(notes); see `NoteAnchorLengths`.
    let notes = NoteAnchorLengths::of(document);
    let comment = CommentId::new(ids.node()?);
    let start_id = ids.node()?;
    let end_id = ids.node()?;
    let reference_id = ids.node()?;
    let body_id = ids.node()?;
    let run_id = ids.node()?;
    let range_start = CommentRangeStart {
        id: start_id,
        comment,
    };
    let range_end = CommentRangeEnd {
        id: end_id,
        comment,
    };
    let reference = CommentReference {
        id: reference_id,
        comment,
    };
    let (body, caret) = if start_node == end_node {
        let mut body = review_paragraph_body(document, start_node)?;
        if !insert_review_comment_markers(
            &notes,
            &mut body,
            start_node,
            start,
            end,
            range_start,
            range_end,
            reference,
            ids.nodes,
        ) {
            return Err(COMMENT_RANGE_REFUSAL.to_string());
        }
        (body, Pos::new(start_node, end))
    } else {
        // Cross-paragraph span: the start marker is placed in `start_node` and
        // the end marker plus reference in `end_node`. Both paragraphs ride in
        // one body vec so the single review operation carries both changed
        // paragraphs (its inverse removes both markers).
        let mut body = review_paragraph_body(document, start_node)?;
        body.extend(review_paragraph_body(document, end_node)?);
        if !insert_comment_markers_at(
            &notes,
            &mut body,
            start_node,
            start,
            &[InlineNode::CommentRangeStart(range_start)],
            ids.nodes,
        ) || !insert_comment_markers_at(
            &notes,
            &mut body,
            end_node,
            end,
            &[
                InlineNode::CommentRangeEnd(range_end),
                InlineNode::CommentReference(reference),
            ],
            ids.nodes,
        ) {
            return Err(COMMENT_RANGE_REFUSAL.to_string());
        }
        (body, Pos::new(end_node, end))
    };
    // With no mention the body is exactly what it always was, ids included.
    let inlines = if mentions.is_empty() {
        vec![InlineNode::Run(Run {
            id: run_id,
            properties: RunProperties::default().into(),
            text: text.to_owned(),
        })]
    } else {
        mention_inlines(ids, text, mentions)?
    };
    let mut comments = document.definitions().comments.clone();
    let para_id = next_comment_para_id(&comments)?;
    comments.insert(
        comment,
        Comment {
            blocks: vec![BlockNode::Paragraph(Paragraph {
                id: body_id,
                properties: ParagraphProperties::default().into(),
                inlines,
            })],
            author,
            initials,
            date,
            para_id: Some(para_id),
            parent_para_id: None,
            done: false,
            durable_id: None,
            person: None,
        },
    );
    let operation = update_review_operation(document, &body, Some(comments))?;
    Ok(Lowered::new(vec![operation], caret))
}

/// Resolves or reopens a comment.
pub fn set_comment_resolved(
    document: &Document,
    comment: &str,
    done: bool,
) -> Result<Lowered, String> {
    let id = comment_id(comment)?;
    let mut comments = document.definitions().comments.clone();
    let Some(mut value) = comments.get(&id).cloned() else {
        return Err("comment not found".to_string());
    };
    value.done = done;
    comments.insert(id, value);
    Ok(comments_only(document, comments))
}

/// Adds a reply to a comment thread.
pub fn reply_to_comment(
    document: &Document,
    ids: &mut ReviewIds<'_>,
    parent: &str,
    text: &str,
    author: Option<String>,
    initials: Option<String>,
    date: Option<String>,
) -> Result<Lowered, String> {
    reply_mentioning(document, ids, parent, text, &[], author, initials, date)
}

/// [`reply_to_comment`], with `@` mentions linked (see [`Mention`]).
#[allow(clippy::too_many_arguments)]
pub fn reply_mentioning(
    document: &Document,
    ids: &mut ReviewIds<'_>,
    parent: &str,
    text: &str,
    mentions: &[Mention],
    author: Option<String>,
    initials: Option<String>,
    date: Option<String>,
) -> Result<Lowered, String> {
    if text.is_empty() {
        return Err(refused!(
            "comment.reply-text-required",
            "Write something before posting the reply."
        )
        .to_string());
    }
    let parent = comment_id(parent)?;
    let mut comments = document.definitions().comments.clone();
    let Some(mut parent_comment) = comments.get(&parent).cloned() else {
        return Err("comment not found".to_string());
    };
    let parent_para_id = match parent_comment.para_id.clone() {
        Some(para_id) => para_id,
        None => {
            let para_id = next_comment_para_id(&comments)?;
            parent_comment.para_id = Some(para_id.clone());
            comments.insert(parent, parent_comment);
            para_id
        }
    };
    let para_id = next_comment_para_id(&comments)?;
    let id = CommentId::new(ids.node()?);
    let blocks = comment_body_mentioning(ids, text, mentions)?;
    comments.insert(
        id,
        Comment {
            blocks,
            author,
            initials,
            date,
            para_id: Some(para_id),
            parent_para_id: Some(parent_para_id),
            done: false,
            durable_id: None,
            person: None,
        },
    );
    Ok(comments_only(document, comments))
}

/// `removed` plus every reply below it, by the `paraId` parent chain.
fn with_descendant_replies(
    source: &DefinitionMap<CommentId, Comment>,
    root: CommentId,
    root_para_id: Option<String>,
) -> BTreeSet<CommentId> {
    let mut removed = BTreeSet::from([root]);
    let mut parent_keys = BTreeSet::from([root.node_id().to_string()]);
    if let Some(para_id) = root_para_id {
        parent_keys.insert(para_id);
    }
    loop {
        let mut changed = false;
        for (key, value) in source.iter() {
            if !removed.contains(key)
                && value
                    .parent_para_id
                    .as_ref()
                    .is_some_and(|parent| parent_keys.contains(parent))
            {
                removed.insert(*key);
                parent_keys.insert(key.node_id().to_string());
                if let Some(para_id) = value.para_id.clone() {
                    parent_keys.insert(para_id);
                }
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    removed
}

fn without(
    source: &DefinitionMap<CommentId, Comment>,
    removed: &BTreeSet<CommentId>,
) -> DefinitionMap<CommentId, Comment> {
    let mut comments = DefinitionMap::default();
    for (key, value) in source.iter() {
        if !removed.contains(key) {
            comments.insert(*key, value.clone());
        }
    }
    comments
}

/// Removes a comment thread, its replies and all of its markers, on every
/// surface.
pub fn delete_comment(document: &Document, comment: &str) -> Result<Lowered, String> {
    let id = comment_id(comment)?;
    let source_comments = document.definitions().comments.clone();
    let Some(root) = source_comments.get(&id) else {
        return Err("comment not found".to_string());
    };
    let removed = with_descendant_replies(&source_comments, id, root.para_id.clone());
    let comments = without(&source_comments, &removed);
    let mut surfaces = review_surfaces(document);
    for blocks in &mut surfaces {
        for removed_id in &removed {
            remove_review_comment_markers(blocks, *removed_id);
        }
    }
    let operation = update_review_operation_across(document, &surfaces, Some(comments))?;
    Ok(Lowered::new(vec![operation], Pos::new(document.id(), 0)))
}

/// Replaces the body text of a comment or reply, keeping its threading,
/// resolved state, author and anchors.
pub fn update_comment(
    document: &Document,
    ids: &mut ReviewIds<'_>,
    comment: &str,
    text: &str,
) -> Result<Lowered, String> {
    if text.is_empty() {
        return Err(refused!(
            "comment.edit-text-required",
            "Write something before posting the comment."
        )
        .to_string());
    }
    let id = comment_id(comment)?;
    let mut comments = document.definitions().comments.clone();
    let Some(mut value) = comments.get(&id).cloned() else {
        return Err("comment not found".to_string());
    };
    value.blocks = comment_body(ids, text)?;
    comments.insert(id, value);
    Ok(comments_only(document, comments))
}

/// Deletes one reply and the replies below it. A thread root is refused:
/// deleting a whole thread is [`delete_comment`]'s job.
pub fn delete_reply(document: &Document, reply: &str) -> Result<Lowered, String> {
    let id = comment_id(reply)?;
    let source_comments = document.definitions().comments.clone();
    let Some(target) = source_comments.get(&id) else {
        return Err("reply not found".to_string());
    };
    if target.parent_para_id.is_none() {
        return Err("comment is a thread root, not a reply; use deleteComment".to_string());
    }
    let removed = with_descendant_replies(&source_comments, id, target.para_id.clone());
    Ok(comments_only(document, without(&source_comments, &removed)))
}

/// What a suggested insertion carries.
#[derive(Clone, Debug)]
pub enum InsertStyle<'a> {
    /// A plain run with no direct formatting (`suggestInsert`).
    Plain,
    /// A run carrying the typing format armed at the caret (`suggestStyledInsert`):
    /// `delta` applied over default properties, or over the caret's own run
    /// properties when `inherit_caret` is set.
    Delta {
        /// The formatting to apply to the inserted run.
        delta: &'a FormatDelta,
        /// Start from the run properties at the caret instead of the default.
        inherit_caret: bool,
    },
}

/// A tracked insertion at a same-paragraph caret.
///
/// `continuing_group` is the typing group of the previous keystroke when this
/// one continues it (the caller tracks typing sessions); the result's `group`
/// is the group this insertion joined or opened. Typing strictly inside the
/// author's own pending insertion extends that suggestion (docs/86 decision 1).
#[allow(clippy::too_many_arguments)]
pub fn suggest_insert(
    document: &Document,
    ids: &mut ReviewIds<'_>,
    node: &str,
    offset: u32,
    text: &str,
    style: InsertStyle<'_>,
    author: Option<String>,
    date: Option<String>,
    continuing_group: Option<RevisionGroup>,
) -> Result<Lowered, String> {
    if text.is_empty() {
        return Err("suggested text must be non-empty".to_string());
    }
    validate_authored_revision_author(author.as_deref())?;
    let node = node_id(node)?;
    let group = match continuing_group {
        Some(group) => group,
        None => RevisionGroup {
            id: ids.node()?,
            kind: RevisionGroupKind::Typing,
        },
    };
    let run = ids.node()?;
    let addition = match style {
        InsertStyle::Plain => vec![InlineNode::Run(Run {
            id: run,
            properties: RunProperties::default().into(),
            text: text.to_owned(),
        })],
        InsertStyle::Delta {
            delta,
            inherit_caret,
        } => {
            let mut inlines = vec![InlineNode::Run(Run {
                id: run,
                properties: if inherit_caret {
                    let mut props = caret_run_properties(document, node, offset)
                        .cloned()
                        .unwrap_or_default();
                    props.prop_change = None;
                    props.into()
                } else {
                    RunProperties::default().into()
                },
                text: text.to_owned(),
            })];
            apply_review_delta(&mut inlines, delta);
            inlines
        }
    };
    let mut body = review_paragraph_body(document, node)?;
    // The note-ordinal index for this one command, built once before the
    // review body is spliced. O(notes); see `NoteAnchorLengths`.
    let notes = NoteAnchorLengths::of(document);
    let caret = Pos::new(node, offset + text.len() as u32);
    if let Some(interior_group) = extend_authored_insertion_interior(
        &notes,
        &mut body,
        node,
        offset,
        &addition,
        author.as_deref(),
        ids.nodes,
    ) {
        let operation = update_review_operation(document, &body, None)?;
        return Ok(Lowered {
            operations: vec![operation],
            caret,
            group: Some(interior_group),
        });
    }
    let changed = if continuing_group.is_some() {
        extend_review_group_insertion(&notes, &mut body, node, offset, group, addition)
    } else {
        let revision = ids.node()?;
        let revision_id = ids.revisions.allocate()?;
        insert_review_revision(
            &notes,
            &mut body,
            node,
            offset,
            Revision {
                id: revision,
                kind: RevisionKind::Insertion,
                author,
                date,
                revision_id: Some(revision_id),
                editor_group: Some(group),
                inlines: addition,
            },
            ids.nodes,
        )
    };
    if !changed {
        return Err("suggestion requires a top-level paragraph text caret".to_string());
    }
    let operation = update_review_operation(document, &body, None)?;
    Ok(Lowered {
        operations: vec![operation],
        caret,
        group: Some(group),
    })
}

/// Replaces a same-paragraph range with a tracked deletion plus a tracked
/// insertion, in one replacement group decided as one.
#[allow(clippy::too_many_arguments)]
pub fn suggest_replace(
    document: &Document,
    ids: &mut ReviewIds<'_>,
    node: &str,
    start: u32,
    end: u32,
    text: &str,
    author: Option<String>,
    date: Option<String>,
) -> Result<Lowered, String> {
    if start >= end || text.is_empty() {
        return Err("suggested replacement requires a range and text".to_string());
    }
    validate_authored_revision_author(author.as_deref())?;
    let node = node_id(node)?;
    let group = RevisionGroup {
        id: ids.node()?,
        kind: RevisionGroupKind::Replacement,
    };
    let deletion = ids.node()?;
    let insertion = ids.node()?;
    let run = ids.node()?;
    let deletion_revision_id = ids.revisions.allocate()?;
    let insertion_revision_id = ids.revisions.allocate()?;
    let mut body = review_paragraph_body(document, node)?;
    let notes = NoteAnchorLengths::of(document);
    if !wrap_review_deletion(
        &notes,
        &mut body,
        node,
        start,
        end,
        Revision {
            id: deletion,
            kind: RevisionKind::Deletion,
            author: author.clone(),
            date: date.clone(),
            revision_id: Some(deletion_revision_id),
            editor_group: Some(group),
            inlines: Vec::new(),
        },
        ids.nodes,
    ) {
        return Err("suggested replacement requires top-level paragraph text".to_string());
    }
    if !insert_review_revision(
        &notes,
        &mut body,
        node,
        start,
        Revision {
            id: insertion,
            kind: RevisionKind::Insertion,
            author,
            date,
            revision_id: Some(insertion_revision_id),
            editor_group: Some(group),
            inlines: vec![InlineNode::Run(Run {
                id: run,
                properties: RunProperties::default().into(),
                text: text.to_owned(),
            })],
        },
        ids.nodes,
    ) {
        return Err("suggested replacement insertion failed".to_string());
    }
    let operation = update_review_operation(document, &body, None)?;
    let caret = Pos::new(
        node,
        start.saturating_add(u32::try_from(text.len()).unwrap_or(u32::MAX)),
    );
    Ok(Lowered {
        operations: vec![operation],
        caret,
        group: Some(group),
    })
}

/// A tracked deletion of a same-paragraph range.
pub fn suggest_delete(
    document: &Document,
    ids: &mut ReviewIds<'_>,
    node: &str,
    start: u32,
    end: u32,
    author: Option<String>,
    date: Option<String>,
) -> Result<Lowered, String> {
    if start >= end {
        return Err("suggested deletion requires a range".to_string());
    }
    validate_authored_revision_author(author.as_deref())?;
    let node = node_id(node)?;
    let mut body = review_paragraph_body(document, node)?;
    let notes = NoteAnchorLengths::of(document);
    suggest_deletion_into_body(ids, &notes, &mut body, node, start, end, author, date)?;
    let operation = update_review_operation(document, &body, None)?;
    Ok(Lowered::new(vec![operation], Pos::new(node, start)))
}

/// One paragraph's share of a suggested deletion, applied to a review body that
/// may hold several paragraphs. Single- and cross-paragraph deletion share it,
/// so both follow docs/86 decision 2 the same way: the author's own pending
/// insertions are removed outright and only accepted text is suggested deleted.
#[allow(clippy::too_many_arguments)]
fn suggest_deletion_into_body(
    ids: &mut ReviewIds<'_>,
    notes: &NoteAnchorLengths,
    body: &mut Vec<BlockNode>,
    node: NodeId,
    start: u32,
    end: u32,
    author: Option<String>,
    date: Option<String>,
) -> Result<(), String> {
    if remove_authored_review_insertion(notes, body, node, start, end, author.as_deref()) {
        return Ok(());
    }
    let mut mixed = body.clone();
    if let Some(removed) = strip_authored_insertions_in_range(
        &NoteAnchorLengths::default(),
        &mut mixed,
        node,
        start,
        end,
        author.as_deref(),
    ) {
        let new_end = end.saturating_sub(removed);
        if new_end > start {
            let revision = ids.node()?;
            let revision_id = ids.revisions.allocate()?;
            if !wrap_review_deletion(
                notes,
                &mut mixed,
                node,
                start,
                new_end,
                Revision {
                    id: revision,
                    kind: RevisionKind::Deletion,
                    author: author.clone(),
                    date: date.clone(),
                    revision_id: Some(revision_id),
                    editor_group: None,
                    inlines: Vec::new(),
                },
                ids.nodes,
            ) {
                return Err(refused!(
                    "review.deletion-not-body-text",
                    "A deletion can only be suggested over ordinary paragraph text."
                )
                .to_string());
            }
        }
        *body = mixed;
        return Ok(());
    }
    let revision = ids.node()?;
    let revision_id = ids.revisions.allocate()?;
    if !wrap_review_deletion(
        notes,
        body,
        node,
        start,
        end,
        Revision {
            id: revision,
            kind: RevisionKind::Deletion,
            author,
            date,
            revision_id: Some(revision_id),
            editor_group: None,
            inlines: Vec::new(),
        },
        ids.nodes,
    ) {
        return Err(refused!(
            "review.deletion-not-body-text",
            "A deletion can only be suggested over ordinary paragraph text."
        )
        .to_string());
    }
    Ok(())
}

/// Records a paragraph formatting change as a tracked `w:pPrChange`.
///
/// The snapshot is the paragraph as it was before ANY pending formatting
/// suggestion: a second change keeps the first one's prior, so Reject returns
/// the paragraph to where review started. A change that lands back on the
/// prior leaves no revision at all.
pub fn track_paragraph_change(
    revisions: &mut RevisionIdAllocator,
    author: Option<String>,
    date: Option<String>,
    current: &ParagraphProperties,
    mut next: ParagraphProperties,
) -> Result<ParagraphProperties, String> {
    validate_authored_revision_author(author.as_deref())?;
    let (prior, revision_id) = match &current.prop_change {
        Some(change) => (change.prior.as_ref().clone(), change.revision_id.clone()),
        None => (paragraph_base(current), None),
    };
    if paragraph_base(&next) == prior {
        next.prop_change = None;
        return Ok(next);
    }
    let revision_id = match revision_id {
        Some(id) => Some(id),
        None => Some(revisions.allocate()?),
    };
    next.prop_change = Some(Box::new(PropChange {
        author,
        date,
        revision_id,
        editor_group: None,
        prior: Box::new(prior),
    }));
    Ok(next)
}

/// Enter while Suggesting: splits the paragraph and marks the LEADING half's
/// paragraph mark as a tracked insertion, as Word does.
pub fn suggest_split(
    document: &Document,
    ids: &mut ReviewIds<'_>,
    node: &str,
    offset: u32,
    author: Option<String>,
    date: Option<String>,
) -> Result<Lowered, String> {
    validate_authored_revision_author(author.as_deref())?;
    let node = node_id(node)?;
    let current =
        paragraph_properties(document, node).ok_or_else(|| "paragraph not found".to_string())?;
    let len = paragraph_text(document, node).len() as u32;
    if offset > len {
        return Err("the split is past the end of the paragraph".to_string());
    }
    let revision_id = ids.revisions.allocate()?;
    let mut leading = current.clone();
    leading.mark_revision = Some(Box::new(MarkRevision {
        kind: MarkRevisionKind::Insertion,
        author,
        date,
        revision_id: Some(revision_id),
    }));
    let mut trailing = current.clone();
    // Word's `w:next`: Enter at the END of a heading starts its follow-on style.
    if offset == len
        && let Some(style) = current.style_ref
        && let Some(next) = document
            .definitions()
            .styles
            .get(&style)
            .and_then(|definition| definition.next)
        && next != style
    {
        trailing.style_ref = Some(next);
    }
    let new_id = ids.node()?;
    Ok(Lowered::new(
        vec![Operation::SplitParagraph {
            at: Pos::new(node, offset),
            new_id,
            properties: Some(Box::new(SplitProperties { leading, trailing })),
        }],
        Pos::new(new_id, 0),
    ))
}

/// Delete at a paragraph's end while Suggesting: marks the paragraph mark as a
/// tracked deletion. The author's own pending break is removed outright; a
/// break another reviewer inserted is refused; a mark already suggested for
/// deletion only moves the caret (empty `operations`).
pub fn suggest_delete_paragraph_mark(
    document: &Document,
    ids: &mut ReviewIds<'_>,
    node: &str,
    author: Option<String>,
    date: Option<String>,
) -> Result<Lowered, String> {
    validate_authored_revision_author(author.as_deref())?;
    let node = NodeId::from_str(node).map_err(|_| "invalid node".to_string())?;
    let current =
        paragraph_properties(document, node).ok_or_else(|| "paragraph not found".to_string())?;
    let caret = Pos::new(node, paragraph_text(document, node).len() as u32);
    let Some(next) = adjacent_next_paragraph(document, node) else {
        return Err(refused!(
            "review.no-following-paragraph",
            "There is no following paragraph to join this one to."
        )
        .to_string());
    };
    let operation = match &current.mark_revision {
        Some(mark) if mark.kind == MarkRevisionKind::Insertion && mark.author == author => {
            let survivor = paragraph_properties(document, next)
                .ok_or_else(|| "paragraph not found".to_string())?;
            Operation::JoinParagraphs {
                first: node,
                second: next,
                properties: Some(Box::new(survivor)),
            }
        }
        Some(mark) if mark.kind == MarkRevisionKind::Deletion => {
            return Ok(Lowered::new(Vec::new(), caret));
        }
        Some(_) => {
            return Err(
                "Another reviewer suggested this paragraph break; accept or reject it first"
                    .to_string(),
            );
        }
        None => {
            let mut deleted = current.clone();
            deleted.mark_revision = Some(Box::new(MarkRevision {
                kind: MarkRevisionKind::Deletion,
                author,
                date,
                revision_id: Some(ids.revisions.allocate()?),
            }));
            Operation::SetParagraphProperties {
                node,
                properties: Box::new(deleted),
            }
        }
    };
    Ok(Lowered::new(vec![operation], caret))
}

/// A tracked deletion (and, with `text`, a replacement) from inside paragraph A
/// to inside paragraph B, in the shape Word writes (Word-saved fixture RP040).
///
/// `start` and `end` are in document order and `nodes` are the paragraphs the
/// selection covers, in order (the caller resolves both from its selection).
#[allow(clippy::too_many_arguments)]
pub fn suggest_range_edit(
    document: &Document,
    ids: &mut ReviewIds<'_>,
    start: Pos,
    end: Pos,
    nodes: &[NodeId],
    text: Option<&str>,
    author: Option<String>,
    date: Option<String>,
) -> Result<Lowered, String> {
    validate_authored_revision_author(author.as_deref())?;
    if nodes.len() < 2 {
        return Err(refused!(
            "review.cross-paragraph-range-required",
            "This suggestion needs a selection that crosses a paragraph break."
        )
        .to_string());
    }
    for pair in nodes.windows(2) {
        if adjacent_next_paragraph(document, pair[0]) != Some(pair[1]) {
            return Err(
                "A tracked deletion cannot cross a table or leave its container".to_string(),
            );
        }
    }
    let last = nodes.len() - 1;
    // One index for the whole cross-paragraph suggestion. O(notes).
    let notes = NoteAnchorLengths::of(document);
    let mut body = Vec::new();
    for node in nodes {
        body.extend(review_paragraph_body(document, *node)?);
    }
    for (index, node) in nodes.iter().enumerate() {
        let from = if index == 0 { start.offset } else { 0 };
        let to = if index == last {
            end.offset
        } else {
            paragraph_text(document, *node).len() as u32
        };
        if to > from {
            suggest_deletion_into_body(
                ids,
                &notes,
                &mut body,
                *node,
                from,
                to,
                author.clone(),
                date.clone(),
            )?;
        }
    }
    if let Some(text) = text {
        let insertion = ids.node()?;
        let run = ids.node()?;
        let revision_id = ids.revisions.allocate()?;
        if !insert_review_revision(
            &notes,
            &mut body,
            start.node,
            start.offset,
            Revision {
                id: insertion,
                kind: RevisionKind::Insertion,
                author: author.clone(),
                date: date.clone(),
                revision_id: Some(revision_id),
                editor_group: None,
                inlines: vec![InlineNode::Run(Run {
                    id: run,
                    properties: RunProperties::default().into(),
                    text: text.to_owned(),
                })],
            },
            ids.nodes,
        ) {
            return Err("suggested replacement insertion failed".to_string());
        }
    }
    let mut ops = Vec::new();
    match update_review_operation(document, &body, None) {
        Ok(operation) => ops.push(operation),
        Err(message) if message == "review command made no change" => {}
        Err(message) => return Err(message),
    }
    // A suggestion over a Select-All range is bounded by the document, and
    // `paragraph_properties` walks it per node — one index, one walk.
    let by_id = ParagraphIndex::build(document);
    let first = by_id
        .properties(nodes[0])
        .ok_or_else(|| "paragraph not found".to_string())?;
    let partial_first = start.offset > 0;
    let mut decided: BTreeMap<NodeId, ParagraphProperties> = BTreeMap::new();
    let mut merges = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        let current = by_id
            .properties(*node)
            .ok_or_else(|| "paragraph not found".to_string())?;
        let mut next = current.clone();
        if index < last {
            match &current.mark_revision {
                Some(mark) if mark.kind == MarkRevisionKind::Insertion && mark.author == author => {
                    merges.push(index);
                }
                Some(mark) if mark.kind == MarkRevisionKind::Deletion => {}
                Some(_) => {
                    return Err(refused!(
                        "review.paragraph-break-other-author",
                        "A paragraph break in this range is another reviewer's suggestion. \
                         Accept or reject that first."
                    )
                    .to_string());
                }
                None => {
                    next.mark_revision = Some(Box::new(MarkRevision {
                        kind: MarkRevisionKind::Deletion,
                        author: author.clone(),
                        date: date.clone(),
                        revision_id: Some(ids.revisions.allocate()?),
                    }));
                }
            }
        }
        if index > 0 && partial_first {
            let (prior, revision_id) = match &current.prop_change {
                Some(change) => (change.prior.as_ref().clone(), change.revision_id.clone()),
                None => (paragraph_base(&current), None),
            };
            let mut shaped = paragraph_base(&first);
            shaped.mark_run = next.mark_run.clone();
            shaped.mark_revision = next.mark_revision.clone();
            shaped.section_break = next.section_break;
            if paragraph_base(&shaped) != prior {
                let revision_id = match revision_id {
                    Some(id) => Some(id),
                    None => Some(ids.revisions.allocate()?),
                };
                shaped.prop_change = Some(Box::new(PropChange {
                    author: author.clone(),
                    date: date.clone(),
                    revision_id,
                    editor_group: None,
                    prior: Box::new(prior),
                }));
            }
            next = shaped;
        }
        if next != current {
            ops.push(Operation::SetParagraphProperties {
                node: *node,
                properties: Box::new(next.clone()),
            });
        }
        decided.insert(*node, next);
    }
    for index in merges.into_iter().rev() {
        let (first_node, second) = (nodes[index], nodes[index + 1]);
        let survivor = decided
            .get(&second)
            .cloned()
            .ok_or_else(|| "paragraph not found".to_string())?;
        decided.insert(first_node, survivor.clone());
        ops.push(Operation::JoinParagraphs {
            first: first_node,
            second,
            properties: Some(Box::new(survivor)),
        });
    }
    if ops.is_empty() {
        return Err("review command made no change".to_string());
    }
    let typed = text.map_or(0, |text| text.len() as u32);
    Ok(Lowered::new(
        ops,
        Pos::new(start.node, start.offset + typed),
    ))
}

/// A tracked character-formatting change (`w:rPrChange`) over a same-paragraph
/// range, in one formatting group.
#[allow(clippy::too_many_arguments)]
pub fn suggest_format(
    document: &Document,
    ids: &mut ReviewIds<'_>,
    node: &str,
    start: u32,
    end: u32,
    delta: FormatDelta,
    author: Option<String>,
    date: Option<String>,
) -> Result<Lowered, String> {
    // An empty `delta` is the caller's to refuse, with the same sentence: the
    // editor's wrapper refuses before parsing its arguments into one.
    if start >= end {
        return Err("suggested formatting requires a range and change".to_string());
    }
    validate_authored_revision_author(author.as_deref())?;
    let node = node_id(node)?;
    let group = RevisionGroup {
        id: ids.node()?,
        kind: RevisionGroupKind::Formatting,
    };
    let mut body = review_paragraph_body(document, node)?;
    let notes = NoteAnchorLengths::of(document);
    if !apply_review_format_change(
        &notes,
        &mut body,
        node,
        start,
        end,
        delta,
        author,
        date,
        group,
        ids.revisions,
        ids.nodes,
    )? {
        return Err("suggested formatting requires a top-level paragraph text range".to_string());
    }
    let operation = update_review_operation(document, &body, None)?;
    Ok(Lowered::new(vec![operation], Pos::new(node, start)))
}

/// Accepts or rejects one revision by id: an inline insertion or deletion, a
/// run formatting change, or a paragraph-level revision (`{paragraph}:mark` /
/// `{paragraph}:format`). Editor-authored grouped revisions and tracked moves
/// are refused here; they are decided by [`decide_revision_group`] and
/// [`decide_move_pair`].
pub fn decide_revision(
    document: &Document,
    revision: &str,
    accept: bool,
) -> Result<Lowered, String> {
    if let Some((paragraph, part)) = parse_paragraph_revision_id(revision) {
        return decide_paragraph_revision(document, paragraph, part, accept);
    }
    let id = node_id(revision)?;
    if let Some((node, start, end)) = find_review_format_anchor_all(document, id) {
        if find_review_format_change(document.body(), id)
            .and_then(|change| change.editor_group)
            .is_some()
        {
            return Err(
                "editor-authored grouped formatting must be decided atomically".to_string(),
            );
        }
        let mut body = review_paragraph_body(document, node)?;
        if !decide_review_format_change(&mut body, id, accept) {
            return Err("formatting change not found".to_string());
        }
        let operation = update_review_operation(document, &body, None)?;
        return Ok(Lowered::new(
            vec![operation],
            Pos::new(node, if accept { end } else { start }),
        ));
    }
    let Some((node, start, end, kind)) = find_review_revision_anchor_all(document, id) else {
        return Err("revision not found".to_string());
    };
    if matches!(kind, RevisionKind::MoveFrom | RevisionKind::MoveTo) {
        return Err(
            "tracked moves must be accepted or rejected as one source/destination pair".to_string(),
        );
    }
    if find_review_revision(document.body(), id)
        .and_then(|revision| revision.editor_group)
        .is_some()
    {
        return Err("editor-authored grouped revisions must be decided atomically".to_string());
    }
    let mut body = review_paragraph_body(document, node)?;
    if !decide_review_revision(&mut body, id, accept) {
        return Err("revision not found".to_string());
    }
    let keep = matches!(kind, RevisionKind::Insertion | RevisionKind::MoveTo) == accept;
    let operation = update_review_operation(document, &body, None)?;
    Ok(Lowered::new(
        vec![operation],
        Pos::new(node, if keep { end } else { start }),
    ))
}

/// Accepts or rejects one paragraph-level revision (docs/108 Decision 2).
fn decide_paragraph_revision(
    document: &Document,
    paragraph: &str,
    part: ParagraphRevisionPart,
    accept: bool,
) -> Result<Lowered, String> {
    let node = node_id(paragraph)?;
    let Some(current) = find_paragraph_any(document, node) else {
        return Err("revision not found".to_string());
    };
    let properties = current.properties.get().clone();
    let end = paragraph_text(document, node).len() as u32;
    let ops = match part {
        ParagraphRevisionPart::Format => {
            let Some(change) = &properties.prop_change else {
                return Err("revision not found".to_string());
            };
            let mut decided = if accept {
                properties.clone()
            } else {
                paragraph_properties_before_change(&properties, change.prior.as_ref())
            };
            decided.prop_change = None;
            vec![Operation::SetParagraphProperties {
                node,
                properties: Box::new(decided),
            }]
        }
        ParagraphRevisionPart::Mark => {
            let Some(mark) = &properties.mark_revision else {
                return Err("revision not found".to_string());
            };
            let stays_separate = matches!(mark.kind, MarkRevisionKind::Insertion) == accept;
            match (stays_separate, adjacent_next_paragraph(document, node)) {
                (false, Some(next)) => {
                    let survivor = find_paragraph_any(document, next)
                        .map(|paragraph| paragraph.properties.get().clone())
                        .ok_or_else(|| "revision not found".to_string())?;
                    vec![Operation::JoinParagraphs {
                        first: node,
                        second: next,
                        properties: Some(Box::new(survivor)),
                    }]
                }
                // A mark with nothing after it cannot merge. Word never writes
                // that shape; clearing the revision honours what can be honoured
                // and deletes nothing the user can see (docs/108 Decision 2).
                _ => {
                    let mut cleared = properties.clone();
                    cleared.mark_revision = None;
                    vec![Operation::SetParagraphProperties {
                        node,
                        properties: Box::new(cleared),
                    }]
                }
            }
        }
    };
    Ok(Lowered::new(ops, Pos::new(node, end)))
}

/// Accepts or rejects one tracked move as an atomic source/destination pair,
/// named by its two range-start markers.
pub fn decide_move_pair(
    document: &Document,
    from_start: &str,
    to_start: &str,
    accept: bool,
) -> Result<Lowered, String> {
    let from_start = node_id(from_start)?;
    let to_start = node_id(to_start)?;
    let pairs = collect_review_move_pairs_all(document);
    let pair = pairs
        .iter()
        .find(|pair| pair.from.start == from_start && pair.to.start == to_start)
        .ok_or_else(|| "complete tracked move pair not found".to_string())?;
    let target_ids = if accept {
        &pair.to.revisions
    } else {
        &pair.from.revisions
    };
    let Some(target) = target_ids.first().copied() else {
        return Err("tracked move pair has no surviving range".to_string());
    };
    let Some((node, mut offset, _, _)) = find_review_revision_anchor_all(document, target) else {
        return Err("tracked move pair anchor not found".to_string());
    };
    let removed_ids = if accept {
        &pair.from.revisions
    } else {
        &pair.to.revisions
    };
    for id in removed_ids {
        if let Some((removed_node, start, end, _)) = find_review_revision_anchor_all(document, *id)
            && removed_node == node
            && start < offset
        {
            offset = offset.saturating_sub(end.saturating_sub(start));
        }
    }
    let mut body = document.body().to_vec();
    for id in pair
        .from
        .revisions
        .iter()
        .chain(pair.to.revisions.iter())
        .copied()
    {
        if !decide_review_revision(&mut body, id, accept) {
            return Err("tracked move pair is incomplete".to_string());
        }
    }
    let marker_ids = BTreeSet::from([pair.from.start, pair.from.end, pair.to.start, pair.to.end]);
    remove_review_move_markers(&mut body, &marker_ids);
    let operation = update_review_operation(document, &body, None)?;
    Ok(Lowered::new(vec![operation], Pos::new(node, offset)))
}

/// Accepts or rejects every editor-authored revision in one logical group.
pub fn decide_revision_group(
    document: &Document,
    group: &str,
    accept: bool,
) -> Result<Lowered, String> {
    let group = node_id(group)?;
    let validated =
        validate_review_group(&NoteAnchorLengths::of(document), document.body(), group)?;
    let mut body = Vec::new();
    for node in &validated.nodes {
        body.extend(review_paragraph_body(document, *node)?);
    }
    for target in validated.targets {
        let decided = match target {
            ReviewDecisionTarget::Revision(id) => decide_review_revision(&mut body, id, accept),
            ReviewDecisionTarget::RunFormatting(id) => {
                decide_review_format_change(&mut body, id, accept)
            }
        };
        if !decided {
            return Err("revision group is incomplete".to_string());
        }
    }
    let operation = update_review_operation(document, &body, None)?;
    Ok(Lowered::new(
        vec![operation],
        Pos::new(validated.node, validated.start),
    ))
}

/// Accepts or rejects every tracked change on every surface.
pub fn decide_all_revisions(document: &Document, accept: bool) -> Result<Lowered, String> {
    let mut ids = Vec::new();
    collect_review_revision_ids_all(document, &mut ids);
    let mut format_ids = Vec::new();
    collect_review_format_ids_all(document, &mut format_ids);
    let paragraph_revisions = collect_paragraph_revisions_all(document);
    if ids.is_empty() && format_ids.is_empty() && paragraph_revisions.is_empty() {
        return Err(refused!(
            "review.no-revisions",
            "This document has no tracked changes."
        )
        .to_string());
    }
    let pairs = collect_review_move_pairs_all(document);
    let paired_move_ids: BTreeSet<NodeId> = pairs
        .iter()
        .flat_map(|pair| {
            pair.from
                .revisions
                .iter()
                .chain(pair.to.revisions.iter())
                .copied()
        })
        .collect();
    for id in &ids {
        if let Some((_, _, _, kind)) = find_review_revision_anchor_all(document, *id)
            && matches!(kind, RevisionKind::MoveFrom | RevisionKind::MoveTo)
            && !paired_move_ids.contains(id)
        {
            return Err(
                "cannot decide all changes while an incomplete tracked move is present".to_string(),
            );
        }
    }
    let mut editor_groups = BTreeSet::new();
    collect_review_editor_group_ids(document.body(), &mut editor_groups);
    // One index for every group validated, not one per group.
    let notes = NoteAnchorLengths::of(document);
    for group in editor_groups {
        validate_review_group(&notes, document.body(), group)?;
    }
    let marker_ids: BTreeSet<NodeId> = pairs
        .iter()
        .flat_map(|pair| [pair.from.start, pair.from.end, pair.to.start, pair.to.end])
        .collect();
    // Every surface, not the body alone: a document with `w:ins` in a header
    // must not still carry tracked changes after Accept All.
    let mut surfaces = review_surfaces(document);
    for blocks in &mut surfaces {
        decide_all_review_revisions(blocks, accept);
        if !marker_ids.is_empty() {
            remove_review_move_markers(blocks, &marker_ids);
        }
    }
    let mut ops = Vec::new();
    if !ids.is_empty() || !format_ids.is_empty() {
        ops.push(update_review_operation_across(document, &surfaces, None)?);
    }
    ops.extend(paragraph_decision_ops(
        document,
        &paragraph_revisions,
        accept,
    ));
    Ok(Lowered::new(ops, Pos::new(document.id(), 0)))
}

/// A tracked character-formatting change over several same-paragraph ranges
/// (a selection that crosses paragraphs), in ONE formatting group so it is
/// decided as one. `ranges` are `(paragraph, start, end)` in document order;
/// the caret lands on `caret`.
pub fn suggest_format_ranges(
    document: &Document,
    ids: &mut ReviewIds<'_>,
    ranges: &[(NodeId, u32, u32)],
    delta: &FormatDelta,
    author: Option<String>,
    date: Option<String>,
    caret: Pos,
) -> Result<Lowered, String> {
    validate_authored_revision_author(author.as_deref())?;
    if ranges.is_empty() {
        return Err("tracked character formatting requires a range".into());
    }
    let group = RevisionGroup {
        id: ids.node()?,
        kind: RevisionGroupKind::Formatting,
    };
    let notes = NoteAnchorLengths::of(document);
    let mut ops = Vec::new();
    for &(node, start, end) in ranges {
        let mut body = review_paragraph_body(document, node)?;
        if !apply_review_format_change(
            &notes,
            &mut body,
            node,
            start,
            end,
            delta.clone(),
            author.clone(),
            date.clone(),
            group,
            ids.revisions,
            ids.nodes,
        )? {
            return Err("tracked formatting requires paragraph text".into());
        }
        ops.push(update_review_operation(document, &body, None)?);
    }
    Ok(Lowered::new(ops, caret))
}

/// One compared change, recorded into a review body as tracked revisions: the
/// range `start..end` of `node` wrapped as a `marked` revision (an insertion the
/// other side lacks), and `removed` text the other side had, re-inserted at
/// `start` as a tracked deletion. When both are present they form one
/// replacement group, decided as one card.
///
/// Returns the loss code when the change could not be recorded over ordinary
/// paragraph text (`notParagraphText`), so the caller reports it instead of
/// dropping it silently.
#[allow(clippy::too_many_arguments)]
pub fn record_compared_change(
    ids: &mut ReviewIds<'_>,
    notes: &NoteAnchorLengths,
    body: &mut [BlockNode],
    node: NodeId,
    start: u32,
    end: u32,
    marked: Option<RevisionKind>,
    removed: Option<String>,
    author: &str,
    date: &Option<String>,
) -> Result<Option<&'static str>, String> {
    let group = match (marked, removed.as_ref()) {
        (Some(_), Some(_)) => Some(RevisionGroup {
            id: ids.node()?,
            kind: RevisionGroupKind::Replacement,
        }),
        _ => None,
    };
    if let Some(kind) = marked {
        let revision = Revision {
            id: ids.node()?,
            kind,
            author: Some(author.to_owned()),
            date: date.clone(),
            revision_id: Some(ids.revisions.allocate()?),
            editor_group: group,
            inlines: Vec::new(),
        };
        // Misnamed by history: it wraps a range in whatever revision it is
        // given, which is how a comparison's INSERTION is marked with it.
        if !wrap_review_deletion(notes, body, node, start, end, revision, ids.nodes) {
            return Ok(Some("notParagraphText"));
        }
    }
    if let Some(text) = removed {
        let run = ids.node()?;
        let revision = Revision {
            id: ids.node()?,
            kind: RevisionKind::Deletion,
            author: Some(author.to_owned()),
            date: date.clone(),
            revision_id: Some(ids.revisions.allocate()?),
            editor_group: group,
            inlines: vec![InlineNode::Run(Run {
                id: run,
                properties: RunProperties::default().into(),
                text,
            })],
        };
        if !insert_review_revision(notes, body, node, start, revision, ids.nodes) {
            return Ok(Some("notParagraphText"));
        }
    }
    Ok(None)
}
