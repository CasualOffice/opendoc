// SPDX-License-Identifier: Apache-2.0

//! What a committed change did, read from the operation **and its inverse** together.
//!
//! This is the module the design turns on (doc 150 §2.3). A spreadsheet delete names its
//! victims by address, so the operation describes itself; a document delete names them by
//! identity, and the identities are only in the inverse. Invariant I2 — every operation
//! returns its inverse — was built for undo, and it is what lets everything here stay a
//! pure function of two operations with no document to consult.

use casual_doc_edit::{Operation, RunningRegion};
use casual_doc_model::NodeId;
// Separate `use` lines for the three definition tables an editing command may now
// introduce (`147`, ADR-005) — added apart from the shared sorted block so parallel lanes
// do not conflict in it.
use casual_doc_model::v1::AbstractNumberingId;
use casual_doc_model::v1::MediaId;
use casual_doc_model::v1::NumberingInstanceId;
use casual_doc_model::v1::{
    BlockNode, BookmarkId, FieldRangeId, HeaderFooterId, InlineNode, NoteId, NoteKind, SectionId,
    StyleId, Table,
};

use super::{Change, Container, MALFORMED_CHANGE, block_id};

/// A structural band: which container, where, how many, inserting or removing.
#[derive(Clone, Copy, Debug)]
pub(super) struct Band {
    pub(super) container: Container,
    pub(super) at: u32,
    pub(super) count: u32,
    pub(super) inserting: bool,
}

/// Bytes added to or removed from one paragraph's projected text.
#[derive(Clone, Copy, Debug)]
pub(super) enum TextEffect {
    Insert { node: NodeId, at: u32, len: u32 },
    Delete { node: NodeId, start: u32, end: u32 },
}

/// What a committed change did to the coordinate spaces other operations address.
#[derive(Clone, Copy, Debug)]
pub(super) enum Effect {
    /// Nothing another operation addresses moved.
    Inert,
    /// Bytes changed in one paragraph. A text change also changes that paragraph's inline
    /// membership at an index no operation records, which is what refusal U4 is about.
    Text(TextEffect),
    /// A paragraph was cut at `at`; the tail became `new_node`, inserted immediately after
    /// it in the same block container.
    Split {
        original: NodeId,
        new_node: NodeId,
        at: u32,
    },
    /// `second` was appended to `first` at `at` and removed from its block container.
    Join {
        first: NodeId,
        second: NodeId,
        at: u32,
    },
    /// Children inserted into or removed from one container at a recorded index.
    Band(Band),
    /// A zero-width inline was added to or removed from one or two paragraphs' inline
    /// lists at a position the operation expresses as a byte offset, not an index.
    InlineMembership {
        first: NodeId,
        second: Option<NodeId>,
    },
}

