//! The one-call layout driver — turning a [`Document`] into a finished
//! [`PaginatedLayout`](crate::page::PaginatedLayout).
//!
//! Rendering a real document is a fixed pipeline of already-built pieces: derive
//! the page geometry from the section, build the body galley, flow the section's
//! headers/footers, paginate, then run the three post-pagination passes (running
//! content, page-number fields, anchored drawings) in the order the
//! incremental-golden invariant requires. [`paginate_document`] wires them
//! together so callers — the viewer and the fidelity harness — get a
//! ready-to-render layout from a single call, at the document's *true* geometry
//! (its real page size and margins), rather than the hand-built US-Letter
//! `PageConfig` every call site used to carry.
//!
//! It composes the existing engine pieces only; it does **not** touch the
//! incremental/halt core of [`crate::paginate`]. Every step is the same public
//! function the manual wiring calls (verified by the `equals_manual_wiring`
//! regression test), so the driver can never drift from the pipeline it replaces.
//!
//! ## Sections and column-aware flow
//!
//! The body is partitioned into one run per section
//! ([`Definitions::sections`](casual_doc_model::v1::Definitions::sections)); each
//! run is flowed at *its own* column width and paginated by
//! [`crate::columns`], which fills a section's columns in order and carries the
//! page cursor across section boundaries (a `continuous` section shares the
//! previous section's page). This is what flows a `w:cols` document into newspaper
//! columns instead of one full-width column. A document with no sections at all
//! (which a valid imported DOCX never is — Word always writes a trailing `sectPr`)
//! falls back to a single full-width run under US-Letter with 1-inch margins.
//!
//! Header/footer variants and band reservations are resolved independently for
//! every section. Each produced page selects the running-content plan identified
//! by its immutable [`Page::section`](crate::page::Page::section), and `titlePg`
//! is evaluated against that section's first page rather than only document page
//! one. Per-section balancing of the last column page remains a documented
//! deferral (see [`crate::columns`]).
//!
//! Two consequences of keying on `Page::section` are worth naming, because they
//! decide *which pages a header appears on*:
//!
//! - A blank page inserted for `evenPage`/`oddPage` parity is charged to the
//!   **preceding** section (see [`crate::columns`]), so it keeps that section's
//!   running content and that section's page geometry — which is Word's behavior
//!   and the reason the pad is emitted before the new section's config is
//!   entered.
//! - When a `continuous` section shares a page with the section before it, the
//!   page is charged to the **last** section that placed content on it, so that
//!   section's bands are the ones painted. One page has one header, so some rule
//!   has to break the tie; this one is recorded rather than accidental.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Document, HeaderFooterKind, HeaderFooterRef, NoteId, NoteKind, NotePosition,
    PageBorders, PageVerticalAlignment, SectionBoundary,
};

use crate::anchor::{body_wrap_rects, header_float_reserve_for_section, place_floats};
use crate::block::{BlockFragment, CellFragment, CellVerticalMerge};
// Separate `use` line (kept out of the sorted block) so a parallel branch adding
// its own import does not collide here: `BreakControl` is read only by
// `suspend_page_break_constraints`.
use crate::block::BreakControl;
use crate::columns::{
    ColumnLayout, SectionRun, column_layout, paginate_columns, section_start_parity,
    section_starts_new_page,
};
use crate::flow::{
    NoteFlow, ParagraphFloatExclusion, ParagraphFloatExclusions, ReviewView,
    build_galley_cached_labeled, build_galley_for_blocks_inner, flow_header_footer_labeled,
    line_grid_for_section,
};
// Own line (anti-conflict): the per-viewer fold filter (ADR-049).
use crate::fold::FoldSet;
use crate::incremental::{DirtySet, GalleyCache};
use crate::note_numbering::{
    NoteLabels, note_props_for_section, resolve_note_labels, visit_block_note_refs,
};
use crate::notes::{paginate_section_footnotes, run_has_body_footnotes};
use crate::paginate::{
    PageConfig, page_number_labels, resolve_anchored_fields_labeled, resolve_fields_labeled,
};
use crate::running::{HeaderFooter, RunningContent, place_running_content_on_page};
use crate::units::{Point, Rect, Size, Twip, emu_to_twip_extent};
// Own line (anti-conflict): the single wrap-side rule.
use crate::wrap_side::band_exclusion;
use casual_doc_model::v1::SectionId;

/// US-Letter page size in twips (8.5in × 11in), the fallback for a document that
/// declares no section.
const LETTER: Size = Size {
    width: Twip(12_240),
    height: Twip(15_840),
};
/// A 1-inch margin in twips — the fallback margin when no section is declared.
const ONE_INCH: Twip = Twip(1_440);
/// Word's default header/footer band distance from the page edge (`w:pgMar`
/// `@w:header`/`@w:footer`), used when the attribute is absent.
const DEFAULT_BAND_DISTANCE: Twip = Twip(720);

/// The default height each [`LayoutView::Reflow`] tile is cut at — 11in, one
/// Letter page tall.
///
/// A tile is an artefact of rasterisation and should be invisible to the reader,
/// so the height is picked to be **large relative to a viewport** (a scroll
/// rarely crosses one) and **small relative to a canvas limit**
/// ([`crate::compose::compose_page`] rasterises one page into one surface, and a
/// browser canvas maxes out near 32,767px in either axis — which is why "one tall
/// page" was rejected). ONLYOFFICE's reader mode instead keeps the *original*
/// page height, so their reflow still breaks where the paper chose; ours is
/// chosen for the reader. `docs/151` §4.4 and §8 item 3 record this as a decision
/// with a stated basis rather than a constant with no source.
pub const DEFAULT_TILE_HEIGHT: Twip = Twip(15_840);

/// The narrowest reflow column that is still a reading column — 1in. Below this
/// line breaking degenerates (an ordinary word exceeds the measure on every line)
/// and the result is unreadable at any zoom, so it is refused rather than laid
/// out.
const MIN_REFLOW_COLUMN: Twip = Twip(1_440);
/// The widest reflow column — 22in, wider than any paper this engine supports. A
/// caller asking for more has converted units wrongly.
const MAX_REFLOW_COLUMN: Twip = Twip(31_680);
/// The shortest tile — 2in. Tile count is `document height / tile_height`, so a
/// shorter tile multiplies the per-tile raster and the post-pagination constant
/// by the same factor for no reader benefit.
const MIN_TILE_HEIGHT: Twip = Twip(2_880);
/// The tallest tile — 33in. At 33in a tile is 3,168px at 96dpi and 12,672px at a
/// 4x device ratio, both comfortably inside the ~32,767px canvas limit that
/// rejected "one tall page" in the first place (`docs/151` §4.4).
const MAX_TILE_HEIGHT: Twip = Twip(47_520);

/// What a `PAGE` or `NUMPAGES` field prints in a [`LayoutView::Reflow`] layout.
///
/// A reflow tile index is **not** a page number, and printing one as though it
/// were is the dishonest-number class this repository forbids outright: a reader
/// looking at "Page 7 of 34" in a reflowed document is being told something false
/// about a document that has no pages in this view. The engine has no paginated
/// value to substitute — it is not laying the document out on paper — so it
/// refuses with a token that cannot be mistaken for a count. `docs/151` §6.5
/// makes the host responsible for the better answer (carrying over the value from
/// the last `Paged` layout when it has one); this is the floor beneath it, and it
/// is here rather than left to the host because the host cannot un-print a number
/// the engine already shaped into a glyph run.
const REFLOW_FIELD_REFUSAL: &str = "—";

/// Which geometry a layout pass lays a document out in: the document's own paper,
/// or a reflowed column of the caller's width (ADR-046, `docs/151`).
///
/// This is a **view parameter and never a document edit.** Nothing here is
/// written back to a [`SectionBoundary`], no operation is issued, and the export
/// path cannot observe it. `casual-doc-wasm`'s `setPageSetup` would produce a
/// similar visual result today by issuing a section-geometry mutation, and must
/// not be used for this: it would pollute undo, dirty autosave, and persist a
/// 390px-wide "page" into the user's DOCX. That is the silent-data-loss class the
/// engineering priority order forbids, and it is the whole of ADR-046.
///
/// [`LayoutView::Paged`] is the [`Default`], and every entry point that does not
/// name a view delegates with it, so paged output is byte-for-byte what it was
/// before this type existed. Two guards hold that: `tests/reflow.rs`'s inertness
/// case (`paginate_document` and `paginate_document_in(.., Paged)` agree
/// page-for-page) and `geometry_snapshot.golden` not moving.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LayoutView {
    /// The document's own paper: every page's geometry comes from its own
    /// section's `w:pgSz`/`w:pgMar`.
    #[default]
    Paged,
    /// Lay the body out at `content_width` and cut the result into fixed-height
    /// tiles — Google's Pageless, ONLYOFFICE's reader mode, but still editable.
    ///
    /// Construct one with [`LayoutView::reflow`], which refuses geometry that is
    /// not a reading column instead of laying out something unusable.
    Reflow {
        /// The measure lines are broken at — the reader's column width. This is
        /// the number the whole design turns on: the flow engine is already
        /// width-parametric, so reflow is a question of where the width comes
        /// from and of nothing else.
        content_width: Twip,
        /// The height each tile is cut at. See [`DEFAULT_TILE_HEIGHT`]. It is an
        /// **upper** bound: the final pass trims every tile to its own content
        /// so the tiles drawn edge to edge read as one continuous column.
        tile_height: Twip,
        /// Padding on each side of the column, inside the tile, so the text does
        /// not run to the raster's edge.
        gutter: Twip,
    },
}

/// Why a [`LayoutView::reflow`] was refused. Every variant carries the offending
/// value and a sentence; none of them silently substitutes a bound, because a
/// substituted width lays the document out at a measure the caller did not ask
/// for and cannot see.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ReflowRefused {
    /// The column is narrower than one inch.
    ColumnTooNarrow(Twip),
    /// The column is wider than 22 inches.
    ColumnTooWide(Twip),
    /// The tile is shorter than two inches.
    TileTooShort(Twip),
    /// The tile is taller than 33 inches (a canvas limit).
    TileTooTall(Twip),
    /// The gutter is negative, or wider than the column it pads.
    GutterOutOfRange(Twip),
}

impl ReflowRefused {
    /// A sentence naming the value that was refused and the bound it missed, for
    /// a host to show a reader. Never a trap name.
    #[must_use]
    pub fn reason(self) -> String {
        match self {
            Self::ColumnTooNarrow(w) => format!(
                "a reflow column of {} twips is narrower than the {} twip (1in) minimum, which is \
                 the narrowest measure text still breaks into lines at",
                w.raw(),
                MIN_REFLOW_COLUMN.raw()
            ),
            Self::ColumnTooWide(w) => format!(
                "a reflow column of {} twips is wider than the {} twip (22in) maximum, which is \
                 wider than any paper this engine lays out",
                w.raw(),
                MAX_REFLOW_COLUMN.raw()
            ),
            Self::TileTooShort(h) => format!(
                "a reflow tile of {} twips is shorter than the {} twip (2in) minimum; a shorter \
                 tile multiplies the number of rasters for no reader benefit",
                h.raw(),
                MIN_TILE_HEIGHT.raw()
            ),
            Self::TileTooTall(h) => format!(
                "a reflow tile of {} twips is taller than the {} twip (33in) maximum, which is the \
                 tallest raster a browser canvas accepts",
                h.raw(),
                MAX_TILE_HEIGHT.raw()
            ),
            Self::GutterOutOfRange(g) => format!(
                "a reflow gutter of {} twips is out of range; it must be zero or more and no wider \
                 than the column it pads",
                g.raw()
            ),
        }
    }
}

impl core::fmt::Display for ReflowRefused {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.reason())
    }
}

impl std::error::Error for ReflowRefused {}

impl LayoutView {
    /// A validated [`LayoutView::Reflow`].
    ///
    /// # Errors
    ///
    /// [`ReflowRefused`] when the geometry is not a reading column. The bounds
    /// exist so an unconverted pixel value (390 twips is a quarter of an inch)
    /// or an inverted argument order is refused **at the seam**, rather than
    /// producing a layout nobody can read and no test would notice.
    ///
    /// Complexity: `O(1)`.
    pub fn reflow(
        content_width: Twip,
        tile_height: Twip,
        gutter: Twip,
    ) -> Result<Self, ReflowRefused> {
        if content_width < MIN_REFLOW_COLUMN {
            return Err(ReflowRefused::ColumnTooNarrow(content_width));
        }
        if content_width > MAX_REFLOW_COLUMN {
            return Err(ReflowRefused::ColumnTooWide(content_width));
        }
        if tile_height < MIN_TILE_HEIGHT {
            return Err(ReflowRefused::TileTooShort(tile_height));
        }
        if tile_height > MAX_TILE_HEIGHT {
            return Err(ReflowRefused::TileTooTall(tile_height));
        }
        if gutter < Twip::ZERO || gutter > content_width {
            return Err(ReflowRefused::GutterOutOfRange(gutter));
        }
        Ok(Self::Reflow {
            content_width,
            tile_height,
            gutter,
        })
    }

    /// Whether this view reflows — i.e. whether the page-shaped constraints are
    /// suspended, the page-shaped furniture suppressed, and the tiles trimmed.
    #[must_use]
    pub const fn is_reflow(self) -> bool {
        matches!(self, Self::Reflow { .. })
    }

    /// The approximations a [`LayoutView::Reflow`] pass knowingly makes, as
    /// sentences, so a host reports them instead of a reader discovering them.
    /// Empty for [`LayoutView::Paged`].
    ///
    /// These are recorded rather than hidden because each is visible to a reader
    /// and none is fixed by this increment (`docs/151` §8 item 1 stays open on
    /// the first of them).
    ///
    /// Complexity: `O(1)` — a fixed list, allocating only its strings.
    #[must_use]
    pub fn approximations(self) -> Vec<String> {
        if !self.is_reflow() {
            return Vec::new();
        }
        vec![
            "A drawing anchored to the page or to the margin keeps its paper-relative position, \
             because a reflow tile has no page edges for it to be relative to; it can therefore \
             sit away from the text it belongs with (docs/151 section 8 item 1, still open)."
                .to_owned(),
            "A footnote is placed at the bottom of the tile its reference lands in, which is a \
             raster boundary rather than a page bottom, so it can fall mid-thought rather than \
             under the page that cites it."
                .to_owned(),
            "A PAGE or NUMPAGES field prints a refusal token rather than a number, because a tile \
             index is not a page number (docs/151 section 6.5)."
                .to_owned(),
        ]
    }
}

