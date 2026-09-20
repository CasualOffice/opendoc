//! Caret routing for table cells whose clickable area carries no text, end to
//! end (package → import → section-aware pagination → `hit_test`).
//!
//! This guards a data-safety defect the owner hit on a real loan agreement: on
//! the document's last page, clicking a form's **empty value box** and typing
//! put the characters into the **label cell beside it**. The user clicked one
//! box and the text appeared in another — silently, with nothing refused and
//! nothing reported.
//!
//! The mechanism is not specific to empty cells. A cell's border box is much
//! larger than its text — the `w:trHeight` / `w:vAlign` slack above and below
//! the content, the blank area around a picture, the whole of an empty cell —
//! and hit-testing reasoned only about LINES. The vertical-band rule cannot see
//! a cell whose content does not cover the pointer's `y`, and the nearest-line
//! fallback treated the cell as a tie-breaker while dropping text-free lines
//! from the candidate set outright. Between them, a click inside one cell
//! resolved into a different one.
//!
//! The invariant this file pins is the general one:
//!
//! > A click inside a table cell resolves to a position **in that cell**.
//!
//! `fixtures/generated/cell-hit-routing.docx` puts both shapes on **page 2**,
//! reached through an explicit page break — every other browser-level spec in
//! this repository operates inside the first viewport and near the document
//! start, which is exactly why this survived.

use casual_doc_import::ImportConfig;
use casual_doc_import::ImportMode;
use casual_doc_import::import_package;
use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::block::CellFragment;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::hittest::LayoutSnapshot;
use casual_doc_layout::page::PaginatedLayout;
use casual_doc_layout::page::PlacedFragment;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::units::Point;
use casual_doc_layout::units::Twip;
use casual_doc_model::NodeId;
use casual_doc_ooxml::DocxPackage;
use casual_doc_ooxml::PackageLimits;

const CELL_HIT_ROUTING_DOCX: &[u8] =
    include_bytes!("../../../fixtures/generated/cell-hit-routing.docx");

/// The page the fixture's table sits on (1-based). The whole point of the
/// fixture is that it is not page 1.
const TABLE_PAGE: u32 = 2;

fn layout() -> PaginatedLayout {
    let mut package = DocxPackage::open(CELL_HIT_ROUTING_DOCX, PackageLimits::default())
        .expect("the cell-hit-routing fixture opens");
    let imported = import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .expect("the cell-hit-routing fixture imports");
    paginate_document(&imported.document, &ParleyShaper::new())
}

/// The placed table rows on `page`, in flow order.
fn rows(pages: &PaginatedLayout, page: u32) -> Vec<&PlacedFragment> {
    pages
        .pages
        .iter()
        .find(|p| p.number == page)
        .expect("the fixture has that page")
        .placed
        .iter()
        .filter(|placed| matches!(placed.fragment, BlockFragment::TableRow { .. }))
        .collect()
}

fn cells(placed: &PlacedFragment) -> &[CellFragment] {
    match &placed.fragment {
        BlockFragment::TableRow { cells, .. } => cells,
        BlockFragment::Paragraph { .. } => unreachable!("filtered to rows"),
    }
}

/// The id of the first paragraph directly inside `cell` — the node a caret
/// placed in that cell must address.
fn paragraph_of(cell: &CellFragment) -> NodeId {
    cell.blocks
        .iter()
        .find_map(|block| match block {
            BlockFragment::Paragraph { id, .. } => Some(*id),
            BlockFragment::TableRow { .. } => None,
        })
        .expect("every cell in the fixture holds a paragraph")
}

/// Clicks `down` twips into `row`, horizontally centred in cell `index`, and
/// answers the model position the caret landed on.
fn click_in_cell(
    pages: &PaginatedLayout,
    row: &PlacedFragment,
    index: usize,
    down: i32,
) -> (NodeId, u32) {
    let cell = &cells(row)[index];
    let x = row.rect.origin.x + cell.x + Twip(cell.width.raw() / 2);
    let y = Twip(row.rect.origin.y.raw() + down);
    let hit = LayoutSnapshot::new(pages)
        .hit_test(TABLE_PAGE, Point::new(x, y))
        .expect("a click inside the table resolves");
    (hit.pos.node, hit.pos.offset)
}

