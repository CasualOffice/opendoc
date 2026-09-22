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
use casual_doc_model::v1::{SectionBoundary, SectionId};
// Separate `use` line (kept out of the sorted block above) to avoid import-list
// merge collisions with other agents editing this file.
use casual_doc_model::v1::NumberFormat;

use crate::block::{BlockFragment, BoxMetrics, BreakControl, CellFragment, ParagraphDecor};
use crate::flow::shape_field_run;
// Separate `use` lines (anti-conflict, and the measure tier is a distinct
// concern from the block/flow types above): the two-tier galley of `docs/113`.
use crate::measure::FragmentMeasure;
use crate::measure::MeasureLayout;
use crate::measure::Paginable;
use crate::measure::PaginableCell;
use crate::measure::cells_content_height;
use crate::model::ModelPos;
use crate::page::{AnchorContent, FlowPos, FlowSpan, Page, PaginatedLayout, PlacedFragment};
use crate::text::{FieldKind, GlyphRun, Line, LineLayout, LineShaper};
use crate::units::{Point, Rect, Size, Twip};

/// The page geometry of a section: the page box, its margins, and the header /
/// footer bands **nested inside** the top and bottom margins (Word's geometry).
///
/// Word does not stack the header/footer band on top of the margins; it nests
/// them within. The header sits `header_distance` from the top edge and the
/// footer `footer_distance` from the bottom edge (the `w:pgMar/@w:header` and
/// `@w:footer` values), and the body only loses space to a band when that band
/// extends *past* the margin:
///
/// ```text
/// body_top    = header_height == 0 ? margin_top    : max(margin_top,    header_distance + header_height)
/// body_bottom = footer_height == 0 ? margin_bottom : max(margin_bottom, footer_distance + footer_height)
/// ```
///
/// The zero-height guards are not an optimization: a section with no header part
/// still carries `w:pgMar/@w:header` (usually the 720-twip default), so without
/// them a headerless page reserves a band that does not exist.
///
/// So a header shorter than the top margin costs the body nothing (the common
/// case) — the previous implementation subtracted the full band height on top of
/// the full margin, over-reserving `header_height + footer_height` on every page
/// and over-paginating every header/footer document (`docs/46` §F6a). Band
/// heights are **fixed per section** (the tallest of the section's header/footer
/// variants), so every page in the section shares one content area — which is
/// what lets the incremental paginator reuse pages (it keys reuse on a single
/// `content_area()`).
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
    /// Leading (start) margin, **including the binding gutter**
    /// (`w:pgMar/@w:gutter`), which Word adds to the inside edge — see
    /// [`crate::document_layout::document_page_config`]. This is the page's
    /// *physical* left margin on a recto (odd) page.
    pub margin_start: Twip,
    /// Trailing (end) margin — the outside edge on a recto (odd) page.
    ///
    /// Under `w:mirrorMargins` the inside and outside margins swap on verso
    /// (even) pages; the section paginator ([`crate::columns`]) applies that
    /// per page, so this field always holds the recto geometry.
    pub margin_end: Twip,
    /// Distance from the top page edge to the header band (`w:pgMar/@w:header`,
    /// Word default 720 twips). The header band is anchored here, nested inside
    /// the top margin.
    pub header_distance: Twip,
    /// Distance from the bottom page edge to the footer band (`w:pgMar/@w:footer`,
    /// Word default 720 twips). The footer band is anchored here, nested inside
    /// the bottom margin.
    pub footer_distance: Twip,
    /// Reserved header-band height (`0` = no header). The body content area only
    /// starts below the top margin if `header_distance + header_height` exceeds it.
    pub header_height: Twip,
    /// Reserved footer-band height (`0` = no footer). The body content area only
    /// ends above the bottom margin if `footer_distance + footer_height` exceeds it.
    pub footer_height: Twip,
}

impl PageConfig {
    /// The y of the top of the body content area: the top margin, or the bottom of
    /// the header band if the band (nested at `header_distance`) reaches past it.
    ///
    /// A section with **no header** has no band at all, so `header_distance` must
    /// not participate: Word starts the body at the top margin. Guarding on
    /// `header_height == 0` matters because `w:pgMar/@w:header` is written on
    /// every section whether or not a header part exists, and it is usually the
    /// 720-twip default — larger than a narrow top margin. Without the guard a
    /// headerless document with `top="567" header="737"` silently lost 170 twips
    /// of body on every page to a header that does not exist.
    #[must_use]
    fn body_top(&self) -> Twip {
        if self.header_height.is_zero() {
            return self.margin_top;
        }
        self.margin_top
            .max(self.header_distance + self.header_height)
    }

    /// The distance from the bottom page edge to the bottom of the body content
    /// area: the bottom margin, or the top of the footer band if it reaches past it.
    ///
    /// Mirrors [`body_top`](Self::body_top): with **no footer** there is no band,
    /// so `footer_distance` must not participate and the body ends at the bottom
    /// margin.
    #[must_use]
    fn body_bottom(&self) -> Twip {
        if self.footer_height.is_zero() {
            return self.margin_bottom;
        }
        self.margin_bottom
            .max(self.footer_distance + self.footer_height)
    }

    /// The content area (page box minus margins, growing only for header/footer
    /// bands that extend past the margins) — where flow content is placed.
    #[must_use]
    pub fn content_area(&self) -> Rect {
        let width = self.page_size.width - self.margin_start - self.margin_end;
        let body_top = self.body_top();
        let height = self.page_size.height - body_top - self.body_bottom();
        Rect::new(
            Point::new(self.margin_start, body_top),
            Size::new(width.max(Twip::ZERO), height.max(Twip::ZERO)),
        )
    }

    /// The header band rectangle (nested `header_distance` from the top edge,
    /// `header_height` tall), where the running-content pass lays the selected
    /// header. Zero-height when the section has no header.
    #[must_use]
    pub fn header_band(&self) -> Rect {
        let width = self.page_size.width - self.margin_start - self.margin_end;
        Rect::new(
            Point::new(self.margin_start, self.header_distance),
            Size::new(width.max(Twip::ZERO), self.header_height),
        )
    }

    /// The footer band rectangle (its bottom `footer_distance` from the bottom
    /// edge, `footer_height` tall). Zero-height when the section has no footer.
    #[must_use]
    pub fn footer_band(&self) -> Rect {
        let width = self.page_size.width - self.margin_start - self.margin_end;
        let y = self.page_size.height - self.footer_distance - self.footer_height;
        Rect::new(
            Point::new(self.margin_start, y),
            Size::new(width.max(Twip::ZERO), self.footer_height),
        )
    }
}

fn reservation_for_page(reservations: &[Twip], page_index: usize) -> Twip {
    reservations.get(page_index).copied().unwrap_or(Twip::ZERO)
}

fn content_area_with_reservation(config: &PageConfig, reservation: Twip) -> Rect {
    let mut content = config.content_area();
    content.size.height = (content.size.height - reservation).max(Twip::ZERO);
    content
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
    paginate_with_checkpoints(fragments, config, 0).0
}

/// [`paginate`] that also records a resumable [`Checkpoint`] every
/// `checkpoint_interval` pages (`0` = none).
///
/// The layout is byte-identical to [`paginate`]'s — recording a checkpoint
/// observes the paginator's state, it does not steer it.
#[must_use]
pub fn paginate_with_checkpoints(
    fragments: &[BlockFragment],
    config: &PageConfig,
    checkpoint_interval: usize,
) -> (PaginatedLayout, Vec<Checkpoint>) {
    let mut p = Paginator::new(config, fragments, Vec::new(), 0, FlowPos::at(0), None, &[]);
    p.checkpoint_interval = checkpoint_interval;
    p.run(0);
    p.flush();
    (PaginatedLayout { pages: p.pages }, p.checkpoints)
}

/// Paginates the **measure tier** of a galley: the page boundaries of the whole
/// document, and the checkpoints any page can be re-derived from.
///
/// This is the pass that makes an exact page count affordable for a document
/// whose shaped galley would not fit in memory (`docs/113` §4 Q1). It runs the
/// *same* `Paginator` as [`paginate`], over fragments that carry heights and
/// break opportunities but no glyphs, and emits
/// [`PageOutline`](crate::measure::PageOutline)s instead of [`Page`]s. Because
/// the walk is shared, the boundaries it reports are the boundaries a full
/// pagination would report — asserted by the `measure_equals_full_*` tests.
#[must_use]
pub fn paginate_measures(
    measures: &[FragmentMeasure],
    config: &PageConfig,
    checkpoint_interval: usize,
) -> MeasureLayout {
    let mut p = Paginator::new(config, measures, Vec::new(), 0, FlowPos::at(0), None, &[]);
    p.checkpoint_interval = checkpoint_interval;
    p.run(0);
    p.flush();
    MeasureLayout {
        pages: p.pages,
        checkpoints: p.checkpoints,
    }
}

