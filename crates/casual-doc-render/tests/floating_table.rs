//! A positioned table reaches layout from a real package — package → import →
//! pagination → float layer (`docs/109` row 64 / `docs/105` FID-L-07).
//!
//! `crates/casual-doc-layout/tests/floating_tables.rs` drives the same
//! behaviours from a hand-built model. This one exists because "modeled" is not
//! "shipped" and "built" is not "reachable" (`SKILL` §9.4): `w:tblpPr` has
//! round-tripped through the model since `P1F-29` with **zero** layout
//! consumers, and only an actual `.docx` proves the property survives the
//! importer and arrives where the placement pass can read it.
//!
//! `fixtures/generated/floating-table.docx` carries the exact `w:tblpPr` the
//! owner's corpus does (`demo.docx`): `w:vertAnchor="text" w:tblpY="1"
//! w:rightFromText="187" w:bottomFromText="72"`, `w:tblOverlap="never"`, a
//! 1818 + 1620 twip grid on a US-Letter page with 1-inch margins.

use casual_doc_import::ImportConfig;
use casual_doc_import::ImportMode;
use casual_doc_import::import_package;
use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::page::{AnchorContent, PaginatedLayout};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::units::Twip;
use casual_doc_model::v1::{BlockNode, Document};
use casual_doc_ooxml::DocxPackage;
use casual_doc_ooxml::PackageLimits;

const FLOATING_TABLE_DOCX: &[u8] =
    include_bytes!("../../../fixtures/generated/floating-table.docx");

/// The fixture's authored geometry.
const CONTENT_LEFT: Twip = Twip(1_440);
const GRID_WIDTH: Twip = Twip(3_438);
const RIGHT_FROM_TEXT: Twip = Twip(187);

fn imported() -> Document {
    let mut package = DocxPackage::open(FLOATING_TABLE_DOCX, PackageLimits::default())
        .expect("the floating-table fixture opens");
    import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .expect("the floating-table fixture imports")
    .document
}

fn layout(document: &Document) -> PaginatedLayout {
    paginate_document(document, &ParleyShaper::new())
}

#[test]
fn the_importer_carries_tblppr_through_to_the_model() {
    let document = imported();
    let table = document
        .body()
        .iter()
        .find_map(|block| match block {
            BlockNode::Table(table) => Some(table),
            _ => None,
        })
        .expect("the fixture's table");
    let position = table
        .properties
        .float_position
        .as_ref()
        .expect("`w:tblpPr` must survive import");
    assert_eq!(position.tbl_py_twips, Some(1));
    assert_eq!(position.right_from_text_twips, Some(187));
    assert_eq!(position.bottom_from_text_twips, Some(72));
    assert_eq!(
        position.vert_anchor,
        Some(casual_doc_model::v1::TableAnchor::Text)
    );
}

#[test]
fn the_imported_positioned_table_is_placed_on_the_float_layer_with_text_beside_it() {
    let document = imported();
    let layout = layout(&document);
    let page = &layout.pages[0];

    let float = page
        .anchored
        .iter()
        .find(|anchor| matches!(anchor.content, AnchorContent::Table { .. }))
        .expect(
            "an imported `w:tblpPr` table must reach the float layer — \
             this is the whole of FID-L-07",
        );
    let AnchorContent::Table { rows } = &float.content else {
        unreachable!("filtered above");
    };
    assert_eq!(rows.len(), 2, "both authored rows travel with the float");
    assert_eq!(
        float.rect.origin.x, CONTENT_LEFT,
        "no `w:tblpX`/`w:tblpXSpec`, so the table sits at the text column's leading edge"
    );
    assert_eq!(float.rect.size.width, GRID_WIDTH, "1818 + 1620");

    // It is NOT also in block flow.
    assert!(
        !page
            .placed
            .iter()
            .any(|placed| matches!(placed.fragment, BlockFragment::TableRow { .. })),
        "a positioned table must be lifted out of the galley, not laid out twice"
    );

    // And the prose that follows wraps beside it rather than being pushed below.
    let prose = page
        .placed
        .iter()
        .filter_map(|placed| match &placed.fragment {
            BlockFragment::Paragraph { lines, .. } if lines.lines.len() > 1 => {
                Some((placed.rect, lines))
            }
            _ => None,
        })
        .next_back()
        .expect("the prose paragraph");
    let wrap_edge = float.rect.right() + RIGHT_FROM_TEXT;
    let beside = prose
        .1
        .lines
        .iter()
        .filter_map(|line| line.runs.first())
        .filter(|run| prose.0.origin.y + run.origin.y < float.rect.bottom())
        .collect::<Vec<_>>();
    assert!(
        beside.len() >= 2,
        "several prose lines should share the table's vertical band"
    );
    assert!(
        beside
            .iter()
            .all(|run| prose.0.origin.x + run.origin.x >= wrap_edge),
        "every line beside the table must start clear of it plus `w:rightFromText`"
    );
}