/// What `change` did, read from the operation and the inverse together.
///
/// The inverse is what makes this a pure function: a delete names its victims by identity
/// and the identities are only in the inverse. O(1) except for the payload walks in
/// [`removed_by`].
pub(super) fn effect_of(change: Change<'_>) -> Result<Effect, &'static str> {
    Ok(match (change.operation, change.inverse) {
        // The inserted length is read off the INVERSE, not off `text`: the edit crate
        // strips XML-forbidden characters at the choke point, so the bytes that landed can
        // be fewer than the bytes offered. Reading `text.len()` here would put transform
        // and `apply` on two different normal forms.
        (Operation::InsertText { at, .. }, Operation::DeleteText { range }) => {
            Effect::Text(TextEffect::Insert {
                node: at.node,
                at: at.offset,
                len: range.end.offset.saturating_sub(range.start.offset),
            })
        }
        (Operation::InsertText { .. }, _) => {
            return Err(MALFORMED_CHANGE);
        }
        (Operation::DeleteText { range }, _) => Effect::Text(TextEffect::Delete {
            node: range.start.node,
            start: range.start.offset,
            end: range.end.offset,
        }),
        (Operation::SplitParagraph { at, new_id, .. }, _) => Effect::Split {
            original: at.node,
            new_node: *new_id,
            at: at.offset,
        },
        // The join boundary is the former end of `first`, which only the inverse split
        // carries.
        (Operation::JoinParagraphs { first, second, .. }, Operation::SplitParagraph { at, .. }) => {
            Effect::Join {
                first: *first,
                second: *second,
                at: at.offset,
            }
        }
        (Operation::JoinParagraphs { .. }, _) => {
            return Err(MALFORMED_CHANGE);
        }
        (Operation::InsertField { at, field }, _) => Effect::Text(TextEffect::Insert {
            node: at.node,
            at: at.offset,
            len: casual_doc_edit::field_text_len(field),
        }),
        (Operation::RemoveField { .. }, Operation::InsertField { at, field }) => {
            let len = casual_doc_edit::field_text_len(field);
            Effect::Text(TextEffect::Delete {
                node: at.node,
                start: at.offset,
                end: at.offset.saturating_add(len),
            })
        }
        (Operation::RemoveField { .. }, _) => {
            return Err(MALFORMED_CHANGE);
        }

        // Sibling bands whose position the operation itself records.
        (Operation::InsertRow { table, index, .. }, _) => Effect::Band(Band {
            container: Container::Rows(*table),
            at: *index,
            count: 1,
            inserting: true,
        }),
        (Operation::DeleteRow { table, index }, _) => Effect::Band(Band {
            container: Container::Rows(*table),
            at: *index,
            count: 1,
            inserting: false,
        }),
        (Operation::InsertColumn { table, index, .. }, _) => Effect::Band(Band {
            container: Container::GridColumns(*table),
            at: *index,
            count: 1,
            inserting: true,
        }),
        (Operation::DeleteColumn { table, index }, _) => Effect::Band(Band {
            container: Container::GridColumns(*table),
            at: *index,
            count: 1,
            inserting: false,
        }),
        (
            Operation::InsertTable {
                container, index, ..
            },
            _,
        ) => Effect::Band(Band {
            container: Container::Blocks(*container),
            at: *index,
            count: 1,
            inserting: true,
        }),
        (
            Operation::InsertBlocks {
                container,
                index,
                blocks,
            },
            _,
        ) => Effect::Band(Band {
            container: Container::Blocks(*container),
            at: *index,
            count: u32::try_from(blocks.len()).unwrap_or(u32::MAX),
            inserting: true,
        }),
        (
            Operation::DeleteBlocks {
                container,
                index,
                count,
            },
            _,
        ) => Effect::Band(Band {
            container: Container::Blocks(*container),
            at: *index,
            count: *count,
            inserting: false,
        }),
        (Operation::InsertFieldRange { index, blocks, .. }, _) => Effect::Band(Band {
            container: Container::Blocks(None),
            at: *index,
            count: u32::try_from(blocks.len()).unwrap_or(u32::MAX),
            inserting: true,
        }),
        (Operation::InsertObjectNode { owner, index, .. }, _) => Effect::Band(Band {
            container: Container::Inlines(*owner),
            at: *index,
            count: 1,
            inserting: true,
        }),

        // Sibling bands whose position only the inverse records.
        (
            Operation::DeleteTable { .. },
            Operation::InsertTable {
                container, index, ..
            },
        ) => Effect::Band(Band {
            container: Container::Blocks(*container),
            at: *index,
            count: 1,
            inserting: false,
        }),
        (Operation::DeleteTable { .. }, _) => {
            return Err(MALFORMED_CHANGE);
        }
        (Operation::RemoveFieldRange { .. }, Operation::InsertFieldRange { index, blocks, .. }) => {
            Effect::Band(Band {
                container: Container::Blocks(None),
                at: *index,
                count: u32::try_from(blocks.len()).unwrap_or(u32::MAX),
                inserting: false,
            })
        }
        (Operation::RemoveFieldRange { .. }, _) => {
            return Err(MALFORMED_CHANGE);
        }
        (Operation::DeleteObject { .. }, Operation::InsertObjectNode { owner, index, .. }) => {
            Effect::Band(Band {
                container: Container::Inlines(*owner),
                at: *index,
                count: 1,
                inserting: false,
            })
        }
        (Operation::DeleteObject { .. }, _) => {
            return Err(MALFORMED_CHANGE);
        }

        // Inline membership changed at a byte offset. These inlines project ZERO bytes —
        // a drawing, a note reference and a bookmark marker are all zero-width in the
        // paragraph's text (`casual_doc_edit`'s own length rule) — so no offset moves and
        // only an inline-index operation in the same paragraph is affected.
        (Operation::InsertInlineObject { at, .. } | Operation::InsertNote { at, .. }, _) => {
            Effect::InlineMembership {
                first: at.node,
                second: None,
            }
        }
        (Operation::RemoveInlineObject { .. }, Operation::InsertInlineObject { at, .. })
        | (Operation::RemoveNote { .. }, Operation::InsertNote { at, .. }) => {
            Effect::InlineMembership {
                first: at.node,
                second: None,
            }
        }
        (Operation::RemoveInlineObject { .. } | Operation::RemoveNote { .. }, _) => {
            return Err(MALFORMED_CHANGE);
        }
        (Operation::CreateBookmark { start, end, .. }, _)
        | (Operation::DeleteBookmark { .. }, Operation::CreateBookmark { start, end, .. }) => {
            Effect::InlineMembership {
                first: start.node,
                second: Some(end.node),
            }
        }
        (Operation::DeleteBookmark { .. }, _) => {
            return Err(MALFORMED_CHANGE);
        }

        // `SetHyperlink` rebuilds a paragraph's inline tree around the range but changes no
        // byte offset, and its inverse is a `SetInlines` of the whole paragraph. The
        // membership change is what an inline-index operation has to notice.
        (Operation::SetHyperlink { range, .. }, _) => Effect::InlineMembership {
            first: range.start.node,
            second: None,
        },
        // Run splitting only: the projected text is unchanged and so is every offset.
        // Inline indices do move, which `InlineMembership` reports.
        (Operation::FormatText { range, .. } | Operation::ClearFormatting { range }, _) => {
            Effect::InlineMembership {
                first: range.start.node,
                second: None,
            }
        }
        (Operation::SetInlines { node, .. }, _) => Effect::InlineMembership {
            first: *node,
            second: None,
        },

        // Everything else moves nothing another operation addresses. Contention and
        // liveness still apply; those are separate steps.
        (
            Operation::SetParagraphProperties { .. }
            | Operation::SetExtent { .. }
            | Operation::SetGroupGeometry { .. }
            | Operation::SetAnchor { .. }
            | Operation::SetImageCrop { .. }
            | Operation::SetObjectDescr { .. }
            | Operation::SetTableCellProperties { .. }
            | Operation::SetTableProperties { .. }
            | Operation::ReplaceTable { .. }
            | Operation::SetCoreProperties { .. }
            | Operation::UpdateReviewState { .. }
            | Operation::SetSectionGeometry { .. }
            | Operation::SpliceSectionBoundary { .. }
            | Operation::SetStyleDefinition { .. }
            | Operation::SetAbstractNumbering { .. }
            | Operation::SetNumberingInstance { .. }
            | Operation::SetMediaReference { .. }
            // One registry row in or out. Nothing addresses a chart projection
            // positionally, and `anchor_key` reports that no operation depends on a chart
            // key, so a removal here destroys nothing another operation is anchored to —
            // which is why it is inert rather than a `Key` in `removed_by` below.
            | Operation::SetChartDefinition { .. }
            | Operation::RenameBookmark { .. }
            | Operation::CreateHeaderFooterBody { .. }
            | Operation::RemoveHeaderFooterBody { .. }
            | Operation::SetSectionRunningRef { .. }
            | Operation::SetSectionTitlePage { .. }
            | Operation::SetSectionWatermark { .. }
            | Operation::SetSectionLineNumbering { .. }
            | Operation::SetSectionPageNumbering { .. }
            | Operation::SetSectionVerticalAlignment { .. }
            | Operation::SetEvenAndOddHeaders { .. }
            // Inert: it moves no text, splits and joins nothing, and destroys no key —
            // it swaps one `Option` in the settings record (ADR-059).
            | Operation::SetDocumentProtection { .. }
            | Operation::SetShapeFill { .. }
            | Operation::SetShapeStroke { .. }
            | Operation::SetTextBoxBody { .. },
            _,
        ) => Effect::Inert,
    })
}

/// A registry key an operation can destroy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Key {
    Bookmark(BookmarkId),
    Style(StyleId),
    AbstractNumbering(AbstractNumberingId),
    NumberingInstance(NumberingInstanceId),
    Media(MediaId),
    Section(SectionId),
    Note(NoteKind, NoteId),
    HeaderFooter(RunningRegion, HeaderFooterId),
    FieldRange(FieldRangeId),
}

