// SPDX-License-Identifier: Apache-2.0

//! `p:graphicFrame`, `a:tbl` and `tableStyles.xml`, driven from the real
//! `.pptx`.
//!
//! Every guard here was written, driven **red** by a mutation of the production
//! code, and only then restored. The mutations and their verbatim output are in
//! the commit message.
//!
//! Two rules shape what is asserted, both of them lessons this crate already
//! paid for:
//!
//! * **A fixture with no competition cannot test selection.** Every value the
//!   table states is distinct from every other value of the same kind — three
//!   grid widths, three row heights, four margins, two borders with different
//!   widths AND colours, six flags at `1 0 0 1 1 0`, and a style GUID that is
//!   neither the part's `@def` nor its first entry. So an assertion that passes
//!   cannot be passing because two candidates happen to agree.
//! * **Presence is nearly vacuous.** Each guard asserts the exact value, the
//!   count and the ORDER.

use casual_pres_model::{CellMerge, SlideTable, TextAnchor};

use super::{features, import_fixture};

/// The table on the fixture's third slide, reached the way a consumer would.
fn fixture_table() -> SlideTable {
    let imported = import_fixture();
    let slide = imported
        .presentation
        .slides()
        .get(2)
        .expect("the fixture deck has three slides")
        .clone();
    slide
        .shapes
        .children
        .iter()
        .find_map(|node| node.table.clone())
        .expect("slide 10 carries one a:tbl")
}

/// The frame is a positioned, unpainted box, and the table hangs off the node it
/// is on rather than inside the drawing.
///
/// The box matters more than it looks: without it the table has no rectangle at
/// all, because an `a:tbl` states its width only as the sum of its grid and
/// states no position anywhere.
#[test]
fn a_graphic_frame_arrives_as_a_positioned_unpainted_box_carrying_its_table() {
    let imported = import_fixture();
    let slide = &imported.presentation.slides()[2];
    let names: Vec<Option<&str>> = slide
        .shapes
        .children
        .iter()
        .map(|node| node.name.as_deref())
        .collect();
    assert_eq!(
        names,
        vec![
            Some("Title 1"),
            Some("Freeform"),
            Some("Callout"),
            Some("Table 4"),
            Some("Chart 5"),
            // The connector that closes the tree; it is here because a suppressed
            // outline on a LINE is the one case layout answers by emitting no
            // anchor at all, and it sits last so a reopened deck's node ids cannot
            // shift (`deck.rs` says why).
            Some("Invisible Rule"),
        ],
        "both frames join the tree in document order, which is paint order: a          frame dropped or appended would move the chart over the table"
    );
    let table_frame = slide
        .shapes
        .children
        .iter()
        .find(|node| node.table.is_some())
        .expect("the table frame arrives");
    assert_eq!(
        table_frame.name.as_deref(),
        Some("Table 4"),
        "p:cNvPr@name on a p:nvGraphicFramePr is the same name a p:sp carries"
    );
    let casual_doc_model::v1::GroupChild::Shape(shape) = &table_frame.content else {
        panic!("a frame imports as the shape every other slide child is");
    };
    // p:xfrm, NOT a:xfrm, and these are the fixture's own numbers.
    assert_eq!(
        (shape.offset.x_emu, shape.offset.y_emu),
        (7_315_200, 2_286_000),
        "the frame's p:xfrm/a:off is its position in slide EMU"
    );
    assert_eq!(
        (shape.extent.width_emu, shape.extent.height_emu),
        (3_657_600, 1_371_600),
        "the frame's p:xfrm/a:ext is its extent in slide EMU"
    );
    assert_eq!(
        (shape.fill.is_none(), shape.stroke.is_none()),
        (true, true),
        "a p:graphicFrame has no a:solidFill and no a:ln in its schema, so \
         painting either would invent chrome the file does not ask for"
    );
    assert_eq!(
        (shape.rotation, shape.flip_h, shape.flip_v),
        (None, false, false),
        "the fixture's frame states no rotation or flip, and none is invented"
    );

    // The CHART frame arrives too, and arrives WITHOUT a table — which is the
    // fact that makes `graphicFrame`'s absence from the report honest.
    let chart_frame = slide
        .shapes
        .children
        .iter()
        .find(|node| node.name.as_deref() == Some("Chart 5"))
        .expect("the chart frame arrives as a positioned box");
    assert!(
        chart_frame.table.is_none(),
        "a c:chart payload is left to docs/155; only an a:tbl populates the table"
    );
    assert!(
        features(&imported).contains(&"chart"),
        "and the chart payload is still reported as lost"
    );
}

