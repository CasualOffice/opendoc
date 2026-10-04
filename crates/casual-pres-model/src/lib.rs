// SPDX-License-Identifier: Apache-2.0

//! The normalized presentation model: slides, layouts, masters, placeholder slots
//! and the surface they are laid out on.
//!
//! # Where this sits
//!
//! This is the second **document class** in the runtime, designed in
//! `docs/156-PRESENTATION-SUPPORT-AND-SHARED-DRAWING-CORE-DESIGN.md` and settled by
//! ADR-055. It is strictly additive: `casual_doc_model::v1::Document` is not
//! modified, nothing existing imports this crate, and a DOCX caller cannot observe
//! that it exists.
//!
//! # What is shared with the document model, and what is not
//!
//! Shared by direct reuse, not by a common supertype:
//!
//! | Reused from `casual_doc_model` | Why it transfers unchanged |
//! | --- | --- |
//! | [`NodeId`](casual_doc_model::NodeId), `IdSpace` | identity is a property of the id space, not of the document class |
//! | `v1::Definitions` | theme, media and font tables are the same tables |
//! | `v1::GroupChild` and the whole DrawingML vocabulary | `p:sp`/`p:pic`/`p:grpSp` **are** `a:xfrm` + geometry + fill + line |
//! | `v1::visit_definition_node_ids`, `v1::visit_group_child_node_ids` | one traversal, so the two classes cannot disagree |
//!
//! Not shared, because a deck has no flow: pagination, sections, headers and
//! footers as page furniture, and the `BlockNode` body itself.
//!
//! # What is genuinely new here
//!
//! [`Placeholder`] — the slot a shape inherits position, size and text properties
//! through. It is the reason this crate exists rather than a `Document` profile: a
//! slide shape states only what it overrides, so discarding the slot would discard
//! the geometry of nearly every shape in a real deck.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod error;
mod ids;
mod layout_kind;
mod placeholder;
mod presentation;
mod shape_tree;
mod slide_size;

pub use error::{PresentationError, SlideAxis};
pub use ids::{SlideId, SlideLayoutId, SlideMasterId};
pub use layout_kind::LayoutKind;
pub use placeholder::{Placeholder, PlaceholderKind, PlaceholderOrientation, PlaceholderSize};
pub use presentation::{Presentation, SCHEMA_VERSION, Slide, SlideLayout, SlideMaster};
pub use shape_tree::{ShapeTree, SlideNode};
pub use slide_size::{MAX_SLIDE_EMU, MIN_SLIDE_EMU, SlideSize, SlideSizeKind};

#[cfg(test)]
mod tests;
