//! Host-facing editing command requests and their transaction results.

use std::collections::BTreeSet;

use casual_doc_transaction as transaction;

use crate::selection::{Affinity, Position, Range, SelectionSnapshot};
use crate::value::{Mark, NodeId, Revision};

/// Request for the first transaction-backed SDK command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsertTextRequest {
    /// Revision against which the host constructed the request.
    pub base_revision: Revision,
    /// Insertion boundary.
    pub at: Position,
    /// Inserted text.
    pub text: String,
    /// Marks for the inserted run.
    pub marks: BTreeSet<Mark>,
}

/// Request to delete a non-empty range inside one paragraph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteRangeRequest {
    /// Revision against which the host constructed the request.
    pub base_revision: Revision,
    /// Range to delete.
    pub range: Range,
}

/// Request to split one paragraph at a grapheme boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SplitParagraphRequest {
    /// Revision against which the host constructed the request.
    pub base_revision: Revision,
    /// Split boundary in the original paragraph.
    pub at: Position,
}

/// Request to join two adjacent paragraphs in document order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JoinParagraphRequest {
    /// Revision against which the host constructed the request.
    pub base_revision: Revision,
    /// Paragraph retaining its identity.
    pub first: NodeId,
    /// Adjacent paragraph removed by the join.
    pub second: NodeId,
}

/// Request to replace session selection without mutating the document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetSelectionRequest {
    /// Revision against which the host resolved both endpoints.
    pub base_revision: Revision,
    /// Directed logical text selection.
    pub selection: SelectionSnapshot,
}

/// One public deterministic position-mapping step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MappingStep {
    /// Text insertion.
    Insert {
        /// Paragraph containing the insertion.
        node: NodeId,
        /// Boundary before insertion.
        at: u32,
        /// Number of inserted graphemes.
        graphemes: u32,
    },
    /// Text deletion.
    Delete {
        /// Paragraph containing the deletion.
        node: NodeId,
        /// Inclusive deletion start.
        start: u32,
        /// Exclusive deletion end.
        end: u32,
    },
    /// Paragraph split.
    Split {
        /// Original paragraph.
        original: NodeId,
        /// New trailing paragraph.
        new_node: NodeId,
        /// Split boundary in the original.
        at: u32,
    },
    /// Adjacent paragraph join.
    Join {
        /// Paragraph retaining identity.
        first: NodeId,
        /// Removed paragraph.
        second: NodeId,
        /// Former end of the first paragraph.
        at: u32,
    },
}

/// Ordered position map returned by a committed transaction.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PositionMap {
    steps: Vec<MappingStep>,
}

impl PositionMap {
    /// Returns mapping steps in transaction order.
    #[must_use]
    pub fn steps(&self) -> &[MappingStep] {
        &self.steps
    }

    /// Maps a host position through every transaction step.
    #[must_use]
    pub fn map(&self, mut position: Position) -> Position {
        for step in &self.steps {
            match step {
                MappingStep::Insert {
                    node,
                    at,
                    graphemes,
                } if position.node == *node => {
                    if position.grapheme_offset > *at
                        || (position.grapheme_offset == *at && position.affinity == Affinity::After)
                    {
                        position.grapheme_offset =
                            position.grapheme_offset.saturating_add(*graphemes);
                    }
                }
                MappingStep::Delete { node, start, end } if position.node == *node => {
                    if position.grapheme_offset > *start {
                        position.grapheme_offset = if position.grapheme_offset < *end {
                            *start
                        } else {
                            position.grapheme_offset - (*end - *start)
                        };
                    }
                }
                MappingStep::Split {
                    original,
                    new_node,
                    at,
                } if position.node == *original => {
                    if position.grapheme_offset > *at
                        || (position.grapheme_offset == *at && position.affinity == Affinity::After)
                    {
                        position.node = new_node.clone();
                        position.grapheme_offset -= *at;
                    }
                }
                MappingStep::Join { first, second, at } if position.node == *second => {
                    position.node = first.clone();
                    position.grapheme_offset = position.grapheme_offset.saturating_add(*at);
                }
                _ => {}
            }
        }
        position
    }
}

/// Result of a successful editing command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransactionResult {
    /// Committed revision.
    pub revision: Revision,
    /// Position mapping from the prior revision.
    pub position_map: PositionMap,
    /// Number of operations committed.
    pub operations_applied: usize,
}

pub(crate) fn transaction_result(commit: &transaction::Commit) -> TransactionResult {
    TransactionResult {
        revision: Revision(commit.revision.get()),
        position_map: PositionMap {
            steps: commit
                .position_map
                .steps()
                .iter()
                .map(|step| match *step {
                    transaction::MappingStep::Insert {
                        node,
                        at,
                        graphemes,
                    } => MappingStep::Insert {
                        node: NodeId::from_internal(node),
                        at,
                        graphemes,
                    },
                    transaction::MappingStep::Delete { node, start, end } => MappingStep::Delete {
                        node: NodeId::from_internal(node),
                        start,
                        end,
                    },
                    transaction::MappingStep::Split {
                        original,
                        new_node,
                        at,
                    } => MappingStep::Split {
                        original: NodeId::from_internal(original),
                        new_node: NodeId::from_internal(new_node),
                        at,
                    },
                    transaction::MappingStep::Join { first, second, at } => MappingStep::Join {
                        first: NodeId::from_internal(first),
                        second: NodeId::from_internal(second),
                        at,
                    },
                })
                .collect(),
        },
        operations_applied: commit.operations_applied,
    }
}
