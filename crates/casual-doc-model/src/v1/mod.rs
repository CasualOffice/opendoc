//! Normalized document schema v1: typed properties, styles, numbering,
//! sections, theme and media references, strict validation, and a deterministic
//! total v0-to-v1 migration.
//!
//! v1 is additive: the crate-root v0 model is unchanged and remains the runtime
//! edit model this slice. v1 is the import/export and migration target
//! (`38-NORMALIZED-SCHEMA-V1-DESIGN.md`, ADR-027).

mod body;
mod definitions;
mod document;
mod ids;
mod metadata;
mod migration;
mod properties;
mod table;

pub use body::*;
pub use definitions::*;
pub use document::*;
pub use ids::*;
pub use metadata::*;
pub use migration::*;
pub use properties::*;
pub use table::*;

#[cfg(test)]
mod tests;
