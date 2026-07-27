//! The closed editing op set on `v1::Document` (doc 59).
//!
//! Editing mutates the **same model that is rendered** (`v1::Document`), not the
//! minimal v0 model the Phase-0 transaction layer edits. This crate is the choke
//! point (doc 45 I1): [`apply`] takes an [`Operation`] (the closed set, I2),
//! mutates the document in place, and returns the **inverse** operation so undo/
//! redo are just re-applying inverses.
//!
//! Positions are the **layout anchor space** shared with hit-testing (doc 58 §3):
//! `(NodeId paragraph, u32 byte_offset)`, a node-relative UTF-8 byte offset into
//! the paragraph's shaped plain text (`node_plain_text`). No grapheme/affinity
//! model, no byte↔grapheme bridge — hit-testing, selection, and editing all speak
//! byte offsets.
//!
//! **This is slice 1: text `InsertText` + `DeleteText` on top-level runs.**
//! `SplitParagraph`/`JoinParagraphs`, nested-wrapper edits, and object/table ops
//! are additive follow-ups (doc 59 staging).

use casual_doc_model::NodeId;
use casual_doc_model::v1::{BlockNode, Document, InlineNode, Paragraph, Run, RunProperties};

/// A caret position: a paragraph node and a node-relative UTF-8 byte offset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pos {
    /// The paragraph node.
    pub node: NodeId,
    /// UTF-8 byte offset into the paragraph's shaped plain text.
    pub offset: u32,
}

impl Pos {
    /// A position at `offset` within `node`.
    #[must_use]
    pub const fn new(node: NodeId, offset: u32) -> Self {
        Self { node, offset }
    }
}

/// A half-open range `[start, end)` within one paragraph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Range {
    /// Inclusive start.
    pub start: Pos,
    /// Exclusive end.
    pub end: Pos,
}

/// The closed editing op set (I2). Slice 1 carries the two text ops; structural
/// and object ops are additive variants (doc 59).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Operation {
    /// Insert `text` at a caret position.
    InsertText {
        /// Where to insert.
        at: Pos,
        /// The text inserted.
        text: String,
    },
    /// Delete a non-empty range within one paragraph.
    DeleteText {
        /// The range removed.
        range: Range,
    },
    /// Split a paragraph at `at`, moving the trailing content into a new
    /// paragraph with id `new_id` inserted immediately after (Enter).
    SplitParagraph {
        /// The split boundary in the original paragraph.
        at: Pos,
        /// The id of the new trailing paragraph.
        new_id: NodeId,
    },
    /// Join `second` (which must immediately follow `first` in the same
    /// container) into the end of `first`, removing `second` (Backspace at a
    /// paragraph start). `first` keeps its own paragraph properties.
    JoinParagraphs {
        /// The paragraph that receives the content.
        first: NodeId,
        /// The paragraph appended and removed.
        second: NodeId,
    },
}

/// Why an edit could not be applied. No partial mutation ever occurs: an op
/// validates before it mutates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditError {
    /// The target paragraph node does not exist.
    NodeNotFound,
    /// The offset is past the paragraph's text length.
    OffsetOutOfRange,
    /// The offset does not fall on a UTF-8 character boundary.
    NotCharBoundary,
    /// An empty insert, or an empty/inverted delete range.
    EmptyEdit,
    /// The range spans more than one paragraph (slice 1 is single-paragraph).
    CrossParagraph,
    /// The position is not inside an editable top-level run (e.g. inside a
    /// hyperlink/field wrapper or a tab). Slice-1 limitation.
    Unsupported,
    /// The node-id space is exhausted.
    IdExhausted,
}