/// The synthetic [`PageConfig`] every tile of a [`LayoutView::Reflow`] pass is
/// laid out in — the reflow counterpart of [`section_page_config`], and the
/// *only* place reflow geometry is invented. `None` under [`LayoutView::Paged`],
/// so a caller reads "the section's own geometry stands" from the type.
///
/// This is ONLYOFFICE's mechanism (`CDocumentReadView::Set` builds a synthetic
/// section with a substituted page size and tiny margins and hands it to the
/// existing paginator), and it is deliberately not a second layout path.
///
/// `section` is the caller's own section id, so `Page::section` still resolves to
/// a real boundary for every consumer that looks one up, and the per-section plan
/// lookup keeps working. The **vertical** metrics are NOT the section's, which is
/// where this deviates from what `docs/151` §4.5 row 1 originally specified — and
/// the deviation is the point: a tile's top and bottom edges are not page edges,
/// so a section top margin would paint a one-inch white band across the middle of
/// a paragraph every 11 inches of scroll. A tile is exactly `tile_height` tall
/// with zero vertical margin and no bands; the reader's breathing room at the top
/// and bottom of the *document* belongs to the host's scroller, not to every tile.
///
/// Complexity: `O(1)` — it reads nothing but the view and allocates nothing.
#[must_use]
fn reflow_page_config(section: SectionId, view: LayoutView) -> Option<PageConfig> {
    let LayoutView::Reflow {
        content_width,
        tile_height,
        gutter,
    } = view
    else {
        return None;
    };
    Some(PageConfig {
        section,
        page_size: Size::new(content_width + gutter + gutter, tile_height),
        margin_top: Twip::ZERO,
        margin_bottom: Twip::ZERO,
        margin_start: gutter,
        margin_end: gutter,
        // There is no band to nest, so there is no distance to nest it at.
        // `PageConfig::content_area` only consults a distance when the matching
        // height is non-zero, so these are zero for the same reason the heights
        // are: a tile has no header and no footer (`docs/151` §3.3).
        header_distance: Twip::ZERO,
        footer_distance: Twip::ZERO,
        header_height: Twip::ZERO,
        footer_height: Twip::ZERO,
    })
}

/// Clears the page-shaped break **constraints** from a flowed galley, which is
/// how [`LayoutView::Reflow`] suspends them (`docs/151` §3.3).
///
/// Doing it here — once, on the galley, before any paginator sees it — rather
/// than by teaching each paginator about the view is what keeps this **one
/// mechanism**: three paginators read a fragment's break control (the
/// single-column one, the column one, and the footnote one) and all three are
/// covered without a line changing in any of them. `docs/151` §4.5 row 2 asked
/// for edits to `columns.rs` and `paginate.rs`; they are not needed, and the
/// driver-side version is strictly more "one mechanism" than the specified one.
///
/// What is suspended is the paper-shaped set, and it is suspended at both tiers
/// the galley expresses it in:
///
/// - a paragraph's `w:pageBreakBefore`, `w:keepNext`, `w:keepLines` and
///   `w:widowControl`, which live on its break control;
/// - a **line's forced page break**, which is where two different paper-shaped
///   instructions both end up: an explicit `<w:br w:type="page"/>`, and a section
///   break whose successor starts on a new page (`crate::flow`'s
///   `apply_section_break` stamps the section break onto the last line using the
///   same flag, so the two are indistinguishable by the time a paginator sees
///   them). Both are cleared. The line still *ends* there — `LineBreak::Page` is
///   left alone — so a page break becomes a line break, which is exactly what
///   Google's Pageless does with one, and a `nextPage` section break stops
///   opening a tile of its own.
///
/// What is deliberately NOT suspended is a table row's `w:cantSplit` and its
/// vertical-merge keep group, which are about the table's own correctness rather
/// than about paper: splitting a vertically merged row would paint a merged cell
/// twice.
///
/// The walk descends into table cells and inline text boxes, because those carry
/// block content flowed through the same pipeline as the body and a paragraph in
/// a cell can carry every one of these flags.
///
/// **This mutates the galley in place, and the incremental path RETAINS that
/// galley in the cache.** A cleared break control is therefore carried into the
/// next pass, which is harmless while the view stays the same (clearing an
/// already-cleared flag is a no-op) and *wrong* the moment it does not: a paged
/// rebuild served a reflow-suspended fragment would silently lose the author's
/// `w:pageBreakBefore`. So a host changing the view must discard its
/// [`GalleyCache`] and rebuild whole through [`paginate_document_in`] — which is
/// what `casual-doc-wasm`'s `setLayoutView` does, and why it does it.
///
/// Complexity: `O(galley content)` — one visit per fragment, line and nested
/// block, once per layout pass, and only under reflow.
fn suspend_page_break_constraints(galley: &mut [BlockFragment]) {
    for fragment in galley {
        suspend_breaks_in_fragment(fragment);
    }
}

/// [`suspend_page_break_constraints`] for one fragment, recursing the way
/// `crate::paginate`'s field pass does.
fn suspend_breaks_in_fragment(fragment: &mut BlockFragment) {
    match fragment {
        BlockFragment::Paragraph {
            lines,
            break_control,
            ..
        } => {
            *break_control = BreakControl::default();
            for line in &mut lines.lines {
                line.page_break_after = false;
                for text_box in &mut line.text_boxes {
                    for block in &mut text_box.blocks {
                        suspend_breaks_in_fragment(block);
                    }
                }
            }
        }
        BlockFragment::TableRow { cells, .. } => {
            for cell in cells {
                for block in &mut cell.blocks {
                    suspend_breaks_in_fragment(block);
                }
            }
        }
    }
}

/// Trims every tile of a [`LayoutView::Reflow`] layout to its own content, so
/// tiles drawn edge to edge read as one continuous column.
///
/// **Why a zero gap is not enough on its own.** The paginator fills a tile with
/// whole chunks: the chunk that does not fit is carried to the next tile, and the
/// space it would have occupied stays empty at the bottom of this one. That slack
/// is up to one line high (or one table row), so a reader scrolling a zero-gap
/// tile band still sees a blank band at every cut — the very artefact reflow
/// exists to remove. Trimming each tile's page box and content area to the extent
/// of what is actually on it makes the cut invisible **wherever it falls**, which
/// is also what makes the painted column independent of `tile_height`.
///
/// The extent is the lowest bottom edge of anything the tile paints: placed body
/// content, floats resolved onto it, and footnotes. Header, footer, page border,
/// line numbers and watermark are suppressed under reflow and contribute nothing.
/// A float that the page-anchored approximation left *below* the text therefore
/// holds the tile open rather than being clipped — a tile is only ever made
/// shorter than `tile_height`, never taller.
///
/// **Idempotent**, which is what lets it live in the shared post-pagination pass
/// and survive the incremental resume path: a second run over an already-trimmed
/// tile measures the same extent and writes the same height. A tile with nothing
/// on it is left at full height rather than trimmed to zero, because a
/// zero-height raster cannot be allocated.
///
/// Complexity: `O(content on the layout)` — one walk of each tile's three placed
/// lists, no lookup by id, nothing quadratic.
fn trim_reflow_tiles(layout: &mut crate::page::PaginatedLayout) {
    for page in &mut layout.pages {
        let flowed = page
            .placed
            .iter()
            .chain(page.footnotes.iter())
            .map(|placed| placed.rect.bottom());
        let floats = page.anchored.iter().map(|anchor| anchor.rect.bottom());
        let Some(extent) = flowed.chain(floats).max() else {
            continue;
        };
        if extent <= page.content_area.origin.y || extent >= page.page_size.height {
            continue;
        }
        // The content area starts at y = 0 under reflow (`reflow_page_config`
        // gives a tile no vertical margin), so a tile's height and its content
        // area's height are trimmed to the same value.
        page.page_size.height = extent;
        page.content_area.size.height = extent - page.content_area.origin.y;
    }
}

/// Derives the page geometry ([`PageConfig`]) for a document from its first
/// section, with **zero** header/footer bands — the pure page box and margins.
///
/// [`paginate_document`] calls this and then fills in the band heights once it has
/// flowed the running content; callers that only need the page dimensions (for
/// example, to size a render surface) can use it directly, since the full page
/// size and margins do not depend on the bands.
///
/// A document with no section falls back to US-Letter with 1-inch margins.
#[must_use]
pub fn document_page_config(document: &Document) -> PageConfig {
    let sections = &document.definitions().sections;
    match sections.first() {
        Some(section) => section_page_config(section),
        None => PageConfig {
            section: SectionId::new(document.id()),
            page_size: LETTER,
            margin_top: ONE_INCH,
            margin_bottom: ONE_INCH,
            margin_start: ONE_INCH,
            margin_end: ONE_INCH,
            header_distance: DEFAULT_BAND_DISTANCE,
            footer_distance: DEFAULT_BAND_DISTANCE,
            header_height: Twip::ZERO,
            footer_height: Twip::ZERO,
        },
    }
}

/// The zero-band [`PageConfig`] for one section's page box and margins.
///
/// The binding gutter (`w:pgMar/@w:gutter`) is folded into the **inside**
/// margin here — the start edge on a recto page — which is where Word adds it.
/// Folding it once, at the single place page geometry is derived, is what puts
/// it into the content area, the header/footer bands, and the column geometry
/// together (`docs/105` FID-L-16). The per-page half of two-sided geometry
/// (`w:mirrorMargins`, which swaps the inside and outside margins on verso
/// pages) is applied by [`crate::columns`] and [`mirrored_page_config`].
///
/// Not closed: `w:gutterAtTop` (the gutter on the **top** edge instead of the
/// inside edge), so the gutter always lands on the inside edge. It is blocked
/// upstream of layout, not here: `DocumentSettings` carries no field for it and
/// the settings importer routes it to the compatibility report, so there is
/// nothing for this function to read. Closing it is a three-crate change —
/// `gutter_at_top` on `DocumentSettings`, an `apply_setting` arm in
/// `casual-doc-import`'s `settings.rs`, and a writer arm in `casual-doc-export`
/// in `w:settings` schema order — and it must land as one unit: consuming the
/// element without writing it back would convert a *reported* loss into a silent
/// one, which release behaviour forbids. Once the flag exists, the only change
/// here is adding the gutter to `margin_top` instead of `margin_start`, and
/// `mirrored_page_config` must then stop swapping it (a top gutter does not
/// mirror). `docs/105` FID-L-16.
fn section_page_config(section: &SectionBoundary) -> PageConfig {
    let gutter = Twip(section.page_margins.gutter_twips.unwrap_or(0).max(0));
    PageConfig {
        section: section.id,
        page_size: Size::new(
            Twip(section.page_size.width_twips),
            Twip(section.page_size.height_twips),
        ),
        margin_top: Twip(section.page_margins.top_twips),
        margin_bottom: Twip(section.page_margins.bottom_twips),
        margin_start: Twip(section.page_margins.start_twips) + gutter,
        margin_end: Twip(section.page_margins.end_twips),
        // The `w:header`/`w:footer` distances the header/footer bands nest at,
        // falling back to Word's 720-twip default when the attribute is absent.
        header_distance: section
            .page_margins
            .header_twips
            .map_or(DEFAULT_BAND_DISTANCE, Twip),
        footer_distance: section
            .page_margins
            .footer_twips
            .map_or(DEFAULT_BAND_DISTANCE, Twip),
        header_height: Twip::ZERO,
        footer_height: Twip::ZERO,
    }
}

/// Builds one section's [`RunningContent`] — its header/footer
/// variants, each flowed through the same [`flow_header_footer`] path the body
/// uses (so a header holding a paragraph, table, or image lays out identically),
/// plus the two flags that drive per-page variant selection.
///
/// `section` must already carry this section's **effective** references — the
/// per-variant link-to-previous merge in [`build_section_plans`] — because this
/// function reads `section.headers`/`section.footers` verbatim and performs no
/// inheritance of its own.
///
/// Each [`HeaderFooterRef`](casual_doc_model::v1::HeaderFooterRef) is resolved
/// against the header/footer definition store; a reference that does not resolve
/// contributes nothing, leaving that variant empty — which renders as a blank
/// band, not as a fall-back to `default` (see
/// [`HeaderFooter::select`](crate::running::HeaderFooter::select)).
///
/// `content_width` is **this** section's body content width (its own page width
/// minus its own side margins), so an inherited header is re-flowed at the
/// inheriting section's width: an orientation change mid-document re-breaks the
/// band's lines at the new width instead of keeping the donor section's.
fn build_running_content(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    section: &SectionBoundary,
    content_width: Twip,
    labels: &NoteLabels,
) -> RunningContent {
    let defs = document.definitions();
    let mut header = HeaderFooter::default();
    let mut footer = HeaderFooter::default();

    for reference in &section.headers {
        if let Some(hf) = defs.headers.get(&reference.reference) {
            let flowed =
                flow_header_footer_labeled(document, &hf.blocks, shaper, content_width, labels);
            *variant_mut(&mut header, reference.kind) = flowed;
        }
    }
    for reference in &section.footers {
        if let Some(hf) = defs.footers.get(&reference.reference) {
            let flowed =
                flow_header_footer_labeled(document, &hf.blocks, shaper, content_width, labels);
            *variant_mut(&mut footer, reference.kind) = flowed;
        }
    }

    RunningContent {
        header,
        footer,
        title_page: section.title_page.unwrap_or(false),
        even_and_odd: defs.settings.even_and_odd_headers,
    }
}

/// The variant slot of `hf` a given [`HeaderFooterKind`] writes into.
fn variant_mut(
    hf: &mut HeaderFooter,
    kind: HeaderFooterKind,
) -> &mut Vec<crate::block::BlockFragment> {
    match kind {
        HeaderFooterKind::Default => &mut hf.default,
        HeaderFooterKind::First => &mut hf.first,
        HeaderFooterKind::Even => &mut hf.even,
    }
}

/// All layout inputs that are local to one section. The config already includes
/// that section's measured header/footer (and positioned-header-float) bands.
#[derive(Clone, Debug)]
pub(crate) struct SectionPlan {
    pub(crate) config: PageConfig,
    pub(crate) running: RunningContent,
    pub(crate) page_borders: PageBorders,
}

