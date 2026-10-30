// SPDX-License-Identifier: Apache-2.0

//! The table's borders, its position on the page, and drawing a table with the
//! pointer (`docs/153` `table.borders-dialog`, `table.text-wrapping`,
//! `table.position-dialog`, `table.draw`).
//!
//! # What each half is, and the competitive standard
//!
//! * **Borders and shading.** Word's Borders and Shading dialog (Table Design ▸
//!   Borders ▸ Borders and Shading…): a preview whose edges are clicked on and
//!   off — top, bottom, left, right and the two inside lines — preset buttons, a
//!   line style, width and colour, a shading fill, and "Apply to: Table / Cell".
//!   ONLYOFFICE's Table settings ▸ Borders & background is the same shape
//!   (`web-apps/apps/documenteditor/main/app/view/TableSettingsAdvanced.js`, the
//!   `CellBorders`/`TableBorders` preview it builds at `:1290-1330`; read for the
//!   behaviour, nothing copied). [`WasmDocument::table_border_state`] is the read
//!   the dialog opens on — per edge, for the selected cells or the whole table,
//!   INCLUDING the inside lines — and [`WasmDocument::apply_table_borders`] is the
//!   one undoable write.
//! * **Text wrapping and positioning.** Word's Table Properties ▸ Table ▸ Text
//!   wrapping None / Around, and its Positioning… dialog (horizontal and vertical
//!   position, relative to, distance from surrounding text, move with text, allow
//!   overlap). They author `w:tblpPr` and `w:tblOverlap`, which the model, the
//!   importer, the exporter and `casual_doc_layout::table_float` already carry.
//!   "Move with text" is Word's own mapping: vertical position relative to the
//!   paragraph, `w:vertAnchor="text"`.
//! * **Draw table.** Word's Insert ▸ Table ▸ Draw Table (and Layout ▸ Draw
//!   Table): drag a rectangle on the page for a table, then drag lines inside it
//!   to split rows and columns. ONLYOFFICE's pencil is the same gesture
//!   (`sdkjs/word/Editor/Document.js` `DrawTable`). The rectangle becomes a 1 x 1
//!   table positioned where it was drawn (`w:tblpPr` against the page); a line
//!   splits the cells it crosses at the place it was drawn, and leaves the cells
//!   it did not cross whole — the merge Word writes.
//!
//! Every binding has an `_inner` twin returning `Result<_, String>`, because a
//! native test cannot build the `JsValue` the boundary returns.
//!
//! # Complexity
//!
//! Resolving a node is the existing O(document) id lookup every table command
//! makes; the border read and write are O(cells of the table) after it. Drawing
//! a line asks the painted layout for each cell's rectangle, which is
//! O(cells x fragments) — once per drawn line, never on a keystroke or a hover.

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

use casual_doc_edit::table_structure::{normalize_vertical_merges, sync_cell_widths};
use casual_doc_edit::{Operation, Pos, find_table, locate_cell, refused};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, BorderEdge, GridColumn, HeightRule, Paragraph, ParagraphProperties, RgbColor,
    RowHeight, Table, TableAnchor, TableBorders, TableCell, TableCellProperties,
    TableFloatPosition, TableGridMap, TableOverlap, TableWidth, TableXAlign, TableYAlign,
    VerticalMerge,
};
use wasm_bindgen::prelude::*;

use crate::{
    EditResult, HistoryKind, WasmDocument, border_edge, border_style_or_default,
    border_style_token, first_paragraph_of_cell, parse_hex_color, to_js,
};

// ---- Borders ----------------------------------------------------------------

/// The six lines Word's Borders preview draws, in its order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Line {
    Top,
    Bottom,
    Left,
    Right,
    InsideH,
    InsideV,
}

const LINES: [Line; 6] = [
    Line::Top,
    Line::Bottom,
    Line::Left,
    Line::Right,
    Line::InsideH,
    Line::InsideV,
];

impl Line {
    const fn name(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Left => "left",
            Self::Right => "right",
            Self::InsideH => "insideH",
            Self::InsideV => "insideV",
        }
    }

    fn parse(name: &str) -> Option<Self> {
        LINES.into_iter().find(|line| line.name() == name)
    }
}

/// One side of one physical cell (`w:tcBorders`), leading to trailing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Side {
    Top,
    Bottom,
    Start,
    End,
}

const SIDES: [Side; 4] = [Side::Top, Side::Bottom, Side::Start, Side::End];

fn side_ref(borders: &TableBorders, side: Side) -> Option<&BorderEdge> {
    match side {
        Side::Top => borders.top.as_ref(),
        Side::Bottom => borders.bottom.as_ref(),
        Side::Start => borders.start.as_ref(),
        Side::End => borders.end.as_ref(),
    }
}

fn side_mut(borders: &mut TableBorders, side: Side) -> &mut Option<BorderEdge> {
    match side {
        Side::Top => &mut borders.top,
        Side::Bottom => &mut borders.bottom,
        Side::Start => &mut borders.start,
        Side::End => &mut borders.end,
    }
}

fn table_line_mut(borders: &mut TableBorders, line: Line) -> &mut Option<BorderEdge> {
    match line {
        Line::Top => &mut borders.top,
        Line::Bottom => &mut borders.bottom,
        Line::Left => &mut borders.start,
        Line::Right => &mut borders.end,
        Line::InsideH => &mut borders.inside_h,
        Line::InsideV => &mut borders.inside_v,
    }
}

/// A grid rectangle, inclusive: rows `r0..=r1`, grid columns `c0..=c1`.
#[derive(Clone, Copy, Debug)]
struct GridBox {
    r0: usize,
    r1: usize,
    c0: usize,
    c1: usize,
}

/// Whether `edge` paints a line — the layout's own rule (`flow::is_visible_border`).
fn visible(edge: &BorderEdge) -> bool {
    !matches!(edge.style.as_str(), "" | "nil" | "none")
}

/// The explicit "no line here" edge Word writes (`w:val="nil"`), which also wins
/// over a neighbour's or the table's line (`flow::resolve_edge`).
fn nil_edge() -> BorderEdge {
    BorderEdge {
        style: "nil".to_owned(),
        size_eighth_points: None,
        color: None,
        theme_color: None,
        space_points: None,
    }
}

/// `#rrggbb` for a colour; `auto` (no explicit colour) reads as black, which is
/// what it paints.
fn hex(color: Option<RgbColor>) -> String {
    let c = color.unwrap_or(RgbColor { r: 0, g: 0, b: 0 });
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// Which of the six preview lines each side of physical cell `(row, cell)` is,
/// within `area` — `None` for a side that is not on a line of the area (the top
/// of a vertical-merge continuation, which is inside its merged cell), and
/// `None` overall when the cell's merged region is not inside `area`.
fn lines_of(
    grid: &TableGridMap,
    row: usize,
    cell: usize,
    area: GridBox,
) -> Option<[Option<Line>; 4]> {
    let slot = grid.cell(row, cell)?;
    let region = grid.region(slot.region);
    let inside = region.first_row >= area.r0
        && region.last_row <= area.r1
        && region.first_column >= area.c0
        && region.last_column <= area.c1;
    if !inside {
        return None;
    }
    let top = (row == region.first_row).then_some(if region.first_row == area.r0 {
        Line::Top
    } else {
        Line::InsideH
    });
    let bottom = (row == region.last_row).then_some(if region.last_row == area.r1 {
        Line::Bottom
    } else {
        Line::InsideH
    });
    let start = Some(if region.first_column == area.c0 {
        Line::Left
    } else {
        Line::InsideV
    });
    let end = Some(if region.last_column == area.c1 {
        Line::Right
    } else {
        Line::InsideV
    });
    Some([top, bottom, start, end])
}

/// The table-level edge (`w:tblBorders`) that paints side `side` of physical
/// cell `(row, cell)` when the cell says nothing of its own: an outer edge on the
/// table's boundary, an inside line elsewhere.
fn table_edge_for<'a>(
    table: &'a Table,
    grid: &TableGridMap,
    row: usize,
    cell: usize,
    side: Side,
) -> Option<&'a BorderEdge> {
    let slot = grid.cell(row, cell)?;
    let region = grid.region(slot.region);
    let b = &table.properties.borders;
    let last_row = grid.row_count().saturating_sub(1);
    let last_column = grid.column_count().saturating_sub(1);
    match side {
        Side::Top if region.first_row == 0 => b.top.as_ref(),
        Side::Bottom if region.last_row == last_row => b.bottom.as_ref(),
        Side::Start if region.first_column == 0 => b.start.as_ref(),
        Side::End if region.last_column == last_column => b.end.as_ref(),
        Side::Top | Side::Bottom => b.inside_h.as_ref(),
        Side::Start | Side::End => b.inside_v.as_ref(),
    }
}

