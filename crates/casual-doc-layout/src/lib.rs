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
//! - [`text`] — line-level types + the [`text::LineShaper`] seam.
//! - [`block`] — block/flow fragments (the galley).
//! - [`page`] — immutable paginated output.
//! - [`display`] — the backend-neutral paint list.
//! - [`model`] — layout-side anchors back into the document model.
//! - [`hittest`] — the read-only editing bridge (pixel↔model position).
//!
//! The engine owns layout/pagination/hit-testing; hosts own windows and paint a
//! [`display::DisplayList`] with a `casual-doc-render` backend (`00-README.md`).

#![forbid(unsafe_code)]

pub mod block;
pub mod compose;
pub mod display;
pub mod flow;
pub mod fonts;
pub mod hittest;
pub mod incremental;
pub mod model;
pub mod page;
pub mod paginate;
pub mod resolve;
pub mod running;
pub mod section;
pub mod shape;
pub mod tabs;
pub mod text;
pub mod units;
