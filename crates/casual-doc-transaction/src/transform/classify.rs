// SPDX-License-Identifier: Apache-2.0

//! Classification: which tier an operation belongs to, what it is called, and what
//! coordinates it carries — three matches over [`Operation`] with **no wildcard arm**, plus
//! the mirror that writes rebased coordinates back.
//!
//! Adding a 56th operation is a compile error in each of them, and each is a decision
//! someone has to take. That is `107` exit gate 2, enforced by the type system rather than
//! by a CI grep — and it is why the classification lives here rather than in 55 doc comments
//! in another crate, where nothing would check it.
//!
//! Four more exhaustive matches live next to the steps that read them: `anchors` and
//! `anchor_key` (liveness), `footprint` (contention) and
//! [`effect_of`](super::effect::effect_of) (what a change did).

use casual_doc_edit::{Operation, Pos};
use casual_doc_model::NodeId;

use super::EditRange;

// ---------------------------------------------------------------------------------------

/// The concurrency tier of an operation (doc 150 §5.2, `107` §3.1).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tier {
    /// Paragraph-local: offsets inside one named paragraph, and relocation across a
    /// concurrent split or join. The hot path, and the only place offsets collide.
    ParagraphLocal,
    /// Sibling-indexed: a position among one container's children.
    SiblingIndexed,
    /// Node-addressed: a liveness check and aspect contention, no arithmetic. Most of the
    /// operations, almost none of the traffic.
    NodeAddressed,
    /// Document-scope: last-writer-wins per registry key, in the settled order.
    DocumentScope,
}

/// The concurrency tier of `operation`.
///
/// Exhaustive by construction: adding an operation without classifying it does not
/// compile. O(1).
#[must_use]
pub const fn tier(operation: &Operation) -> Tier {
    match operation {
        Operation::InsertText { .. }
        | Operation::DeleteText { .. }
        | Operation::SplitParagraph { .. }
        | Operation::JoinParagraphs { .. }
        | Operation::FormatText { .. }
        | Operation::ClearFormatting { .. }
        | Operation::SetHyperlink { .. }
        | Operation::InsertInlineObject { .. }
        | Operation::InsertField { .. }
        | Operation::InsertNote { .. }
        | Operation::CreateBookmark { .. } => Tier::ParagraphLocal,

        Operation::InsertRow { .. }
        | Operation::DeleteRow { .. }
        | Operation::InsertColumn { .. }
        | Operation::DeleteColumn { .. }
        | Operation::InsertTable { .. }
        | Operation::InsertBlocks { .. }
        | Operation::DeleteBlocks { .. }
        | Operation::InsertObjectNode { .. }
        | Operation::InsertFieldRange { .. } => Tier::SiblingIndexed,

        Operation::SetInlines { .. }
        | Operation::SetParagraphProperties { .. }
        | Operation::DeleteTable { .. }
        | Operation::SetExtent { .. }
        | Operation::SetGroupGeometry { .. }
        | Operation::SetAnchor { .. }
        | Operation::SetImageCrop { .. }
        | Operation::SetObjectDescr { .. }
        | Operation::DeleteObject { .. }
        | Operation::RemoveInlineObject { .. }
        | Operation::SetTableCellProperties { .. }
        | Operation::SetTableProperties { .. }
        | Operation::ReplaceTable { .. }
        | Operation::UpdateReviewState { .. }
        | Operation::RemoveField { .. }
        | Operation::RemoveFieldRange { .. }
        | Operation::RemoveNote { .. }
        | Operation::SetShapeFill { .. }
        | Operation::SetShapeStroke { .. }
        | Operation::SetTextBoxBody { .. }
        | Operation::SetObjectLocks { .. } => Tier::NodeAddressed,

        Operation::SetCoreProperties { .. }
        | Operation::SetSectionGeometry { .. }
        | Operation::SpliceSectionBoundary { .. }
        | Operation::SetStyleDefinition { .. }
        | Operation::SetAbstractNumbering { .. }
        | Operation::SetNumberingInstance { .. }
        | Operation::SetMediaReference { .. }
        // A chart projection is a registry row keyed by `ChartId`, exactly like the media
        // reference beside it: two replicas writing the same key settle last-writer-wins,
        // and the key is minted from each replica's own reserved id block, so two of them
        // cannot collide by accident.
        | Operation::SetChartDefinition { .. }
        | Operation::DeleteBookmark { .. }
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
        | Operation::SetTrackRevisions { .. }
        // Document-global policy with no node and no registry key: the definition of
        // document scope (ADR-059).
        | Operation::SetDocumentProtection { .. } => Tier::DocumentScope,
    }
}