/// The edge that actually paints a side: the cell's own, unless it is absent or
/// `none` (then the table's), and nothing when the winner is not a visible line —
/// `flow::effective_border` followed by `is_visible_border`.
fn effective<'a>(
    own: Option<&'a BorderEdge>,
    table: Option<&'a BorderEdge>,
) -> Option<&'a BorderEdge> {
    let chosen = match own {
        Some(edge) if matches!(edge.style.as_str(), "" | "none") => table,
        Some(edge) => Some(edge),
        None => table,
    };
    chosen.filter(|edge| visible(edge))
}

/// The cells facing side `side` of the merged region physical cell `(row, cell)`
/// belongs to, as `(row, cell, their side)`: the cells across that boundary.
fn neighbours(
    grid: &TableGridMap,
    row: usize,
    cell: usize,
    side: Side,
) -> Vec<(usize, usize, Side)> {
    let Some(slot) = grid.cell(row, cell) else {
        return Vec::new();
    };
    let region = *grid.region(slot.region);
    let mut out: Vec<(usize, usize, Side)> = Vec::new();
    let mut push = |r: usize, column: usize, facing: Side| {
        if let Some(found) = grid.cell_at(r, column) {
            let entry = (found.row, found.cell, facing);
            if !out.contains(&entry) {
                out.push(entry);
            }
        }
    };
    match side {
        Side::Top => {
            if let Some(above) = region.first_row.checked_sub(1) {
                for column in region.first_column..=region.last_column {
                    push(above, column, Side::Bottom);
                }
            }
        }
        Side::Bottom => {
            let below = region.last_row + 1;
            if below < grid.row_count() {
                for column in region.first_column..=region.last_column {
                    push(below, column, Side::Top);
                }
            }
        }
        Side::Start => {
            if let Some(before) = region.first_column.checked_sub(1) {
                for r in region.first_row..=region.last_row {
                    push(r, before, Side::End);
                }
            }
        }
        Side::End => {
            let after = region.last_column + 1;
            for r in region.first_row..=region.last_row {
                push(r, after, Side::Start);
            }
        }
    }
    out
}

/// The dialog's state for one line: what a reader sees on the page there.
#[derive(Default)]
struct LineState {
    on: usize,
    off: usize,
}

impl LineState {
    fn token(&self) -> &'static str {
        match (self.on, self.off) {
            (0, 0) => "unavailable",
            (_, 0) => "on",
            (0, _) => "off",
            _ => "mixed",
        }
    }
}

/// The pen the dialog opens on: the first visible line, read back.
#[derive(serde::Serialize)]
struct PenJson {
    style: &'static str,
    size: u32,
    color: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct BorderStateJson {
    scope: &'static str,
    rows: usize,
    columns: usize,
    edges: BTreeMap<&'static str, &'static str>,
    pen: Option<PenJson>,
    /// `#rrggbb`, `""` for no fill, or `"mixed"`.
    shading: String,
}

/// What the dialog sends: the lines it changed, the pen, and the shading.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BorderPatch {
    scope: String,
    #[serde(default)]
    edges: BTreeMap<String, String>,
    #[serde(default)]
    style: Option<String>,
    #[serde(default)]
    size: Option<u32>,
    #[serde(default)]
    color: Option<String>,
    /// `"none"` or `#rrggbb`; absent leaves the shading alone.
    #[serde(default)]
    shading: Option<String>,
}

/// The cell's fill as a reader sees it: its own, or the table's when the cell
/// does not say `w:fill="nil"`.
fn painted_fill(table: &Table, properties: &TableCellProperties) -> Option<RgbColor> {
    if properties.shading.fill_none {
        return None;
    }
    properties.shading.fill.or(table.properties.shading.fill)
}

/// The cells' new properties, keyed by `(row, cell)`, seeded from the table.
type CellEdits = BTreeMap<(usize, usize), TableCellProperties>;

fn edit_of<'a>(
    edits: &'a mut CellEdits,
    table: &Table,
    row: usize,
    cell: usize,
) -> &'a mut TableCellProperties {
    edits
        .entry((row, cell))
        .or_insert_with(|| table.rows[row].cells[cell].properties.clone())
}

impl WasmDocument {
    /// The table holding paragraph `anchor`, and the grid box the dialog acts on:
    /// the cell range `anchor..focus` (`""` = the anchor's own cell) for
    /// `"cells"`, the whole table for `"table"`.
    fn border_target(
        &self,
        anchor: &str,
        focus: &str,
        scope: &str,
    ) -> Result<(Table, GridBox), String> {
        let focus = if focus.is_empty() { anchor } else { focus };
        match scope {
            "cells" => {
                let (_, table, range) = self.cell_range_of(anchor, focus)?;
                Ok((
                    table.clone(),
                    GridBox {
                        r0: range.first_row(),
                        r1: range.last_row(),
                        c0: range.first_column(),
                        c1: range.last_column(),
                    },
                ))
            }
            "table" => {
                let nid = NodeId::from_str(anchor).map_err(|_| "invalid node id".to_owned())?;
                let (table, _) = locate_cell(&self.document, nid).ok_or_else(|| {
                    refused!("table.caret-outside", "Put the caret in a table first.").to_owned()
                })?;
                let table = find_table(&self.document, table)
                    .ok_or_else(|| "table not found".to_owned())?
                    .clone();
                let grid = TableGridMap::build(&table);
                Ok((
                    table,
                    GridBox {
                        r0: 0,
                        r1: grid.row_count().saturating_sub(1),
                        c0: 0,
                        c1: grid.column_count().saturating_sub(1),
                    },
                ))
            }
            _ => Err(format!("unknown border scope \u{201c}{scope}\u{201d}")),
        }
    }

    /// See [`WasmDocument::table_border_state`].
    pub(crate) fn table_border_state_inner(
        &self,
        anchor: &str,
        focus: &str,
        scope: &str,
    ) -> Result<String, String> {
        let (table, area) = self.border_target(anchor, focus, scope)?;
        let grid = TableGridMap::build(&table);
        let mut states: BTreeMap<Line, LineState> = BTreeMap::new();
        let mut pen: Option<(Line, BorderEdge)> = None;
        let mut fills: BTreeSet<Option<(u8, u8, u8)>> = BTreeSet::new();
        for r in area.r0..=area.r1 {
            for (c, cell) in table
                .rows
                .get(r)
                .map_or(&[][..], |row| &row.cells[..])
                .iter()
                .enumerate()
            {
                let Some(lines) = lines_of(&grid, r, c, area) else {
                    continue;
                };
                if grid.cell(r, c).is_some_and(|slot| !slot.continuation) {
                    fills.insert(painted_fill(&table, &cell.properties).map(|f| (f.r, f.g, f.b)));
                }
                for (side, line) in SIDES.into_iter().zip(lines) {
                    let Some(line) = line else {
                        continue;
                    };
                    let shown = effective(
                        side_ref(&cell.properties.borders, side),
                        table_edge_for(&table, &grid, r, c, side),
                    );
                    let state = states.entry(line).or_default();
                    match shown {
                        Some(edge) => {
                            state.on += 1;
                            if pen.as_ref().is_none_or(|(at, _)| line < *at) {
                                pen = Some((line, edge.clone()));
                            }
                        }
                        None => state.off += 1,
                    }
                }
            }
        }
        let edges = LINES
            .into_iter()
            .map(|line| {
                (
                    line.name(),
                    states.get(&line).map_or("unavailable", LineState::token),
                )
            })
            .collect();
        let shading = match fills.len() {
            1 => fills
                .first()
                .copied()
                .flatten()
                .map_or_else(String::new, |(r, g, b)| hex(Some(RgbColor { r, g, b }))),
            _ => "mixed".to_owned(),
        };
        let state = BorderStateJson {
            scope: if scope == "table" { "table" } else { "cells" },
            rows: area.r1 - area.r0 + 1,
            columns: area.c1 - area.c0 + 1,
            edges,
            pen: pen.map(|(_, edge)| PenJson {
                style: border_style_token(&edge.style).unwrap_or("single"),
                size: edge.size_eighth_points.unwrap_or(4),
                color: hex(edge.color),
            }),
            shading,
        };
        serde_json::to_string(&state).map_err(|error| error.to_string())
    }

