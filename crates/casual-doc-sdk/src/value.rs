//! Core public value objects shared across the SDK surface.

use std::fmt;

use casual_doc_model as model;
use serde::{Deserialize, Serialize};

use crate::error::{ErrorCode, ErrorSeverity, SdkError};

/// Stable host-visible node identity.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct NodeId(String);

impl NodeId {
    pub(crate) fn from_internal(id: model::NodeId) -> Self {
        Self(id.to_string())
    }

    pub(crate) fn to_internal(&self) -> Result<model::NodeId, SdkError> {
        self.0.parse().map_err(|_| {
            SdkError::new(
                ErrorCode::InvalidArgument,
                ErrorSeverity::Error,
                "node ID is invalid",
            )
        })
    }

    /// Returns the fixed-width lowercase hexadecimal representation.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Session-local document revision.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Revision(pub(crate) u64);

impl Revision {
    /// Creates a revision value.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the numeric revision.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Inline marks accepted by the initial insertion command.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mark {
    /// Bold text.
    Bold,
    /// Italic text.
    Italic,
    /// Underlined text.
    Underline,
    /// Struck-through text.
    Strike,
}

impl Mark {
    pub(crate) fn to_internal(self) -> model::Mark {
        match self {
            Self::Bold => model::Mark::Bold,
            Self::Italic => model::Mark::Italic,
            Self::Underline => model::Mark::Underline,
            Self::Strike => model::Mark::Strike,
        }
    }

    pub(crate) fn from_internal(mark: model::Mark) -> Self {
        match mark {
            model::Mark::Bold => Self::Bold,
            model::Mark::Italic => Self::Italic,
            model::Mark::Underline => Self::Underline,
            model::Mark::Strike => Self::Strike,
        }
    }
}
