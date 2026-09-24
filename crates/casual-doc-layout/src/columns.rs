//! Column-aware, multi-section pagination — flowing a document's body into the
//! newspaper-style columns Word declares with `w:cols`, section by section.
//!
//! The single-column [`paginate`](crate::paginate::paginate) fills one page top to
//! bottom. A multi-column section instead fills **column 0** top to bottom, then
//! column 1, … then the next page (Word's non-balanced fill order). Content that
//! wraps at the *column* width — not the full body width — packs far more onto a
//! page, which is the dominant page-fidelity win for column-heavy documents such
//! as the SDS corpus sample (single-column flow spreads it over ~7 extra pages).
//!
//! ## Sections and the `continuous` mid-page case
//!
//! A DOCX body is a sequence of sections, each ending at the paragraph whose
//! `w:pPr` carries the section's `w:sectPr` (the final section is body-level). Each
//! section declares its own column layout, so the driver
//! ([`paginate_columns`]) partitions the body into per-section runs, flows each at
//! its column width, and paginates them in order while **carrying the page cursor
//! across section boundaries**:
//!
//! - a `nextPage` (and the unspecified default) section starts a fresh page;
//! - an `evenPage`/`oddPage` section starts a fresh page **of that parity**,
//!   padding with one blank page when the next page would have the wrong one
//!   (`docs/105` FID-L-03) — see `ColPaginator::pad_to_parity`;
//! - a `continuous` (or `nextColumn`) section continues on the *same* page, its
//!   column band beginning just below the previous section's deepest content — the
//!   common, hard "column-set change mid-page" case the SDS exercises.
//!
//! ## Two-sided (bound) geometry
//!
//! The binding gutter (`w:pgMar/@w:gutter`) is folded into the inside margin when
//! the [`PageConfig`] is derived, so it is already part of the content area and
//! the column x positions. `w:mirrorMargins` is the per-page half of that: on a
//! verso (even) page Word swaps the inside and outside margins, which moves the
//! whole body band horizontally without changing its width. This paginator
//! applies that shift as it emits each page (`docs/105` FID-L-16).
//!
//! ## Documented approximations (deferred fidelity)
//!
//! - **Ordinary soft-split continuation across unequal widths.** Unequal physical
//!   columns receive width-specific galleys, and block/forced-break boundaries
//!   select the matching layout. A paragraph split only because the current column
//!   runs out of vertical space is moved whole when its alternate-width layout fits
//!   the next column; an oversized paragraph may still retain its starting-width
//!   line layout across that soft split until the shaper exposes a resumable cursor.
//! - **No last-page balancing.** Columns fill in order; the final page of a section
//!   is left unbalanced (Word balances the last page of a non-final column section).
//!   Because a following `continuous` section must begin below *all* of the previous
//!   section's columns to avoid overlap, an unbalanced last page can push the next
//!   section onto a new page where balancing would have kept it.
//! - A table inside a multi-column section flows within the current column but does
//!   not repeat its header rows across columns.

use casual_doc_model::NodeId;
use casual_doc_model::v1::{SectionBoundary, SectionColumns};

use crate::block::{BlockFragment, BoxMetrics, BreakControl, CellFragment, ParagraphDecor};
use crate::page::{ColumnSeparator, FlowPos, FlowSpan, Page, PaginatedLayout, PlacedFragment};
use crate::paginate::{
    PageConfig, build_blank_page, build_page, make_row_chunk, slice_paragraph, split_cells,
};
use crate::text::LineLayout;
use crate::units::{Point, Rect, Size, Twip};

/// The minimum lines kept together at a column/page break (Word's widow/orphan
/// default), mirroring [`crate::paginate`].
const MIN_WIDOW_ORPHAN: usize = 2;

/// Word's default inter-column spacing (`w:cols/@w:space`) when the attribute is
/// absent — 0.5 inch.
const DEFAULT_COLUMN_SPACE: i32 = 720;

/// One column's horizontal geometry: its leading edge and its width (the width the
/// section's content was flowed at, so lines never overrun the column).
#[derive(Clone, Copy, Debug)]
struct ColumnGeom {
    /// The column's leading-edge x (page-local twips).
    x: Twip,
    /// The column's width — the flow/line-break width.
    width: Twip,
}

/// The resolved column layout of one section: the ordered column geometries and
/// whether a separator rule is drawn between them.
#[derive(Clone, Debug)]
pub struct ColumnLayout {
    /// One geometry per column (at least one).
    columns: Vec<ColumnGeom>,
    /// Draw a separator rule between columns (`w:cols/@w:sep`). Consumed by the
    /// column paginator, which emits a [`ColumnSeparator`] per gap per page band.
    separator: bool,
}

impl ColumnLayout {
    /// A single full-width column spanning `content` — the layout for a
    /// single-column (or section-less) run.
    #[must_use]
    pub fn single(content: Rect) -> Self {
        Self {
            columns: vec![ColumnGeom {
                x: content.origin.x,
                width: content.size.width,
            }],
            separator: false,
        }
    }

    /// The canonical galley width. Equal columns share it; unequal-column sections
    /// also build per-column galleys via `flow_widths`, while retaining the
    /// widest galley as the deterministic canonical topology.
    #[must_use]
    pub fn flow_width(&self) -> Twip {
        self.columns
            .iter()
            .map(|c| c.width)
            .max_by_key(|w| w.raw())
            .unwrap_or(Twip::ZERO)
    }

    /// The physical flow width of every column, in placement order.
    ///
    /// Equal-width sections can share one galley. Unequal sections need one
    /// width-specific galley per entry so a fragment placed in a narrow column was
    /// actually line-broken at that narrow measure.
    #[must_use]
    pub(crate) fn flow_widths(&self) -> Vec<Twip> {
        self.columns.iter().map(|column| column.width).collect()
    }

    /// Whether every physical column has the same line-breaking measure.
    #[must_use]
    pub(crate) fn has_unequal_widths(&self) -> bool {
        self.columns.first().is_some_and(|first| {
            self.columns
                .iter()
                .any(|column| column.width != first.width)
        })
    }
}

