// SPDX-License-Identifier: Apache-2.0

//! A slide table into the display list, through the **document** engine's table
//! paint path.
//!
//! # There is no second table painter, and that is the whole design
//!
//! `casual-doc-layout` already has an arm for "a table that is not in flow": a
//! positioned DOCX table (`w:tblpPr`) is lifted out of the galley, flowed, and
//! carried as [`AnchorContent::Table`] whose `rows` are the ordinary
//! [`BlockFragment::TableRow`]s an in-flow table produces. `compose_anchor`
//! stacks those rows from the anchor rectangle's origin through `compose_blocks`
//! — the identical function an in-flow table goes through — so cell borders, cell
//! shading, vertical-merge boxes, cell margins, vertical alignment and the cells'
//! own paragraph content all paint through code that has no notion of where the
//! rectangle came from.
//!
//! A slide table is exactly that case: a rectangle computed by somebody else,
//! holding rows. So this module produces `BlockFragment::TableRow` values and
//! nothing else. It contains no painter, no border-conflict resolution and no
//! glyph work.
//!
//! ## What a slide table genuinely cannot reuse, and why
//!
//! `casual_doc_layout::table_float` resolves and places a positioned table, and
//! that half is **not** reusable — not because a slide is special, but because
//! every input it takes is a `Document`: it looks a `BlockNode::Table` up by body
//! index, flows it with `flow::build_galley_for_blocks_inner`, and resolves a
//! `TableFloatPosition` against a page/margin/text reference frame with wrap
//! rectangles and inter-float displacement. A slide has no body index, no
//! `BlockNode`, no page reference frame and nothing to wrap. The `p:xfrm` IS the
//! placement, already resolved.
//!
//! So the seam this crate consumes is one layer lower than `table_float`: the
//! `BlockFragment`/`CellFragment` vocabulary and `AnchorContent::Table`, both
//! already public. Lifting the *cell geometry* arithmetic out of
//! `casual-doc-layout`'s flow would be better still, and it is named here rather
//! than done because that arithmetic is `pub(crate)` inside `flow.rs` and keyed
//! to `w:tcPr`'s vocabulary — a `w:tcW` with its unit enum, `w:tblCellSpacing`,
//! `w:cnfStyle`, border-conflict resolution between two abutting `w:tcBorders` —
//! none of which an `a:tcPr` has. **That is the upstream change this lane would
//! ask for**; it is recorded in the hand-back rather than made, because
//! `casual-doc-layout` belongs to another lane.
//!
//! # Units
//!
//! Everything arriving here is EMU and everything leaving is twips. The
//! conversion happens once per value, at the point of use, through
//! `casual_doc_layout::units`' own helpers — never by hand, because 635 EMU to
//! the twip written out as arithmetic is the conversion that gets inverted.
//!
//! # What this does NOT do, stated rather than left ambiguous
//!
//! * **A table style is not applied.** `a:tblPr`'s six flags and the
//!   `a:tableStyleId` GUID arrive and resolve to a `TableStyle` entry, and that
//!   entry carries an id and a name — `casual-pres-import` reports every
//!   formatting part of it as lost. So a styled table paints its CELLS' own
//!   fills and borders and nothing the style would have added. That is the
//!   largest remaining gap in a slide table and it is a model gap, not a layout
//!   one.
//! * **`@rtl` is not applied.** Grid column zero is painted leftmost whatever
//!   `a:tblPr@rtl` says. Mirroring the grid also mirrors each cell's `a:lnL`/
//!   `a:lnR` and its `@marL`/`@marR`, and doing half of that is worse than doing
//!   none: a right-to-left table with left-to-right borders looks like a bug
//!   rather than a gap.
//! * **A row is never split.** There is no pagination on a slide, so
//!   `BlockFragment::TableRow::can_split` is `false` for every row and
//!   `header` is `false` — a repeated header row is a property of crossing a page
//!   boundary and a slide has none.
//! * **A cell's diagonal rule** (`a:lnTlToBr`/`a:lnBlToTr`) has no field and no
//!   primitive; the importer reports it.

use casual_doc_layout::block::{
    BlockFragment, CellBorderReserve, CellBorders, CellContentMargins, CellFragment, CellVAlign,
    CellVerticalMerge, ResolvedEdge,
};
use casual_doc_layout::page::AnchorContent;
use casual_doc_layout::text::LineShaper;
use casual_doc_layout::units::{Twip, emu_to_twip_extent};
use casual_doc_model::v1::{Fill, ShapeStroke};
use casual_pres_model::{
    CellMerge, Presentation, Slide, SlideNode, SlideTable, TableCell, TextAnchor,
};

use crate::text::{self, UnresolvedTextProperty};

