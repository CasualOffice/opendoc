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

/// A block fragment placed at an absolute rectangle on a page.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PlacedFragment {
    /// The fragment (or the portion of it placed on this page).
    pub fragment: BlockFragment,
    /// Its rectangle in page-local twip coordinates.
    pub rect: Rect,
}

/// One laid-out page.
#[derive(Clone, Debug, Deserialize, Serialize)]
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
    /// Footnotes placed at the bottom of this page.
    pub footnotes: Vec<PlacedFragment>,
    /// First model position on this page (the stabilization-halt key).
    pub start: ModelPos,
    /// One-past-last model position on this page.
    pub end: ModelPos,
}

/// The full paginated layout — the immutable result consumed by rendering and
/// hit-testing.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
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
