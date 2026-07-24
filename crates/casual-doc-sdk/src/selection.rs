//! Public position and directed-selection value objects.

use casual_doc_selection as selection;
use casual_doc_transaction as transaction;
use serde::{Deserialize, Serialize};

use crate::error::SdkError;
use crate::value::NodeId;

/// Boundary behavior when content is inserted at a position.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Affinity {
    /// Keep the position before inserted content.
    Before,
    /// Move the position after inserted content.
    After,
}

impl Affinity {
    fn to_internal(self) -> transaction::Affinity {
        match self {
            Self::Before => transaction::Affinity::Before,
            Self::After => transaction::Affinity::After,
        }
    }

    fn from_internal(affinity: transaction::Affinity) -> Self {
        match affinity {
            transaction::Affinity::Before => Self::Before,
            transaction::Affinity::After => Self::After,
        }
    }
}

/// Public text position at an extended-grapheme boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    /// Paragraph node ID.
    pub node: NodeId,
    /// Zero-based extended grapheme boundary.
    pub grapheme_offset: u32,
    /// Mapping behavior at an equal insertion boundary.
    pub affinity: Affinity,
}

impl Position {
    pub(crate) fn to_internal(&self) -> Result<transaction::Position, SdkError> {
        Ok(transaction::Position {
            node: self.node.to_internal()?,
            grapheme_offset: self.grapheme_offset,
            affinity: self.affinity.to_internal(),
        })
    }

    fn from_internal(position: transaction::Position) -> Self {
        Self {
            node: NodeId::from_internal(position.node),
            grapheme_offset: position.grapheme_offset,
            affinity: Affinity::from_internal(position.affinity),
        }
    }
}

/// Directed logical text selection returned by a document session.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionSnapshot {
    /// Endpoint where the selection began.
    pub anchor: Position,
    /// Active selection endpoint.
    pub focus: Position,
}

impl SelectionSnapshot {
    /// Returns whether anchor and focus resolve to the same logical boundary.
    #[must_use]
    pub fn is_collapsed(&self) -> bool {
        self.anchor.node == self.focus.node
            && self.anchor.grapheme_offset == self.focus.grapheme_offset
    }

    pub(crate) fn from_internal(selection: selection::TextSelection) -> Self {
        Self {
            anchor: Position::from_internal(selection.anchor()),
            focus: Position::from_internal(selection.focus()),
        }
    }
}

/// A public ordered text range.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Range {
    /// Inclusive start boundary.
    pub start: Position,
    /// Exclusive end boundary.
    pub end: Position,
}

impl Range {
    pub(crate) fn to_internal(&self) -> Result<transaction::Range, SdkError> {
        Ok(transaction::Range {
            start: self.start.to_internal()?,
            end: self.end.to_internal()?,
        })
    }
}
