//! Atomic transaction application, inverses, and position mapping for OpenDoc.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

use casual_doc_model::{Document, Mark, ModelError, NodeId, TextRun};
use unicode_segmentation::UnicodeSegmentation;

/// Monotonic session-local document revision.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct RevisionId(u64);

impl RevisionId {
    /// Creates a revision from its numeric value.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the numeric revision.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    fn next(self) -> Result<Self, TransactionError> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or(TransactionError::RevisionExhausted)
    }
}

/// Stable identity of one transaction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransactionId(u128);

impl TransactionId {
    /// Creates a transaction ID.
    #[must_use]
    pub const fn new(value: u128) -> Self {
        Self(value)
    }

    /// Returns the numeric representation.
    #[must_use]
    pub const fn get(self) -> u128 {
        self.0
    }
}

/// Boundary behavior when an edit occurs at a position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Affinity {
    /// Stay before content inserted or split at the same boundary.
    Before,
    /// Move after content inserted or split at the same boundary.
    After,
}

/// A grapheme boundary inside a paragraph node.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Position {
    /// Paragraph node.
    pub node: NodeId,
    /// Zero-based extended grapheme boundary.
    pub grapheme_offset: u32,
    /// Mapping behavior at an equal edit boundary.
    pub affinity: Affinity,
}

/// An ordered text range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Range {
    /// Inclusive start boundary.
    pub start: Position,
    /// Exclusive end boundary.
    pub end: Position,
}

/// A primitive normalized-model operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Operation {
    /// Inserts marked text at a paragraph grapheme boundary.
    InsertText {
        /// Insertion boundary.
        at: Position,
        /// Inserted text.
        text: String,
        /// Marks applied to inserted text.
        marks: BTreeSet<Mark>,
    },
    /// Deletes a non-empty range inside one paragraph.
    DeleteRange {
        /// Range to delete.
        range: Range,
    },
    /// Splits one paragraph and inserts a new trailing paragraph.
    SplitParagraph {
        /// Split boundary in the original paragraph.
        at: Position,
        /// Stable ID assigned to the new paragraph.
        new_id: NodeId,
    },
    /// Joins two adjacent paragraphs in document order.
    JoinParagraph {
        /// Paragraph retaining its identity.
        first: NodeId,
        /// Adjacent paragraph removed by the join.
        second: NodeId,
    },
    /// Reinserts exact marked runs captured by an inverse.
    InsertRuns {
        /// Insertion boundary.
        at: Position,
        /// Marked runs to restore.
        runs: Vec<TextRun>,
    },
}

/// An atomic set of operations against one base revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transaction {
    id: TransactionId,
    base_revision: RevisionId,
    operations: Vec<Operation>,
}

impl Transaction {
    /// Creates a transaction.
    #[must_use]
    pub fn new(id: TransactionId, base_revision: RevisionId, operations: Vec<Operation>) -> Self {
        Self {
            id,
            base_revision,
            operations,
        }
    }

    /// Returns the transaction ID.
    #[must_use]
    pub const fn id(&self) -> TransactionId {
        self.id
    }

    /// Returns the declared base revision.
    #[must_use]
    pub const fn base_revision(&self) -> RevisionId {
        self.base_revision
    }

    /// Returns operations in application order.
    #[must_use]
    pub fn operations(&self) -> &[Operation] {
        &self.operations
    }
}

/// One deterministic position-mapping step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MappingStep {
    /// Text insertion into one paragraph.
    Insert {
        /// Paragraph containing the insertion.
        node: NodeId,
        /// Boundary before insertion.
        at: u32,
        /// Number of inserted graphemes.
        graphemes: u32,
    },
    /// Text deletion inside one paragraph.
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

