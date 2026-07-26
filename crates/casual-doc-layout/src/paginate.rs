//! Pagination — slicing a galley of block fragments into fixed-size pages.
//!
//! The block/flow engine produces a galley (a `Vec<BlockFragment>` in flow
//! order, in twips); this paginator places them into [`Page`]s whose content
//! area comes from the section's page box and margins. Fragments are placed
//! atomically (a paragraph or row goes wholly on a page) with break control
//! (`P1D-002`): `w:pageBreakBefore` forces a page, and `w:keepNext` groups a
//! paragraph with the next block so a heading is never orphaned at a page foot.
//! Line-level splitting of a tall paragraph (+ widow/orphan), tables across
//! pages, and footnotes are the following slices (`P1D-002b/003`, `43-…` §7).
//!
//! Following the LayoutNG discipline (`42-…` §1.4) the output [`PaginatedLayout`]
//! is immutable; each [`Page`] records the model range it spans, which is the key
//! that makes incremental re-pagination (the stabilization halt) and per-page
//! incremental rendering possible (`43-…` §3.4).

use std::collections::HashMap;

use casual_doc_model::NodeId;
use casual_doc_model::v1::SectionId;

use crate::block::{BlockFragment, BoxMetrics, BreakControl, CellFragment, ParagraphDecor};
use crate::model::ModelPos;
use crate::page::{FlowPos, FlowSpan, Page, PaginatedLayout, PlacedFragment};
use crate::text::LineLayout;
use crate::units::{Point, Rect, Size, Twip};

/// The page geometry of a section: the page box and its margins.
#[derive(Clone, Copy, Debug)]
pub struct PageConfig {
    /// The section this geometry belongs to.
    pub section: SectionId,
    /// Full page size (twips).
    pub page_size: Size,
    /// Top margin.
    pub margin_top: Twip,
    /// Bottom margin.
    pub margin_bottom: Twip,
    /// Leading (start) margin.
    pub margin_start: Twip,
    /// Trailing (end) margin.
    pub margin_end: Twip,
}

impl PageConfig {
    /// The content area (page box minus margins) — where flow content is placed.
    #[must_use]
    pub fn content_area(&self) -> Rect {
        let width = self.page_size.width - self.margin_start - self.margin_end;
        let height = self.page_size.height - self.margin_top - self.margin_bottom;
        Rect::new(
            Point::new(self.margin_start, self.margin_top),
            Size::new(width.max(Twip::ZERO), height.max(Twip::ZERO)),
        )
    }
}

/// The minimum lines kept together at a page break (Word's default widow/orphan
/// count) when a paragraph carries `w:widowControl`.
const MIN_WIDOW_ORPHAN: usize = 2;

/// Paginates a galley of block fragments into pages under one section geometry.
///
/// A paragraph that does not fit the remaining space is **split at a line
/// boundary** — the lines that fit stay, the rest carry to the next page — unless
/// it is `keep_lines` (moved whole) or part of a `keep_next` group (kept
/// together). Forced breaks (`pageBreakBefore`) and widow/orphan control are
/// honored (`docs/42-…` §2.5, `43-…` §7). A block taller than a whole page
/// overflows rather than looping, so pagination always terminates.
#[must_use]
pub fn paginate(fragments: &[BlockFragment], config: &PageConfig) -> PaginatedLayout {
    let mut p = Paginator::new(config, Vec::new(), FlowPos::at(0), None);
    p.run(fragments, 0);
    p.flush();
    PaginatedLayout { pages: p.pages }
}

/// Re-paginates `new_galley` given the previous layout and the galley it came
/// from, doing work bounded to the edit neighborhood — the guarantee is that the
/// result is **field-for-field identical to a full [`paginate`] of the new
/// galley** (verified by the `incremental_equals_full_*` golden tests).
///
/// The pages that lie entirely above the first changed fragment are reused
/// verbatim (their layout cannot depend on anything below them, since pagination
/// is a forward fill and each page begins at a fresh content-top cursor). Only
/// the changed page and everything after it are re-flowed. This makes editing
/// near the end of a document nearly free.
///
/// **Above** the edit, pages are reused verbatim; **below** it, the *stabilization
/// halt* reuses the unchanged tail: once a re-flowed page boundary re-lands on a
/// previous boundary within the common galley suffix, the rest of the previous
/// layout tiles identically, so it is spliced in (renumbered, indices shifted by
/// any insert/delete delta) instead of being re-flowed to EOF. Work is therefore
/// bounded to the pages the edit actually disturbs — even for an edit near the
/// top of a long document.
///
/// `prev` must be the layout `paginate(prev_galley, config)` produced under the
/// same `config`; if it is not (or the geometry changed) this still returns a
/// correct layout — it simply reuses nothing.
#[must_use]
pub fn repaginate(
    prev: &PaginatedLayout,
    prev_galley: &[BlockFragment],
    new_galley: &[BlockFragment],
    config: &PageConfig,
) -> PaginatedLayout {
    repaginate_with_stats(prev, prev_galley, new_galley, config).0
}

/// Cost accounting for one incremental re-pagination: how many previous pages
/// were reused above the edit (`reused_prefix`), how many were re-flowed
/// (`reflowed` — the actual work done), and how many were spliced from the
/// unchanged tail (`reused_tail`). The stabilization guarantee is that `reflowed`
/// stays proportional to the pages the edit disturbs, not to document length.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RepaginateStats {
    /// Pages reused verbatim from above the edit.
    pub reused_prefix: usize,
    /// Pages re-flowed (the work the edit actually cost).
    pub reflowed: usize,
    /// Pages spliced from the unchanged tail by the stabilization halt.
    pub reused_tail: usize,
}

/// [`repaginate`] plus its [`RepaginateStats`] — the incremental cost, for
/// telemetry and for asserting that work stays bounded to the edit neighborhood.
#[must_use]
pub fn repaginate_with_stats(
    prev: &PaginatedLayout,
    prev_galley: &[BlockFragment],
    new_galley: &[BlockFragment],
    config: &PageConfig,
) -> (PaginatedLayout, RepaginateStats) {
    // The first galley index whose fragment changed. Everything before it is
    // byte-identical in both galleys, so its pagination is unaffected.
    let first_dirty = new_galley
        .iter()
        .zip(prev_galley.iter())
        .position(|(a, b)| a != b)
        .unwrap_or(new_galley.len().min(prev_galley.len()));

    // Choose a safe resume point: a page that begins at a fragment boundary
    // (`line == 0`) that is also a keep-group start, at or before `first_dirty`.
    // Re-running the group walk from a group start with a fresh page-top cursor
    // reproduces exactly what a full paginate does from there.
    let Some(resume_page) = safe_resume_page(prev, prev_galley, first_dirty, config) else {
        let layout = paginate(new_galley, config);
        let reflowed = layout.pages.len();
        return (
            layout,
            RepaginateStats {
                reused_prefix: 0,
                reflowed,
                reused_tail: 0,
            },
        );
    };
    let resume_index = prev.pages[resume_page].flow.start.fragment as usize;

    // The unchanged tail the halt may splice, keyed by index-from-end.
    let suffix = common_suffix_len(prev_galley, new_galley);
    let halt = HaltLookup::build(
        prev,
        prev_galley.len() as u32,
        new_galley.len() as u32,
        suffix,
        resume_page,
    );

    let prefix: Vec<Page> = prev.pages[..resume_page].to_vec();
    let mut p = Paginator::new(config, prefix, FlowPos::at(resume_index as u32), halt);
    p.run(new_galley, resume_index);
    p.flush();

    let reflowed = p.pages.len() - resume_page;

    // If the walk stabilized, splice the previous layout's tail: its pages are
    // identical block-for-block, needing only sequential renumbering and a shift
    // of the galley-relative flow indices by the insert/delete delta.
    let mut reused_tail = 0;
    if let Some(from) = p.halted {
        let delta = new_galley.len() as i64 - prev_galley.len() as i64;
        for page in &prev.pages[from..] {
            let mut page = page.clone();
            page.number = p.pages.len() as u32 + 1;
            page.flow.start.fragment = (i64::from(page.flow.start.fragment) + delta) as u32;
            page.flow.end.fragment = (i64::from(page.flow.end.fragment) + delta) as u32;
            p.pages.push(page);
        }
        reused_tail = prev.pages.len() - from;
    }
    let stats = RepaginateStats {
        reused_prefix: resume_page,
        reflowed,
        reused_tail,
    };
    (PaginatedLayout { pages: p.pages }, stats)
}