/// The operation's variant name, for refusals and tombstone reports.
///
/// Exhaustive by construction. O(1).
#[must_use]
pub const fn variant_name(operation: &Operation) -> &'static str {
    match operation {
        Operation::InsertText { .. } => "InsertText",
        Operation::DeleteText { .. } => "DeleteText",
        Operation::SplitParagraph { .. } => "SplitParagraph",
        Operation::JoinParagraphs { .. } => "JoinParagraphs",
        Operation::FormatText { .. } => "FormatText",
        Operation::ClearFormatting { .. } => "ClearFormatting",
        Operation::SetHyperlink { .. } => "SetHyperlink",
        Operation::SetInlines { .. } => "SetInlines",
        Operation::SetParagraphProperties { .. } => "SetParagraphProperties",
        Operation::InsertRow { .. } => "InsertRow",
        Operation::DeleteRow { .. } => "DeleteRow",
        Operation::InsertColumn { .. } => "InsertColumn",
        Operation::DeleteColumn { .. } => "DeleteColumn",
        Operation::DeleteTable { .. } => "DeleteTable",
        Operation::InsertTable { .. } => "InsertTable",
        Operation::InsertBlocks { .. } => "InsertBlocks",
        Operation::DeleteBlocks { .. } => "DeleteBlocks",
        Operation::SetExtent { .. } => "SetExtent",
        Operation::SetGroupGeometry { .. } => "SetGroupGeometry",
        Operation::SetAnchor { .. } => "SetAnchor",
        Operation::SetImageCrop { .. } => "SetImageCrop",
        Operation::SetObjectDescr { .. } => "SetObjectDescr",
        Operation::DeleteObject { .. } => "DeleteObject",
        Operation::InsertObjectNode { .. } => "InsertObjectNode",
        Operation::InsertInlineObject { .. } => "InsertInlineObject",
        Operation::RemoveInlineObject { .. } => "RemoveInlineObject",
        Operation::SetTableCellProperties { .. } => "SetTableCellProperties",
        Operation::SetTableProperties { .. } => "SetTableProperties",
        Operation::ReplaceTable { .. } => "ReplaceTable",
        Operation::SetCoreProperties { .. } => "SetCoreProperties",
        Operation::UpdateReviewState { .. } => "UpdateReviewState",
        Operation::SetSectionGeometry { .. } => "SetSectionGeometry",
        Operation::SpliceSectionBoundary { .. } => "SpliceSectionBoundary",
        Operation::SetStyleDefinition { .. } => "SetStyleDefinition",
        Operation::SetAbstractNumbering { .. } => "SetAbstractNumbering",
        Operation::SetNumberingInstance { .. } => "SetNumberingInstance",
        Operation::SetMediaReference { .. } => "SetMediaReference",
        Operation::SetChartDefinition { .. } => "SetChartDefinition",
        Operation::SetDocumentProtection { .. } => "SetDocumentProtection",
        Operation::CreateBookmark { .. } => "CreateBookmark",
        Operation::DeleteBookmark { .. } => "DeleteBookmark",
        Operation::RenameBookmark { .. } => "RenameBookmark",
        Operation::InsertField { .. } => "InsertField",
        Operation::RemoveField { .. } => "RemoveField",
        Operation::InsertFieldRange { .. } => "InsertFieldRange",
        Operation::RemoveFieldRange { .. } => "RemoveFieldRange",
        Operation::InsertNote { .. } => "InsertNote",
        Operation::RemoveNote { .. } => "RemoveNote",
        Operation::CreateHeaderFooterBody { .. } => "CreateHeaderFooterBody",
        Operation::RemoveHeaderFooterBody { .. } => "RemoveHeaderFooterBody",
        Operation::SetSectionRunningRef { .. } => "SetSectionRunningRef",
        Operation::SetSectionTitlePage { .. } => "SetSectionTitlePage",
        Operation::SetSectionWatermark { .. } => "SetSectionWatermark",
        Operation::SetSectionLineNumbering { .. } => "SetSectionLineNumbering",
        Operation::SetSectionPageNumbering { .. } => "SetSectionPageNumbering",
        Operation::SetSectionVerticalAlignment { .. } => "SetSectionVerticalAlignment",
        Operation::SetEvenAndOddHeaders { .. } => "SetEvenAndOddHeaders",
        Operation::SetShapeFill { .. } => "SetShapeFill",
        Operation::SetShapeStroke { .. } => "SetShapeStroke",
        Operation::SetTextBoxBody { .. } => "SetTextBoxBody",
        Operation::SetObjectLocks { .. } => "SetObjectLocks",
        Operation::SetTrackRevisions { .. } => "SetTrackRevisions",
    }
}