/// Ordered mapping steps produced by a committed transaction.
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

    /// Maps a position through all transaction steps.
    #[must_use]
    pub fn map(&self, mut position: Position) -> Position {
        for step in &self.steps {
            match *step {
                MappingStep::Insert {
                    node,
                    at,
                    graphemes,
                } if position.node == node => {
                    if position.grapheme_offset > at
                        || (position.grapheme_offset == at && position.affinity == Affinity::After)
                    {
                        position.grapheme_offset =
                            position.grapheme_offset.saturating_add(graphemes);
                    }
                }
                MappingStep::Delete { node, start, end } if position.node == node => {
                    if position.grapheme_offset > start {
                        position.grapheme_offset = if position.grapheme_offset < end {
                            start
                        } else {
                            position.grapheme_offset - (end - start)
                        };
                    }
                }
                MappingStep::Split {
                    original,
                    new_node,
                    at,
                } if position.node == original => {
                    if position.grapheme_offset > at
                        || (position.grapheme_offset == at && position.affinity == Affinity::After)
                    {
                        position.node = new_node;
                        position.grapheme_offset -= at;
                    }
                }
                MappingStep::Join { first, second, at } if position.node == second => {
                    position.node = first;
                    position.grapheme_offset = position.grapheme_offset.saturating_add(at);
                }
                _ => {}
            }
        }
        position
    }
}

/// Result of a committed transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Commit {
    /// New normalized document.
    pub document: Document,
    /// New session revision.
    pub revision: RevisionId,
    /// Position mapping from the prior revision.
    pub position_map: PositionMap,
    /// Semantic operations that reverse this commit.
    pub inverse_operations: Vec<Operation>,
    /// Number of applied operations.
    pub operations_applied: usize,
}

/// Applies a transaction atomically to a cloned working document.
pub fn apply(
    document: &Document,
    current_revision: RevisionId,
    transaction: &Transaction,
) -> Result<Commit, TransactionError> {
    if transaction.base_revision != current_revision {
        return Err(TransactionError::StaleRevision {
            expected: transaction.base_revision,
            actual: current_revision,
        });
    }
    if transaction.operations.is_empty() {
        return Err(TransactionError::EmptyTransaction);
    }

    let mut working = document.clone();
    let mut map = PositionMap::default();
    let mut inverse_operations = Vec::with_capacity(transaction.operations.len());

    for operation in &transaction.operations {
        let inverse = apply_operation(&mut working, &mut map, operation)?;
        inverse_operations.push(inverse);
    }

    working.validate().map_err(TransactionError::Model)?;
    inverse_operations.reverse();
    Ok(Commit {
        document: working,
        revision: current_revision.next()?,
        position_map: map,
        inverse_operations,
        operations_applied: transaction.operations.len(),
    })
}