/// Paginates the pages from `checkpoint` onward, without paginating anything
/// above it.
///
/// The guarantee is the same shape as [`repaginate`]'s: the returned pages are
/// **field-for-field identical** to `paginate(fragments, config).pages[checkpoint.page_index..]`,
/// including their page numbers, their flow spans (galley-absolute, not
/// window-relative) and a table's repeated header rows when the table straddles
/// the checkpoint. That is verified by the `paginate_from_equals_full_*` tests.
///
/// The checkpoint may come from either tier — [`paginate_measures`] produces
/// them from heights alone, which is the point: the window that has to be
/// painted is reached without the galley above it ever being shaped.
#[must_use]
pub fn paginate_from(
    fragments: &[BlockFragment],
    config: &PageConfig,
    checkpoint: &Checkpoint,
) -> PaginatedLayout {
    let mut p = Paginator::new(
        config,
        fragments,
        Vec::new(),
        checkpoint.page_index as usize,
        checkpoint.at,
        None,
        &[],
    );
    p.seed_table_context(checkpoint);
    p.run(checkpoint.at.fragment as usize);
    p.flush();
    PaginatedLayout { pages: p.pages }
}

/// [`paginate_from`] over a **window** of the galley: `fragments` is the slice
/// starting at galley index `base`, and the checkpoint's indices are
/// galley-absolute.
///
/// This is what lets a window be painted without the fragments above it
/// existing. Rather than teaching the paginator a second coordinate system —
/// which would put an index translation on every line of the walk — the
/// checkpoint is translated *in*, the paginator runs unchanged, and the only
/// galley-relative thing it emits, [`FlowSpan`], is translated *out*. With
/// `base == 0` it is [`paginate_from`] exactly, which is how the equality is
/// asserted (`paginate_from_based_at_zero_is_paginate_from`).
///
/// # Panics
///
/// If the checkpoint names a position or a repeated header row that lies
/// before `base` — the window does not contain what it would have to resume
/// from, and producing a page anyway would produce a wrong one.
#[must_use]
pub fn paginate_from_based(
    fragments: &[BlockFragment],
    base: u32,
    config: &PageConfig,
    checkpoint: &Checkpoint,
) -> PaginatedLayout {
    assert!(
        checkpoint.at.fragment >= base && checkpoint.table_headers.iter().all(|i| *i >= base),
        "a windowed pagination must contain everything its checkpoint names \
         (base {base}, checkpoint at {:?}, headers {:?})",
        checkpoint.at,
        checkpoint.table_headers,
    );
    let local = Checkpoint {
        page_index: checkpoint.page_index,
        at: FlowPos {
            fragment: checkpoint.at.fragment - base,
            line: checkpoint.at.line,
        },
        current_table: checkpoint.current_table,
        table_headers: checkpoint.table_headers.iter().map(|i| i - base).collect(),
    };
    let mut layout = paginate_from(fragments, config, &local);
    for page in &mut layout.pages {
        page.flow.start.fragment += base;
        page.flow.end.fragment += base;
    }
    layout
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
    let mut p = Paginator::new(
        config,
        new_galley,
        prefix,
        0,
        FlowPos::at(resume_index as u32),
        halt,
        &[],
    );
    p.run(resume_index);
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
    if first.page_size != config.page_size
        || first.content_area != config.content_area()
        || first.section != config.section
    {
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

/// The carry state that lets pagination restart at a page boundary instead of
/// at the top of the document (`docs/113` §3.1).
///
/// Pagination is a **forward fill** and each page begins at a fresh content-top
/// cursor, so everything below a page boundary is a pure function of the state
/// at that boundary plus the geometry and the downstream galley. These are
/// exactly the `Paginator`'s own fields at such a boundary:
///
/// - `at` — the flow position of the next content to place. Restricted to a
///   whole-fragment boundary (`line == 0`) that starts a keep-group, because
///   those are the positions the group walk can restart from and reproduce
///   what the forward fill did; see `Paginator::is_resumable_boundary`, which
///   applies the same two conditions `safe_resume_page` applies to the
///   incremental path.
/// - `page_index` — how many pages precede it, so resumed pages carry the page
///   numbers they would have had (`PAGE` fields and `Page::number`).
/// - `current_table` / `table_headers` — the table whose rows are being placed
///   across the boundary and its repeated `w:tblHeader` rows. Without them, a
///   table resumed mid-way would lose its repeated header on the first resumed
///   page. The headers are named by **galley index** rather than copied, which
///   keeps a checkpoint small and makes it valid for either tier: the measure
///   pass produces the checkpoints, the paint pass consumes them.
///
/// A checkpoint is therefore 80 bytes of struct (measured by the committed
/// `layout_footprint` example on macOS arm64) plus four bytes per repeated
/// header row, one every [`DEFAULT_CHECKPOINT_INTERVAL`] pages — 5,680 bytes
/// for the 4,546-page synthetic document that probe paginates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Checkpoint {
    /// Number of pages that precede this boundary; the next page is
    /// `page_index + 1`.
    pub page_index: u32,
    /// Flow position of the first content on the next page.
    pub at: FlowPos,
    /// The table whose rows are being placed across this boundary, if any.
    pub current_table: Option<NodeId>,
    /// Galley indices of that table's repeated header rows, in capture order.
    pub table_headers: Vec<u32>,
}

/// How many pages apart resumable checkpoints are placed by default
/// (`docs/113` §4 Q2 starting value).
pub const DEFAULT_CHECKPOINT_INTERVAL: usize = 64;

/// Mutable pagination state, walked fragment by fragment.
///
/// Generic over the galley tier ([`Paginable`]). The same walk drives the
/// **paint** tier ([`BlockFragment`] → [`Page`]) and the **measure** tier
/// ([`FragmentMeasure`] → [`PageOutline`](crate::measure::PageOutline)), so the
/// two cannot disagree about where a page ends — which is the property
/// `docs/113` §5 requires and the `measure_equals_full_*` tests assert.
struct Paginator<'a, F: Paginable> {
    config: &'a PageConfig,
    /// The galley being walked. Held so a checkpoint can name a table's header
    /// rows by galley index instead of copying the fragments, and so the
    /// resumability of a boundary can be tested when it is reached.
    galley: &'a [F],
    reservations: &'a [Twip],
    content: Rect,
    content_bottom: i32,
    content_height: i32,
    /// The pages this run emitted. A resumed run starts empty and counts from
    /// `number_base`.
    pages: Vec<F::Page>,
    /// How many pages precede the ones in `pages` (0 for a full run).
    number_base: usize,
    placed: Vec<F::Placed>,
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
    table_headers: Vec<F>,
    /// The galley indices `table_headers` were captured from, so a
    /// [`Checkpoint`] can carry the header context without carrying fragments.
    table_header_indices: Vec<u32>,
    /// Set while a row is being cut across a page boundary. A page closed in
    /// the middle of a fragment cannot be resumed from — see
    /// `Paginator::split_table_row`.
    mid_fragment: bool,
    /// Record a [`Checkpoint`] every this many pages; `0` records none.
    checkpoint_interval: usize,
    /// The checkpoints recorded so far.
    checkpoints: Vec<Checkpoint>,
}

impl<'a, F: Paginable> Paginator<'a, F> {
    /// Creates a paginator over `galley` seeded with already-emitted `pages`
    /// (and `number_base` pages before those), resuming at flow position `at`
    /// (a page-top cursor). For a full run pass an empty prefix, a base of 0,
    /// `FlowPos::at(0)`, and no halt lookup.
    fn new(
        config: &'a PageConfig,
        galley: &'a [F],
        pages: Vec<F::Page>,
        number_base: usize,
        at: FlowPos,
        halt: Option<HaltLookup>,
        reservations: &'a [Twip],
    ) -> Self {
        let content = content_area_with_reservation(
            config,
            reservation_for_page(reservations, number_base + pages.len()),
        );
        Self {
            config,
            galley,
            reservations,
            content,
            content_bottom: content.bottom().raw(),
            content_height: content.size.height.raw(),
            pages,
            number_base,
            placed: Vec::new(),
            cursor_y: content.origin.y,
            at,
            page_start: at,
            halt,
            halted: None,
            current_table: None,
            table_headers: Vec::new(),
            table_header_indices: Vec::new(),
            mid_fragment: false,
            checkpoint_interval: 0,
            checkpoints: Vec::new(),
        }
    }

