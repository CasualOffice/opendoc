// SPDX-License-Identifier: Apache-2.0

//! The OpenDoc layout, pagination, and display-list engine (Phases 1C–1E).
//!
//! This crate turns a [`casual_doc_model::v1::Document`] into an immutable
//! paginated layout and a backend-neutral display list, following the accepted
//! design in `docs/43-PHASE-1C-LAYOUT-RENDERING-DESIGN.md`. It is a production,
//! Word-grade engine delivered in slices — this module is the type spine and the
//! [`text::LineShaper`] seam; shaping (`parley`), the block/flow engine, the
//! paginator, and rendering backends land in following slices.
//!
//! Layering (`43-…` §3):
//! - [`units`] — device-independent geometry (everything computes in twips).
//! - [`quantity`] — the user-facing unit boundary (cm/mm/in/pt/pica to twips).
//! - [`text`] — line-level types + the [`text::LineShaper`] seam.
//! - [`block`] — block/flow fragments (the galley).
//! - [`chart`] — DrawingML chart composition from the typed projection (`docs/155`).
//! - [`arc`] — circular arcs and sectors as cubic path commands (`docs/155` §7.4).
//! - [`page`] — immutable paginated output.
//! - [`display`] — the backend-neutral paint list.
//! - [`formatting_marks`] — the paint-only non-printing-character overlay.
//! - [`model`] — layout-side anchors back into the document model.
//! - [`hittest`] — the read-only editing bridge (pixel↔model position).
//!
//! The engine owns layout/pagination/hit-testing; hosts own windows and paint a
//! [`display::DisplayList`] with a `casual-doc-render` backend (`00-README.md`).

#![forbid(unsafe_code)]

pub mod anchor;
// Own line (anti-conflict): circular arcs as cubic paths, shared by charts and
// the preset-shape table (`docs/155` §7.4).
pub mod arc;
pub mod block;
pub mod cascade;
// Own line (anti-conflict): chart composition from the typed projection.
pub mod chart;
pub mod columns;
pub mod compose;
pub mod display;
pub mod document_layout;
pub mod flow;
// Own line (anti-conflict): the per-viewer fold filter (ADR-049).
pub mod fold;
pub mod font_registry;
// Own line (anti-conflict): the paint-only non-printing-character overlay.
pub mod font_substitution;
pub mod fonts;
pub mod formatting_marks;
pub mod hittest;
pub mod incremental;
mod line_number;
pub mod measure;
pub mod model;
mod note_numbering;
pub mod notes;
pub mod numbering;
pub mod page;
mod page_border;
pub mod paginate;
pub mod paint_values;
// Own line (anti-conflict): the user-facing measurement-unit layer.
pub mod quantity;
// Own line (anti-conflict): what a reflowed column approximates in a document.
mod reflow_report;
// Own line (anti-conflict): per-table horizontal scrolling in a reflowed column.
pub mod reflow_scroll;
pub mod resolve;
pub mod running;
pub mod script;
pub mod shape;
pub mod symbol_map;
mod table_float;
pub mod tabs;
pub mod text;
pub mod text_region;
pub mod units;
mod watermark;
pub mod windowed;
// Own line (anti-conflict): the single wrap-side/exclusion-width rule.
mod wrap_side;
// Own line (anti-conflict): tight/through wrap to the authored contour.
mod wrap_contour;