/// Resolves running-content inheritance and geometry for every section before
/// body flow.
///
/// In OOXML "link to previous" is the *absence* of a reference: a section that
/// omits a `w:headerReference`/`w:footerReference` of some type inherits the
/// previous section's for that type, and an explicit reference (including one to
/// an empty part) replaces it. Carrying `effective_headers`/`effective_footers`
/// forward across the whole section list makes that inheritance **transitive** —
/// section three inherits from section one through a silent section two — and
/// [`merge_running_refs`] makes it **per variant**, so a section that declares
/// only `default` still inherits the chain's `first` and `even`. Nothing shows
/// only when no earlier section declared that type either.
///
/// Geometry is resolved here too, per section: each plan's [`PageConfig`] comes
/// from its *own* `w:pgSz`/`w:pgMar`/`w:headerDistance`, and the running content
/// is flowed at that section's content width.
///
/// Under [`LayoutView::Reflow`] the whole of that is replaced by the synthetic
/// tile geometry and an **empty** plan: no header, no footer, no page border.
/// Suppressing the page-shaped furniture by emptying the plan — rather than by
/// teaching each post-pagination pass about the view — is what keeps it one
/// mechanism: `place_running_content_on_page` and the page-border resolver find
/// nothing to place and are already correct for a section that declares none
/// (`docs/151` §3.3, §4.5 row 4). It also skips flowing the bands at all, which
/// is work a reflow pass would throw away.
pub(crate) fn build_section_plans(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    labels: &NoteLabels,
    view: LayoutView,
) -> Vec<SectionPlan> {
    let mut plans = Vec::new();
    let mut effective_headers: Vec<HeaderFooterRef> = Vec::new();
    let mut effective_footers: Vec<HeaderFooterRef> = Vec::new();

    for section in &document.definitions().sections {
        if let Some(config) = reflow_page_config(section.id, view) {
            plans.push(SectionPlan {
                config,
                running: RunningContent::default(),
                page_borders: PageBorders::default(),
            });
            continue;
        }
        merge_running_refs(&mut effective_headers, &section.headers);
        merge_running_refs(&mut effective_footers, &section.footers);

        let mut effective_section = section.clone();
        effective_section.headers.clone_from(&effective_headers);
        effective_section.footers.clone_from(&effective_footers);

        let mut config = section_page_config(section);
        let content_width = config.content_area().size.width;
        let running =
            build_running_content(document, shaper, &effective_section, content_width, labels);
        let (header_height, footer_height) = running.band_heights();
        let header_float =
            header_float_reserve_for_section(document, shaper, &config, &effective_section);
        config.header_height = header_height.max(header_float);
        config.footer_height = footer_height;
        plans.push(SectionPlan {
            config,
            running,
            page_borders: section.page_borders.clone(),
        });
    }

    // A sectionless model is malformed for imported DOCX but remains a supported
    // deterministic fallback for programmatic callers.
    if plans.is_empty() {
        let fallback = document_page_config(document);
        match reflow_page_config(fallback.section, view) {
            Some(config) => plans.push(SectionPlan {
                config,
                running: RunningContent::default(),
                page_borders: PageBorders::default(),
            }),
            None => plans.push(SectionPlan {
                config: fallback,
                running: RunningContent {
                    even_and_odd: document.definitions().settings.even_and_odd_headers,
                    ..RunningContent::default()
                },
                page_borders: PageBorders::default(),
            }),
        }
    }
    plans
}

/// Applies explicit references over inherited references, one variant at a time.
fn merge_running_refs(effective: &mut Vec<HeaderFooterRef>, current: &[HeaderFooterRef]) {
    for reference in current {
        if let Some(slot) = effective
            .iter_mut()
            .find(|existing| existing.kind == reference.kind)
        {
            *slot = *reference;
        } else {
            effective.push(*reference);
        }
    }
}

