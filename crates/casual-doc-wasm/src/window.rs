//! What the viewer holds instead of a whole `PaginatedLayout`.
//!
//! `docs/113` §8 item 1. The engine can lay out a 1,303,306-paragraph document
//! in 1.23 GiB instead of 4.14 GiB (`docs/113` §6.4), but only if the host stops
//! asking for every page at once. This module is the host side of that: one type
//! that is *either* the whole document's pages or one window of them, and that
//! answers an absolute page index the same way in both cases.
//!
//! ## The rule this type exists to enforce
//!
//! A page outside the resident window is **not** an empty page. Every accessor
//! here either hands back the page it was asked for or says it has not got it —
//! never a silently blank `Page`, which `AGENTS.md`'s "no silent data loss"
//! forbids outright and which is the specific failure `docs/113` §8 warns about.
//! `page_at` is therefore `Option`, and the callers that must not fail move the
//! window first with `ensure_resident`.
//!
//! ## Every consumer that wants more than one page, and what it does now
//!
//! `docs/113` §8: "each such consumer must either re-materialise what it needs
//! or refuse loudly — never quietly return nothing. Enumerate them; do not
//! discover them by accident." This is the enumeration. It is exhaustive by
//! construction: `WasmDocument::painted_layout`, `WasmDocument::body_page_at`
//! and `WasmDocument::editing_layout` are the ONLY ways into the page list, so
//! every row below is a call site of one of the three (or of the model, which
//! is fully resident and unaffected).
//!
//! | consumer | wasm surface | windowed answer |
//! | --- | --- | --- |
//! | page count | `pageCount` | **Re-materialised** — the measure tier's exact count, which is the whole document's |
//! | page count, again | `documentStats().pages` | same accessor, same count |
//! | `PAGE` / `NUMPAGES` fields | resolved onto each page | `window_of` resolves `NUMPAGES` from the measure tier's total, not the window's length |
//! | rasterize a page | `renderPage(i)` | **Re-materialised** — moves the window to `i` first, so every page of the document rasterizes, in any order |
//! | page geometry | `pageSize(i)` | **Re-materialised** from the outline, *without* moving the window (a host asks this once per page before rendering anything) |
//! | print every page | host loops `renderPage(0..pageCount)` | each call moves the window; slow, correct, never blank |
//! | page thumbnails | `renderPage` | as above |
//! | export (DOCX / ODT / TXT / JSON / HTML) | `exportDocx`, `exportAs` | **Unaffected** — written from the model, which stays whole; the layout is never consulted |
//! | find / find next | `findText` | **Unaffected** — model text, all surfaces |
//! | accessibility mirror | `accessibilityTree`, `accessibilityTreeWindow` | **Unaffected** — a model projection, already windowed for its own reasons |
//! | word and paragraph counts | `documentStats` | **Unaffected** — counted over the model's text on every surface |
//! | any edit (all 47 operations) | everything through `apply_group` | **Refused loudly**, before the model changes — see `windowed_not_available` |
//! | show-changes preview | `setShowChanges(true)` | **Refused loudly** — it is a second whole-document layout |
//! | font registration | `registerFallbackFont` → `repaginate` | **Re-measured**: one more shaping pass at the bounded peak, and the window is rebuilt where it was |
//! | dirty-page set after an edit | `finish_edit` | unreachable — editing is refused above it |
//! | caret and selection rectangles, table and checklist chrome, object boxes, marker rects, vertical caret movement, running bands | `caretRect`, `selectionRects`, `hitTest`, … | **Window-local, and that is the whole question**: these convert between screen pixels and the model, and the pixels are the window. A page that was never painted has no rectangles to be wrong about. They look pages up by `Page::number`, so they can never answer about the wrong page |
//!
//! The one thing no row does is return an empty page for a page that exists.
//! `crates/casual-doc-wasm`'s
//! `every_page_of_a_windowed_document_equals_the_whole_layout_field_for_field`
//! and `a_page_outside_the_window_rasterizes_to_the_same_pixels` assert that
//! positively: every page index of a windowed document equals — field for
//! field, and pixel for pixel — the same page of a layout built whole.
//!
//! ## What windowing costs, and why it is only used when it has to be
//!
//! A windowed body cannot answer a question about a page it has not built, and
//! re-deriving the whole document's layout after an edit is exactly the peak
//! this path exists to avoid. So the viewer opens **whole** whenever the whole
//! document has been measured to fit, and windows only above that — where the
//! alternative is not a smaller feature set but a refusal to open at all. The
//! threshold lives in `lib.rs` next to the ceiling it is measured against.

use casual_doc_layout::incremental::PageRange;
use casual_doc_layout::page::Page;
use casual_doc_layout::page::PaginatedLayout;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::units::Size;
use casual_doc_layout::windowed::DocumentMeasures;
use casual_doc_layout::windowed::NotWindowable;
use casual_doc_layout::windowed::WindowPolicy;
use casual_doc_layout::windowed::extend_measures;
use casual_doc_layout::windowed::measure_document_prefix;
use casual_doc_layout::windowed::window_of;
use casual_doc_model::v1::Document;

