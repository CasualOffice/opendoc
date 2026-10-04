// SPDX-License-Identifier: Apache-2.0

//! `p:graphicFrame` and the `a:tbl` inside it, plus the deck's
//! `tableStyles.xml`.
//!
//! # The frame is a shape; only its payload is new
//!
//! A `p:graphicFrame` is `p:nvGraphicFramePr` + `p:xfrm` + `a:graphic`. The first
//! two are the same non-visual properties and the same box every `p:sp` carries,
//! so they go through [`read_non_visual`](crate::shapes) and import as the same
//! `GroupChild::Shape` — which is what lets `casual-pres-layout` compute a
//! frame's rectangle through the SHARED placement walk with no table arm in it.
//! Only the payload is genuinely new, and it hangs off `SlideNode::table`.
//!
//! The frame's shape is given **no fill and no stroke**: a `p:graphicFrame` has no
//! `a:solidFill` and no `a:ln` in its schema, so painting one would be inventing
//! chrome the file does not ask for.
//!
//! `p:xfrm`, not `a:xfrm`. The local names of its children are the same (`a:off`,
//! `a:ext`) and this reader matches on local names, so the prefix costs nothing —
//! but the element is PresentationML's and looking for `a:xfrm` finds nothing.
//! The schema does admit `@rot`/`@flipH`/`@flipV` on it (it is an
//! `a:CT_Transform2D`), and PowerPoint never writes them on a frame because a
//! table cannot be rotated in its UI. They are therefore **reported rather than
//! applied**: claiming they cannot occur would be a claim about producers rather
//! than about the schema.
//!
//! # Only a table arrives
//!
//! `a:graphicData` also carries a `c:chart`, a `dgm:relIds` (SmartArt) and an
//! `p:oleObj`. `docs/156` §8 puts charts and SmartArt with `docs/155`/ADR-050, so
//! each is reported by the local name of the payload element — `chart`,
//! `relIds`, `oleObj` — together with `graphicData/@uri`. `graphicFrame` itself
//! is NOT reported any more, and that is a true statement rather than a
//! convenience: the frame's position, name and hidden flag now arrive in every
//! case, so the loss is the payload's and the report names the payload.
//!
//! # The two encodings of a merge
//!
//! `casual_pres_model::CellMerge`'s own documentation carries the full argument.
//! The reader's half is one rule: `@gridSpan`/`@rowSpan` produce
//! [`CellMerge::Origin`] and `@hMerge`/`@vMerge` produce
//! [`CellMerge::Continuation`], never both for one axis, and a cell stating both
//! is reported and read as the CONTINUATION — because a covered cell owns no
//! content, so treating it as an origin is the error that duplicates text, while
//! treating a mislabelled origin as covered loses at most one cell's box.
//!
//! # Self-closing elements carry their whole meaning in their attributes
//!
//! `<a:gridCol w="1828800"/>`, `<a:tc gridSpan="2"/>` and `<a:tr h="370840"/>`
//! are all written self-closing by real producers, and every attribute above is
//! the only thing those elements say. So every attribute here is read
//! **unconditionally**, before any decision about children, and children are
//! entered only through [`enter`], which refuses to consume a sibling's events.
//!
//! # Units
//!
//! `a:gridCol@w`, `a:tr@h`, the four `a:tcPr` margins and `a:lnL@w` are all
//! **EMU**. There is no twip, point or half-point anywhere in this file.

use casual_doc_model::v1::{Extent, GroupChild, GroupShape, PointEmu, ShapeGeometry};
use casual_pres_model::{
    CellMerge, SlideNode, SlidePaint, SlideTable, TableCell, TableCellProperties, TableGridColumn,
    TableProperties, TableRow, TableStyle, TableStyles, TextAnchor,
};
use quick_xml::events::BytesStart;

use crate::ImportError;
use crate::color::{FillRead, read_fill_child, read_line};
use crate::ids::Ids;
use crate::limits::ImportLimits;
use crate::loss::Reporter;
use crate::shapes::read_non_visual;
use crate::theme::Resolver;
use crate::xml::{
    Cursor, attribute, boolean_attribute, children, enter, integer_attribute, local_name,
};