    /// See [`WasmDocument::apply_table_borders`].
    pub(crate) fn apply_table_borders_inner(
        &mut self,
        anchor: &str,
        focus: &str,
        patch: &str,
    ) -> Result<EditResult, String> {
        let patch: BorderPatch = serde_json::from_str(patch)
            .map_err(|error| format!("invalid border settings: {error}"))?;
        let style = border_style_or_default(patch.style.as_deref())?;
        let color = match patch.color.as_deref() {
            Some(text) => parse_hex_color(text).ok_or_else(|| format!("invalid colour {text}"))?,
            None => RgbColor { r: 0, g: 0, b: 0 },
        };
        let pen = border_edge(style, color.r, color.g, color.b, patch.size.unwrap_or(4));
        let mut changes: BTreeMap<Line, bool> = BTreeMap::new();
        for (name, value) in &patch.edges {
            let line = Line::parse(name).ok_or_else(|| format!("unknown border line {name}"))?;
            let on = match value.as_str() {
                "on" => true,
                "off" => false,
                other => return Err(format!("a border line is on or off, not {other}")),
            };
            changes.insert(line, on);
        }
        let shading = match patch.shading.as_deref() {
            None => None,
            Some("none") => Some(None),
            Some(text) => Some(Some(
                parse_hex_color(text).ok_or_else(|| format!("invalid colour {text}"))?,
            )),
        };
        let table_scope = match patch.scope.as_str() {
            "table" => true,
            "cells" => false,
            other => return Err(format!("unknown border scope \u{201c}{other}\u{201d}")),
        };
        let (table, area) = self.border_target(anchor, focus, &patch.scope)?;
        let grid = TableGridMap::build(&table);
        let styled = table.properties.style_ref.is_some();
        let mut edits = CellEdits::new();
        let mut properties = table.properties.clone();

        if table_scope {
            for (&line, &on) in &changes {
                *table_line_mut(&mut properties.borders, line) = if on {
                    Some(pen.clone())
                } else if styled {
                    // A table style may draw this line; only `nil` says "none".
                    Some(nil_edge())
                } else {
                    None
                };
            }
            if let Some(fill) = shading {
                properties.shading.fill = fill;
                properties.shading.theme_fill = None;
            }
        }

        for r in area.r0..=area.r1 {
            let Some(row) = table.rows.get(r) else {
                continue;
            };
            for (c, cell) in row.cells.iter().enumerate() {
                let Some(lines) = lines_of(&grid, r, c, area) else {
                    continue;
                };
                for (side, line) in SIDES.into_iter().zip(lines) {
                    let Some(line) = line else {
                        continue;
                    };
                    let Some(&on) = changes.get(&line) else {
                        continue;
                    };
                    if table_scope {
                        // "Apply to: Table" is the table's line everywhere, so a
                        // cell's own override of that line goes (Word's rule).
                        if side_ref(&cell.properties.borders, side).is_some() {
                            *side_mut(&mut edit_of(&mut edits, &table, r, c).borders, side) = None;
                        }
                        continue;
                    }
                    let edge = if on {
                        Some(pen.clone())
                    } else {
                        let table_draws =
                            table_edge_for(&table, &grid, r, c, side).is_some_and(visible);
                        let neighbour_draws =
                            neighbours(&grid, r, c, side)
                                .into_iter()
                                .any(|(nr, nc, facing)| {
                                    side_ref(&table.rows[nr].cells[nc].properties.borders, facing)
                                        .is_some_and(visible)
                                });
                        (table_draws || neighbour_draws || styled).then(nil_edge)
                    };
                    *side_mut(&mut edit_of(&mut edits, &table, r, c).borders, side) = edge;
                    // A line drawn on the range's OUTER edge must not be cancelled
                    // by a `nil` the neighbour across it carries.
                    if on && matches!(line, Line::Top | Line::Bottom | Line::Left | Line::Right) {
                        for (nr, nc, facing) in neighbours(&grid, r, c, side) {
                            let nil =
                                side_ref(&table.rows[nr].cells[nc].properties.borders, facing)
                                    .is_some_and(|e| e.style == "nil");
                            if nil {
                                *side_mut(
                                    &mut edit_of(&mut edits, &table, nr, nc).borders,
                                    facing,
                                ) = None;
                            }
                        }
                    }
                }
                if let Some(fill) = shading {
                    let continuation = grid.cell(r, c).is_some_and(|slot| slot.continuation);
                    let props = edit_of(&mut edits, &table, r, c);
                    if table_scope {
                        props.shading.fill = None;
                        props.shading.theme_fill = None;
                        props.shading.fill_none = false;
                    } else if !continuation {
                        props.shading.fill = fill;
                        props.shading.theme_fill = None;
                        props.shading.fill_none =
                            fill.is_none() && table.properties.shading.fill.is_some();
                    }
                }
            }
        }

        let mut ops = Vec::new();
        if properties != table.properties {
            ops.push(Operation::SetTableProperties {
                table: table.id,
                properties: Box::new(properties),
            });
        }
        // The cells whose properties actually change, lowered by the command
        // crate the agent's cell-format command uses too (docs/181 G7).
        let changed: Vec<_> = edits
            .into_iter()
            .filter_map(|((r, c), props)| {
                let cell = &table.rows[r].cells[c];
                (props != cell.properties).then_some((cell.id, props))
            })
            .collect();
        ops.extend(casual_doc_command::table::set_cell_properties(
            changed,
            |_| {},
        ));
        if ops.is_empty() {
            return Err(refused!(
                "table.borders-unchanged",
                "The borders and shading are already set that way."
            )
            .to_owned());
        }
        self.apply_action_as(ops, HistoryKind::TableFormatting)
    }
}

// ---- Position and text wrapping --------------------------------------------

/// Word's default distance from surrounding text, left and right (0.13 in);
/// what it writes the moment Text wrapping becomes Around.
const DEFAULT_SIDE_DISTANCE: i32 = 180;

/// `ST_SignedTwipsMeasure`'s bound, which the importer enforces.
const MAX_OFFSET: i32 = 31_680;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PositionJson {
    /// `"none"` or `"around"`.
    wrap: &'static str,
    /// Whether this table can float at all: a top-level body table.
    floatable: bool,
    h_align: &'static str,
    h_offset_twips: i32,
    h_relative: &'static str,
    v_align: &'static str,
    v_offset_twips: i32,
    v_relative: &'static str,
    top_twips: i32,
    bottom_twips: i32,
    left_twips: i32,
    right_twips: i32,
    allow_overlap: bool,
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PositionPatch {
    wrap: Option<String>,
    h_align: Option<String>,
    h_offset_twips: Option<i32>,
    h_relative: Option<String>,
    v_align: Option<String>,
    v_offset_twips: Option<i32>,
    v_relative: Option<String>,
    top_twips: Option<i32>,
    bottom_twips: Option<i32>,
    left_twips: Option<i32>,
    right_twips: Option<i32>,
    allow_overlap: Option<bool>,
}

const fn x_align_name(align: TableXAlign) -> &'static str {
    match align {
        TableXAlign::Left => "left",
        TableXAlign::Center => "center",
        TableXAlign::Right => "right",
        TableXAlign::Inside => "inside",
        TableXAlign::Outside => "outside",
    }
}

const fn y_align_name(align: TableYAlign) -> &'static str {
    match align {
        TableYAlign::Inline => "inline",
        TableYAlign::Top => "top",
        TableYAlign::Center => "center",
        TableYAlign::Bottom => "bottom",
        TableYAlign::Inside => "inside",
        TableYAlign::Outside => "outside",
    }
}

fn parse_x_align(name: &str) -> Result<Option<TableXAlign>, String> {
    Ok(Some(match name {
        "offset" => return Ok(None),
        "left" => TableXAlign::Left,
        "center" => TableXAlign::Center,
        "right" => TableXAlign::Right,
        "inside" => TableXAlign::Inside,
        "outside" => TableXAlign::Outside,
        other => return Err(format!("unknown horizontal position {other}")),
    }))
}

fn parse_y_align(name: &str) -> Result<Option<TableYAlign>, String> {
    Ok(Some(match name {
        "offset" => return Ok(None),
        "inline" => TableYAlign::Inline,
        "top" => TableYAlign::Top,
        "center" => TableYAlign::Center,
        "bottom" => TableYAlign::Bottom,
        "inside" => TableYAlign::Inside,
        "outside" => TableYAlign::Outside,
        other => return Err(format!("unknown vertical position {other}")),
    }))
}

/// Word's horizontal "relative to": column (`text`), margin, page.
fn parse_h_relative(name: &str) -> Result<TableAnchor, String> {
    match name {
        "column" => Ok(TableAnchor::Text),
        "margin" => Ok(TableAnchor::Margin),
        "page" => Ok(TableAnchor::Page),
        other => Err(format!("unknown horizontal anchor {other}")),
    }
}

/// Word's vertical "relative to": paragraph (`text` — "move with text"),
/// margin, page.
fn parse_v_relative(name: &str) -> Result<TableAnchor, String> {
    match name {
        "paragraph" => Ok(TableAnchor::Text),
        "margin" => Ok(TableAnchor::Margin),
        "page" => Ok(TableAnchor::Page),
        other => Err(format!("unknown vertical anchor {other}")),
    }
}

const fn h_relative_name(anchor: TableAnchor) -> &'static str {
    match anchor {
        TableAnchor::Text => "column",
        TableAnchor::Margin => "margin",
        TableAnchor::Page => "page",
    }
}

const fn v_relative_name(anchor: TableAnchor) -> &'static str {
    match anchor {
        TableAnchor::Text => "paragraph",
        TableAnchor::Margin => "margin",
        TableAnchor::Page => "page",
    }
}