/// The grid is read in order with each column's own width, and the widths are
/// three DIFFERENT numbers so a transposition cannot pass.
#[test]
fn the_grid_keeps_every_column_width_in_grid_order() {
    let table = fixture_table();
    let widths: Vec<i64> = table.grid.iter().map(|column| column.width_emu).collect();
    assert_eq!(
        widths,
        vec![1_828_800, 2_743_200, 914_400],
        "a:gridCol@w is EMU, in a:tblGrid order; the three values differ so a \
         reversed or de-duplicated grid is visible"
    );
    assert_eq!(
        table.width_emu(),
        5_486_400,
        "the table's authored width is the grid's sum and is stated nowhere else"
    );
    // `<a:gridCol w="…"/>` is self-closing, which is the shape that used to be
    // answered with `default()` and lose every attribute.
    assert!(
        widths.iter().all(|width| *width > 0),
        "a self-closing a:gridCol states its whole meaning in @w"
    );
}

/// Each row keeps its own `a:tr@h`, in order, and holds exactly one cell per
/// grid column — continuations included.
#[test]
fn every_row_keeps_its_height_and_one_cell_per_grid_column() {
    let table = fixture_table();
    let heights: Vec<i64> = table.rows.iter().map(|row| row.height_emu).collect();
    assert_eq!(
        heights,
        vec![370_840, 457_200, 533_400],
        "a:tr@h is EMU and is a MINIMUM; the three values differ so a row that \
         took its neighbour's height is visible"
    );
    let cells: Vec<usize> = table.rows.iter().map(|row| row.cells.len()).collect();
    assert_eq!(
        cells,
        vec![table.grid.len(); 3],
        "PresentationML writes one a:tc per a:gridCol, continuations included"
    );
}

/// `a:tblPr`'s six conditional-format flags and `@rtl` land on their own fields.
///
/// The fixture states `1 0 0 1 1 0`, so **no pair of flags can be swapped
/// without this failing** — which a fixture setting all six to `1` could not
/// tell you.
#[test]
fn the_banding_flags_land_on_their_own_fields_and_cannot_be_swapped() {
    let table = fixture_table();
    let properties = &table.properties;
    assert_eq!(
        (
            properties.first_row,
            properties.last_row,
            properties.first_column,
            properties.last_column,
            properties.banded_rows,
            properties.banded_columns,
            properties.rtl,
        ),
        (true, false, false, true, true, false, false),
        "firstRow/lastRow/firstCol/lastCol/bandRow/bandCol/rtl, each from its own \
         attribute; the fixture's pattern makes every transposition observable"
    );
}

/// The style GUID travels WITH its braces and joins to the right entry.
///
/// The competition is the point: `tableStyles.xml`'s `@def` names a different
/// style, and the entry the table names is the SECOND one. So three wrong answers
/// are each distinguishable from the right one — take `@def`, take `styles[0]`,
/// or strip the braces and match nothing.
#[test]
fn a_table_style_joins_by_its_braced_guid_and_not_by_the_part_default() {
    let imported = import_fixture();
    let table = fixture_table();
    let styles = imported.presentation.table_styles();

    const MEDIUM: &str = "{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}";
    const NO_GRID: &str = "{2D5ABB26-0587-4C30-8999-92F81FD0307C}";

    assert_eq!(
        table.properties.style_id.as_deref(),
        Some(MEDIUM),
        "the braces are part of the token; a trimmed id breaks the join silently"
    );
    assert_eq!(
        styles.default_style_id.as_deref(),
        Some(NO_GRID),
        "a:tblStyleLst@def is the deck's default and is NOT this table's style"
    );
    assert_ne!(
        table.properties.style_id.as_deref(),
        styles.default_style_id.as_deref(),
        "the fixture makes the two disagree on purpose, so resolving through \
         @def cannot pass"
    );

    // Both entries, in part order — which is what a self-closing first entry
    // whose children were entered would destroy, by eating the second entry's
    // events as its own.
    let ids: Vec<&str> = styles
        .styles
        .iter()
        .map(|style| style.id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec![NO_GRID, MEDIUM],
        "both a:tblStyle entries arrive, in part order"
    );
    let names: Vec<Option<&str>> = styles
        .styles
        .iter()
        .map(|style| style.name.as_deref())
        .collect();
    assert_eq!(
        names,
        vec![Some("No Style, No Grid"), Some("Medium Style 2 - Accent 1")],
        "a self-closing a:tblStyle keeps @styleName, and the populated one is \
         still the second entry"
    );

    let joined = styles
        .style(table.properties.style_id.as_deref().expect("a style id"))
        .expect("the table's GUID resolves to an entry");
    assert_eq!(
        joined.name.as_deref(),
        Some("Medium Style 2 - Accent 1"),
        "the GUID joins to the entry the FILE names, not to the first one"
    );
    assert!(
        styles
            .style("5C22544A-7EE6-4342-B048-85BDC9FD1C3A")
            .is_none(),
        "the unbraced spelling is a different token and must not match"
    );
}