/// Reads one `p:graphicFrame`, having just entered it.
///
/// Returns the frame as a `SlideNode` whatever its payload turns out to be: a
/// positioned, unpainted box carrying the frame's name and hidden flag, with
/// `SlideNode::table` populated only for an `a:tbl`.
///
/// # Complexity
///
/// O(cells in the table), one pass, bounded by `ImportLimits::max_table_rows` and
/// `max_table_columns`.
pub(crate) fn read_graphic_frame(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    ids: &mut Ids,
    resolver: Resolver,
) -> Result<Option<SlideNode>, ImportError> {
    let part = cursor.part().to_owned();
    let id = ids.next()?;
    let mut non_visual = None;
    let mut offset = PointEmu { x_emu: 0, y_emu: 0 };
    let mut extent = Extent {
        width_emu: 0,
        height_emu: 0,
    };
    let mut stated_transform = false;
    let mut table: Option<SlideTable> = None;

    children(cursor, |cursor, element, empty| {
        let local = local_name(element);
        match local {
            b"nvGraphicFramePr" => {
                if empty {
                    return Ok(false);
                }
                non_visual = Some(read_non_visual(cursor, reporter)?);
                Ok(true)
            }
            b"xfrm" => {
                stated_transform = true;
                // The three attributes the schema admits and PowerPoint does not
                // write. Reported, not applied: the frame's box is axis-aligned
                // here, and a rotated table painted upright is a lie the report
                // should carry.
                for name in [b"rot".as_slice(), b"flipH".as_slice(), b"flipV".as_slice()] {
                    if attribute(element, name, cursor.part())?.is_some() {
                        reporter.degraded_attribute(&part, b"xfrm", name);
                    }
                }
                enter(cursor, empty, |cursor, child, _child_empty| {
                    match local_name(child) {
                        b"off" => {
                            offset = PointEmu {
                                x_emu: integer_attribute(child, b"x", cursor.part())?.unwrap_or(0),
                                y_emu: integer_attribute(child, b"y", cursor.part())?.unwrap_or(0),
                            };
                        }
                        b"ext" => {
                            extent = Extent {
                                width_emu: integer_attribute(child, b"cx", cursor.part())?
                                    .unwrap_or(0),
                                height_emu: integer_attribute(child, b"cy", cursor.part())?
                                    .unwrap_or(0),
                            };
                        }
                        _ => {}
                    }
                    Ok(false)
                })
            }
            b"graphic" => enter(cursor, empty, |cursor, child, child_empty| {
                if local_name(child) != b"graphicData" {
                    return Ok(false);
                }
                let uri = attribute(child, b"uri", cursor.part())?;
                enter(cursor, child_empty, |cursor, payload, payload_empty| {
                    match local_name(payload) {
                        b"tbl" => {
                            if payload_empty {
                                // An `a:tbl` with no grid and no rows: nothing to
                                // place, and `SlideTable::validate` would refuse
                                // the empty grid anyway.
                                reporter.invalid(&part, b"tbl");
                                return Ok(false);
                            }
                            table = Some(read_table(cursor, reporter, ids, resolver)?);
                            Ok(true)
                        }
                        other => {
                            // A chart, a SmartArt diagram or an OLE object. The
                            // payload is the loss, so the payload is what is
                            // named — and the `@uri` goes with it, because
                            // `relIds` alone does not say "SmartArt".
                            reporter.omitted(&part, other);
                            if uri.is_some() {
                                reporter.degraded_attribute(&part, b"graphicData", b"uri");
                            }
                            Ok(false)
                        }
                    }
                })
            }),
            b"extLst" => Ok(false),
            other => {
                reporter.omitted(&part, other);
                Ok(false)
            }
        }
    })?;

    if !stated_transform {
        // Unlike a `p:sp`, a frame has no placeholder cascade to fall back on in
        // practice — PowerPoint writes a full `p:xfrm` on every inserted table —
        // so an absent one is a malformed frame rather than inheritance.
        reporter.degraded_attribute(&part, b"graphicFrame", b"xfrm");
    }
    let non_visual = non_visual.unwrap_or_default();
    Ok(Some(SlideNode {
        placeholder: non_visual.placeholder,
        name: non_visual.name,
        hidden: non_visual.hidden,
        // A `p:graphicFrame` has no `p:spPr` and so states nothing about either:
        // `CT_GraphicalObjectFrame` is a `p:xfrm` and a payload, with no fill or
        // outline element in its content model at all. `Inherited` is the reading
        // of an element that cannot be written, not a default reached for.
        fill: SlidePaint::Inherited,
        outline: SlidePaint::Inherited,
        content: GroupChild::Shape(GroupShape {
            id,
            offset,
            extent,
            // A frame has no geometry element at all. `Rectangle` is the box it
            // occupies, and with no fill and no stroke it paints nothing — the
            // frame is a position, not a drawing.
            geometry: ShapeGeometry::Rectangle,
            preset: None,
            adjustments: Vec::new(),
            path: None,
            fill: None,
            stroke: None,
            flip_h: false,
            flip_v: false,
            rotation: None,
            hyperlink: None,
        }),
        text: None,
        table,
    }))
}