/// The position Word writes when Text wrapping becomes Around: where the table
/// already sits horizontally (its alignment, else its indent from the column),
/// level with its paragraph, 0.13 in from the text beside it.
fn default_float(table: &Table) -> TableFloatPosition {
    use casual_doc_model::v1::Alignment;
    let (x_spec, tbl_px_twips) = match table.properties.alignment {
        Some(Alignment::Center) => (Some(TableXAlign::Center), None),
        Some(Alignment::End) => (Some(TableXAlign::Right), None),
        _ => (None, Some(table.properties.indent_twips.unwrap_or(0))),
    };
    TableFloatPosition {
        horz_anchor: Some(TableAnchor::Text),
        vert_anchor: Some(TableAnchor::Text),
        tbl_px_twips,
        tbl_py_twips: Some(0),
        x_spec,
        y_spec: None,
        left_from_text_twips: Some(DEFAULT_SIDE_DISTANCE),
        right_from_text_twips: Some(DEFAULT_SIDE_DISTANCE),
        top_from_text_twips: None,
        bottom_from_text_twips: None,
    }
}

impl WasmDocument {
    /// The table holding paragraph `node`, and whether it may float — the layout
    /// floats top-level body tables only (`table_float`'s recorded limit).
    fn table_and_floatable(&self, node: &str) -> Result<(Table, bool), String> {
        let nid = NodeId::from_str(node).map_err(|_| "invalid node id".to_owned())?;
        let (table, _) = locate_cell(&self.document, nid).ok_or_else(|| {
            refused!("table.caret-outside", "Put the caret in a table first.").to_owned()
        })?;
        let table = find_table(&self.document, table)
            .ok_or_else(|| "table not found".to_owned())?
            .clone();
        let floatable = matches!(
            casual_doc_command::references::locate_block(&self.document, table.id),
            Some((None, _))
        );
        Ok((table, floatable))
    }

    /// See [`WasmDocument::table_float_position`].
    pub(crate) fn table_float_position_inner(&self, node: &str) -> Result<String, String> {
        let (table, floatable) = self.table_and_floatable(node)?;
        let wrap = if table.properties.float_position.is_some() {
            "around"
        } else {
            "none"
        };
        let p = table
            .properties
            .float_position
            .unwrap_or_else(|| default_float(&table));
        let json = PositionJson {
            wrap,
            floatable,
            h_align: p.x_spec.map_or("offset", x_align_name),
            h_offset_twips: p.tbl_px_twips.unwrap_or(0),
            h_relative: h_relative_name(p.horz_anchor.unwrap_or(TableAnchor::Text)),
            v_align: p.y_spec.map_or("offset", y_align_name),
            v_offset_twips: p.tbl_py_twips.unwrap_or(0),
            v_relative: v_relative_name(p.vert_anchor.unwrap_or(TableAnchor::Text)),
            top_twips: p.top_from_text_twips.unwrap_or(0),
            bottom_twips: p.bottom_from_text_twips.unwrap_or(0),
            left_twips: p.left_from_text_twips.unwrap_or(0),
            right_twips: p.right_from_text_twips.unwrap_or(0),
            allow_overlap: table.properties.overlap != Some(TableOverlap::Never),
        };
        serde_json::to_string(&json).map_err(|error| error.to_string())
    }

    /// See [`WasmDocument::set_table_float_position`].
    pub(crate) fn set_table_float_position_inner(
        &mut self,
        node: &str,
        patch: &str,
    ) -> Result<EditResult, String> {
        let patch: PositionPatch = serde_json::from_str(patch)
            .map_err(|error| format!("invalid position settings: {error}"))?;
        let (table, floatable) = self.table_and_floatable(node)?;
        let mut properties = table.properties.clone();
        let wrap_none = match patch.wrap.as_deref() {
            Some("none") => true,
            Some("around") | None => false,
            Some(other) => return Err(format!("text wrapping is none or around, not {other}")),
        };
        if wrap_none {
            // Word's None: the table is back in the line of text, so the
            // positioning and the overlap rule that only a floating table has
            // both go.
            properties.float_position = None;
            properties.overlap = None;
        } else {
            if !floatable {
                return Err(refused!(
                    "table.float-nested",
                    "Only a table in the body of the document can have text wrap around it."
                )
                .to_owned());
            }
            if patch.wrap.is_none() && properties.float_position.is_none() {
                return Err(refused!(
                    "table.float-off",
                    "Set Text wrapping to Around before positioning the table."
                )
                .to_owned());
            }
            let mut p = properties
                .float_position
                .unwrap_or_else(|| default_float(&table));
            let offset = |v: i32| v.clamp(-MAX_OFFSET, MAX_OFFSET);
            let distance = |v: i32| v.clamp(0, MAX_OFFSET);
            if let Some(name) = patch.h_align.as_deref() {
                p.x_spec = parse_x_align(name)?;
                if p.x_spec.is_none() && p.tbl_px_twips.is_none() {
                    p.tbl_px_twips = Some(0);
                }
            }
            if let Some(v) = patch.h_offset_twips {
                p.tbl_px_twips = Some(offset(v));
            }
            if let Some(name) = patch.h_relative.as_deref() {
                p.horz_anchor = Some(parse_h_relative(name)?);
            }
            if let Some(name) = patch.v_align.as_deref() {
                p.y_spec = parse_y_align(name)?;
                if p.y_spec.is_none() && p.tbl_py_twips.is_none() {
                    p.tbl_py_twips = Some(0);
                }
            }
            if let Some(v) = patch.v_offset_twips {
                p.tbl_py_twips = Some(offset(v));
            }
            if let Some(name) = patch.v_relative.as_deref() {
                p.vert_anchor = Some(parse_v_relative(name)?);
            }
            if let Some(v) = patch.top_twips {
                p.top_from_text_twips = Some(distance(v));
            }
            if let Some(v) = patch.bottom_twips {
                p.bottom_from_text_twips = Some(distance(v));
            }
            if let Some(v) = patch.left_twips {
                p.left_from_text_twips = Some(distance(v));
            }
            if let Some(v) = patch.right_twips {
                p.right_from_text_twips = Some(distance(v));
            }
            if let Some(allow) = patch.allow_overlap {
                // Word writes `never` when the box is cleared and nothing when it
                // is ticked: overlapping is the default.
                properties.overlap = (!allow).then_some(TableOverlap::Never);
            }
            properties.float_position = Some(p);
        }
        if properties == table.properties {
            return Err(refused!(
                "table.position-unchanged",
                "The table is already positioned that way."
            )
            .to_owned());
        }
        self.apply_action_as(
            vec![Operation::SetTableProperties {
                table: table.id,
                properties: Box::new(properties),
            }],
            HistoryKind::TableFormatting,
        )
    }
}

// ---- Draw table -------------------------------------------------------------

/// The smallest table a drag draws (0.25 in x 1/6 in), so a click with a
/// tremor is not a sliver of a table.
const MIN_DRAWN: (i32, i32) = (360, 240);

/// How short a drawn line may be (0.1 in) before it is a click, not a line.
const MIN_LINE: i32 = 144;

/// How close (0.05 in) a drawn line may come to an existing grid line before it
/// IS that line — then it splits the merged cells across it instead of minting
/// a sliver column or row.
const SNAP: i32 = 72;

fn empty_cell(
    ids: &mut dyn FnMut() -> Option<NodeId>,
    mut properties: TableCellProperties,
) -> Result<TableCell, String> {
    properties.cell_revision = None;
    properties.cell_merge = None;
    properties.prop_change = None;
    let exhausted = || "id space exhausted".to_owned();
    Ok(TableCell {
        id: ids().ok_or_else(exhausted)?,
        properties,
        blocks: vec![BlockNode::Paragraph(Paragraph {
            id: ids().ok_or_else(exhausted)?,
            properties: ParagraphProperties::default().into(),
            inlines: Vec::new(),
        })],
    })
}

