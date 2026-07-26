//! Paginated output — the immutable page fragments the paginator produces.
//!
//! Following the LayoutNG discipline (`42-…` §1.4), the paginator *produces*
//! immutable [`Page`]s and painting + hit-testing are read-only walks of them.
//! Each page records the model range it spans and the placement of every
//! fragment, which is exactly what makes incremental re-pagination (the
//! stabilization halt) and hit-testing cheap (`43-…` §3.5, §9).

use casual_doc_model::v1::SectionId;
use serde::{Deserialize, Serialize};

use crate::block::BlockFragment;
use crate::model::ModelPos;
use crate::units::Rect;

/// A position in the galley's flow: a fragment (by index) and a line offset
/// within it (`0` for a whole fragment or a split paragraph's first chunk).
///
/// This is the *carry state* at a page boundary. Because every page begins at a
/// fresh content-top cursor, the flow position of a page's first content is the
/// only state that determines everything below it — so two paginations that
/// reach the same [`FlowPos`] over identical downstream content lay out
/// identically from there. That is the key the incremental paginator matches on
/// to reuse pages unchanged (the stabilization halt, `43-…` §3.4).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Deserialize, Serialize)]
pub struct FlowPos {
    /// Index of the fragment in the galley.
    pub fragment: u32,
    /// Line offset within that fragment (0 unless a paragraph was split).
    pub line: u32,
}

impl FlowPos {
    /// The flow position at galley index `fragment`, line 0.
    #[must_use]
    pub fn at(fragment: u32) -> Self {
        Self { fragment, line: 0 }
    }
}

/// The half-open span of the galley a page covers, `[start, end)`: `start` is
/// the flow position of the page's first content and `end` is the position of
/// the first content *not* on the page (i.e. the next page's `start`).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct FlowSpan {
    /// Flow position of the page's first content.
    pub start: FlowPos,
    /// Flow position one past the page's last content.
    pub end: FlowPos,
}

/// A block fragment placed at an absolute rectangle on a page.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct PlacedFragment {
    /// The fragment (or the portion of it placed on this page).
    pub fragment: BlockFragment,
    /// Its rectangle in page-local twip coordinates.
    pub rect: Rect,
}

/// One laid-out page.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct Page {
    /// 1-based page number.
    pub number: u32,
    /// The section whose geometry + header/footer set applies to this page.
    pub section: SectionId,
    /// The content area (page box minus margins, header/footer, and any
    /// footnote reservation), in page-local twips.
    pub content_area: Rect,
    /// Fragments placed in the content area, in flow order.
    pub placed: Vec<PlacedFragment>,
    /// The running header laid out in the top band (the per-page-selected
    /// header for this page's number + section). Empty until the running-content
    /// pass ([`crate::running::place_running_content`]) fills it; kept off the
    /// pagination hot path so page reuse stays field-value-free.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub header: Vec<PlacedFragment>,
    /// The running footer laid out in the bottom band (see [`Page::header`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub footer: Vec<PlacedFragment>,
    /// Footnotes placed at the bottom of this page.
    pub footnotes: Vec<PlacedFragment>,
    /// First model position on this page (the stabilization-halt key).
    pub start: ModelPos,
    /// One-past-last model position on this page.
    pub end: ModelPos,
    /// The half-open galley span this page covers — the flow provenance the
    /// incremental paginator uses to reuse pages (`43-…` §3.4).
    pub flow: FlowSpan,
}

/// The full paginated layout — the immutable result consumed by rendering and
/// hit-testing.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct PaginatedLayout {
    /// Pages in order.
    pub pages: Vec<Page>,
}

impl PaginatedLayout {
    /// The number of pages.
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }
}