/// Resolves a section's [`SectionColumns`] into a laid-out [`ColumnLayout`] across
/// the body content area.
///
/// When the section declares explicit unequal per-column widths (`w:equalWidth="0"`
/// with `w:col` entries), the columns are placed at those widths and the per-column
/// (`w:col/@w:space`) gaps, left to right from the content's leading edge. Otherwise
/// the content width is divided into *N* equal columns separated by the declared (or
/// default) inter-column space.
#[must_use]
pub fn column_layout(columns: &SectionColumns, content: Rect) -> ColumnLayout {
    let n = (columns.count.max(1)) as i32;
    let separator = columns.separator.unwrap_or(false);
    if n <= 1 {
        return ColumnLayout {
            columns: vec![ColumnGeom {
                x: content.origin.x,
                width: content.size.width,
            }],
            separator,
        };
    }
    // Explicit per-column widths win unless the section forces equal widths. Word
    // writes `w:col` widths with `equalWidth="0"`; a stray `equalWidth="1"` alongside
    // `w:col` means Word ignores the widths, so honor that.
    if !columns.columns.is_empty() && columns.equal_width != Some(true) {
        return unequal_layout(columns, content, separator);
    }
    let space = columns.space_twips.unwrap_or(DEFAULT_COLUMN_SPACE).max(0);
    let total_gap = space * (n - 1);
    let col_w = ((content.size.width.raw() - total_gap) / n).max(1);
    let geoms = (0..n)
        .map(|i| ColumnGeom {
            x: Twip(content.origin.x.raw() + i * (col_w + space)),
            width: Twip(col_w),
        })
        .collect();
    ColumnLayout {
        columns: geoms,
        separator,
    }
}

/// Lays out a section's explicit per-column widths (`w:col`) left to right from the
/// content's leading edge, each followed by its own (`w:col/@w:space`) gap — falling
/// back to the section space, then the default, when a column omits its gap.
fn unequal_layout(columns: &SectionColumns, content: Rect, separator: bool) -> ColumnLayout {
    let default_gap = columns.space_twips.unwrap_or(DEFAULT_COLUMN_SPACE).max(0);
    let last = columns.columns.len().saturating_sub(1);
    let mut x = content.origin.x.raw();
    let mut geoms = Vec::with_capacity(columns.columns.len());
    for (i, def) in columns.columns.iter().enumerate() {
        let width = def.width_twips.max(1);
        geoms.push(ColumnGeom {
            x: Twip(x),
            width: Twip(width),
        });
        // The gap follows every column but the last; a column's own `@w:space`
        // overrides the section default.
        if i < last {
            let gap = def.space_twips.unwrap_or(default_gap).max(0);
            x += width + gap;
        }
    }
    ColumnLayout {
        columns: geoms,
        separator,
    }
}

/// One section's paginated input: its page geometry, resolved column layout, the
/// galley flowed at the column width, and whether the section begins a new page.
#[derive(Clone, Debug)]
pub struct SectionRun {
    /// The section's page geometry (page box, margins, header/footer bands).
    pub config: PageConfig,
    /// The resolved (equal-width) column layout.
    pub layout: ColumnLayout,
    /// The section's body galley, flowed at [`ColumnLayout::flow_width`].
    pub galley: Vec<BlockFragment>,
    /// Width-specific galleys for unequal physical columns, indexed in the same
    /// order as the layout's physical columns. Empty for equal-width sections. Every
    /// entry must have the same block-fragment topology as [`Self::galley`].
    pub column_galleys: Vec<Vec<BlockFragment>>,
    /// `true` when the section starts on a fresh page (`nextPage`/`evenPage`/
    /// `oddPage`/default); `false` for `continuous`/`nextColumn`.
    pub starts_new_page: bool,
    /// The page parity the section must start on (`w:type="evenPage"` /
    /// `"oddPage"`), or `None` for every other start type. When the next page
    /// has the wrong parity the paginator inserts one blank page before the
    /// section, exactly as Word does.
    pub start_parity: Option<PageParity>,
    /// `w:mirrorMargins` — the document prints two-sided, so the inside and
    /// outside margins swap on verso (even) pages. A document setting rather
    /// than a section one, carried per run because page geometry is per section.
    pub mirror_margins: bool,
}

impl SectionRun {
    /// The galley whose line-breaking measure matches `column`. A malformed or
    /// topology-mismatched width-specific entry falls back to the canonical galley
    /// instead of risking an out-of-bounds fragment lookup.
    fn galley_for_column(&self, column: usize) -> &[BlockFragment] {
        self.column_galleys
            .get(column)
            .filter(|galley| galley.len() == self.galley.len())
            .map_or(self.galley.as_slice(), Vec::as_slice)
    }

    fn fragment_for_column(&self, column: usize, index: usize) -> &BlockFragment {
        &self.galley_for_column(column)[index]
    }
}

/// Whether a [`SectionBoundary`]'s start type begins a new page (as opposed to
/// continuing on the current one).
#[must_use]
pub fn section_starts_new_page(section: &SectionBoundary) -> bool {
    use casual_doc_model::v1::SectionType;
    !matches!(
        section.section_type,
        Some(SectionType::Continuous | SectionType::NextColumn)
    )
}

/// The parity of a page number — which side of a two-sided sheet it falls on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageParity {
    /// An even page number (the verso, left-hand side of a spread).
    Even,
    /// An odd page number (the recto, right-hand side of a spread).
    Odd,
}

impl PageParity {
    /// Whether the (1-based) page `number` already has this parity.
    #[must_use]
    pub fn matches(self, number: u32) -> bool {
        match self {
            Self::Even => number.is_multiple_of(2),
            Self::Odd => !number.is_multiple_of(2),
        }
    }
}

/// The page parity a [`SectionBoundary`] must start on: `w:type="evenPage"` and
/// `"oddPage"` demand one, every other start type (including the `nextPage`
/// default) accepts whichever page comes next.
#[must_use]
pub fn section_start_parity(section: &SectionBoundary) -> Option<PageParity> {
    use casual_doc_model::v1::SectionType;
    match section.section_type {
        Some(SectionType::EvenPage) => Some(PageParity::Even),
        Some(SectionType::OddPage) => Some(PageParity::Odd),
        Some(SectionType::NextPage | SectionType::Continuous | SectionType::NextColumn) | None => {
            None
        }
    }
}

/// Paginates an ordered list of section runs into a single [`PaginatedLayout`],
/// carrying the page cursor across section boundaries (so a `continuous` section
/// shares the previous section's page). This is the multi-column counterpart to
/// [`paginate`](crate::paginate::paginate); the incremental/halt machinery is
/// deliberately not part of it (the driver uses it on full runs only).
#[must_use]
pub fn paginate_columns(runs: &[SectionRun]) -> PaginatedLayout {
    paginate_columns_with_reservations(runs, &[])
}

/// [`paginate_columns`], with page-local bottom reservations subtracted from each
/// emitted page's body content area. This is used by footnote pagination; callers
/// provide reservations from a previous pass, and missing entries mean no
/// reservation for that page.
#[must_use]
pub(crate) fn paginate_columns_with_reservations(
    runs: &[SectionRun],
    reservations: &[Twip],
) -> PaginatedLayout {
    let mut p = ColPaginator::new(reservations);
    for run in runs {
        p.run_section(run);
    }
    p.flush_page();
    PaginatedLayout { pages: p.pages }
}