/// Splits every physical cell of the merged regions in `crossed` at grid
/// boundary `boundary` (the column index the right half starts at). With
/// `insert = Some((column, left_width))` the grid column `column` is first cut in
/// two at `left_width` twips, and every cell NOT crossed that covers it grows by
/// one column, so it stays whole — Word's vertical pencil line.
///
/// The left half keeps the content and the leading border; the right half is an
/// empty cell with the same formatting and the same `w:vMerge`, so a merged cell
/// split top to bottom is two merged cells. O(cells).
///
/// # Errors
/// When no crossed cell straddles the boundary (there is already a line), or
/// ids run out.
fn split_columns(
    table: &Table,
    crossed: &BTreeSet<usize>,
    boundary: usize,
    insert: Option<(usize, i32)>,
    ids: &mut dyn FnMut() -> Option<NodeId>,
) -> Result<(Table, Option<NodeId>), String> {
    let grid = TableGridMap::build(table);
    let mut out = table.clone();
    if let Some((column, left)) = insert {
        let width = table
            .grid
            .get(column)
            .and_then(|g| g.width_twips)
            .unwrap_or(left * 2);
        out.grid[column] = GridColumn {
            width_twips: Some(left),
        };
        out.grid.insert(
            column + 1,
            GridColumn {
                width_twips: Some((width - left).max(1)),
            },
        );
    }
    let mut split_any = false;
    let mut caret = None;
    for (r, row) in table.rows.iter().enumerate() {
        let mut cells = Vec::with_capacity(row.cells.len() + 1);
        let (before, after) = grid.row_skips(r);
        let mut end = before;
        for (c, cell) in row.cells.iter().enumerate() {
            let Some(slot) = grid.cell(r, c) else {
                cells.push(cell.clone());
                continue;
            };
            end = slot.end_column();
            let (mut first, mut span) = (slot.first_column, slot.span);
            if let Some((column, _)) = insert {
                if slot.covers(column) {
                    span += 1;
                } else if slot.first_column > column {
                    first += 1;
                }
            }
            let straddles = first < boundary && boundary < first + span;
            if crossed.contains(&slot.region) && straddles {
                split_any = true;
                let mut left = cell.clone();
                let left_span = boundary - first;
                left.properties.grid_span =
                    (left_span > 1).then(|| u32::try_from(left_span).unwrap_or(1));
                let trailing = left.properties.borders.end.take();
                let mut right_props = left.properties.clone();
                right_props.borders.start = None;
                right_props.borders.end = trailing;
                let right_span = first + span - boundary;
                right_props.grid_span =
                    (right_span > 1).then(|| u32::try_from(right_span).unwrap_or(1));
                let right = empty_cell(ids, right_props)?;
                if caret.is_none() && !slot.continuation {
                    caret = first_paragraph_of_cell(&right);
                }
                cells.push(left);
                cells.push(right);
            } else {
                let mut kept = cell.clone();
                kept.properties.grid_span = (span > 1).then(|| u32::try_from(span).unwrap_or(1));
                cells.push(kept);
            }
        }
        let target = &mut out.rows[r];
        target.cells = cells;
        if let Some((column, _)) = insert {
            if column < before {
                target.properties.grid_before = Some(u32::try_from(before + 1).unwrap_or(u32::MAX));
            } else if column >= end && column < end + after {
                target.properties.grid_after = Some(u32::try_from(after + 1).unwrap_or(u32::MAX));
            }
        }
    }
    if !split_any {
        return Err(
            refused!("table.draw-existing-line", "There is already a line there.").to_owned(),
        );
    }
    normalize_vertical_merges(&mut out);
    sync_cell_widths(&mut out);
    Ok((out, caret))
}

/// Splits row `row` at a horizontal pencil line `dy` twips below its top (of
/// `height` painted): a new row goes in below it, the crossed cells become two,
/// and every cell the line did NOT cross is merged down over the new row so it
/// stays whole. A merged cell the line crosses in its middle row becomes two
/// merged cells. The row's own height, when it has one, is shared at the line.
/// O(cells).
///
/// # Errors
/// When `row` is not in the table, or ids run out.
fn split_row(
    table: &Table,
    crossed: &BTreeSet<usize>,
    row: usize,
    dy_height: (i32, i32),
    ids: &mut dyn FnMut() -> Option<NodeId>,
) -> Result<(Table, Option<NodeId>), String> {
    let grid = TableGridMap::build(table);
    // The command crate's row insert, the one an agent's command runs (docs/181
    // G7): its table after the edit and the new row's index.
    let edit = casual_doc_command::table::insert_row(table, row, true, &mut *ids)?;
    let (mut out, at) = (edit.table, edit.row);
    let template = &table.rows[row];
    let mut caret = None;
    for (c, cell) in template.cells.iter().enumerate() {
        let Some(slot) = grid.cell(row, c) else {
            continue;
        };
        let region = grid.region(slot.region);
        let through = region.last_row > row;
        if crossed.contains(&slot.region) {
            if through {
                // The new cell grew the merge; the line cuts it here, so the new
                // row starts a merged cell of its own (the rows below join it).
                if let Some(new) = out.rows[at].cells.get_mut(c) {
                    new.properties.vertical_merge = Some(VerticalMerge::Restart);
                }
            }
            if caret.is_none() {
                caret = out.rows[at].cells.get(c).and_then(first_paragraph_of_cell);
            }
        } else if !through {
            // Not crossed and ending here: it stays one cell, spanning the new row.
            if cell.properties.vertical_merge.is_none() {
                out.rows[row].cells[c].properties.vertical_merge = Some(VerticalMerge::Restart);
            }
            if let Some(new) = out.rows[at].cells.get_mut(c) {
                new.properties.vertical_merge = Some(VerticalMerge::Continue);
            }
        }
    }
    let (dy, height) = dy_height;
    let rule = template.properties.height.rule;
    if let Some(value) = template.properties.height.value_twips
        && matches!(rule, Some(HeightRule::Exact | HeightRule::AtLeast))
    {
        let total = i32::try_from(value).unwrap_or(i32::MAX).max(height);
        let top = dy.clamp(SNAP, (total - SNAP).max(SNAP));
        out.rows[row].properties.height.value_twips = u32::try_from(top).ok();
        out.rows[at].properties.height.value_twips = u32::try_from((total - top).max(SNAP)).ok();
    }
    normalize_vertical_merges(&mut out);
    Ok((out, caret))
}

/// Ends a vertical merge at row boundary `boundary` (the row the lower part
/// starts on) for every crossed merged region that runs across it: the cell
/// there restarts. Word's pencil drawn along an existing row line through a
/// merged cell. O(cells).
///
/// # Errors
/// When no crossed region runs across the boundary.
fn split_merge_at_row(
    table: &Table,
    crossed: &BTreeSet<usize>,
    boundary: usize,
) -> Result<Table, String> {
    let grid = TableGridMap::build(table);
    let mut out = table.clone();
    let mut any = false;
    if let Some(row) = out.rows.get_mut(boundary) {
        for (c, cell) in row.cells.iter_mut().enumerate() {
            let Some(slot) = grid.cell(boundary, c) else {
                continue;
            };
            let region = grid.region(slot.region);
            if crossed.contains(&slot.region) && region.first_row < boundary && slot.continuation {
                cell.properties.vertical_merge = Some(VerticalMerge::Restart);
                any = true;
            }
        }
    }
    if !any {
        return Err(
            refused!("table.draw-existing-line", "There is already a line there.").to_owned(),
        );
    }
    normalize_vertical_merges(&mut out);
    Ok(out)
}