/// Applies `op` to `doc`, returning the inverse operation (for undo). `ids` mints
/// new run identities when an edit must create a run (e.g. typing into an empty
/// paragraph). On `Err`, `doc` is unchanged.
pub fn apply(
    doc: &mut Document,
    ids: &mut dyn RunIds,
    op: &Operation,
) -> Result<Operation, EditError> {
    match op {
        Operation::InsertText { at, text } => {
            if text.is_empty() {
                return Err(EditError::EmptyEdit);
            }
            let para =
                find_paragraph_mut(doc.body_mut(), at.node).ok_or(EditError::NodeNotFound)?;
            if at.offset > paragraph_text_len(para) {
                return Err(EditError::OffsetOutOfRange);
            }
            insert_text(&mut para.inlines, at.offset, text, ids)?;
            let end = Pos::new(at.node, at.offset + text.len() as u32);
            Ok(Operation::DeleteText {
                range: Range { start: *at, end },
            })
        }
        Operation::DeleteText { range } => {
            if range.start.node != range.end.node {
                return Err(EditError::CrossParagraph);
            }
            if range.end.offset <= range.start.offset {
                return Err(EditError::EmptyEdit);
            }
            let para = find_paragraph_mut(doc.body_mut(), range.start.node)
                .ok_or(EditError::NodeNotFound)?;
            let removed = delete_text(&mut para.inlines, range.start.offset, range.end.offset)?;
            Ok(Operation::InsertText {
                at: range.start,
                text: removed,
            })
        }
        Operation::SplitParagraph { at, new_id } => {
            if !split_paragraph(doc.body_mut(), at.node, at.offset, *new_id, ids)? {
                return Err(EditError::NodeNotFound);
            }
            Ok(Operation::JoinParagraphs {
                first: at.node,
                second: *new_id,
            })
        }
        Operation::JoinParagraphs { first, second } => {
            match join_paragraphs(doc.body_mut(), *first, *second)? {
                Some(split_at) => Ok(Operation::SplitParagraph {
                    at: Pos::new(*first, split_at),
                    new_id: *second,
                }),
                None => Err(EditError::NodeNotFound),
            }
        }
    }
}

/// A source of fresh run identities. Backed by
/// [`IdGenerator`](casual_doc_model::IdGenerator) in practice; a trait so the
/// edit crate does not dictate the id-allocation policy.
pub trait RunIds {
    /// Returns a fresh, unique node id, or `None` if the space is exhausted.
    fn next(&mut self) -> Option<NodeId>;
}

impl RunIds for casual_doc_model::IdGenerator {
    fn next(&mut self) -> Option<NodeId> {
        self.next_id().ok()
    }
}

/// The byte range of one top-level [`InlineNode::Run`] in a paragraph's text.
struct RunSeg {
    /// Index into the paragraph's `inlines`.
    idx: usize,
    /// Byte offset of the run's first byte in the concatenated text.
    start: u32,
    /// Byte offset one past the run's last byte.
    end: u32,
}

/// The text bytes a single inline contributes — identical to
/// `node_plain_text`'s accounting, so offsets align with hit-testing.
fn inline_text_len(inline: &InlineNode) -> u32 {
    match inline {
        InlineNode::Run(run) => run.text.len() as u32,
        InlineNode::Tab(_) => 1,
        InlineNode::Symbol(symbol) => {
            char::from_u32(symbol.char).map_or(0, |c| c.len_utf8() as u32)
        }
        InlineNode::Hyperlink(hyperlink) => nested_len(&hyperlink.inlines),
        InlineNode::Revision(revision) => nested_len(&revision.inlines),
        InlineNode::Sdt(sdt) => nested_len(&sdt.inlines),
        _ => 0,
    }
}

fn nested_len(inlines: &[InlineNode]) -> u32 {
    inlines.iter().map(inline_text_len).sum()
}

/// The paragraph's total shaped-text byte length.
fn paragraph_text_len(para: &Paragraph) -> u32 {
    para.inlines.iter().map(inline_text_len).sum()
}

/// The byte ranges of the paragraph's top-level runs (the editable segments).
fn run_segments(inlines: &[InlineNode]) -> Vec<RunSeg> {
    let mut segs = Vec::new();
    let mut cum = 0u32;
    for (idx, inline) in inlines.iter().enumerate() {
        let len = inline_text_len(inline);
        if matches!(inline, InlineNode::Run(_)) {
            segs.push(RunSeg {
                idx,
                start: cum,
                end: cum + len,
            });
        }
        cum += len;
    }
    segs
}

/// Inserts `text` at `offset` into a paragraph's inlines, splicing into the run
/// the offset lands in (or the nearest run, or a new run for an empty paragraph).
fn insert_text(
    inlines: &mut Vec<InlineNode>,
    offset: u32,
    text: &str,
    ids: &mut dyn RunIds,
) -> Result<(), EditError> {
    let segs = run_segments(inlines);

    // The run whose range contains the offset (interior or either boundary).
    if let Some(seg) = segs.iter().find(|s| offset >= s.start && offset <= s.end) {
        let local = (offset - seg.start) as usize;
        if let InlineNode::Run(run) = &mut inlines[seg.idx] {
            if !run.text.is_char_boundary(local) {
                return Err(EditError::NotCharBoundary);
            }
            run.text.insert_str(local, text);
            return Ok(());
        }
    }
    // Offset sits in a non-run gap: append to the nearest preceding run…
    if let Some(seg) = segs.iter().rev().find(|s| s.end <= offset)
        && let InlineNode::Run(run) = &mut inlines[seg.idx]
    {
        run.text.push_str(text);
        return Ok(());
    }
    // …else prepend to the nearest following run…
    if let Some(seg) = segs.iter().find(|s| s.start >= offset)
        && let InlineNode::Run(run) = &mut inlines[seg.idx]
    {
        run.text.insert_str(0, text);
        return Ok(());
    }
    // …else the paragraph has no runs: create one at the front.
    let id = ids.next().ok_or(EditError::IdExhausted)?;
    inlines.insert(
        0,
        InlineNode::Run(Run {
            id,
            properties: RunProperties::default(),
            text: text.to_string(),
        }),
    );
    Ok(())
}

