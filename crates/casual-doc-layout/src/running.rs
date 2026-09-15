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
//! the tallest *selectable* variant), so every page in the section shares one
//! content area — which is what keeps the incremental paginator's page reuse
//! valid. See [`HeaderFooter::band_height`] for where that reservation is and is
//! not Word's behavior.
//!
//! Which section's variants a page draws from is decided upstream, in
//! [`crate::document_layout`]: every page carries an immutable
//! [`Page::section`], link-to-previous inheritance is resolved per variant before
//! flow, and the section-local page number drives `titlePg`. The full page ×
//! variant decision matrix — and the two rules where this engine deliberately
//! differs from Word — is on [`HeaderFooter::select`].

use crate::block::BlockFragment;
use crate::page::{Page, PaginatedLayout, PlacedFragment};
use crate::paginate::PageConfig;
use crate::units::{Point, Rect, Size, Twip};

/// One section's three header (or footer) variants, each a flowed galley (see
/// [`crate::flow::flow_header_footer`]).
///
/// The variants here are already **post-inheritance**: OOXML's "link to previous"
/// is the *absence* of a `w:headerReference`/`w:footerReference` of a given type,
/// and [`crate::document_layout`] resolves that per variant, transitively back
/// through the earlier sections, before flowing anything. So an empty `Vec` here
/// means "no reference of this type anywhere in this section's inheritance chain,
/// or a reference to a part with no blocks" — both of which render as a **blank**
/// band, never as a fall-back to `default` (ECMA-376 §17.10.5).
#[derive(Clone, Debug, Default)]
pub struct HeaderFooter {
    /// The default header/footer (`w:headerReference` / `w:footerReference` of type
    /// `default`) — the *odd*-page variant in ECMA-376's wording, shown on every
    /// page that does not select a more specific variant.
    pub default: Vec<BlockFragment>,
    /// The first-page variant (type `first`), shown on the section's first page
    /// when the section sets `w:titlePg`, and on no other page.
    pub first: Vec<BlockFragment>,
    /// The even-page variant (type `even`), shown on even-numbered pages when the
    /// document sets `w:evenAndOddHeaders`, and on no other page.
    pub even: Vec<BlockFragment>,
}

impl HeaderFooter {
    /// The band height to reserve for a section: the tallest variant this section
    /// can actually **select**, so the band fits whatever any of its pages shows
    /// while every page keeps one uniform body content area.
    ///
    /// `title_page` (`w:titlePg`) and `even_and_odd` (`w:evenAndOddHeaders`) gate
    /// which variants are reachable at all: with `w:titlePg` off no page can ever
    /// show [`Self::first`], and without `w:evenAndOddHeaders` no page can ever
    /// show [`Self::even`] (see [`Self::select`]). An unreachable variant must not
    /// steal body area — a `header2.xml` left in the package by an earlier edit is
    /// otherwise enough to push every page's text down.
    ///
    /// **Deliberately not Word, and why.** Word's band is per *page*: the body
    /// starts at `max(topMargin, headerDistance + that page's header height)`, so a
    /// tall `first`-page header pushes the text down on the section's first page
    /// only. Here the tallest reachable variant is reserved across the whole
    /// section, so a section whose remaining pages show a short `default` still
    /// loses that body area. The reservation is what makes one content area per
    /// section — and therefore the incremental paginator's page reuse — valid;
    /// making it per page would move the reservation inside
    /// [`crate::paginate`]. The visible difference is confined to sections whose
    /// tallest reachable variant overflows `topMargin - headerDistance`
    /// (`footerDistance` for footers); below that, `PageConfig::content_area`
    /// clamps to the margin and the two models agree exactly.
    #[must_use]
    pub fn band_height(&self, title_page: bool, even_and_odd: bool) -> Twip {
        let stacked = |frags: &[BlockFragment]| {
            frags
                .iter()
                .map(BlockFragment::height)
                .fold(Twip::ZERO, |a, h| a + h)
        };
        let mut height = stacked(&self.default);
        if title_page {
            height = height.max(stacked(&self.first));
        }
        if even_and_odd {
            height = height.max(stacked(&self.even));
        }
        height
    }

