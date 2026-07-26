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
//! - a `nextPage`/`evenPage`/`oddPage` (and the unspecified default) section starts
//!   a fresh page;
//! - a `continuous` (or `nextColumn`) section continues on the *same* page, its
//!   column band beginning just below the previous section's deepest content — the
//!   common, hard "column-set change mid-page" case the SDS exercises.
//!
//! ## Documented approximations (deferred fidelity)
//!
//! - **Unequal columns share one flow width.** A section's galley is flowed once, at
//!   a single width, then placed column by column. When the section declares unequal
//!   per-column widths (`w:cols/@w:equalWidth="0"` with explicit `w:col` widths),
//!   the columns are *placed* at their true widths and x-positions (so the layout
//!   shows the correct narrow/wide proportions and the separator sits in the real
//!   gap), but the galley is flowed at the **widest** column's width — the common
//!   "narrow label + wide content" case, where the wide column carries the bulk of
//!   the text and the narrow column holds short labels that do not wrap either way.
//!   Content in a narrow column that is wider than that column can overrun it; true
//!   per-column reflow (a distinct galley per column width) is deferred.
//! - **No last-page balancing.** Columns fill in order; the final page of a section
//!   is left unbalanced (Word balances the last page of a non-final column section).
//!   Because a following `continuous` section must begin below *all* of the previous
//!   section's columns to avoid overlap, an unbalanced last page can push the next
//!   section onto a new page where balancing would have kept it.
//! - **Column breaks collapse to page breaks** (as in single-column flow), and a
//!   table inside a multi-column section flows within the current column but does
//!   not repeat its header rows across columns.

use casual_doc_model::NodeId;
use casual_doc_model::v1::{SectionBoundary, SectionColumns};