/// The body pages the viewer is working from.
#[derive(Debug)]
pub(crate) enum BodyLayout {
    /// Every page is resident. Identical in behaviour to what the viewer did
    /// before windowing existed, and what it still does for every document that
    /// fits — which is every document it could previously open.
    Whole(PaginatedLayout),
    /// Only one window of pages is resident; the rest are re-derivable from the
    /// measure tier on demand.
    Windowed(Box<WindowedBody>),
}

impl BodyLayout {
    /// The document's **real** page count, windowed or not.
    ///
    /// For a windowed body this is the measure tier's count, which `docs/113`
    /// §6.4 measured as identical to `paginate_document`'s at 29,621 pages —
    /// *not* the number of pages currently resident, which would make a
    /// 29,621-page document report 5.
    pub(crate) fn page_count(&self) -> usize {
        match self {
            Self::Whole(layout) => layout.page_count(),
            Self::Windowed(body) => body.measures.page_count(),
        }
    }

    /// Whether [`page_count`](Self::page_count) is the document's page count
    /// rather than a measured prefix's.
    ///
    /// A whole body is always exact: every page exists. A windowed one is
    /// exact once its background measure has reached the end.
    pub(crate) fn page_count_is_exact(&self) -> bool {
        match self {
            Self::Whole(_) => true,
            Self::Windowed(body) => body.measures.is_complete(),
        }
    }

    /// The page count when it is known, and an estimate scaled from the
    /// measured prefix while it is not. Paired with
    /// [`page_count_is_exact`](Self::page_count_is_exact), never presented
    /// alone.
    pub(crate) fn estimated_page_count(&self) -> usize {
        match self {
            Self::Whole(layout) => layout.page_count(),
            Self::Windowed(body) => body.measures.estimated_page_count(),
        }
    }

    /// Measures the next `block_budget` blocks of a windowed body. Returns
    /// whether the document is now measured whole; a whole body was never
    /// partial and answers `true`.
    pub(crate) fn extend(
        &mut self,
        document: &Document,
        shaper: &ParleyShaper,
        block_budget: usize,
    ) -> bool {
        match self {
            Self::Whole(_) => true,
            Self::Windowed(body) => {
                extend_measures(&mut body.measures, document, shaper, block_budget)
            }
        }
    }

    /// The pages that are resident right now — the ones a geometry query can be
    /// answered from.
    ///
    /// Every `Page` in here carries its own absolute `number`, so a consumer
    /// that looks a page up by number (hit-testing, running bands) is correct
    /// against a window without knowing one exists. A consumer that indexes
    /// positionally must go through `page_at` instead.
    pub(crate) fn resident(&self) -> &PaginatedLayout {
        match self {
            Self::Whole(layout) => layout,
            Self::Windowed(body) => &body.resident,
        }
    }

    /// The page BOX of absolute page `index`, answered without materializing
    /// the page.
    ///
    /// This matters more than it looks. A host builds one wrapper per page
    /// before it renders anything, and it asks each wrapper's size — 29,621
    /// times on the owner's file. Routing that through the window made every
    /// one of those calls re-flow and re-paginate from a checkpoint: measured
    /// at 121.1 s to open 262,146 blocks against 35.8 s for the whole path,
    /// which is the whole saving spent on a question the measure tier already
    /// holds the answer to. A windowed document has exactly one section (more
    /// is [`NotWindowable::MultipleSections`]), so every page's box is in its
    /// outline and no page has to exist to report it.
    pub(crate) fn page_box_at(&self, index: usize) -> Option<Size> {
        match self {
            Self::Whole(layout) => layout.pages.get(index).map(|page| page.page_size),
            Self::Windowed(body) => body.measures.pages.get(index).map(|page| page.page_size),
        }
    }

    /// The page at absolute index `index`, if it is resident.
    ///
    /// `None` means "not resident", never "empty page". A caller that needs the
    /// page rather than an answer about the current window must call
    /// `ensure_resident` first.
    pub(crate) fn page_at(&self, index: usize) -> Option<&Page> {
        match self {
            Self::Whole(layout) => layout.pages.get(index),
            Self::Windowed(body) => body.page_at(index),
        }
    }

    /// Moves the window so that absolute page `index` is resident. A no-op for a
    /// whole body, and a no-op for a windowed body already covering `index`.
    ///
    /// Returns whether a window was actually built, which is what the scroll
    /// guard counts.
    pub(crate) fn ensure_resident(
        &mut self,
        document: &Document,
        shaper: &ParleyShaper,
        index: usize,
    ) -> bool {
        match self {
            Self::Whole(_) => false,
            Self::Windowed(body) => body.ensure(document, shaper, index),
        }
    }