/// The index of the previous page from which re-pagination resumes for an edit
/// at `first_dirty`. Pages before it are reused verbatim; it and everything after
/// are re-flowed. Returns `None` when nothing is reusable (fall back to a full
/// paginate) — e.g. the page geometry changed.
///
/// Two subtleties make this correct (both from the stabilization-halt analysis):
///
/// - **Keep-groups pull content upward.** A `keep_next` group whose size shrinks
///   can migrate up into the page above it, so a page that merely *ends before*
///   the edit is not automatically safe. We first extend the dirty point up over
///   any keep-with-next chain containing the edit (`dirty`), then reuse only pages
///   that end *strictly* above `dirty`. A page ending exactly at `dirty` is the
///   page the shrunken group could move into, so it is re-flowed. This is why a
///   heading (which defaults to `keep_next`) directly above the edited paragraph
///   is handled correctly.
/// - **Resume at a clean boundary.** The resume page must begin at a fragment
///   boundary (`line == 0`) that is a keep-group start, or re-running the group
///   walk from a fresh page-top cursor would not reproduce the split. If it does
///   not, we back up to an earlier page (still all unchanged content, so reusing
///   less is always safe).
fn safe_resume_page(
    prev: &PaginatedLayout,
    prev_galley: &[BlockFragment],
    first_dirty: usize,
    config: &PageConfig,
) -> Option<usize> {
    let pages = &prev.pages;
    let first = pages.first()?;
    // Geometry must match, or reused pages would carry stale content areas.
    if first.content_area != config.content_area() || first.section != config.section {
        return None;
    }

    // Extend the dirty point up over the keep-with-next chain containing the edit,
    // so we never reuse a page whose bottom keep-group's size changed.
    let mut dirty = first_dirty;
    while dirty > 0 && prev_galley[dirty - 1].break_control().keep_next {
        dirty -= 1;
    }

    // The first page whose content reaches `dirty` (ends at or after it): the
    // earliest page that might change. Reuse everything strictly above it.
    let dirty = dirty as u32;
    let mut resume = pages.partition_point(|p| p.flow.end.fragment < dirty);
    resume = resume.min(pages.len() - 1);

    // Back up to a page that begins at a clean, re-derivable resume boundary.
    while resume > 0 && !is_clean_boundary(pages[resume].flow.start, prev_galley) {
        resume -= 1;
    }
    Some(resume)
}

/// Whether re-pagination can resume at `pos`: it is a fragment boundary (not a
/// split-paragraph continuation) that starts a keep-group (the fragment above it
/// does not keep-with-next).
fn is_clean_boundary(pos: FlowPos, galley: &[BlockFragment]) -> bool {
    let f = pos.fragment as usize;
    pos.line == 0 && (f == 0 || f > galley.len() || !galley[f - 1].break_control().keep_next)
}

/// The number of trailing fragments that are identical in both galleys (the
/// common suffix). Content from here to EOF is unchanged, so a page boundary
/// that re-lands inside it tiles the rest of the document exactly as before.
fn common_suffix_len(prev: &[BlockFragment], new: &[BlockFragment]) -> u32 {
    let mut n = 0u32;
    let (mut i, mut j) = (prev.len(), new.len());
    while i > 0 && j > 0 && prev[i - 1] == new[j - 1] {
        i -= 1;
        j -= 1;
        n += 1;
    }
    n
}

/// Precomputed lookup that lets the paginator **halt** as soon as a re-flowed
/// page boundary re-lands on a previous page's boundary within the unchanged
/// tail — the *stabilization halt* (`docs/43-…` §3.4).
///
/// The key is `(index_from_end, line)`. Two paginations that reach the same flow
/// position — the same distance from the end of the galley and the same
/// intra-paragraph line offset — over content that is identical from there to
/// EOF lay out identically from that point on (the forward pass is a pure
/// function of its start position, geometry, and downstream content). So a single
/// match justifies splicing the entire previous tail rather than re-flowing it.
/// Keying by index-from-end (not absolute index) makes this robust to fragments
/// inserted or removed above — pressing Enter/Backspace shifts absolute indices
/// but not distance-from-the-end.
struct HaltLookup {
    new_len: u32,
    /// Length of the common galley suffix (the largest reusable index-from-end).
    suffix: u32,
    /// `(index_from_end, start_line)` → previous page index (only pages that both
    /// begin inside the unchanged suffix and lie at or after the resume point).
    starts: HashMap<(u32, u32), usize>,
}

impl HaltLookup {
    /// Builds the lookup, or `None` if there is no reusable suffix. `resume` is
    /// the first re-flowed page index, so pages already reused as the prefix are
    /// never spliced (no overlap).
    fn build(
        prev: &PaginatedLayout,
        prev_len: u32,
        new_len: u32,
        suffix: u32,
        resume: usize,
    ) -> Option<Self> {
        if suffix == 0 {
            return None;
        }
        let mut starts = HashMap::new();
        for (idx, page) in prev.pages.iter().enumerate().skip(resume) {
            let ife = prev_len - page.flow.start.fragment;
            if (1..=suffix).contains(&ife) {
                starts.insert((ife, page.flow.start.line), idx);
            }
        }
        Some(Self {
            new_len,
            suffix,
            starts,
        })
    }

    /// The previous page index whose tail can be spliced when a re-flowed page
    /// begins at flow position `at`, or `None` if `at` is not a stabilization
    /// point.
    fn match_at(&self, at: FlowPos) -> Option<usize> {
        let ife = self.new_len.checked_sub(at.fragment)?;
        if !(1..=self.suffix).contains(&ife) {
            return None;
        }
        self.starts.get(&(ife, at.line)).copied()
    }
}

/// Mutable pagination state, walked fragment by fragment.
struct Paginator<'a> {
    config: &'a PageConfig,
    content: Rect,
    content_bottom: i32,
    content_height: i32,
    pages: Vec<Page>,
    placed: Vec<PlacedFragment>,
    cursor_y: Twip,
    /// Flow position of the next content to be placed.
    at: FlowPos,
    /// Flow position where the current (open) page began.
    page_start: FlowPos,
    /// When set (incremental runs), lets the walk halt at a stabilization point.
    halt: Option<HaltLookup>,
    /// Once the walk halts, the previous page index whose tail is to be spliced.
    halted: Option<usize>,
    /// The table whose rows are currently being placed (so a table's header rows
    /// can be repeated when it continues onto a new page).
    current_table: Option<NodeId>,
    /// The current table's header rows (`w:tblHeader`), repeated at the top of
    /// each continuation page.
    table_headers: Vec<BlockFragment>,
}

