//! Tables: the grid, merges by grid column, and each cell's borders, fill,
//! padding and alignment as the page resolves them.

use casual_doc_layout::paint_values::{cell_margins, table_cell_borders, table_cell_layers};
use casual_doc_model::v1::{
    BorderEdge, CellVerticalAlignment, HeightRule, Table, TableWidth, TextDirection, VerticalMerge,
    WidthType,
};

use super::{Cx, Flow, Writer, round3};
use crate::html::css::{self, Declarations, hex, twips};
use crate::html::escape_attribute;
use crate::{AdapterError, ModelOutcome};

impl Writer<'_> {
    /// A real HTML table: the grid's column widths, merged cells as `colspan`
    /// and `rowspan`, each cell's borders, fill, padding and alignment as the
    /// page resolves them — table style, banding and conflict rules included.
    pub(super) fn table(
        &mut self,
        table: &Table,
        cx: Cx<'_>,
        flow: &mut Flow,
    ) -> Result<(), AdapterError> {
        if cx.depth >= self.limits.max_nesting_depth {
            return Err(AdapterError::new(
                "limit html_nesting_depth exceeded while walking nested tables",
            ));
        }
        let layers = table_cell_layers(table, &self.cascade);
        let borders = table_cell_borders(table, &layers);
        let columns = grid_columns(table);

        let mut css = Declarations::default();
        let properties = &table.properties;
        if let Some(width) = properties.width.as_ref().and_then(width_css) {
            css.set("width", width);
        }
        if properties.layout == Some(casual_doc_model::v1::TableLayout::Fixed) {
            css.set("table-layout", "fixed");
        }
        if let Some(indent) = properties.indent_twips.filter(|indent| *indent != 0) {
            css.set("margin-inline-start", twips(i64::from(indent)));
        }
        match properties.alignment {
            Some(casual_doc_model::v1::Alignment::Center) => {
                css.set("margin-inline-start", "auto");
                css.set("margin-inline-end", "auto");
            }
            Some(casual_doc_model::v1::Alignment::End) => {
                css.set("margin-inline-start", "auto");
            }
            _ => {}
        }
        if let Some(spacing) = properties.cell_spacing_twips.filter(|spacing| *spacing > 0) {
            css.set("border-collapse", "separate");
            css.set("border-spacing", twips(i64::from(spacing) * 2));
        }
        if flow.pending_after > 0 {
            css.set("margin-top", twips(flow.pending_after));
        }
        if std::mem::take(&mut flow.page_break) {
            css.set("break-before", "page");
        }
        flow.pending_after = 0;
        flow.previous = None;
        if properties.float_position.is_some() {
            // A floating table's page position has no meaning on a web page;
            // it flows where it is anchored.
            self.losses
                .record("html.table_float_position", ModelOutcome::Degraded);
        }

        self.push("<table")?;
        if !css.is_empty() {
            self.push(" style=\"")?;
            self.push(&escape_attribute(&css.to_css()))?;
            self.push("\"")?;
        }
        self.push(">\n")?;
        if table.grid.iter().any(|column| column.width_twips.is_some()) {
            self.push("<colgroup>")?;
            for column in &table.grid {
                match column.width_twips {
                    Some(width) => {
                        self.push("<col style=\"width:")?;
                        self.push(&twips(i64::from(width.max(0))))?;
                        self.push("\">")?;
                    }
                    None => self.push("<col>")?,
                }
            }
            self.push("</colgroup>\n")?;
        }

        let deeper = Cx {
            depth: cx.depth + 1,
            layer: None,
        };
        // Header rows — those Word repeats on every page — are the table's
        // head, and their cells are header cells. A first row that is not one
        // is an ordinary row.
        let header_rows = table
            .rows
            .iter()
            .take_while(|row| row.properties.header)
            .count();
        for (row_index, row) in table.rows.iter().enumerate() {
            if row_index == 0 && header_rows > 0 {
                self.push("<thead>\n")?;
            }
            if row_index == header_rows {
                if header_rows > 0 {
                    self.push("</thead>\n")?;
                }
                self.push("<tbody>\n")?;
            }
            let header = row_index < header_rows;
            self.push("<tr")?;
            if let Some(height) = row
                .properties
                .height
                .value_twips
                .filter(|height| *height > 0)
                && row.properties.height.rule != Some(HeightRule::Auto)
            {
                self.push(" style=\"height:")?;
                self.push(&twips(i64::from(height)))?;
                self.push("\"")?;
            }
            self.push(">\n")?;
            for (cell_index, cell) in row.cells.iter().enumerate() {
                if cell.properties.vertical_merge == Some(VerticalMerge::Continue) {
                    continue;
                }
                let layer = &layers[row_index][cell_index];
                let tag = if header { "th" } else { "td" };
                self.push("<")?;
                self.push(tag)?;
                if let Some(span) = cell.properties.grid_span.filter(|span| *span > 1) {
                    self.push(&format!(" colspan=\"{span}\""))?;
                }
                let rows_spanned = vertical_span(table, &columns, row_index, cell_index);
                if rows_spanned > 1 {
                    self.push(&format!(" rowspan=\"{rows_spanned}\""))?;
                }
                let mut cell_css = Declarations::default();
                let candidates = &borders[row_index][cell_index];
                for (property, edge) in [
                    ("border-top", candidates.top.as_ref()),
                    ("border-bottom", candidates.bottom.as_ref()),
                    ("border-inline-start", candidates.start.as_ref()),
                    ("border-inline-end", candidates.end.as_ref()),
                ] {
                    if let Some(value) = edge.and_then(|edge| self.cell_edge(edge)) {
                        cell_css.set(property, value);
                    }
                }
                if let Some(fill) = self.palette.cell_fill(table, cell, layer) {
                    cell_css.set("background-color", hex(fill));
                }
                let margins = cell_margins(&cell.properties, &table.properties);
                for (property, value) in [
                    ("padding-top", margins.top.raw()),
                    ("padding-bottom", margins.bottom.raw()),
                    ("padding-inline-start", margins.start.raw()),
                    ("padding-inline-end", margins.end.raw()),
                ] {
                    if value > 0 {
                        cell_css.set(property, twips(i64::from(value)));
                    }
                }
                match cell.properties.vertical_alignment {
                    Some(CellVerticalAlignment::Center) => cell_css.set("vertical-align", "middle"),
                    Some(CellVerticalAlignment::Bottom) => cell_css.set("vertical-align", "bottom"),
                    _ => {}
                }
                if let Some(width) = cell.properties.width.as_ref().and_then(width_css) {
                    cell_css.set("width", width);
                }
                match cell.properties.text_direction {
                    Some(TextDirection::TbRl) => cell_css.set("writing-mode", "vertical-rl"),
                    Some(TextDirection::BtLr) => {
                        cell_css.set("writing-mode", "vertical-rl");
                        cell_css.set("transform", "rotate(180deg)");
                    }
                    _ => {}
                }
                if !cell_css.is_empty() {
                    self.push(" style=\"")?;
                    self.push(&escape_attribute(&cell_css.to_css()))?;
                    self.push("\"")?;
                }
                self.push(">\n")?;
                let mut cell_flow = Flow::default();
                let in_cell = Cx {
                    layer: Some(layer),
                    ..deeper
                };
                self.blocks(&cell.blocks, in_cell, &mut cell_flow)?;
                self.push("</")?;
                self.push(tag)?;
                self.push(">\n")?;
            }
            self.push("</tr>\n")?;
        }
        if header_rows == table.rows.len() && header_rows > 0 {
            self.push("</thead>\n")?;
        } else if !table.rows.is_empty() {
            self.push("</tbody>\n")?;
        }
        self.push("</table>\n")?;
        Ok(())
    }

    /// One cell side as CSS: the winning edge as the page draws it, `hidden`
    /// for an explicit `nil` (which suppresses the shared edge on both sides,
    /// as it does on the page), nothing for no border.
    fn cell_edge(&self, edge: &BorderEdge) -> Option<String> {
        if edge.style == "nil" {
            return Some("hidden".to_owned());
        }
        css::border_css(&self.palette, edge)
    }
}