    /// Whether this body is windowed — the one question the capabilities a
    /// window cannot serve have to ask.
    pub(crate) const fn is_windowed(&self) -> bool {
        matches!(self, Self::Windowed(_))
    }

    /// The windowed body, if this is one. Test-only: production code asks
    /// `is_windowed` or goes through the accessors above.
    #[cfg(test)]
    pub(crate) fn windowed(&self) -> Option<&WindowedBody> {
        match self {
            Self::Whole(_) => None,
            Self::Windowed(body) => Some(body),
        }
    }

    /// Re-derives the layout after something font-shaped changed underneath it.
    ///
    /// A whole body re-paginates (its caller does that directly); a windowed
    /// body re-measures and rebuilds the window it was showing. Both are a full
    /// shaping pass — which is what a font registration costs either way — but
    /// the windowed one keeps the bounded peak.
    pub(crate) fn remeasure(&mut self, document: &Document, shaper: &ParleyShaper) {
        let Self::Windowed(body) = self else {
            return;
        };
        let Ok(measures) = measure_document_prefix(document, shaper, OPEN_BLOCK_BUDGET) else {
            // Unreachable in practice: every `NotWindowable` reason is a property
            // of the document (sections, columns, footnotes, anchored objects,
            // line numbering, per-page note restart), none of which registering a
            // font can change, and the document was windowable when it opened.
            // Keeping the previous measures is the safe branch — the page list
            // stays one the engine really produced — where a panic here would
            // abort the module.
            return;
        };
        let at = body.range.start;
        body.measures = measures;
        body.build(document, shaper, at);
    }
}

/// How many top-level blocks are measured before the first frame.
///
/// `docs/116` §3 measured the measure pass at ~6 µs/block natively and ~13 µs
/// in the browser, so this is the open path's time budget written as the only
/// unit the engine can enforce it in: ~260 ms of main thread in a browser,
/// whatever the document's length. The rest is measured by
/// [`BodyLayout::extend`] between frames.
///
/// Deliberately far above a screenful. A budget sized to the first window
/// would make the second window wait for a measure, and the point is that
/// scrolling stays ahead of the reader; this one covers a document of ordinary
/// length outright, so nothing but a very long one ever opens partial.
const OPEN_BLOCK_BUDGET: usize = 20_000;

/// A document whose pages are derived one window at a time.
#[derive(Debug)]
pub(crate) struct WindowedBody {
    /// Every page's boundaries, and the checkpoints any page is re-derivable
    /// from. This is the resident half — 1,021 B/paragraph measured at 1.3M
    /// paragraphs (`docs/113` §6.4).
    measures: DocumentMeasures,
    /// Byte budget and lead pages (`docs/113` §4 Q3).
    policy: WindowPolicy,
    /// The materialized window. Its `pages[i].number` is ABSOLUTE; its position
    /// in the vector is `i - range.start`.
    resident: PaginatedLayout,
    /// The absolute page range `resident` covers.
    range: PageRange,
}

impl WindowedBody {
    /// Measures `document` and materializes the window around its first page.
    ///
    /// # Errors
    ///
    /// [`NotWindowable`] when a post-pagination pass could move a page boundary
    /// — footnotes, anchored floats, multiple sections or columns, margin line
    /// numbering, per-page note restart. Such a document is not refused by this
    /// returning `Err`; it is laid out whole instead (see `open_document_as`).
    pub(crate) fn open(document: &Document, shaper: &ParleyShaper) -> Result<Self, NotWindowable> {
        let measures = measure_document_prefix(document, shaper, OPEN_BLOCK_BUDGET)?;
        let mut body = Self {
            measures,
            policy: WindowPolicy::default(),
            resident: PaginatedLayout { pages: Vec::new() },
            range: PageRange::new(0, 0),
        };
        body.build(document, shaper, 0);
        Ok(body)
    }

    /// The absolute page range currently resident. Test-only.
    #[cfg(test)]
    pub(crate) const fn range(&self) -> PageRange {
        self.range
    }

    fn page_at(&self, index: usize) -> Option<&Page> {
        if index < self.range.start || index >= self.range.end {
            return None;
        }
        self.resident.pages.get(index - self.range.start)
    }

    fn ensure(&mut self, document: &Document, shaper: &ParleyShaper, index: usize) -> bool {
        if index >= self.range.start && index < self.range.end {
            return false;
        }
        self.build(document, shaper, index);
        true
    }

    /// Builds the window around absolute page `index`.
    fn build(&mut self, document: &Document, shaper: &ParleyShaper, index: usize) {
        let total = self.measures.page_count();
        let start = index.min(total.saturating_sub(1));
        let visible = PageRange::new(start, (start + 1).min(total));
        let window = window_of(document, shaper, &self.measures, visible, self.policy);
        self.range = window.range;
        self.resident = PaginatedLayout {
            pages: window
                .viewport
                .pages
                .into_iter()
                .map(|visible_page| visible_page.page)
                .collect(),
        };
    }
}