/// One merged region as painted: its rectangle on its first page.
struct PaintedRegion {
    region: usize,
    page: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl WasmDocument {
    /// Every merged region of `table` with its painted rectangle.
    /// O(regions x fragments): one painted-layout lookup per region.
    fn painted_regions(&self, table: &Table, grid: &TableGridMap) -> Vec<PaintedRegion> {
        let layout = self.painted_snapshot();
        grid.regions()
            .iter()
            .enumerate()
            .filter_map(|(index, region)| {
                let anchor = table
                    .rows
                    .get(region.first_row)?
                    .cells
                    .get(region.anchor_cell)
                    .and_then(first_paragraph_of_cell)?;
                let (page, rect) = layout.cell_rect(anchor)?;
                Some(PaintedRegion {
                    region: index,
                    page,
                    x: rect.origin.x.raw(),
                    y: rect.origin.y.raw(),
                    w: rect.size.width.raw(),
                    h: rect.size.height.raw(),
                })
            })
            .collect()
    }

    /// See [`WasmDocument::draw_table`].
    pub(crate) fn draw_table_inner(
        &mut self,
        node: &str,
        rect: &[i32],
    ) -> Result<EditResult, String> {
        let [x, y, w, h] = <[i32; 4]>::try_from(rect)
            .map_err(|_| "a drawn table is x, y, width and height".to_owned())?;
        let nid = NodeId::from_str(node).map_err(|_| "invalid node id".to_owned())?;
        let (w, h) = (w.abs().max(MIN_DRAWN.0), h.abs().max(MIN_DRAWN.1));
        let (container, index, _) = self.table_insertion_point(nid);
        let ids = &mut self.edit_ids;
        let mut edit =
            casual_doc_command::table::insert_table_in(container, index, 1, 1, w, &mut || {
                ids.next_id().ok()
            })?;
        for op in &mut edit.operations {
            // A match arm, not `if let`: the G7 guard reads a pattern as a pattern
            // only in this shape (command_parity_tests), and this changes the table
            // the command crate built rather than lowering one itself.
            #[allow(clippy::single_match)]
            match op {
                Operation::InsertTable { table, .. } => {
                    table.properties.width = Some(TableWidth::dxa(w));
                    if let Some(row) = table.rows.first_mut() {
                        row.properties.height = RowHeight {
                            value_twips: u32::try_from(h).ok(),
                            rule: Some(HeightRule::AtLeast),
                        };
                    }
                    // A table drawn in the body floats where it was drawn, as Word's
                    // does. One drawn inside a cell is a nested table, which the
                    // layout keeps in line (`table_float`'s recorded limit), so it
                    // takes the drawn width and stays in the cell's flow.
                    if container.is_none() {
                        table.properties.float_position = Some(TableFloatPosition {
                            horz_anchor: Some(TableAnchor::Page),
                            vert_anchor: Some(TableAnchor::Page),
                            tbl_px_twips: Some(x.clamp(-MAX_OFFSET, MAX_OFFSET)),
                            tbl_py_twips: Some(y.clamp(-MAX_OFFSET, MAX_OFFSET)),
                            left_from_text_twips: Some(DEFAULT_SIDE_DISTANCE),
                            right_from_text_twips: Some(DEFAULT_SIDE_DISTANCE),
                            ..TableFloatPosition::default()
                        });
                    }
                }
                _ => {}
            }
        }
        let caret = Pos::new(edit.caret.unwrap_or(nid), 0);
        self.apply_action_caret_as(edit.operations, caret, HistoryKind::TableStructure)
    }

    /// See [`WasmDocument::draw_table_line`].
    pub(crate) fn draw_table_line_inner(
        &mut self,
        node: &str,
        line: &[i32],
    ) -> Result<EditResult, String> {
        let [page, x0, y0, x1, y1] = <[i32; 5]>::try_from(line)
            .map_err(|_| "a drawn line is a page and two points".to_owned())?;
        let nid = NodeId::from_str(node).map_err(|_| "invalid node id".to_owned())?;
        let (table_id, _) = locate_cell(&self.document, nid).ok_or_else(|| {
            refused!("table.caret-outside", "Put the caret in a table first.").to_owned()
        })?;
        let table = find_table(&self.document, table_id)
            .ok_or_else(|| "table not found".to_owned())?
            .clone();
        let (dx, dy) = ((x1 - x0).abs(), (y1 - y0).abs());
        if dx.max(dy) < MIN_LINE {
            return Err(
                refused!("table.draw-short", "Drag a longer line to split the cells.").to_owned(),
            );
        }
        let grid = TableGridMap::build(&table);
        let painted = self.painted_regions(&table, &grid);
        let page = u32::try_from(page).unwrap_or(0);
        let vertical = dy > dx;
        let (lo, hi) = if vertical {
            (y0.min(y1), y0.max(y1))
        } else {
            (x0.min(x1), x0.max(x1))
        };
        let at = if vertical {
            (x0 + x1) / 2
        } else {
            (y0 + y1) / 2
        };
        let mut hits: Vec<&PaintedRegion> = painted
            .iter()
            .filter(|p| p.page == page)
            .filter(|p| {
                let (start, extent, cross_start, cross_extent) = if vertical {
                    (p.x, p.w, p.y, p.h)
                } else {
                    (p.y, p.h, p.x, p.w)
                };
                // Within SNAP of a cell counts as crossing it, so a line drawn ON
                // an existing grid line finds the cells on both sides of it.
                start - SNAP < at
                    && at < start + extent + SNAP
                    && cross_start < hi
                    && lo < cross_start + cross_extent
            })
            .collect();
        if hits.is_empty() {
            return Err(refused!(
                "table.draw-outside",
                "Draw the line across the cells you want to split."
            )
            .to_owned());
        }
        hits.sort_by_key(|p| (p.y, p.x));
        let ids = &mut self.edit_ids;
        let mut mint = || ids.next_id().ok();
        let (replacement, caret) = if vertical {
            let first = hits[0];
            let region = *grid.region(first.region);
            let columns: Vec<i32> = (region.first_column..=region.last_column)
                .map(|c| {
                    table
                        .grid
                        .get(c)
                        .and_then(|g| g.width_twips)
                        .unwrap_or(0)
                        .max(0)
                })
                .collect();
            let model_width: i32 = columns.iter().sum();
            let scale = |v: i32| -> i32 {
                if model_width > 0 && first.w > 0 {
                    i32::try_from(i64::from(v) * i64::from(model_width) / i64::from(first.w))
                        .unwrap_or(v)
                } else {
                    v
                }
            };
            let offset = scale(at - first.x);
            let mut start = 0;
            let mut found = None;
            for (k, &width) in columns.iter().enumerate() {
                if offset < start + width || k + 1 == columns.len() {
                    found = Some((region.first_column + k, offset - start, width));
                    break;
                }
                start += width;
            }
            let (column, into, width) =
                found.ok_or_else(|| "the cell has no columns".to_owned())?;
            let crossed: BTreeSet<usize> = hits
                .iter()
                .filter(|p| {
                    let r = grid.region(p.region);
                    r.first_column <= column && column <= r.last_column
                })
                .map(|p| p.region)
                .collect();
            if into < SNAP {
                split_columns(&table, &crossed, column, None, &mut mint)?
            } else if width - into < SNAP {
                split_columns(&table, &crossed, column + 1, None, &mut mint)?
            } else {
                split_columns(
                    &table,
                    &crossed,
                    column + 1,
                    Some((column, into)),
                    &mut mint,
                )?
            }
        } else {
            // Row extents from the single-row cells painted on this page, so a
            // line through a merged cell still finds the row it is in.
            let mut rows: BTreeMap<usize, (i32, i32)> = BTreeMap::new();
            for p in painted.iter().filter(|p| p.page == page) {
                let r = grid.region(p.region);
                if r.row_count() == 1 {
                    let entry = rows.entry(r.first_row).or_insert((p.y, p.y + p.h));
                    entry.0 = entry.0.min(p.y);
                    entry.1 = entry.1.max(p.y + p.h);
                }
            }
            let crossed: BTreeSet<usize> = hits.iter().map(|p| p.region).collect();
            let (row, (top, bottom)) = rows
                .iter()
                .find(|(_, (top, bottom))| *top <= at && at < *bottom)
                .map(|(r, extent)| (*r, *extent))
                .ok_or_else(|| {
                    refused!(
                        "table.draw-outside",
                        "Draw the line across the cells you want to split."
                    )
                    .to_owned()
                })?;
            if at - top < SNAP && row > 0 {
                (split_merge_at_row(&table, &crossed, row)?, None)
            } else if bottom - at < SNAP && row + 1 < table.rows.len() {
                (split_merge_at_row(&table, &crossed, row + 1)?, None)
            } else {
                split_row(&table, &crossed, row, (at - top, bottom - top), &mut mint)?
            }
        };
        let caret = Pos::new(caret.unwrap_or(nid), 0);
        self.apply_action_caret_as(
            vec![Operation::ReplaceTable {
                table: table.id,
                replacement: Box::new(replacement),
            }],
            caret,
            HistoryKind::TableStructure,
        )
    }
}

#[wasm_bindgen]
impl WasmDocument {
    /// The Borders and Shading dialog's state, as JSON: for `scope` `"cells"`
    /// (the range `anchor..focus`, `""` = the anchor's cell) or `"table"`, each of
    /// the six lines (`top`, `bottom`, `left`, `right`, `insideH`, `insideV`) as
    /// `"on"`, `"off"`, `"mixed"` or `"unavailable"` (no inside line in a single
    /// row or column), what a reader SEES there — a cell's own edge, else the
    /// table's; the pen of the first line drawn (`style`, `size` in
    /// eighth-points, `color`); and the shading (`#rrggbb`, `""` or `"mixed"`).
    ///
    /// **O(document)** for the lookup, then O(cells of the table).
    ///
    /// # Errors
    /// When the node is not in a table or the range refuses.
    #[wasm_bindgen(js_name = tableBorderState)]
    pub fn table_border_state(
        &self,
        anchor: &str,
        focus: &str,
        scope: &str,
    ) -> Result<String, JsValue> {
        self.table_border_state_inner(anchor, focus, scope)
            .map_err(to_js)
    }

    /// Applies the Borders and Shading dialog in ONE undoable step. `patch` is
    /// JSON: `{"scope":"cells"|"table","edges":{"top":"on"|"off",…},"style",
    /// "size","color","shading":"none"|"#rrggbb"}`; a line not named is left as
    /// it is. "On" lines are drawn with the pen; "off" lines go (an explicit
    /// `nil` where the table, a style or a neighbour would still draw one).
    /// `"table"` sets `w:tblBorders` and drops the cells' own overrides of the
    /// lines it changes, which is Word's Apply to: Table.
    ///
    /// # Errors
    /// When the settings are malformed, the line style is not one the renderer
    /// draws, the range refuses, or nothing would change.
    #[wasm_bindgen(js_name = applyTableBorders)]
    pub fn apply_table_borders(
        &mut self,
        anchor: &str,
        focus: &str,
        patch: &str,
    ) -> Result<EditResult, JsValue> {
        self.apply_table_borders_inner(anchor, focus, patch)
            .map_err(to_js)
    }

