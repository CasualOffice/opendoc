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

use casual_doc_model::v1::SectionId;

use crate::block::BlockFragment;
use crate::model::ModelPos;
use crate::page::{Page, PaginatedLayout, PlacedFragment};
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

/// Paginates a galley of block fragments into pages under one section geometry.
///
/// A fragment is placed on the current page if it fits in the remaining content
/// height; otherwise a new page begins. A fragment taller than the whole content
/// area is placed alone (it overflows the page rather than looping) — line-level
/// splitting arrives in `P1D-002`.
#[must_use]
pub fn paginate(fragments: &[BlockFragment], config: &PageConfig) -> PaginatedLayout {
    let content = config.content_area();
    let content_bottom = content.bottom().raw();
    let content_height = content.size.height.raw();

    let mut pages = Vec::new();
    let mut placed: Vec<PlacedFragment> = Vec::new();
    let mut cursor_y = content.origin.y;

    let flush = |pages: &mut Vec<Page>, placed: &mut Vec<PlacedFragment>, cursor_y: &mut _| {
        if !placed.is_empty() {
            pages.push(build_page(
                pages.len(),
                config,
                content,
                std::mem::take(placed),
            ));
            *cursor_y = content.origin.y;
        }
    };

    // Walk keep-with-next groups: a maximal run of fragments where every fragment
    // but the last has `keep_next`. The whole group is kept on one page when it
    // fits a full content area; a group taller than a page degrades to a greedy
    // split so pagination always terminates (`43-…` §7).
    let mut i = 0;
    while i < fragments.len() {
        let mut j = i;
        while j < fragments.len() && fragments[j].break_control().keep_next {
            j += 1;
        }
        j = (j + 1).min(fragments.len());
        let group = &fragments[i..j];
        let group_height: i32 = group.iter().map(|f| f.height().raw()).sum();
        let group_fits_page = group_height <= content_height;

        // Forced break before the group, or a keep-together group that fits a page
        // but not the remaining space, starts a new page.
        let forced = group[0].break_control().page_break_before;
        let doesnt_fit_here = cursor_y.raw() + group_height > content_bottom;
        if !placed.is_empty() && (forced || (group_fits_page && doesnt_fit_here)) {
            flush(&mut pages, &mut placed, &mut cursor_y);
        }

        for fragment in group {
            let height = fragment.height();
            // Inside a page-sized-or-smaller group we never break; only an
            // oversized group (or a lone oversized fragment) splits greedily.
            if !group_fits_page
                && !placed.is_empty()
                && cursor_y.raw() + height.raw() > content_bottom
            {
                flush(&mut pages, &mut placed, &mut cursor_y);
            }
            let rect = Rect::new(
                Point::new(content.origin.x, cursor_y),
                Size::new(content.size.width, height),
            );
            placed.push(PlacedFragment {
                fragment: fragment.clone(),
                rect,
            });
            cursor_y = cursor_y + height;
        }
        i = j;
    }
    flush(&mut pages, &mut placed, &mut cursor_y);

    PaginatedLayout { pages }
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