/// Mutable state for the column paginator, walked section by section and, within a
/// section, column by column.
struct ColPaginator {
    /// Emitted pages.
    pages: Vec<Page>,
    /// Fragments placed on the page currently being built.
    placed: Vec<PlacedFragment>,
    /// The current section's page geometry (fixed while its run is processed).
    config: PageConfig,
    /// The document mirrors its margins for two-sided printing
    /// (`w:mirrorMargins`), so verso pages swap the inside and outside margins.
    mirror_margins: bool,
    /// The current section's body content area, already shifted for the parity
    /// of the page being built when the document mirrors its margins.
    content: Rect,
    /// The current section's column geometries.
    columns: Vec<ColumnGeom>,
    /// Whether the current section draws a separator rule between its columns.
    separator: bool,
    /// Separator rules accumulated for the page currently being built, flushed into
    /// the [`Page`] alongside its placed fragments.
    page_separators: Vec<ColumnSeparator>,
    /// The current column index within the active band.
    col: usize,
    /// The y at which the active column band begins (content top on a fresh page,
    /// or just below the previous section's content for a `continuous` section).
    band_top: Twip,
    /// The placement cursor within the current column.
    y: Twip,
    /// The deepest y any column reached on the current page — where a following
    /// `continuous` section's band begins (so it never overlaps a filled column).
    page_max_y: Twip,
    /// The flow provenance base for the current section: the number of galley
    /// fragments in all prior sections, so per-section galley indices become
    /// document-global monotonic flow positions. Zero for a single-section
    /// document, which makes the flow spans identical to the single-column
    /// [`paginate`](crate::paginate::paginate).
    base: u32,
    /// Running sum of processed section galley lengths (the next section's `base`).
    next_base: u32,
    /// Flow position of the next content to be placed.
    at: FlowPos,
    /// Flow position where the current (open) page began.
    page_start: FlowPos,
    /// Page-local reserved bottom bands, indexed by the page currently being
    /// produced.
    reservations: Vec<Twip>,
}

impl ColPaginator {
    fn new(reservations: &[Twip]) -> Self {
        Self {
            pages: Vec::new(),
            placed: Vec::new(),
            config: PageConfig {
                section: casual_doc_model::v1::SectionId::new(NodeId::from_parts(1, 1).unwrap()),
                page_size: Size::new(Twip(12_240), Twip(15_840)),
                margin_top: Twip(1_440),
                margin_bottom: Twip(1_440),
                margin_start: Twip(1_440),
                margin_end: Twip(1_440),
                header_distance: Twip(720),
                footer_distance: Twip(720),
                header_height: Twip::ZERO,
                footer_height: Twip::ZERO,
            },
            mirror_margins: false,
            content: Rect::new(
                Point::new(Twip::ZERO, Twip::ZERO),
                Size::new(Twip::ZERO, Twip::ZERO),
            ),
            columns: Vec::new(),
            separator: false,
            page_separators: Vec::new(),
            col: 0,
            band_top: Twip::ZERO,
            y: Twip::ZERO,
            page_max_y: Twip::ZERO,
            base: 0,
            next_base: 0,
            at: FlowPos::at(0),
            page_start: FlowPos::at(0),
            reservations: reservations.to_vec(),
        }
    }

    /// Processes one section: enters its geometry, positions its column band
    /// (new page vs. mid-page continuation), then fills its galley into columns.
    fn run_section(&mut self, run: &SectionRun) {
        self.base = self.next_base;
        self.next_base += run.galley.len() as u32;
        self.at = FlowPos::at(self.base);

        // Transition off the previous section's band, closing its separator rule
        // with the *previous* section's geometry — before this section's geometry is
        // adopted below. A `nextPage`-style section starts a fresh page (unless the
        // page is already empty); a `continuous` section keeps the page and drops
        // its band below the previous section's deepest content.
        if run.starts_new_page && !self.placed.is_empty() {
            self.flush_page();
        } else if !self.placed.is_empty() {
            self.close_band(self.page_max_y);
        }
        // `w:type="evenPage"`/`"oddPage"`: pad with a blank page when the page
        // this section would open on has the wrong parity. Emitted *before* the
        // new geometry is adopted below, because Word charges the blank to the
        // preceding section — it keeps that section's page setup and running
        // content (`docs/105` FID-L-03).
        if let Some(parity) = run.start_parity {
            self.pad_to_parity(parity);
        }

        self.config = run.config;
        self.mirror_margins = run.mirror_margins;
        self.content = self.current_content_area();
        self.columns = run.layout.columns.clone();
        self.separator = run.layout.separator;
        self.band_top = if self.placed.is_empty() {
            self.content.origin.y
        } else {
            self.page_max_y.max(self.content.origin.y)
        };
        // No room left on this page for the new band → start a fresh page.
        if self.band_top.raw() >= self.content.bottom().raw() {
            self.flush_page();
            self.band_top = self.content.origin.y;
        }
        self.col = 0;
        self.y = self.band_top;

        // Walk keep-with-next groups, exactly like the single-column paginator, but
        // "advance" moves to the next column (then the next page).
        let mut i = 0;
        while i < run.galley.len() {
            // Keep grouping is structural and therefore identical across the
            // width-specific galleys. Heights come from the currently active
            // column's galley.
            let frags = run.galley_for_column(self.col);
            let mut j = i;
            while j < frags.len() && frags[j].break_control().keep_next {
                j += 1;
            }
            j = (j + 1).min(frags.len());
            // A forced page break (`pageBreakBefore`) inside a keep-next chain wins
            // over keep-with-next (as in Word): end the group right before the first
            // later member that forces a new page, so its break is honored on the
            // next iteration as that group's head (`group[0]`, checked below).
            if let Some(k) = frags[i..j]
                .iter()
                .enumerate()
                .skip(1)
                .find(|(_, f)| f.break_control().page_break_before)
                .map(|(k, _)| k)
            {
                j = i + k;
            }
            let group = &frags[i..j];
            let group_h: i32 = group.iter().map(|f| f.height().raw()).sum();
            let col_full = self.content.size.height.raw();
            let group_fits_col = group_h <= col_full;
            let is_keep_group = group.len() > 1 || group[0].break_control().keep_lines;
            let forced = group[0].break_control().page_break_before;
            let doesnt_fit_here = self.y.raw() + group_h > self.content.bottom().raw();

            if !self.at_page_start() {
                if forced {
                    self.new_page();
                } else if is_keep_group && group_fits_col && doesnt_fit_here {
                    self.advance();
                }
            }

            let allow_split = !is_keep_group || !group_fits_col;
            for offset in 0..group.len() {
                self.place(run, i + offset, allow_split);
            }
            i = j;
        }
    }