    /// The table's text wrapping and position, as JSON: `wrap` (`none` |
    /// `around`), `floatable`, `hAlign` (`left|center|right|inside|outside|offset`),
    /// `hOffsetTwips`, `hRelative` (`column|margin|page`), `vAlign`
    /// (`top|center|bottom|inside|outside|inline|offset`), `vOffsetTwips`,
    /// `vRelative` (`paragraph|margin|page`), the four distances from text
    /// (`topTwips` …) and `allowOverlap`. A table that does not float reports the
    /// position Around would start from.
    ///
    /// **O(document)** for the lookup.
    ///
    /// # Errors
    /// When the node is not in a table.
    #[wasm_bindgen(js_name = tableFloatPosition)]
    pub fn table_float_position(&self, node: &str) -> Result<String, JsValue> {
        self.table_float_position_inner(node).map_err(to_js)
    }

    /// Changes the table's text wrapping and position (`w:tblpPr`,
    /// `w:tblOverlap`) in one undoable step. `patch` is JSON with any of
    /// [`tableFloatPosition`](Self::table_float_position)'s fields; a field not
    /// named is left as it is. `wrap: "none"` removes the positioning.
    ///
    /// # Errors
    /// When the settings are malformed, the table cannot float (it is nested),
    /// or nothing would change.
    #[wasm_bindgen(js_name = setTableFloatPosition)]
    pub fn set_table_float_position(
        &mut self,
        node: &str,
        patch: &str,
    ) -> Result<EditResult, JsValue> {
        self.set_table_float_position_inner(node, patch)
            .map_err(to_js)
    }

    /// Draw Table's rectangle: a 1 x 1 table of the drawn size, inserted after
    /// the block holding paragraph `node` (where the drag began) and floating at
    /// the drawn place on the page. `rect` is `[x, y, width, height]` in
    /// page-local twips. One undoable step.
    ///
    /// # Errors
    /// When `rect` is not four numbers, or ids run out.
    #[wasm_bindgen(js_name = drawTable)]
    pub fn draw_table(&mut self, node: &str, rect: &[i32]) -> Result<EditResult, JsValue> {
        self.draw_table_inner(node, rect).map_err(to_js)
    }

