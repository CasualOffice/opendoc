//! Normalized document schema v1: typed properties, styles, numbering,
//! sections, theme and media references, strict validation, and a deterministic
//! total v0-to-v1 migration.
//!
//! v1 is additive: the crate-root v0 model is unchanged and remains the runtime
//! edit model this slice. v1 is the import/export and migration target
//! (`38-NORMALIZED-SCHEMA-V1-DESIGN.md`, ADR-027).

mod body;
mod chart;
// The inline container set, declared once for the whole workspace (`docs/109`
// HF-212). Its own module for the reason the `docs/109` row gives: the exhaustive
// match that makes a new `InlineNode` variant a COMPILE ERROR is one screen rather
// than buried in `body.rs`, and the three descent axes are documented where a
// reader will find them. `crate::container_audit` is the guard that holds every
// consumer to it.
mod containers;
mod definitions;
mod document;
mod ids;
mod intern;
// Resolving a paragraph by id in O(1) in document size (`docs/109` HF-184): a bounded,
// self-validating route cache on the document. Its own module because the interesting
// part is the argument for why a hint needs no invalidation, and that argument is worth
// one screen beside the code rather than a line in `document.rs`.
mod locate;
mod metadata;
mod migration;
mod numbering;
mod properties;
mod table;

pub use body::*;
pub use chart::*;
pub use containers::*;
pub use definitions::*;
pub use document::*;
pub use ids::*;
pub use intern::*;
pub use locate::{reset_route_block_visits, route_block_visits};
pub use metadata::*;
pub use migration::*;
pub use numbering::*;
pub use properties::*;
pub use table::*;

#[cfg(test)]
mod tests;