/// A table or cell width as CSS, or `None` for automatic.
fn width_css(width: &TableWidth) -> Option<String> {
    match width.width_type {
        WidthType::Dxa if width.value > 0 => Some(twips(i64::from(width.value))),
        // Fiftieths of a percent: 5000 is the full width.
        WidthType::Pct if width.value > 0 => {
            Some(format!("{}%", round3(f64::from(width.value) / 50.0)))
        }
        _ => None,
    }
}

/// The grid column each cell starts in, `[row][cell]`: `w:gridBefore` plus
/// the spans before it. A vertical merge continues in the same *grid column*,
/// which is not the same cell index once a row has a span.
fn grid_columns(table: &Table) -> Vec<Vec<u32>> {
    table
        .rows
        .iter()
        .map(|row| {
            let mut column = row.properties.grid_before.unwrap_or(0);
            row.cells
                .iter()
                .map(|cell| {
                    let start = column;
                    column = column.saturating_add(cell.properties.grid_span.unwrap_or(1).max(1));
                    start
                })
                .collect()
        })
        .collect()
}

/// How many rows a cell's vertical merge spans, counting from `row_index`.
///
/// Complexity: O(rows below × cells per row) for a merged cell and O(1) for
/// the common case.
fn vertical_span(table: &Table, columns: &[Vec<u32>], row_index: usize, cell_index: usize) -> u32 {
    let start = table
        .rows
        .get(row_index)
        .and_then(|row| row.cells.get(cell_index));
    if start.map(|cell| cell.properties.vertical_merge) != Some(Some(VerticalMerge::Restart)) {
        return 1;
    }
    let column = columns[row_index][cell_index];
    let mut span = 1;
    for (offset, row) in table.rows.iter().enumerate().skip(row_index + 1) {
        let continues = columns[offset]
            .iter()
            .position(|start| *start == column)
            .and_then(|index| row.cells.get(index))
            .is_some_and(|cell| cell.properties.vertical_merge == Some(VerticalMerge::Continue));
        if !continues {
            break;
        }
        span += 1;
    }
    span
}
