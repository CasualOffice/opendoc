//! Pagination — slicing a galley of block fragments into fixed-size pages.
//!
//! The block/flow engine produces a galley (a `Vec<BlockFragment>` in flow
//! order, in twips); this paginator places them into [`Page`]s whose content
//! area comes from the section's page box and margins. This is the single-section
//! paginator (`P1D-001`): fragments are placed atomically (a paragraph or row
//! goes wholly on a page, moving to the next when it does not fit); line-level
//! splitting of a tall paragraph, break control (`keepNext`/widow-orphan), tables
//! across pages, and footnotes are the following slices (`P1D-002…`, `43-…` §7).
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
    let content_bottom = content.bottom();

    let mut pages = Vec::new();
    let mut placed: Vec<PlacedFragment> = Vec::new();
    let mut cursor_y = content.origin.y;

    for fragment in fragments {
        let height = fragment.height();
        // Start a new page when the fragment does not fit and the current page is
        // not empty (an oversized fragment on an empty page is placed as overflow).
        if !placed.is_empty() && (cursor_y + height).raw() > content_bottom.raw() {
            pages.push(build_page(
                pages.len(),
                config,
                content,
                std::mem::take(&mut placed),
            ));
            cursor_y = content.origin.y;
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
    if !placed.is_empty() {
        pages.push(build_page(pages.len(), config, content, placed));
    }

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
    use crate::block::BoxMetrics;
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
}
