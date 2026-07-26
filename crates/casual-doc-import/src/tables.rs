//! Table-structure builder used by the body parser.
//!
//! The body parser is a flat state machine over `word/document.xml`. Tables are
//! the one nested block construct, so their partially-built state lives here in
//! a small stack instead of bloating the parser. The parser drives this builder
//! from `w:tbl`/`w:tr`/`w:tc` events; finished paragraphs (and nested tables)
//! are routed into the innermost open cell via [`TableStack::push_block`].

use casual_doc_model::v1::{Alignment, RgbColor};
use casual_doc_model::v1::{
    BlockNode, BorderEdge, CellMargins, CellVerticalAlignment, GridColumn, HeightRule,
    MAX_TABLE_DEPTH, Paragraph, ParagraphProperties, Table, TableBorders, TableCell,
    TableCellProperties, TableFloatPosition, TableLayout, TableOverlap, TableProperties, TableRow,
    TableRowProperties, TextDirection, VerticalMerge,
};
use casual_doc_model::{IdGenerator, NodeId};

use crate::error::ImportError;

/// A cell being accumulated inside an open row.
struct CellBuilder {
    id: NodeId,
    properties: TableCellProperties,
    blocks: Vec<BlockNode>,
}

/// A row being accumulated inside an open table.
struct RowBuilder {
    id: NodeId,
    properties: TableRowProperties,
    cells: Vec<TableCell>,
    cell: Option<CellBuilder>,
}

/// A table being accumulated.
struct TableBuilder {
    id: NodeId,
    grid: Vec<GridColumn>,
    properties: TableProperties,
    rows: Vec<TableRow>,
    row: Option<RowBuilder>,
}

/// The open-table stack (outermost first). Empty when no table is open.
#[derive(Default)]
pub(crate) struct TableStack {
    stack: Vec<TableBuilder>,
}

fn next_id(ids: &mut IdGenerator) -> Result<NodeId, ImportError> {
    ids.next_id()
        .map_err(|_| ImportError::LimitExceeded { limit: "node_ids" })
}

/// Routes a border edge to its field by OOXML local name (`start`/`left` and
/// `end`/`right` are the transitional aliases of the same logical edge).
fn set_border_field(borders: &mut TableBorders, edge: &[u8], value: BorderEdge) {
    match edge {
        b"top" => borders.top = Some(value),
        b"start" | b"left" => borders.start = Some(value),
        b"bottom" => borders.bottom = Some(value),
        b"end" | b"right" => borders.end = Some(value),
        b"insideH" => borders.inside_h = Some(value),
        b"insideV" => borders.inside_v = Some(value),
        _ => {}
    }
}

/// Routes a cell margin to its field by OOXML local name.
fn set_margin_field(margins: &mut CellMargins, edge: &[u8], twips: i32) {
    match edge {
        b"top" => margins.top_twips = Some(twips),
        b"start" | b"left" => margins.start_twips = Some(twips),
        b"bottom" => margins.bottom_twips = Some(twips),
        b"end" | b"right" => margins.end_twips = Some(twips),
        _ => {}
    }
}

impl TableStack {
    /// Whether a table is currently open.
    pub(crate) fn is_active(&self) -> bool {
        !self.stack.is_empty()
    }

    /// Whether the innermost open table has an open row with an open cell, i.e.
    /// block content routes into a cell right now. Used to distinguish a
    /// block-level content control inside a cell (allowed) from one sitting in a
    /// table's structural gap between rows/cells (deferred as passthrough).
    pub(crate) fn in_cell(&self) -> bool {
        self.stack
            .last()
            .and_then(|table| table.row.as_ref())
            .map(|row| row.cell.is_some())
            .unwrap_or(false)
    }

    fn current_cell(&mut self) -> Option<&mut CellBuilder> {
        self.stack.last_mut()?.row.as_mut()?.cell.as_mut()
    }

    /// Opens a new table, allocating its id in document order. Returns `false`
    /// (caller reports and treats the table transparently) when opening it would
    /// exceed [`MAX_TABLE_DEPTH`]; the model would otherwise reject the nesting.
    pub(crate) fn open_table(&mut self, ids: &mut IdGenerator) -> Result<bool, ImportError> {
        if self.stack.len() as u32 >= MAX_TABLE_DEPTH {
            return Ok(false);
        }
        let id = next_id(ids)?;
        self.stack.push(TableBuilder {
            id,
            grid: Vec::new(),
            properties: TableProperties::default(),
            rows: Vec::new(),
            row: None,
        });
        Ok(true)
    }

