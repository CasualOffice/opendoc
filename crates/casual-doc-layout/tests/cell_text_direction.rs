//! Turned table-cell text (`w:textDirection` `tbRl` / `btLr`,
//! `table.cell-text-direction`, `docs/168` RM-37) through layout, composition
//! and hit-testing.
//!
//! The guarantees, each asserted on the real pipeline (paginate → compose →
//! hit-test):
//!
//! - a turned cell's lines are flowed upright at a length that runs ALONG the
//!   row, and an `auto` row grows until the text fits on one line (Word's rule);
//! - an `exact` row fixes that length, and the text wraps against it;
//! - the text is composed inside one quarter-turn layer, clipped to the cell;
//! - a caret in turned text is a bar ACROSS the line, and a click on a turned
//!   character resolves back to that character;
//! - a cell that states `lrTb`, or no direction, lays out exactly as before.

use casual_doc_layout::block::{BlockFragment, CellFragment, CellTurn};
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::display::PaintItem;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::hittest::LayoutSnapshot;
use casual_doc_layout::model::ModelPos;
use casual_doc_layout::page::{PaginatedLayout, QUARTER_TURN};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::units::{Point, Twip};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Definitions, Document, GridColumn, HeightRule, InlineNode, Paragraph,
    ParagraphProperties, RowHeight, Run, RunProperties, Table, TableCell, TableCellProperties,
    TableProperties, TableRow, TableRowProperties, TextDirection,
};

/// Long enough to wrap in a 1 in column, short enough to fit on one line along
/// a row of a few inches.
const TEXT: &str = "Quarterly revenue by region";