/// Flows a slide table into the display list's positioned-table content.
///
/// It takes **no rectangle**, and that is deliberate rather than an oversight. The
/// shared `GroupChild` walk computes the `p:graphicFrame`'s rectangle and the
/// caller uses its ORIGIN; the SIZE comes from [`width`] and [`authored_height`],
/// because a table's width is the sum of its grid and its height the sum of its
/// rows, and PowerPoint re-derives the frame's `a:ext` from those rather than the
/// other way round. A frame whose `a:ext` has gone stale therefore paints at the
/// size its grid states, which is what PowerPoint shows. Taking a rectangle here
/// and ignoring its size would invite a later caller to believe it was honoured.
///
/// `None` when the table has no grid columns, which the model already refuses —
/// so this is a belt on a braces rather than a case a validated deck reaches.
///
/// # Complexity
///
/// O(cells), in three passes over the same rows: build, resolve the row heights,
/// then spend the merged heights. Three passes rather than one because a merge
/// origin's box height is the sum of row heights that are not known until every
/// row in its span has been measured — the document flow does the same thing for
/// the same reason.
pub(crate) fn flow(
    presentation: &Presentation,
    slide: &Slide,
    frame: &SlideNode,
    table: &SlideTable,
    shaper: &dyn LineShaper,
    report: &mut Vec<UnresolvedTextProperty>,
) -> Option<AnchorContent> {
    if table.grid.is_empty() {
        return None;
    }
    let columns = column_edges(table);
    // The four tiers under a cell belong to the FRAME; the cell supplies tier 5
    // from its own `a:txBody/a:lstStyle`. A frame carries no `p:ph`, so the
    // master tier this resolves to is `p:otherStyle` — which is the tier
    // PowerPoint uses for a table's text, so the right answer falls out of the
    // existing cascade rather than needing a table-shaped exception.
    let frame_cascade = presentation.text_cascade(slide, frame);

    // Pass one: every cell's box, content and natural height.
    let mut rows: Vec<Vec<CellFragment>> = Vec::with_capacity(table.rows.len());
    let last_row_index = table.rows.len().saturating_sub(1);
    for (row_index, row) in table.rows.iter().enumerate() {
        let mut fragments: Vec<CellFragment> = Vec::with_capacity(row.cells.len());
        for (column_index, cell) in row.cells.iter().enumerate() {
            // A HORIZONTAL continuation owns nothing and is not emitted: the
            // origin's `grid_span` already covers its columns, exactly as a DOCX
            // row emits one cell per `w:gridSpan` group. A VERTICAL continuation
            // IS emitted, with `CellVerticalMerge::Continue`, because it holds a
            // column position in its own row that the composition needs. Those
            // two are different because the two axes are different, not because
            // the encodings are.
            if cell.horizontal.is_continuation() {
                continue;
            }
            fragments.push(cell_fragment(
                presentation,
                &frame_cascade,
                cell,
                &columns,
                column_index,
                row_index == last_row_index,
                shaper,
                report,
            ));
        }
        rows.push(fragments);
    }

    // Pass two: each row's final height. `a:tr@h` is a MINIMUM — there is no
    // `exact` spelling for an `a:tr`, unlike `w:trHeight` — so the row takes
    // whichever of the authored height and the content height is larger.
    let heights: Vec<Twip> = table
        .rows
        .iter()
        .zip(&rows)
        .map(|(row, fragments)| {
            let authored = emu_to_twip_extent(row.height_emu);
            let content = BlockFragment::cells_content_height(fragments);
            Twip(authored.raw().max(content.raw()).max(1))
        })
        .collect();

    // Pass three: a vertical merge origin's box spans every row it covers, so its
    // height is only knowable now.
    for (row_index, (row, fragments)) in table.rows.iter().zip(rows.iter_mut()).enumerate() {
        let mut emitted = 0_usize;
        for cell in &row.cells {
            if cell.horizontal.is_continuation() {
                continue;
            }
            let Some(fragment) = fragments.get_mut(emitted) else {
                break;
            };
            emitted += 1;
            if let CellMerge::Origin(span) = cell.vertical {
                let span = span as usize;
                let total = heights
                    .iter()
                    .skip(row_index)
                    .take(span)
                    .fold(Twip::ZERO, |sum, height| sum + *height);
                fragment.vertical_merge = CellVerticalMerge::Restart { height: total };
            }
        }
    }

    let fragments: Vec<BlockFragment> = table
        .rows
        .iter()
        .zip(rows)
        .zip(&heights)
        .map(|((row, cells), height)| BlockFragment::TableRow {
            id: row.id,
            table: table.id,
            cells,
            height: *height,
            // A slide has no page boundary, so neither flag can be true: a row
            // split and a repeated header row are both properties of crossing one.
            can_split: false,
            header: false,
            merge_keep_next: false,
            clip: false,
        })
        .collect();
    if fragments.is_empty() {
        return None;
    }
    Some(AnchorContent::Table { rows: fragments })
}