    /// The variant one page shows — the whole page × variant decision.
    ///
    /// `number` is the page's final **document** page number (1-based, so page 1 is
    /// odd), `is_section_first` marks the first page of the section that owns the
    /// page, `title_page` is that section's `w:titlePg`, and `even_and_odd` is the
    /// document-level `w:settings/w:evenAndOddHeaders`.
    ///
    /// | `titlePg` | `evenAndOddHeaders` | section's first page | other odd page | other even page |
    /// |---|---|---|---|---|
    /// | off | off     | `default` | `default` | `default` |
    /// | off | **on**  | `default` on odd, `even` on even | `default` | `even` |
    /// | **on** | off  | `first`   | `default` | `default` |
    /// | **on** | **on** | `first` (wins even on an even page) | `default` | `even` |
    ///
    /// Read with the inheritance note on [`HeaderFooter`], that is ECMA-376
    /// §17.10.5 (`headerReference`/`footerReference`) and §17.10.6 (`titlePg`):
    ///
    /// 1. A variant that is switched off is **never** selected. Without
    ///    `w:titlePg`, "no first page header shall be shown, and the odd page
    ///    header shall be used in its place"; without `w:evenAndOddHeaders`, the
    ///    same for the even page header.
    /// 2. A variant that is switched **on** is selected even when it is empty — it
    ///    then paints a blank band. `w:titlePg` with no `first` reference anywhere
    ///    in the inheritance chain "shall create a new blank header", *not* fall
    ///    back to `default`; the same holds for `w:evenAndOddHeaders` with no
    ///    `even` reference. This is why the emptiness of a variant is not consulted
    ///    here.
    /// 3. `first` outranks `even`: the section's first page shows `first` even when
    ///    it is an even-numbered document page.
    ///
    /// **Deliberately not Word (two places).**
    ///
    /// - *Which page counts as "even"*: the physical document page number is used,
    ///   so a section that restarts numbering with `w:pgNumType/@w:start` (or
    ///   formats it as `i`, `ii`, `iii`) can show the `even` variant on a page
    ///   whose printed number is odd. Word's own tie between `w:pgNumType` and
    ///   odd/even band selection is not established here, so the physical number —
    ///   which is what ECMA-376's "the first page of the document is an odd page"
    ///   describes — is used and recorded rather than guessed at.
    /// - *Band height*: see [`Self::band_height`].
    #[must_use]
    pub fn select(
        &self,
        number: u32,
        is_section_first: bool,
        title_page: bool,
        even_and_odd: bool,
    ) -> &[BlockFragment] {
        if is_section_first && title_page {
            return &self.first;
        }
        if even_and_odd && number.is_multiple_of(2) {
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
    /// the tallest variant this section can actually select, under its own
    /// `w:titlePg` and the document's `w:evenAndOddHeaders`. Feed these into
    /// `PageConfig::header_height`/`footer_height` before paginating so the body
    /// content area is reserved correctly.
    #[must_use]
    pub fn band_heights(&self) -> (Twip, Twip) {
        (
            self.header.band_height(self.title_page, self.even_and_odd),
            self.footer.band_height(self.title_page, self.even_and_odd),
        )
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
///
/// This entry point is the **single-section** one: it treats document page 1 as
/// the section's first page, which is only correct when one section owns the whole
/// layout. A multi-section document must go through
/// [`crate::document_layout::paginate_document`], which places each page against
/// the plan of the section that owns it and counts that section's own pages, so
/// `w:titlePg` fires on every section's first page.
pub fn place_running_content(
    layout: &mut PaginatedLayout,
    content: &RunningContent,
    config: &PageConfig,
) {
    let header_band = config.header_band();
    let footer_band = config.footer_band();
    for page in &mut layout.pages {
        place_page(page, content, header_band, footer_band, page.number == 1);
    }
}

/// Places one page from a section-aware driver. `section_page_number` is local to
/// the section that owns the page (1 for its first physical page), which is what
/// makes `w:titlePg` section-local; odd/even selection still uses the page's final
/// document page number, because `w:evenAndOddHeaders` is a document-level flag
/// about page parity.
///
/// `config` must be the owning section's geometry: a page inherits the *section's*
/// page size, margins and header/footer distances, so a landscape section's band
/// is laid at the landscape text width even when the content it shows was
/// inherited from a portrait section.
pub(crate) fn place_running_content_on_page(
    page: &mut Page,
    content: &RunningContent,
    config: &PageConfig,
    section_page_number: u32,
) {
    place_page(
        page,
        content,
        config.header_band(),
        config.footer_band(),
        section_page_number == 1,
    );
}

/// Places one page's header and footer bands.
fn place_page(
    page: &mut Page,
    content: &RunningContent,
    header_band: Rect,
    footer_band: Rect,
    is_section_first: bool,
) {
    let n = page.number;
    let header = content.header.select(
        n,
        is_section_first,
        content.title_page,
        content.even_and_odd,
    );
    let footer = content.footer.select(
        n,
        is_section_first,
        content.title_page,
        content.even_and_odd,
    );
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
            section: None,
        });
        y = y + height;
    }
    placed
}