fn node(id: u64) -> NodeId {
    NodeId::from_parts(78, id).unwrap()
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

/// A one-row table of three 1 in columns: a horizontal label, then two cells
/// holding [`TEXT`] in `middle` and `last` directions.
fn document(
    middle: Option<TextDirection>,
    last: Option<TextDirection>,
    height: RowHeight,
) -> Document {
    let cell = |id: u64, text: &str, direction: Option<TextDirection>| TableCell {
        id: node(id),
        properties: TableCellProperties {
            text_direction: direction,
            ..TableCellProperties::default()
        },
        blocks: vec![paragraph(id + 1, text)],
    };
    let table = BlockNode::Table(Box::new(Table {
        id: node(10),
        grid: vec![
            GridColumn {
                width_twips: Some(1_440),
            },
            GridColumn {
                width_twips: Some(1_440),
            },
            GridColumn {
                width_twips: Some(1_440),
            },
        ],
        grid_change: None,
        properties: TableProperties::default(),
        rows: vec![TableRow {
            id: node(20),
            properties: TableRowProperties {
                height,
                ..TableRowProperties::default()
            },
            cells: vec![
                cell(30, "Label", None),
                cell(40, TEXT, middle),
                cell(50, TEXT, last),
            ],
        }],
    }));
    Document::new(
        node(1),
        vec![paragraph(2, "Before"), table, paragraph(3, "After")],
        Definitions::default(),
    )
    .unwrap()
}

fn layout_of(document: &Document) -> PaginatedLayout {
    paginate_document(document, &ParleyShaper::new())
}

/// The table row placed on page 1: its cells, its height and its rect origin.
fn row_of(layout: &PaginatedLayout) -> (&[CellFragment], Twip, Point) {
    let page = &layout.pages[0];
    page.placed
        .iter()
        .find_map(|placed| match &placed.fragment {
            BlockFragment::TableRow { cells, .. } => Some((
                cells.as_slice(),
                placed.fragment.height(),
                placed.rect.origin,
            )),
            BlockFragment::Paragraph { .. } => None,
        })
        .expect("the table row is placed on the first page")
}

fn line_count(cell: &CellFragment) -> usize {
    cell.blocks
        .iter()
        .map(|block| match block {
            BlockFragment::Paragraph { lines, .. } => lines.lines.len(),
            BlockFragment::TableRow { .. } => 0,
        })
        .sum()
}

#[test]
fn an_auto_row_grows_until_turned_text_fits_on_one_line() {
    let turned = layout_of(&document(
        Some(TextDirection::TbRl),
        Some(TextDirection::BtLr),
        RowHeight::default(),
    ));
    let upright = layout_of(&document(None, None, RowHeight::default()));
    let (cells, height, _) = row_of(&turned);
    let (upright_cells, upright_height, _) = row_of(&upright);

    assert_eq!(cells[0].rotation, None, "the label is horizontal");
    assert_eq!(
        cells[1].rotation.map(|r| r.turn),
        Some(CellTurn::Down),
        "tbRl turns the text down"
    );
    assert_eq!(
        cells[2].rotation.map(|r| r.turn),
        Some(CellTurn::Up),
        "btLr turns the text up"
    );

    // Upright in a 1 in column the text wraps; turned, its row grew until the
    // text fits on one line.
    assert!(
        line_count(&upright_cells[1]) > 1,
        "the premise: the text wraps in a 1 in column"
    );
    assert_eq!(line_count(&cells[1]), 1, "turned, it is one line");
    assert_eq!(line_count(&cells[2]), 1, "turned, it is one line");
    assert!(
        height.raw() > upright_height.raw(),
        "the row grew to the turned text: {height:?} vs {upright_height:?}"
    );

    // After layout the flowed length IS the content height the row gives the
    // cell, so painting along it fills the row.
    for cell in &cells[1..] {
        let rotation = cell.rotation.expect("turned");
        assert_eq!(
            Twip(rotation.length.raw() + cell.vertical_insets().raw()),
            height,
            "the turned cell's length is its row's content height"
        );
    }
}

#[test]
fn an_exact_row_wraps_turned_text_against_its_height() {
    let exact = RowHeight {
        value_twips: Some(900),
        rule: Some(HeightRule::Exact),
    };
    let layout = layout_of(&document(Some(TextDirection::TbRl), None, exact));
    let (cells, height, _) = row_of(&layout);
    assert_eq!(height, Twip(900), "an exact row keeps its height");
    let rotation = cells[1].rotation.expect("turned");
    assert_eq!(
        Twip(rotation.length.raw() + cells[1].vertical_insets().raw()),
        Twip(900),
        "the line length is the exact row's content height"
    );
    assert!(
        line_count(&cells[1]) > 1,
        "the text wraps against the row's height, not the column's width"
    );
}

#[test]
fn turned_text_is_painted_through_one_quarter_turn_layer_clipped_to_its_cell() {
    let layout = layout_of(&document(
        Some(TextDirection::TbRl),
        Some(TextDirection::BtLr),
        RowHeight::default(),
    ));
    let list = compose_page(&layout.pages[0]);
    let turns: Vec<i32> = list
        .items
        .windows(2)
        .filter_map(|pair| match pair {
            [
                PaintItem::PushClip(_),
                PaintItem::PushLayer {
                    transform: Some(transform),
                    ..
                },
            ] => Some(transform.rotation),
            _ => None,
        })
        .collect();
    assert_eq!(
        turns,
        vec![QUARTER_TURN, 3 * QUARTER_TURN],
        "one clipped layer per turned cell: a quarter turn for tbRl, three for btLr"
    );
    // The upright control paints no layer at all.
    let upright = layout_of(&document(None, None, RowHeight::default()));
    assert!(
        !compose_page(&upright.pages[0])
            .items
            .iter()
            .any(|item| matches!(item, PaintItem::PushLayer { .. })),
        "horizontal cells paint no layer"
    );
}

#[test]
fn a_caret_in_turned_text_crosses_the_line_and_a_click_there_returns_it() {
    let layout = layout_of(&document(
        Some(TextDirection::TbRl),
        Some(TextDirection::BtLr),
        RowHeight::default(),
    ));
    let (cells, height, origin) = row_of(&layout);
    let snapshot = LayoutSnapshot::new(&layout);
    for (index, paragraph) in [(1_usize, 41_u64), (2, 51)] {
        let cell = &cells[index];
        let (left, right) = (
            origin.x.raw() + cell.x.raw(),
            origin.x.raw() + cell.x.raw() + cell.width.raw(),
        );
        let (top, bottom) = (origin.y.raw(), origin.y.raw() + height.raw());
        for offset in [3_u32, 10, 17] {
            let pos = ModelPos {
                node: node(paragraph),
                offset,
            };
            let (page, rect) = snapshot.caret_rect(pos).expect("the caret has geometry");
            assert_eq!(page, 1);
            assert_eq!(
                rect.size.height,
                Twip::ZERO,
                "a caret in turned text is a bar across the line, not along it"
            );
            assert!(
                rect.size.width.raw() > 0,
                "the bar has the line's thickness"
            );
            let mid = Point::new(
                Twip(rect.origin.x.raw() + rect.size.width.raw() / 2),
                rect.origin.y,
            );
            assert!(
                (left..right).contains(&mid.x.raw()) && (top..bottom).contains(&mid.y.raw()),
                "the caret sits inside its own cell: {mid:?} not in x {left}..{right}, \
                 y {top}..{bottom}"
            );
            let hit = snapshot.hit_test(1, mid).expect("a hit");
            assert_eq!(
                hit.pos, pos,
                "a click on the caret's spot in a turned cell returns that position"
            );
        }
    }
}

#[test]
fn turned_down_text_runs_top_to_bottom_and_turned_up_runs_bottom_to_top() {
    let layout = layout_of(&document(
        Some(TextDirection::TbRl),
        Some(TextDirection::BtLr),
        RowHeight::default(),
    ));
    let snapshot = LayoutSnapshot::new(&layout);
    let y = |paragraph: u64, offset: u32| {
        snapshot
            .caret_rect(ModelPos {
                node: node(paragraph),
                offset,
            })
            .expect("geometry")
            .1
            .origin
            .y
            .raw()
    };
    assert!(y(41, 10) > y(41, 2), "tbRl: later text is lower");
    assert!(y(51, 10) < y(51, 2), "btLr: later text is higher");
}

#[test]
fn a_cell_stating_lr_tb_or_nothing_lays_out_exactly_as_before() {
    let none = layout_of(&document(None, None, RowHeight::default()));
    let lr_tb = layout_of(&document(
        Some(TextDirection::LrTb),
        Some(TextDirection::LrTb),
        RowHeight::default(),
    ));
    let (none_cells, none_height, _) = row_of(&none);
    let (lr_cells, lr_height, _) = row_of(&lr_tb);
    assert!(none_cells.iter().all(|cell| cell.rotation.is_none()));
    assert!(lr_cells.iter().all(|cell| cell.rotation.is_none()));
    assert_eq!(none_height, lr_height);
    assert_eq!(none_cells, lr_cells, "lrTb is the horizontal default");
}
