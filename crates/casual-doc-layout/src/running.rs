//! Running content — placing a section's headers and footers on each page.
//!
//! A section carries up to three header variants and three footer variants
//! (default / first-page / even-page); which one a given page shows depends on the
//! page **number**, the section's `w:titlePg`, and the document's
//! `w:evenAndOddHeaders` setting. Because a page's number can change when the
//! incremental paginator reuses or splices it (a spliced tail page is renumbered),
//! the *selection* — like `PAGE`/`NUMPAGES` field values — must be a **pure
//! function of the final page list**, not baked during pagination. So this is a
//! post-pagination pass ([`place_running_content`]): it reads each page's final
//! number and writes the selected, placed header/footer fragments into
//! [`Page::header`]/[`Page::footer`].
//!
//! The band geometry is reserved up front in the [`PageConfig`] (the body content
//! area shrinks by the header/footer band heights, computed once per section from
//! the tallest variant), so every page in the section shares one content area —
//! which is what keeps the incremental paginator's page reuse valid.

use crate::block::BlockFragment;
use crate::page::{Page, PaginatedLayout, PlacedFragment};
use crate::paginate::PageConfig;
use crate::units::{Point, Rect, Size, Twip};

/// One section's three header (or footer) variants, each a flowed galley (see
/// [`crate::flow::flow_header_footer`]). A missing variant is an empty `Vec` and
/// falls back to `default` at selection time.
#[derive(Clone, Debug, Default)]
pub struct HeaderFooter {
    /// The default header/footer (`w:headerReference` / `w:footerReference` of type
    /// `default`), shown on every page without a more specific variant.
    pub default: Vec<BlockFragment>,
    /// The first-page variant (type `first`), shown on page 1 when the section sets
    /// `w:titlePg`.
    pub first: Vec<BlockFragment>,
    /// The even-page variant (type `even`), shown on even pages when the document
    /// sets `w:evenAndOddHeaders`.
    pub even: Vec<BlockFragment>,
}

impl HeaderFooter {
    /// The reserved band height: the tallest stacked variant, so the band fits
    /// whichever variant a page selects (keeping the body content area uniform).
    #[must_use]
    pub fn band_height(&self) -> Twip {
        let stacked = |frags: &[BlockFragment]| {
            frags
                .iter()
                .map(BlockFragment::height)
                .fold(Twip::ZERO, |a, h| a + h)
        };
        stacked(&self.default)
            .max(stacked(&self.first))
            .max(stacked(&self.even))
    }

    /// The variant a page of the given `number` shows, given the section's
    /// `title_page` (`w:titlePg`) and the document's `even_and_odd` setting. A
    /// first-page header wins on page 1; otherwise an even-page header shows on even
    /// pages when even/odd is on; otherwise the default. A selected variant that is
    /// empty falls back to the default (Word's behavior when a reference is absent).
    #[must_use]
    fn select(&self, number: u32, title_page: bool, even_and_odd: bool) -> &[BlockFragment] {
        if number == 1 && title_page && !self.first.is_empty() {
            return &self.first;
        }
        if even_and_odd && number.is_multiple_of(2) && !self.even.is_empty() {
            return &self.even;
        }
        &self.default
    }
}

/// A section's running content: its header/footer variant sets and the two flags
/// that drive per-page selection.
#[derive(Clone, Debug, Default)]
pub struct RunningContent {
    /// The header variants.
    pub header: HeaderFooter,
    /// The footer variants.
    pub footer: HeaderFooter,
    /// The section uses a distinct first-page header/footer (`w:titlePg`).
    pub title_page: bool,
    /// The document distinguishes even and odd headers/footers
    /// (`w:evenAndOddHeaders`).
    pub even_and_odd: bool,
}

impl RunningContent {
    /// The `(header, footer)` band heights to reserve in the [`PageConfig`] — each
    /// the tallest of that band's variants. Feed these into
    /// `PageConfig::header_height`/`footer_height` before paginating so the body
    /// content area is reserved correctly.
    #[must_use]
    pub fn band_heights(&self) -> (Twip, Twip) {
        (self.header.band_height(), self.footer.band_height())
    }
}

/// Selects and places each page's header and footer — the post-pagination running
/// -content pass. For every page it picks the header/footer variant for the page's
/// final number (so a reused or renumbered page always shows the right one) and
/// lays the chosen fragments into the header/footer band from
/// [`PageConfig::header_band`]/[`PageConfig::footer_band`].
///
/// This is a pure function of the final page list and the section content, so it
/// applies identically after a full [`crate::paginate::paginate`] and an
/// incremental [`crate::paginate::repaginate`] — `repaginate == paginate` still
/// holds. Run it before [`crate::paginate::resolve_fields`], which resolves any
/// `PAGE`/`NUMPAGES` fields the placed header/footer contains.
pub fn place_running_content(
    layout: &mut PaginatedLayout,
    content: &RunningContent,
    config: &PageConfig,
) {
    let header_band = config.header_band();
    let footer_band = config.footer_band();
    for page in &mut layout.pages {
        place_page(page, content, header_band, footer_band);
    }
}

/// The multi-section running-content pass: each page is routed to *its own*
/// section's header/footer set and band geometry, matched by
/// [`Page::section`](crate::page::Page::section). Pass one
/// `(RunningContent, PageConfig)` per section (the config supplies the section id
/// via [`PageConfig::section`] and the band rects). A page whose section is not in
/// the list is left without running content.
///
/// Like [`place_running_content`] this is a pure function of the final page list,
/// so it composes with the incremental paginator; run it before
/// [`crate::paginate::resolve_fields`].
pub fn place_running_content_sections(
    layout: &mut PaginatedLayout,
    sections: &[(RunningContent, PageConfig)],
) {
    let bands: std::collections::HashMap<_, _> = sections
        .iter()
        .map(|(content, config)| {
            (
                config.section,
                (content, config.header_band(), config.footer_band()),
            )
        })
        .collect();
    for page in &mut layout.pages {
        if let Some((content, header_band, footer_band)) = bands.get(&page.section) {
            place_page(page, content, *header_band, *footer_band);
        }
    }
}

/// Places one page's header and footer bands.
fn place_page(page: &mut Page, content: &RunningContent, header_band: Rect, footer_band: Rect) {
    let n = page.number;
    let header = content
        .header
        .select(n, content.title_page, content.even_and_odd);
    let footer = content
        .footer
        .select(n, content.title_page, content.even_and_odd);
    page.header = stack_in_band(header, header_band);
    page.footer = stack_in_band(footer, footer_band);
}

/// Stacks `fragments` from the top of `band`, each at the band's leading edge and
/// full width, advancing by each fragment's height. Content taller than the band
/// overflows downward (Word grows the band; the fixed reservation is a documented
/// simplification), never clipped here.
fn stack_in_band(fragments: &[BlockFragment], band: Rect) -> Vec<PlacedFragment> {
    let mut placed = Vec::with_capacity(fragments.len());
    let mut y = band.origin.y;
    for fragment in fragments {
        let height = fragment.height();
        let rect = Rect::new(
            Point::new(band.origin.x, y),
            Size::new(band.size.width, height),
        );
        placed.push(PlacedFragment {
            fragment: fragment.clone(),
            rect,
        });
        y = y + height;
    }
    placed
}