impl<'a> Paginator<'a> {
    /// Creates a paginator seeded with already-emitted `pages` and resuming at
    /// flow position `at` (page-top cursor). For a full run pass an empty prefix,
    /// `FlowPos::at(0)`, and no halt lookup.
    fn new(
        config: &'a PageConfig,
        pages: Vec<Page>,
        at: FlowPos,
        halt: Option<HaltLookup>,
    ) -> Self {
        let content = config.content_area();
        Self {
            config,
            content,
            content_bottom: content.bottom().raw(),
            content_height: content.size.height.raw(),
            pages,
            placed: Vec::new(),
            cursor_y: content.origin.y,
            at,
            page_start: at,
            halt,
            halted: None,
            current_table: None,
            table_headers: Vec::new(),
        }
    }

    /// Walks keep-with-next groups of `fragments[from..]`, placing them into
    /// pages. A keep-together group (a keep-next chain, or a single `keep_lines`
    /// paragraph) is moved whole when it fits a page but not the remaining space;
    /// a normal paragraph splits to fill the current page.
    fn run(&mut self, fragments: &[BlockFragment], from: usize) {
        let mut i = from;
        while i < fragments.len() && self.halted.is_none() {
            self.at = FlowPos::at(i as u32);
            let mut j = i;
            while j < fragments.len() && fragments[j].break_control().keep_next {
                j += 1;
            }
            j = (j + 1).min(fragments.len());
            let group = &fragments[i..j];
            let group_height: i32 = group.iter().map(|f| f.height().raw()).sum();
            let group_fits_page = group_height <= self.content_height;
            let is_keep_group = group.len() > 1 || group[0].break_control().keep_lines;

            let forced = group[0].break_control().page_break_before;
            let doesnt_fit_here = self.cursor_y.raw() + group_height > self.content_bottom;
            if !self.placed.is_empty()
                && (forced || (is_keep_group && group_fits_page && doesnt_fit_here))
            {
                self.flush();
            }
            // The group's start is a page top: it may be a stabilization point.
            if self.halted.is_some() {
                break;
            }

            // A keep-together group that fits a page is placed atomically;
            // otherwise (a normal paragraph, or an oversized keep group) paragraphs
            // may split.
            let allow_split = !is_keep_group || !group_fits_page;
            for (offset, fragment) in group.iter().enumerate() {
                self.place(i + offset, fragment, allow_split);
                if self.halted.is_some() {
                    return;
                }
            }
            i = j;
        }
    }

    /// Ends the current page (if it has content) and resets the cursor to the top.
    fn flush(&mut self) {
        if !self.placed.is_empty() {
            let flow = FlowSpan {
                start: self.page_start,
                end: self.at,
            };
            let page = build_page(
                self.pages.len(),
                self.config,
                self.content,
                std::mem::take(&mut self.placed),
                flow,
            );
            self.pages.push(page);
            self.cursor_y = self.content.origin.y;
            self.page_start = self.at;
            // The page just closed; `self.at` is the next page's start. If it
            // re-lands on a previous boundary inside the unchanged tail, stop —
            // the caller splices the rest of the previous layout verbatim.
            if self.halted.is_none()
                && let Some(halt) = &self.halt
            {
                self.halted = halt.match_at(self.at);
            }
        }
    }

    /// Remaining content height below the cursor.
    fn remaining(&self) -> i32 {
        self.content_bottom - self.cursor_y.raw()
    }

    /// Appends a placed fragment at the cursor and advances by `height`. Records
    /// the page's start flow position when this is the page's first content.
    fn push(&mut self, fragment: BlockFragment, height: Twip) {
        if self.placed.is_empty() {
            self.page_start = self.at;
        }
        let rect = Rect::new(
            Point::new(self.content.origin.x, self.cursor_y),
            Size::new(self.content.size.width, height),
        );
        self.placed.push(PlacedFragment { fragment, rect });
        self.cursor_y = self.cursor_y + height;
    }

    /// Places fragment `idx`, splitting a multi-line paragraph across pages when
    /// `allow_split` and it does not fit whole.
    fn place(&mut self, idx: usize, fragment: &BlockFragment, allow_split: bool) {
        self.at = FlowPos::at(idx as u32);
        // Leaving a run of table rows ends the current table's header context.
        if !matches!(fragment, BlockFragment::TableRow { .. }) {
            self.leave_table();
        }
        match fragment {
            BlockFragment::Paragraph {
                id,
                lines,
                box_metrics,
                break_control,
                decor,
            } if lines.lines.len() > 1
                && (allow_split && !break_control.keep_lines
                    || lines.lines.iter().any(|l| l.page_break_after)) =>
            {
                // A paragraph with a forced mid-paragraph page/column break is
                // always routed through the line splitter (even under `keepLines`
                // or a keep-group) so the explicit break is honored.
                self.place_paragraph(idx, *id, lines, *box_metrics, *break_control, *decor);
            }
            BlockFragment::TableRow {
                table,
                can_split,
                header,
                ..
            } => {
                self.place_table_row(idx, fragment, *table, *can_split, *header);
            }
            _ => {
                let height = fragment.height();
                if !self.placed.is_empty() && height.raw() > self.remaining() {
                    self.flush();
                    // The flush may have landed on a stabilization point; if so,
                    // this fragment belongs to the spliced tail — do not place it.
                    if self.halted.is_some() {
                        return;
                    }
                }
                self.push(fragment.clone(), height);
                self.at = FlowPos::at(idx as u32 + 1);
            }
        }
    }

    /// Places a table row (`P1D-003`): it stays whole when it fits; a `cantSplit`
    /// row (and any header row) that does not fit moves whole to the next page;
    /// otherwise the row is split across the page boundary. Whenever a table's
    /// body continues onto a fresh page, its header rows are repeated on top.
    fn place_table_row(
        &mut self,
        idx: usize,
        fragment: &BlockFragment,
        table: NodeId,
        can_split: bool,
        header: bool,
    ) {
        self.enter_table(table);
        self.at = FlowPos::at(idx as u32);
        // Header rows are never split — they move whole and repeat.
        let can_split = can_split && !header;
        let height = fragment.height();
        let fits = height.raw() <= self.remaining();

        // A splittable row that does not fit is broken across the boundary —
        // whether it is the page's first content (taller than a whole page) or
        // follows other content.
        if !fits && can_split {
            self.split_table_row(idx, fragment);
            self.capture_header(fragment, header);
            return;
        }
        // A `cantSplit`/header row that does not fit moves whole to the next page
        // (unless the page is already empty, when it overflows in place).
        if !fits && !self.placed.is_empty() {
            self.flush();
            if self.halted.is_some() {
                return;
            }
        }
        self.repeat_headers_if_needed(idx, header);
        self.push(fragment.clone(), height);
        self.at = FlowPos::at(idx as u32 + 1);
        self.capture_header(fragment, header);
    }

    /// Splits a table row across a page boundary at block/line boundaries within
    /// its cells, mirroring the paragraph line-splitting path. Each chunk is a
    /// row fragment carrying the cells' content that fits; the remainder carries
    /// to the next page (with header rows repeated).
    fn split_table_row(&mut self, idx: usize, fragment: &BlockFragment) {
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
        let mut remaining: Vec<CellFragment> = cells.clone();
        let mut chunk = 0u32;
        loop {
            self.at = FlowPos {
                fragment: idx as u32,
                line: chunk,
            };
            let (head, tail, used) = split_cells(&remaining, self.remaining());
            if used == 0 {
                // Nothing fits here. On an empty page place the remainder whole
                // as overflow (so we never loop); otherwise start a fresh page.
                if self.placed.is_empty() {
                    let h = BlockFragment::cells_content_height(&remaining);
                    let row = make_row_chunk(*id, *table, remaining, h, *can_split, is_header);
                    self.push(row, h);
                    self.at = FlowPos::at(idx as u32 + 1);
                    return;
                }
                self.flush();
                if self.halted.is_some() {
                    return;
                }
                self.repeat_headers_if_needed(idx, is_header);
                continue;
            }
            let row = make_row_chunk(*id, *table, head, Twip(used), *can_split, is_header);
            self.push(row, Twip(used));
            if tail.is_empty() {
                self.at = FlowPos::at(idx as u32 + 1);
                return;
            }
            remaining = tail;
            chunk += 1;
            self.flush();
            if self.halted.is_some() {
                return;
            }
            self.repeat_headers_if_needed(idx, is_header);
        }
    }

