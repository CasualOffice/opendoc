//! Layout-side references back into the document model.
//!
//! A laid-out line/fragment must know which model positions it covers so that
//! hit-testing (pixel → caret) and painting (caret → pixel) can bridge layout
//! and model. These are minimal anchors keyed on the v1 [`NodeId`]; when
//! interactive hit-testing lands (Phase 1E) they will be reconciled with the
//! canonical `casual-doc-selection` position type (which currently targets v0).

use casual_doc_model::NodeId;
use serde::{Deserialize, Serialize};

/// A position within the document model: an offset into a node's text.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ModelPos {
    /// The block/inline node this position is within.
    pub node: NodeId,
    /// A UTF-8 byte offset within the node's text (0 at its start).
    pub offset: u32,
}

impl ModelPos {
    /// A position at `offset` within `node`.
    #[must_use]
    pub const fn new(node: NodeId, offset: u32) -> Self {
        Self { node, offset }
    }
}

/// A half-open range of model positions `[start, end)`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ModelRange {
    /// Inclusive start.
    pub start: ModelPos,
    /// Exclusive end.
    pub end: ModelPos,
}

impl ModelRange {
    /// A range from `start` to `end`.
    #[must_use]
    pub const fn new(start: ModelPos, end: ModelPos) -> Self {
        Self { start, end }
    }
}