/// The two encodings of a merge stay two different facts.
///
/// `@gridSpan`/`@rowSpan` mark the ORIGIN and `@hMerge`/`@vMerge` mark a
/// CONTINUATION. This is the guard the whole model shape exists for: conflating
/// them gives the origin's content to every covered cell, so the assertion is on
/// the per-cell roles AND on the resulting text, column by column.
#[test]
fn a_span_and_a_continuation_are_not_the_same_fact() {
    let table = fixture_table();

    let roles: Vec<Vec<(CellMerge, CellMerge)>> = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| (cell.horizontal, cell.vertical))
                .collect()
        })
        .collect();
    assert_eq!(
        roles,
        vec![
            vec![
                // `gridSpan="2"` — the origin of a two-column region.
                (CellMerge::Origin(2), CellMerge::None),
                // `<a:tc hMerge="1"/>` — covered, and self-closing, so its whole
                // meaning is the attribute.
                (CellMerge::Continuation, CellMerge::None),
                // `rowSpan="2"` — the origin of a two-ROW region, on the other
                // axis, in the same row.
                (CellMerge::None, CellMerge::Origin(2)),
            ],
            vec![
                (CellMerge::None, CellMerge::None),
                (CellMerge::None, CellMerge::None),
                // `<a:tc vMerge="1"/>` — covered by the row above.
                (CellMerge::None, CellMerge::Continuation),
            ],
            vec![
                (CellMerge::None, CellMerge::None),
                (CellMerge::None, CellMerge::None),
                (CellMerge::None, CellMerge::None),
            ],
        ],
        "each axis's origin and continuation are distinct variants of one enum, \
         so a reader cannot hold both for one axis"
    );

    // And the consequence the roles exist to prevent: no string appears twice.
    let text: Vec<Vec<String>> = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| {
                    cell.text
                        .as_ref()
                        .map(casual_pres_model::TextBody::plain_text)
                        .unwrap_or_default()
                })
                .collect()
        })
        .collect();
    assert_eq!(
        text,
        vec![
            vec![
                "Spans two".to_owned(),
                String::new(),
                "Tall right".to_owned()
            ],
            vec![
                "Middle left".to_owned(),
                "Middle mid".to_owned(),
                // A covered cell's own `a:txBody` is RETAINED, because a round
                // trip has to write it back. Whether it PAINTS is layout's
                // question, and `casual-pres-layout` answers no.
                "Covered".to_owned()
            ],
            vec![
                "Bottom left".to_owned(),
                "Bottom mid".to_owned(),
                "Bottom right".to_owned()
            ],
        ],
        "a covered cell owns NO content; giving it the origin's text is what \
         renders a table with duplicated cells"
    );
    let mut strings: Vec<&str> = text
        .iter()
        .flatten()
        .map(String::as_str)
        .filter(|value| !value.is_empty())
        .collect();
    let total = strings.len();
    strings.sort_unstable();
    strings.dedup();
    assert_eq!(
        strings.len(),
        total,
        "every cell's text is distinct in the fixture, so a duplicate here is a \
         conflated merge and nothing else"
    );
    assert_eq!(
        total, 8,
        "eight of the nine grid cells carry text; only the self-closing hMerge \
         continuation carries none"
    );
}

