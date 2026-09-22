//! Positioned (floating) tables — `w:tblPr/w:tblpPr` — end to end
//! (`docs/109` row 64 / `docs/105` FID-L-07).
//!
//! These drive the real pipeline (`paginate_document`), not a resolver in
//! isolation, and assert the four behaviours a positioned table has to have and
//! did not have before:
//!
//! 1. it is **placed at its `w:tblpPr` rectangle**, on the float layer, not in
//!    block flow;
//! 2. it **reserves no band in the flow** — the text after it moves up into the
//!    space it would have taken;
//! 3. the surrounding text **wraps beside it** at the `*FromText` distances
//!    instead of being pushed below;
//! 4. it is still **hit-testable**, so a click in a positioned table's cell
//!    resolves to that cell's paragraph rather than to the body text behind it.
//!
//! Plus the two rules that come with it: `w:tblOverlap` displacement, and the
//! windowed driver's refusal superset.

use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::display::PaintItem;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::hittest::{HitZone, LayoutSnapshot};
use casual_doc_layout::page::{AnchorContent, PaginatedLayout};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::units::{Point, Twip};
use casual_doc_layout::windowed::{NotWindowable, measure_document};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Definitions, Document, GridColumn, InlineNode, Paragraph, ParagraphProperties, Run,
    RunProperties, Table, TableAnchor, TableCell, TableCellProperties, TableFloatPosition,
    TableOverlap, TableProperties, TableRow, TableRowProperties,
};

fn node(id: u64) -> NodeId {
    NodeId::from_parts(77, id).unwrap()
}

fn paragraph(id: u64, text: &str) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: node(id),
        properties: ParagraphProperties::default().into(),
        inlines: vec![InlineNode::Run(Run {
            id: node(id + 1_000),
            properties: RunProperties::default().into(),
            text: text.to_owned(),
        })],
    })
}

/// A two-column, two-row table 3 438 twips wide — the shape the owner's
/// `demo.docx` floats (`LightList-Accent3`, `w:gridCol` 1818 + 1620).
fn table(
    id: u64,
    position: Option<TableFloatPosition>,
    overlap: Option<TableOverlap>,
) -> BlockNode {
    let cell = |cell_id: u64, text: &str| TableCell {
        id: node(cell_id),
        properties: TableCellProperties::default(),
        blocks: vec![paragraph(cell_id + 1, text)],
    };
    let row = |row_id: u64, a: &str, b: &str| TableRow {
        id: node(row_id),
        properties: TableRowProperties::default(),
        cells: vec![cell(row_id + 10, a), cell(row_id + 20, b)],
    };
    BlockNode::Table(Box::new(Table {
        id: node(id),
        grid: vec![
            GridColumn {
                width_twips: Some(1_818),
            },
            GridColumn {
                width_twips: Some(1_620),
            },
        ],
        grid_change: None,
        properties: TableProperties {
            float_position: position,
            overlap,
            ..TableProperties::default()
        },
        rows: vec![row(id + 100, "ITEM", "COST"), row(id + 200, "Widget", "12")],
    }))
}

/// `demo.docx`'s own positioning: anchored to the text (so it sits at the flow
/// position), nudged one twip down, with a 187-twip right wrap gap.
fn demo_position() -> TableFloatPosition {
    TableFloatPosition {
        vert_anchor: Some(TableAnchor::Text),
        tbl_py_twips: Some(1),
        right_from_text_twips: Some(187),
        bottom_from_text_twips: Some(72),
        ..TableFloatPosition::default()
    }
}

const PROSE: &str = "the body text beside a positioned table must wrap around it ";

fn document_of(blocks: Vec<BlockNode>) -> Document {
    Document::new(node(9_999), blocks, Definitions::default()).unwrap()
}

fn layout_of(document: &Document) -> PaginatedLayout {
    paginate_document(document, &ParleyShaper::new())
}

/// The single positioned table placed on page 0, and its rows.
fn placed_float_table(layout: &PaginatedLayout) -> (casual_doc_layout::units::Rect, usize) {
    let anchored: Vec<_> = layout
        .pages
        .iter()
        .flat_map(|page| page.anchored.iter())
        .filter(|anchor| matches!(anchor.content, AnchorContent::Table { .. }))
        .collect();
    assert_eq!(
        anchored.len(),
        1,
        "exactly one positioned table should reach the float layer"
    );
    let AnchorContent::Table { rows } = &anchored[0].content else {
        unreachable!("filtered above");
    };
    assert_eq!(rows.len(), 2, "both rows are carried on the float");
    (anchored[0].rect, rows.len())
}

fn table_rows_in_flow(layout: &PaginatedLayout, table_id: NodeId) -> usize {
    layout
        .pages
        .iter()
        .flat_map(|page| page.placed.iter())
        .filter(
            |placed| matches!(&placed.fragment, BlockFragment::TableRow { table, .. } if *table == table_id),
        )
        .count()
}