/// What a committed change destroyed.
#[derive(Clone, Debug, Default)]
pub(super) struct Removed {
    pub(super) nodes: Vec<NodeId>,
    pub(super) keys: Vec<Key>,
}

impl Removed {
    pub(super) fn holds(&self, node: NodeId) -> bool {
        self.nodes.contains(&node)
    }

    pub(super) fn holds_key(&self, key: Key) -> bool {
        self.keys.contains(&key)
    }
}

/// Every node and registry key `change` destroyed.
///
/// Read from the **inverse**, which is where a document delete keeps the identities it
/// removed. O(nodes the concurrent change carried) — bounded by that operation's own
/// payload, never by the document.
///
/// # The bound, stated
///
/// `DeleteText`'s general-path inverse is a whole-paragraph `SetInlines`, not a list of
/// victims, so an inline object removed by a range delete is not enumerated here. The
/// consequence is a *refused apply* on one replica rather than a divergence — the operation
/// that should have been tombstoned addresses a node inside the deleted span, so both
/// orders reach the same document either way — and it is the reason
/// `no_supported_pair_produces_an_apply_failure` exists rather than being assumed.
pub(super) fn removed_by(change: Change<'_>) -> Removed {
    let mut removed = Removed::default();
    match (change.operation, change.inverse) {
        (Operation::JoinParagraphs { second, .. }, _) => removed.nodes.push(*second),
        (Operation::DeleteBlocks { .. }, Operation::InsertBlocks { blocks, .. }) => {
            for block in blocks {
                collect_block(block, &mut removed.nodes);
            }
        }
        (Operation::DeleteTable { .. }, Operation::InsertTable { table, .. }) => {
            collect_table(table, &mut removed.nodes);
        }
        (Operation::DeleteRow { .. }, Operation::InsertRow { row, .. }) => {
            removed.nodes.push(row.id);
            for cell in &row.cells {
                removed.nodes.push(cell.id);
                for block in &cell.blocks {
                    collect_block(block, &mut removed.nodes);
                }
            }
        }
        (Operation::DeleteColumn { .. }, Operation::InsertColumn { cells, .. }) => {
            for cell in cells {
                removed.nodes.push(cell.id);
                for block in &cell.blocks {
                    collect_block(block, &mut removed.nodes);
                }
            }
        }
        (
            Operation::DeleteObject { .. } | Operation::RemoveInlineObject { .. },
            Operation::InsertObjectNode { node, .. },
        ) => collect_inline(node, &mut removed.nodes),
        (Operation::RemoveInlineObject { .. }, Operation::InsertInlineObject { node, .. }) => {
            collect_inline(node, &mut removed.nodes);
        }
        (Operation::RemoveField { .. }, Operation::InsertField { field, .. }) => {
            removed.nodes.push(field.id);
            for inline in &field.inlines {
                collect_inline(inline, &mut removed.nodes);
            }
        }
        (Operation::RemoveFieldRange { field }, Operation::InsertFieldRange { blocks, .. }) => {
            removed.keys.push(Key::FieldRange(*field));
            for block in blocks {
                collect_block(block, &mut removed.nodes);
            }
        }
        (
            Operation::RemoveNote {
                kind,
                note,
                reference_id,
            },
            _,
        ) => {
            removed.keys.push(Key::Note(*kind, *note));
            removed.nodes.push(*reference_id);
        }
        (
            Operation::DeleteBookmark { bookmark },
            Operation::CreateBookmark {
                start_id, end_id, ..
            },
        ) => {
            removed.keys.push(Key::Bookmark(*bookmark));
            removed.nodes.push(*start_id);
            removed.nodes.push(*end_id);
        }
        (Operation::DeleteBookmark { bookmark }, _) => removed.keys.push(Key::Bookmark(*bookmark)),
        (Operation::RemoveHeaderFooterBody { region, id }, _) => {
            removed.keys.push(Key::HeaderFooter(*region, *id));
        }
        (
            Operation::SpliceSectionBoundary {
                at: Some(section),
                boundary: None,
            },
            _,
        ) => removed.keys.push(Key::Section(*section)),
        (Operation::SetStyleDefinition { id, style: None }, _) => {
            removed.keys.push(Key::Style(*id));
        }
        // A definition removal destroys a key, exactly as a style removal does: an
        // operation that arrives naming it has nothing to name, and that is a tombstone
        // rather than a silent no-op.
        (
            Operation::SetAbstractNumbering {
                id,
                definition: None,
            },
            _,
        ) => {
            removed.keys.push(Key::AbstractNumbering(*id));
        }
        (Operation::SetNumberingInstance { id, instance: None }, _) => {
            removed.keys.push(Key::NumberingInstance(*id));
        }
        (
            Operation::SetMediaReference {
                id,
                reference: None,
            },
            _,
        ) => {
            removed.keys.push(Key::Media(*id));
        }
        // A whole-subtree rewrite destroys whatever the old tree held and the new one does
        // not. The inverse carries the old tree, which is what makes the difference
        // computable without the document.
        (Operation::SetInlines { inlines, .. }, Operation::SetInlines { inlines: old, .. }) => {
            let mut before = Vec::new();
            let mut after = Vec::new();
            for inline in old {
                collect_inline(inline, &mut before);
            }
            for inline in inlines {
                collect_inline(inline, &mut after);
            }
            removed.nodes = before
                .into_iter()
                .filter(|id| !after.contains(id))
                .collect();
        }
        (
            Operation::ReplaceTable { replacement, .. },
            Operation::ReplaceTable {
                replacement: old, ..
            },
        ) => {
            let mut before = Vec::new();
            let mut after = Vec::new();
            collect_table(old, &mut before);
            collect_table(replacement, &mut after);
            removed.nodes = before
                .into_iter()
                .filter(|id| !after.contains(id))
                .collect();
        }
        _ => {}
    }
    removed
}