    /// Begins (or continues) placing a table's rows; clears the header context
    /// when a new table starts.
    fn enter_table(&mut self, table: NodeId) {
        if self.current_table != Some(table) {
            self.current_table = Some(table);
            self.table_headers.clear();
        }
    }

    /// Ends the current table's header context (called when a non-row fragment
    /// interrupts the run of rows).
    fn leave_table(&mut self) {
        self.current_table = None;
        self.table_headers.clear();
    }

    /// Records a header row so it can be repeated on continuation pages.
    fn capture_header(&mut self, fragment: &BlockFragment, header: bool) {
        if header {
            self.table_headers.push(fragment.clone());
        }
    }

    /// When a table's body row lands at the top of a fresh page, repeats the
    /// table's header rows above it (Word's `w:tblHeader` behavior). The repeated
    /// headers are extra placed fragments and do not advance the flow position —
    /// the page's flow provenance stays anchored to the real body row `idx`.
    fn repeat_headers_if_needed(&mut self, idx: usize, header: bool) {
        if header || self.table_headers.is_empty() || !self.placed.is_empty() {
            return;
        }
        self.page_start = FlowPos::at(idx as u32);
        for h in self.table_headers.clone() {
            let height = h.height();
            let rect = Rect::new(
                Point::new(self.content.origin.x, self.cursor_y),
                Size::new(self.content.size.width, height),
            );
            self.placed.push(PlacedFragment { fragment: h, rect });
            self.cursor_y = self.cursor_y + height;
        }
    }

    /// Places paragraph `idx`'s lines, breaking across pages at line boundaries
    /// with widow/orphan control.
    fn place_paragraph(
        &mut self,
        idx: usize,
        id: NodeId,
        lines: &LineLayout,
        box_metrics: BoxMetrics,
        break_control: BreakControl,
        decor: ParagraphDecor,
    ) {
        let n = lines.lines.len();
        let widow = break_control.widow_control;
        let mut start = 0;
        let mut is_head = true;
        while start < n && self.halted.is_none() {
            // The next content to place is this paragraph's line `start`; a flush
            // triggered below (nothing fits / orphan) must end the page here.
            self.at = FlowPos {
                fragment: idx as u32,
                line: start as u32,
            };
            let space_before = if is_head {
                box_metrics.space_before.raw()
            } else {
                0
            };
            let avail = self.remaining() - space_before;

            // Greedily count leading lines that fit.
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
                // Nothing fits here. Move to a fresh page unless we are already on
                // one (then place a single line as overflow so we never loop).
                if self.placed.is_empty() {
                    take = 1;
                    used = lines.lines[start].height.raw();
                } else {
                    self.flush();
                    continue;
                }
            }

            // A forced page/column break (`w:br` type page/column) caps this chunk:
            // include up to and through the break line, then flush unconditionally.
            // This is just another line-split point, so the incremental halt/prefix
            // bookkeeping (keyed on `{fragment, line}`) is unchanged.
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

            // Orphan: don't strand fewer than the minimum head lines at a page
            // foot — move the whole paragraph to the next page (only when the page
            // already has content, else we would loop). A forced break defines the
            // split point itself, so widow/orphan reshuffling is skipped for it.
            if !forced
                && widow
                && is_head
                && !self.placed.is_empty()
                && start + take < n
                && take < MIN_WIDOW_ORPHAN
                && n >= MIN_WIDOW_ORPHAN
            {
                self.flush();
                continue;
            }
            // Widow: don't leave fewer than the minimum lines for the tail — pull a
            // line down (keeping at least one on this page).
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
            self.push(chunk, Twip(used + space_before + space_after));
            start += take;
            is_head = false;
            // Advance the flow cursor past the placed chunk.
            self.at = if is_tail {
                FlowPos::at(idx as u32 + 1)
            } else {
                FlowPos {
                    fragment: idx as u32,
                    line: start as u32,
                }
            };
            if start < n {
                self.flush();
            }
        }
    }
}

/// Builds a paragraph fragment for `lines[range]`, re-basing each line's run
/// origins so the first placed line sits at the fragment top, and keeping
/// `space_before`/`space_after` only on the head/tail chunk. Whether this slice is
/// the paragraph's head (starts at line 0) and/or tail (ends at the last line) is
/// derived from `range` — a slice covering the whole paragraph is both.
fn slice_paragraph(
    id: NodeId,
    lines: &LineLayout,
    box_metrics: BoxMetrics,
    break_control: BreakControl,
    decor: ParagraphDecor,
    range: core::ops::Range<usize>,
) -> BlockFragment {
    let is_head = range.start == 0;
    let is_tail = range.end == lines.lines.len();
    let y_offset: i32 = lines.lines[..range.start]
        .iter()
        .map(|l| l.height.raw())
        .sum();
    let sliced: Vec<_> = lines.lines[range]
        .iter()
        .map(|line| {
            let mut line = line.clone();
            for run in &mut line.runs {
                run.origin.y = Twip(run.origin.y.raw() - y_offset);
            }
            line
        })
        .collect();
    BlockFragment::Paragraph {
        id,
        lines: LineLayout { lines: sliced },
        box_metrics: BoxMetrics {
            space_before: if is_head {
                box_metrics.space_before
            } else {
                Twip::ZERO
            },
            space_after: if is_tail {
                box_metrics.space_after
            } else {
                Twip::ZERO
            },
            ..box_metrics
        },
        break_control,
        decor,
    }
}

/// Builds a table-row chunk fragment (a continuation piece of a split row). The
/// piece carries the resolved height it occupies; splitting never clips (only a
/// whole `exact`-height row does).
fn make_row_chunk(
    id: NodeId,
    table: NodeId,
    cells: Vec<CellFragment>,
    height: Twip,
    can_split: bool,
    header: bool,
) -> BlockFragment {
    BlockFragment::TableRow {
        id,
        table,
        cells,
        height,
        can_split,
        header,
        clip: false,
    }
}

/// Splits a row's cells at a vertical cut `avail` twips below the row top,
/// returning the head cells (content that fits), the tail cells (the remainder,
/// preserving every column so the continuation row keeps its geometry), and the
/// head height actually used (the tallest cell's fitted content). A `used` of 0
/// means nothing fit.
fn split_cells(cells: &[CellFragment], avail: i32) -> (Vec<CellFragment>, Vec<CellFragment>, i32) {
    let mut head = Vec::with_capacity(cells.len());
    let mut tail = Vec::with_capacity(cells.len());
    let mut used = 0;
    let mut has_tail = false;
    for cell in cells {
        let (head_blocks, tail_blocks, cell_used) = split_blocks(&cell.blocks, avail);
        used = used.max(cell_used);
        head.push(CellFragment {
            blocks: head_blocks,
            ..cell.clone()
        });
        if !tail_blocks.is_empty() {
            has_tail = true;
        }
        tail.push(CellFragment {
            blocks: tail_blocks,
            ..cell.clone()
        });
    }
    if !has_tail {
        tail.clear();
    }
    (head, tail, used)
}