    /// Draw Table's pencil inside a table: `line` is `[page, x0, y0, x1, y1]` in
    /// page-local twips, and `node` a paragraph of the table. Mostly horizontal
    /// splits the row it crosses at the line (a new row; cells not crossed stay
    /// whole); mostly vertical splits the column at the line's x (a new grid
    /// column; cells not crossed stay whole). Drawn along an existing line, it
    /// splits the merged cells across it. One undoable step.
    ///
    /// # Errors
    /// When the line is too short, crosses no cell, or lies on a line already
    /// there.
    #[wasm_bindgen(js_name = drawTableLine)]
    pub fn draw_table_line(&mut self, node: &str, line: &[i32]) -> Result<EditResult, JsValue> {
        self.draw_table_line_inner(node, line).map_err(to_js)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::wasm_document;
    use crate::{node_plain_text, open_document};

    /// A document from body markup, imported so the tables are real ones.
    fn document(body: &str) -> WasmDocument {
        let xml = format!(r#"<w:document xmlns:w="urn:w"><w:body>{body}</w:body></w:document>"#);
        let document = casual_doc_import::import_main_document_xml(
            xml.as_bytes(),
            casual_doc_import::ImportConfig::default(),
        )
        .expect("import")
        .document;
        wasm_document(document)
    }

    fn p(text: &str) -> String {
        format!(r#"<w:p><w:r><w:t>{text}</w:t></w:r></w:p>"#)
    }

    /// A `rows` x `columns` table whose cells read "rc", with a single line on
    /// every table edge when `lines` is set.
    fn table_xml(rows: usize, columns: usize, lines: bool) -> String {
        let mut xml = String::from("<w:tbl>");
        if lines {
            xml.push_str("<w:tblPr><w:tblBorders>");
            for edge in ["top", "left", "bottom", "right", "insideH", "insideV"] {
                xml.push_str(&format!(
                    r#"<w:{edge} w:val="single" w:sz="4" w:color="000000"/>"#
                ));
            }
            xml.push_str("</w:tblBorders></w:tblPr>");
        }
        xml.push_str("<w:tblGrid>");
        for _ in 0..columns {
            xml.push_str(r#"<w:gridCol w:w="2000"/>"#);
        }
        xml.push_str("</w:tblGrid>");
        for r in 0..rows {
            xml.push_str("<w:tr>");
            for c in 0..columns {
                xml.push_str(&format!("<w:tc>{}</w:tc>", p(&format!("{r}{c}"))));
            }
            xml.push_str("</w:tr>");
        }
        xml.push_str("</w:tbl>");
        xml
    }

    /// The id of the first paragraph (at any depth) reading exactly `text`.
    fn paragraph_reading(d: &WasmDocument, text: &str) -> String {
        fn walk(blocks: &[BlockNode], text: &str) -> Option<NodeId> {
            blocks.iter().find_map(|block| match block {
                BlockNode::Paragraph(p) if node_plain_text(&p.inlines) == text => Some(p.id),
                BlockNode::Table(t) => t
                    .rows
                    .iter()
                    .flat_map(|r| r.cells.iter())
                    .find_map(|c| walk(&c.blocks, text)),
                _ => None,
            })
        }
        walk(d.document.body(), text)
            .unwrap_or_else(|| panic!("no paragraph reading {text:?}"))
            .to_string()
    }

    fn first_table(d: &WasmDocument) -> &Table {
        d.document
            .body()
            .iter()
            .find_map(|block| match block {
                BlockNode::Table(t) => Some(t),
                _ => None,
            })
            .expect("a table")
    }

    fn state(d: &WasmDocument, a: &str, f: &str, scope: &str) -> serde_json::Value {
        serde_json::from_str(&d.table_border_state_inner(a, f, scope).expect("state"))
            .expect("json")
    }

    #[test]
    fn the_dialog_opens_on_every_line_including_the_inside_ones() {
        let d = document(&table_xml(2, 2, true));
        let a = paragraph_reading(&d, "00");
        let f = paragraph_reading(&d, "11");
        let all = state(&d, &a, &f, "cells");
        for line in ["top", "bottom", "left", "right", "insideH", "insideV"] {
            assert_eq!(all["edges"][line], "on", "{line}");
        }
        assert_eq!(all["pen"]["style"], "single");
        assert_eq!(all["pen"]["size"], 4);
        assert_eq!(all["rows"], 2);
        let one = state(&d, &a, "", "cells");
        assert_eq!(one["edges"]["insideH"], "unavailable");
        assert_eq!(one["edges"]["insideV"], "unavailable");
        assert_eq!(one["edges"]["top"], "on");
        let bare = document(&table_xml(2, 2, false));
        let a = paragraph_reading(&bare, "00");
        let table = state(&bare, &a, "", "table");
        assert_eq!(table["edges"]["insideH"], "off");
        assert!(table["pen"].is_null());
        assert_eq!(table["shading"], "");
    }

    #[test]
    fn clicking_an_inside_line_off_on_cells_is_one_undoable_step() {
        let mut d = document(&table_xml(2, 2, true));
        let a = paragraph_reading(&d, "00");
        let f = paragraph_reading(&d, "11");
        let before = first_table(&d).clone();
        d.apply_table_borders_inner(
            &a,
            &f,
            r##"{"scope":"cells","edges":{"insideH":"off","top":"on"},"style":"double","size":12,"color":"#ff0000","shading":"#ffff00"}"##,
        )
        .expect("applies");
        let after = state(&d, &a, &f, "cells");
        assert_eq!(
            after["edges"]["insideH"], "off",
            "the table still draws it, so nil wins"
        );
        assert_eq!(after["edges"]["insideV"], "on", "an untouched line stays");
        assert_eq!(after["edges"]["top"], "on");
        assert_eq!(after["pen"]["style"], "double");
        assert_eq!(after["pen"]["color"], "#ff0000");
        assert_eq!(after["shading"], "#ffff00");
        let table = first_table(&d);
        assert_eq!(
            table.rows[0].cells[0]
                .properties
                .borders
                .bottom
                .as_ref()
                .map(|e| e.style.as_str()),
            Some("nil")
        );
        d.undo_inner().expect("undo");
        assert_eq!(*first_table(&d), before, "ONE undo restores every cell");
        let refusal = d
            .apply_table_borders_inner(&a, &f, r#"{"scope":"cells","style":"wavy"}"#)
            .unwrap_err();
        assert!(refusal.contains("wavy"), "{refusal}");
        let refusal = d
            .apply_table_borders_inner(&a, &f, r#"{"scope":"cells"}"#)
            .unwrap_err();
        assert!(refusal.contains("already"), "{refusal}");
    }

    #[test]
    fn apply_to_table_sets_the_table_lines_and_drops_cell_overrides() {
        let mut d = document(&table_xml(2, 2, false));
        let a = paragraph_reading(&d, "00");
        d.apply_table_borders_inner(&a, "", r#"{"scope":"cells","edges":{"bottom":"on"}}"#)
            .expect("cell line");
        assert!(
            first_table(&d).rows[0].cells[0]
                .properties
                .borders
                .bottom
                .is_some()
        );
        d.apply_table_borders_inner(
            &a,
            "",
            r##"{"scope":"table","edges":{"insideH":"off","top":"on","bottom":"on","left":"on","right":"on"},"shading":"#00ff00"}"##,
        )
        .expect("table");
        let table = first_table(&d);
        assert!(table.properties.borders.top.is_some());
        assert!(table.properties.borders.inside_h.is_none());
        assert!(
            table.rows[0].cells[0].properties.borders.bottom.is_none(),
            "the cell's own inside line is gone with the table's"
        );
        assert_eq!(
            table.properties.shading.fill,
            Some(RgbColor { r: 0, g: 255, b: 0 })
        );
        let s = state(&d, &a, "", "table");
        assert_eq!(s["edges"]["insideH"], "off");
        assert_eq!(s["edges"]["top"], "on");
        assert_eq!(s["shading"], "#00ff00");
    }

    fn position(d: &WasmDocument, node: &str) -> serde_json::Value {
        serde_json::from_str(&d.table_float_position_inner(node).expect("position")).expect("json")
    }

    #[test]
    fn wrapping_around_positions_and_round_trips_through_docx() {
        let mut d = document(&format!("{}{}", table_xml(1, 2, true), p("after")));
        let a = paragraph_reading(&d, "00");
        let start = position(&d, &a);
        assert_eq!(start["wrap"], "none");
        assert_eq!(start["floatable"], true);
        d.set_table_float_position_inner(&a, r#"{"wrap":"around"}"#)
            .expect("around");
        let float = first_table(&d).properties.float_position.expect("floats");
        assert_eq!(float.left_from_text_twips, Some(180), "Word's 0.13 in");
        assert_eq!(
            float.vert_anchor,
            Some(TableAnchor::Text),
            "moves with text"
        );
        d.set_table_float_position_inner(
            &a,
            r#"{"hAlign":"center","hRelative":"page","vAlign":"offset","vOffsetTwips":720,"vRelative":"margin","topTwips":144,"allowOverlap":false}"#,
        )
        .expect("position");
        let read = position(&d, &a);
        assert_eq!(read["wrap"], "around");
        assert_eq!(read["hAlign"], "center");
        assert_eq!(read["hRelative"], "page");
        assert_eq!(read["vAlign"], "offset");
        assert_eq!(read["vOffsetTwips"], 720);
        assert_eq!(read["vRelative"], "margin");
        assert_eq!(read["topTwips"], 144);
        assert_eq!(read["allowOverlap"], false);

        // What was authored is what the file says, and what reopens.
        let bytes =
            casual_doc_export::write_document(&d.document, d.resources.as_map()).expect("export");
        let reopened = open_document(&bytes).expect("reopen");
        let table = first_table(&reopened);
        assert_eq!(
            table.properties.float_position,
            first_table(&d).properties.float_position
        );
        assert_eq!(table.properties.overlap, Some(TableOverlap::Never));

        d.set_table_float_position_inner(&a, r#"{"wrap":"none"}"#)
            .expect("none");
        assert!(first_table(&d).properties.float_position.is_none());
        assert!(first_table(&d).properties.overlap.is_none());
        d.undo_inner().expect("undo");
        d.undo_inner().expect("undo");
        d.undo_inner().expect("undo");
        assert!(first_table(&d).properties.float_position.is_none());
        let refusal = d
            .set_table_float_position_inner(&a, r#"{"hAlign":"left"}"#)
            .unwrap_err();
        assert!(refusal.contains("Around"), "{refusal}");
    }

    #[test]
    fn a_nested_table_cannot_wrap() {
        let inner = table_xml(1, 1, false).replace(">00<", ">in<");
        let outer = format!(
            r#"<w:tbl><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc>{inner}{}</w:tc></w:tr></w:tbl>"#,
            p("o")
        );
        let mut d = document(&outer);
        let node = paragraph_reading(&d, "in");
        assert_eq!(position(&d, &node)["floatable"], false);
        let refusal = d
            .set_table_float_position_inner(&node, r#"{"wrap":"around"}"#)
            .unwrap_err();
        assert!(refusal.contains("body of the document"), "{refusal}");
    }

    #[test]
    fn a_drawn_rectangle_is_a_floating_one_cell_table_where_it_was_drawn() {
        let mut d = document(&format!("{}{}", p("one"), p("two")));
        let node = paragraph_reading(&d, "one");
        d.draw_table_inner(&node, &[2880, 3600, 2880, 720])
            .expect("draws");
        let table = first_table(&d);
        assert_eq!((table.rows.len(), table.rows[0].cells.len()), (1, 1));
        assert_eq!(table.properties.width, Some(TableWidth::dxa(2880)));
        let float = table.properties.float_position.expect("floats");
        assert_eq!(float.horz_anchor, Some(TableAnchor::Page));
        assert_eq!(
            (float.tbl_px_twips, float.tbl_py_twips),
            (Some(2880), Some(3600))
        );
        assert_eq!(table.rows[0].properties.height.value_twips, Some(720));
        d.undo_inner().expect("undo");
        assert!(
            d.document
                .body()
                .iter()
                .all(|b| !matches!(b, BlockNode::Table(_)))
        );
        assert!(d.draw_table_inner(&node, &[1, 2, 3]).is_err());
    }

    /// The painted rectangle of the cell holding paragraph `text`, as
    /// `(page, x, y, w, h)`.
    fn cell_box(d: &WasmDocument, text: &str) -> (i32, i32, i32, i32, i32) {
        let node = NodeId::from_str(&paragraph_reading(d, text)).expect("id");
        let (page, rect) = d.painted_snapshot().cell_rect(node).expect("painted");
        (
            i32::try_from(page).expect("page"),
            rect.origin.x.raw(),
            rect.origin.y.raw(),
            rect.size.width.raw(),
            rect.size.height.raw(),
        )
    }

    #[test]
    fn a_vertical_line_through_one_cell_splits_it_and_keeps_the_rest_whole() {
        let mut d = document(&table_xml(2, 2, true));
        let node = paragraph_reading(&d, "00");
        let (page, x, y, w, h) = cell_box(&d, "00");
        let at = x + w / 3;
        d.draw_table_line_inner(&node, &[page, at, y + 20, at, y + h - 20])
            .expect("splits");
        let table = first_table(&d);
        assert_eq!(table.grid.len(), 3, "a grid column was cut in two");
        assert_eq!(table.rows[0].cells.len(), 3, "the crossed cell is two");
        assert_eq!(table.rows[1].cells.len(), 2, "the row below is untouched");
        assert_eq!(
            table.rows[1].cells[0].properties.grid_span,
            Some(2),
            "and stays whole"
        );
        let total: i32 = table.grid.iter().filter_map(|g| g.width_twips).sum();
        assert_eq!(total, 4000, "the table keeps its width");
        d.undo_inner().expect("undo");
        assert_eq!(first_table(&d).grid.len(), 2);

        // Along the existing line between the columns: nothing to split.
        let edge = x + w;
        let refusal = d
            .draw_table_line_inner(&node, &[page, edge, y + 20, edge, y + h - 20])
            .unwrap_err();
        assert!(refusal.contains("already a line"), "{refusal}");
        assert!(
            d.draw_table_line_inner(&node, &[page, x, y, x + 10, y])
                .is_err(),
            "a click is not a line"
        );
    }

    #[test]
    fn a_horizontal_line_through_one_cell_adds_a_row_and_merges_the_others_down() {
        let mut d = document(&table_xml(1, 2, true));
        let node = paragraph_reading(&d, "00");
        let (page, x, y, w, h) = cell_box(&d, "00");
        let at = y + h / 2;
        d.draw_table_line_inner(&node, &[page, x + 20, at, x + w - 20, at])
            .expect("splits");
        let table = first_table(&d);
        assert_eq!(table.rows.len(), 2);
        assert_eq!(
            table.rows[0].cells[1].properties.vertical_merge,
            Some(VerticalMerge::Restart),
            "the cell not crossed spans both rows"
        );
        assert_eq!(
            table.rows[1].cells[1].properties.vertical_merge,
            Some(VerticalMerge::Continue)
        );
        assert_eq!(
            table.rows[1].cells[0].properties.vertical_merge, None,
            "the crossed one is two cells"
        );
        d.undo_inner().expect("undo");
        assert_eq!(first_table(&d).rows.len(), 1);
    }
}