pub(super) fn collect_block(block: &BlockNode, out: &mut Vec<NodeId>) {
    out.push(block_id(block));
    match block {
        BlockNode::Paragraph(paragraph) => {
            for inline in &paragraph.inlines {
                collect_inline(inline, out);
            }
        }
        BlockNode::Table(table) => collect_table(table, out),
        BlockNode::Sdt(sdt) => {
            for child in &sdt.blocks {
                collect_block(child, out);
            }
        }
        BlockNode::AltChunk(_) => {}
    }
}

pub(super) fn collect_table(table: &Table, out: &mut Vec<NodeId>) {
    out.push(table.id);
    for row in &table.rows {
        out.push(row.id);
        for cell in &row.cells {
            out.push(cell.id);
            for block in &cell.blocks {
                collect_block(block, out);
            }
        }
    }
}

/// Collects `inline`'s id and the ids of every inline or block nested inside it.
///
/// DrawingML group internals are deliberately not descended into: a shape removed with its
/// containing paragraph is not enumerated, which costs a tombstone *report* and never
/// costs convergence (see [`removed_by`]).
pub(super) fn collect_inline(inline: &InlineNode, out: &mut Vec<NodeId>) {
    out.push(inline.id());
    match inline {
        InlineNode::Hyperlink(node) => {
            for child in &node.inlines {
                collect_inline(child, out);
            }
        }
        InlineNode::Field(node) => {
            for child in &node.inlines {
                collect_inline(child, out);
            }
        }
        InlineNode::Revision(node) => {
            for child in &node.inlines {
                collect_inline(child, out);
            }
        }
        InlineNode::Sdt(node) => {
            for child in &node.inlines {
                collect_inline(child, out);
            }
        }
        InlineNode::TextBox(node) => {
            for block in &node.blocks {
                collect_block(block, out);
            }
        }
        InlineNode::Run(_)
        | InlineNode::Tab(_)
        | InlineNode::Break(_)
        | InlineNode::Drawing(_)
        | InlineNode::AnchoredDrawing(_)
        | InlineNode::EmbeddedObject(_)
        | InlineNode::Group(_)
        | InlineNode::NoteReference(_)
        | InlineNode::NoteNumberMark(_)
        | InlineNode::CommentReference(_)
        | InlineNode::CommentRangeStart(_)
        | InlineNode::CommentRangeEnd(_)
        | InlineNode::BookmarkStart(_)
        | InlineNode::BookmarkEnd(_)
        | InlineNode::FieldRangeStart(_)
        | InlineNode::FieldRangeEnd(_)
        | InlineNode::MoveRangeStart(_)
        | InlineNode::MoveRangeEnd(_)
        | InlineNode::Math(_)
        | InlineNode::Symbol(_)
        | InlineNode::HorizontalRule(_)
        | InlineNode::NoBreakHyphen(_)
        | InlineNode::SoftHyphen(_)
        | InlineNode::PositionalTab(_) => {}
    }
}