/// Builds one [`SectionRun`] per document section, in body order. The body is
/// partitioned at each paragraph carrying a section break (the final section is
/// body-level and covers the trailing blocks); each section's block slice is
/// flowed at that section's column width, so line breaking happens at the column
/// — not the full body — width.
///
/// Each section uses the page geometry and header/footer band heights already
/// resolved in its matching [`SectionPlan`]. A document with no declared section
/// produces one full-width run under the fallback plan.
fn build_section_runs(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    plans: &[SectionPlan],
    review_view: ReviewView,
    labels: &NoteLabels,
    view: LayoutView,
    folds: &FoldSet,
) -> Vec<SectionRun> {
    build_section_runs_inner(
        document,
        shaper,
        plans,
        None,
        review_view,
        labels,
        view,
        folds,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_section_runs_with_exclusions(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    plans: &[SectionPlan],
    exclusions: &ParagraphFloatExclusions,
    review_view: ReviewView,
    labels: &NoteLabels,
    view: LayoutView,
    folds: &FoldSet,
) -> Vec<SectionRun> {
    build_section_runs_inner(
        document,
        shaper,
        plans,
        Some(exclusions),
        review_view,
        labels,
        view,
        folds,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_section_runs_inner(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    plans: &[SectionPlan],
    exclusions: Option<&ParagraphFloatExclusions>,
    review_view: ReviewView,
    labels: &NoteLabels,
    view: LayoutView,
    folds: &FoldSet,
) -> Vec<SectionRun> {
    let sections = &document.definitions().sections;
    let body = document.body();
    if sections.is_empty() {
        let config = plans[0].config;
        let content = config.content_area();
        let layout = ColumnLayout::single(content);
        let blocks = blocks_with_endnotes(document, body, &referenced_endnotes(body));
        let mut galley = build_body_galley(
            document,
            shaper,
            &blocks,
            layout.flow_width(),
            exclusions,
            review_view,
            None,
            labels,
            folds,
        );
        if view.is_reflow() {
            suspend_page_break_constraints(&mut galley);
        }
        return vec![SectionRun {
            config,
            layout,
            galley,
            column_galleys: Vec::new(),
            starts_new_page: true,
            start_parity: None,
            // See `push_section_run`: a tile has no recto and no verso.
            mirror_margins: !view.is_reflow() && document.definitions().settings.mirror_margins,
        }];
    }

    let mut runs = Vec::new();
    let mut start = 0usize;
    // Endnote bodies already emitted, so one referenced from a `sectEnd` section
    // and again later is still laid out exactly once.
    let mut emitted: Vec<NoteId> = Vec::new();
    // Non-final sections each end at a paragraph carrying that section's break.
    for (end_excl, boundary) in section_break_points(body, sections) {
        let slice = &body[start..end_excl];
        // `w:endnotePr/w:pos="sectEnd"`: this section's endnote bodies are laid out
        // at the end of *this* section rather than travelling to the document end
        // (`docEnd`, Word's default). `docs/105` FID-L-05.
        let section_endnotes = if section_ends_endnotes(document, boundary) {
            take_unemitted(&referenced_endnotes(slice), &mut emitted)
        } else {
            Vec::new()
        };
        push_section_run(
            document,
            shaper,
            plan_for_section(plans, boundary.id),
            boundary,
            &blocks_with_endnotes(document, slice, &section_endnotes),
            exclusions,
            review_view,
            labels,
            view,
            folds,
            &mut runs,
        );
        start = end_excl;
    }
    // The trailing (body-level) final section covers everything left, and carries
    // every endnote no `sectEnd` section already placed.
    if let Some(last) = sections.last() {
        let remaining = take_unemitted(&referenced_endnotes(body), &mut emitted);
        push_section_run(
            document,
            shaper,
            plan_for_section(plans, last.id),
            last,
            &blocks_with_endnotes(document, &body[start..], &remaining),
            exclusions,
            review_view,
            labels,
            view,
            folds,
            &mut runs,
        );
    }
    runs
}

/// Whether `section` places its endnote bodies at its own end (`w:pos="sectEnd"`)
/// rather than at the document end (`docEnd`, Word's default).
fn section_ends_endnotes(document: &Document, section: &SectionBoundary) -> bool {
    note_props_for_section(document, Some(section), NoteKind::Endnote).position
        == NotePosition::SectionEnd
}

/// The subset of `wanted` not already in `emitted`, marking them emitted.
fn take_unemitted(wanted: &[NoteId], emitted: &mut Vec<NoteId>) -> Vec<NoteId> {
    let mut out = Vec::new();
    for id in wanted {
        if !emitted.contains(id) {
            emitted.push(*id);
            out.push(*id);
        }
    }
    out
}

/// `blocks` with the listed endnote bodies appended, in the order given.
pub(crate) fn blocks_with_endnotes<'a>(
    document: &Document,
    blocks: &'a [BlockNode],
    endnotes: &[NoteId],
) -> Cow<'a, [BlockNode]> {
    // Borrowed in the overwhelmingly common case. This used to be an
    // unconditional `blocks.to_vec()`, which cloned the ENTIRE body on every
    // layout pass — 1.3 GB of transient copy for the 1.3M-paragraph file
    // `docs/111` is about, on a document with no endnotes at all.
    if endnotes.is_empty() {
        return Cow::Borrowed(blocks);
    }
    let mut out = blocks.to_vec();
    for id in endnotes {
        if let Some(note) = document.definitions().endnotes.get(id) {
            out.extend(note.blocks.clone());
        }
    }
    Cow::Owned(out)
}

/// Every endnote referenced by `blocks`, in first-reference order, through the one
/// shared note-reference walker (`crate::note_numbering::visit_block_note_refs`) so
/// a new container that can hold a reference is taught to both this and note
/// numbering at once.
pub(crate) fn referenced_endnotes(blocks: &[BlockNode]) -> Vec<NoteId> {
    let mut out = Vec::new();
    for block in blocks {
        visit_block_note_refs(block, &mut |kind, note| {
            if kind == NoteKind::Endnote {
                push_unique_endnote(&mut out, note);
            }
        });
    }
    out
}

fn push_unique_endnote(out: &mut Vec<NoteId>, note: NoteId) {
    if !out.contains(&note) {
        out.push(note);
    }
}

/// Finds the precomputed plan for `section`, falling back to the first plan for
/// malformed section references in the same way as [`section_break_points`].
fn plan_for_section(plans: &[SectionPlan], section: SectionId) -> &SectionPlan {
    plans
        .iter()
        .find(|plan| plan.config.section == section)
        .unwrap_or(&plans[0])
}

/// The `(end_exclusive, boundary)` cut points of the non-final sections: for each
/// body paragraph carrying a `w:sectPr`, the index one past it and the matching
/// [`SectionBoundary`]. A break whose id does not resolve falls back to the first
/// section's geometry so a malformed document still lays out.
fn section_break_points<'a>(
    body: &[BlockNode],
    sections: &'a [SectionBoundary],
) -> Vec<(usize, &'a SectionBoundary)> {
    let mut points = Vec::new();
    for (i, block) in body.iter().enumerate() {
        if let BlockNode::Paragraph(paragraph) = block
            && let Some(sid) = paragraph.properties.section_break
        {
            let boundary = sections
                .iter()
                .find(|s| s.id == sid)
                .or_else(|| sections.first());
            if let Some(boundary) = boundary {
                points.push((i + 1, boundary));
            }
        }
    }
    points
}

/// Flows one section's block slice at its column width and appends its
/// [`SectionRun`]. An empty slice (a section that carries no body block of its own)
/// is skipped so it never emits a stray band.
///
/// This is where three of reflow's four suspensions happen, and they happen here
/// — in the **driver**, as it builds the runs — rather than inside the paginators
/// (`docs/151` §4.5 row 2 asked for `columns.rs` and `paginate.rs` edits; none is
/// needed):
///
/// - `w:cols` is forced to one column, by constructing the single-column layout
///   the section-less body already uses. A phone cannot show two newspaper
///   columns of a reflowed measure, and ONLYOFFICE's reader view zeroes the
///   column spacing for the same reason;
/// - the section's start type stops forcing a page: `starts_new_page: false` and
///   `start_parity: None` make every section after the first continue in the same
///   tile band, so a `nextPage`/`oddPage` section break does not leave a visible
///   blank stripe in a continuous column;
/// - the paragraph break constraints are cleared once on the flowed galley by
///   [`suspend_page_break_constraints`], which covers all three paginators.
#[allow(clippy::too_many_arguments)]
fn push_section_run(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    plan: &SectionPlan,
    boundary: &SectionBoundary,
    blocks: &[BlockNode],
    exclusions: Option<&ParagraphFloatExclusions>,
    review_view: ReviewView,
    labels: &NoteLabels,
    view: LayoutView,
    folds: &FoldSet,
    runs: &mut Vec<SectionRun>,
) {
    if blocks.is_empty() {
        return;
    }
    let config = plan.config;

    let layout = if view.is_reflow() {
        ColumnLayout::single(config.content_area())
    } else {
        column_layout(&boundary.columns, config.content_area())
    };
    let mut galley = build_body_galley(
        document,
        shaper,
        blocks,
        layout.flow_width(),
        exclusions,
        review_view,
        line_grid_for_section(
            boundary,
            document.definitions().settings.adjust_line_height_in_table,
        ),
        labels,
        folds,
    );
    if view.is_reflow() {
        suspend_page_break_constraints(&mut galley);
    }
    let column_galleys = if layout.has_unequal_widths() {
        layout
            .flow_widths()
            .into_iter()
            .map(|width| {
                build_body_galley(
                    document,
                    shaper,
                    blocks,
                    width,
                    exclusions,
                    review_view,
                    line_grid_for_section(
                        boundary,
                        document.definitions().settings.adjust_line_height_in_table,
                    ),
                    labels,
                    folds,
                )
            })
            .collect()
    } else {
        Vec::new()
    };
    runs.push(SectionRun {
        config,
        layout,
        galley,
        column_galleys,
        starts_new_page: !view.is_reflow() && section_starts_new_page(boundary),
        start_parity: if view.is_reflow() {
            None
        } else {
            section_start_parity(boundary)
        },
        // `w:mirrorMargins` swaps the inside and outside margins on a verso
        // page. A tile has no recto and no verso, and its two side margins are
        // the same gutter anyway, so the swap is geometrically inert here; it is
        // switched off rather than left inert so nothing downstream reads a
        // two-sided document out of a continuous column.
        mirror_margins: !view.is_reflow() && document.definitions().settings.mirror_margins,
    });
}

#[allow(clippy::too_many_arguments)]
fn build_body_galley(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    blocks: &[BlockNode],
    width: Twip,
    exclusions: Option<&ParagraphFloatExclusions>,
    review_view: ReviewView,
    line_grid: Option<crate::flow::LineGrid>,
    labels: &NoteLabels,
    folds: &FoldSet,
) -> Vec<BlockFragment> {
    let mut galley = build_galley_for_blocks_inner(
        document,
        shaper,
        blocks,
        width,
        exclusions,
        review_view,
        NoteFlow {
            label: None,
            labels: Some(labels),
        },
        line_grid,
        folds,
    );
    // A positioned table (`w:tblPr/w:tblpPr`) is not a block in the flow: drop
    // its rows here so the paginator never reserves a band for them and the
    // text that follows flows into the space they would have taken. The table
    // itself is flowed and placed by the float layer
    // (`crate::table_float::place_floating_tables`), and the exclusions that
    // push surrounding text aside come from its resolved rectangle. `O(galley)`,
    // and a no-op for a document that positions no table.
    crate::table_float::lift_floating_rows(&mut galley, document);
    galley
}

/// Lays a whole [`Document`] out into a finished, ready-to-render
/// [`PaginatedLayout`](crate::page::PaginatedLayout) in one call — the single entry point the viewer and the
/// fidelity harness build on.
///
/// The pipeline, all composed from existing engine functions:
///
/// 1. Derive a section-local [`PageConfig`] and [`RunningContent`] plan for every
///    section, including inherited references and band reservations.
/// 3. Partition the body into per-section runs (`build_section_runs`), each flowed
///    at its own column width.
/// 4. [`paginate_columns`] the runs, then run the post-pagination passes **in
///    order** — section-scoped running-content placement, `resolve_fields`
///    (with section-`pgNumType`-aware page-number labels),
///    [`place_floats`] —
///    the order the incremental-golden post-passes require (running content before
///    fields so a `Page X of Y` footer resolves; anchors last, off the pagination
///    hot path).
///
/// Header/footer variants, page-number fields, inline and anchored drawings, and
/// tables flow through the identical pipeline the manual wiring used; only body
/// pagination is now column- and section-aware (see the module docs and
/// [`crate::columns`]).
#[must_use]
pub fn paginate_document(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
) -> crate::page::PaginatedLayout {
    paginate_document_view(document, shaper, ReviewView::Editing)
}

/// [`paginate_document`] under an explicit [`ReviewView`] (docs/93). `Editing`
/// (the default entry) is the live-editor byte space; `Markup` produces a
/// **read-only** layout that shows struck deletions and author-colored/underlined
/// insertions + highlighted comment ranges — for a "show changes" viewer. The
/// markup layout's byte space differs (deletions are shown), so it must not drive
/// caret/selection; it is a render-only view.
#[must_use]
pub fn paginate_document_view(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    review_view: ReviewView,
) -> crate::page::PaginatedLayout {
    paginate_document_in(document, shaper, review_view, LayoutView::Paged)
}

/// [`paginate_document_view`] under an explicit [`LayoutView`] — the one entry
/// point that can produce a **reflow** layout (ADR-046, `docs/151`).
///
/// This is the seam the whole reflow design turns on, and it is deliberately the
/// only one: the flow engine is already width-parametric end to end
/// ([`crate::flow::build_galley`] and its siblings take `content_width` as an
/// argument), so reflow is a question of *where the width comes from* and of
/// nothing else. Under [`LayoutView::Reflow`] the driver substitutes a synthetic
/// [`PageConfig`] (built by the private `reflow_page_config`, named here rather
/// than linked because this comment is public) for the section's own, forces one
/// column, suspends the page-shaped break constraints, suppresses the page-shaped
/// furniture, refuses to print a tile index as a page number, and trims each tile
/// to its content. Not one line of [`crate::flow`], [`crate::columns`] or
/// [`crate::paginate`] changes.
///
/// [`LayoutView::Paged`] produces **byte-for-byte** what
/// [`paginate_document_view`] produced before this parameter existed, which is
/// what `tests/reflow.rs`'s inertness case and the unmoved
/// `tests/geometry_snapshot.golden` assert.
///
/// Complexity: the same `O(document)` as [`paginate_document`] — entering or
/// leaving reflow is a full re-shape (the galley cache is width-scoped), so it is
/// a **mode change**, not an interaction, and a host must run it the way it runs
/// an open: off the main thread, cancellable, never straight off a resize event
/// (`docs/151` §5).
#[must_use]
pub fn paginate_document_in(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    review_view: ReviewView,
    view: LayoutView,
) -> crate::page::PaginatedLayout {
    paginate_document_folded(document, shaper, review_view, view, &FoldSet::EMPTY)
}

/// [`paginate_document_in`] with a [`FoldSet`] — the one entry point that can
/// produce a **folded** layout (ADR-049, `docs/157`).
///
/// `folds` is the set of collapsed heading [`NodeId`]s
/// this viewer is looking at. The blocks of each collapsed subtree contribute no
/// fragments and no height, so pagination closes up and **the page count falls**
/// — reflow, not blanking, which is Word's behaviour and the one thing
/// ONLYOFFICE's architecture could not reproduce (their pagination loop has no
/// visibility filter at the block tier at all).
///
/// Folding filters **content, never document structure**: a hidden block still
/// closes its section, so the geometry and running content of the visible pages
/// *before* a fold are unchanged, and it still advances list counters and note
/// numbering. The one thing that necessarily changes is page numbers — collapsed
/// content occupies no pages — so a host must say on screen that the page count
/// is not the printed one. Print, PDF and DOCX export are always fully expanded.
///
/// An **empty** `folds` produces byte-for-byte what [`paginate_document_in`]
/// produced before this parameter existed, which is what
/// `tests/folding.rs`'s inertness case and the unmoved
/// `tests/geometry_snapshot.golden` assert.
///
/// Complexity: `O(blocks + hidden blocks stepped)`, **not** `O(viewport)`. The
/// filter is one integer and one `continue` per block, so a full pass stays
/// `O(blocks)`; but a window still has to STEP OVER the hidden blocks to find
/// the next visible one, and that stepping is `O(hidden blocks)`. A single fold
/// over a million paragraphs is a million cheap visits — one outline-level read
/// each, no shaping and no allocation — and the honest statement of that is the
/// claim above, not `O(viewport)`. If it proves too slow the textbook escape is
/// a skip list over fold boundaries, rebuilt on a fold toggle (a gesture) rather
/// than on a keystroke; that is deliberately not built yet.
#[must_use]
pub fn paginate_document_folded(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    review_view: ReviewView,
    view: LayoutView,
    folds: &FoldSet,
) -> crate::page::PaginatedLayout {
    // Note numbering first: the reference marker's *text* (`w:numFmt`/`w:numStart`/
    // `w:numRestart`, `docs/105` FID-L-05) is an input to line breaking, so it has
    // to be resolved before anything is flowed.
    let mut labels = resolve_note_labels(document, None);
    let mut layout = paginate_with_note_labels(document, shaper, review_view, &labels, view, folds);
    if !labels.restarts_each_page() {
        return layout;
    }
    // `w:numRestart="eachPage"` is the one case whose numbers depend on the pages
    // they are numbering, so it is a bounded fixed point: re-resolve the labels
    // against the produced pages, re-flow, repaginate, and stop as soon as the
    // labels stop moving. Three passes cover a marker whose width change
    // (`1` → `12`) moves a reference across a page boundary and back; beyond that
    // the last computed layout stands, so termination never depends on the
    // document.
    for _ in 0..NOTE_PAGE_RESTART_PASSES {
        let next = resolve_note_labels(document, Some(&layout));
        if next == labels {
            return layout;
        }
        labels = next;
        layout = paginate_with_note_labels(document, shaper, review_view, &labels, view, folds);
    }
    layout
}

/// The bound on the `eachPage` note-renumbering fixed point (see
/// [`paginate_document_view`]).
const NOTE_PAGE_RESTART_PASSES: usize = 3;

/// One full layout pass at a fixed set of resolved note labels.
fn paginate_with_note_labels(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    review_view: ReviewView,
    labels: &NoteLabels,
    view: LayoutView,
    folds: &FoldSet,
) -> crate::page::PaginatedLayout {
    let plans = build_section_plans(document, shaper, labels, view);
    // Build one paginated run per section, each flowed at its own column width,
    // then paginate them into shared pages (column-aware, section boundaries
    // carried across pages).
    let runs = build_section_runs(document, shaper, &plans, review_view, labels, view, folds);
    finish_pagination(
        document,
        shaper,
        &plans,
        &runs,
        review_view,
        labels,
        view,
        folds,
    )
}

/// The incremental counterpart to [`paginate_document`]: identical output, but the
/// single-section body galley is built through `cache` so unchanged paragraphs are
/// **not re-shaped** — an edit is `O(edit)` instead of `O(document)`.
///
/// Shaping is ~99% of the layout cost on a large prose document, so reusing the
/// shaped lines of the untouched paragraphs is what keeps a keystroke's per-page
/// repaint under budget. `dirty` force-reshapes the listed nodes even if their
/// content hash is unchanged (a belt-and-suspenders over the hash); pass an empty
/// set to rely on hash invalidation alone, which is already correct.
///
/// Documents with explicit section breaks or multi-column sections fall back to a
/// full re-shape (each section's block slice would need its own cache); numbered
/// text-box, and note-referencing paragraphs always re-shape (see
/// [`crate::flow::build_galley_cached`]), and so does a document whose notes restart
/// numbering each page, which needs the pagination fixed point in
/// [`paginate_document_view`]. Every post-pagination pass runs identically to
/// [`paginate_document`], so the result is byte-for-byte the same.
#[must_use]
pub fn paginate_document_cached(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    cache: &mut GalleyCache,
    dirty: &DirtySet,
) -> crate::page::PaginatedLayout {
    paginate_document_view_cached(document, shaper, cache, dirty, ReviewView::Editing)
}

/// [`paginate_document_cached`] for any review view, so the MARKUP layout an
/// editor rebuilds on every keystroke can reuse shaped lines too.
///
/// It could not, and the cost was not small: measured on a 28-page document,
/// a keystroke costs 3.6 ms through the cached editing path and **23.5 ms**
/// through the uncached markup one — 6.6x, and on its own more than a 60 Hz
/// frame. Tracked changes are on by default for any document that carries
/// them, which is precisely the document the review features exist for, so the
/// slowest path was the one the feature's own users were on
/// (`109` HF-182, `benchmarks` `layout.repaginate.keystroke_240_paragraphs*`).
///
/// ONE cache serves both views. The entry hash is taken over the flow items,
/// which are produced *after* the review view has been applied — a struck
/// deletion, an author colour, a comment-range highlight all change the items
/// and therefore the hash — so a markup request can never be served an editing
/// fragment. This was not obvious and the first version of this code carried a
/// second cache "to be safe"; `one_cache_serves_both_views` is what showed the
/// second cache was dead weight, halving the hit rate every time a reader
/// toggled the view.
#[must_use]
pub fn paginate_document_view_cached(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    cache: &mut GalleyCache,
    dirty: &DirtySet,
    review_view: ReviewView,
) -> crate::page::PaginatedLayout {
    paginate_document_view_after_edit(document, shaper, cache, dirty, review_view, None).layout
}

/// What one incremental layout pass produced, and how much of the previous layout
/// it reused — see [`paginate_document_view_after_edit`].
#[derive(Debug)]
pub struct LayoutUpdate {
    /// The new layout. Field-for-field what a fresh [`paginate_document_view`]
    /// would have produced, however much was reused.
    pub layout: crate::page::PaginatedLayout,
    /// The previous layout, handed back **unconsumed** because the pass did not
    /// resume from it. A caller that needs to know which pages changed can compare
    /// against it.
    pub previous: Option<crate::page::PaginatedLayout>,
    /// The half-open range of page indices whose rendered content can differ from
    /// the previous layout's, when the pass resumed from it. Every page outside it
    /// was moved across unchanged and its post-pagination output recomputed
    /// identically, so a host repaints only this range.
    ///
    /// `None` means the pass did not resume: the caller compares against
    /// `previous` when that was handed back, and otherwise must treat every page
    /// as changed.
    pub changed_pages: Option<std::ops::Range<usize>>,
}

/// [`paginate_document_view_cached`] handed the layout the LAST build produced, so
/// it can reuse that layout's pages as well as the galley's fragments.
///
/// This is the editor's entry point, and the whole of `109` HF-182. Re-deriving
/// the galley and re-paginating it from page one is `O(document)` per keystroke,
/// which `docs/107` §4 B1 forbids: measured on a plain-prose body, one keystroke
/// cost 1.0 ms at 240 paragraphs, 1.9 ms at 480 and 3.9 ms at 960 — exactly
/// linear. With `previous` supplied and a [`DirtySet::complete`] damage set, the
/// work is bounded to the edit: the unchanged blocks' fragments move across, and
/// the pages above the edit and below the stabilization point move across too.
///
/// `previous` is taken by value because its pages are **moved** into the result.
/// Cloning them would be `O(document)` in deep copies, which is the cost being
/// removed. Pass `None` for a first layout, or when the caller no longer holds the
/// previous one; the result is identical either way — reuse changes what the pass
/// costs, never what it produces, which
/// `incremental_matches_the_fresh_path_*` asserts over every block shape the
/// reuse path has to survive.
///
/// Complexity: `O(edit)` in derivation and re-flow, plus an `O(blocks)` walk of
/// the body and an `O(pages)` walk for the resume point and the post-pagination
/// passes, both at a small constant and neither touching the paint tier. It falls
/// back to the full `O(document)` build — silently and correctly — for a
/// multi-section body, multi-column text, body footnotes, mirrored margins,
/// section `w:vAlign`, paragraph-anchored floats, `eachPage` note numbering, a
/// drop-cap pair, a positioned table, or a damage set that is not complete.
#[must_use]
pub fn paginate_document_view_after_edit(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    cache: &mut GalleyCache,
    dirty: &DirtySet,
    review_view: ReviewView,
    previous: Option<crate::page::PaginatedLayout>,
) -> LayoutUpdate {
    paginate_document_after_edit_in(
        document,
        shaper,
        cache,
        dirty,
        review_view,
        previous,
        LayoutView::Paged,
    )
}

/// [`paginate_document_view_after_edit`] under an explicit [`LayoutView`], so a
/// keystroke in a **reflow** layout stays incremental instead of silently
/// reverting the reader to paper.
///
/// A keystroke in reflow is `O(edit)` in *shaping*, exactly as in
/// [`LayoutView::Paged`] — the galley cache is keyed on the flow items, and the
/// width it was built at is the reflow width, so nothing about where the width
/// came from reaches it. Pagination itself is not resumed: a reflow tile's height
/// is trimmed to its own content, so it no longer matches the tile height the
/// config declares, and [`crate::paginate::repaginate_at`]'s existing geometry
/// check — which exists precisely to stop a page with a stale content area being
/// reused — declines the resume and re-tiles the galley from the top. That is an
/// `O(pages)` walk with no re-shaping, and it is stated here rather than left to
/// be discovered.
///
/// **The caller must never carry a layout across a view change.** A layout built
/// in the other view has different page geometry and different break decisions in
/// every page; passing it as `previous` would offer the resume a baseline from a
/// document that was laid out to different rules. A host changing the view drops
/// its galley cache and rebuilds whole through [`paginate_document_in`].
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn paginate_document_after_edit_in(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    cache: &mut GalleyCache,
    dirty: &DirtySet,
    review_view: ReviewView,
    previous: Option<crate::page::PaginatedLayout>,
    view: LayoutView,
) -> LayoutUpdate {
    paginate_document_after_edit_folded(
        document,
        shaper,
        cache,
        dirty,
        review_view,
        previous,
        view,
        &FoldSet::EMPTY,
    )
}

/// [`paginate_document_after_edit_in`] under a [`FoldSet`] — the incremental
/// path while something is folded (ADR-049).
///
/// A fold set is a **generation input** to the galley cache, beside the width and
/// the note-label generation, so the three cases separate the way they should:
/// typing at a fixed fold set still reuses the retained galley and costs
/// `O(edit)`; toggling a fold invalidates it and pays one full re-shape, which is
/// a gesture and is allowed to; and a galley built with a heading collapsed is
/// never served after it is expanded. Bypassing the cache while folded — the
/// other available answer — would make every keystroke re-shape the body, which
/// breaches the per-interaction budget in `docs/107` §4.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn paginate_document_after_edit_folded(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    cache: &mut GalleyCache,
    dirty: &DirtySet,
    review_view: ReviewView,
    previous: Option<crate::page::PaginatedLayout>,
    view: LayoutView,
    folds: &FoldSet,
) -> LayoutUpdate {
    let labels = resolve_note_labels(document, None);
    if labels.restarts_each_page() {
        // `eachPage` note numbering needs the pagination fixed point in
        // [`paginate_document_view`]; a cached single pass could serve a marker
        // numbered for a page the reference no longer sits on. Correctness over
        // incrementality, on a rare path.
        cache.discard_retention();
        return LayoutUpdate {
            layout: paginate_document_folded(document, shaper, review_view, view, folds),
            previous,
            changed_pages: None,
        };
    }
    let plans = build_section_plans(document, shaper, &labels, view);
    let mut runs = build_section_runs_cached(
        document,
        shaper,
        &plans,
        cache,
        dirty,
        &labels,
        review_view,
        view,
        folds,
    );
    let resumed = match previous {
        Some(previous) => resume_pagination(document, shaper, &plans, &runs, cache, previous, view),
        None => Resume::NotOffered,
    };
    let (layout, returned, changed_pages) = match resumed {
        Resume::Resumed {
            layout,
            reflowed,
            changed_pages,
        } => {
            cache.note_reflowed_pages(reflowed, true);
            (layout, None, Some(changed_pages))
        }
        Resume::Refused(previous) => {
            let layout = finish_pagination(
                document,
                shaper,
                &plans,
                &runs,
                review_view,
                &labels,
                view,
                folds,
            );
            // A full build re-flowed every page it produced. Charging it keeps the
            // number a complexity guard reads honest whichever path ran.
            cache.note_reflowed_pages(layout.pages.len(), false);
            (layout, Some(previous), None)
        }
        Resume::NotOffered => {
            let layout = finish_pagination(
                document,
                shaper,
                &plans,
                &runs,
                review_view,
                &labels,
                view,
                folds,
            );
            cache.note_reflowed_pages(layout.pages.len(), false);
            (layout, None, None)
        }
    };
    // Hand the body galley back to the cache so the NEXT edit can move the
    // fragments of its unchanged blocks across instead of deriving them
    // (`109` HF-182). The galley is moved, not copied: pagination is done with
    // it, and a copy per keystroke would reintroduce the cost this removes.
    if let [run] = runs.as_mut_slice() {
        cache.retain_galley(std::mem::take(&mut run.galley), labels.fingerprint());
    } else {
        cache.discard_retention();
    }
    LayoutUpdate {
        layout,
        previous: returned,
        changed_pages,
    }
}