/// Splits a cell's stacked block fragments at `avail` twips: blocks fully above
/// the cut go to the head, the straddling block is split (a multi-line paragraph
/// at a line boundary; anything else moves whole to the tail), and the rest go to
/// the tail. Returns `(head, tail, used_height)`.
fn split_blocks(
    blocks: &[BlockFragment],
    avail: i32,
) -> (Vec<BlockFragment>, Vec<BlockFragment>, i32) {
    let mut head = Vec::new();
    let mut tail = Vec::new();
    let mut y = 0;
    let mut splitting = true;
    for block in blocks {
        if !splitting {
            tail.push(block.clone());
            continue;
        }
        let height = block.height().raw();
        if y + height <= avail {
            head.push(block.clone());
            y += height;
            continue;
        }
        match block {
            BlockFragment::Paragraph {
                id,
                lines,
                box_metrics,
                break_control,
                decor,
            } if lines.lines.len() > 1 && !break_control.keep_lines => {
                let (head_frag, tail_frag, used) =
                    split_paragraph_at(*id, lines, *box_metrics, *break_control, *decor, avail - y);
                if let Some(head_frag) = head_frag {
                    head.push(head_frag);
                    y += used;
                }
                if let Some(tail_frag) = tail_frag {
                    tail.push(tail_frag);
                }
            }
            _ => tail.push(block.clone()),
        }
        splitting = false;
    }
    (head, tail, y)
}

/// Splits one paragraph at a `avail`-twip vertical cut, greedily keeping the
/// leading lines that fit. Returns the head chunk (if any line fits), the tail
/// chunk (the rest), and the head height used.
fn split_paragraph_at(
    id: NodeId,
    lines: &LineLayout,
    box_metrics: BoxMetrics,
    break_control: BreakControl,
    decor: ParagraphDecor,
    avail: i32,
) -> (Option<BlockFragment>, Option<BlockFragment>, i32) {
    let n = lines.lines.len();
    let space_before = box_metrics.space_before.raw();
    let budget = avail - space_before;
    let mut take = 0;
    let mut used = 0;
    while take < n {
        let line_h = lines.lines[take].height.raw();
        if used + line_h > budget {
            break;
        }
        used += line_h;
        take += 1;
    }
    if take == 0 {
        let whole = slice_paragraph(id, lines, box_metrics, break_control, decor, 0..n);
        return (None, Some(whole), 0);
    }
    let head = slice_paragraph(id, lines, box_metrics, break_control, decor, 0..take);
    let tail = slice_paragraph(id, lines, box_metrics, break_control, decor, take..n);
    (Some(head), Some(tail), space_before + used)
}