/// `a:tcPr`'s margins, anchor, fill and four borders land on their own fields,
/// with the absent edges absent.
///
/// `a:lnL` and `a:lnB` are present with DIFFERENT widths and DIFFERENT colours
/// while `a:lnR` and `a:lnT` are absent — so a reader that filed the leading edge
/// as the trailing one, or copied one edge to all four, fails here. All four
/// margins differ for the same reason.
#[test]
fn a_cells_properties_land_on_their_own_fields_and_the_absent_edges_stay_absent() {
    let table = fixture_table();
    let cell = &table.rows[0].cells[0];
    let properties = &cell.properties;
    assert_eq!(
        (
            properties.margin_left_emu,
            properties.margin_right_emu,
            properties.margin_top_emu,
            properties.margin_bottom_emu,
        ),
        (137_160, 228_600, 45_720, 91_440),
        "marL/marR/marT/marB are EMU and the fixture's four values differ, so a \
         swapped pair cannot pass"
    );
    assert_eq!(
        properties.anchor,
        TextAnchor::Center,
        "a:tcPr@anchor is the same five-token enum a:bodyPr@anchor takes"
    );
    assert_eq!(
        properties.fill,
        Some(casual_doc_model::v1::Fill::Solid(
            casual_doc_model::v1::Rgba {
                r: 0xFF,
                g: 0xF2,
                b: 0xCC,
                a: 0xFF,
            }
        )),
        "a cell's a:solidFill is the same v1::Fill a shape's is"
    );

    let left = properties
        .border_left
        .as_ref()
        .expect("a:lnL is present in the fixture");
    let bottom = properties
        .border_bottom
        .as_ref()
        .expect("a:lnB is present in the fixture");
    assert_eq!(
        (left.width_emu, left.color.r, left.color.g, left.color.b),
        (12_700, 0xFF, 0x00, 0x00),
        "a:lnL@w is EMU and the edge keeps its own colour"
    );
    assert_eq!(
        (
            bottom.width_emu,
            bottom.color.r,
            bottom.color.g,
            bottom.color.b
        ),
        (38_100, 0x00, 0x00, 0xFF),
        "a:lnB is a different width AND a different colour, so one edge copied \
         to another is observable"
    );
    assert_eq!(
        (
            properties.border_right.is_none(),
            properties.border_top.is_none()
        ),
        (true, true),
        "a:lnR and a:lnT are absent and must stay absent: a cell with two \
         borders is not a cell with four"
    );
}

/// A self-closing `a:tcPr` keeps its attributes.
///
/// `<a:tcPr marL="320040" anchor="b"/>` has no children, so a reader that only
/// looks at attributes when it is about to descend discards both values — the
/// precise shape of the bug `xml::enter` was written for, in a new element.
#[test]
fn a_self_closing_cell_properties_element_keeps_its_attributes() {
    let table = fixture_table();
    let properties = &table.rows[1].cells[1].properties;
    assert_eq!(
        (properties.margin_left_emu, properties.anchor),
        (320_040, TextAnchor::Bottom),
        "a self-closing a:tcPr states its whole meaning in its attributes"
    );
    assert_eq!(
        properties.margin_right_emu,
        casual_pres_model::DEFAULT_CELL_MARGIN_HORIZONTAL_EMU,
        "an attribute it does NOT state takes DrawingML's own default, not zero: \
         a cell defaulted to zero margins has its text touching its borders"
    );
}

/// A cell's text goes through the one `a:txBody` reader, so a cell's paragraph
/// carries everything a shape's does.
#[test]
fn a_cells_text_is_the_same_drawingml_text_body_a_shape_carries() {
    let table = fixture_table();
    let body = table.rows[0].cells[0]
        .text
        .as_ref()
        .expect("the origin cell carries text");
    assert_eq!(
        body.paragraphs.len(),
        1,
        "one a:p, read by the reader a p:txBody uses"
    );
    let runs = &body.paragraphs[0].runs;
    assert_eq!(runs.len(), 1, "one a:r");
    assert_eq!(
        runs[0].text(),
        "Spans two",
        "and its a:t arrives verbatim rather than through a second reader"
    );
}
