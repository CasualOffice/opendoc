//! Page borders (`w:pgBorders`) resolution — the paint half of the P1F-36
//! model/round-trip work (docs/46 §F6c, docs/55 §12).
//!
//! A section's page border is resolved to a per-page frame by a post-pagination
//! pass (mirroring the running-content and column-separator seams): it reads the
//! page's section border, applies the `display` policy against the page's
//! *section-local* number, computes the frame rectangle from `offsetFrom`, and
//! resolves each edge through the shared [`crate::flow::resolve_edge`]. The
//! result is stored on [`crate::page::Page::page_borders`] and painted on top of
//! the body by [`crate::compose::compose_page`]. Off the pagination hot path, so
//! page reuse (the stabilization halt) stays position-free.

use casual_doc_model::v1::{BorderEdge, PageBorderDisplay, PageBorderOffset, PageBorders};

use crate::flow::resolve_edge;
use crate::page::ResolvedPageBorders;
use crate::units::{Point, Rect, Size, Twip};

/// Points → twips (`w:space` is in points, `0..=31`).
fn space_twips(edge: Option<&BorderEdge>) -> i32 {
    edge.and_then(|e| e.space_points).unwrap_or(0) as i32 * 20
}

/// Resolves a section's `w:pgBorders` for one page, or `None` when the border is
/// absent, suppressed by its `display` policy on this page, has no visible edge,
/// or collapses to a degenerate frame.
///
/// `section_page_number` is the 1-based page number *within the section* — the
/// `firstPage`/`notFirstPage` policy is section-relative (title-page semantics).
pub(crate) fn resolve_page_borders(
    borders: &PageBorders,
    section_page_number: u32,
    page_size: Size,
    content_area: Rect,
) -> Option<ResolvedPageBorders> {
    let shown = match borders.display.unwrap_or(PageBorderDisplay::AllPages) {
        PageBorderDisplay::AllPages => true,
        PageBorderDisplay::FirstPage => section_page_number == 1,
        PageBorderDisplay::NotFirstPage => section_page_number != 1,
    };
    if !shown {
        return None;
    }

    // A `nil`/`none` edge resolves to `None` (suppressed), so a declared border
    // with only such edges paints nothing.
    let top = resolve_edge(&[borders.top.as_ref()]);
    let bottom = resolve_edge(&[borders.bottom.as_ref()]);
    let start = resolve_edge(&[borders.start.as_ref()]);
    let end = resolve_edge(&[borders.end.as_ref()]);
    if top.is_none() && bottom.is_none() && start.is_none() && end.is_none() {
        return None;
    }

    // `offsetFrom=page` measures the space *inward* from each page edge;
    // `offsetFrom=text` measures it *outward* from the text extent.
    let (reference, sign) = match borders.offset_from.unwrap_or(PageBorderOffset::Page) {
        PageBorderOffset::Page => (Rect::new(Point::new(Twip::ZERO, Twip::ZERO), page_size), 1),
        PageBorderOffset::Text => (content_area, -1),
    };
    let left = reference.origin.x.raw() + sign * space_twips(borders.start.as_ref());
    let top_y = reference.origin.y.raw() + sign * space_twips(borders.top.as_ref());
    let right = reference.right().raw() - sign * space_twips(borders.end.as_ref());
    let bottom_y = reference.bottom().raw() - sign * space_twips(borders.bottom.as_ref());
    if right <= left || bottom_y <= top_y {
        return None;
    }
    let rect = Rect::new(
        Point::new(Twip(left), Twip(top_y)),
        Size::new(Twip(right - left), Twip(bottom_y - top_y)),
    );
    Some(ResolvedPageBorders {
        rect,
        top,
        bottom,
        start,
        end,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use casual_doc_model::v1::RgbColor;

    fn edge(size: u32, space: u32) -> BorderEdge {
        BorderEdge {
            style: "single".to_owned(),
            size_eighth_points: Some(size),
            color: Some(RgbColor { r: 0, g: 0, b: 0 }),
            space_points: Some(space),
        }
    }

    fn all_edges(
        display: Option<PageBorderDisplay>,
        offset: Option<PageBorderOffset>,
    ) -> PageBorders {
        PageBorders {
            display,
            offset_from: offset,
            top: Some(edge(24, 24)),
            bottom: Some(edge(24, 24)),
            start: Some(edge(24, 24)),
            end: Some(edge(24, 24)),
        }
    }

    // An 8.5×11in page (12240×15840 twips) with 1in margins (content 1440..10800 ×
    // 1440..14400) — the common Letter geometry.
    fn page_size() -> Size {
        Size::new(Twip(12240), Twip(15840))
    }
    fn content_area() -> Rect {
        Rect::new(
            Point::new(Twip(1440), Twip(1440)),
            Size::new(Twip(9360), Twip(12960)),
        )
    }

    #[test]
    fn all_pages_offset_from_page_insets_the_frame_from_the_page_edge() {
        let resolved = resolve_page_borders(
            &all_edges(
                Some(PageBorderDisplay::AllPages),
                Some(PageBorderOffset::Page),
            ),
            1,
            page_size(),
            content_area(),
        )
        .expect("a border on every page");
        // 24pt space → 480 twips inset from each page edge.
        assert_eq!(resolved.rect.origin.x, Twip(480));
        assert_eq!(resolved.rect.origin.y, Twip(480));
        assert_eq!(resolved.rect.right(), Twip(12240 - 480));
        assert_eq!(resolved.rect.bottom(), Twip(15840 - 480));
        assert!(resolved.top.is_some() && resolved.bottom.is_some());
        assert!(resolved.start.is_some() && resolved.end.is_some());
    }

    #[test]
    fn offset_from_text_tracks_the_content_area_outward() {
        let resolved = resolve_page_borders(
            &all_edges(None, Some(PageBorderOffset::Text)),
            1,
            page_size(),
            content_area(),
        )
        .expect("a border measured from text");
        // 24pt (480 twips) outside the content edges.
        assert_eq!(resolved.rect.origin.x, Twip(1440 - 480));
        assert_eq!(resolved.rect.right(), Twip(10800 + 480));
    }

    #[test]
    fn first_page_policy_shows_only_on_the_sections_first_page() {
        let borders = all_edges(
            Some(PageBorderDisplay::FirstPage),
            Some(PageBorderOffset::Page),
        );
        assert!(resolve_page_borders(&borders, 1, page_size(), content_area()).is_some());
        assert!(resolve_page_borders(&borders, 2, page_size(), content_area()).is_none());
    }

    #[test]
    fn not_first_page_policy_is_absent_on_the_first_page() {
        let borders = all_edges(
            Some(PageBorderDisplay::NotFirstPage),
            Some(PageBorderOffset::Page),
        );
        assert!(resolve_page_borders(&borders, 1, page_size(), content_area()).is_none());
        assert!(resolve_page_borders(&borders, 2, page_size(), content_area()).is_some());
    }

    #[test]
    fn a_none_edge_is_suppressed_and_an_all_none_border_resolves_to_nothing() {
        let mut borders = all_edges(None, None);
        borders.top = Some(BorderEdge {
            style: "none".to_owned(),
            size_eighth_points: None,
            color: None,
            space_points: None,
        });
        let resolved =
            resolve_page_borders(&borders, 1, page_size(), content_area()).expect("others remain");
        assert!(resolved.top.is_none(), "the none edge is suppressed");
        assert!(resolved.bottom.is_some());

        let empty = PageBorders::default();
        assert!(resolve_page_borders(&empty, 1, page_size(), content_area()).is_none());
    }
}