/// Assembles one page from the fragments placed on it.
fn build_page(
    index: usize,
    config: &PageConfig,
    content: Rect,
    placed: Vec<PlacedFragment>,
    flow: FlowSpan,
) -> Page {
    let start = ModelPos::new(placed.first().unwrap().fragment.node_id(), 0);
    let end = ModelPos::new(placed.last().unwrap().fragment.node_id(), 0);
    Page {
        number: (index + 1) as u32,
        section: config.section,
        content_area: content,
        placed,
        footnotes: Vec::new(),
        start,
        end,
        flow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{BoxMetrics, BreakControl};
    use crate::model::{ModelPos, ModelRange};
    use crate::text::{Line, LineBreak, LineLayout};
    use casual_doc_model::NodeId;

    /// A US-Letter page (12240×15840 twips) with 1-inch (1440) margins → an
    /// 1152×12960-twip content area.
    fn letter_config() -> PageConfig {
        PageConfig {
            section: SectionId::new(NodeId::from_parts(9, 1).unwrap()),
            page_size: Size::new(Twip(12_240), Twip(15_840)),
            margin_top: Twip(1_440),
            margin_bottom: Twip(1_440),
            margin_start: Twip(1_440),
            margin_end: Twip(1_440),
        }
    }

    /// A paragraph fragment of a given height (one line tall).
    fn paragraph(id: u64, height: Twip) -> BlockFragment {
        paragraph_with(id, height, BreakControl::default())
    }

    /// A paragraph fragment with explicit break control (for break tests).
    fn paragraph_with(id: u64, height: Twip, break_control: BreakControl) -> BlockFragment {
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
        };
        BlockFragment::Paragraph {
            id: node,
            lines: LineLayout { lines: vec![line] },
            box_metrics: BoxMetrics::default(),
            break_control,
            decor: ParagraphDecor::default(),
        }
    }

    /// A multi-line paragraph: `count` lines each `line_h` tall, with a single
    /// glyph run per line at baseline `y` so re-origining is observable.
    fn multiline(
        id: u64,
        count: usize,
        line_h: Twip,
        break_control: BreakControl,
    ) -> BlockFragment {
        use crate::text::{FontId, Glyph, GlyphRun};
        let node = NodeId::from_parts(id, 1).unwrap();
        let lines = (0..count)
            .map(|i| {
                let baseline = Twip(line_h.raw() * (i as i32 + 1));
                Line {
                    runs: vec![GlyphRun {
                        font: FontId(0),
                        size: line_h,
                        color: [0, 0, 0, 255],
                        origin: Point::new(Twip::ZERO, baseline),
                        bidi_level: 0,
                        decoration: crate::text::Decoration::default(),
                        highlight: None,
                        glyphs: vec![Glyph {
                            id: 1,
                            advance: line_h,
                            cluster: 0,
                        }],
                    }],
                    ascent: line_h,
                    descent: Twip::ZERO,
                    height: line_h,
                    range: ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 0)),
                    line_break: LineBreak::Wrap,
                    page_break_after: false,
                    bars: Vec::new(),
                }
            })
            .collect();
        BlockFragment::Paragraph {
            id: node,
            lines: LineLayout { lines },
            box_metrics: BoxMetrics::default(),
            break_control,
            decor: ParagraphDecor::default(),
        }
    }

    /// A multi-line paragraph like [`multiline`] but with a forced page break
    /// (`w:br` type page) after line index `break_after` — modeling a
    /// mid-paragraph page break.
    fn multiline_forced(id: u64, count: usize, line_h: Twip, break_after: usize) -> BlockFragment {
        let mut fragment = multiline(id, count, line_h, BreakControl::default());
        if let BlockFragment::Paragraph { lines, .. } = &mut fragment
            && let Some(line) = lines.lines.get_mut(break_after)
        {
            line.page_break_after = true;
            line.line_break = LineBreak::Hard;
        }
        fragment
    }

    #[test]
    fn a_forced_page_break_splits_the_paragraph_mid_paragraph() {
        let config = letter_config();
        // Four 240-twip lines (960 twips) fit a page easily, but a forced page
        // break after line 1 must still split the paragraph onto a second page.
        let fragments = vec![multiline_forced(1, 4, Twip(240), 1)];
        let layout = paginate(&fragments, &config);
        assert_eq!(
            layout.page_count(),
            2,
            "the forced break starts a second page"
        );
        let head = &layout.pages[0].placed[0].fragment;
        let tail = &layout.pages[1].placed[0].fragment;
        let lines_of = |f: &BlockFragment| match f {
            BlockFragment::Paragraph { lines, .. } => lines.lines.len(),
            BlockFragment::TableRow { .. } => 0,
        };
        assert_eq!(
            lines_of(head),
            2,
            "the head holds the lines up to the break"
        );
        assert_eq!(lines_of(tail), 2, "the remainder continues on page 2");
    }

    #[test]
    fn incremental_golden_holds_with_a_forced_break_paragraph() {
        let config = letter_config();
        // A galley containing a mid-paragraph page break; edit a later paragraph
        // and assert `repaginate == paginate` still holds field-for-field.
        let prev = vec![
            multiline_forced(1, 4, Twip(240), 1),
            paragraph(2, Twip(240)),
            paragraph(3, Twip(240)),
        ];
        let new = vec![
            multiline_forced(1, 4, Twip(240), 1),
            paragraph(2, Twip(600)),
            paragraph(3, Twip(240)),
        ];
        golden(&prev, &new, &config);
    }

    #[test]
    fn short_content_fits_on_one_page() {
        let config = letter_config();
        let fragments = vec![paragraph(1, Twip(240)), paragraph(2, Twip(240))];
        let layout = paginate(&fragments, &config);
        assert_eq!(layout.page_count(), 1);
        assert_eq!(layout.pages[0].placed.len(), 2);
        assert_eq!(layout.pages[0].number, 1);
    }

    #[test]
    fn overflowing_content_breaks_to_multiple_pages() {
        let config = letter_config();
        // Content area is 12_960 twips tall; 60 paragraphs of 300 twips = 18_000
        // twips → must span more than one page.
        let fragments: Vec<_> = (0..60).map(|i| paragraph(i + 1, Twip(300))).collect();
        let layout = paginate(&fragments, &config);
        assert!(
            layout.page_count() >= 2,
            "content taller than a page paginates"
        );
        // No page's placed fragments exceed the content height.
        let content_h = config.content_area().size.height.raw();
        for page in &layout.pages {
            let used: i32 = page.placed.iter().map(|p| p.rect.size.height.raw()).sum();
            assert!(used <= content_h, "a page never overfills its content area");
        }
        // Every fragment is placed exactly once.
        let placed: usize = layout.pages.iter().map(|p| p.placed.len()).sum();
        assert_eq!(placed, 60);
    }

    #[test]
    fn a_fragment_taller_than_the_page_is_placed_as_overflow() {
        let config = letter_config();
        // 20_000 twips > the 12_960-twip content area — must not loop forever.
        let fragments = vec![paragraph(1, Twip(20_000)), paragraph(2, Twip(240))];
        let layout = paginate(&fragments, &config);
        assert_eq!(
            layout.pages[0].placed.len(),
            1,
            "the oversized block gets its own page"
        );
        assert_eq!(layout.page_count(), 2);
    }

    fn with(page_break_before: bool, keep_next: bool) -> BreakControl {
        BreakControl {
            page_break_before,
            keep_next,
            ..BreakControl::default()
        }
    }

    #[test]
    fn a_long_paragraph_splits_across_pages() {
        let config = letter_config();
        let content_h = config.content_area().size.height.raw();
        // 200 lines of 200 twips = 40_000 twips >> a 12_960-twip content area.
        let para = multiline(1, 200, Twip(200), BreakControl::default());
        let total_lines = 200;
        let layout = paginate(&[para], &config);
        assert!(
            layout.page_count() >= 3,
            "a tall paragraph fills several pages"
        );
        // Every page's placed lines fit its content area, and all lines survive.
        let mut placed_lines = 0;
        for page in &layout.pages {
            for placed in &page.placed {
                let BlockFragment::Paragraph { lines, .. } = &placed.fragment else {
                    panic!()
                };
                placed_lines += lines.lines.len();
                let used: i32 = lines.lines.iter().map(|l| l.height.raw()).sum();
                assert!(used <= content_h, "a page never overfills");
                // The first line of each chunk is re-based near the top (its run
                // baseline is one line-height, not an accumulated offset).
                if let Some(first) = lines.lines.first() {
                    assert!(
                        first.runs[0].origin.y.raw() <= 200,
                        "split chunks are re-origined to the fragment top"
                    );
                }
            }
        }
        assert_eq!(placed_lines, total_lines, "no lines are lost in the split");
    }

    #[test]
    fn keep_lines_paragraph_is_not_split() {
        let config = letter_config();
        // A 20-line paragraph that fits a page but not the remaining space after a
        // filler; keepLines must move it whole, not split it.
        let content_h = config.content_area().size.height.raw();
        let filler = paragraph(1, Twip(content_h - 400));
        let kept = multiline(
            2,
            20,
            Twip(200),
            BreakControl {
                keep_lines: true,
                ..BreakControl::default()
            },
        );
        let layout = paginate(&[filler, kept], &config);
        assert_eq!(layout.page_count(), 2);
        // The kept paragraph is a single un-split fragment on page 2.
        assert_eq!(layout.pages[1].placed.len(), 1);
        let BlockFragment::Paragraph { lines, .. } = &layout.pages[1].placed[0].fragment else {
            panic!()
        };
        assert_eq!(
            lines.lines.len(),
            20,
            "keepLines keeps all 20 lines together"
        );
    }

    #[test]
    fn widow_control_avoids_a_lone_trailing_line() {
        let config = letter_config();
        let content_h = config.content_area().size.height.raw();
        // Fill the page so exactly one line of the next paragraph would fit — with
        // widow control the split leaves >= 2 lines for the continuation.
        let line_h = 200;
        let fits = (content_h / line_h) as usize - 1; // lines that fit after filler
        let filler = paragraph(1, Twip(content_h - (fits as i32 + 1) * line_h));
        let para = multiline(
            2,
            fits + 2, // one more than fits, so a naive split leaves 1 widow
            Twip(line_h),
            BreakControl {
                widow_control: true,
                ..BreakControl::default()
            },
        );
        let layout = paginate(&[filler, para], &config);
        // The paragraph's continuation must have at least 2 lines (no widow).
        let last = layout.pages.last().unwrap();
        let BlockFragment::Paragraph { lines, .. } = &last.placed.last().unwrap().fragment else {
            panic!()
        };
        assert!(
            lines.lines.len() >= 2,
            "widow control keeps >= 2 lines on the continuation (got {})",
            lines.lines.len()
        );
    }

    #[test]
    fn page_break_before_forces_a_new_page() {
        let config = letter_config();
        // Both fit on one page, but para 2 has pageBreakBefore -> page 2.
        let fragments = vec![
            paragraph(1, Twip(240)),
            paragraph_with(2, Twip(240), with(true, false)),
        ];
        let layout = paginate(&fragments, &config);
        assert_eq!(layout.page_count(), 2, "pageBreakBefore forces a new page");
        assert_eq!(layout.pages[0].placed.len(), 1);
        assert_eq!(layout.pages[1].placed.len(), 1);
    }

    #[test]
    fn page_break_before_on_the_first_fragment_makes_no_blank_page() {
        let config = letter_config();
        let fragments = vec![paragraph_with(1, Twip(240), with(true, false))];
        let layout = paginate(&fragments, &config);
        assert_eq!(layout.page_count(), 1, "no leading blank page");
    }

    #[test]
    fn keep_next_keeps_a_heading_with_its_body() {
        let config = letter_config();
        // Fill most of page 1, then a keepNext "heading" that alone fits the
        // remaining space, then a body block that does not — the group moves
        // together to page 2 so the heading is never orphaned at the page foot.
        let content_h = config.content_area().size.height.raw();
        let mut fragments = vec![paragraph(1, Twip(content_h - 500))];
        fragments.push(paragraph_with(2, Twip(300), with(false, true))); // heading, keepNext
        fragments.push(paragraph(3, Twip(400))); // body
        let layout = paginate(&fragments, &config);
        assert_eq!(layout.page_count(), 2);
        // The heading (node 2) and body (node 3) are on the same page (page 2).
        let page2_nodes: Vec<_> = layout.pages[1]
            .placed
            .iter()
            .map(|p| p.fragment.node_id())
            .collect();
        assert!(
            page2_nodes.contains(&NodeId::from_parts(2, 1).unwrap())
                && page2_nodes.contains(&NodeId::from_parts(3, 1).unwrap()),
            "keepNext keeps the heading with its body on page 2"
        );
    }

    // --- Incremental re-pagination (P1D-004a) ----------------------------------
    //
    // The golden invariant: for ANY edit, `repaginate` must return a layout that
    // is *field-for-field identical* to a full `paginate` of the new galley
    // (page count, every placed fragment, every rect, every page number, and the
    // flow provenance). If these ever diverge, the incremental path is unsound.

    /// A galley of `n` single-line paragraphs, node ids `1..=n`, `height` twips.
    fn galley(n: usize, height: Twip) -> Vec<BlockFragment> {
        (1..=n).map(|i| paragraph(i as u64, height)).collect()
    }

    /// A galley with a forced page break every `every` paragraphs — modeling
    /// headings/section starts. These are hard re-anchor points, so an edit's
    /// effect is contained between two breaks and pagination re-stabilizes: the
    /// realistic case where the stabilization halt reuses the tail. (A perfectly
    /// uniform stream never re-syncs after a non-page-multiple shift; that is the
    /// `whole_tail_reflows` case, correct but pathological.)
    fn galley_anchored(n: usize, height: Twip, every: usize) -> Vec<BlockFragment> {
        (1..=n)
            .map(|i| {
                let bc = if i > 1 && i % every == 1 {
                    with(true, false) // pageBreakBefore
                } else {
                    BreakControl::default()
                };
                paragraph_with(i as u64, height, bc)
            })
            .collect()
    }

    /// Asserts the incremental result equals a full re-paginate (the golden
    /// invariant) and that its cost accounting is self-consistent, then returns
    /// the [`RepaginateStats`] so callers can assert boundedness.
    fn golden(
        prev: &[BlockFragment],
        new: &[BlockFragment],
        config: &PageConfig,
    ) -> RepaginateStats {
        let prev_layout = paginate(prev, config);
        let (inc, stats) = repaginate_with_stats(&prev_layout, prev, new, config);
        let full = paginate(new, config);
        assert_eq!(
            inc, full,
            "incremental re-pagination must equal a full paginate"
        );
        assert_eq!(
            stats.reused_prefix + stats.reflowed + stats.reused_tail,
            full.page_count(),
            "every page is accounted for as reused-prefix, reflowed, or reused-tail"
        );
        stats
    }

    #[test]
    fn incremental_equals_full_for_an_edit_in_the_middle() {
        let config = letter_config();
        // Sectioned document (a forced break every 20 paragraphs); grow one
        // paragraph in the middle. The edit is contained between two breaks, so
        // the pages above AND below are reused — only the edited section reflows.
        let prev = galley_anchored(300, Twip(400), 20);
        let mut new = prev.clone();
        new[150] = paragraph(151, Twip(1_200));
        let stats = golden(&prev, &new, &config);
        assert!(
            stats.reused_prefix > 0,
            "an edit mid-document reuses the pages above it: {stats:?}"
        );
        assert!(
            stats.reused_tail > 0,
            "the stabilization halt reuses the pages below it: {stats:?}"
        );
    }

    #[test]
    fn incremental_reuses_almost_everything_for_an_edit_near_the_end() {
        let config = letter_config();
        let prev = galley(300, Twip(400));
        let mut new = prev.clone();
        new[299] = paragraph(300, Twip(1_200));
        let stats = golden(&prev, &new, &config);
        // Only the last page (or two, if the edit pushed a page) is re-flowed;
        // everything above is reused as the prefix.
        assert!(
            stats.reflowed <= 2,
            "editing the last paragraph re-flows at most the final page(s): {stats:?}"
        );
        assert!(
            stats.reused_prefix > 0,
            "the pages above are reused: {stats:?}"
        );
    }

    #[test]
    fn incremental_equals_full_for_an_edit_on_the_first_page() {
        let config = letter_config();
        let prev = galley(300, Twip(400));
        let mut new = prev.clone();
        new[0] = paragraph(1, Twip(1_200));
        // Correct even when nothing above the edit can be reused.
        golden(&prev, &new, &config);
    }

    #[test]
    fn incremental_equals_full_when_a_fragment_is_inserted() {
        let config = letter_config();
        let prev = galley(300, Twip(400));
        let mut new = prev.clone();
        new.insert(150, paragraph(10_000, Twip(500))); // fresh node id
        golden(&prev, &new, &config);
    }

    #[test]
    fn incremental_equals_full_when_a_fragment_is_removed() {
        let config = letter_config();
        let prev = galley(300, Twip(400));
        let mut new = prev.clone();
        new.remove(150);
        golden(&prev, &new, &config);
    }

    #[test]
    fn incremental_equals_full_when_content_is_appended() {
        let config = letter_config();
        let prev = galley(300, Twip(400));
        let mut new = prev.clone();
        new.push(paragraph(10_001, Twip(400)));
        let stats = golden(&prev, &new, &config);
        assert!(
            stats.reused_prefix > 0,
            "appending reuses the whole document above"
        );
    }

    #[test]
    fn incremental_equals_full_editing_a_paragraph_that_splits_across_pages() {
        let config = letter_config();
        // A tall multi-line paragraph spanning pages, edited in place, with
        // ordinary paragraphs on either side.
        let mut prev = vec![paragraph(1, Twip(400))];
        prev.push(multiline(2, 120, Twip(240), BreakControl::default()));
        prev.extend((3..=40).map(|i| paragraph(i as u64, Twip(400))));
        let mut new = prev.clone();
        // Shorten the split paragraph (fewer lines) -> re-flows from there down.
        new[1] = multiline(2, 80, Twip(240), BreakControl::default());
        golden(&prev, &new, &config);
    }

    #[test]
    fn incremental_equals_full_with_keep_next_groups_around_the_edit() {
        let config = letter_config();
        // Interleave keepNext heading/body pairs so the edit lands near a group.
        let mut prev = Vec::new();
        for i in 0..60u64 {
            if i % 5 == 0 {
                prev.push(paragraph_with(i * 2 + 1, Twip(300), with(false, true))); // heading
            } else {
                prev.push(paragraph(i * 2 + 1, Twip(400)));
            }
        }
        let mut new = prev.clone();
        new[31] = paragraph(63, Twip(900)); // grow one body block mid-document
        golden(&prev, &new, &config);
    }

    #[test]
    fn incremental_equals_full_for_an_identity_edit() {
        let config = letter_config();
        let prev = galley(120, Twip(400));
        let new = prev.clone();
        // No change -> everything reused, still equal to a full paginate.
        golden(&prev, &new, &config);
    }

    #[test]
    fn incremental_falls_back_to_full_when_geometry_changes() {
        let config = letter_config();
        let prev = galley(120, Twip(400));
        let prev_layout = paginate(&prev, &config);
        let mut new = prev.clone();
        new[10] = paragraph(11, Twip(800));
        // A different page geometry: reused pages would carry stale content areas,
        // so nothing is reused — but the result is still a correct full paginate.
        let taller = PageConfig {
            page_size: Size::new(Twip(12_240), Twip(20_000)),
            ..config
        };
        let inc = repaginate(&prev_layout, &prev, &new, &taller);
        assert_eq!(inc, paginate(&new, &taller));
    }

    #[test]
    fn incremental_equals_full_editing_after_a_page_straddling_keepnext_heading() {
        let config = letter_config();
        let content_h = config.content_area().size.height.raw();
        // Page 1 nearly full (500 twips of slack), then a keepNext heading whose
        // body does NOT fit the slack -> the (heading, body) group is pushed whole
        // to page 2. Shrinking the body so the group now fits page 1's slack must
        // pull BOTH up into page 1 -> the reused page 1 must be invalidated. This
        // is hazard H1: a keep-group straddling the reuse boundary flows upward.
        let filler = paragraph(1, Twip(content_h - 500));
        let heading = paragraph_with(2, Twip(300), with(false, true)); // keepNext
        let prev = vec![
            filler.clone(),
            heading.clone(),
            paragraph(3, Twip(400)), // 300+400=700 > 500 slack -> group to page 2
            paragraph(4, Twip(400)),
        ];
        let new = vec![
            filler,
            heading,
            paragraph(3, Twip(100)), // 300+100=400 <= 500 -> group fits page 1
            paragraph(4, Twip(400)),
        ];
        golden(&prev, &new, &config);
    }

    #[test]
    fn incremental_equals_full_when_the_whole_tail_reflows() {
        let config = letter_config();
        // A densely packed doc with no keep constraints, edited so total height
        // shifts by a non-page-multiple: every downstream break de-phases and the
        // tail never re-stabilizes. `repaginate` must still equal a full paginate
        // (it simply re-flows to the end — no halt is required for correctness).
        let prev = galley(400, Twip(431)); // 431 doesn't divide the content area
        let mut new = prev.clone();
        new[3] = paragraph(4, Twip(431 + 137)); // odd delta, ripples forever
        golden(&prev, &new, &config);
    }

    #[test]
    fn stabilization_halt_bounds_work_for_an_edit_near_the_top() {
        let config = letter_config();
        // Sectioned document. Edit the SECOND paragraph (first section): nothing
        // above to reuse, but the stabilization halt reuses almost the entire
        // tail — so the work (reflowed pages) is a small constant, independent of
        // document length. This is the case editors classically get wrong.
        let prev = galley_anchored(300, Twip(400), 20);
        let mut new = prev.clone();
        new[1] = paragraph(2, Twip(1_200));
        let stats = golden(&prev, &new, &config);
        assert_eq!(
            stats.reused_prefix, 0,
            "the edit is on the first page: {stats:?}"
        );
        assert!(
            stats.reused_tail > 0,
            "the tail is reused via the stabilization halt: {stats:?}"
        );
        assert!(
            stats.reflowed <= 3,
            "an edit near the top re-flows only a handful of pages, not the whole \
             document: {stats:?}"
        );
    }

    #[test]
    fn stabilization_halt_survives_fragment_insertion_upstream() {
        let config = letter_config();
        // Pressing Enter near the top shifts every downstream absolute index by
        // one, yet the halt (keyed by index-from-end) must still splice the tail.
        let prev = galley_anchored(300, Twip(400), 20);
        let mut new = prev.clone();
        new.insert(2, paragraph(10_100, Twip(400)));
        let stats = golden(&prev, &new, &config);
        assert!(
            stats.reused_tail > 0,
            "index-from-end keying reuses the tail across an insertion: {stats:?}"
        );
    }

    #[test]
    fn incremental_equals_full_editing_above_a_paragraph_that_spans_a_page_break() {
        let config = letter_config();
        // A tall paragraph straddles a page boundary; an edit ABOVE it changes how
        // many of its lines sit on each page (the seam line_offset moves). H4.
        let mut prev: Vec<BlockFragment> = (1..=30).map(|i| paragraph(i, Twip(400))).collect();
        prev.push(multiline(100, 120, Twip(240), BreakControl::default()));
        prev.extend((31..=40).map(|i| paragraph(i, Twip(400))));
        let mut new = prev.clone();
        new[5] = paragraph(6, Twip(760)); // grow a paragraph above the straddler
        golden(&prev, &new, &config);
    }

    // --- Table pagination (P1D-003) -------------------------------------------

    use crate::block::{CellBorders, CellFragment};

    fn tnode(id: u64) -> NodeId {
        NodeId::from_parts(id, 1).unwrap()
    }

    /// A one-column cell holding the given block fragments.
    fn cell_of(id: u64, blocks: Vec<BlockFragment>) -> CellFragment {
        CellFragment {
            id: tnode(id),
            grid_span: 1,
            x: Twip::ZERO,
            width: Twip(3000),
            blocks,
            borders: CellBorders::default(),
            shading: None,
        }
    }

    /// A table row whose height is its cells' content height.
    fn table_row(
        id: u64,
        table: u64,
        cells: Vec<CellFragment>,
        can_split: bool,
        header: bool,
    ) -> BlockFragment {
        let height = BlockFragment::cells_content_height(&cells);
        BlockFragment::TableRow {
            id: tnode(id),
            table: tnode(table),
            cells,
            height,
            can_split,
            header,
            clip: false,
        }
    }

    #[test]
    fn a_cant_split_row_taller_than_the_remaining_space_moves_whole() {
        let config = letter_config();
        let content_h = config.content_area().size.height.raw();
        // Fill the page to a 500-twip slack, then a `cantSplit` row 1000 twips
        // tall: it cannot fit and must move whole to page 2.
        let filler = paragraph(1, Twip(content_h - 500));
        let row = table_row(
            2,
            500,
            vec![cell_of(3, vec![paragraph(4, Twip(1000))])],
            false,
            false,
        );
        let layout = paginate(&[filler, row], &config);
        assert_eq!(layout.page_count(), 2, "the row moved to a second page");
        assert_eq!(
            layout.pages[0].placed.len(),
            1,
            "only the filler is on page 1"
        );
        // The row is on page 2, whole (its full 1000-twip content height).
        let page2 = &layout.pages[1];
        assert_eq!(page2.placed.len(), 1);
        assert_eq!(
            page2.placed[0].fragment.height(),
            Twip(1000),
            "the row is intact"
        );
    }

    #[test]
    fn a_table_repeats_its_header_row_on_each_continuation_page() {
        let config = letter_config();
        // A header row then 20 body rows of 1000 twips: the table overflows page 1,
        // and the header must reappear at the top of page 2.
        let header = table_row(
            10,
            5,
            vec![cell_of(11, vec![paragraph(12, Twip(300))])],
            false,
            true,
        );
        let mut frags = vec![header];
        for i in 0..20u64 {
            frags.push(table_row(
                100 + i,
                5,
                vec![cell_of(200 + i, vec![paragraph(300 + i, Twip(1000))])],
                true,
                false,
            ));
        }
        let layout = paginate(&frags, &config);
        assert!(layout.page_count() >= 2, "the table spans multiple pages");
        let header_id = tnode(10);
        assert_eq!(
            layout.pages[0].placed[0].fragment.node_id(),
            header_id,
            "the header leads page 1"
        );
        let page2 = &layout.pages[1];
        let first = &page2.placed[0].fragment;
        assert_eq!(first.node_id(), header_id, "the header repeats atop page 2");
        assert!(
            matches!(first, BlockFragment::TableRow { header: true, .. }),
            "the repeated fragment is the header row"
        );
        // The repeated header does not corrupt flow provenance: page 2 begins at a
        // real body row, not back at the header's galley index (0).
        assert_ne!(
            page2.flow.start.fragment, 0,
            "flow provenance skips the repeated header"
        );
    }

    #[test]
    fn a_tall_table_row_splits_across_pages() {
        let config = letter_config();
        let content_h = config.content_area().size.height.raw();
        // A single splittable row whose cell holds a 120-line paragraph
        // (28_800 twips) — far taller than the page.
        let row = table_row(
            20,
            5,
            vec![cell_of(
                21,
                vec![multiline(22, 120, Twip(240), BreakControl::default())],
            )],
            true,
            false,
        );
        let layout = paginate(&[row], &config);
        assert!(layout.page_count() >= 3, "the tall row splits over pages");
        // No page overfills, and every line survives across the chunks.
        let mut total_lines = 0;
        for page in &layout.pages {
            let used: i32 = page.placed.iter().map(|p| p.rect.size.height.raw()).sum();
            assert!(used <= content_h, "a page never overfills");
            for placed in &page.placed {
                let BlockFragment::TableRow { cells, .. } = &placed.fragment else {
                    panic!("expected a table row");
                };
                for cell in cells {
                    for block in &cell.blocks {
                        if let BlockFragment::Paragraph { lines, .. } = block {
                            total_lines += lines.lines.len();
                        }
                    }
                }
            }
        }
        assert_eq!(total_lines, 120, "no lines are lost when the row splits");
    }
}