/// Reads an `a:tbl`, having just entered it.
fn read_table(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    ids: &mut Ids,
    resolver: Resolver,
) -> Result<SlideTable, ImportError> {
    let part = cursor.part().to_owned();
    let limits = cursor.limits();
    let id = ids.next()?;
    let mut properties = TableProperties::default();
    let mut grid: Vec<TableGridColumn> = Vec::new();
    let mut rows: Vec<TableRow> = Vec::new();

    children(cursor, |cursor, element, empty| {
        let local = local_name(element);
        match local {
            b"tblPr" => {
                properties = read_table_properties(cursor, reporter, element, empty)?;
                Ok(!empty)
            }
            b"tblGrid" => {
                if empty {
                    // No columns at all. Reported here rather than left to
                    // `SlideTable::validate`, which fails the whole import.
                    reporter.invalid(&part, b"tblGrid");
                    return Ok(false);
                }
                grid = read_grid(cursor, reporter, limits)?;
                Ok(true)
            }
            b"tr" => {
                // Every attribute first, children second: `<a:tr h="370840"/>` is
                // a legal empty row and its height is all it says.
                let height_emu = integer_attribute(element, b"h", cursor.part())?.unwrap_or(0);
                if rows.len() >= limits.max_table_rows {
                    reporter.invalid(&part, b"tr");
                    return Ok(false);
                }
                let row_id = ids.next()?;
                let mut cells: Vec<TableCell> = Vec::new();
                let consumed = enter(cursor, empty, |cursor, child, child_empty| {
                    if local_name(child) != b"tc" {
                        return Ok(false);
                    }
                    if cells.len() >= limits.max_table_columns {
                        reporter.invalid(&part, b"tc");
                        return Ok(false);
                    }
                    let (cell, consumed) =
                        read_cell(cursor, reporter, ids, child, child_empty, resolver)?;
                    cells.push(cell);
                    Ok(consumed)
                })?;
                rows.push(TableRow {
                    id: row_id,
                    height_emu,
                    cells,
                });
                Ok(consumed)
            }
            b"extLst" => Ok(false),
            other => {
                reporter.omitted(&part, other);
                Ok(false)
            }
        }
    })?;

    reconcile_rows(&mut rows, &grid, reporter, ids, &part)?;
    Ok(SlideTable {
        id,
        properties,
        grid,
        rows,
    })
}

/// Pads or truncates each row to the grid's column count, reporting either.
///
/// PresentationML requires one `a:tc` per `a:gridCol` and `SlideTable::validate`
/// refuses a row that has not got one. Repairing here rather than failing the
/// import is the same trade the `p:clrMap` and `p:spTree` readers already make: a
/// malformed row costs one row's geometry, and refusing the package costs the
/// whole deck.
///
/// A pad cell is UNMERGED, never a continuation — a continuation with no origin
/// is the one thing validation would still refuse, and a reader that padded with
/// one would turn a repair into a second failure.
fn reconcile_rows(
    rows: &mut [TableRow],
    grid: &[TableGridColumn],
    reporter: &mut Reporter,
    ids: &mut Ids,
    part: &str,
) -> Result<(), ImportError> {
    for row in rows {
        if row.cells.len() == grid.len() {
            continue;
        }
        reporter.invalid(part, b"tr");
        while row.cells.len() > grid.len() {
            row.cells.pop();
        }
        while row.cells.len() < grid.len() {
            row.cells.push(TableCell::new(ids.next()?));
        }
    }
    Ok(())
}