    /// The 0-based index of the page currently being filled — equivalently, how
    /// many pages precede it.
    fn page_index(&self) -> usize {
        self.number_base + self.pages.len()
    }

    /// Restores the table context a [`Checkpoint`] captured, so a table whose
    /// rows straddle the boundary keeps repeating its header rows.
    fn seed_table_context(&mut self, checkpoint: &Checkpoint) {
        let galley = self.galley;
        self.current_table = checkpoint.current_table;
        self.table_header_indices
            .clone_from(&checkpoint.table_headers);
        self.table_headers = checkpoint
            .table_headers
            .iter()
            .filter_map(|index| galley.get(*index as usize).cloned())
            .collect();
    }

    fn reset_page_content(&mut self) {
        self.content = content_area_with_reservation(
            self.config,
            reservation_for_page(self.reservations, self.page_index()),
        );
        self.content_bottom = self.content.bottom().raw();
        self.content_height = self.content.size.height.raw();
    }

    /// Walks keep-with-next groups of the galley from `from`, placing them into
    /// pages. A keep-together group (a keep-next chain, or a single `keep_lines`
    /// paragraph) is moved whole when it fits a page but not the remaining space;
    /// a normal paragraph splits to fill the current page.
    fn run(&mut self, from: usize) {
        let fragments = self.galley;
        let mut i = from;
        while i < fragments.len() && self.halted.is_none() {
            self.at = FlowPos::at(i as u32);
            let mut j = i;
            while j < fragments.len() && fragments[j].break_control().keep_next {
                j += 1;
            }
            j = (j + 1).min(fragments.len());
            // A forced page break (`pageBreakBefore`) inside a keep-next chain wins
            // over keep-with-next (as in Word): end the group right before the first
            // later member that forces a new page, so its break is honored on the
            // next iteration as that group's head (`group[0]`, checked below).
            if let Some(k) = fragments[i..j]
                .iter()
                .enumerate()
                .skip(1)
                .find(|(_, f)| f.break_control().page_break_before)
                .map(|(k, _)| k)
            {
                j = i + k;
            }
            let group = &fragments[i..j];
            let group_height: i32 = group.iter().map(|f| f.height().raw()).sum();
            let group_fits_page = group_height <= self.content_height;
            let is_keep_group = group.len() > 1 || group[0].break_control().keep_lines;
            let is_vertical_merge_group = group.iter().any(Paginable::is_vertical_merge_row);

            let forced = group[0].break_control().page_break_before;
            let doesnt_fit_here = self.cursor_y.raw() + group_height > self.content_bottom;
            if !self.placed.is_empty()
                && (forced
                    || (is_keep_group && group_fits_page && doesnt_fit_here)
                    || (is_vertical_merge_group && doesnt_fit_here))
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
            let allow_split = !is_vertical_merge_group && (!is_keep_group || !group_fits_page);
            // A body merge group landing on a fresh continuation page also needs
            // the table's repeated headers. Include them when deciding whether
            // the explicit intact-overflow fallback is required.
            let repeated_header_height = if is_vertical_merge_group && self.placed.is_empty() {
                match group.first().and_then(Paginable::row_info) {
                    Some(row) if !row.header && self.current_table == Some(row.table) => self
                        .table_headers
                        .iter()
                        .map(|header| header.height().raw())
                        .sum(),
                    _ => 0,
                }
            } else {
                0
            };
            let target_height = group_height.saturating_add(repeated_header_height);
            let force_overflow = is_vertical_merge_group && target_height > self.content_height;
            for (offset, fragment) in group.iter().enumerate() {
                self.place(i + offset, fragment, allow_split, force_overflow);
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
            let page = F::build_page(
                self.page_index(),
                self.config,
                self.content,
                std::mem::take(&mut self.placed),
                flow,
            );
            self.pages.push(page);
            self.reset_page_content();
            self.cursor_y = self.content.origin.y;
            self.page_start = self.at;
            self.record_checkpoint();
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

    /// Records a [`Checkpoint`] at the page boundary just closed, when the
    /// interval falls due and the boundary is one [`run`](Self::run) can
    /// restart from.
    fn record_checkpoint(&mut self) {
        if self.checkpoint_interval == 0 {
            return;
        }
        let pages = self.page_index();
        if pages == 0
            || !pages.is_multiple_of(self.checkpoint_interval)
            || !self.is_resumable_boundary()
        {
            return;
        }
        self.checkpoints.push(Checkpoint {
            page_index: pages as u32,
            at: self.at,
            current_table: self.current_table,
            table_headers: self.table_header_indices.clone(),
        });
    }

    /// Whether [`run`](Self::run) can restart at [`at`](Self::at) and reproduce
    /// what the forward fill did from there.
    ///
    /// Three conditions, two of them the ones `safe_resume_page` applies to the
    /// incremental path:
    ///
    /// - **A whole-fragment boundary.** `run` always begins a fragment at its
    ///   first line, so it cannot express a resume part-way through one. That
    ///   rules out `line > 0` (a split paragraph) and, through `mid_fragment`,
    ///   a page closed inside a row being cut — whose recorded flow position
    ///   says `line == 0` but whose remaining content is the row's tail.
    /// - **A keep-group start.** `run` re-forms keep-with-next groups from
    ///   their head, so resuming inside one would hand the walk a *shorter*
    ///   group than the full pass saw, with a different `allow_split`. This
    ///   condition is deliberately **conservative**: it is not known that every
    ///   such resume diverges, only that the group the walk re-forms is not the
    ///   one the full pass split, and refusing the boundary costs at most a
    ///   checkpoint. Reasoning case by case about when it would happen to agree
    ///   is exactly the kind of subtlety `docs/113` §5 says not to rely on.
    /// - **Real downstream content.** A boundary at the end of the galley names
    ///   nothing to resume.
    fn is_resumable_boundary(&self) -> bool {
        let fragment = self.at.fragment as usize;
        !self.mid_fragment
            && self.at.line == 0
            && fragment < self.galley.len()
            && (fragment == 0 || !self.galley[fragment - 1].break_control().keep_next)
    }

    /// Remaining content height below the cursor.
    fn remaining(&self) -> i32 {
        self.content_bottom - self.cursor_y.raw()
    }

    /// Appends a placed fragment at the cursor and advances by `height`. Records
    /// the page's start flow position when this is the page's first content.
    fn push(&mut self, fragment: F, height: Twip) {
        if self.placed.is_empty() {
            self.page_start = self.at;
        }
        let rect = Rect::new(
            Point::new(self.content.origin.x, self.cursor_y),
            Size::new(self.content.size.width, height),
        );
        self.placed
            .push(fragment.into_placed(rect, self.config.section));
        self.cursor_y = self.cursor_y + height;
    }

    /// Places fragment `idx`, splitting a multi-line paragraph across pages when
    /// `allow_split` and it does not fit whole.
    fn place(&mut self, idx: usize, fragment: &F, allow_split: bool, force_overflow: bool) {
        self.at = FlowPos::at(idx as u32);
        let row = fragment.row_info();
        // Leaving a run of table rows ends the current table's header context.
        if row.is_none() {
            self.leave_table();
        }
        match row {
            Some(row) => {
                self.place_table_row(
                    idx,
                    fragment,
                    row.table,
                    row.can_split && allow_split,
                    row.header,
                    force_overflow,
                );
            }
            // A paragraph with a forced page/column break is always routed
            // through the line splitter (even under `keepLines`, a keep-group,
            // or when it is a *single* line — e.g. an empty paragraph carrying a
            // section break or a lone `w:br`) so the explicit break is honored;
            // the whole-placement branch below would drop the break.
            None if fragment.any_line_page_break_after()
                || (fragment.line_count() > 1
                    && allow_split
                    && !fragment.break_control().keep_lines) =>
            {
                self.place_paragraph(idx, fragment);
            }
            None => {
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
        fragment: &F,
        table: NodeId,
        can_split: bool,
        header: bool,
        force_overflow: bool,
    ) {
        self.enter_table(table);
        self.at = FlowPos::at(idx as u32);
        if force_overflow {
            self.repeat_headers_if_needed(idx, header);
            self.push(fragment.clone(), fragment.height());
            self.at = FlowPos::at(idx as u32 + 1);
            self.capture_header(idx, fragment, header);
            return;
        }
        // Header rows are never split — they move whole and repeat.
        let can_split = can_split && !header;
        let height = fragment.height();
        let fits = height.raw() <= self.remaining();

        // A splittable row that does not fit is broken across the boundary —
        // whether it is the page's first content (taller than a whole page) or
        // follows other content.
        if !fits && can_split {
            self.split_table_row(idx, fragment);
            self.capture_header(idx, fragment, header);
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
        self.capture_header(idx, fragment, header);
    }

    /// Splits a table row across a page boundary at block/line boundaries within
    /// its cells, mirroring the paragraph line-splitting path. Each chunk is a
    /// row fragment carrying the cells' content that fits; the remainder carries
    /// to the next page (with header rows repeated).
    fn split_table_row(&mut self, idx: usize, fragment: &F) {
        // Every page this cut closes ends *inside* fragment `idx`, and the flow
        // position recorded at such a boundary names the row rather than the
        // chunk within it (`repeat_headers_if_needed` deliberately re-anchors
        // `page_start` to the body row, so the page's provenance stays on the
        // real row). A boundary like that looks resumable — `line == 0` — and is
        // not: restarting `run` there would place the whole row again instead of
        // its tail. Mark the span so no checkpoint is taken inside it.
        self.mid_fragment = true;
        self.split_row_chunks(idx, fragment);
        self.mid_fragment = false;
    }

    /// The body of [`split_table_row`](Self::split_table_row); see its comment
    /// for why it is wrapped.
    fn split_row_chunks(&mut self, idx: usize, fragment: &F) {
        let Some(row) = fragment.row_info() else {
            return;
        };
        let is_header = row.header;
        let mut remaining: Vec<F::Cell> = fragment.cells().to_vec();
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
                    let h = cells_content_height(&remaining);
                    let row = fragment.row_chunk(remaining, h);
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
            let row = fragment.row_chunk(head, Twip(used));
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
            self.table_header_indices.clear();
        }
    }

    /// Ends the current table's header context (called when a non-row fragment
    /// interrupts the run of rows).
    fn leave_table(&mut self) {
        self.current_table = None;
        self.table_headers.clear();
        self.table_header_indices.clear();
    }

    /// Records a header row so it can be repeated on continuation pages. `idx`
    /// is the row's galley index, which is what a [`Checkpoint`] stores.
    fn capture_header(&mut self, idx: usize, fragment: &F, header: bool) {
        if header {
            self.table_headers.push(fragment.clone());
            self.table_header_indices.push(idx as u32);
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
        // Repeating the headers must leave room for the row they caption. A
        // header taller than the usable content area — a full-width logo in the
        // header cell is enough — otherwise consumed the whole fresh page, so the
        // body row could not be placed, which flushed the page, which started
        // another fresh page, which repeated the headers again. Pagination never
        // terminated: the tab froze and memory grew until it was killed.
        //
        // Word drops the repetition in exactly this situation rather than
        // looping, and so do we: a page that cannot also carry a body row is not
        // a continuation, it is a page of nothing but headers. Losing the
        // repeated caption on such a table is a visible but bounded fidelity
        // cost; the alternative is an unrecoverable hang.
        let header_total: i32 = self.table_headers.iter().map(|h| h.height().raw()).sum();
        if header_total >= self.remaining() {
            return;
        }
        self.page_start = FlowPos::at(idx as u32);
        for h in self.table_headers.clone() {
            let height = h.height();
            let rect = Rect::new(
                Point::new(self.content.origin.x, self.cursor_y),
                Size::new(self.content.size.width, height),
            );
            self.placed.push(h.into_placed(rect, self.config.section));
            self.cursor_y = self.cursor_y + height;
        }
    }

    /// Places paragraph `idx`'s lines, breaking across pages at line boundaries
    /// with widow/orphan control.
    fn place_paragraph(&mut self, idx: usize, fragment: &F) {
        let n = fragment.line_count();
        let widow = fragment.break_control().widow_control;
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
                fragment.space_before().raw()
            } else {
                0
            };
            let avail = self.remaining() - space_before;

            // Greedily count leading lines that fit.
            let mut take = 0;
            let mut used = 0;
            while start + take < n {
                let h = fragment.line_height(start + take).raw();
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
                    used = fragment.line_height(start).raw();
                } else {
                    self.flush();
                    continue;
                }
            }

            // A forced page/column break (`w:br` type page/column) caps this chunk:
            // include up to and through the break line, then flush unconditionally.
            // This is just another line-split point, so the incremental halt/prefix
            // bookkeeping (keyed on `{fragment, line}`) is unchanged.
            let forced = (0..take).find(|offset| fragment.line_page_break_after(start + offset));
            if let Some(k) = forced {
                let new_take = k + 1;
                used -= (new_take..take)
                    .map(|offset| fragment.line_height(start + offset).raw())
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
                used -= fragment.line_height(start + take).raw();
            }

            let is_tail = start + take == n;
            let space_after = if is_tail {
                fragment.space_after().raw()
            } else {
                0
            };
            let chunk = fragment.slice_paragraph(start..start + take);
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
            // Flush between the split chunks of this paragraph, and also after a
            // forced page/column break that fell on the paragraph's *final* line
            // (`start == n`): the break ends the page so the next fragment starts
            // fresh. Without this, a trailing break — a section break or a `w:br`
            // on the last line — would be silently dropped. A flush with nothing
            // more to place is a no-op, so a document-final break adds no blank page.
            if start < n || forced {
                self.flush();
            }
        }
    }
}

/// Builds a paragraph fragment for `lines[range]`, re-basing every paintable
/// line child so the first placed line sits at the fragment top, and keeping
/// `space_before`/`space_after` only on the head/tail chunk. Whether this slice is
/// the paragraph's head (starts at line 0) and/or tail (ends at the last line) is
/// derived from `range` — a slice covering the whole paragraph is both.
pub(crate) fn slice_paragraph(
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
            line.translate_contents_y(Twip(-y_offset));
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
pub(crate) fn make_row_chunk(
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
        merge_keep_next: false,
        clip: false,
    }
}

/// Splits a row's cells at a vertical cut `avail` twips below the row top,
/// returning the head cells (content that fits), the tail cells (the remainder,
/// preserving every column so the continuation row keeps its geometry), and the
/// head height actually used (the tallest content-bearing cell's fitted content
/// plus its cloned top/bottom margins). A `used` of 0 means nothing fit.
pub(crate) fn split_cells<C: PaginableCell>(cells: &[C], avail: i32) -> (Vec<C>, Vec<C>, i32) {
    let mut head = Vec::with_capacity(cells.len());
    let mut tail = Vec::with_capacity(cells.len());
    let mut used = 0;
    let mut has_tail = false;
    for cell in cells {
        let vertical_margins = cell.vertical_margins().raw();
        let content_avail = avail.saturating_sub(vertical_margins);
        let (head_blocks, tail_blocks, cell_used) = split_blocks(cell.blocks(), content_avail);
        if cell_used > 0 || !head_blocks.is_empty() {
            used = used.max(cell_used.saturating_add(vertical_margins));
        }
        head.push(cell.with_blocks(head_blocks));
        if !tail_blocks.is_empty() {
            has_tail = true;
        }
        tail.push(cell.with_blocks(tail_blocks));
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
fn split_blocks<F: Paginable>(blocks: &[F], avail: i32) -> (Vec<F>, Vec<F>, i32) {
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
        // A table row reports zero lines, so only a multi-line paragraph that is
        // not `keepLines` is ever cut here; anything else moves whole.
        if block.line_count() > 1 && !block.break_control().keep_lines {
            let (head_frag, tail_frag, used) = split_paragraph_at(block, avail - y);
            if let Some(head_frag) = head_frag {
                head.push(head_frag);
                y += used;
            }
            if let Some(tail_frag) = tail_frag {
                tail.push(tail_frag);
            }
        } else {
            tail.push(block.clone());
        }
        splitting = false;
    }
    (head, tail, y)
}

/// Splits one paragraph at a `avail`-twip vertical cut, greedily keeping the
/// leading lines that fit. Returns the head chunk (if any line fits), the tail
/// chunk (the rest), and the head height used.
fn split_paragraph_at<F: Paginable>(fragment: &F, avail: i32) -> (Option<F>, Option<F>, i32) {
    let n = fragment.line_count();
    let space_before = fragment.space_before().raw();
    let budget = avail - space_before;
    let mut take = 0;
    let mut used = 0;
    while take < n {
        let line_h = fragment.line_height(take).raw();
        if used + line_h > budget {
            break;
        }
        used += line_h;
        take += 1;
    }
    if take == 0 {
        return (None, Some(fragment.slice_paragraph(0..n)), 0);
    }
    let head = fragment.slice_paragraph(0..take);
    let tail = fragment.slice_paragraph(take..n);
    (Some(head), Some(tail), space_before + used)
}

/// Assembles one page from the fragments placed on it.
pub(crate) fn build_page(
    index: usize,
    config: &PageConfig,
    content: Rect,
    placed: Vec<PlacedFragment>,
    flow: FlowSpan,
) -> Page {
    let start = ModelPos::new(placed.first().unwrap().fragment.node_id(), 0);
    let end = ModelPos::new(placed.last().unwrap().fragment.node_id(), 0);
    let mut page = page_shell(index, config, content, flow, start);
    page.placed = placed;
    page.end = end;
    page
}

/// Assembles a page that carries **no** body content — the parity blank a
/// `w:type="evenPage"`/`"oddPage"` section break inserts when the page the next
/// section would otherwise open on has the wrong parity (`docs/105` FID-L-03).
///
/// Word still paints running content on that page, so it is a real page in every
/// other respect: it belongs to the *preceding* section (whose header/footer it
/// shows and whose page-number sequence it advances), and it collapses to a
/// single model position — `anchor`, the end of the page before it — so hit
/// testing and the model-range queries stay monotonic across it.
pub(crate) fn build_blank_page(
    index: usize,
    config: &PageConfig,
    content: Rect,
    flow: FlowSpan,
    anchor: ModelPos,
) -> Page {
    page_shell(index, config, content, flow, anchor)
}

/// The common page skeleton: everything both a content page and a parity blank
/// share, with the post-pagination bands/floats/borders left for their passes.
fn page_shell(
    index: usize,
    config: &PageConfig,
    content: Rect,
    flow: FlowSpan,
    at: ModelPos,
) -> Page {
    Page {
        number: (index + 1) as u32,
        section: config.section,
        page_size: config.page_size,
        content_area: content,
        placed: Vec::new(),
        header: Vec::new(),
        footer: Vec::new(),
        // Filled by the post-pagination anchored-placement pass, off the hot path.
        anchored: Vec::new(),
        footnotes: Vec::new(),
        // Filled by the column paginator for multi-column separator sections; the
        // single-column paginator leaves it empty.
        separators: Vec::new(),
        // Filled by the post-pagination page-border pass, off the hot path.
        page_borders: None,
        // Filled by the post-pagination line-number pass, off the hot path.
        line_numbers: Vec::new(),
        start: at,
        end: at,
        flow,
    }
}

/// Resolves the page-dependent fields (`PAGE`, `NUMPAGES`) in a **final** paginated
/// layout — the post-pagination field pass.
///
/// Pagination is field-value-free: [`paginate`]/[`repaginate`] place field runs
/// carrying only a marker (never a baked number), so a page the incremental
/// paginator reuses or splices carries no stale value. This pass is a **pure
/// function of the final page list**: it stamps each `PAGE` marker with its page's
/// [`number`](Page::number) and each `NUMPAGES` marker with the total page count,
/// then reshapes the field's glyph run in place. Because it depends only on the
/// final list — the same list a full [`paginate`] and an incremental
/// [`repaginate`] both produce — applying it to either yields identical layouts, so
/// `repaginate == paginate` still holds with fields resolved. It is idempotent
/// (running it twice is a no-op), so re-stamping a reused page is always safe.
///
/// Body content, headers, and footers are all resolved (a footer may hold a
/// `Page X of Y`). Call it after pagination and after
/// [`crate::running::place_running_content`] (so headers/footers exist to stamp).
///
/// Fields nested inside table cells and inline text boxes are resolved too (the
/// pass recurses into both). Fields inside *anchored* (floating) text boxes are
/// handled by [`resolve_anchored_fields`], which must run after
/// [`crate::anchor::place_floats`] has populated [`Page::anchored`].
pub fn resolve_fields(layout: &mut PaginatedLayout, shaper: &dyn LineShaper) {
    let labels = decimal_page_labels(layout);
    resolve_fields_labeled(layout, &labels, shaper);
}

/// [`resolve_fields`] with an explicit per-page `PAGE` label for each page (index
/// aligned to `layout.pages`), so the document driver can honor a section's
/// `w:pgNumType` format (`lowerRoman`/`upperLetter`/…) and `@start` restart
/// instead of the physical decimal index. A page whose label is missing falls
/// back to its physical `number`.
pub(crate) fn resolve_fields_labeled(
    layout: &mut PaginatedLayout,
    labels: &[String],
    shaper: &dyn LineShaper,
) {
    let total = layout.pages.len() as u32;
    resolve_fields_labeled_with_total(layout, labels, total, shaper);
}

/// [`resolve_fields_labeled`] for a layout that holds only **part** of the
/// document: `total` is the document's real page count, which `NUMPAGES` must
/// print.
///
/// `docs/113` §7's fourth unknown, answered: a windowed layout cannot take the
/// page count from its own `pages.len()`, and the measure tier is exactly what
/// supplies the right one.
pub(crate) fn resolve_fields_labeled_with_total(
    layout: &mut PaginatedLayout,
    labels: &[String],
    total: u32,
    shaper: &dyn LineShaper,
) {
    for (index, page) in layout.pages.iter_mut().enumerate() {
        let fallback = page.number.to_string();
        let label = labels.get(index).unwrap_or(&fallback);
        for placed in &mut page.placed {
            resolve_in_fragment(&mut placed.fragment, label, total, shaper);
        }
        for placed in &mut page.header {
            resolve_in_fragment(&mut placed.fragment, label, total, shaper);
        }
        for placed in &mut page.footer {
            resolve_in_fragment(&mut placed.fragment, label, total, shaper);
        }
    }
}

/// The physical-decimal `PAGE` label for every page — the no-`pgNumType` default.
fn decimal_page_labels(layout: &PaginatedLayout) -> Vec<String> {
    layout
        .pages
        .iter()
        .map(|page| page.number.to_string())
        .collect()
}

/// Computes each page's `PAGE` display label honoring section `w:pgNumType`: the
/// `@fmt` number format and the `@start` restart (a section carrying a start value
/// resets the running counter at its first page; a section without one continues
/// the count). NUMPAGES is unaffected (it stays the physical total).
pub(crate) fn page_number_labels(
    layout: &PaginatedLayout,
    sections: &[SectionBoundary],
) -> Vec<String> {
    page_number_labels_for(layout.pages.iter().map(|page| page.section), sections)
}

/// The `PAGE` label of the page at 0-based index `index` of a document with a
/// **single** section — computed directly rather than by walking every page
/// before it.
///
/// [`page_number_labels`] is a running fold over the whole page list, which a
/// window does not have and should not have to materialize (60,000 formatted
/// strings per scroll). With one section the fold is closed-form: the counter
/// starts at `w:pgNumType/@start` (or 1) and increments. The two are asserted
/// to agree in `page_number_label_at_matches_the_running_fold`.
#[must_use]
pub(crate) fn page_number_label_at(section: Option<&SectionBoundary>, index: usize) -> String {
    let numbering = section.map(|s| &s.page_numbering);
    let start = numbering
        .and_then(|n| n.start)
        .map_or(1, |value| value.max(0) as u32);
    let fmt = numbering
        .and_then(|n| n.format.as_ref())
        .map(page_number_format_token);
    format_page_number(start.saturating_add(index as u32), fmt)
}

/// The [`page_number_labels`] core over a page→section sequence, so the restart /
/// format logic is unit-testable without building full [`Page`] values.
fn page_number_labels_for(
    page_sections: impl Iterator<Item = SectionId>,
    sections: &[SectionBoundary],
) -> Vec<String> {
    let numbering = |id: SectionId| {
        sections
            .iter()
            .find(|s| s.id == id)
            .map(|s| &s.page_numbering)
    };
    let mut labels = Vec::new();
    let mut counter: u32 = 0;
    let mut prev_section: Option<SectionId> = None;
    for section in page_sections {
        let page_numbering = numbering(section);
        let start = page_numbering.and_then(|n| n.start);
        counter = if prev_section != Some(section) {
            // First page of this section: restart at `@start` (clamped to a
            // non-negative page number) or continue the running count.
            match start {
                Some(value) => value.max(0) as u32,
                None => counter + 1,
            }
        } else {
            counter + 1
        };
        prev_section = Some(section);
        let fmt = page_numbering
            .and_then(|n| n.format.as_ref())
            .map(page_number_format_token);
        labels.push(format_page_number(counter, fmt));
    }
    labels
}

/// The `ST_NumberFormat` token a typed [`NumberFormat`] renders as, so page-number
/// formatting can reuse the token-driven [`format_page_number`].
fn page_number_format_token(format: &NumberFormat) -> &str {
    match format {
        NumberFormat::Decimal => "decimal",
        NumberFormat::Bullet => "bullet",
        NumberFormat::LowerRoman => "lowerRoman",
        NumberFormat::UpperRoman => "upperRoman",
        NumberFormat::LowerLetter => "lowerLetter",
        NumberFormat::UpperLetter => "upperLetter",
        NumberFormat::Ordinal => "ordinal",
        NumberFormat::CardinalText => "cardinalText",
        NumberFormat::OrdinalText => "ordinalText",
        NumberFormat::DecimalZero => "decimalZero",
        NumberFormat::None => "none",
        NumberFormat::Other(value) => value.as_str(),
    }
}

/// Formats a page number per a `w:pgNumType/@w:fmt` token. Unknown/absent tokens
/// (and values outside a format's expressible range) fall back to decimal.
fn format_page_number(number: u32, fmt: Option<&str>) -> String {
    let formatted = match fmt {
        Some("lowerRoman") => to_roman(number).map(|s| s.to_lowercase()),
        Some("upperRoman") => to_roman(number),
        Some("lowerLetter") => to_letters(number),
        Some("upperLetter") => to_letters(number).map(|s| s.to_uppercase()),
        _ => None,
    };
    formatted.unwrap_or_else(|| number.to_string())
}

/// Classic Roman numeral for `1..=3999`; `None` outside that range.
fn to_roman(mut number: u32) -> Option<String> {
    if number == 0 || number >= 4000 {
        return None;
    }
    const TABLE: [(u32, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for (value, symbol) in TABLE {
        while number >= value {
            out.push_str(symbol);
            number -= value;
        }
    }
    Some(out)
}

/// Bijective base-26 letters (`1=a, 26=z, 27=aa, …`, Word's `lowerLetter`); `None`
/// for `0`.
fn to_letters(mut number: u32) -> Option<String> {
    if number == 0 {
        return None;
    }
    let mut buf = Vec::new();
    while number > 0 {
        let rem = ((number - 1) % 26) as u8;
        buf.push(b'a' + rem);
        number = (number - 1) / 26;
    }
    buf.reverse();
    // `buf` holds only ASCII `a..=z`, so this never fails.
    String::from_utf8(buf).ok()
}

/// Resolves `PAGE`/`NUMPAGES` fields inside anchored (floating) text boxes — the
/// footer/header page-number box the SDS corpus uses lives here, a positioned VML
/// `v:textbox` modeled as an anchored [`AnchorContent::TextBox`].
///
/// [`Page::anchored`] is populated by [`crate::anchor::place_floats`], which runs
/// *after* [`resolve_fields`]; this pass must therefore run last, after floats are
/// placed, so the box's `PAGE` field shows the current page instead of the cached
/// result baked into the model. Recursing into table cells and nested text boxes,
/// it is idempotent for the same reason [`resolve_fields`] is.
pub fn resolve_anchored_fields(layout: &mut PaginatedLayout, shaper: &dyn LineShaper) {
    let labels = decimal_page_labels(layout);
    resolve_anchored_fields_labeled(layout, &labels, shaper);
}

/// [`resolve_anchored_fields`] with explicit per-page `PAGE` labels (see
/// [`resolve_fields_labeled`]), so a floating text box's page-number field shows
/// the same `w:pgNumType`-formatted value as the body/footer.
pub(crate) fn resolve_anchored_fields_labeled(
    layout: &mut PaginatedLayout,
    labels: &[String],
    shaper: &dyn LineShaper,
) {
    let total = layout.pages.len() as u32;
    for (index, page) in layout.pages.iter_mut().enumerate() {
        let fallback = page.number.to_string();
        let label = labels.get(index).unwrap_or(&fallback);
        for anchor in &mut page.anchored {
            if let AnchorContent::TextBox { blocks, .. } = &mut anchor.content {
                for block in blocks {
                    resolve_in_fragment(block, label, total, shaper);
                }
            }
        }
    }
}

/// Resolves fields inside one block fragment, recursing into table cells and inline
/// text boxes (both carry block content flowed through the shared pipeline, which
/// may hold `PAGE`/`NUMPAGES` fields).
fn resolve_in_fragment(
    fragment: &mut BlockFragment,
    page_label: &str,
    total: u32,
    shaper: &dyn LineShaper,
) {
    match fragment {
        BlockFragment::Paragraph { lines, .. } => {
            for line in &mut lines.lines {
                resolve_in_line(line, page_label, total, shaper);
                for text_box in &mut line.text_boxes {
                    for block in &mut text_box.blocks {
                        resolve_in_fragment(block, page_label, total, shaper);
                    }
                }
            }
        }
        BlockFragment::TableRow { cells, .. } => {
            for cell in cells {
                for block in &mut cell.blocks {
                    resolve_in_fragment(block, page_label, total, shaper);
                }
            }
        }
    }
}

/// Stamps every field marker on `line` with its resolved value, reshapes the
/// field's glyph run, then repositions the runs from the first field onward so a
/// value whose width changed (e.g. `9` → `10`) keeps the following text contiguous.
/// The reposition seeds from each field's stored `base_x` (its flow anchor), so the
/// pass is idempotent.
fn resolve_in_line(line: &mut Line, page_label: &str, total: u32, shaper: &dyn LineShaper) {
    if line.fields.is_empty() {
        return;
    }
    // Phase 1: stamp values and reshape each field's glyph run in place.
    let field_runs: Vec<usize> = line.fields.iter().map(|f| f.run as usize).collect();
    for field in &mut line.fields {
        let idx = field.run as usize;
        if idx >= line.runs.len() {
            continue;
        }
        field.value = match field.kind {
            FieldKind::Page => page_label.to_string(),
            FieldKind::NumPages => total.to_string(),
            // Any other field displays its cached result verbatim.
            FieldKind::Passthrough => std::mem::take(&mut field.value),
        };
        let baseline = line.runs[idx].origin.y;
        let shape = shape_field_run(
            shaper,
            &field.value,
            field.style,
            Point::new(field.base_x, baseline),
        );
        line.runs[idx] = shape.run;
    }
    // Phase 2: reflow from the first field so trailing runs stay contiguous. Runs
    // before the first field are field-independent and keep their shaper positions.
    let Some(first) = field_runs.iter().copied().min() else {
        return;
    };
    let Some(start_x) = line
        .fields
        .iter()
        .filter(|f| f.run as usize == first)
        .map(|f| f.base_x)
        .next()
    else {
        return;
    };
    let mut pen = start_x;
    for idx in first..line.runs.len() {
        line.runs[idx].origin.x = pen;
        pen = pen + advance_of(&line.runs[idx]);
    }
}

/// The total advance of a glyph run (sum of its glyphs' advances).
fn advance_of(run: &GlyphRun) -> Twip {
    run.glyphs.iter().fold(Twip::ZERO, |a, g| a + g.advance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{BoxMetrics, BreakControl};
    use crate::model::{ModelPos, ModelRange};
    use crate::text::{
        Decoration, FontId, Glyph, GlyphRun, InlineImage, InlineRule, InlineTextBox, Line,
        LineBreak, LineLayout, TextBoxContentLayout,
    };
    use casual_doc_model::NodeId;

    #[test]
    fn page_number_formats_cover_roman_and_letters_with_decimal_fallback() {
        assert_eq!(format_page_number(1, Some("lowerRoman")), "i");
        assert_eq!(format_page_number(4, Some("lowerRoman")), "iv");
        assert_eq!(format_page_number(2024, Some("upperRoman")), "MMXXIV");
        assert_eq!(format_page_number(1, Some("lowerLetter")), "a");
        assert_eq!(format_page_number(27, Some("lowerLetter")), "aa");
        assert_eq!(format_page_number(28, Some("upperLetter")), "AB");
        assert_eq!(format_page_number(5, None), "5");
        // Unknown token and out-of-range roman fall back to decimal.
        assert_eq!(format_page_number(5, Some("cardinalText")), "5");
        assert_eq!(format_page_number(4000, Some("upperRoman")), "4000");
    }

    /// The windowed driver's closed-form page label must be the same string
    /// the driver's running fold produces — a window has no page list to fold
    /// over, so the two must be asserted equal rather than assumed.
    #[test]
    fn page_number_label_at_matches_the_running_fold() {
        for (fmt, start) in [
            (None, None),
            (Some("upperRoman"), Some(7)),
            (Some("lowerLetter"), None),
            (None, Some(0)),
            (Some("lowerRoman"), Some(1)),
        ] {
            let section = numbering_section(30, fmt, start);
            let id = section.id;
            let sections = vec![section];
            let folded = page_number_labels_for(std::iter::repeat_n(id, 300), &sections);
            for (index, expected) in folded.iter().enumerate() {
                assert_eq!(
                    &page_number_label_at(sections.first(), index),
                    expected,
                    "page {index} under fmt {fmt:?} start {start:?}"
                );
            }
        }
    }

    fn numbering_section(id: u64, fmt: Option<&str>, start: Option<i32>) -> SectionBoundary {
        use casual_doc_model::v1::{
            DocGrid, NoteProperties, NumberFormat, PageBorders, PageMargins, PageNumbering,
            PageSize, PaperSource, SectionColumns,
        };
        SectionBoundary {
            id: SectionId::new(NodeId::from_parts(id, 1).unwrap()),
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
            page_numbering: PageNumbering {
                format: fmt.map(|f| match f {
                    "lowerRoman" => NumberFormat::LowerRoman,
                    "upperRoman" => NumberFormat::UpperRoman,
                    "lowerLetter" => NumberFormat::LowerLetter,
                    "upperLetter" => NumberFormat::UpperLetter,
                    other => NumberFormat::Other(other.to_owned()),
                }),
                start,
            },
            doc_grid: DocGrid::default(),
            orientation: None,
            paper_source: PaperSource::default(),
            page_borders: PageBorders::default(),
            line_numbering: Default::default(),
            footnote_props: NoteProperties::default(),
            endnote_props: NoteProperties::default(),
            text_direction: None,
            bidi: false,
            section_change: None,
        }
    }

    #[test]
    fn page_labels_apply_section_format_and_start_restart() {
        let front = SectionId::new(NodeId::from_parts(10, 1).unwrap());
        let body = SectionId::new(NodeId::from_parts(11, 1).unwrap());
        let sections = [
            // Front matter: lower-roman, default start (i, ii).
            numbering_section(10, Some("lowerRoman"), None),
            // Body: decimal, restart at 1 (1, 2, 3) despite being physical page 3+.
            numbering_section(11, None, Some(1)),
        ];
        // Two front-matter pages then three body pages.
        let page_sections = [front, front, body, body, body];
        let labels = page_number_labels_for(page_sections.into_iter(), &sections);
        assert_eq!(labels, ["i", "ii", "1", "2", "3"]);
    }

    #[test]
    fn page_labels_continue_the_count_when_a_section_has_no_start() {
        let a = SectionId::new(NodeId::from_parts(20, 1).unwrap());
        let b = SectionId::new(NodeId::from_parts(21, 1).unwrap());
        // Neither section restarts: the running count carries across the boundary.
        let sections = [
            numbering_section(20, None, None),
            numbering_section(21, None, None),
        ];
        let labels = page_number_labels_for([a, a, b, b].into_iter(), &sections);
        assert_eq!(labels, ["1", "2", "3", "4"]);
    }

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
            header_distance: Twip(720),
            footer_distance: Twip(720),
            header_height: Twip::ZERO,
            footer_height: Twip::ZERO,
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
                        is_marker: false,
                        is_leader: false,
                        font: FontId(0),
                        size: line_h,
                        ascent: Twip(0),
                        descent: Twip(0),
                        character_scale_percent: 100,
                        color: [0, 0, 0, 255],
                        origin: Point::new(Twip::ZERO, baseline),
                        bidi_level: 0,
                        decoration: crate::text::Decoration::default(),
                        highlight: None,
                        shading: None,
                        glyphs: vec![Glyph {
                            id: 1,
                            advance: line_h,
                            cluster: 0,
                            is_whitespace: false,
                        }],
                    }],
                    ascent: line_h,
                    descent: Twip::ZERO,
                    clip: false,
                    height: line_h,
                    range: ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 0)),
                    line_break: LineBreak::Wrap,
                    page_break_after: false,
                    bars: Vec::new(),
                    images: Vec::new(),
                    fields: Vec::new(),
                    notes: Vec::new(),
                    text_boxes: Vec::new(),
                    rules: Vec::new(),
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
    fn paragraph_slices_rebase_every_paintable_line_child() {
        let node = tnode(900);
        let range = ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 1));
        let blank_line = |height| Line {
            runs: Vec::new(),
            ascent: height,
            descent: Twip::ZERO,
            height,
            clip: false,
            range,
            line_break: LineBreak::Wrap,
            page_break_after: false,
            bars: Vec::new(),
            images: Vec::new(),
            fields: Vec::new(),
            notes: Vec::new(),
            text_boxes: Vec::new(),
            rules: Vec::new(),
        };
        let mut second = blank_line(Twip(200));
        second.runs.push(GlyphRun {
            is_marker: false,
            is_leader: false,
            font: FontId(0),
            size: Twip(100),
            ascent: Twip(0),
            descent: Twip(0),
            character_scale_percent: 100,
            color: [0, 0, 0, 255],
            origin: Point::new(Twip(10), Twip(250)),
            bidi_level: 0,
            decoration: Decoration::default(),
            highlight: None,
            shading: None,
            glyphs: vec![Glyph {
                id: 1,
                advance: Twip(50),
                cluster: 0,
                is_whitespace: false,
            }],
        });
        second.images.push(InlineImage {
            opacity: None,
            media: "word/media/image1.png".into(),
            origin: Point::new(Twip(20), Twip(260)),
            size: Size::new(Twip(40), Twip(50)),
            crop: None,
        });
        second.text_boxes.push(InlineTextBox {
            origin: Point::new(Twip(30), Twip(270)),
            size: Size::new(Twip(60), Twip(70)),
            blocks: Vec::new(),
            border: None,
            fill: None,
            content_layout: TextBoxContentLayout::default(),
        });
        second.rules.push(InlineRule {
            origin: Point::new(Twip(40), Twip(280)),
            size: Size::new(Twip(80), Twip(10)),
            color: [0, 0, 0, 255],
        });
        let lines = LineLayout {
            lines: vec![blank_line(Twip(200)), second],
        };

        let sliced = slice_paragraph(
            node,
            &lines,
            BoxMetrics::default(),
            BreakControl::default(),
            ParagraphDecor::default(),
            1..2,
        );
        let BlockFragment::Paragraph { lines, .. } = sliced else {
            unreachable!()
        };
        let line = &lines.lines[0];
        assert_eq!(line.runs[0].origin.y, Twip(50));
        assert_eq!(line.images[0].origin.y, Twip(60));
        assert_eq!(line.text_boxes[0].origin.y, Twip(70));
        assert_eq!(line.rules[0].origin.y, Twip(80));
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
    fn page_break_before_wins_over_keep_next() {
        let config = letter_config();
        // para1 keeps-with-next (like a Heading), para2 forces a break. The break
        // wins over keep-with-next (as in Word): para1 stays on page 1, para2 on
        // page 2 — the keep-next chain is split at the forced break.
        let fragments = vec![
            paragraph_with(1, Twip(240), with(false, true)),
            paragraph_with(2, Twip(240), with(true, false)),
        ];
        let layout = paginate(&fragments, &config);
        assert_eq!(
            layout.page_count(),
            2,
            "pageBreakBefore breaks the keep-next chain"
        );
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

    use crate::block::{
        CellBorders, CellContentMargins, CellFragment, CellVAlign, CellVerticalMerge,
    };

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
            cell_spacing: Default::default(),
            blocks,
            margins: CellContentMargins::default(),
            vertical_alignment: CellVAlign::default(),
            vertical_merge: CellVerticalMerge::None,
            borders: CellBorders::default(),
            table_borders: CellBorders::default(),
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
            merge_keep_next: false,
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
    fn a_vertical_merge_group_moves_whole_when_only_part_fits() {
        let config = letter_config();
        let content_h = config.content_area().size.height.raw();
        let filler = paragraph(1, Twip(content_h - 700));
        let mut restart = table_row(
            2,
            500,
            vec![cell_of(3, vec![paragraph(4, Twip(500))])],
            true,
            false,
        );
        let BlockFragment::TableRow {
            merge_keep_next, ..
        } = &mut restart
        else {
            unreachable!()
        };
        *merge_keep_next = true;
        let continuation = table_row(
            5,
            500,
            vec![cell_of(6, vec![paragraph(7, Twip(500))])],
            true,
            false,
        );

        let layout = paginate(&[filler, restart, continuation], &config);
        assert_eq!(layout.page_count(), 2);
        assert_eq!(
            layout.pages[0].placed.len(),
            1,
            "neither half of the merge remains below the filler"
        );
        assert_eq!(
            layout.pages[1].placed.len(),
            2,
            "both physical rows move together"
        );
        assert_eq!(layout.pages[1].placed[0].fragment.node_id(), tnode(2));
        assert_eq!(layout.pages[1].placed[1].fragment.node_id(), tnode(5));
    }

    #[test]
    fn a_moved_vertical_merge_group_reserves_its_repeated_table_header() {
        let config = letter_config();
        let content_h = config.content_area().size.height.raw();
        let header = table_row(
            1,
            500,
            vec![cell_of(2, vec![paragraph(3, Twip(300))])],
            false,
            true,
        );
        let body = table_row(
            4,
            500,
            vec![cell_of(5, vec![paragraph(6, Twip(content_h - 1000))])],
            false,
            false,
        );
        let mut restart = table_row(
            7,
            500,
            vec![cell_of(8, vec![paragraph(9, Twip(600))])],
            true,
            false,
        );
        let BlockFragment::TableRow {
            merge_keep_next, ..
        } = &mut restart
        else {
            unreachable!()
        };
        *merge_keep_next = true;
        let continuation = table_row(
            10,
            500,
            vec![cell_of(11, vec![paragraph(12, Twip(600))])],
            true,
            false,
        );

        let layout = paginate(&[header, body, restart, continuation], &config);
        assert_eq!(layout.page_count(), 2);
        assert_eq!(
            layout.pages[1].placed.len(),
            3,
            "the repeated header and both merge rows share page two"
        );
        assert_eq!(layout.pages[1].placed[0].fragment.node_id(), tnode(1));
        assert_eq!(layout.pages[1].placed[1].fragment.node_id(), tnode(7));
        assert_eq!(layout.pages[1].placed[2].fragment.node_id(), tnode(10));
        assert!(
            layout.pages[1].placed[2].rect.bottom().raw() <= config.content_area().bottom().raw()
        );
    }

    #[test]
    fn an_oversized_vertical_merge_group_overflows_intact_without_looping() {
        let config = letter_config();
        let content_h = config.content_area().size.height.raw();
        let mut restart = table_row(
            2,
            500,
            vec![cell_of(3, vec![paragraph(4, Twip(content_h / 2 + 500))])],
            true,
            false,
        );
        let BlockFragment::TableRow {
            merge_keep_next, ..
        } = &mut restart
        else {
            unreachable!()
        };
        *merge_keep_next = true;
        let continuation = table_row(
            5,
            500,
            vec![cell_of(6, vec![paragraph(7, Twip(content_h / 2 + 500))])],
            true,
            false,
        );

        let layout = paginate(&[restart, continuation], &config);
        assert_eq!(layout.page_count(), 1);
        assert_eq!(layout.pages[0].placed.len(), 2);
        assert!(
            layout.pages[0].placed[1].rect.bottom().raw() > config.content_area().bottom().raw(),
            "the explicit fallback is one intact overflowing merged box"
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

    /// A repeated table header taller than the usable content area used to hang
    /// pagination outright: the header consumed the whole fresh page, the body
    /// row could not be placed, the page flushed, a new page began, and the
    /// header was repeated again — forever. The tab froze and memory grew until
    /// it was killed, on a document Word opens fine (a full-width logo in the
    /// header cell is enough to reach this).
    ///
    /// Word drops the repetition rather than looping, and so do we.
    #[test]
    fn an_oversized_repeated_header_does_not_hang_pagination() {
        let config = letter_config();
        let content_h = config.content_area().size.height.raw();

        // A header row TALLER than the whole content area.
        let header = table_row(
            10,
            5,
            vec![cell_of(11, vec![paragraph(12, Twip(content_h + 1000))])],
            false,
            true,
        );
        let mut frags = vec![header];
        for i in 0..3u64 {
            frags.push(table_row(
                100 + i,
                5,
                vec![cell_of(200 + i, vec![paragraph(300 + i, Twip(1000))])],
                true,
                false,
            ));
        }

        // The assertion that matters is that this call RETURNS at all. The bound
        // then proves it did not merely terminate by accident after thousands of
        // header-only pages.
        let layout = paginate(&frags, &config);
        assert!(
            layout.page_count() <= 8,
            "pagination must stay bounded, got {} pages",
            layout.page_count()
        );
        // Every body row still reaches a page: dropping the repeated caption must
        // never drop content.
        let placed_body_rows: usize = layout
            .pages
            .iter()
            .flat_map(|page| page.placed.iter())
            .filter(|p| matches!(p.fragment, BlockFragment::TableRow { header: false, .. }))
            .count();
        assert_eq!(placed_body_rows, 3, "no body row is lost");
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

    #[test]
    fn a_margined_split_cell_cannot_overpaint_successor_rows() {
        let config = letter_config();
        let mut tall_paragraph = multiline(22, 120, Twip(240), BreakControl::default());
        if let BlockFragment::Paragraph { lines, .. } = &mut tall_paragraph {
            lines.lines[70].images.push(InlineImage {
                opacity: None,
                media: "word/media/continuation.png".into(),
                origin: Point::new(Twip(20), Twip(70 * 240 + 20)),
                size: Size::new(Twip(80), Twip(80)),
                crop: None,
            });
        }
        let mut tall_cell = cell_of(21, vec![tall_paragraph]);
        tall_cell.margins.top = Twip(120);
        tall_cell.margins.bottom = Twip(180);
        let row = table_row(20, 5, vec![tall_cell], true, false);
        let next = table_row(
            23,
            5,
            vec![cell_of(24, vec![paragraph(25, Twip(400))])],
            true,
            false,
        );
        let after_next = table_row(
            26,
            5,
            vec![cell_of(27, vec![paragraph(28, Twip(400))])],
            true,
            false,
        );

        let layout = paginate(&[row, next, after_next], &config);
        assert!(layout.page_count() >= 3);
        for page in &layout.pages {
            for pair in page.placed.windows(2) {
                assert!(
                    pair[0].rect.bottom().raw() <= pair[1].rect.origin.y.raw(),
                    "successor rows must start below the preceding row chunk"
                );
            }
            for placed in &page.placed {
                let BlockFragment::TableRow { cells, height, .. } = &placed.fragment else {
                    continue;
                };
                for cell in cells.iter().filter(|cell| !cell.blocks.is_empty()) {
                    assert!(
                        cell.occupied_height().raw() <= height.raw(),
                        "the row chunk must contain its content and cloned margins"
                    );
                    for block in &cell.blocks {
                        let BlockFragment::Paragraph { lines, .. } = block else {
                            continue;
                        };
                        for image in lines.lines.iter().flat_map(|line| &line.images) {
                            assert!(
                                image.origin.y.raw() + image.size.height.raw()
                                    <= lines.height().raw(),
                                "a continuation image must stay in its paragraph slice"
                            );
                        }
                    }
                }
            }
        }
        let placed_ids: Vec<_> = layout
            .pages
            .iter()
            .flat_map(|page| &page.placed)
            .map(|placed| placed.fragment.node_id())
            .collect();
        assert!(placed_ids.contains(&tnode(23)));
        assert!(placed_ids.contains(&tnode(26)));
    }
}
