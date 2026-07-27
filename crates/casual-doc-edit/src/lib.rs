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
