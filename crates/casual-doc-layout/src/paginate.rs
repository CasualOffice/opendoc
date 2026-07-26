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

use casual_doc_model::NodeId;
use casual_doc_model::v1::SectionId;

use crate::block::{BlockFragment, BoxMetrics, BreakControl};
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
    let mut p = Paginator::new(config, Vec::new(), FlowPos::at(0));
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
/// near the end of a document nearly free; the *stabilization halt* that also
/// reuses the unchanged tail (bounding edits near the top) is the next slice.
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
        return paginate(new_galley, config);
    };
    let resume_index = prev.pages[resume_page].flow.start.fragment as usize;

    let prefix: Vec<Page> = prev.pages[..resume_page].to_vec();
    let mut p = Paginator::new(config, prefix, FlowPos::at(resume_index as u32));
    p.run(new_galley, resume_index);
    p.flush();
    PaginatedLayout { pages: p.pages }
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
}

impl<'a> Paginator<'a> {
    /// Creates a paginator seeded with already-emitted `pages` and resuming at
    /// flow position `at` (page-top cursor). For a full run pass an empty prefix
    /// and `FlowPos::at(0)`.
    fn new(config: &'a PageConfig, pages: Vec<Page>, at: FlowPos) -> Self {
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
        }
    }

    /// Walks keep-with-next groups of `fragments[from..]`, placing them into
    /// pages. A keep-together group (a keep-next chain, or a single `keep_lines`
    /// paragraph) is moved whole when it fits a page but not the remaining space;
    /// a normal paragraph splits to fill the current page.
    fn run(&mut self, fragments: &[BlockFragment], from: usize) {
        let mut i = from;
        while i < fragments.len() {
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

            // A keep-together group that fits a page is placed atomically;
            // otherwise (a normal paragraph, or an oversized keep group) paragraphs
            // may split.
            let allow_split = !is_keep_group || !group_fits_page;
            for (offset, fragment) in group.iter().enumerate() {
                self.place(i + offset, fragment, allow_split);
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
        match fragment {
            BlockFragment::Paragraph {
                id,
                lines,
                box_metrics,
                break_control,
            } if allow_split && !break_control.keep_lines && lines.lines.len() > 1 => {
                self.place_paragraph(idx, *id, lines, *box_metrics, *break_control);
            }
            _ => {
                let height = fragment.height();
                if !self.placed.is_empty() && height.raw() > self.remaining() {
                    self.flush();
                }
                self.push(fragment.clone(), height);
                self.at = FlowPos::at(idx as u32 + 1);
            }
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
    ) {
        let n = lines.lines.len();
        let widow = break_control.widow_control;
        let mut start = 0;
        let mut is_head = true;
        while start < n {
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

            // Orphan: don't strand fewer than the minimum head lines at a page
            // foot — move the whole paragraph to the next page (only when the page
            // already has content, else we would loop).
            if widow
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
            if widow && start + take < n && (n - start - take) < MIN_WIDOW_ORPHAN && take > 1 {
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
                start..start + take,
                is_head,
                is_tail,
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

/// Builds a paragraph fragment for `lines[start..end]`, re-basing each line's run
/// origins so the first placed line sits at the fragment top, and keeping
/// `space_before`/`space_after` only on the head/tail chunk.
fn slice_paragraph(
    id: NodeId,
    lines: &LineLayout,
    box_metrics: BoxMetrics,
    break_control: BreakControl,
    range: core::ops::Range<usize>,
    is_head: bool,
    is_tail: bool,
) -> BlockFragment {
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
    }
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
        };
        BlockFragment::Paragraph {
            id: node,
            lines: LineLayout { lines: vec![line] },
            box_metrics: BoxMetrics::default(),
            break_control,
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
                }
            })
            .collect();
        BlockFragment::Paragraph {
            id: node,
            lines: LineLayout { lines },
            box_metrics: BoxMetrics::default(),
            break_control,
        }
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

    /// Asserts the incremental result equals a full re-paginate, and returns how
    /// many leading pages were reused (0 = nothing reusable).
    fn golden(prev: &[BlockFragment], new: &[BlockFragment], config: &PageConfig) -> usize {
        let prev_layout = paginate(prev, config);
        let inc = repaginate(&prev_layout, prev, new, config);
        let full = paginate(new, config);
        assert_eq!(
            inc, full,
            "incremental re-pagination must equal a full paginate"
        );
        let first_dirty = new
            .iter()
            .zip(prev.iter())
            .position(|(a, b)| a != b)
            .unwrap_or(new.len().min(prev.len()));
        safe_resume_page(&prev_layout, prev, first_dirty, config).unwrap_or(0)
    }

    #[test]
    fn incremental_equals_full_for_an_edit_in_the_middle() {
        let config = letter_config();
        // ~10 pages of single-line paragraphs; grow one paragraph in the middle.
        let prev = galley(300, Twip(400));
        let mut new = prev.clone();
        new[150] = paragraph(151, Twip(1_200)); // taller -> shifts everything below
        let reused = golden(&prev, &new, &config);
        assert!(reused > 0, "an edit mid-document reuses the pages above it");
    }

    #[test]
    fn incremental_reuses_almost_everything_for_an_edit_near_the_end() {
        let config = letter_config();
        let prev = galley(300, Twip(400));
        let mut new = prev.clone();
        new[299] = paragraph(300, Twip(1_200));
        let reused = golden(&prev, &new, &config);
        let full = paginate(&new, &config);
        // Only the last page (or two, if the edit pushed a page) is re-flowed.
        assert!(
            reused >= full.page_count().saturating_sub(2),
            "editing the last paragraph reuses all but the final page(s): reused {reused}, pages {}",
            full.page_count()
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
        let reused = golden(&prev, &new, &config);
        assert!(reused > 0, "appending reuses the whole document above");
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
}