/// A container whose children carry sibling indices.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Container {
    /// A block list: the document body (`None`) or a cell / content control / text box.
    Blocks(Option<NodeId>),
    /// A table's rows.
    Rows(NodeId),
    /// A table's shared column grid, and the matching cell in every row.
    GridColumns(NodeId),
    /// One paragraph's (or wrapper's) inline list.
    Inlines(NodeId),
}

/// The coordinates an operation carries that a concurrent change can move.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Coordinates {
    /// Nothing positional: the operation names nodes or registry keys only.
    None,
    /// One caret inside one paragraph.
    Caret(Pos),
    /// One half-open range inside one paragraph.
    Range(EditRange),
    /// Two carets, possibly in different paragraphs (`CreateBookmark`).
    Carets(Pos, Pos),
    /// Two paragraphs that must remain adjacent (`JoinParagraphs`).
    Adjacent(NodeId, NodeId),
    /// A span of `count` children of `container` starting at `index`; `count == 0` is an
    /// insertion **gap** rather than a span, and the two map differently at a tie.
    Slot {
        container: Container,
        index: u32,
        count: u32,
    },
}

/// The coordinates `operation` carries.
///
/// Exhaustive by construction. O(1).
pub(super) fn coordinates(operation: &Operation) -> Coordinates {
    match operation {
        Operation::InsertText { at, .. }
        | Operation::SplitParagraph { at, .. }
        | Operation::InsertInlineObject { at, .. }
        | Operation::InsertField { at, .. }
        | Operation::InsertNote { at, .. } => Coordinates::Caret(*at),

        Operation::DeleteText { range }
        | Operation::FormatText { range, .. }
        | Operation::ClearFormatting { range }
        | Operation::SetHyperlink { range, .. } => Coordinates::Range(*range),

        Operation::CreateBookmark { start, end, .. } => Coordinates::Carets(*start, *end),

        Operation::JoinParagraphs { first, second, .. } => Coordinates::Adjacent(*first, *second),

        Operation::InsertRow { table, index, .. } => Coordinates::Slot {
            container: Container::Rows(*table),
            index: *index,
            count: 0,
        },
        Operation::DeleteRow { table, index } => Coordinates::Slot {
            container: Container::Rows(*table),
            index: *index,
            count: 1,
        },
        Operation::InsertColumn { table, index, .. } => Coordinates::Slot {
            container: Container::GridColumns(*table),
            index: *index,
            count: 0,
        },
        Operation::DeleteColumn { table, index } => Coordinates::Slot {
            container: Container::GridColumns(*table),
            index: *index,
            count: 1,
        },
        Operation::InsertTable {
            container, index, ..
        }
        | Operation::InsertBlocks {
            container, index, ..
        } => Coordinates::Slot {
            container: Container::Blocks(*container),
            index: *index,
            count: 0,
        },
        Operation::DeleteBlocks {
            container,
            index,
            count,
        } => Coordinates::Slot {
            container: Container::Blocks(*container),
            index: *index,
            count: *count,
        },
        // Body-level only, by the operation's own contract.
        Operation::InsertFieldRange { index, .. } => Coordinates::Slot {
            container: Container::Blocks(None),
            index: *index,
            count: 0,
        },
        Operation::InsertObjectNode { owner, index, .. } => Coordinates::Slot {
            container: Container::Inlines(*owner),
            index: *index,
            count: 0,
        },

        Operation::SetInlines { .. }
        | Operation::SetParagraphProperties { .. }
        | Operation::DeleteTable { .. }
        | Operation::SetExtent { .. }
        | Operation::SetGroupGeometry { .. }
        | Operation::SetAnchor { .. }
        | Operation::SetImageCrop { .. }
        | Operation::SetObjectDescr { .. }
        | Operation::DeleteObject { .. }
        | Operation::RemoveInlineObject { .. }
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
        | Operation::SetChartDefinition { .. }
        | Operation::DeleteBookmark { .. }
        | Operation::RenameBookmark { .. }
        | Operation::RemoveField { .. }
        | Operation::RemoveFieldRange { .. }
        | Operation::RemoveNote { .. }
        | Operation::CreateHeaderFooterBody { .. }
        | Operation::RemoveHeaderFooterBody { .. }
        | Operation::SetSectionRunningRef { .. }
        | Operation::SetSectionTitlePage { .. }
        | Operation::SetSectionWatermark { .. }
        | Operation::SetSectionLineNumbering { .. }
        | Operation::SetSectionPageNumbering { .. }
        | Operation::SetSectionVerticalAlignment { .. }
        | Operation::SetEvenAndOddHeaders { .. }
        | Operation::SetDocumentProtection { .. }
        | Operation::SetShapeFill { .. }
        | Operation::SetShapeStroke { .. }
        | Operation::SetTextBoxBody { .. }
        | Operation::SetObjectLocks { .. }
        | Operation::SetTrackRevisions { .. } => Coordinates::None,
    }
}