    /// Emits the blank page an `evenPage`/`oddPage` section needs when the page
    /// it would otherwise open on has the wrong parity (`docs/105` FID-L-03).
    ///
    /// At most one page is ever inserted, since parity alternates. Nothing is
    /// inserted at the very start of the document: there is no preceding page to
    /// pad after, and Word does not open a document with a blank sheet. The
    /// inserted page belongs to the **preceding** section — it keeps that
    /// section's [`PageConfig`] and, through [`Page::section`], is given that
    /// section's header/footer by the running-content pass, which is what Word
    /// paints on it.
    fn pad_to_parity(&mut self, parity: PageParity) {
        // A parity section always starts a new page, so the caller has already
        // closed any open page: nothing is in flight here.
        debug_assert!(self.placed.is_empty(), "parity pad with an open page");
        let Some(anchor) = self.pages.last().map(|page| page.end) else {
            return;
        };
        let next_number = self.pages.len() as u32 + 1;
        if parity.matches(next_number) {
            return;
        }
        let flow = FlowSpan {
            start: self.at,
            end: self.at,
        };
        let content = self.current_content_area();
        let page = build_blank_page(self.pages.len(), &self.config, content, flow, anchor);
        self.pages.push(page);
        // A blank page carries no band, so no separator rule can be owed on it.
        self.page_separators.clear();
        self.reset_to_fresh_page();
    }

    /// Whether the page currently being built is a verso (even) page of a
    /// document that mirrors its margins — where Word swaps the inside and
    /// outside margins for two-sided printing.
    fn building_verso(&self) -> bool {
        self.mirror_margins && (self.pages.len() as u32 + 1).is_multiple_of(2)
    }

    /// The horizontal offset the page being built carries relative to the
    /// section's recto geometry. Column x positions were resolved against the
    /// recto content area, so every placement adds this; it is zero for every
    /// page of a document that does not mirror its margins.
    fn x_shift(&self) -> Twip {
        self.content.origin.x - self.config.content_area().origin.x
    }

    /// Whether nothing has been placed yet on the current fresh page (guards
    /// against a leading blank page from a forced/keep break at the very start).
    fn at_page_start(&self) -> bool {
        self.placed.is_empty() && self.col == 0 && self.y.raw() == self.content.origin.y.raw()
    }

    /// Whether the current column is still empty (its cursor is at the band top).
    fn at_column_start(&self) -> bool {
        self.y.raw() == self.band_top.raw()
    }

    /// The current column's geometry.
    fn column(&self) -> ColumnGeom {
        self.columns[self.col.min(self.columns.len().saturating_sub(1))]
    }

    /// Remaining height below the cursor in the current column.
    fn remaining(&self) -> i32 {
        self.content.bottom().raw() - self.y.raw()
    }

    /// Moves to the next column of the current band, or to a fresh page when the
    /// last column is full.
    fn advance(&mut self) {
        self.page_max_y = Twip(self.page_max_y.raw().max(self.y.raw()));
        if self.col + 1 < self.columns.len() {
            self.col += 1;
            self.y = self.band_top;
        } else {
            self.flush_page();
        }
    }

    /// Ends the current page (flushing it if non-empty) and resets to a fresh
    /// full-height column band at the content top.
    fn new_page(&mut self) {
        self.flush_page();
    }

    /// Places fragment `idx`, splitting a paragraph or table row across columns
    /// when `allow_split` and it does not fit whole.
    fn place(&mut self, run: &SectionRun, idx: usize, allow_split: bool) {
        let gidx = self.base + idx as u32;
        self.at = FlowPos::at(gidx);
        let mut fragment = run.fragment_for_column(self.col, idx).clone();

        // A soft split can otherwise carry lines shaped for a wide column into the
        // following narrow column. Until ordinary split continuations have a
        // resumable shaping cursor, keep a paragraph whole when the next
        // different-width column can contain its width-specific layout. This
        // preserves every model byte while keeping placement and shaping widths
        // identical.
        let next_col = if self.col + 1 < self.columns.len() {
            self.col + 1
        } else {
            0
        };
        let next_capacity = if self.col + 1 < self.columns.len() {
            self.content.bottom() - self.band_top
        } else {
            self.content.size.height
        };
        let advance_whole = matches!(
            &fragment,
            BlockFragment::Paragraph {
                lines,
                break_control,
                ..
            } if !self.at_column_start()
                && allow_split
                && !break_control.keep_lines
                && lines.lines.len() > 1
                && !lines.lines.iter().any(|line| line.page_break_after)
                && fragment.height().raw() > self.remaining()
                && self.columns[next_col].width != self.column().width
                && run.fragment_for_column(next_col, idx).height() <= next_capacity
        );
        if advance_whole {
            self.advance();
            fragment = run.fragment_for_column(self.col, idx).clone();
        }

        match &fragment {
            BlockFragment::Paragraph {
                id,
                lines,
                box_metrics,
                break_control,
                decor,
            } if lines.lines.iter().any(|l| l.page_break_after)
                || (lines.lines.len() > 1 && allow_split && !break_control.keep_lines) =>
            {
                self.place_paragraph(
                    run,
                    idx,
                    gidx,
                    *id,
                    lines,
                    *box_metrics,
                    *break_control,
                    *decor,
                );
            }
            BlockFragment::TableRow {
                table, can_split, ..
            } => {
                self.place_table_row(run, idx, gidx, &fragment, *table, *can_split);
            }
            _ => {
                let mut selected = fragment;
                let mut height = selected.height();
                if !self.at_column_start() && height.raw() > self.remaining() {
                    self.advance();
                    selected = run.fragment_for_column(self.col, idx).clone();
                    height = selected.height();
                }
                self.push(selected, height, self.column().width);
                self.at = FlowPos::at(gidx + 1);
            }
        }
    }

    /// Appends a placed fragment at the current column cursor and advances `y`.
    /// Records the page's start flow position when it is the page's first content.
    fn push(&mut self, fragment: BlockFragment, height: Twip, width: Twip) {
        if self.placed.is_empty() {
            self.page_start = self.at;
        }
        let col = self.column();
        let rect = Rect::new(
            Point::new(col.x + self.x_shift(), self.y),
            Size::new(width, height),
        );
        self.placed.push(PlacedFragment {
            fragment,
            rect,
            section: Some(self.config.section),
        });
        self.y = self.y + height;
        self.page_max_y = Twip(self.page_max_y.raw().max(self.y.raw()));
    }