/// The outcome of offering the previous layout to [`resume_pagination`].
enum Resume {
    /// It resumed. `reflowed` is the pages it re-flowed (the work), and
    /// `changed_pages` the pages whose rendered content can differ (what a host
    /// must repaint) — the two differ when the page COUNT changed, because then
    /// every page's `NUMPAGES` moves and every page is changed.
    Resumed {
        layout: crate::page::PaginatedLayout,
        reflowed: usize,
        changed_pages: std::ops::Range<usize>,
    },
    /// It refused before consuming the previous layout — here it is back, so the
    /// caller can still compare against it.
    Refused(crate::page::PaginatedLayout),
    /// No previous layout was offered, or it was consumed before the refusal.
    NotOffered,
}

/// Re-paginates by **resuming from `previous`** instead of walking the galley from
/// page one, or `None` when this body is not one the resume is defined for.
///
/// The result is field-for-field what [`finish_pagination`] would have produced —
/// the guarantee [`crate::paginate::repaginate_at`] owns for the pages, and the
/// post-pagination passes re-run over every page for the rest. Returning `None` is
/// always safe: the caller then does the full build.
///
/// The conditions are deliberately narrow, and each one is a place where a reused
/// page would not be the page a fresh pagination produces:
///
/// - the galley build must itself have reused the retained galley, which is what
///   supplies the changed range without a galley diff (and only happens on the
///   single-trailing-section, single-column, cached path);
/// - no body footnote, which paginates through a different paginator that fills
///   `Page::footnotes`;
/// - no mirrored margins, which make a page's content area depend on its own
///   parity, so a page that moves is no longer the page that was reused;
/// - no section `w:vAlign`, because that pass SHIFTS placed content rather than
///   writing to a field of its own, so it is not idempotent over a reused page;
/// - no paragraph-anchored float, because those drive a pagination fixed point
///   that re-flows the whole body anyway.
#[allow(clippy::too_many_arguments)]
fn resume_pagination(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    plans: &[SectionPlan],
    runs: &[SectionRun],
    cache: &GalleyCache,
    previous: crate::page::PaginatedLayout,
    view: LayoutView,
) -> Resume {
    let [run] = runs else {
        return Resume::Refused(previous);
    };
    let Some((first_dirty, dirty_end, previous_galley_len)) = cache.rebuilt_span_last_build()
    else {
        return Resume::Refused(previous);
    };
    if !run.column_galleys.is_empty()
        || run.mirror_margins
        || run_has_body_footnotes(run)
        || previous.pages.is_empty()
        || dirty_end > run.galley.len()
    {
        return Resume::Refused(previous);
    }
    if document.definitions().sections.iter().any(|section| {
        !matches!(
            section.vertical_alignment,
            None | Some(PageVerticalAlignment::Top)
        )
    }) {
        return Resume::Refused(previous);
    }
    let previous_page_count = previous.pages.len();
    // Every fragment above `dirty_end` was moved across unchanged, so the galleys
    // are identical from there to the end. A smaller suffix than the true one is
    // safe (it only offers the halt fewer places to splice), which is why the
    // conservative bound is used rather than a diff.
    let suffix = (run.galley.len() - dirty_end) as u32;
    let (mut layout, stats) = crate::paginate::repaginate_at(
        previous,
        previous_galley_len,
        &run.galley,
        first_dirty,
        suffix,
        &run.config,
    );
    // A reused page still carries the running content, borders, floats, line
    // numbers and watermark the PREVIOUS layout's post-pagination passes put on
    // it. Those passes are about to run again over every page, so their previous
    // output goes first — otherwise the page gets a second header.
    for page in &mut layout.pages {
        page.clear_post_pagination();
    }
    post_pagination_passes(document, shaper, plans, &mut layout, view);
    // A paragraph-anchored float drives the exclusion fixed point in
    // `finish_pagination`, which re-flows the body at a narrowed width; there is
    // nothing incremental about it, so hand the whole job back.
    if !paragraph_float_exclusions(document, shaper, plans, &layout).is_empty() {
        return Resume::NotOffered;
    }
    // What a host must repaint. A page outside the re-flowed range was moved
    // across unchanged and its post-pagination output recomputed identically —
    // UNLESS the page count moved, which changes every `NUMPAGES` in the document
    // and every page number after the insertion, so then everything is changed.
    let changed_pages = if layout.pages.len() == previous_page_count {
        stats.reused_prefix..stats.reused_prefix + stats.reflowed
    } else {
        // Every page: a page appearing or disappearing changes every `NUMPAGES`
        // and every page number after it. The range runs to the LONGER of the two
        // page lists so a host that dropped pages is told which indices are gone,
        // which is what comparing two layouts used to report.
        0..layout.pages.len().max(previous_page_count)
    };
    Resume::Resumed {
        layout,
        reflowed: stats.reflowed,
        changed_pages,
    }
}

/// The shared pagination tail: paginate the section runs into pages, then run the
/// post-pagination passes in the required order. Both [`paginate_document`] and
/// [`paginate_document_cached`] funnel through here so the only difference between
/// them is how the section-run galleys were built.
#[allow(clippy::too_many_arguments)]
fn finish_pagination(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    plans: &[SectionPlan],
    runs: &[SectionRun],
    review_view: ReviewView,
    labels: &NoteLabels,
    view: LayoutView,
    folds: &FoldSet,
) -> crate::page::PaginatedLayout {
    let mut layout = finish_pagination_pass(document, shaper, plans, runs, labels, view);
    let mut exclusions = paragraph_float_exclusions(document, shaper, plans, &layout);
    if exclusions.is_empty() {
        return layout;
    }

    // Float placement and paragraph line measures depend on each other. Three
    // bounded passes cover the practical page-relative/grouped-shape cases while
    // making termination independent of document input.
    let mut previous_exclusions = exclusions.clone();
    for _ in 0..3 {
        let applied_exclusions = exclusions.clone();
        let runs = build_section_runs_with_exclusions(
            document,
            shaper,
            plans,
            &exclusions,
            review_view,
            labels,
            view,
            folds,
        );
        let next = finish_pagination_pass(document, shaper, plans, &runs, labels, view);
        let next_exclusions = paragraph_float_exclusions(document, shaper, plans, &next);
        if next_exclusions == exclusions {
            return next;
        }
        layout = next;
        previous_exclusions = applied_exclusions;
        exclusions = next_exclusions;
        if exclusions.is_empty() {
            return layout;
        }
    }

    // Non-convergence uses a conservative edge envelope: the widest exclusion
    // observed on either side persists for the greatest observed clearance. One
    // final pagination cannot paint text into a previously observed float band.
    let conservative = conservative_exclusions(&previous_exclusions, &exclusions);
    let runs = build_section_runs_with_exclusions(
        document,
        shaper,
        plans,
        &conservative,
        review_view,
        labels,
        view,
        folds,
    );
    finish_pagination_pass(document, shaper, plans, &runs, labels, view)
}

fn finish_pagination_pass(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    plans: &[SectionPlan],
    runs: &[SectionRun],
    labels: &NoteLabels,
    view: LayoutView,
) -> crate::page::PaginatedLayout {
    let mut layout = if runs.iter().any(run_has_body_footnotes) {
        paginate_section_footnotes(document, shaper, runs, labels)
    } else {
        paginate_columns(runs)
    };
    post_pagination_passes(document, shaper, plans, &mut layout, view);
    layout
}

/// Everything that runs AFTER pagination has decided the page boundaries, in the
/// required order. Split out from [`finish_pagination_pass`] because the
/// incremental path ([`resume_pagination`]) produces its pages a different way —
/// by reusing the previous layout's — and then needs exactly these passes.
///
/// Every pass here writes to a field of [`crate::page::Page`] that pagination
/// itself leaves empty (`header`, `footer`, `anchored`, `page_borders`,
/// `line_numbers`, `watermark`), or stamps field values idempotently
/// ([`resolve_fields_labeled`]). That is what makes a reused page safe to run them
/// over again once those fields are cleared, and it is why
/// [`crate::page::Page::clear_post_pagination`] exists.
///
/// `O(pages)`, at a small constant — measured at 13 us for 67 pages, against
/// 1.5 ms for the galley rebuild it sits beside.
///
/// Under [`LayoutView::Reflow`] four of these passes do not run, and the reason is
/// the same in each case: they answer a question about a **page** that a tile
/// cannot be asked (`docs/151` §3.3, §4.5 rows 3 and 4).
///
/// - `w:vAlign` centres or bottom-aligns content *within a page*. On a tile whose
///   height is about to be trimmed to its own content there is no slack to
///   distribute, and distributing it before the trim would move every line of a
///   continuous column by an arbitrary amount.
/// - margin line numbers (`w:lnNumType`) and the section watermark are page
///   furniture by definition.
/// - `PAGE` and `NUMPAGES` resolve against tiles, so they are refused rather than
///   printed (see [`REFLOW_FIELD_REFUSAL`]).
///
/// The header, the footer and the page border need no arm here: the plan
/// [`build_section_plans`] built for a reflow pass is empty, so the passes that
/// place them find nothing — one mechanism instead of a second set of conditions.
fn post_pagination_passes(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    plans: &[SectionPlan],
    layout: &mut crate::page::PaginatedLayout,
    view: LayoutView,
) {
    let fallback_config = plans[0].config;
    // Section `w:vAlign` (center/both/bottom): shift each page's placed body
    // content within its content area. Runs first, before any pass reads body
    // positions (float exclusions, anchored placement, the display list), since
    // every glyph/image/text-box origin is relative to its fragment's rect.
    if !view.is_reflow() {
        apply_page_vertical_alignment(layout, &document.definitions().sections);
    }

    // Post-pagination passes, in the required order: running content is placed
    // first so its fields exist to stamp, then the field pass resolves every
    // `PAGE`/`NUMPAGES` (body and running content), then anchored drawings are
    // placed onto the pages their paragraphs landed on.
    let mirror_margins = document.definitions().settings.mirror_margins;
    // Resolved once for the document, not per page: a themed page border resolves
    // against the same palette the body runs use.
    let palette = document
        .definitions()
        .theme(None)
        .color_scheme
        .map(crate::flow::resolve_palette);
    let mut section_page_numbers: BTreeMap<SectionId, u32> = BTreeMap::new();
    for page in &mut layout.pages {
        let section_page_number = section_page_numbers.entry(page.section).or_default();
        *section_page_number = section_page_number.saturating_add(1);
        let plan = plan_for_section(plans, page.section);
        // The header/footer bands span the text width, so they mirror with the
        // body on a verso page of a two-sided document.
        let config = mirrored_page_config(&plan.config, mirror_margins, page.number);
        place_running_content_on_page(page, &plan.running, &config, *section_page_number);
        // Resolve this section's `w:pgBorders` into a per-page frame, off the hot
        // path like the running content above (docs/46 §F6c).
        page.page_borders = crate::page_border::resolve_page_borders(
            &plan.page_borders,
            *section_page_number,
            page.page_size,
            page.content_area,
            palette.as_ref(),
        );
    }
    // Per-page `PAGE` labels honoring each section's `w:pgNumType` (@fmt format +
    // @start restart); the same labels feed the anchored-field pass below so a
    // floating page-number box matches the body/footer. Under reflow both the
    // per-page label and the total are refusals: a tile index is not a page
    // number, and neither is a tile count.
    let (page_labels, total_label) = if view.is_reflow() {
        (
            vec![REFLOW_FIELD_REFUSAL.to_owned(); layout.pages.len()],
            REFLOW_FIELD_REFUSAL.to_owned(),
        )
    } else {
        (
            page_number_labels(layout, &document.definitions().sections),
            layout.pages.len().to_string(),
        )
    };
    resolve_fields_labeled(layout, &page_labels, &total_label, shaper);
    // Floating objects last: anchored pictures, floating text boxes, and DrawingML
    // groups, over body AND header/footer bands, each resolved to a rect + z-key
    // for the float layer to paint in order.
    place_floats(layout, document, shaper, &fallback_config);
    // Positioned tables (`w:tblPr/w:tblpPr`) join the same float layer, straight
    // after the drawings, so one z-space covers both (`docs/109` row 64).
    crate::table_float::place_floating_tables(layout, document, shaper, &fallback_config);
    // A floating text box (e.g. the SDS footer's positioned `v:textbox` page-number
    // box) can itself hold `PAGE`/`NUMPAGES` fields; resolve them now that the
    // floats — and their flowed block content — exist on each page.
    resolve_anchored_fields_labeled(layout, &page_labels, &total_label, shaper);
    // Margin line numbers (`w:lnNumType`) last: they stamp each numbered line's
    // FINAL baseline, so they must follow the vertical-alignment shift above and
    // cannot precede it. Inert unless a section declares line numbering
    // (`docs/105` FID-L-09).
    if !view.is_reflow() {
        crate::line_number::place_line_numbers(layout, document, shaper);
        crate::watermark::place_watermarks(layout, document, shaper);
    }
    // Absolutely last, and only under reflow: every tile is cut down to what is
    // actually on it, so the tiles drawn edge to edge read as one column. It has
    // to follow every pass that can put content on a page — the floats above
    // included — or a tile would be trimmed above a float that had not been
    // placed yet.
    if view.is_reflow() {
        trim_reflow_tiles(layout);
    }
}