/// Deletes `[start, end)` when it lies within a single top-level run, returning
/// the removed text (for the inverse). Cross-run / non-run ranges are a slice-2
/// concern and report `Unsupported` rather than corrupting.
fn delete_text(inlines: &mut Vec<InlineNode>, start: u32, end: u32) -> Result<String, EditError> {
    let segs = run_segments(inlines);
    let seg = segs
        .iter()
        .find(|s| start >= s.start && end <= s.end)
        .ok_or(EditError::Unsupported)?;
    let (from, to) = ((start - seg.start) as usize, (end - seg.start) as usize);
    let idx = seg.idx;

    let InlineNode::Run(run) = &mut inlines[idx] else {
        return Err(EditError::Unsupported);
    };
    if !run.text.is_char_boundary(from) || !run.text.is_char_boundary(to) {
        return Err(EditError::NotCharBoundary);
    }
    let removed = run.text[from..to].to_string();
    run.text.replace_range(from..to, "");
    // A run's text is non-empty by invariant; drop it if the edit emptied it.
    if run.text.is_empty() {
        inlines.remove(idx);
    }
    Ok(removed)
}

/// Splits the paragraph `id` at byte `offset` into two, inserting the trailing
/// half as a new paragraph `new_id` immediately after. Recurses into tables and
/// content controls to find the paragraph's container. Returns whether it was
/// found and split.
fn split_paragraph(
    blocks: &mut Vec<BlockNode>,
    id: NodeId,
    offset: u32,
    new_id: NodeId,
    ids: &mut dyn RunIds,
) -> Result<bool, EditError> {
    if let Some(index) = blocks
        .iter()
        .position(|b| matches!(b, BlockNode::Paragraph(p) if p.id == id))
    {
        let BlockNode::Paragraph(para) = &mut blocks[index] else {
            unreachable!("index selected a paragraph");
        };
        if offset > paragraph_text_len(para) {
            return Err(EditError::OffsetOutOfRange);
        }
        let inlines = std::mem::take(&mut para.inlines);
        let (left, right) = split_inlines(inlines, offset, ids)?;
        let properties = para.properties.clone();
        para.inlines = left;
        blocks.insert(
            index + 1,
            BlockNode::Paragraph(Paragraph {
                id: new_id,
                properties,
                inlines: right,
            }),
        );
        return Ok(true);
    }
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        if split_paragraph(&mut cell.blocks, id, offset, new_id, ids)? {
                            return Ok(true);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if split_paragraph(&mut sdt.blocks, id, offset, new_id, ids)? {
                    return Ok(true);
                }
            }
            _ => {}
        }
    }
    Ok(false)
}

/// Joins `second` (which must immediately follow `first`) into `first`, removing
/// `second`. Returns the byte offset at which `first` ended before the join (the
/// inverse split point), or `None` if `first` was not found. `Err(Unsupported)`
/// if `second` is not the adjacent next paragraph.
fn join_paragraphs(
    blocks: &mut Vec<BlockNode>,
    first: NodeId,
    second: NodeId,
) -> Result<Option<u32>, EditError> {
    if let Some(i) = blocks
        .iter()
        .position(|b| matches!(b, BlockNode::Paragraph(p) if p.id == first))
    {
        if !matches!(blocks.get(i + 1), Some(BlockNode::Paragraph(p)) if p.id == second) {
            return Err(EditError::Unsupported);
        }
        let BlockNode::Paragraph(second_para) = blocks.remove(i + 1) else {
            unreachable!("checked it is a paragraph");
        };
        let BlockNode::Paragraph(first_para) = &mut blocks[i] else {
            unreachable!("position found a paragraph");
        };
        let split_at = paragraph_text_len(first_para);
        first_para.inlines.extend(second_para.inlines);
        return Ok(Some(split_at));
    }
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        if let Some(at) = join_paragraphs(&mut cell.blocks, first, second)? {
                            return Ok(Some(at));
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(at) = join_paragraphs(&mut sdt.blocks, first, second)? {
                    return Ok(Some(at));
                }
            }
            _ => {}
        }
    }
    Ok(None)
}