/// Reads `a:tblPr`: the six conditional-format flags, `@rtl`, and the style GUID.
///
/// The attributes are read whether or not the element has children, because
/// `<a:tblPr firstRow="1" bandRow="1"/>` is what PowerPoint writes for a table
/// with a style and no explicit fill — and answering `default()` for it is
/// exactly the self-closing bug this crate's `enter` exists for.
fn read_table_properties(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    element: &BytesStart<'_>,
    empty: bool,
) -> Result<TableProperties, ImportError> {
    let part = cursor.part().to_owned();
    let flag = |name: &[u8]| -> Result<bool, ImportError> {
        Ok(boolean_attribute(element, name, &part)?.unwrap_or(false))
    };
    let mut properties = TableProperties {
        first_row: flag(b"firstRow")?,
        last_row: flag(b"lastRow")?,
        first_column: flag(b"firstCol")?,
        last_column: flag(b"lastCol")?,
        banded_rows: flag(b"bandRow")?,
        banded_columns: flag(b"bandCol")?,
        rtl: flag(b"rtl")?,
        style_id: None,
    };
    if empty {
        return Ok(properties);
    }
    children(cursor, |cursor, child, child_empty| {
        let local = local_name(child);
        match local {
            b"tableStyleId" => {
                if child_empty {
                    reporter.invalid(&part, b"tableStyleId");
                    return Ok(false);
                }
                // The GUID with its braces, verbatim. `{5C22544A-…}` is one token
                // and `tableStyles.xml` spells `a:tblStyle@styleId` the same way,
                // so trimming the braces breaks the join silently.
                let text = cursor.read_text(MAX_STYLE_ID_BYTES)?;
                let text = text.trim();
                if text.is_empty() {
                    reporter.invalid(&part, b"tableStyleId");
                } else {
                    properties.style_id = Some(text.to_owned());
                }
                Ok(true)
            }
            // A table may state its own fill, outline and effects, and an `a:tbl`
            // has no field for any of them: the grid's appearance comes from the
            // style or from each cell.
            b"fill" | b"fillRef" | b"lnRef" | b"effectRef" | b"fontRef" | b"effectLst" => {
                reporter.omitted(&part, local);
                Ok(false)
            }
            b"extLst" => Ok(false),
            other => {
                reporter.omitted(&part, other);
                Ok(false)
            }
        }
    })?;
    Ok(properties)
}

/// The ceiling on an `a:tableStyleId`'s text. A GUID in braces is 38 bytes; the
/// slack admits a producer that writes a name instead without admitting a part
/// that hides a megabyte in one element.
const MAX_STYLE_ID_BYTES: usize = 256;

/// Reads `a:tblGrid`'s `a:gridCol` children.
fn read_grid(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    limits: ImportLimits,
) -> Result<Vec<TableGridColumn>, ImportError> {
    let part = cursor.part().to_owned();
    let mut grid = Vec::new();
    children(cursor, |cursor, element, _empty| {
        let local = local_name(element);
        if local != b"gridCol" {
            reporter.omitted(&part, local);
            return Ok(false);
        }
        if grid.len() >= limits.max_table_columns {
            reporter.invalid(&part, b"gridCol");
            return Ok(false);
        }
        // `<a:gridCol w="1828800"/>` is the shape every producer writes, and `@w`
        // is the only thing it says. Read unconditionally, never `default()`.
        grid.push(TableGridColumn {
            width_emu: integer_attribute(element, b"w", cursor.part())?.unwrap_or(0),
        });
        Ok(false)
    })?;
    Ok(grid)
}

/// Reads one `a:tc`, returning the cell and whether its subtree was consumed.
///
/// The merge attributes are read from the start tag **before** anything else, so
/// `<a:tc gridSpan="2"/>` — a self-closing origin, which is exactly what a
/// producer writes for a merged cell with no content — keeps its span.
fn read_cell(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    ids: &mut Ids,
    element: &BytesStart<'_>,
    empty: bool,
    resolver: Resolver,
) -> Result<(TableCell, bool), ImportError> {
    let part = cursor.part().to_owned();
    let id = ids.next()?;
    let horizontal = read_merge(
        reporter,
        element,
        &part,
        b"gridSpan",
        b"hMerge",
        cursor.part(),
    )?;
    let vertical = read_merge(
        reporter,
        element,
        &part,
        b"rowSpan",
        b"vMerge",
        cursor.part(),
    )?;
    let mut cell = TableCell {
        id,
        horizontal,
        vertical,
        properties: TableCellProperties::default(),
        text: None,
    };
    if empty {
        return Ok((cell, false));
    }
    children(cursor, |cursor, child, child_empty| {
        let local = local_name(child);
        match local {
            b"txBody" => {
                if child_empty {
                    return Ok(false);
                }
                // The SAME `a:txBody` reader a shape's text goes through. A second
                // one would diverge at the first property either side gained, and
                // `docs/156` §8 is explicit that a parallel path is evidence the
                // abstraction is wrong.
                cell.text = Some(crate::text::read_text_body(
                    cursor, reporter, ids, resolver,
                )?);
                Ok(true)
            }
            b"tcPr" => {
                cell.properties =
                    read_cell_properties(cursor, reporter, child, child_empty, resolver)?;
                Ok(!child_empty)
            }
            b"extLst" => Ok(false),
            other => {
                reporter.omitted(&part, other);
                Ok(false)
            }
        }
    })?;
    Ok((cell, true))
}