/// The physical page geometry of page `number` under `w:mirrorMargins`: on a
/// verso (even) page Word swaps the inside and outside margins, so everything
/// aligned to the text width — the body band and the header/footer bands —
/// moves with them. The binding gutter is already folded into the inside margin
/// by [`section_page_config`], so it travels with the swap (`docs/105`
/// FID-L-16).
///
/// A document that does not mirror (the overwhelming majority) gets the section
/// geometry back unchanged, so this is inert on the common path.
pub(crate) fn mirrored_page_config(
    config: &PageConfig,
    mirror_margins: bool,
    number: u32,
) -> PageConfig {
    let mut config = *config;
    if mirror_margins && number.is_multiple_of(2) {
        core::mem::swap(&mut config.margin_start, &mut config.margin_end);
    }
    config
}

/// Applies each section's `w:vAlign` to its pages: shifts the placed body
/// content within the page's content area. `Top` (Word's default) is a no-op;
/// `Center` centers the content block, `Bottom` pushes it to the bottom, and
/// `Both` distributes the vertical slack evenly between the placed blocks
/// (vertical justification). The slack is the content-area height minus the
/// content's own height; a page whose content fills (or overflows) the area has
/// no slack and is left untouched. Every glyph/image/text-box origin is relative
/// to its fragment's `rect`, so moving `rect.origin.y` moves the whole block.
pub(crate) fn apply_page_vertical_alignment(
    layout: &mut crate::page::PaginatedLayout,
    sections: &[SectionBoundary],
) {
    for page in &mut layout.pages {
        let Some(align) = sections
            .iter()
            .find(|section| section.id == page.section)
            .and_then(|section| section.vertical_alignment)
        else {
            continue;
        };
        if matches!(align, PageVerticalAlignment::Top) || page.placed.is_empty() {
            continue;
        }
        let content_top = page.content_area.origin.y;
        let content_bottom = content_top + page.content_area.size.height;
        // The content's own extent: the lowest placed-fragment bottom.
        let used_bottom = page
            .placed
            .iter()
            .map(|placed| placed.rect.origin.y + placed.rect.size.height)
            .max()
            .unwrap_or(content_top);
        let slack = content_bottom - used_bottom;
        if slack <= Twip::ZERO {
            continue;
        }
        match align {
            PageVerticalAlignment::Center => {
                let offset = Twip(slack.raw() / 2);
                for placed in &mut page.placed {
                    placed.rect.origin.y = placed.rect.origin.y + offset;
                }
            }
            PageVerticalAlignment::Bottom => {
                for placed in &mut page.placed {
                    placed.rect.origin.y = placed.rect.origin.y + slack;
                }
            }
            PageVerticalAlignment::Both => {
                // Vertical justification: spread the slack across the gaps between
                // the placed blocks (block `i` of `n` moves down by `slack*i/(n-1)`).
                // A single block has no gap and stays at the top.
                let gaps = page.placed.len().saturating_sub(1);
                if gaps == 0 {
                    continue;
                }
                for (index, placed) in page.placed.iter_mut().enumerate() {
                    let offset = Twip((slack.raw() as i64 * index as i64 / gaps as i64) as i32);
                    placed.rect.origin.y = placed.rect.origin.y + offset;
                }
            }
            PageVerticalAlignment::Top => {}
        }
    }
}

fn paragraph_float_exclusions(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    plans: &[SectionPlan],
    layout: &crate::page::PaginatedLayout,
) -> ParagraphFloatExclusions {
    let wraps = body_wrap_rects(layout, document, shaper, &plans[0].config);
    if wraps.is_empty() {
        return ParagraphFloatExclusions::new();
    }

    let body_order: BTreeMap<NodeId, usize> = document
        .body()
        .iter()
        .enumerate()
        .filter_map(|(index, block)| match block {
            BlockNode::Paragraph(paragraph) => Some((paragraph.id, index)),
            _ => None,
        })
        .collect();
    // Top-level body tables, so a page-relative float's exclusion can descend
    // into their cells' paragraphs (P1F-FLOAT-SQUARE-2) without also reaching
    // note-body or running-content tables, which share the same placed-fragment
    // list but are not real body-order content (mirroring `body_order` above).
    let body_table_ids: BTreeSet<NodeId> = document
        .body()
        .iter()
        .filter_map(|block| match block {
            BlockNode::Table(table) => Some(table.id),
            _ => None,
        })
        .collect();
    let mut paragraphs = Vec::new();
    for (page_index, page) in layout.pages.iter().enumerate() {
        for placed in &page.placed {
            match &placed.fragment {
                BlockFragment::Paragraph { id, .. } if body_order.contains_key(id) => {
                    paragraphs.push((page_index, *id, placed.rect));
                }
                BlockFragment::TableRow { table, cells, .. } if body_table_ids.contains(table) => {
                    collect_cell_paragraph_rects(cells, placed.rect, page_index, &mut paragraphs);
                }
                _ => {}
            }
        }
    }

    let mut exclusions = ParagraphFloatExclusions::new();
    for wrap in wraps {
        // A wrap's source is the top-level body PARAGRAPH that anchors it — or,
        // for a positioned table (`w:tblpPr`), the table's own id, because a
        // positioned table anchors itself rather than hanging off a run.
        if !body_order.contains_key(&wrap.source) && !body_table_ids.contains(&wrap.source) {
            continue;
        }
        let left = wrap.rect.origin.x - emu_to_twip_extent(wrap.distances.start_emu);
        let right = wrap.rect.right() + emu_to_twip_extent(wrap.distances.end_emu);
        let top = wrap.rect.origin.y - emu_to_twip_extent(wrap.distances.top_emu);
        let bottom = wrap.rect.bottom() + emu_to_twip_extent(wrap.distances.bottom_emu);
        for (page_index, paragraph, rect) in &paragraphs {
            // Page- and margin-relative objects can sit above their anchoring
            // paragraph (the right arrow in demo.docx is one such object), so
            // every overlapping top-level paragraph on the resolved page must be
            // considered. The bounded fixed point below makes that backward
            // dependency deterministic.
            //
            // The test is a true band intersection: a float whose top falls
            // *inside* a paragraph (a positioned table nudged `w:tblpY="1"`
            // below the flow position is the canonical case, and a `posOffset`
            // drawing does the same) used to be skipped outright, so text ran
            // straight through it. APPROXIMATION, recorded rather than hidden:
            // `ParagraphFloatExclusion` is a *leading* exclusion measured from
            // the paragraph's own top, so such a float also narrows the lines
            // above its top edge. That over-excludes; running text through a
            // float does not, and the conservative envelope below already
            // treats over-exclusion as the safe direction.
            //
            // Only the VERTICAL half of the intersection is tested here; the
            // horizontal half belongs to the one shared rule below, so a single
            // place decides whether a band overlaps a measure.
            if *page_index != wrap.page_index || top >= rect.bottom() || bottom <= rect.origin.y {
                continue;
            }
            // The side and width are the authored `w:wrap@wrapText` rule, shared
            // with the paragraph-local and carry paths in `crate::flow` so one
            // float cannot wrap two different ways depending on which pass saw
            // it (`crate::wrap_side`). A band that does not overlap this
            // paragraph's measure at all narrows nothing.
            let Some(resolved) =
                band_exclusion(left, right, rect.origin.x, rect.right(), wrap.sides)
            else {
                continue;
            };
            let exclusion = ParagraphFloatExclusion {
                side: resolved.side,
                width: resolved.width,
                height: Twip((bottom.raw() - rect.origin.y.raw()).max(1)),
            };
            let values = exclusions.entry(*paragraph).or_default();
            if !values.contains(&exclusion) {
                values.push(exclusion);
            }
        }
    }
    exclusions
}

/// Appends every paragraph fragment's absolute page rect nested under a
/// top-level table row's cells to `out`, so a page-relative float's exclusion
/// zone can reach text inside a table cell — not only top-level body
/// paragraphs. Descends through nested tables (a table inside a cell) to
/// arbitrary depth.
///
/// The geometry mirrors [`crate::compose::compose_page`]'s cell-content
/// placement exactly (top/bottom margin inset, `w:vAlign` slack, block
/// stacking by [`BlockFragment::height`]) so a computed exclusion always lines
/// up with what the page actually paints; a merged-away `w:vMerge` continuation
/// cell contributes no box, matching the painter.
fn collect_cell_paragraph_rects(
    cells: &[CellFragment],
    row_rect: Rect,
    page_index: usize,
    out: &mut Vec<(usize, NodeId, Rect)>,
) {
    for cell in cells {
        if matches!(cell.vertical_merge, CellVerticalMerge::Continue) {
            continue;
        }
        let cell_height = cell.box_height(row_rect.size.height);
        let content_width =
            Twip((cell.width.raw() - cell.margins.start.raw() - cell.margins.end.raw()).max(1));
        let x = row_rect.origin.x + cell.x + cell.margins.start;
        let mut y = row_rect.origin.y + cell.cell_spacing.top + cell.content_y_offset(cell_height);
        for block in &cell.blocks {
            collect_block_paragraph_rects(block, Point::new(x, y), content_width, page_index, out);
            y = y + block.height();
        }
    }
}

/// One block's contribution to [`collect_cell_paragraph_rects`]: a paragraph
/// records its rect directly; a nested table row recurses into its own cells.
fn collect_block_paragraph_rects(
    block: &BlockFragment,
    origin: Point,
    width: Twip,
    page_index: usize,
    out: &mut Vec<(usize, NodeId, Rect)>,
) {
    match block {
        BlockFragment::Paragraph { id, .. } => {
            out.push((
                page_index,
                *id,
                Rect::new(origin, Size::new(width, block.height())),
            ));
        }
        BlockFragment::TableRow { cells, .. } => {
            let row_rect = Rect::new(origin, Size::new(width, block.height()));
            collect_cell_paragraph_rects(cells, row_rect, page_index, out);
        }
    }
}

fn conservative_exclusions(
    first: &ParagraphFloatExclusions,
    second: &ParagraphFloatExclusions,
) -> ParagraphFloatExclusions {
    let mut result = ParagraphFloatExclusions::new();
    for source in [first, second] {
        for (paragraph, exclusions) in source {
            for exclusion in exclusions {
                let values = result.entry(*paragraph).or_default();
                if let Some(existing) = values
                    .iter_mut()
                    .find(|existing| existing.side == exclusion.side)
                {
                    existing.width = existing.width.max(exclusion.width);
                    existing.height = existing.height.max(exclusion.height);
                } else {
                    values.push(*exclusion);
                }
            }
        }
    }
    result
}