#[test]
fn a_positioned_table_places_at_its_tblppr_rect_and_leaves_block_flow() {
    let position = TableFloatPosition {
        horz_anchor: Some(TableAnchor::Page),
        vert_anchor: Some(TableAnchor::Page),
        tbl_px_twips: Some(1_440),
        tbl_py_twips: Some(2_880),
        ..TableFloatPosition::default()
    };
    let document = document_of(vec![
        paragraph(1, "before"),
        table(2_000, Some(position), None),
        paragraph(3, "after"),
    ]);
    let layout = layout_of(&document);

    let (rect, _) = placed_float_table(&layout);
    assert_eq!(
        (rect.origin.x, rect.origin.y),
        (Twip(1_440), Twip(2_880)),
        "a page-anchored table resolves to the absolute page point, \
         not to wherever block flow left the cursor"
    );
    assert_eq!(rect.size.width, Twip(3_438), "grid width 1818 + 1620");

    assert_eq!(
        table_rows_in_flow(&layout, node(2_000)),
        0,
        "a positioned table must not also occupy the flow"
    );

    // It reserves no band: the paragraph after it starts immediately below the
    // paragraph before it.
    let page = &layout.pages[0];
    let before = page
        .placed
        .iter()
        .find(|placed| placed.fragment.node_id() == node(1))
        .expect("paragraph before");
    let after = page
        .placed
        .iter()
        .find(|placed| placed.fragment.node_id() == node(3))
        .expect("paragraph after");
    assert_eq!(
        after.rect.origin.y,
        before.rect.bottom(),
        "the positioned table reserves no vertical space in the flow"
    );
}

#[test]
fn a_positioned_table_at_the_flow_position_wraps_the_following_text_beside_it() {
    let document = document_of(vec![
        paragraph(1, "before"),
        table(2_000, Some(demo_position()), None),
        paragraph(3, &PROSE.repeat(30)),
    ]);
    let layout = layout_of(&document);
    assert_eq!(
        layout,
        layout_of(&document),
        "the bounded float fixed point must be deterministic"
    );

    let (rect, _) = placed_float_table(&layout);
    let page = &layout.pages[0];
    let placed = page
        .placed
        .iter()
        .find(|placed| placed.fragment.node_id() == node(3))
        .expect("the prose paragraph");
    let BlockFragment::Paragraph { lines, .. } = &placed.fragment else {
        panic!("expected a paragraph fragment");
    };

    // The wrap gap Word applies is the table's right edge plus `rightFromText`.
    let wrap_edge = rect.right() + Twip(187);
    let mut beside = 0;
    let mut below = 0;
    for line in &lines.lines {
        let Some(run) = line.runs.first() else {
            continue;
        };
        let baseline = placed.rect.origin.y + run.origin.y;
        let x = placed.rect.origin.x + run.origin.x;
        if baseline < rect.bottom() {
            assert!(
                x >= wrap_edge,
                "line at y={} starts at x={}, inside the table's wrap zone ending at {}",
                baseline.raw(),
                x.raw(),
                wrap_edge.raw()
            );
            beside += 1;
        } else if run.origin.x == Twip::ZERO {
            below += 1;
        }
    }
    assert!(
        beside >= 2,
        "several lines should sit BESIDE the positioned table, not under it"
    );
    assert!(
        below >= 1,
        "the full measure returns once the text clears the table"
    );
    assert!(
        placed.rect.origin.y < rect.bottom(),
        "the prose must start alongside the table, not below it"
    );
}

#[test]
fn a_click_inside_a_positioned_table_resolves_to_its_own_cell_paragraph() {
    let document = document_of(vec![
        paragraph(1, "before"),
        table(2_000, Some(demo_position()), None),
        paragraph(3, &PROSE.repeat(30)),
    ]);
    let layout = layout_of(&document);
    let (rect, _) = placed_float_table(&layout);

    // A point well inside the first cell of the first row.
    let inside = Point::new(
        Twip(rect.origin.x.raw() + 200),
        Twip(rect.origin.y.raw() + 100),
    );
    let hit = LayoutSnapshot::new(&layout)
        .hit_test_text_box(1, inside)
        .expect("a point inside a positioned table must resolve on the float layer");
    assert_eq!(hit.zone, HitZone::Content);
    assert_eq!(
        hit.pos.node,
        node(2_111),
        "the click must land on the floating table's own first-cell paragraph, \
         not on the body text painted behind it"
    );
}

