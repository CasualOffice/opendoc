//! Validated logical caret and text-range state for OpenDoc sessions.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt;

use casual_doc_model::{BlockNode, Document, NodeId};
use casual_doc_transaction::{Affinity, Position, PositionMap};

/// One directed logical text selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextSelection {
    anchor: Position,
    focus: Position,
}

impl TextSelection {
    /// Creates and validates a selection against a document.
    pub fn new(
        document: &Document,
        anchor: Position,
        focus: Position,
    ) -> Result<Self, SelectionError> {
        let selection = Self { anchor, focus };
        selection.validate(document)?;
        Ok(selection)
    }

    /// Creates the default collapsed caret for a valid document.
    pub fn default_for(document: &Document) -> Result<Self, SelectionError> {
        let first = document
            .body()
            .first()
            .ok_or(SelectionError::EmptyDocument)?;
        let node = match first {
            BlockNode::Paragraph(paragraph) => paragraph.id(),
        };
        let caret = Position {
            node,
            grapheme_offset: 0,
            affinity: Affinity::After,
        };
        Ok(Self {
            anchor: caret,
            focus: caret,
        })
    }

    /// Returns the endpoint where selection began.
    #[must_use]
    pub const fn anchor(&self) -> Position {
        self.anchor
    }

    /// Returns the active selection endpoint.
    #[must_use]
    pub const fn focus(&self) -> Position {
        self.focus
    }

    /// Returns whether anchor and focus describe the same logical boundary.
    #[must_use]
    pub fn is_collapsed(&self) -> bool {
        self.anchor.node == self.focus.node
            && self.anchor.grapheme_offset == self.focus.grapheme_offset
    }

    /// Validates both endpoints against a normalized document.
    pub fn validate(&self, document: &Document) -> Result<(), SelectionError> {
        validate_position(document, self.anchor)?;
        validate_position(document, self.focus)
    }

    /// Maps both endpoints and validates the result against the new document.
    pub fn mapped(&self, map: &PositionMap, document: &Document) -> Result<Self, SelectionError> {
        Self::new(document, map.map(self.anchor), map.map(self.focus))
    }
}

fn validate_position(document: &Document, position: Position) -> Result<(), SelectionError> {
    let paragraph = document
        .paragraph(position.node)
        .ok_or(SelectionError::InvalidPosition {
            node: position.node,
            offset: position.grapheme_offset,
        })?;
    let length =
        u32::try_from(paragraph.grapheme_len()).map_err(|_| SelectionError::OffsetOverflow {
            node: position.node,
        })?;
    if position.grapheme_offset > length {
        return Err(SelectionError::InvalidPosition {
            node: position.node,
            offset: position.grapheme_offset,
        });
    }
    Ok(())
}

/// Logical selection construction or mapping failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectionError {
    /// The document had no initial body paragraph.
    EmptyDocument,
    /// A selection endpoint did not resolve.
    InvalidPosition {
        /// Requested node.
        node: NodeId,
        /// Requested grapheme offset.
        offset: u32,
    },
    /// A paragraph exceeded the public offset representation.
    OffsetOverflow {
        /// Paragraph whose length overflowed.
        node: NodeId,
    },
}

impl fmt::Display for SelectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyDocument => formatter.write_str("document has no initial paragraph"),
            Self::InvalidPosition { node, offset } => {
                write!(formatter, "selection position {node}:{offset} is invalid")
            }
            Self::OffsetOverflow { node } => {
                write!(
                    formatter,
                    "paragraph {node} exceeds selection offset capacity"
                )
            }
        }
    }
}

impl Error for SelectionError {}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use casual_doc_model::IdGenerator;
    use casual_doc_transaction::{Operation, RevisionId, Transaction, TransactionId, apply};

    use super::*;

    fn blank() -> (Document, NodeId) {
        let mut ids = IdGenerator::new(1);
        let document_id = ids.next_id().unwrap();
        let paragraph_id = ids.next_id().unwrap();
        (
            Document::blank(document_id, paragraph_id).unwrap(),
            paragraph_id,
        )
    }

    #[test]
    fn default_selection_is_a_collapsed_after_affinity_caret() {
        let (document, paragraph) = blank();
        let selection = TextSelection::default_for(&document).unwrap();

        assert!(selection.is_collapsed());
        assert_eq!(selection.anchor().node, paragraph);
        assert_eq!(selection.anchor().grapheme_offset, 0);
        assert_eq!(selection.anchor().affinity, Affinity::After);
    }

    #[test]
    fn invalid_endpoint_is_rejected() {
        let (document, paragraph) = blank();
        let invalid = Position {
            node: paragraph,
            grapheme_offset: 1,
            affinity: Affinity::After,
        };

        assert!(matches!(
            TextSelection::new(&document, invalid, invalid),
            Err(SelectionError::InvalidPosition { .. })
        ));
    }

    #[test]
    fn selection_maps_through_transaction_position_map() {
        let (document, paragraph) = blank();
        let caret = Position {
            node: paragraph,
            grapheme_offset: 0,
            affinity: Affinity::After,
        };
        let selection = TextSelection::new(&document, caret, caret).unwrap();
        let commit = apply(
            &document,
            RevisionId::new(0),
            &Transaction::new(
                TransactionId::new(1),
                RevisionId::new(0),
                vec![Operation::InsertText {
                    at: caret,
                    text: "👩🏽‍💻".to_owned(),
                    marks: BTreeSet::new(),
                }],
            ),
        )
        .unwrap();

        let mapped = selection
            .mapped(&commit.position_map, &commit.document)
            .unwrap();
        assert_eq!(mapped.anchor().grapheme_offset, 1);
        assert!(mapped.is_collapsed());
    }
}