use crate::block::{BlockFragment, BoxMetrics, BreakControl, CellFragment, ParagraphDecor};
use crate::page::{ColumnSeparator, FlowPos, FlowSpan, Page, PaginatedLayout, PlacedFragment};
use crate::paginate::{PageConfig, build_page, make_row_chunk, slice_paragraph, split_cells};
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

    /// The width the section's galley is flowed (line-broken) at. Equal columns all
    /// share one width; unequal columns are flowed at the **widest** column's width
    /// (see the module-level "unequal columns share one flow width" note), so the
    /// wide content column wraps at its true width and short labels in a narrow
    /// column are unaffected.
    #[must_use]
    pub fn flow_width(&self) -> Twip {
        self.columns
            .iter()
            .map(|c| c.width)
            .max_by_key(|w| w.raw())
            .unwrap_or(Twip::ZERO)
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
    /// `true` when the section starts on a fresh page (`nextPage`/`evenPage`/
    /// `oddPage`/default); `false` for `continuous`/`nextColumn`.
    pub starts_new_page: bool,
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

/// Paginates an ordered list of section runs into a single [`PaginatedLayout`],
/// carrying the page cursor across section boundaries (so a `continuous` section
/// shares the previous section's page). This is the multi-column counterpart to
/// [`paginate`](crate::paginate::paginate); the incremental/halt machinery is
/// deliberately not part of it (the driver uses it on full runs only).
#[must_use]
pub fn paginate_columns(runs: &[SectionRun]) -> PaginatedLayout {
    let mut p = ColPaginator::new();
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
    /// The current section's body content area.
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
}

impl ColPaginator {
    fn new() -> Self {
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

        self.config = run.config;
        self.content = run.config.content_area();
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
        let frags = &run.galley;
        let mut i = 0;
        while i < frags.len() {
            let mut j = i;
            while j < frags.len() && frags[j].break_control().keep_next {
                j += 1;
            }
            j = (j + 1).min(frags.len());
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
            for (offset, fragment) in group.iter().enumerate() {
                self.place(i + offset, fragment, allow_split);
            }
            i = j;
        }
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
    fn place(&mut self, idx: usize, fragment: &BlockFragment, allow_split: bool) {
        let gidx = self.base + idx as u32;
        self.at = FlowPos::at(gidx);
        match fragment {
            BlockFragment::Paragraph {
                id,
                lines,
                box_metrics,
                break_control,
                decor,
            } if lines.lines.iter().any(|l| l.page_break_after)
                || (lines.lines.len() > 1 && allow_split && !break_control.keep_lines) =>
            {
                self.place_paragraph(gidx, *id, lines, *box_metrics, *break_control, *decor);
            }
            BlockFragment::TableRow {
                table, can_split, ..
            } => {
                self.place_table_row(gidx, fragment, *table, *can_split);
            }
            _ => {
                let height = fragment.height();
                if !self.at_column_start() && height.raw() > self.remaining() {
                    self.advance();
                }
                self.push(fragment.clone(), height, self.column().width);
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
        let rect = Rect::new(Point::new(col.x, self.y), Size::new(width, height));
        self.placed.push(PlacedFragment { fragment, rect });
        self.y = self.y + height;
        self.page_max_y = Twip(self.page_max_y.raw().max(self.y.raw()));
    }

    /// Places a paragraph's lines, breaking across columns at line boundaries with
    /// widow/orphan control (mirrors `paginate::Paginator::place_paragraph`, with
    /// column advance instead of page flush and a forced break to a new page).
    fn place_paragraph(
        &mut self,
        gidx: u32,
        id: NodeId,
        lines: &LineLayout,
        box_metrics: BoxMetrics,
        break_control: BreakControl,
        decor: ParagraphDecor,
    ) {
        let n = lines.lines.len();
        let widow = break_control.widow_control;
        let width = self.column().width;
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
                let h = lines.lines[start + take].height.raw();
                if used + h > avail {
                    break;
                }
                used += h;
                take += 1;
            }

            if take == 0 {
                if self.at_column_start() {
                    take = 1;
                    used = lines.lines[start].height.raw();
                } else {
                    self.advance();
                    continue;
                }
            }

            // A forced page/column break caps this chunk at the break line.
            let forced = lines.lines[start..start + take]
                .iter()
                .position(|l| l.page_break_after);
            if let Some(k) = forced {
                let new_take = k + 1;
                used -= lines.lines[start + new_take..start + take]
                    .iter()
                    .map(|l| l.height.raw())
                    .sum::<i32>();
                take = new_take;
            }
            let forced = forced.is_some();

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
                used -= lines.lines[start + take].height.raw();
            }

            let is_tail = start + take == n;
            let space_after = if is_tail {
                box_metrics.space_after.raw()
            } else {
                0
            };
            let chunk = slice_paragraph(
                id,
                lines,
                box_metrics,
                break_control,
                decor,
                start..start + take,
            );
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
            if start < n {
                self.advance();
            } else if forced {
                self.new_page();
            }
        }
    }

    /// Places a table row: whole when it fits the current column; split across
    /// columns when it is splittable and taller than the remaining space; otherwise
    /// moved whole to the next column.
    fn place_table_row(
        &mut self,
        gidx: u32,
        fragment: &BlockFragment,
        table: NodeId,
        can_split: bool,
    ) {
        let _ = table;
        let height = fragment.height();
        let fits = height.raw() <= self.remaining();
        if !fits && can_split && !self.remaining_too_small() {
            self.split_table_row(gidx, fragment);
            return;
        }
        if !fits && !self.at_column_start() {
            self.advance();
        }
        self.push(fragment.clone(), height, self.column().width);
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
        for pair in self.columns.windows(2) {
            // The gap between the left column's trailing edge and the right column's
            // leading edge; the rule sits at its horizontal center.
            let gap_left = pair[0].x.raw() + pair[0].width.raw();
            let gap_right = pair[1].x.raw();
            let x = Twip((gap_left + gap_right) / 2);
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
        self.col = 0;
        self.band_top = self.content.origin.y;
        self.y = self.content.origin.y;
        self.page_max_y = self.content.origin.y;
        self.page_start = self.at;
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
            range: ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 0)),
            line_break: LineBreak::ParagraphEnd,
            page_break_after: false,
            bars: Vec::new(),
            images: Vec::new(),
            fields: Vec::new(),
            text_boxes: Vec::new(),
        };
        BlockFragment::Paragraph {
            id: node,
            lines: LineLayout { lines: vec![line] },
            box_metrics: BoxMetrics::default(),
            break_control: BreakControl::default(),
            decor: ParagraphDecor::default(),
        }
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
            starts_new_page: true,
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
            starts_new_page: true,
        };
        let second = SectionRun {
            config,
            layout: ColumnLayout::single(config.content_area()),
            galley: vec![paragraph(2, Twip(1_000))],
            starts_new_page: false,
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
            starts_new_page: true,
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
}