#[test]
fn tbl_overlap_never_pushes_the_later_positioned_table_clear_of_the_earlier() {
    let same_spot = || TableFloatPosition {
        horz_anchor: Some(TableAnchor::Page),
        vert_anchor: Some(TableAnchor::Page),
        tbl_px_twips: Some(1_440),
        tbl_py_twips: Some(2_880),
        ..TableFloatPosition::default()
    };
    let document = document_of(vec![
        paragraph(1, "before"),
        table(2_000, Some(same_spot()), None),
        table(4_000, Some(same_spot()), None),
        paragraph(3, "after"),
    ]);
    let layout = layout_of(&document);
    let anchors: Vec<_> = layout.pages[0]
        .anchored
        .iter()
        .filter(|anchor| matches!(anchor.content, AnchorContent::Table { .. }))
        .collect();
    assert_eq!(anchors.len(), 2);
    let (first, second) = (anchors[0].rect, anchors[1].rect);
    assert_eq!(first.origin.y, Twip(2_880));
    assert_eq!(
        second.origin.y,
        first.bottom(),
        "`w:tblOverlap` defaults to never, so the second table is displaced \
         DOWN clear of the first — never sideways, which would re-break its text"
    );

    // And with `overlap` they are allowed to sit on top of each other.
    let overlapping = document_of(vec![
        paragraph(1, "before"),
        table(2_000, Some(same_spot()), Some(TableOverlap::Overlap)),
        table(4_000, Some(same_spot()), Some(TableOverlap::Overlap)),
        paragraph(3, "after"),
    ]);
    let layout = layout_of(&overlapping);
    let ys: Vec<_> = layout.pages[0]
        .anchored
        .iter()
        .filter_map(|anchor| match anchor.content {
            AnchorContent::Table { .. } => Some(anchor.rect.origin.y),
            _ => None,
        })
        .collect();
    assert_eq!(
        ys,
        vec![Twip(2_880), Twip(2_880)],
        "`w:tblOverlap=\"overlap\" leaves both tables where they were authored"
    );
}

#[test]
fn an_ordinary_table_is_untouched_by_the_float_pass() {
    let document = document_of(vec![
        paragraph(1, "before"),
        table(2_000, None, None),
        paragraph(3, "after"),
    ]);
    let layout = layout_of(&document);
    assert!(
        layout.pages[0].anchored.is_empty(),
        "a table with no `w:tblpPr` must never reach the float layer"
    );
    assert_eq!(
        table_rows_in_flow(&layout, node(2_000)),
        2,
        "an ordinary table still flows as a block"
    );
}

#[test]
fn the_windowed_driver_refuses_a_document_that_positions_a_table() {
    // `document_has_anchored_object` is the windowed driver's float refusal,
    // and it is only sound as a SUPERSET: `false` has to mean the float passes
    // really are inert. A positioned table places a float and re-flows the body
    // against its exclusions, so a window cannot be paginated independently of
    // it — and a document windowed anyway would paginate differently depending
    // on how it was opened. The existing `no_anchored_object_means_no_floats`
    // corpus guard cannot see this: its corpus has no positioned table.
    let document = document_of(vec![
        paragraph(1, "before"),
        table(2_000, Some(demo_position()), None),
        paragraph(3, &PROSE.repeat(30)),
    ]);
    assert!(
        !layout_of(&document)
            .pages
            .iter()
            .all(|page| page.anchored.is_empty()),
        "precondition: the full layout of this document DOES place a float"
    );
    assert!(
        matches!(
            measure_document(&document, &ParleyShaper::new()),
            Err(NotWindowable::AnchoredFloats)
        ),
        "the windowed driver must refuse a document that positions a table"
    );

    // And the refusal is specific: the same document without `w:tblpPr` windows.
    let plain = document_of(vec![
        paragraph(1, "before"),
        table(2_000, None, None),
        paragraph(3, &PROSE.repeat(30)),
    ]);
    assert!(
        measure_document(&plain, &ParleyShaper::new()).is_ok(),
        "an ordinary table must still be windowable"
    );
}

#[test]
fn a_positioned_table_actually_paints_at_its_placed_rectangle() {
    // Placing a float on the page is not the same as showing it: "built" is not
    // "reachable". This asserts the composed display list really carries the
    // table's glyphs inside its resolved rectangle, so a positioned table that
    // reached the float layer but painted nothing would be caught.
    let document = document_of(vec![
        paragraph(1, "before"),
        table(2_000, Some(demo_position()), None),
        paragraph(3, &PROSE.repeat(30)),
    ]);
    let layout = layout_of(&document);
    let (rect, _) = placed_float_table(&layout);
    let list = compose_page(&layout.pages[0]);

    let inside: Vec<_> = list
        .items
        .iter()
        .filter_map(|item| match item {
            PaintItem::Glyphs { run } => Some(run),
            _ => None,
        })
        .filter(|run| {
            run.origin.x.raw() >= rect.origin.x.raw()
                && run.origin.x.raw() < rect.right().raw()
                && run.origin.y.raw() >= rect.origin.y.raw()
                && run.origin.y.raw() <= rect.bottom().raw()
        })
        .collect();
    assert!(
        inside.len() >= 4,
        "the positioned table's four cells must paint inside its rectangle; \
         found {} glyph runs there",
        inside.len()
    );
}