/// The table's width in twips: the sum of its grid, which is the only place an
/// `a:tbl` states a width at all.
///
/// # Complexity
///
/// O(columns).
pub(crate) fn width(table: &SlideTable) -> Twip {
    column_edges(table)
        .last()
        .copied()
        .map_or(Twip(1), |edge| Twip(edge.raw().max(1)))
}

/// The table's height in twips, at the authored row heights.
///
/// This is the frame rectangle's height BEFORE any cell's content grows a row,
/// and it is deliberately not the laid-out height: the laid-out height is only
/// known once the shaper has run, and the anchor rectangle has to exist before
/// the content is flowed into it. A row that grows therefore paints past the
/// rectangle's bottom edge, which is what PowerPoint shows for a table whose
/// `p:xfrm` is stale — and `AnchorContent::Table` carries no clip, so nothing is
/// cut off.
///
/// # Complexity
///
/// O(rows).
pub(crate) fn authored_height(table: &SlideTable) -> Twip {
    let total = table
        .rows
        .iter()
        .map(|row| emu_to_twip_extent(row.height_emu))
        .fold(Twip::ZERO, |sum, height| sum + height);
    Twip(total.raw().max(1))
}

/// The cumulative right edge of each grid column, in twips, with `edges[i]` the
/// left edge of column `i + 1`.
///
/// Cumulative rather than per-column so a cell's `x` is one index rather than a
/// sum inside a loop over cells — the same reason `CellFragment` carries `x`
/// rather than a column index.
///
/// # Complexity
///
/// O(columns).
fn column_edges(table: &SlideTable) -> Vec<Twip> {
    let mut edges = Vec::with_capacity(table.grid.len() + 1);
    let mut running = Twip::ZERO;
    edges.push(running);
    for column in &table.grid {
        running = running + emu_to_twip_extent(column.width_emu);
        edges.push(running);
    }
    edges
}

/// One cell's laid-out fragment: its box, its margins, its borders, its
/// alignment and its flowed content.
#[allow(
    clippy::too_many_arguments,
    reason = "each argument is an independent input; bundling them would add a \
              lifetime without reducing what has to be threaded"
)]
fn cell_fragment(
    presentation: &Presentation,
    frame_cascade: &casual_pres_model::TextCascade<'_>,
    cell: &TableCell,
    columns: &[Twip],
    column_index: usize,
    last_in_table: bool,
    shaper: &dyn LineShaper,
    report: &mut Vec<UnresolvedTextProperty>,
) -> CellFragment {
    let span = match cell.horizontal {
        CellMerge::Origin(span) => span.max(1),
        // A vertical continuation still occupies its one grid column.
        CellMerge::None | CellMerge::Continuation => 1,
    };
    let start = columns.get(column_index).copied().unwrap_or(Twip::ZERO);
    let end = columns
        .get(column_index + span as usize)
        .or_else(|| columns.last())
        .copied()
        .unwrap_or(start);
    let width = Twip((end.raw() - start.raw()).max(1));

    let properties = &cell.properties;
    let margins = CellContentMargins {
        top: emu_to_twip_extent(properties.margin_top_emu),
        start: emu_to_twip_extent(properties.margin_left_emu),
        bottom: emu_to_twip_extent(properties.margin_bottom_emu),
        end: emu_to_twip_extent(properties.margin_right_emu),
    };
    let borders = CellBorders {
        top: edge(properties.border_top.as_ref()),
        start: edge(properties.border_left.as_ref()),
        bottom: edge(properties.border_bottom.as_ref()),
        end: edge(properties.border_right.as_ref()),
        top_segments: Vec::new(),
        bottom_segments: Vec::new(),
    };
    // The bottom edge's share is conditional on the row being the table's last,
    // and that rule is `CellBorderReserve`'s, not this crate's: a horizontal
    // boundary between two rows is ONE edge and charging both cells for it
    // double-counts every interior rule. Resolved through the shared constructor
    // so a slide table and a DOCX table reserve the same band.
    let border_reserve = CellBorderReserve::resolve(&borders, last_in_table);

    // The content box: the cell width less its own margins and the width its
    // vertical borders take. Same subtraction the document flow makes, so a line
    // breaks at the same place in a table on a slide and in a table in a document.
    let inner = Twip(
        (width.raw() - margins.start.raw() - margins.end.raw() - border_reserve.horizontal().raw())
            .max(1),
    );
    let blocks = match cell.text.as_ref() {
        // A horizontal continuation is not emitted at all and a VERTICAL one is
        // emitted empty: the origin above owns the content, so flowing the
        // continuation's own `a:txBody` — which PowerPoint writes as an empty
        // `<a:p/>` — would be flowing a blank at a cost.
        Some(body) if !cell.vertical.is_continuation() => {
            let cascade = frame_cascade.with_shape_tier(Some(&body.list_style));
            let prepared = text::prepare_body(presentation, &cascade, cell.id, body);
            text::flow_blocks(&prepared, inner, shaper, report).0
        }
        _ => Vec::new(),
    };

    CellFragment {
        id: cell.id,
        grid_span: span,
        x: start,
        width,
        cell_spacing: casual_doc_layout::block::CellBoxSpacing::default(),
        blocks,
        margins,
        vertical_alignment: vertical_alignment(properties.anchor),
        // Set to `Restart` by the caller's third pass for an origin, because the
        // merged height is not knowable until every covered row is measured.
        vertical_merge: if cell.vertical.is_continuation() {
            CellVerticalMerge::Continue
        } else {
            CellVerticalMerge::None
        },
        borders,
        border_reserve,
        // Empty: a slide table has no separated-cell mode, so the table
        // perimeter never paints as a second layer over a grid slot.
        table_borders: CellBorders::default(),
        shading: shading(properties.fill.as_ref()),
    }
}