/// Splits a paragraph's inline content at byte `offset` into (left, right). A run
/// straddling the offset is split (the right half gets a fresh id). A non-run
/// inline straddling the offset (a wrapper) is a slice-2 limit → `Unsupported`;
/// at a boundary it goes wholly to one side.
fn split_inlines(
    inlines: Vec<InlineNode>,
    offset: u32,
    ids: &mut dyn RunIds,
) -> Result<(Vec<InlineNode>, Vec<InlineNode>), EditError> {
    let mut left = Vec::new();
    let mut right = Vec::new();
    let mut cum = 0u32;
    for inline in inlines {
        let len = inline_text_len(&inline);
        if cum >= offset {
            right.push(inline);
        } else if cum + len <= offset {
            left.push(inline);
        } else if let InlineNode::Run(run) = inline {
            let local = (offset - cum) as usize;
            if !run.text.is_char_boundary(local) {
                return Err(EditError::NotCharBoundary);
            }
            let (head, tail) = run.text.split_at(local);
            left.push(InlineNode::Run(Run {
                id: run.id,
                properties: run.properties.clone(),
                text: head.to_string(),
            }));
            let tail_id = ids.next().ok_or(EditError::IdExhausted)?;
            right.push(InlineNode::Run(Run {
                id: tail_id,
                properties: run.properties,
                text: tail.to_string(),
            }));
        } else {
            return Err(EditError::Unsupported);
        }
        cum += len;
    }
    Ok((left, right))
}