fn apply_operation(
    working: &mut Document,
    map: &mut PositionMap,
    operation: &Operation,
) -> Result<Operation, TransactionError> {
    match operation {
        Operation::InsertText { at, text, marks } => {
            validate_insert_text(text)?;
            let graphemes = unicode_grapheme_count(text)?;
            validate_insert_capacity(working, *at, graphemes)?;
            working
                .paragraph_mut(at.node)
                .ok_or(TransactionError::InvalidPosition {
                    node: at.node,
                    offset: at.grapheme_offset,
                })?
                .insert_text(at.grapheme_offset as usize, text.clone(), marks.clone())
                .map_err(|error| map_model_error(error, at.node, at.grapheme_offset))?;

            let end = at
                .grapheme_offset
                .checked_add(graphemes)
                .ok_or(TransactionError::TextTooLong)?;
            map.steps.push(MappingStep::Insert {
                node: at.node,
                at: at.grapheme_offset,
                graphemes,
            });
            Ok(Operation::DeleteRange {
                range: Range {
                    start: Position {
                        node: at.node,
                        grapheme_offset: at.grapheme_offset,
                        affinity: Affinity::Before,
                    },
                    end: Position {
                        node: at.node,
                        grapheme_offset: end,
                        affinity: Affinity::After,
                    },
                },
            })
        }
        Operation::DeleteRange { range } => {
            validate_range(*range)?;
            let removed = working
                .paragraph_mut(range.start.node)
                .ok_or(TransactionError::InvalidPosition {
                    node: range.start.node,
                    offset: range.start.grapheme_offset,
                })?
                .delete_range(
                    range.start.grapheme_offset as usize,
                    range.end.grapheme_offset as usize,
                )
                .map_err(|error| {
                    map_model_error(error, range.start.node, range.end.grapheme_offset)
                })?;
            map.steps.push(MappingStep::Delete {
                node: range.start.node,
                start: range.start.grapheme_offset,
                end: range.end.grapheme_offset,
            });
            Ok(Operation::InsertRuns {
                at: Position {
                    node: range.start.node,
                    grapheme_offset: range.start.grapheme_offset,
                    affinity: Affinity::After,
                },
                runs: removed,
            })
        }
        Operation::SplitParagraph { at, new_id } => {
            working
                .split_paragraph(at.node, at.grapheme_offset as usize, *new_id)
                .map_err(|error| map_model_error(error, at.node, at.grapheme_offset))?;
            map.steps.push(MappingStep::Split {
                original: at.node,
                new_node: *new_id,
                at: at.grapheme_offset,
            });
            Ok(Operation::JoinParagraph {
                first: at.node,
                second: *new_id,
            })
        }
        Operation::JoinParagraph { first, second } => {
            let boundary = working
                .join_paragraphs(*first, *second)
                .map_err(|error| map_model_error(error, *first, 0))?;
            map.steps.push(MappingStep::Join {
                first: *first,
                second: *second,
                at: boundary,
            });
            Ok(Operation::SplitParagraph {
                at: Position {
                    node: *first,
                    grapheme_offset: boundary,
                    affinity: Affinity::After,
                },
                new_id: *second,
            })
        }
        Operation::InsertRuns { at, runs } => {
            if runs.is_empty() {
                return Err(TransactionError::EmptyTransaction);
            }
            let graphemes = runs.iter().try_fold(0_u32, |total, run| {
                total
                    .checked_add(unicode_grapheme_count(run.text())?)
                    .ok_or(TransactionError::TextTooLong)
            })?;
            validate_insert_capacity(working, *at, graphemes)?;
            working
                .paragraph_mut(at.node)
                .ok_or(TransactionError::InvalidPosition {
                    node: at.node,
                    offset: at.grapheme_offset,
                })?
                .insert_runs(at.grapheme_offset as usize, runs.clone())
                .map_err(|error| map_model_error(error, at.node, at.grapheme_offset))?;
            let end = at
                .grapheme_offset
                .checked_add(graphemes)
                .ok_or(TransactionError::TextTooLong)?;
            map.steps.push(MappingStep::Insert {
                node: at.node,
                at: at.grapheme_offset,
                graphemes,
            });
            Ok(Operation::DeleteRange {
                range: Range {
                    start: Position {
                        node: at.node,
                        grapheme_offset: at.grapheme_offset,
                        affinity: Affinity::Before,
                    },
                    end: Position {
                        node: at.node,
                        grapheme_offset: end,
                        affinity: Affinity::After,
                    },
                },
            })
        }
    }
}

fn validate_insert_capacity(
    document: &Document,
    at: Position,
    inserted: u32,
) -> Result<(), TransactionError> {
    let paragraph = document
        .paragraph(at.node)
        .ok_or(TransactionError::InvalidPosition {
            node: at.node,
            offset: at.grapheme_offset,
        })?;
    let current =
        u32::try_from(paragraph.grapheme_len()).map_err(|_| TransactionError::TextTooLong)?;
    if at.grapheme_offset > current {
        return Err(TransactionError::InvalidPosition {
            node: at.node,
            offset: at.grapheme_offset,
        });
    }
    current
        .checked_add(inserted)
        .ok_or(TransactionError::TextTooLong)?;
    Ok(())
}

fn validate_range(range: Range) -> Result<(), TransactionError> {
    if range.start.node != range.end.node
        || range.start.grapheme_offset >= range.end.grapheme_offset
    {
        return Err(TransactionError::InvalidRange {
            start_node: range.start.node,
            start: range.start.grapheme_offset,
            end_node: range.end.node,
            end: range.end.grapheme_offset,
        });
    }
    Ok(())
}

