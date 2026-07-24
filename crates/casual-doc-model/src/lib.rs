//! Normalized document values and invariants used inside the OpenDoc runtime.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod body;
mod document;
mod error;
mod extension;
mod ids;
mod snapshot;

pub mod v1;

pub use body::{BlockNode, InlineNode, Mark, Paragraph, TextRun};
pub use document::Document;
pub use error::ModelError;
pub use extension::ExtensionValue;
pub use ids::{IdGenerator, NodeId};
pub use snapshot::{SnapshotError, SnapshotLimits};

pub(crate) use snapshot::enforce_limit;

/// The normalized document schema implemented at the crate root (v0).
pub const SCHEMA_VERSION: u32 = 0;

#[cfg(test)]
mod tests;