/// An `a:lnL`-style stroke as a resolved border edge.
///
/// A zero-width stroke is NOT an edge: `<a:lnL w="0">` with a colour is
/// DrawingML's spelling of a hairline that paints nothing, and admitting it would
/// reserve a band of zero and then ask the backend to stroke it.
fn edge(stroke: Option<&ShapeStroke>) -> Option<ResolvedEdge> {
    let stroke = stroke?;
    if stroke.width_emu <= 0 {
        return None;
    }
    Some(ResolvedEdge {
        color: [
            stroke.color.r,
            stroke.color.g,
            stroke.color.b,
            stroke.color.a,
        ],
        width: emu_to_twip_extent(stroke.width_emu),
        // `a:prstDash` carries fourteen tokens and `BorderPattern` five, so the
        // mapping is lossy in ONE direction only: every pattern that has a
        // display-list primitive is used, and the rest fall back to solid rather
        // than to nothing. A dashed rule painted solid is wrong; a dashed rule
        // not painted at all is worse, and the loss is already reported on the
        // import side as part of `a:ln`'s own dash handling.
        pattern: pattern(stroke.dash),
    })
}

/// `a:prstDash@val` as the display list's border pattern.
fn pattern(
    dash: Option<casual_doc_model::v1::DashStyle>,
) -> casual_doc_layout::block::BorderPattern {
    use casual_doc_layout::block::BorderPattern;
    use casual_doc_model::v1::DashStyle;
    match dash {
        Some(DashStyle::Dot | DashStyle::SystemDot) => BorderPattern::Dotted,
        Some(DashStyle::Dash | DashStyle::LargeDash | DashStyle::SystemDash) => {
            BorderPattern::Dashed
        }
        Some(DashStyle::DashDot | DashStyle::LargeDashDot | DashStyle::SystemDashDot) => {
            BorderPattern::DotDash
        }
        Some(DashStyle::LargeDashDotDot) => BorderPattern::DotDotDash,
        Some(DashStyle::Solid) | None => BorderPattern::Solid,
        #[allow(
            unreachable_patterns,
            reason = "DashStyle is not exhaustive to this crate's knowledge; a \
                      token added upstream must paint solid rather than fail to build"
        )]
        Some(_) => BorderPattern::Solid,
    }
}

/// `a:tcPr@anchor` as the document engine's cell vertical alignment.
///
/// `just` and `dist` distribute the content through the cell rather than placing
/// it, which needs per-paragraph spacing this slice does not compute — so both
/// take the top, which is where a distributed block starts anyway. The same
/// decision `text::flow` already makes for `a:bodyPr@anchor`, and made the same
/// way so a cell and a shape agree.
fn vertical_alignment(anchor: TextAnchor) -> CellVAlign {
    match anchor {
        TextAnchor::Top | TextAnchor::Justify | TextAnchor::Distribute => CellVAlign::Top,
        TextAnchor::Center => CellVAlign::Center,
        TextAnchor::Bottom => CellVAlign::Bottom,
    }
}

/// A cell's `a:solidFill` as the shading the composition paints behind it.
///
/// A gradient fill resolves to `None` rather than to its first stop: the display
/// list's cell shading is one colour, and painting a gradient's first stop over
/// the whole cell is a wrong answer where `None` is a missing one.
fn shading(fill: Option<&Fill>) -> Option<[u8; 4]> {
    match fill {
        Some(Fill::Solid(color)) => Some([color.r, color.g, color.b, color.a]),
        _ => None,
    }
}