    /// Opens a row in the innermost table (no-op if no table is open).
    pub(crate) fn open_row(&mut self, ids: &mut IdGenerator) -> Result<(), ImportError> {
        if self.stack.is_empty() {
            return Ok(());
        }
        let id = next_id(ids)?;
        if let Some(table) = self.stack.last_mut() {
            table.row = Some(RowBuilder {
                id,
                properties: TableRowProperties::default(),
                cells: Vec::new(),
                cell: None,
            });
        }
        Ok(())
    }

    /// Opens a cell in the innermost open row (no-op if no row is open).
    pub(crate) fn open_cell(&mut self, ids: &mut IdGenerator) -> Result<(), ImportError> {
        let has_row = self
            .stack
            .last()
            .map(|table| table.row.is_some())
            .unwrap_or(false);
        if !has_row {
            return Ok(());
        }
        let id = next_id(ids)?;
        if let Some(row) = self.stack.last_mut().and_then(|table| table.row.as_mut()) {
            row.cell = Some(CellBuilder {
                id,
                properties: TableCellProperties::default(),
                blocks: Vec::new(),
            });
        }
        Ok(())
    }

    /// Adds a grid column to the innermost table.
    pub(crate) fn add_grid_column(&mut self, width_twips: Option<i32>) {
        if let Some(table) = self.stack.last_mut() {
            table.grid.push(GridColumn { width_twips });
        }
    }

    /// Sets the horizontal merge span on the current cell.
    pub(crate) fn set_grid_span(&mut self, span: u32) {
        if let Some(cell) = self.current_cell() {
            cell.properties.grid_span = Some(span);
        }
    }

    /// Sets the vertical merge role on the current cell.
    pub(crate) fn set_vertical_merge(&mut self, merge: VerticalMerge) {
        if let Some(cell) = self.current_cell() {
            cell.properties.vertical_merge = Some(merge);
        }
    }

    /// Sets the (dxa) width on the current cell.
    pub(crate) fn set_cell_width(&mut self, width_twips: i32) {
        if let Some(cell) = self.current_cell() {
            cell.properties.width_twips = Some(width_twips);
        }
    }

    /// Sets the background shading fill on the current cell.
    pub(crate) fn set_cell_shading(&mut self, fill: Option<RgbColor>) {
        if let Some(cell) = self.current_cell() {
            cell.properties.shading.fill = fill;
        }
    }

    /// Sets the vertical alignment on the current cell.
    pub(crate) fn set_cell_vertical_alignment(&mut self, alignment: CellVerticalAlignment) {
        if let Some(cell) = self.current_cell() {
            cell.properties.vertical_alignment = Some(alignment);
        }
    }

    /// Sets the no-wrap flag on the current cell.
    pub(crate) fn set_cell_no_wrap(&mut self, no_wrap: bool) {
        if let Some(cell) = self.current_cell() {
            cell.properties.no_wrap = no_wrap;
        }
    }

    /// Sets the text-flow direction on the current cell.
    pub(crate) fn set_cell_text_direction(&mut self, direction: TextDirection) {
        if let Some(cell) = self.current_cell() {
            cell.properties.text_direction = Some(direction);
        }
    }

    /// Mutable access to the innermost open table's properties, if a table is
    /// open (and, for row properties, a row is open).
    fn table_properties(&mut self) -> Option<&mut TableProperties> {
        self.stack.last_mut().map(|table| &mut table.properties)
    }

    fn row_properties(&mut self) -> Option<&mut TableRowProperties> {
        self.stack
            .last_mut()?
            .row
            .as_mut()
            .map(|row| &mut row.properties)
    }

    /// Sets the table alignment (`w:jc`).
    pub(crate) fn set_table_alignment(&mut self, alignment: Alignment) {
        if let Some(properties) = self.table_properties() {
            properties.alignment = Some(alignment);
        }
    }

    /// Sets the preferred table width in twips (`w:tblW` dxa).
    pub(crate) fn set_table_width(&mut self, width_twips: i32) {
        if let Some(properties) = self.table_properties() {
            properties.width_twips = Some(width_twips);
        }
    }

    /// Sets the table layout algorithm (`w:tblLayout`).
    pub(crate) fn set_table_layout(&mut self, layout: TableLayout) {
        if let Some(properties) = self.table_properties() {
            properties.layout = Some(layout);
        }
    }