/// [`build_section_runs`], but the common **single-section** body is flowed through
/// the galley `cache` so unchanged paragraphs are reused rather than re-shaped.
/// Documents that carry explicit section breaks or referenced endnotes re-shape
/// fully (each section slice or synthetic endnote appendix would need its own
/// cache, and these are rarer than plain body edits).
#[allow(clippy::too_many_arguments)]
fn build_section_runs_cached(
    document: &Document,
    shaper: &dyn crate::text::LineShaper,
    plans: &[SectionPlan],
    cache: &mut GalleyCache,
    dirty: &DirtySet,
    labels: &NoteLabels,
    review_view: ReviewView,
    view: LayoutView,
    folds: &FoldSet,
) -> Vec<SectionRun> {
    // The incremental cache used to be switched off whenever the document
    // declared ANY section — and every Word-produced file ends `w:body` with a
    // trailing `w:sectPr`, so `sections` is non-empty for essentially every
    // imported document. The cache was therefore inert exactly where it matters:
    // real files, where each keystroke re-shaped the whole body.
    //
    // What the fast path actually needs is one full-width flow over the whole
    // body. A single trailing section with one column is precisely that — it
    // differs from the no-section case only in where the page config comes from.
    // Anything else (a real section break mid-body, multiple columns, endnotes
    // appended to the flow) keeps the uncached builder.
    let sections = &document.definitions().sections;
    let single_trailing_section = match sections.as_slice() {
        [] => true,
        [only] => {
            section_break_points(document.body(), sections).is_empty()
                && only.columns.count.max(1) == 1
        }
        _ => false,
    };
    if !single_trailing_section || !referenced_endnotes(document.body()).is_empty() {
        // This build never came through the cached galley builder, so no galley is
        // retained from it — and any galley retained from an earlier build must go
        // too, or it would be reused across a body this one changed without
        // reporting.
        cache.discard_retention();
        return build_section_runs(document, shaper, plans, review_view, labels, view, folds);
    }
    // One full-width run over the whole body, built incrementally. Mirrors the
    // `sections.is_empty()` arm of `build_section_runs`, swapping
    // `build_galley_for_blocks` for the cached builder.
    let config = sections.last().map_or(plans[0].config, |last| {
        plan_for_section(plans, last.id).config
    });
    let layout = ColumnLayout::single(config.content_area());
    let mut galley = build_galley_cached_labeled(
        document,
        shaper,
        layout.flow_width(),
        cache,
        dirty,
        NoteFlow {
            label: None,
            labels: Some(labels),
        },
        review_view,
        folds,
    );
    // Same lift as the uncached builder: a positioned table is not a block in
    // the flow. The incremental path must agree with the fresh one fragment for
    // fragment or the two would paginate the same document differently.
    //
    // The lift REMOVES fragments after the builder recorded which block produced
    // which range, so the galley it leaves behind cannot be retained for reuse —
    // its ranges no longer describe it. Inert for a document that positions no
    // table, which is the overwhelming majority.
    if crate::table_float::has_floating_table(document) {
        cache.refuse_retention();
    }
    crate::table_float::lift_floating_rows(&mut galley, document);
    // The same suspension the uncached builder applies, for the same reason and in
    // the same place: on the galley, once, before any paginator sees it. Applying
    // it AFTER the lift keeps the two builders fragment-for-fragment identical,
    // which is the invariant the incremental path rests on.
    if view.is_reflow() {
        suspend_page_break_constraints(&mut galley);
    }
    vec![SectionRun {
        config,
        layout,
        galley,
        column_galleys: Vec::new(),
        starts_new_page: true,
        // The cached fast path is the single-trailing-section body, which is the
        // document's *first* section: it opens page 1 and can never need a
        // parity pad (there is no page before it to pad after).
        start_parity: None,
        // See `push_section_run`: a tile has no recto and no verso.
        mirror_margins: !view.is_reflow() && document.definitions().settings.mirror_margins,
    }]
}

#[cfg(test)]
mod cached_pagination_tests {
    use super::*;
    use crate::shape::ParleyShaper;
    use casual_doc_model::NodeId;
    use casual_doc_model::v1::{
        BlockNode, CommentId, CommentRangeEnd, CommentRangeStart, Definitions, InlineNode, Note,
        NoteId, NoteKind, NoteReference, Paragraph, ParagraphProperties, Run, RunProperties,
    };

    fn node(id: u64) -> NodeId {
        NodeId::from_parts(id, 1).unwrap()
    }

    /// A body of paragraphs, each `texts[i]` as one run — the i-th paragraph keeps a
    /// stable node id across edits so the galley cache can key on it.
    fn doc(texts: &[&str]) -> Document {
        let body = texts
            .iter()
            .enumerate()
            .map(|(i, text)| {
                let id = i as u64 + 1;
                BlockNode::Paragraph(Paragraph {
                    id: node(id),
                    properties: ParagraphProperties::default().into(),
                    inlines: vec![InlineNode::Run(Run {
                        id: node(id + 1_000),
                        properties: RunProperties::default().into(),
                        text: (*text).to_owned(),
                    })],
                })
            })
            .collect();
        Document::new(node(9_000), body, Definitions::default()).unwrap()
    }

    fn doc_with_endnote() -> Document {
        let endnote = NoteId::new(node(9_100));
        let mut definitions = Definitions::default();
        definitions.endnotes.insert(
            endnote,
            Note {
                blocks: vec![BlockNode::Paragraph(Paragraph {
                    id: node(9_101),
                    properties: ParagraphProperties::default().into(),
                    inlines: vec![InlineNode::Run(Run {
                        id: node(9_102),
                        properties: RunProperties::default().into(),
                        text: "cached endnote body".to_owned(),
                    })],
                })],
            },
        );
        let body = vec![
            BlockNode::Paragraph(Paragraph {
                id: node(9_103),
                properties: ParagraphProperties::default().into(),
                inlines: vec![InlineNode::Run(Run {
                    id: node(9_104),
                    properties: RunProperties::default().into(),
                    text: "body".to_owned(),
                })],
            }),
            BlockNode::Paragraph(Paragraph {
                id: node(9_105),
                properties: ParagraphProperties::default().into(),
                inlines: vec![InlineNode::NoteReference(NoteReference {
                    id: node(9_106),
                    kind: NoteKind::Endnote,
                    note: endnote,
                })],
            }),
        ];
        Document::new(node(9_107), body, definitions).unwrap()
    }

    // Enough prose to span several pages, so an edit that reuses cached paragraphs
    // genuinely skips work the full path would redo.
    fn prose(n: usize) -> Vec<String> {
        (0..n)
            .map(|i| format!("Paragraph {i}. The quick brown fox jumps over the lazy dog."))
            .collect()
    }

    /// A document whose MARKUP view differs from its editing view: one paragraph
    /// carries a comment range, which markup highlights and editing ignores.
    fn doc_with_comment_range(texts: &[&str], on: usize) -> Document {
        let comment = CommentId::new(node(5_000));
        let body = texts
            .iter()
            .enumerate()
            .map(|(i, text)| {
                let id = i as u64 + 1;
                let run = InlineNode::Run(Run {
                    id: node(id + 1_000),
                    properties: RunProperties::default().into(),
                    text: (*text).to_owned(),
                });
                let inlines = if i == on {
                    vec![
                        InlineNode::CommentRangeStart(CommentRangeStart {
                            id: node(id + 2_000),
                            comment,
                        }),
                        run,
                        InlineNode::CommentRangeEnd(CommentRangeEnd {
                            id: node(id + 3_000),
                            comment,
                        }),
                    ]
                } else {
                    vec![run]
                };
                BlockNode::Paragraph(Paragraph {
                    id: node(id),
                    properties: ParagraphProperties::default().into(),
                    inlines,
                })
            })
            .collect();
        Document::new(node(9_000), body, Definitions::default()).unwrap()
    }

    /// The markup view's cached path must produce exactly what its fresh path
    /// produces — before and after an edit.
    ///
    /// This is the guard the speed-up rests on. An editor rebuilds the markup
    /// layout on every keystroke whenever tracked changes are showing, and it
    /// used to rebuild it UNCACHED: 15.4 ms per keystroke against the editing
    /// path's 2.5 ms on a 28-page document, which is a dropped frame from
    /// layout alone, paid by exactly the people the review features are for.
    /// Caching it is only legitimate if the answer does not change.
    #[test]
    fn cached_markup_matches_full_markup_after_an_edit() {
        let shaper = ParleyShaper::new();
        let before: Vec<String> = prose(60);
        let strs: Vec<&str> = before.iter().map(String::as_str).collect();
        let doc_before = doc_with_comment_range(&strs, 30);

        let mut cache = GalleyCache::new();
        let warm = paginate_document_view_cached(
            &doc_before,
            &shaper,
            &mut cache,
            &DirtySet::everything(),
            ReviewView::Markup,
        );
        assert_eq!(
            warm,
            paginate_document_view(&doc_before, &shaper, ReviewView::Markup),
            "a full-dirty cached markup build must equal the fresh markup build"
        );

        let mut after = before.clone();
        after[30] = "Paragraph 30. EDITED — a longer line that rewraps this paragraph.".to_owned();
        let strs_after: Vec<&str> = after.iter().map(String::as_str).collect();
        let doc_after = doc_with_comment_range(&strs_after, 30);

        let cached = paginate_document_view_cached(
            &doc_after,
            &shaper,
            &mut cache,
            &DirtySet::new(),
            ReviewView::Markup,
        );
        assert_eq!(
            cached,
            paginate_document_view(&doc_after, &shaper, ReviewView::Markup),
            "incremental markup re-pagination diverged from a full markup re-shape"
        );
        // …and it was actually INCREMENTAL. Equality alone is satisfied by a
        // path that quietly re-shaped everything, which is exactly what this
        // change exists to stop — the first version of this test passed while
        // the cached galley builder was hard-coded to the editing view,
        // because the fixture never reached it.
        assert_eq!(
            cache.shaped_last_build(),
            1,
            "only the edited paragraph should have been re-shaped in the markup view"
        );
    }

    /// One cache serves both views — and this is why it is allowed to.
    ///
    /// The cache hashes the FLOW ITEMS, which are produced after the review
    /// view has been applied, so the same paragraph under the two views hashes
    /// differently and a markup request cannot be handed an editing fragment.
    /// The first version of the caching change carried a second cache "to be
    /// safe"; this test is what showed it was dead weight — and worse than
    /// dead, since it halves the hit rate every time a reader toggles the view.
    #[test]
    fn one_cache_serves_both_views() {
        let shaper = ParleyShaper::new();
        let texts: Vec<String> = prose(12);
        let strs: Vec<&str> = texts.iter().map(String::as_str).collect();
        let document = doc_with_comment_range(&strs, 4);

        let editing = paginate_document_view(&document, &shaper, ReviewView::Editing);
        let markup = paginate_document_view(&document, &shaper, ReviewView::Markup);
        assert_ne!(
            editing, markup,
            "this fixture must actually differ between the views, or the rest proves nothing"
        );

        // Alternate the way a reader toggling "show changes" does. Every answer
        // must be that view's own, not the one cached a moment ago.
        let mut cache = GalleyCache::new();
        for round in 0..3 {
            assert_eq!(
                paginate_document_view_cached(
                    &document,
                    &shaper,
                    &mut cache,
                    &DirtySet::new(),
                    ReviewView::Editing,
                ),
                editing,
                "editing view wrong on round {round}"
            );
            assert_eq!(
                paginate_document_view_cached(
                    &document,
                    &shaper,
                    &mut cache,
                    &DirtySet::new(),
                    ReviewView::Markup,
                ),
                markup,
                "markup view wrong on round {round}"
            );
        }
    }

    /// The whole point: after a realistic single-paragraph edit, the incremental
    /// path must produce the byte-for-byte same pagination as a full re-shape —
    /// while re-shaping only the one paragraph that changed.
    /// The incremental cache must engage on a document that declares a section.
    ///
    /// Every Word-produced file ends `w:body` with a trailing `w:sectPr`, so
    /// `definitions().sections` is non-empty for essentially every imported
    /// document — and the cached builder bailed to the uncached one whenever ANY
    /// section existed. The cache was therefore inert exactly where it matters:
    /// on real files, every keystroke re-shaped the whole body.
    #[test]
    fn the_cache_engages_on_a_document_with_a_trailing_section() {
        use casual_doc_model::v1::{
            DocGrid, NoteProperties, PageBorders, PageMargins, PageNumbering, PageSize,
            PaperSource, SectionBoundary, SectionColumns, SectionId,
        };

        let shaper = ParleyShaper::new();
        let texts: Vec<String> = prose(60);
        let with_section = |texts: &[String]| {
            let mut d = doc(&texts.iter().map(String::as_str).collect::<Vec<_>>());
            // The trailing body-level sectPr every Word document carries.
            d.definitions_mut().sections = vec![SectionBoundary {
                id: SectionId::new(node(9_000)),
                page_size: PageSize {
                    width_twips: 12_240,
                    height_twips: 15_840,
                },
                page_margins: PageMargins {
                    top_twips: 1_440,
                    bottom_twips: 1_440,
                    start_twips: 1_440,
                    end_twips: 1_440,
                    header_twips: None,
                    footer_twips: None,
                    gutter_twips: None,
                },
                columns: SectionColumns {
                    count: 1,
                    space_twips: None,
                    separator: None,
                    equal_width: None,
                    columns: Vec::new(),
                },
                headers: Vec::new(),
                footers: Vec::new(),
                section_type: None,
                title_page: None,
                vertical_alignment: None,
                page_numbering: PageNumbering::default(),
                doc_grid: DocGrid::default(),
                orientation: None,
                paper_source: PaperSource::default(),
                page_borders: PageBorders::default(),
                line_numbering: Default::default(),
                watermark: None,
                footnote_props: NoteProperties::default(),
                endnote_props: NoteProperties::default(),
                text_direction: None,
                bidi: false,
                section_change: None,
            }];
            d
        };

        let before = with_section(&texts);
        let mut cache = GalleyCache::new();
        let warm = paginate_document_cached(&before, &shaper, &mut cache, &DirtySet::everything());
        assert_eq!(
            warm,
            paginate_document(&before, &shaper),
            "a full-dirty cached build must equal the fresh build"
        );
        assert!(
            cache.len() > 1,
            "the cache must actually hold the document's paragraphs, got {}",
            cache.len()
        );

        // Edit one paragraph; only that paragraph may re-shape.
        let mut after_texts = texts.clone();
        after_texts[30] = "Paragraph 30. EDITED — a longer line that rewraps it.".to_owned();
        let after = with_section(&after_texts);

        let cached = paginate_document_cached(&after, &shaper, &mut cache, &DirtySet::new());
        assert_eq!(
            cached,
            paginate_document(&after, &shaper),
            "incremental re-pagination diverged from a full re-shape"
        );
        assert_eq!(
            cache.shaped_last_build(),
            1,
            "only the edited paragraph re-shapes; the cache was bypassed if this \
             is the whole document"
        );
    }

    #[test]
    fn cached_matches_full_after_a_paragraph_edit() {
        let shaper = ParleyShaper::new();
        let before: Vec<String> = prose(60);
        let doc_before = doc(&before.iter().map(String::as_str).collect::<Vec<_>>());

        // Warm the cache on the pre-edit document (mirrors an open + first paint).
        let mut cache = GalleyCache::new();
        let warm =
            paginate_document_cached(&doc_before, &shaper, &mut cache, &DirtySet::everything());
        assert_eq!(
            warm,
            paginate_document(&doc_before, &shaper),
            "a full-dirty cached build must equal the fresh build"
        );

        // Edit one paragraph's text (its node id is unchanged, its content hash is not).
        let mut after = before.clone();
        after[30] = "Paragraph 30. EDITED — a longer line that rewraps this paragraph.".to_owned();
        let doc_after = doc(&after.iter().map(String::as_str).collect::<Vec<_>>());

        let cached = paginate_document_cached(&doc_after, &shaper, &mut cache, &DirtySet::new());
        let full = paginate_document(&doc_after, &shaper);
        assert_eq!(
            cached, full,
            "incremental re-pagination diverged from a full re-shape"
        );
        assert_eq!(
            cache.shaped_last_build(),
            1,
            "only the edited paragraph should have been re-shaped"
        );
    }