fn validate_insert_text(text: &str) -> Result<(), TransactionError> {
    if text.is_empty() {
        return Err(TransactionError::EmptyTransaction);
    }
    if text.chars().any(|character| {
        matches!(
            character,
            '\0' | '\r' | '\n' | '\t' | '\u{2028}' | '\u{2029}'
        )
    }) {
        return Err(TransactionError::InvalidTextInput);
    }
    Ok(())
}

fn unicode_grapheme_count(text: &str) -> Result<u32, TransactionError> {
    u32::try_from(text.graphemes(true).count()).map_err(|_| TransactionError::TextTooLong)
}

fn map_model_error(error: ModelError, node: NodeId, offset: u32) -> TransactionError {
    match error {
        ModelError::InvalidGraphemeOffset { .. } => {
            TransactionError::InvalidPosition { node, offset }
        }
        ModelError::InvalidGraphemeRange { start, end } => TransactionError::InvalidRange {
            start_node: node,
            start: u32::try_from(start).unwrap_or(u32::MAX),
            end_node: node,
            end: u32::try_from(end).unwrap_or(u32::MAX),
        },
        ModelError::UnknownParagraph(_)
        | ModelError::ParagraphsNotAdjacent { .. }
        | ModelError::DuplicateNodeId(_) => TransactionError::InvalidStructure,
        ModelError::GraphemeCountOverflow(_) => TransactionError::TextTooLong,
        other => TransactionError::Model(other),
    }
}

/// Transaction validation or application failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransactionError {
    /// The transaction was based on a different revision.
    StaleRevision {
        /// Transaction's declared revision.
        expected: RevisionId,
        /// Current session revision.
        actual: RevisionId,
    },
    /// The transaction had no effective operation.
    EmptyTransaction,
    /// A position did not resolve.
    InvalidPosition {
        /// Requested node.
        node: NodeId,
        /// Requested grapheme offset.
        offset: u32,
    },
    /// A range was cross-node, empty, reversed, or outside its paragraph.
    InvalidRange {
        /// Start node.
        start_node: NodeId,
        /// Start offset.
        start: u32,
        /// End node.
        end_node: NodeId,
        /// End offset.
        end: u32,
    },
    /// A structural operation violated identity or adjacency rules.
    InvalidStructure,
    /// Inserted text requires a structural command.
    InvalidTextInput,
    /// Inserted text exceeded addressable grapheme count.
    TextTooLong,
    /// The session revision counter was exhausted.
    RevisionExhausted,
    /// The normalized model rejected the working document.
    Model(ModelError),
}

impl fmt::Display for TransactionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StaleRevision { expected, actual } => write!(
                formatter,
                "transaction revision {} does not match current revision {}",
                expected.get(),
                actual.get()
            ),
            Self::EmptyTransaction => formatter.write_str("transaction has no effective operation"),
            Self::InvalidPosition { node, offset } => {
                write!(formatter, "position {node}:{offset} is invalid")
            }
            Self::InvalidRange {
                start_node,
                start,
                end_node,
                end,
            } => write!(
                formatter,
                "range {start_node}:{start}..{end_node}:{end} is invalid"
            ),
            Self::InvalidStructure => formatter.write_str("structural operation is invalid"),
            Self::InvalidTextInput => {
                formatter.write_str("text contains a structural control character")
            }
            Self::TextTooLong => formatter.write_str("inserted text is too long"),
            Self::RevisionExhausted => formatter.write_str("revision counter is exhausted"),
            Self::Model(error) => write!(formatter, "model validation failed: {error}"),
        }
    }
}