    /// Sets the table indent in twips (`w:tblInd` dxa).
    pub(crate) fn set_table_indent(&mut self, twips: i32) {
        if let Some(properties) = self.table_properties() {
            properties.indent_twips = Some(twips);
        }
    }

    /// Sets the table-level default cell spacing in twips (`w:tblCellSpacing`).
    pub(crate) fn set_table_cell_spacing(&mut self, twips: i32) {
        if let Some(properties) = self.table_properties() {
            properties.cell_spacing_twips = Some(twips);
        }
    }

    /// Sets the floating-overlap behavior (`w:tblOverlap`).
    pub(crate) fn set_table_overlap(&mut self, overlap: TableOverlap) {
        if let Some(properties) = self.table_properties() {
            properties.overlap = Some(overlap);
        }
    }

    /// Sets the floating-table position (`w:tblpPr`).
    pub(crate) fn set_table_float_position(&mut self, position: TableFloatPosition) {
        if let Some(properties) = self.table_properties() {
            properties.float_position = Some(position);
        }
    }

    /// Sets the accessibility caption (`w:tblCaption`).
    pub(crate) fn set_table_caption(&mut self, caption: String) {
        if let Some(properties) = self.table_properties() {
            properties.caption = Some(caption);
        }
    }

    /// Sets the accessibility description (`w:tblDescription`).
    pub(crate) fn set_table_description(&mut self, description: String) {
        if let Some(properties) = self.table_properties() {
            properties.description = Some(description);
        }
    }

    /// Sets the row alignment (`w:trPr > w:jc`).
    pub(crate) fn set_row_alignment(&mut self, alignment: Alignment) {
        if let Some(properties) = self.row_properties() {
            properties.alignment = Some(alignment);
        }
    }

    /// Sets the per-row default cell spacing in twips (`w:trPr > w:tblCellSpacing`).
    pub(crate) fn set_row_cell_spacing(&mut self, twips: i32) {
        if let Some(properties) = self.row_properties() {
            properties.cell_spacing_twips = Some(twips);
        }
    }

    /// Sets the fit-text flag on the current cell (`w:tcFitText`).
    pub(crate) fn set_cell_fit_text(&mut self, on: bool) {
        if let Some(cell) = self.current_cell() {
            cell.properties.fit_text = on;
        }
    }

    /// Sets the hide-mark flag on the current cell (`w:hideMark`).
    pub(crate) fn set_cell_hide_mark(&mut self, on: bool) {
        if let Some(cell) = self.current_cell() {
            cell.properties.hide_mark = on;
        }
    }

    /// Sets the table background shading fill (`w:shd`).
    pub(crate) fn set_table_shading(&mut self, fill: Option<RgbColor>) {
        if let Some(properties) = self.table_properties() {
            properties.shading.fill = fill;
        }
    }

    /// Sets one `w:tblLook` conditional-format flag by its attribute name.
    pub(crate) fn set_table_look_flag(&mut self, flag: &[u8], on: bool) {
        if let Some(properties) = self.table_properties() {
            match flag {
                b"firstRow" => properties.look.first_row = on,
                b"lastRow" => properties.look.last_row = on,
                b"firstColumn" => properties.look.first_column = on,
                b"lastColumn" => properties.look.last_column = on,
                b"noHBand" => properties.look.no_h_band = on,
                b"noVBand" => properties.look.no_v_band = on,
                _ => {}
            }
        }
    }

    /// Sets the row height value and rule (`w:trHeight`).
    pub(crate) fn set_row_height(&mut self, value_twips: Option<u32>, rule: Option<HeightRule>) {
        if let Some(properties) = self.row_properties() {
            properties.height.value_twips = value_twips;
            properties.height.rule = rule;
        }
    }

    /// Sets the row cant-split flag (`w:cantSplit`).
    pub(crate) fn set_row_cant_split(&mut self, cant_split: bool) {
        if let Some(properties) = self.row_properties() {
            properties.cant_split = cant_split;
        }
    }

    /// Sets the row header-repeat flag (`w:tblHeader`).
    pub(crate) fn set_row_header(&mut self, header: bool) {
        if let Some(properties) = self.row_properties() {
            properties.header = header;
        }
    }