/// One axis's merge role, from the span attribute and the continuation attribute.
///
/// ONE function used twice rather than one per axis: the rule is identical and
/// two copies of it is how `@rowSpan` and `@vMerge` end up conflated. See the
/// module header for why a conflict resolves to the continuation.
fn read_merge(
    reporter: &mut Reporter,
    element: &BytesStart<'_>,
    report_part: &str,
    span_attribute: &[u8],
    continuation_attribute: &[u8],
    part: &str,
) -> Result<CellMerge, ImportError> {
    let span = integer_attribute(element, span_attribute, part)?;
    let continues = boolean_attribute(element, continuation_attribute, part)?.unwrap_or(false);
    if continues {
        if span.is_some_and(|span| span > 1) {
            // Both encodings on one axis. A producer writes one or the other; a
            // file writing both is contradicting itself, and the continuation is
            // the safer reading.
            reporter.degraded_attribute(report_part, b"tc", span_attribute);
        }
        return Ok(CellMerge::Continuation);
    }
    match span {
        // `@gridSpan="1"` IS the default, not a one-cell merge, so it reads as
        // unmerged — admitting it would give one fact two models.
        Some(span) if span > 1 => Ok(CellMerge::Origin(u32::try_from(span).unwrap_or(u32::MAX))),
        _ => Ok(CellMerge::None),
    }
}

/// Reads `a:tcPr`: the four EMU margins, `@anchor`, the cell fill and the four
/// border lines.
fn read_cell_properties(
    cursor: &mut Cursor<'_>,
    reporter: &mut Reporter,
    element: &BytesStart<'_>,
    empty: bool,
    resolver: Resolver,
) -> Result<TableCellProperties, ImportError> {
    let part = cursor.part().to_owned();
    let defaults = TableCellProperties::default();
    // EMU, and each attribute's own schema default when absent — not zero. A
    // `<a:tcPr anchor="ctr"/>` is self-closing and states its anchor, which is
    // the whole point of reading the attributes before the children.
    let margin = |name: &[u8], default: i64| -> Result<i64, ImportError> {
        Ok(integer_attribute(element, name, &part)?.unwrap_or(default))
    };
    let mut properties = TableCellProperties {
        margin_left_emu: margin(b"marL", defaults.margin_left_emu)?,
        margin_right_emu: margin(b"marR", defaults.margin_right_emu)?,
        margin_top_emu: margin(b"marT", defaults.margin_top_emu)?,
        margin_bottom_emu: margin(b"marB", defaults.margin_bottom_emu)?,
        anchor: attribute(element, b"anchor", &part)?
            .as_deref()
            .map_or(TextAnchor::Top, anchor_from_token),
        ..defaults
    };
    if attribute(element, b"vert", &part)?.is_some() {
        // A cell's text direction has no field: `TableCellProperties` carries the
        // anchor and nothing rotates a cell's content yet.
        reporter.degraded_attribute(&part, b"tcPr", b"vert");
    }
    if empty {
        return Ok(properties);
    }
    let mut fill = FillRead::default();
    children(cursor, |cursor, child, child_empty| {
        let local = local_name(child);
        let edge = match local {
            b"lnL" => Some(&mut properties.border_left),
            b"lnR" => Some(&mut properties.border_right),
            b"lnT" => Some(&mut properties.border_top),
            b"lnB" => Some(&mut properties.border_bottom),
            b"lnTlToBr" | b"lnBlToTr" => {
                // The two diagonals. No field, and they are visible: a crossed-out
                // cell that reopens blank is loss.
                reporter.omitted(&part, local);
                None
            }
            b"headers" | b"extLst" => None,
            _ => None,
        };
        if let Some(edge) = edge {
            // `a:lnL` IS an `a:CT_LineProperties` — the same element `a:ln` on a
            // `p:spPr` is — so it goes through the one line reader. `@w` is in EMU.
            let line = read_line(cursor, reporter, child, child_empty, resolver)?;
            if line.state.suppresses() {
                // The edge-level half of the `a:noFill` distinction, and the one
                // place it is still a LOSS: a `p:spPr` carries it on the
                // `SlideNode`, while `TableCellProperties` has four
                // `Option<ShapeStroke>` edges and no room for "explicitly no edge".
                // It matters for the same reason it matters on a shape — a cell
                // that states no left border must not inherit the table style's —
                // so it is reported rather than conflated with an absent `a:lnL`.
                //
                // Charged to the EDGE's own name rather than to `ln`, so a cell's
                // suppressed border is distinguishable in the report from a shape's
                // suppressed outline: they are different losses with different
                // owners, and one name for both would make a deck look like it lost
                // the same thing twice.
                reporter.degraded_attribute(&part, local, b"noFill");
            }
            *edge = line.stroke;
            return Ok(!child_empty);
        }
        if matches!(local, b"headers" | b"extLst" | b"lnTlToBr" | b"lnBlToTr") {
            return Ok(false);
        }
        // Everything else a `a:tcPr` can hold is a fill, and the shared fill
        // reader already classifies and reports each kind.
        read_fill_child(cursor, reporter, child, child_empty, &mut fill, resolver)
    })?;
    if fill.state.suppresses() {
        // Same gap as the edges above: `TableCellProperties::fill` is an
        // `Option<Fill>`, so a cell that states `a:noFill` is indistinguishable from
        // one that states nothing and will take its table style's band fill.
        reporter.degraded_attribute(&part, b"tcPr", b"noFill");
    }
    properties.fill = fill.fill;
    Ok(properties)
}