    /// Places a paragraph's lines, breaking across columns at line boundaries with
    /// widow/orphan control (mirrors `paginate::Paginator::place_paragraph`, with
    /// column advance instead of page flush). A forced column break advances to the
    /// next physical column; a forced page break starts a fresh page. When that
    /// transition changes an unequal column's width, the paragraph resumes from the
    /// same model offset in the new column's width-specific galley.
    #[allow(clippy::too_many_arguments)]
    fn place_paragraph(
        &mut self,
        run: &SectionRun,
        index: usize,
        gidx: u32,
        id: NodeId,
        lines: &LineLayout,
        box_metrics: BoxMetrics,
        break_control: BreakControl,
        decor: ParagraphDecor,
    ) {
        let mut active = lines.clone();
        let mut n = active.lines.len();
        let widow = break_control.widow_control;
        let mut start = 0;
        let mut is_head = true;
        while start < n {
            self.at = FlowPos {
                fragment: gidx,
                line: start as u32,
            };
            let space_before = if is_head {
                box_metrics.space_before.raw()
            } else {
                0
            };
            let avail = self.remaining() - space_before;

            let mut take = 0;
            let mut used = 0;
            while start + take < n {
                let h = active.lines[start + take].height.raw();
                if used + h > avail {
                    break;
                }
                used += h;
                take += 1;
            }

            if take == 0 {
                if self.at_column_start() {
                    take = 1;
                    used = active.lines[start].height.raw();
                } else {
                    self.advance();
                    continue;
                }
            }

            // A forced page/column break caps this chunk at the break line.
            let forced = active.lines[start..start + take]
                .iter()
                .position(|l| l.page_break_after);
            if let Some(k) = forced {
                let new_take = k + 1;
                used -= active.lines[start + new_take..start + take]
                    .iter()
                    .map(|l| l.height.raw())
                    .sum::<i32>();
                take = new_take;
            }
            let forced_line = forced.map(|relative| &active.lines[start + relative]);
            // Old serialized galleys carried only the boolean. Treat their hard
            // forced break as a page break; newly shaped lines retain the exact kind.
            let forced_kind = forced_line.map(|line| match line.line_break {
                crate::text::LineBreak::Column => crate::text::LineBreak::Column,
                _ => crate::text::LineBreak::Page,
            });
            let forced_offset = forced_line.map(|line| line.range.end.offset);
            let forced = forced_kind.is_some();

            // Orphan control: don't strand fewer than the minimum head lines.
            if !forced
                && widow
                && is_head
                && !self.at_column_start()
                && start + take < n
                && take < MIN_WIDOW_ORPHAN
                && n >= MIN_WIDOW_ORPHAN
            {
                self.advance();
                continue;
            }
            // Widow control: don't leave a lone trailing line.
            if !forced
                && widow
                && start + take < n
                && (n - start - take) < MIN_WIDOW_ORPHAN
                && take > 1
            {
                take -= 1;
                used -= active.lines[start + take].height.raw();
            }

            let is_tail = start + take == n;
            let space_after = if is_tail {
                box_metrics.space_after.raw()
            } else {
                0
            };
            let chunk = slice_paragraph(
                id,
                &active,
                box_metrics,
                break_control,
                decor,
                start..start + take,
            );
            let width = self.column().width;
            self.push(chunk, Twip(used + space_before + space_after), width);
            start += take;
            is_head = false;
            self.at = if start == n {
                FlowPos::at(gidx + 1)
            } else {
                FlowPos {
                    fragment: gidx,
                    line: start as u32,
                }
            };

            if let Some(kind) = forced_kind {
                match kind {
                    crate::text::LineBreak::Column => self.advance(),
                    crate::text::LineBreak::Page => self.new_page(),
                    _ => unreachable!("forced kind is page or column"),
                }

                // The alternate-width layout contains the same explicit break at
                // the same model byte boundary. Resume immediately after it so the
                // tail is line-broken at the new physical column width.
                if start < n
                    && let BlockFragment::Paragraph {
                        lines: alternate, ..
                    } = run.fragment_for_column(self.col, index)
                    && let Some(offset) = forced_offset
                    && let Some(position) = alternate.lines.iter().position(|line| {
                        line.page_break_after
                            && line.range.end.offset == offset
                            && match kind {
                                crate::text::LineBreak::Column => {
                                    line.line_break == crate::text::LineBreak::Column
                                }
                                crate::text::LineBreak::Page => {
                                    line.line_break != crate::text::LineBreak::Column
                                }
                                _ => false,
                            }
                    })
                {
                    active = alternate.clone();
                    n = active.lines.len();
                    start = position + 1;
                }
            } else if start < n {
                self.advance();
            }
        }
    }

    /// Places a table row: whole when it fits the current column; split across
    /// columns when it is splittable and taller than the remaining space; otherwise
    /// moved whole to the next column.
    fn place_table_row(
        &mut self,
        run: &SectionRun,
        index: usize,
        gidx: u32,
        fragment: &BlockFragment,
        table: NodeId,
        can_split: bool,
    ) {
        let _ = table;
        let mut selected = fragment.clone();
        let mut height = selected.height();
        let fits = height.raw() <= self.remaining();
        if !fits && can_split && !self.remaining_too_small() {
            self.split_table_row(gidx, &selected);
            return;
        }
        if !fits && !self.at_column_start() {
            self.advance();
            selected = run.fragment_for_column(self.col, index).clone();
            height = selected.height();
        }
        self.push(selected, height, self.column().width);
        self.at = FlowPos::at(gidx + 1);
    }

    /// Whether the remaining column space is too small to fit any row content
    /// (avoids splitting into a sliver).
    fn remaining_too_small(&self) -> bool {
        self.remaining() <= 0
    }

    /// Splits a table row across column boundaries at block/line boundaries within
    /// its cells (mirrors `paginate::Paginator::split_table_row`, advancing columns
    /// instead of pages, without cross-column header repetition).
    fn split_table_row(&mut self, gidx: u32, fragment: &BlockFragment) {
        let BlockFragment::TableRow {
            id,
            table,
            cells,
            can_split,
            header,
            ..
        } = fragment
        else {
            return;
        };
        let is_header = *header;
        let width = self.column().width;
        let mut remaining: Vec<CellFragment> = cells.clone();
        let mut chunk = 0u32;
        loop {
            self.at = FlowPos {
                fragment: gidx,
                line: chunk,
            };
            let (head, tail, used) = split_cells(&remaining, self.remaining());
            if used == 0 {
                if self.at_column_start() {
                    let h = BlockFragment::cells_content_height(&remaining);
                    let row = make_row_chunk(*id, *table, remaining, h, *can_split, is_header);
                    self.push(row, h, width);
                    self.at = FlowPos::at(gidx + 1);
                    return;
                }
                self.advance();
                continue;
            }
            let row = make_row_chunk(*id, *table, head, Twip(used), *can_split, is_header);
            self.push(row, Twip(used), width);
            if tail.is_empty() {
                self.at = FlowPos::at(gidx + 1);
                return;
            }
            remaining = tail;
            chunk += 1;
            self.advance();
        }
    }

    /// Emits a separator rule for each inter-column gap of the current band, from
    /// the band top down to `bottom` (the band's deepest content on this page), when
    /// the section draws a separator and has more than one column. A gap whose band
    /// has no height (an empty band) contributes nothing.
    fn close_band(&mut self, bottom: Twip) {
        if !self.separator || self.columns.len() < 2 || bottom.raw() <= self.band_top.raw() {
            return;
        }
        let shift = self.x_shift().raw();
        for pair in self.columns.windows(2) {
            // The gap between the left column's trailing edge and the right column's
            // leading edge; the rule sits at its horizontal center.
            let gap_left = pair[0].x.raw() + pair[0].width.raw();
            let gap_right = pair[1].x.raw();
            let x = Twip((gap_left + gap_right) / 2 + shift);
            self.page_separators.push(ColumnSeparator {
                x,
                top: self.band_top,
                bottom,
            });
        }
    }