    /// Sets one border edge (by OOXML local name) on the innermost table.
    pub(crate) fn set_table_border(&mut self, edge: &[u8], border: BorderEdge) {
        if let Some(properties) = self.table_properties() {
            set_border_field(&mut properties.borders, edge, border);
        }
    }

    /// Sets one border edge on the current cell.
    pub(crate) fn set_cell_border(&mut self, edge: &[u8], border: BorderEdge) {
        if let Some(cell) = self.current_cell() {
            set_border_field(&mut cell.properties.borders, edge, border);
        }
    }

    /// Sets one default cell margin (by OOXML local name) on the innermost table.
    pub(crate) fn set_table_margin(&mut self, edge: &[u8], twips: i32) {
        if let Some(properties) = self.table_properties() {
            set_margin_field(&mut properties.cell_margins, edge, twips);
        }
    }

    /// Sets one content margin on the current cell.
    pub(crate) fn set_cell_margin(&mut self, edge: &[u8], twips: i32) {
        if let Some(cell) = self.current_cell() {
            set_margin_field(&mut cell.properties.margins, edge, twips);
        }
    }

    /// Routes a finished block into the innermost open cell. Returns `Some(block)`
    /// when no cell is open, so the caller sends it to the body root instead.
    pub(crate) fn push_block(&mut self, block: BlockNode) -> Option<BlockNode> {
        match self.current_cell() {
            Some(cell) => {
                cell.blocks.push(block);
                None
            }
            None => Some(block),
        }
    }

    /// Closes the innermost open cell, committing it to its row. A cell with no
    /// blocks gets a synthesized empty paragraph so the model invariant (a cell
    /// holds at least one block) holds.
    pub(crate) fn close_cell(&mut self, ids: &mut IdGenerator) -> Result<(), ImportError> {
        let empty = match self.current_cell() {
            Some(cell) => cell.blocks.is_empty(),
            None => return Ok(()),
        };
        if empty {
            let id = next_id(ids)?;
            if let Some(cell) = self.current_cell() {
                cell.blocks.push(BlockNode::Paragraph(Paragraph {
                    id,
                    properties: ParagraphProperties::default(),
                    inlines: Vec::new(),
                }));
            }
        }
        if let Some(row) = self.stack.last_mut().and_then(|table| table.row.as_mut())
            && let Some(cell) = row.cell.take()
        {
            row.cells.push(TableCell {
                id: cell.id,
                properties: cell.properties,
                blocks: cell.blocks,
            });
        }
        Ok(())
    }

    /// Closes the innermost open row, committing it to its table. Returns `false`
    /// when the row had no cells (caller reports it), so a degenerate row never
    /// produces an invalid model.
    pub(crate) fn close_row(&mut self) -> bool {
        if let Some(table) = self.stack.last_mut()
            && let Some(row) = table.row.take()
        {
            if row.cells.is_empty() {
                return false;
            }
            table.rows.push(TableRow {
                id: row.id,
                properties: row.properties,
                cells: row.cells,
            });
        }
        true
    }

    /// Force-closes every still-open table (innermost first), committing any
    /// partial cell and row, and returns the finished top-level `Table` blocks in
    /// document order. Used at a container boundary (EOF, note/comment/header
    /// close) so a table left open by truncated input is neither dropped nor
    /// bled into the next container — the stack is emptied. A table (or nested
    /// table) that closes with no rows carries no content and is discarded.
    pub(crate) fn flush_open(
        &mut self,
        ids: &mut IdGenerator,
    ) -> Result<Vec<BlockNode>, ImportError> {
        let mut roots = Vec::new();
        while self.is_active() {
            // Commit the innermost open cell and row before closing the table so
            // their partial content is preserved, then fold the finished table
            // into its parent cell (nested) or the returned roots (top level).
            self.close_cell(ids)?;
            self.close_row();
            if let Some(table) = self.close_table()
                && let Some(returned) = self.push_block(BlockNode::Table(table))
            {
                roots.push(returned);
            }
        }
        Ok(roots)
    }

    /// Closes the innermost table, returning the finished `Table`. Returns `None`
    /// when the table had no rows (caller reports it), so a degenerate table is
    /// dropped rather than producing an invalid model.
    pub(crate) fn close_table(&mut self) -> Option<Table> {
        let table = self.stack.pop()?;
        if table.rows.is_empty() {
            return None;
        }
        Some(Table {
            id: table.id,
            grid: table.grid,
            properties: table.properties,
            rows: table.rows,
        })
    }
}
