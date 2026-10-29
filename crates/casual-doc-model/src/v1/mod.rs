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
// DrawingML geometry as data, the engine that draws it, and the standard's 187
// preset definitions read into the same structure (`docs/109` FID-L-04,
// FID-G-02; `docs/119`).
mod custom_geometry;
mod definitions;
mod document;
// Own line (anti-conflict): even-odd to non-zero re-winding (`docs/182` G-2).
mod fill_rule;
mod geometry_program;
mod ids;
mod intern;
// Own line (anti-conflict): legacy VML retained beside its projection.
mod legacy_vml;
// Resolving a paragraph by id in O(1) in document size (`docs/109` HF-184): a bounded,
// self-validating route cache on the document. Its own module because the interesting
// part is the argument for why a hint needs no invalidation, and that argument is worth
// one screen beside the code rather than a line in `document.rs`.
mod locate;
mod metadata;
mod migration;
mod numbering;
// Own line (anti-conflict): the plain-text projection, moved down from layout.
mod plain_text;
mod preset_shapes;
mod properties;
mod table;
// The normalized grid map: grid coordinates to the cell that owns them, the one
// translation every table feature reads (`docs/171`).
mod table_grid;

pub use body::*;
pub use chart::*;
pub use containers::*;
pub use custom_geometry::*;
pub use definitions::*;
pub use document::*;
// Own line (anti-conflict).
pub use fill_rule::*;
pub use geometry_program::*;
pub use ids::*;
pub use intern::*;
pub use legacy_vml::*;
pub use locate::{reset_route_block_visits, route_block_visits};
pub use metadata::*;
pub use migration::*;
pub use numbering::*;
pub use plain_text::*;
pub use preset_shapes::*;
pub use properties::*;
pub use table::*;
pub use table_grid::*;

#[cfg(test)]
mod tests;
