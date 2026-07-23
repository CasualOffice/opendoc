//! Atomic transaction application and position mapping for OpenDoc.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

use casual_doc_model::{Document, Mark, ModelError, NodeId};
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

/// Boundary behavior when an insertion occurs at a position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Affinity {
    /// Stay before text inserted at the same boundary.
    Before,
    /// Move after text inserted at the same boundary.
    After,
}

/// A grapheme boundary inside a paragraph node.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Position {
    /// Paragraph node.
    pub node: NodeId,
    /// Zero-based extended grapheme boundary.
    pub grapheme_offset: u32,
    /// Mapping behavior at an equal insertion boundary.
    pub affinity: Affinity,
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
}

/// One insertion step in a position map.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InsertionMap {
    /// Paragraph containing the insertion.
    pub node: NodeId,
    /// Grapheme boundary before insertion.
    pub at: u32,
    /// Number of inserted graphemes.
    pub graphemes: u32,
}

/// Ordered mapping steps produced by a committed transaction.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PositionMap {
    insertions: Vec<InsertionMap>,
}

impl PositionMap {
    /// Returns insertion steps in transaction order.
    #[must_use]
    pub fn insertions(&self) -> &[InsertionMap] {
        &self.insertions
    }

    /// Maps a position through all transaction steps.
    #[must_use]
    pub fn map(&self, mut position: Position) -> Position {
        for insertion in &self.insertions {
            if position.node != insertion.node {
                continue;
            }
            if position.grapheme_offset > insertion.at
                || (position.grapheme_offset == insertion.at
                    && position.affinity == Affinity::After)
            {
                position.grapheme_offset =
                    position.grapheme_offset.saturating_add(insertion.graphemes);
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

    for operation in &transaction.operations {
        match operation {
            Operation::InsertText { at, text, marks } => {
                validate_insert_text(text)?;
                let paragraph =
                    working
                        .paragraph_mut(at.node)
                        .ok_or(TransactionError::InvalidPosition {
                            node: at.node,
                            offset: at.grapheme_offset,
                        })?;
                paragraph
                    .insert_text(at.grapheme_offset as usize, text.clone(), marks.clone())
                    .map_err(|error| match error {
                        ModelError::InvalidGraphemeOffset { .. } => {
                            TransactionError::InvalidPosition {
                                node: at.node,
                                offset: at.grapheme_offset,
                            }
                        }
                        other => TransactionError::Model(other),
                    })?;

                let graphemes = unicode_grapheme_count(text)?;
                map.insertions.push(InsertionMap {
                    node: at.node,
                    at: at.grapheme_offset,
                    graphemes,
                });
            }
        }
    }

    working.validate().map_err(TransactionError::Model)?;
    Ok(Commit {
        document: working,
        revision: current_revision.next()?,
        position_map: map,
        operations_applied: transaction.operations.len(),
    })
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

    fn blank() -> (Document, NodeId) {
        let mut ids = IdGenerator::new(1);
        let document_id = ids.next_id().unwrap();
        let paragraph_id = ids.next_id().unwrap();
        (
            Document::blank(document_id, paragraph_id).unwrap(),
            paragraph_id,
        )
    }

    fn insert(paragraph: NodeId, offset: u32, text: &str) -> Operation {
        Operation::InsertText {
            at: Position {
                node: paragraph,
                grapheme_offset: offset,
                affinity: Affinity::After,
            },
            text: text.to_owned(),
            marks: BTreeSet::new(),
        }
    }

    #[test]
    fn applies_grapheme_aware_insertions() {
        let (document, paragraph) = blank();
        let first = apply(
            &document,
            RevisionId::new(0),
            &Transaction::new(
                TransactionId::new(1),
                RevisionId::new(0),
                vec![insert(paragraph, 0, "Ae\u{301}👨‍👩‍👧‍👦B")],
            ),
        )
        .unwrap();
        let second = apply(
            &first.document,
            first.revision,
            &Transaction::new(
                TransactionId::new(2),
                first.revision,
                vec![insert(paragraph, 3, "X")],
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
    fn invalid_later_operation_is_atomic() {
        let (document, paragraph) = blank();
        let transaction = Transaction::new(
            TransactionId::new(1),
            RevisionId::new(0),
            vec![
                insert(paragraph, 0, "valid"),
                insert(paragraph, 99, "invalid"),
            ],
        );

        assert!(matches!(
            apply(&document, RevisionId::new(0), &transaction),
            Err(TransactionError::InvalidPosition { .. })
        ));
        assert_eq!(document.paragraph(paragraph).unwrap().plain_text(), "");
    }

    #[test]
    fn position_affinity_controls_equal_boundary_mapping() {
        let (document, paragraph) = blank();
        let commit = apply(
            &document,
            RevisionId::new(0),
            &Transaction::new(
                TransactionId::new(1),
                RevisionId::new(0),
                vec![insert(paragraph, 0, "👨‍👩‍👧‍👦")],
            ),
        )
        .unwrap();

        let before = commit.position_map.map(Position {
            node: paragraph,
            grapheme_offset: 0,
            affinity: Affinity::Before,
        });
        let after = commit.position_map.map(Position {
            node: paragraph,
            grapheme_offset: 0,
            affinity: Affinity::After,
        });

        assert_eq!(before.grapheme_offset, 0);
        assert_eq!(after.grapheme_offset, 1);
    }

    #[test]
    fn structural_controls_are_rejected_without_mutation() {
        let (document, paragraph) = blank();
        let result = apply(
            &document,
            RevisionId::new(0),
            &Transaction::new(
                TransactionId::new(1),
                RevisionId::new(0),
                vec![insert(paragraph, 0, "line\nbreak")],
            ),
        );

        assert_eq!(result, Err(TransactionError::InvalidTextInput));
        assert_eq!(document.paragraph(paragraph).unwrap().plain_text(), "");
    }
}