/// Writes rebased coordinates back into an operation.
///
/// The mirror of [`coordinates`], which is the exhaustive one: an operation that reports
/// `Coordinates::None` never reaches a shape this has to handle, so a new variant is
/// caught there rather than silently defaulted here.
pub(super) fn set_coordinates(operation: &mut Operation, moved: Coordinates) {
    match (operation, moved) {
        (
            Operation::InsertText { at, .. }
            | Operation::SplitParagraph { at, .. }
            | Operation::InsertInlineObject { at, .. }
            | Operation::InsertField { at, .. }
            | Operation::InsertNote { at, .. },
            Coordinates::Caret(moved),
        ) => *at = moved,
        (
            Operation::DeleteText { range }
            | Operation::FormatText { range, .. }
            | Operation::ClearFormatting { range }
            | Operation::SetHyperlink { range, .. },
            Coordinates::Range(moved),
        ) => *range = moved,
        (
            Operation::CreateBookmark { start, end, .. },
            Coordinates::Carets(moved_start, moved_end),
        ) => {
            *start = moved_start;
            *end = moved_end;
        }
        (
            Operation::JoinParagraphs { first, second, .. },
            Coordinates::Adjacent(moved_first, moved_second),
        ) => {
            *first = moved_first;
            *second = moved_second;
        }
        (
            Operation::InsertRow { table, index, .. } | Operation::DeleteRow { table, index },
            Coordinates::Slot {
                container: Container::Rows(moved_table),
                index: moved_index,
                ..
            },
        )
        | (
            Operation::InsertColumn { table, index, .. } | Operation::DeleteColumn { table, index },
            Coordinates::Slot {
                container: Container::GridColumns(moved_table),
                index: moved_index,
                ..
            },
        ) => {
            *table = moved_table;
            *index = moved_index;
        }
        (
            Operation::InsertTable {
                container, index, ..
            }
            | Operation::InsertBlocks {
                container, index, ..
            },
            Coordinates::Slot {
                container: Container::Blocks(moved_container),
                index: moved_index,
                ..
            },
        ) => {
            *container = moved_container;
            *index = moved_index;
        }
        (
            Operation::DeleteBlocks {
                container,
                index,
                count,
            },
            Coordinates::Slot {
                container: Container::Blocks(moved_container),
                index: moved_index,
                count: moved_count,
            },
        ) => {
            *container = moved_container;
            *index = moved_index;
            *count = moved_count;
        }
        (
            Operation::InsertFieldRange { index, .. },
            Coordinates::Slot {
                index: moved_index, ..
            },
        ) => *index = moved_index,
        (
            Operation::InsertObjectNode { owner, index, .. },
            Coordinates::Slot {
                container: Container::Inlines(moved_owner),
                index: moved_index,
                ..
            },
        ) => {
            *owner = moved_owner;
            *index = moved_index;
        }
        _ => {}
    }
}