    /// Ends the current page (if it has content) and resets to a fresh full-height
    /// column band at the content top.
    fn flush_page(&mut self) {
        // Close the active band's separator on this page before the page ends.
        self.close_band(self.page_max_y);
        if !self.placed.is_empty() {
            let flow = FlowSpan {
                start: self.page_start,
                end: self.at,
            };
            let mut page = build_page(
                self.pages.len(),
                &self.config,
                self.content,
                std::mem::take(&mut self.placed),
                flow,
            );
            page.separators = std::mem::take(&mut self.page_separators);
            self.pages.push(page);
        } else {
            // A page with no content carries no separators.
            self.page_separators.clear();
        }
        self.reset_to_fresh_page();
    }

    /// Resets the placement cursor to a fresh full-height column band at the
    /// content top of the page that comes next — re-deriving the content area,
    /// since both the page-local footnote reservation and (under mirrored
    /// margins) the horizontal band position depend on the page's index.
    fn reset_to_fresh_page(&mut self) {
        self.col = 0;
        self.content = self.current_content_area();
        self.band_top = self.content.origin.y;
        self.y = self.content.origin.y;
        self.page_max_y = self.content.origin.y;
        self.page_start = self.at;
    }

    fn current_content_area(&self) -> Rect {
        let mut content = self.config.content_area();
        if self.building_verso() {
            // Mirrored margins: the inside margin (which carries the binding
            // gutter) moves to the right-hand edge on a verso page, so the band
            // starts at the outside margin instead. Its width is unchanged, which
            // is why pagination itself is parity-independent.
            content.origin.x = self.config.margin_end;
        }
        let reserved = self
            .reservations
            .get(self.pages.len())
            .copied()
            .unwrap_or(Twip::ZERO);
        if reserved > Twip::ZERO {
            content.size.height = Twip((content.size.height.raw() - reserved.raw()).max(0));
        }
        content
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::ParagraphDecor;
    use crate::model::{ModelPos, ModelRange};
    use crate::text::{Line, LineBreak, LineLayout};
    use casual_doc_model::v1::{ColumnDef, SectionId};

    /// A US-Letter page with 1-inch margins → a 9360×12960-twip content area.
    fn letter_config() -> PageConfig {
        PageConfig {
            section: SectionId::new(NodeId::from_parts(9, 1).unwrap()),
            page_size: Size::new(Twip(12_240), Twip(15_840)),
            margin_top: Twip(1_440),
            margin_bottom: Twip(1_440),
            margin_start: Twip(1_440),
            margin_end: Twip(1_440),
            header_distance: Twip(720),
            footer_distance: Twip(720),
            header_height: Twip::ZERO,
            footer_height: Twip::ZERO,
        }
    }

    /// A one-line paragraph fragment of a given height.
    fn paragraph(id: u64, height: Twip) -> BlockFragment {
        let node = NodeId::from_parts(id, 1).unwrap();
        let line = Line {
            runs: Vec::new(),
            ascent: height,
            descent: Twip::ZERO,
            height,
            clip: false,
            range: ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 0)),
            line_break: LineBreak::ParagraphEnd,
            page_break_after: false,
            bars: Vec::new(),
            images: Vec::new(),
            fields: Vec::new(),
            notes: Vec::new(),
            text_boxes: Vec::new(),
            rules: Vec::new(),
        };
        BlockFragment::Paragraph {
            id: node,
            lines: LineLayout { lines: vec![line] },
            box_metrics: BoxMetrics::default(),
            break_control: BreakControl::default(),
            decor: ParagraphDecor::default(),
        }
    }

    fn paragraph_with_forced_break(
        id: u64,
        first_height: Twip,
        tail_height: Twip,
        break_kind: LineBreak,
    ) -> BlockFragment {
        let node = NodeId::from_parts(id, 1).unwrap();
        let line = |start, end, height, line_break, page_break_after| Line {
            runs: Vec::new(),
            ascent: height,
            descent: Twip::ZERO,
            height,
            clip: false,
            range: ModelRange::new(ModelPos::new(node, start), ModelPos::new(node, end)),
            line_break,
            page_break_after,
            bars: Vec::new(),
            images: Vec::new(),
            fields: Vec::new(),
            notes: Vec::new(),
            text_boxes: Vec::new(),
            rules: Vec::new(),
        };
        BlockFragment::Paragraph {
            id: node,
            lines: LineLayout {
                lines: vec![
                    line(0, 5, first_height, break_kind, true),
                    line(5, 10, tail_height, LineBreak::ParagraphEnd, false),
                ],
            },
            box_metrics: BoxMetrics::default(),
            break_control: BreakControl::default(),
            decor: ParagraphDecor::default(),
        }
    }

    fn unequal_columns(config: PageConfig) -> ColumnLayout {
        column_layout(
            &SectionColumns {
                count: 2,
                space_twips: None,
                separator: None,
                equal_width: Some(false),
                columns: vec![
                    ColumnDef {
                        width_twips: 3_163,
                        space_twips: Some(40),
                    },
                    ColumnDef {
                        width_twips: 6_447,
                        space_twips: None,
                    },
                ],
            },
            config.content_area(),
        )
    }

    fn two_column_run(config: PageConfig, galley: Vec<BlockFragment>) -> SectionRun {
        let cols = SectionColumns {
            count: 2,
            space_twips: Some(720),
            separator: None,
            equal_width: None,
            columns: Vec::new(),
        };
        SectionRun {
            config,
            layout: column_layout(&cols, config.content_area()),
            galley,
            column_galleys: Vec::new(),
            starts_new_page: true,
            start_parity: None,
            mirror_margins: false,
        }
    }

    #[test]
    fn two_column_section_fills_both_columns_on_one_page() {
        let config = letter_config();
        let content_h = config.content_area().size.height.raw();
        // A galley ~1.5 columns tall: fills column 0, then part of column 1, on a
        // single page (the whole thing would need 1.5 pages single-column).
        let row_h = 240;
        let rows = (content_h + content_h / 2) / row_h;
        let galley: Vec<_> = (0..rows)
            .map(|i| paragraph(i as u64 + 1, Twip(row_h)))
            .collect();
        let layout = paginate_columns(&[two_column_run(config, galley)]);

        assert_eq!(layout.pages.len(), 1, "1.5 columns of content fit one page");
        let xs: std::collections::BTreeSet<i32> = layout.pages[0]
            .placed
            .iter()
            .map(|p| p.rect.origin.x.raw())
            .collect();
        assert_eq!(
            xs.len(),
            2,
            "content occupies two distinct column x-positions"
        );

        // Column 0 fills to the bottom before column 1 receives content.
        let col0_x = *xs.iter().next().unwrap();
        let col0_bottom = layout.pages[0]
            .placed
            .iter()
            .filter(|p| p.rect.origin.x.raw() == col0_x)
            .map(|p| p.rect.bottom().raw())
            .max()
            .unwrap();
        assert!(
            col0_bottom > content_h - row_h,
            "column 0 is filled to the page bottom before column 1 starts"
        );
    }

    #[test]
    fn column_width_is_half_the_content_minus_the_gap() {
        let config = letter_config();
        let cols = SectionColumns {
            count: 2,
            space_twips: Some(720),
            separator: None,
            equal_width: None,
            columns: Vec::new(),
        };
        let layout = column_layout(&cols, config.content_area());
        let content_w = config.content_area().size.width.raw();
        assert_eq!(layout.columns.len(), 2);
        assert_eq!(layout.columns[0].width.raw(), (content_w - 720) / 2);
        // Columns are laid left to right, separated by the gap.
        assert!(layout.columns[1].x.raw() > layout.columns[0].x.raw());
    }

    #[test]
    fn a_continuous_section_shares_the_page_below_the_previous() {
        let config = letter_config();
        // A short single-column section, then a continuous single-column section:
        // both land on one page, the second below the first.
        let first = SectionRun {
            config,
            layout: ColumnLayout::single(config.content_area()),
            galley: vec![paragraph(1, Twip(1_000))],
            column_galleys: Vec::new(),
            starts_new_page: true,
            start_parity: None,
            mirror_margins: false,
        };
        let second = SectionRun {
            config,
            layout: ColumnLayout::single(config.content_area()),
            galley: vec![paragraph(2, Twip(1_000))],
            column_galleys: Vec::new(),
            starts_new_page: false,
            start_parity: None,
            mirror_margins: false,
        };
        let layout = paginate_columns(&[first, second]);
        assert_eq!(
            layout.pages.len(),
            1,
            "a continuous section shares the page"
        );
        assert_eq!(layout.pages[0].placed.len(), 2);
        let top = config.content_area().origin.y.raw();
        assert_eq!(layout.pages[0].placed[0].rect.origin.y.raw(), top);
        assert_eq!(
            layout.pages[0].placed[1].rect.origin.y.raw(),
            top + 1_000,
            "the continuous section begins just below the previous section"
        );
    }

    #[test]
    fn unequal_columns_use_the_specified_per_column_widths_and_gaps() {
        let config = letter_config();
        // The SDS's "narrow label + wide content" split: 3163 + 40-gap + 6447.
        let cols = SectionColumns {
            count: 2,
            space_twips: None,
            separator: Some(true),
            equal_width: Some(false),
            columns: vec![
                ColumnDef {
                    width_twips: 3_163,
                    space_twips: Some(40),
                },
                ColumnDef {
                    width_twips: 6_447,
                    space_twips: None,
                },
            ],
        };
        let content = config.content_area();
        let layout = column_layout(&cols, content);
        assert_eq!(layout.columns.len(), 2);
        // Placed at their true widths, left to right from the content edge.
        assert_eq!(layout.columns[0].x.raw(), content.origin.x.raw());
        assert_eq!(layout.columns[0].width.raw(), 3_163);
        assert_eq!(
            layout.columns[1].x.raw(),
            content.origin.x.raw() + 3_163 + 40,
            "the second column follows the first plus its own 40-twip gap"
        );
        assert_eq!(layout.columns[1].width.raw(), 6_447);
        // The galley flows at the widest column so the wide content column wraps at
        // its true width.
        assert_eq!(layout.flow_width().raw(), 6_447);
    }

    #[test]
    fn forced_column_break_resumes_from_the_matching_wide_galley_offset() {
        let config = letter_config();
        let layout = unequal_columns(config);
        let canonical = paragraph_with_forced_break(1, Twip(640), Twip(740), LineBreak::Column);
        let narrow = paragraph_with_forced_break(1, Twip(310), Twip(410), LineBreak::Column);
        let wide = paragraph_with_forced_break(1, Twip(320), Twip(220), LineBreak::Column);
        let pages = paginate_columns(&[SectionRun {
            config,
            layout,
            galley: vec![canonical],
            column_galleys: vec![vec![narrow], vec![wide]],
            starts_new_page: true,
            start_parity: None,
            mirror_margins: false,
        }])
        .pages;

        assert_eq!(pages.len(), 1, "a column break remains on the same page");
        assert_eq!(pages[0].placed.len(), 2);
        let head = &pages[0].placed[0];
        let tail = &pages[0].placed[1];
        assert_eq!(head.rect.size.width, Twip(3_163));
        assert_eq!(head.rect.size.height, Twip(310));
        assert_eq!(tail.rect.size.width, Twip(6_447));
        assert_eq!(
            tail.rect.size.height,
            Twip(220),
            "the tail comes from the wide galley at the matching model offset"
        );
        assert!(tail.rect.origin.x > head.rect.origin.x);
    }

    #[test]
    fn forced_page_break_starts_a_new_page_instead_of_the_next_column() {
        let config = letter_config();
        let run = two_column_run(
            config,
            vec![paragraph_with_forced_break(
                1,
                Twip(300),
                Twip(200),
                LineBreak::Page,
            )],
        );
        let pages = paginate_columns(&[run]).pages;

        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].placed.len(), 1);
        assert_eq!(pages[1].placed.len(), 1);
        assert_eq!(
            pages[0].placed[0].rect.origin.x, pages[1].placed[0].rect.origin.x,
            "a page break restarts in the first physical column"
        );
    }

    #[test]
    fn equal_width_true_ignores_explicit_column_widths() {
        // A stray `w:equalWidth="1"` alongside `w:col` widths → equal division.
        let config = letter_config();
        let cols = SectionColumns {
            count: 2,
            space_twips: Some(720),
            separator: None,
            equal_width: Some(true),
            columns: vec![
                ColumnDef {
                    width_twips: 3_163,
                    space_twips: Some(40),
                },
                ColumnDef {
                    width_twips: 6_447,
                    space_twips: None,
                },
            ],
        };
        let content_w = config.content_area().size.width.raw();
        let layout = column_layout(&cols, config.content_area());
        assert_eq!(layout.columns[0].width.raw(), (content_w - 720) / 2);
        assert_eq!(layout.columns[1].width.raw(), (content_w - 720) / 2);
    }

    #[test]
    fn separator_rule_is_emitted_between_columns_when_sep_is_set() {
        let config = letter_config();
        let content_h = config.content_area().size.height.raw();
        let cols = SectionColumns {
            count: 2,
            space_twips: Some(720),
            separator: Some(true),
            equal_width: None,
            columns: Vec::new(),
        };
        let row_h = 240;
        let rows = content_h / row_h + 4; // spill into column 1
        let galley: Vec<_> = (0..rows)
            .map(|i| paragraph(i as u64 + 1, Twip(row_h)))
            .collect();
        let run = SectionRun {
            config,
            layout: column_layout(&cols, config.content_area()),
            galley,
            column_galleys: Vec::new(),
            starts_new_page: true,
            start_parity: None,
            mirror_margins: false,
        };
        let layout = paginate_columns(&[run]);
        assert_eq!(layout.pages.len(), 1);
        let seps = &layout.pages[0].separators;
        assert_eq!(seps.len(), 1, "one rule between the two columns");
        let content = config.content_area();
        // Centered in the 720-twip gap between the two equal columns.
        let col_w = (content.size.width.raw() - 720) / 2;
        let expected_x = content.origin.x.raw() + col_w + 720 / 2;
        assert_eq!(seps[0].x.raw(), expected_x);
        assert_eq!(seps[0].top.raw(), content.origin.y.raw());
        assert!(
            seps[0].bottom.raw() > seps[0].top.raw(),
            "the rule spans a positive band height"
        );
    }

    #[test]
    fn no_separator_rule_when_sep_is_absent() {
        let config = letter_config();
        let galley = vec![paragraph(1, Twip(400)), paragraph(2, Twip(400))];
        let layout = paginate_columns(&[two_column_run(config, galley)]);
        assert!(
            layout.pages.iter().all(|p| p.separators.is_empty()),
            "no separator rules without w:cols/@w:sep"
        );
    }

    /// A single-column run of one short paragraph, with an explicit start rule.
    fn parity_run(
        config: PageConfig,
        id: u64,
        start_parity: Option<PageParity>,
        mirror_margins: bool,
    ) -> SectionRun {
        SectionRun {
            config,
            layout: ColumnLayout::single(config.content_area()),
            galley: vec![paragraph(id, Twip(400))],
            column_galleys: Vec::new(),
            starts_new_page: true,
            start_parity,
            mirror_margins,
        }
    }

    #[test]
    fn a_parity_section_at_the_document_start_gets_no_leading_blank_page() {
        let config = letter_config();
        // Word does not open a document with a blank sheet, even when its first
        // section declares `w:type="evenPage"`.
        let layout = paginate_columns(&[parity_run(config, 1, Some(PageParity::Even), false)]);
        assert_eq!(layout.pages.len(), 1);
        assert_eq!(layout.pages[0].placed.len(), 1);
    }

    #[test]
    fn a_parity_blank_page_stays_with_the_preceding_section() {
        let first = letter_config();
        let mut second = letter_config();
        second.section = SectionId::new(NodeId::from_parts(77, 1).unwrap());
        let layout = paginate_columns(&[
            parity_run(first, 1, None, false),
            parity_run(second, 2, Some(PageParity::Odd), false),
        ]);

        assert_eq!(layout.pages.len(), 3, "the parity blank page is missing");
        let blank = &layout.pages[1];
        assert!(blank.placed.is_empty());
        assert_eq!(
            blank.section, first.section,
            "the blank page belongs to the section it follows, so that section's \
             header/footer is painted on it"
        );
        assert_eq!(blank.number, 2);
        // The blank collapses onto the last model position before it, keeping the
        // page model ranges monotonic for hit testing.
        assert_eq!(blank.start, blank.end);
        assert_eq!(blank.start, layout.pages[0].end);
        assert_eq!(layout.pages[2].section, second.section);
    }

    #[test]
    fn section_start_parity_maps_only_the_two_parity_break_types() {
        let mut section = SectionBoundary {
            id: SectionId::new(NodeId::from_parts(5, 1).unwrap()),
            page_size: casual_doc_model::v1::PageSize {
                width_twips: 12_240,
                height_twips: 15_840,
            },
            page_margins: casual_doc_model::v1::PageMargins {
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
            page_numbering: Default::default(),
            doc_grid: Default::default(),
            orientation: None,
            paper_source: Default::default(),
            page_borders: Default::default(),
            line_numbering: Default::default(),
            watermark: None,
            footnote_props: Default::default(),
            endnote_props: Default::default(),
            text_direction: None,
            bidi: false,
            section_change: None,
        };
        use casual_doc_model::v1::SectionType;
        for (start_type, expected) in [
            (None, None),
            (Some(SectionType::NextPage), None),
            (Some(SectionType::Continuous), None),
            (Some(SectionType::NextColumn), None),
            (Some(SectionType::EvenPage), Some(PageParity::Even)),
            (Some(SectionType::OddPage), Some(PageParity::Odd)),
        ] {
            section.section_type = start_type;
            assert_eq!(section_start_parity(&section), expected, "{start_type:?}");
        }
    }

    #[test]
    fn mirrored_margins_move_the_band_to_the_outside_edge_on_verso_pages() {
        let mut config = letter_config();
        // Inside 1440 (the gutter is already folded in upstream), outside 2880.
        config.margin_end = Twip(2_880);
        let tall = config.content_area().size.height.raw();
        let galley = vec![
            paragraph(1, Twip(tall)),
            paragraph(2, Twip(400)),
            paragraph(3, Twip(400)),
        ];
        let run = SectionRun {
            config,
            layout: ColumnLayout::single(config.content_area()),
            galley,
            column_galleys: Vec::new(),
            starts_new_page: true,
            start_parity: None,
            mirror_margins: true,
        };
        let layout = paginate_columns(&[run]);
        assert_eq!(layout.pages.len(), 2);
        assert_eq!(layout.pages[0].content_area.origin.x, config.margin_start);
        assert_eq!(layout.pages[0].placed[0].rect.origin.x, config.margin_start);
        assert_eq!(
            layout.pages[1].content_area.origin.x, config.margin_end,
            "the verso band starts at the outside margin"
        );
        assert_eq!(
            layout.pages[1].placed[0].rect.origin.x, config.margin_end,
            "placed content moves with the band"
        );
        assert_eq!(
            layout.pages[1].content_area.size.width,
            layout.pages[0].content_area.size.width
        );
    }

    #[test]
    fn mirrored_margins_move_the_column_separator_rules_too() {
        let mut config = letter_config();
        config.margin_end = Twip(2_880);
        let content = config.content_area();
        let cols = SectionColumns {
            count: 2,
            space_twips: Some(720),
            separator: Some(true),
            equal_width: None,
            columns: Vec::new(),
        };
        let tall = content.size.height.raw();
        let run = SectionRun {
            config,
            layout: column_layout(&cols, content),
            galley: vec![
                paragraph(1, Twip(tall)),
                paragraph(2, Twip(tall)),
                paragraph(3, Twip(400)),
            ],
            column_galleys: Vec::new(),
            starts_new_page: true,
            start_parity: None,
            mirror_margins: true,
        };
        let layout = paginate_columns(&[run]);
        assert_eq!(layout.pages.len(), 2);
        let shift = config.margin_end.raw() - config.margin_start.raw();
        assert_eq!(
            layout.pages[1].separators[0].x.raw(),
            layout.pages[0].separators[0].x.raw() + shift,
            "the inter-column rule mirrors with the columns it divides"
        );
    }
}