/// Finds the paragraph with `id`, recursing into table cells and block content
/// controls (document order), for in-place mutation.
fn find_paragraph_mut(blocks: &mut [BlockNode], id: NodeId) -> Option<&mut Paragraph> {
    for block in blocks {
        match block {
            BlockNode::Paragraph(p) if p.id == id => return Some(p),
            BlockNode::Paragraph(_) => {}
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        if let Some(p) = find_paragraph_mut(&mut cell.blocks, id) {
                            return Some(p);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(p) = find_paragraph_mut(&mut sdt.blocks, id) {
                    return Some(p);
                }
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use casual_doc_model::IdGenerator;
    use casual_doc_model::v1::{Definitions, ParagraphProperties};

    fn n(counter: u64) -> NodeId {
        NodeId::from_parts(7, counter).unwrap()
    }

    fn run(id: u64, text: &str) -> InlineNode {
        InlineNode::Run(Run {
            id: n(id),
            properties: RunProperties::default(),
            text: text.to_string(),
        })
    }

    fn para(id: u64, inlines: Vec<InlineNode>) -> BlockNode {
        BlockNode::Paragraph(Paragraph {
            id: n(id),
            properties: ParagraphProperties::default(),
            inlines,
        })
    }

    fn doc(paragraphs: Vec<BlockNode>) -> Document {
        Document::new(n(1000), paragraphs, Definitions::default()).expect("valid document")
    }

    /// The concatenated text of paragraph `id` (top-level runs), for assertions.
    fn text_of(document: &Document, id: NodeId) -> String {
        fn walk(blocks: &[BlockNode], id: NodeId) -> Option<String> {
            for block in blocks {
                match block {
                    BlockNode::Paragraph(p) if p.id == id => {
                        return Some(
                            p.inlines
                                .iter()
                                .filter_map(|i| match i {
                                    InlineNode::Run(r) => Some(r.text.clone()),
                                    _ => None,
                                })
                                .collect(),
                        );
                    }
                    BlockNode::Table(t) => {
                        for row in &t.rows {
                            for cell in &row.cells {
                                if let Some(s) = walk(&cell.blocks, id) {
                                    return Some(s);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        walk(document.body(), id).unwrap_or_default()
    }

    #[test]
    fn insert_splices_and_inverse_removes() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![run(3, "Helloworld")])]);
        let mut ids = IdGenerator::new(9);

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::InsertText {
                at: Pos::new(p, 5),
                text: " brave ".to_string(),
            },
        )
        .unwrap();
        assert_eq!(text_of(&d, p), "Hello brave world");
        assert_eq!(
            inverse,
            Operation::DeleteText {
                range: Range {
                    start: Pos::new(p, 5),
                    end: Pos::new(p, 12), // 5 + len(" brave ")
                },
            }
        );

        // Applying the inverse restores the original text (undo).
        apply(&mut d, &mut ids, &inverse).unwrap();
        assert_eq!(text_of(&d, p), "Helloworld");
    }

    #[test]
    fn delete_within_run_and_inverse_reinserts() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![run(3, "Hello world")])]);
        let mut ids = IdGenerator::new(9);

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::DeleteText {
                range: Range {
                    start: Pos::new(p, 5),
                    end: Pos::new(p, 11),
                },
            },
        )
        .unwrap();
        assert_eq!(text_of(&d, p), "Hello");
        assert_eq!(
            inverse,
            Operation::InsertText {
                at: Pos::new(p, 5),
                text: " world".to_string(),
            }
        );
        apply(&mut d, &mut ids, &inverse).unwrap();
        assert_eq!(text_of(&d, p), "Hello world");
    }

    #[test]
    fn split_and_join_are_inverses() {
        let p = n(2);
        let new = n(50);
        let mut d = doc(vec![para(2, vec![run(3, "HelloWorld")])]);
        let mut ids = IdGenerator::new(9);

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::SplitParagraph {
                at: Pos::new(p, 5),
                new_id: new,
            },
        )
        .unwrap();
        assert_eq!(d.body().len(), 2, "one paragraph became two");
        assert_eq!(text_of(&d, p), "Hello");
        assert_eq!(text_of(&d, new), "World");
        assert_eq!(
            inverse,
            Operation::JoinParagraphs {
                first: p,
                second: new
            }
        );

        // The inverse join restores a single paragraph with the joined text.
        apply(&mut d, &mut ids, &inverse).unwrap();
        assert_eq!(d.body().len(), 1);
        assert_eq!(text_of(&d, p), "HelloWorld");
    }

    #[test]
    fn split_at_start_leaves_an_empty_leading_paragraph() {
        let p = n(2);
        let new = n(50);
        let mut d = doc(vec![para(2, vec![run(3, "abc")])]);
        let mut ids = IdGenerator::new(9);
        apply(
            &mut d,
            &mut ids,
            &Operation::SplitParagraph {
                at: Pos::new(p, 0),
                new_id: new,
            },
        )
        .unwrap();
        assert_eq!(text_of(&d, p), "");
        assert_eq!(text_of(&d, new), "abc");
    }

    #[test]
    fn join_requires_the_second_to_be_adjacent() {
        let mut d = doc(vec![
            para(2, vec![run(3, "a")]),
            para(4, vec![run(5, "b")]),
            para(6, vec![run(7, "c")]),
        ]);
        let mut ids = IdGenerator::new(9);
        assert_eq!(
            apply(
                &mut d,
                &mut ids,
                &Operation::JoinParagraphs {
                    first: n(2),
                    second: n(6), // not adjacent to 2
                }
            ),
            Err(EditError::Unsupported)
        );
        assert_eq!(d.body().len(), 3, "no mutation on error");
    }

    #[test]
    fn typing_into_an_empty_paragraph_creates_a_run() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![])]);
        let mut ids = IdGenerator::new(9);
        apply(
            &mut d,
            &mut ids,
            &Operation::InsertText {
                at: Pos::new(p, 0),
                text: "hi".to_string(),
            },
        )
        .unwrap();
        assert_eq!(text_of(&d, p), "hi");
    }

    #[test]
    fn out_of_range_and_missing_node_are_errors_and_do_not_mutate() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![run(3, "abc")])]);
        let mut ids = IdGenerator::new(9);

        assert_eq!(
            apply(
                &mut d,
                &mut ids,
                &Operation::InsertText {
                    at: Pos::new(p, 99),
                    text: "x".into()
                }
            ),
            Err(EditError::OffsetOutOfRange)
        );
        assert_eq!(
            apply(
                &mut d,
                &mut ids,
                &Operation::InsertText {
                    at: Pos::new(n(404), 0),
                    text: "x".into()
                }
            ),
            Err(EditError::NodeNotFound)
        );
        // A cross-paragraph delete range is rejected.
        assert_eq!(
            apply(
                &mut d,
                &mut ids,
                &Operation::DeleteText {
                    range: Range {
                        start: Pos::new(p, 0),
                        end: Pos::new(n(3), 1)
                    }
                }
            ),
            Err(EditError::CrossParagraph)
        );
        assert_eq!(text_of(&d, p), "abc", "no mutation on error");
    }
}
