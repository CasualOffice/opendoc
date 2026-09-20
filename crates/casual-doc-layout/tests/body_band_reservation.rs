//! The body content area only loses space to a header/footer band that exists.
//!
//! `w:pgMar/@w:header` and `@w:footer` are written on every section whether or
//! not the package contains a header or footer part, and Word's default for both
//! is 720 twips — larger than plenty of real top/bottom margins. A body rule of
//! `max(margin, distance + height)` that does not first ask whether the band is
//! there therefore reserves space for nothing, on every page, for the whole
//! document. Seen on a customer A4 form with `top="567" header="737"` and no
//! header part at all: 170 twips a page, invisible except as content that
//! paginates too early.
//!
//! The companion end-to-end guard is
//! `casual-doc-render/tests/pagination_fidelity.rs`, which pins the same rule
//! through a real package. This file pins the geometry itself, in both
//! directions, so the fix cannot be over-applied either: a band that *is* there
//! and does reach past the margin must still push the body.

use casual_doc_layout::paginate::PageConfig;
use casual_doc_layout::units::Size;
use casual_doc_layout::units::Twip;
use casual_doc_model::NodeId;
use casual_doc_model::v1::SectionId;

/// A4 with the customer document's margins: both the header and the footer
/// distance exceed their margin, so an unguarded `max` reserves both bands.
fn config(header_height: Twip, footer_height: Twip) -> PageConfig {
    PageConfig {
        section: SectionId::new(NodeId::from_parts(9, 1).unwrap()),
        page_size: Size::new(Twip(11_906), Twip(16_838)),
        margin_top: Twip(567),
        margin_bottom: Twip(278),
        margin_start: Twip(709),
        margin_end: Twip(709),
        header_distance: Twip(737),
        footer_distance: Twip(737),
        header_height,
        footer_height,
    }
}

fn body_top(config: &PageConfig) -> Twip {
    config.content_area().origin.y
}

fn body_bottom(config: &PageConfig) -> Twip {
    let area = config.content_area();
    area.origin.y + area.size.height
}

#[test]
fn a_section_with_no_header_starts_the_body_at_the_top_margin() {
    let config = config(Twip::ZERO, Twip::ZERO);
    assert_eq!(
        body_top(&config),
        Twip(567),
        "no header part exists, so `w:pgMar/@w:header` must not move the body"
    );
}

#[test]
fn a_section_with_no_footer_ends_the_body_at_the_bottom_margin() {
    let config = config(Twip::ZERO, Twip::ZERO);
    assert_eq!(
        body_bottom(&config),
        Twip(16_838 - 278),
        "no footer part exists, so `w:pgMar/@w:footer` must not move the body"
    );
}

#[test]
fn a_band_that_reaches_past_its_margin_still_moves_the_body() {
    // 737 + 200 = 937 > 567, and 737 + 300 = 1037 > 278.
    let config = config(Twip(200), Twip(300));
    assert_eq!(
        body_top(&config),
        Twip(937),
        "a real header band below the top margin must push the body down"
    );
    assert_eq!(
        body_bottom(&config),
        Twip(16_838 - 1_037),
        "a real footer band above the bottom margin must pull the body up"
    );
}

#[test]
fn a_band_that_fits_inside_its_margin_costs_the_body_nothing() {
    // A tall top margin swallows the band: 1_000 > 737 + 100.
    let mut config = config(Twip(100), Twip(100));
    config.margin_top = Twip(1_000);
    config.margin_bottom = Twip(1_000);
    assert_eq!(body_top(&config), Twip(1_000));
    assert_eq!(body_bottom(&config), Twip(16_838 - 1_000));
}