/// `a:tcPr@anchor`'s token. The same five values `a:bodyPr@anchor` takes, so the
/// model's own enum is reused rather than a cell-only one declared.
fn anchor_from_token(token: &str) -> TextAnchor {
    match token {
        "ctr" => TextAnchor::Center,
        "b" => TextAnchor::Bottom,
        "just" => TextAnchor::Justify,
        "dist" => TextAnchor::Distribute,
        // `t` and any unrecognized token are both the schema's default.
        _ => TextAnchor::Top,
    }
}

/// Reads `ppt/tableStyles.xml`: `a:tblStyleLst@def` and its `a:tblStyle` entries.
///
/// # What is read and what is reported
///
/// The `@def` GUID, and each entry's `@styleId` and `@styleName`. An entry's
/// formatting parts — `a:wholeTbl`, `a:band1H`, `a:firstRow` and the other eight
/// — are each a `CT_TablePartStyle` with nothing in this build that would apply
/// one, so each is reported by its own local name. Reporting the LIST rather than
/// the parts would understate it: a deck with nine populated parts loses nine
/// things.
///
/// # Complexity
///
/// O(part), one pass, bounded by `ImportLimits::max_table_styles`.
pub(crate) fn read_table_styles_part(
    bytes: &[u8],
    part: &str,
    reporter: &mut Reporter,
    limits: ImportLimits,
) -> Result<TableStyles, ImportError> {
    let mut cursor = Cursor::new(bytes, part, limits);
    let root = cursor.root()?;
    let mut styles = TableStyles {
        // On the ROOT element, so it is read before any child — and a part that is
        // nothing but `<a:tblStyleLst def="{GUID}"/>` (which is what PowerPoint
        // writes for a deck with no inserted table) still yields its default.
        default_style_id: attribute(&root, b"def", part)?.filter(|def| !def.is_empty()),
        styles: Vec::new(),
    };
    children(&mut cursor, |cursor, element, empty| {
        let local = local_name(element);
        if local != b"tblStyle" {
            if local != b"extLst" {
                reporter.omitted(part, local);
            }
            return Ok(false);
        }
        if styles.styles.len() >= limits.max_table_styles {
            reporter.invalid(part, b"tblStyle");
            return Ok(false);
        }
        let Some(id) = attribute(element, b"styleId", cursor.part())?.filter(|id| !id.is_empty())
        else {
            // An entry with no id can never be joined to, so it is not an entry.
            reporter.invalid(part, b"tblStyle");
            return Ok(false);
        };
        styles.styles.push(TableStyle {
            id,
            name: attribute(element, b"styleName", cursor.part())?.filter(|name| !name.is_empty()),
        });
        enter(cursor, empty, |_cursor, child, child_empty| {
            let local = local_name(child);
            // A self-closed part states nothing and loses nothing; a populated one
            // is formatting this build cannot apply.
            if !child_empty && local != b"extLst" {
                reporter.omitted(part, local);
            }
            Ok(false)
        })
    })?;
    Ok(styles)
}