impl Error for TransactionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Model(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use casual_doc_model::IdGenerator;

    fn blank() -> (Document, NodeId, IdGenerator) {
        let mut ids = IdGenerator::new(1);
        let document_id = ids.next_id().unwrap();
        let paragraph_id = ids.next_id().unwrap();
        (
            Document::blank(document_id, paragraph_id).unwrap(),
            paragraph_id,
            ids,
        )
    }

    fn position(node: NodeId, offset: u32, affinity: Affinity) -> Position {
        Position {
            node,
            grapheme_offset: offset,
            affinity,
        }
    }

    fn insert(paragraph: NodeId, offset: u32, text: &str, marks: BTreeSet<Mark>) -> Operation {
        Operation::InsertText {
            at: position(paragraph, offset, Affinity::After),
            text: text.to_owned(),
            marks,
        }
    }

    #[test]
    fn applies_grapheme_aware_insertions() {
        let (document, paragraph, _) = blank();
        let first = apply(
            &document,
            RevisionId::new(0),
            &Transaction::new(
                TransactionId::new(1),
                RevisionId::new(0),
                vec![insert(paragraph, 0, "Ae\u{301}👨‍👩‍👧‍👦B", BTreeSet::new())],
            ),
        )
        .unwrap();
        let second = apply(
            &first.document,
            first.revision,
            &Transaction::new(
                TransactionId::new(2),
                first.revision,
                vec![insert(paragraph, 3, "X", BTreeSet::new())],
            ),
        )
        .unwrap();

        assert_eq!(
            second.document.paragraph(paragraph).unwrap().plain_text(),
            "Ae\u{301}👨‍👩‍👧‍👦XB"
        );
        assert_eq!(second.revision.get(), 2);
    }

    #[test]
    fn delete_inverse_restores_exact_marked_document() {
        let (document, paragraph, _) = blank();
        let bold = BTreeSet::from([Mark::Bold]);
        let italic = BTreeSet::from([Mark::Italic]);
        let seeded = apply(
            &document,
            RevisionId::new(0),
            &Transaction::new(
                TransactionId::new(1),
                RevisionId::new(0),
                vec![
                    insert(paragraph, 0, "ab", bold),
                    insert(paragraph, 2, "cd", BTreeSet::new()),
                    insert(paragraph, 4, "ef", italic),
                ],
            ),
        )
        .unwrap();
        let deleted = apply(
            &seeded.document,
            seeded.revision,
            &Transaction::new(
                TransactionId::new(2),
                seeded.revision,
                vec![Operation::DeleteRange {
                    range: Range {
                        start: position(paragraph, 1, Affinity::Before),
                        end: position(paragraph, 5, Affinity::After),
                    },
                }],
            ),
        )
        .unwrap();
        assert_eq!(
            deleted.document.paragraph(paragraph).unwrap().plain_text(),
            "af"
        );

        let restored = apply(
            &deleted.document,
            deleted.revision,
            &Transaction::new(
                TransactionId::new(3),
                deleted.revision,
                deleted.inverse_operations,
            ),
        )
        .unwrap();
        assert_eq!(restored.document, seeded.document);
    }

    #[test]
    fn split_join_and_inverse_preserve_identity_and_content() {
        let (document, paragraph, mut ids) = blank();
        let seeded = apply(
            &document,
            RevisionId::new(0),
            &Transaction::new(
                TransactionId::new(1),
                RevisionId::new(0),
                vec![insert(paragraph, 0, "leftRight", BTreeSet::new())],
            ),
        )
        .unwrap();
        let second = ids.next_id().unwrap();
        let split = apply(
            &seeded.document,
            seeded.revision,
            &Transaction::new(
                TransactionId::new(2),
                seeded.revision,
                vec![Operation::SplitParagraph {
                    at: position(paragraph, 4, Affinity::After),
                    new_id: second,
                }],
            ),
        )
        .unwrap();

        assert_eq!(
            split.document.paragraph(paragraph).unwrap().plain_text(),
            "left"
        );
        assert_eq!(
            split.document.paragraph(second).unwrap().plain_text(),
            "Right"
        );

        let joined = apply(
            &split.document,
            split.revision,
            &Transaction::new(
                TransactionId::new(3),
                split.revision,
                split.inverse_operations,
            ),
        )
        .unwrap();
        assert_eq!(joined.document, seeded.document);
    }

    #[test]
    fn mapping_covers_insert_delete_split_and_join() {
        let (document, paragraph, mut ids) = blank();
        let seeded = apply(
            &document,
            RevisionId::new(0),
            &Transaction::new(
                TransactionId::new(1),
                RevisionId::new(0),
                vec![insert(paragraph, 0, "abcd", BTreeSet::new())],
            ),
        )
        .unwrap();
        let second = ids.next_id().unwrap();
        let split = apply(
            &seeded.document,
            seeded.revision,
            &Transaction::new(
                TransactionId::new(2),
                seeded.revision,
                vec![Operation::SplitParagraph {
                    at: position(paragraph, 2, Affinity::After),
                    new_id: second,
                }],
            ),
        )
        .unwrap();
        assert_eq!(
            split
                .position_map
                .map(position(paragraph, 3, Affinity::After)),
            position(second, 1, Affinity::After)
        );

        let joined = apply(
            &split.document,
            split.revision,
            &Transaction::new(
                TransactionId::new(3),
                split.revision,
                vec![Operation::JoinParagraph {
                    first: paragraph,
                    second,
                }],
            ),
        )
        .unwrap();
        assert_eq!(
            joined
                .position_map
                .map(position(second, 1, Affinity::After)),
            position(paragraph, 3, Affinity::After)
        );

        let deleted = apply(
            &joined.document,
            joined.revision,
            &Transaction::new(
                TransactionId::new(4),
                joined.revision,
                vec![Operation::DeleteRange {
                    range: Range {
                        start: position(paragraph, 1, Affinity::Before),
                        end: position(paragraph, 3, Affinity::After),
                    },
                }],
            ),
        )
        .unwrap();
        assert_eq!(
            deleted
                .position_map
                .map(position(paragraph, 2, Affinity::After))
                .grapheme_offset,
            1
        );
        assert_eq!(
            deleted
                .position_map
                .map(position(paragraph, 4, Affinity::After))
                .grapheme_offset,
            2
        );
    }

    #[test]
    fn invalid_later_operation_is_atomic() {
        let (document, paragraph, _) = blank();
        let transaction = Transaction::new(
            TransactionId::new(1),
            RevisionId::new(0),
            vec![
                insert(paragraph, 0, "valid", BTreeSet::new()),
                insert(paragraph, 99, "invalid", BTreeSet::new()),
            ],
        );

        assert!(matches!(
            apply(&document, RevisionId::new(0), &transaction),
            Err(TransactionError::InvalidPosition { .. })
        ));
        assert_eq!(document.paragraph(paragraph).unwrap().plain_text(), "");
    }

    #[test]
    fn position_affinity_controls_equal_insertion_and_split_mapping() {
        let (document, paragraph, mut ids) = blank();
        let commit = apply(
            &document,
            RevisionId::new(0),
            &Transaction::new(
                TransactionId::new(1),
                RevisionId::new(0),
                vec![insert(paragraph, 0, "AB", BTreeSet::new())],
            ),
        )
        .unwrap();
        assert_eq!(
            commit
                .position_map
                .map(position(paragraph, 0, Affinity::Before))
                .grapheme_offset,
            0
        );
        assert_eq!(
            commit
                .position_map
                .map(position(paragraph, 0, Affinity::After))
                .grapheme_offset,
            2
        );

        let second = ids.next_id().unwrap();
        let split = apply(
            &commit.document,
            commit.revision,
            &Transaction::new(
                TransactionId::new(2),
                commit.revision,
                vec![Operation::SplitParagraph {
                    at: position(paragraph, 1, Affinity::After),
                    new_id: second,
                }],
            ),
        )
        .unwrap();
        assert_eq!(
            split
                .position_map
                .map(position(paragraph, 1, Affinity::Before))
                .node,
            paragraph
        );
        assert_eq!(
            split
                .position_map
                .map(position(paragraph, 1, Affinity::After))
                .node,
            second
        );
    }

    #[test]
    fn structural_controls_are_rejected_without_mutation() {
        let (document, paragraph, _) = blank();
        let result = apply(
            &document,
            RevisionId::new(0),
            &Transaction::new(
                TransactionId::new(1),
                RevisionId::new(0),
                vec![insert(paragraph, 0, "line\nbreak", BTreeSet::new())],
            ),
        );

        assert_eq!(result, Err(TransactionError::InvalidTextInput));
        assert_eq!(document.paragraph(paragraph).unwrap().plain_text(), "");
    }
}
