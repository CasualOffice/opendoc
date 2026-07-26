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
use crate::page::{Page, PaginatedLayout, PlacedFragment};
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
    let content = config.content_area();
    let mut p = Paginator {
        config,
        content,
        content_bottom: content.bottom().raw(),
        content_height: content.size.height.raw(),
        pages: Vec::new(),
        placed: Vec::new(),
        cursor_y: content.origin.y,
    };

    // Walk keep-with-next groups: a maximal run where every fragment but the last
    // has `keep_next`. A keep-together group (a keep-next chain, or a single
    // `keep_lines` paragraph) is moved whole when it fits a page but not the
    // remaining space; a normal paragraph splits to fill the current page.
    let mut i = 0;
    while i < fragments.len() {
        let mut j = i;
        while j < fragments.len() && fragments[j].break_control().keep_next {
            j += 1;
        }
        j = (j + 1).min(fragments.len());
        let group = &fragments[i..j];
        let group_height: i32 = group.iter().map(|f| f.height().raw()).sum();
        let group_fits_page = group_height <= p.content_height;
        let is_keep_group = group.len() > 1 || group[0].break_control().keep_lines;

        let forced = group[0].break_control().page_break_before;
        let doesnt_fit_here = p.cursor_y.raw() + group_height > p.content_bottom;
        if !p.placed.is_empty() && (forced || (is_keep_group && group_fits_page && doesnt_fit_here))
        {
            p.flush();
        }

        // A keep-together group that fits a page is placed atomically; otherwise
        // (a normal paragraph, or an oversized keep group) paragraphs may split.
        let allow_split = !is_keep_group || !group_fits_page;
        for fragment in group {
            p.place(fragment, allow_split);
        }
        i = j;
    }
    p.flush();

    PaginatedLayout { pages: p.pages }
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
}

impl Paginator<'_> {
    /// Ends the current page (if it has content) and resets the cursor to the top.
    fn flush(&mut self) {
        if !self.placed.is_empty() {
            let page = build_page(
                self.pages.len(),
                self.config,
                self.content,
                std::mem::take(&mut self.placed),
            );
            self.pages.push(page);
            self.cursor_y = self.content.origin.y;
        }
    }

    /// Remaining content height below the cursor.
    fn remaining(&self) -> i32 {
        self.content_bottom - self.cursor_y.raw()
    }

    /// Appends a placed fragment at the cursor and advances by `height`.
    fn push(&mut self, fragment: BlockFragment, height: Twip) {
        let rect = Rect::new(
            Point::new(self.content.origin.x, self.cursor_y),
            Size::new(self.content.size.width, height),
        );
        self.placed.push(PlacedFragment { fragment, rect });
        self.cursor_y = self.cursor_y + height;
    }

    /// Places a fragment, splitting a multi-line paragraph across pages when
    /// `allow_split` and it does not fit whole.
    fn place(&mut self, fragment: &BlockFragment, allow_split: bool) {
        match fragment {
            BlockFragment::Paragraph {
                id,
                lines,
                box_metrics,
                break_control,
            } if allow_split && !break_control.keep_lines && lines.lines.len() > 1 => {
                self.place_paragraph(*id, lines, *box_metrics, *break_control);
            }
            _ => {
                let height = fragment.height();
                if !self.placed.is_empty() && height.raw() > self.remaining() {
                    self.flush();
                }
                self.push(fragment.clone(), height);
            }
        }
    }

    /// Places a paragraph's lines, breaking across pages at line boundaries with
    /// widow/orphan control.
    fn place_paragraph(
        &mut self,
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
}
