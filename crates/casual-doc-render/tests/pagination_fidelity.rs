//! Pagination fidelity — the page count and body geometry of a real-shaped
//! DOCX, end to end (package → import → section-aware pagination).
//!
//! This guards a regression that cost a customer document two extra pages and a
//! near-empty page 3. The engine read a `sectPr` carrying
//! `<w:docGrid w:linePitch="299"/>` with **no** `w:type` as an active line grid
//! and rounded every body line up to the pitch; separately, it reserved a header
//! band on a document that has no header part, because `w:pgMar/@w:header` is
//! written regardless and was larger than the top margin. Both inflate content
//! height silently, and both only show up as a page count.
//!
//! `fixtures/generated/pagination-fidelity.docx` reproduces that shape in 2.6 KB
//! (see `generate_fixtures.rs` for the arithmetic behind its counts):
//!
//! - `<w:docGrid w:linePitch="299"/>` with no `w:type`;
//! - a footer part and **no** header part, `header="737"` against `top="567"`;
//! - two sections whose break must **not** paginate (the successor section is
//!   `continuous`), the second inheriting the first's footer;
//! - twenty atomic table rows inside the flow.
//!
//! Its body is exactly 126 lines of 240 twips against a 15,234-twip body area —
//! two full pages, with one line of slack. Re-enable either defect and the same
//! 126 lines need three pages, so [`the page count`](pagination_matches_word)
//! is driven red by either one on its own.

use casual_doc_import::ImportConfig;
use casual_doc_import::ImportMode;
use casual_doc_import::import_package;
use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::page::Page;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::units::Twip;
use casual_doc_model::v1::SectionId;
use casual_doc_ooxml::DocxPackage;
use casual_doc_ooxml::PackageLimits;

const PAGINATION_FIDELITY_DOCX: &[u8] =
    include_bytes!("../../../fixtures/generated/pagination-fidelity.docx");

/// The fixture's page geometry, from its `w:pgSz` / `w:pgMar`.
const PAGE_HEIGHT: Twip = Twip(16_838);
const MARGIN_TOP: Twip = Twip(567);
const FOOTER_DISTANCE: Twip = Twip(737);
/// The footer part's single `lineRule="exact"` paragraph.
const FOOTER_BAND: Twip = Twip(300);
/// The authored `w:line` on every body paragraph (`lineRule="atLeast"`), which
/// no installed font can undercut and only an active document grid can exceed.
const BODY_LINE: Twip = Twip(240);

fn layout() -> Vec<Page> {
    let mut package = DocxPackage::open(PAGINATION_FIDELITY_DOCX, PackageLimits::default())
        .expect("the pagination fidelity fixture opens");
    let imported = import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .expect("the pagination fidelity fixture imports");
    paginate_document(&imported.document, &ParleyShaper::new()).pages
}

/// Every line height in a page's body fragments, table cells included.
fn body_line_heights(page: &Page) -> Vec<Twip> {
    let mut heights = Vec::new();
    for placed in &page.placed {
        match &placed.fragment {
            BlockFragment::Paragraph { lines, .. } => {
                heights.extend(lines.lines.iter().map(|line| line.height));
            }
            BlockFragment::TableRow { cells, .. } => {
                for cell in cells {
                    for fragment in &cell.blocks {
                        if let BlockFragment::Paragraph { lines, .. } = fragment {
                            heights.extend(lines.lines.iter().map(|line| line.height));
                        }
                    }
                }
            }
        }
    }
    heights
}

/// The page count Word and LibreOffice both produce for this shape.
///
/// This is the assertion the two defects move. It is deliberately a *count* and
/// not a geometry snapshot: a count is what the owner sees, it is what both
/// oracles agree on, and it cannot be re-blessed into agreement.
#[test]
fn pagination_matches_word() {
    let pages = layout();
    assert_eq!(
        pages.len(),
        2,
        "126 body lines of 240 twips fit exactly two 15,234-twip pages; \
         got {} pages with per-page content heights {:?}",
        pages.len(),
        pages
            .iter()
            .map(|page| page.content_area.size.height.raw())
            .collect::<Vec<_>>()
    );
}

/// The body starts at the top margin when the document has **no header part**,
/// and still ends above the footer band, which is real.
///
/// `w:pgMar/@w:header` (737) is larger than `w:pgMar/@w:top` (567) here, which
/// is the common shape — Word writes the header distance whether or not a header
/// exists. Taking `max(top, header + height)` unconditionally therefore charges
/// every page for a band that is not there.
#[test]
fn a_missing_header_costs_the_body_nothing_and_a_present_footer_still_costs_it() {
    let pages = layout();
    for page in &pages {
        assert_eq!(
            page.content_area.origin.y, MARGIN_TOP,
            "page {} starts the body below the top margin although the document \
             has no header part",
            page.number
        );
        assert_eq!(
            page.content_area.origin.y + page.content_area.size.height,
            PAGE_HEIGHT - FOOTER_DISTANCE - FOOTER_BAND,
            "page {} does not stop at the top of the real footer band",
            page.number
        );
    }
}

/// A `w:docGrid` with a positive `w:linePitch` and **no** `w:type` is not a line
/// grid: the authored 240-twip lines must stay 240, not round up to 299.
#[test]
fn an_untyped_document_grid_does_not_snap_lines() {
    let pages = layout();
    let heights: Vec<Twip> = pages.iter().flat_map(body_line_heights).collect();
    assert!(!heights.is_empty(), "the fixture has body lines to measure");
    for height in heights {
        assert_eq!(
            height, BODY_LINE,
            "a line was resized away from its authored `atLeast` height, which \
             an untyped `w:docGrid` must not do"
        );
    }
}

/// The section break must not paginate: its successor section is `continuous`,
/// so both sections share the first page.
#[test]
fn a_continuous_successor_section_keeps_flowing_on_the_same_page() {
    let pages = layout();
    let first = pages.first().expect("at least one page");
    let sections: Vec<SectionId> = first
        .placed
        .iter()
        .filter_map(|placed| placed.section)
        .collect();
    let distinct = sections
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    assert_eq!(
        distinct, 2,
        "page 1 should carry both sections' content across a continuous break, \
         but carries {distinct}"
    );
}

/// A section that declares no `w:footerReference` inherits the previous
/// section's, so the footer is on every page and the header band is empty.
#[test]
fn the_inherited_footer_is_on_every_page() {
    let pages = layout();
    for page in &pages {
        assert!(
            !page.footer.is_empty(),
            "page {} lost the inherited footer",
            page.number
        );
        assert!(
            page.header.is_empty(),
            "page {} invented a header the package does not contain",
            page.number
        );
    }
}