    /// Inserting a paragraph (a new node id) reuses the cached fragments of the
    /// paragraphs that did not move and shapes only the newcomer — and still equals
    /// a full re-shape, so a structural edit is incremental and correct.
    #[test]
    fn cached_matches_full_after_a_paragraph_insert() {
        let shaper = ParleyShaper::new();
        let before = prose(40);
        let doc_before = doc(&before.iter().map(String::as_str).collect::<Vec<_>>());
        let mut cache = GalleyCache::new();
        let _ = paginate_document_cached(&doc_before, &shaper, &mut cache, &DirtySet::everything());

        // Splice a brand-new paragraph in the middle. Its node id (900) is not in
        // the cache, so it shapes; every original paragraph keeps its id and hits.
        let mut blocks = doc_before.body().to_vec();
        blocks.insert(
            20,
            BlockNode::Paragraph(Paragraph {
                id: node(900),
                properties: ParagraphProperties::default().into(),
                inlines: vec![InlineNode::Run(Run {
                    id: node(1_900),
                    properties: RunProperties::default().into(),
                    text: "A newly inserted paragraph of prose.".to_owned(),
                })],
            }),
        );
        let doc_after = Document::new(node(9_000), blocks, Definitions::default()).unwrap();

        let cached = paginate_document_cached(&doc_after, &shaper, &mut cache, &DirtySet::new());
        assert_eq!(cached, paginate_document(&doc_after, &shaper));
        assert_eq!(
            cache.shaped_last_build(),
            1,
            "only the inserted paragraph should have been shaped"
        );
    }

    #[test]
    fn cached_matches_full_when_endnotes_are_appended() {
        let shaper = ParleyShaper::new();
        let doc = doc_with_endnote();
        let mut cache = GalleyCache::new();

        let cached = paginate_document_cached(&doc, &shaper, &mut cache, &DirtySet::everything());
        let full = paginate_document(&doc, &shaper);

        assert_eq!(
            cached, full,
            "cached pagination must preserve the synthetic endnote appendix"
        );
        assert!(
            cached
                .pages
                .iter()
                .flat_map(|page| page.placed.iter())
                .any(|placed| placed.fragment.node_id() == node(9_101)),
            "the referenced endnote body should remain visible through the cached entry point"
        );
    }
}

#[cfg(test)]
mod cross_paragraph_float_tests {
    use super::*;
    use crate::shape::ParleyShaper;
    use casual_doc_model::v1::{
        AnchorHorizontal, AnchorVertical, AnchoredDrawing, Definitions, DrawingAnchor, Extent,
        GridColumn, HorizontalAlign, HorizontalAnchor, HorizontalPosition, InlineNode, MediaId,
        MediaReference, Paragraph, ParagraphProperties, Run, RunProperties, Table, TableCell,
        TableCellProperties, TableProperties, TableRow, TableRowProperties, VerticalAlign,
        VerticalAnchor, VerticalPosition, WrapDistances, WrapMode,
    };

    fn node(id: u64) -> NodeId {
        NodeId::from_parts(81, id).unwrap()
    }

    fn paragraph(id: u64, text: String, extra: Vec<InlineNode>) -> BlockNode {
        let mut inlines = vec![InlineNode::Run(Run {
            id: node(id + 100),
            properties: RunProperties::default().into(),
            text,
        })];
        inlines.extend(extra);
        BlockNode::Paragraph(Paragraph {
            id: node(id),
            properties: ParagraphProperties::default().into(),
            inlines,
        })
    }

    fn floating_document() -> (Document, NodeId) {
        let media = MediaId::new(node(900));
        let mut definitions = Definitions::default();
        definitions.media.insert(
            media,
            MediaReference {
                relationship_id: "rIdFloat".to_owned(),
                media_type: "image/png".to_owned(),
                part_name: "word/media/float.png".to_owned(),
            },
        );
        let drawing = InlineNode::AnchoredDrawing(Box::new(AnchoredDrawing {
            hyperlink: None,
            opacity: None,
            id: node(901),
            media,
            extent: Extent {
                width_emu: 1_500 * 635,
                height_emu: 1_800 * 635,
            },
            anchor: DrawingAnchor {
                horizontal: AnchorHorizontal {
                    relative_from: HorizontalAnchor::Margin,
                    position: HorizontalPosition::Align(HorizontalAlign::Left),
                },
                vertical: AnchorVertical {
                    relative_from: VerticalAnchor::Paragraph,
                    position: VerticalPosition::Align(VerticalAlign::Top),
                },
                wrap: WrapMode::Square,
                wrap_text: None,
                wrap_distances: WrapDistances::default(),
                wrap_polygon: None,
                behind_doc: false,
            },
            descr: None,
            relative_height: None,
            crop: None,
            border: None,
            flip_h: false,
            flip_v: false,
            rotation: None,
        }));
        let target = node(2);
        let prose = "following paragraph text wraps beside the floating object ".repeat(90);
        let document = Document::new(
            node(999),
            vec![
                paragraph(1, "anchor".to_owned(), vec![drawing]),
                paragraph(2, prose, Vec::new()),
                paragraph(3, "after".to_owned(), Vec::new()),
            ],
            definitions,
        )
        .unwrap();
        (document, target)
    }

    fn backward_margin_float_document() -> (Document, NodeId) {
        let media = MediaId::new(node(920));
        let mut definitions = Definitions::default();
        definitions.media.insert(
            media,
            MediaReference {
                relationship_id: "rIdBackwardFloat".to_owned(),
                media_type: "image/png".to_owned(),
                part_name: "word/media/backward-float.png".to_owned(),
            },
        );
        let drawing = InlineNode::AnchoredDrawing(Box::new(AnchoredDrawing {
            hyperlink: None,
            opacity: None,
            id: node(921),
            media,
            extent: Extent {
                width_emu: 1_500 * 635,
                height_emu: 1_800 * 635,
            },
            anchor: DrawingAnchor {
                horizontal: AnchorHorizontal {
                    relative_from: HorizontalAnchor::Margin,
                    position: HorizontalPosition::Align(HorizontalAlign::Left),
                },
                vertical: AnchorVertical {
                    relative_from: VerticalAnchor::Margin,
                    position: VerticalPosition::Align(VerticalAlign::Top),
                },
                wrap: WrapMode::Square,
                wrap_text: None,
                wrap_distances: WrapDistances::default(),
                wrap_polygon: None,
                behind_doc: false,
            },
            descr: None,
            relative_height: None,
            crop: None,
            border: None,
            flip_h: false,
            flip_v: false,
            rotation: None,
        }));
        let target = node(11);
        let document = Document::new(
            node(998),
            vec![
                paragraph(
                    11,
                    "paragraph before its later anchor must wrap around the margin object "
                        .repeat(18),
                    Vec::new(),
                ),
                paragraph(12, "later anchor".to_owned(), vec![drawing]),
            ],
            definitions,
        )
        .unwrap();
        (document, target)
    }

    #[test]
    fn square_float_excludes_intersecting_lines_in_following_paragraph() {
        let (document, target) = floating_document();
        let shaper = ParleyShaper::new();
        let layout = paginate_document(&document, &shaper);
        assert_eq!(
            layout,
            paginate_document(&document, &shaper),
            "bounded float reflow must be deterministic"
        );

        let page = &layout.pages[0];
        let float = page.anchored.first().expect("placed anchored drawing");
        let placed = page
            .placed
            .iter()
            .find(|placed| placed.fragment.node_id() == target)
            .expect("following paragraph");
        let BlockFragment::Paragraph { lines, .. } = &placed.fragment else {
            panic!("target should be a paragraph");
        };

        let mut shifted = 0;
        let mut restored = 0;
        for line in &lines.lines {
            let Some(run) = line.runs.first() else {
                continue;
            };
            let baseline = placed.rect.origin.y + run.origin.y;
            let x = placed.rect.origin.x + run.origin.x;
            if baseline < float.rect.bottom() {
                assert!(
                    x >= float.rect.right(),
                    "line at y={} starts at {}, inside float ending at {}",
                    baseline.raw(),
                    x.raw(),
                    float.rect.right().raw()
                );
                shifted += 1;
            } else if run.origin.x == Twip::ZERO {
                restored += 1;
            }
        }
        assert!(
            shifted >= 2,
            "the float should affect multiple following lines"
        );
        assert!(
            restored >= 1,
            "full paragraph measure should return below the float"
        );

        let mut cache = GalleyCache::new();
        let cached =
            paginate_document_cached(&document, &shaper, &mut cache, &DirtySet::everything());
        assert_eq!(
            cached, layout,
            "cached entry point uses the same fixed point"
        );
    }

    #[test]
    fn margin_float_can_exclude_a_paragraph_before_its_anchor() {
        let (document, target) = backward_margin_float_document();
        let layout = paginate_document(&document, &ParleyShaper::new());
        let page = &layout.pages[0];
        let float = page.anchored.first().expect("placed anchored drawing");
        let placed = page
            .placed
            .iter()
            .find(|placed| placed.fragment.node_id() == target)
            .expect("paragraph before the anchor");
        let BlockFragment::Paragraph { lines, .. } = &placed.fragment else {
            panic!("target should be a paragraph");
        };

        let intersecting: Vec<_> = lines
            .lines
            .iter()
            .filter_map(|line| line.runs.first())
            .filter(|run| placed.rect.origin.y + run.origin.y < float.rect.bottom())
            .collect();
        assert!(
            intersecting.len() >= 2,
            "the margin float should intersect multiple earlier lines"
        );
        assert!(
            intersecting
                .iter()
                .all(|run| { placed.rect.origin.x + run.origin.x >= float.rect.right() })
        );
    }

    /// A page/margin-relative float anchored in an ordinary paragraph, followed
    /// immediately by a one-cell table whose cell paragraph overlaps the float's
    /// resolved band. Returns the document, the table's node id, and the cell
    /// paragraph's node id (the exclusion target).
    fn margin_float_over_table_cell_document() -> (Document, NodeId, NodeId) {
        let media = MediaId::new(node(940));
        let mut definitions = Definitions::default();
        definitions.media.insert(
            media,
            MediaReference {
                relationship_id: "rIdTableFloat".to_owned(),
                media_type: "image/png".to_owned(),
                part_name: "word/media/table-float.png".to_owned(),
            },
        );
        let drawing = InlineNode::AnchoredDrawing(Box::new(AnchoredDrawing {
            hyperlink: None,
            opacity: None,
            id: node(941),
            media,
            extent: Extent {
                width_emu: 1_500 * 635,
                height_emu: 1_800 * 635,
            },
            anchor: DrawingAnchor {
                horizontal: AnchorHorizontal {
                    relative_from: HorizontalAnchor::Margin,
                    position: HorizontalPosition::Align(HorizontalAlign::Left),
                },
                vertical: AnchorVertical {
                    relative_from: VerticalAnchor::Margin,
                    position: VerticalPosition::Align(VerticalAlign::Top),
                },
                wrap: WrapMode::Square,
                wrap_text: None,
                wrap_distances: WrapDistances::default(),
                wrap_polygon: None,
                behind_doc: false,
            },
            descr: None,
            relative_height: None,
            crop: None,
            border: None,
            flip_h: false,
            flip_v: false,
            rotation: None,
        }));
        let table_id = node(950);
        let cell_paragraph = node(953);
        let table = BlockNode::Table(Box::new(Table {
            id: table_id,
            grid: vec![GridColumn {
                width_twips: Some(9_000),
            }],
            grid_change: None,
            properties: TableProperties::default(),
            rows: vec![TableRow {
                id: node(951),
                properties: TableRowProperties::default(),
                cells: vec![TableCell {
                    id: node(952),
                    properties: TableCellProperties::default(),
                    blocks: vec![BlockNode::Paragraph(Paragraph {
                        id: cell_paragraph,
                        properties: ParagraphProperties::default().into(),
                        inlines: vec![InlineNode::Run(Run {
                            id: node(954),
                            properties: RunProperties::default().into(),
                            text: "table cell text wraps beside the page relative float "
                                .repeat(20),
                        })],
                    })],
                }],
            }],
        }));
        let document = Document::new(
            node(939),
            vec![paragraph(1, "anchor".to_owned(), vec![drawing]), table],
            definitions,
        )
        .unwrap();
        (document, table_id, cell_paragraph)
    }

    /// P1F-FLOAT-SQUARE-2: a page/margin-relative float's exclusion zone now
    /// reaches a paragraph nested inside a table cell, not just top-level body
    /// paragraphs.
    #[test]
    fn margin_float_excludes_a_paragraph_nested_in_a_table_cell() {
        let (document, table_id, target) = margin_float_over_table_cell_document();
        let shaper = ParleyShaper::new();
        let layout = paginate_document(&document, &shaper);

        let page = &layout.pages[0];
        let float = page.anchored.first().expect("placed anchored drawing");
        let placed_row = page
            .placed
            .iter()
            .find(|placed| {
                matches!(&placed.fragment, BlockFragment::TableRow { table, .. } if *table == table_id)
            })
            .expect("table row placed on the float's page");
        let BlockFragment::TableRow { cells, .. } = &placed_row.fragment else {
            unreachable!("matched above")
        };
        let cell = &cells[0];
        let cell_height = cell.box_height(placed_row.fragment.height());
        let content_origin = Point::new(
            placed_row.rect.origin.x + cell.x + cell.margins.start,
            placed_row.rect.origin.y + cell.cell_spacing.top + cell.content_y_offset(cell_height),
        );
        let BlockFragment::Paragraph { id, lines, .. } = &cell.blocks[0] else {
            panic!("expected the cell's paragraph fragment");
        };
        assert_eq!(*id, target);

        let mut shifted = 0;
        let mut restored = 0;
        for line in &lines.lines {
            let Some(run) = line.runs.first() else {
                continue;
            };
            let baseline = content_origin.y + run.origin.y;
            let x = content_origin.x + run.origin.x;
            if baseline < float.rect.bottom() {
                assert!(
                    x >= float.rect.right(),
                    "cell line at y={} starts at {}, inside float ending at {}",
                    baseline.raw(),
                    x.raw(),
                    float.rect.right().raw()
                );
                shifted += 1;
            } else if run.origin.x == Twip::ZERO {
                restored += 1;
            }
        }
        assert!(
            shifted >= 1,
            "the float should exclude at least one line inside the table cell"
        );
        assert!(
            restored >= 1,
            "the cell's full width should return below the float"
        );

        assert_eq!(
            layout,
            paginate_document(&document, &shaper),
            "bounded float reflow into a table cell must be deterministic"
        );
        let mut cache = GalleyCache::new();
        let cached =
            paginate_document_cached(&document, &shaper, &mut cache, &DirtySet::everything());
        assert_eq!(
            cached, layout,
            "cached entry point uses the same fixed point"
        );
    }
}