/// The paragraph the caret landed in.
fn node_at(pages: &PaginatedLayout, row: &PlacedFragment, index: usize, down: i32) -> NodeId {
    click_in_cell(pages, row, index, down).0
}

/// The fixture must actually put its table on a later page, or the guard is
/// testing the first viewport again.
#[test]
fn the_fixture_puts_its_table_past_the_first_page() {
    let pages = layout();
    assert!(pages.page_count() >= 2, "the fixture paginates");
    assert!(
        rows(&pages, 1).is_empty(),
        "page 1 is filler; the table must not reach it"
    );
    assert_eq!(rows(&pages, TABLE_PAGE).len(), 2, "both rows on page 2");
}

/// Row 2 is the owner's form shape: `LABEL | (empty)`, bottom-aligned in a row
/// three times the height of its content. Clicking anywhere in the empty value
/// box must put the caret in the value box.
#[test]
fn clicking_an_empty_value_cell_lands_in_that_cell() {
    let pages = layout();
    let rows = rows(&pages, TABLE_PAGE);
    let form_row = rows[1];
    let label = paragraph_of(&cells(form_row)[0]);
    let value = paragraph_of(&cells(form_row)[1]);
    assert_ne!(label, value);

    // The row is 1200 twips and its content 240, pinned to the bottom: the top
    // 960 twips of the value cell are blank, and the label's line is the
    // nearest thing that paints anything.
    for down in [40, 200, 600, 900, 1_150] {
        assert_eq!(
            node_at(&pages, form_row, 1, down),
            value,
            "a click {down} twips into the empty value cell stays in that cell"
        );
    }
    for down in [40, 600, 1_150] {
        assert_eq!(
            node_at(&pages, form_row, 0, down),
            label,
            "and the label column still answers with the label"
        );
    }
}

/// Row 1 is `LOGO | TEXT`: a picture-only cell beside a wordy one. Below the
/// picture, a line of the *neighbouring* cell is vertically nearer than the
/// picture cell's own line, so a nearest-line search leaves the clicked cell.
#[test]
fn clicking_a_picture_only_cell_lands_in_that_cell() {
    let pages = layout();
    let rows = rows(&pages, TABLE_PAGE);
    let logo_row = rows[0];
    let logo = paragraph_of(&cells(logo_row)[0]);
    let text = paragraph_of(&cells(logo_row)[1]);
    assert_ne!(logo, text);

    for down in [100, 800, 1_500, 2_300] {
        assert_eq!(
            node_at(&pages, logo_row, 0, down),
            logo,
            "a click {down} twips into the picture cell stays in that cell"
        );
    }
    for down in [100, 800] {
        assert_eq!(
            node_at(&pages, logo_row, 1, down),
            text,
            "and the text column still answers with the text"
        );
    }
}

/// Routing a click into the right cell is only half the job: the OFFSET has to
/// be one the paragraph can hold.
///
/// The logo paragraph carries no text, so the only valid caret offset in it is
/// 0. The line builder gives a drawing-only paragraph the inverted range
/// `[u32::MAX, 0]`, and reading `range.start` off it answered `u32::MAX` — which
/// the edit layer rejects as "text offset overflow". The user clicked the
/// picture box, the caret went to the right cell, and every keystroke was then
/// thrown away with "That edit isn't supported for this selection yet".
///
/// The inverted range is an upstream defect in the line builder and is reported
/// as one; this pins the contract `hit_test` owes its caller either way.
#[test]
fn a_picture_only_cell_resolves_to_an_offset_its_paragraph_can_hold() {
    let pages = layout();
    let rows = rows(&pages, TABLE_PAGE);
    let logo_row = rows[0];
    for down in [100, 800, 1_500, 2_300] {
        let (_, offset) = click_in_cell(&pages, logo_row, 0, down);
        assert_eq!(
            offset, 0,
            "a picture-only paragraph holds no text, so offset 0 is the only \
             caret position in it (got {offset} at {down} twips down)"
        );
    }
}
