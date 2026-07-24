//! Table-structure builder used by the body parser.
//!
//! The body parser is a flat state machine over `word/document.xml`. Tables are
//! the one nested block construct, so their partially-built state lives here in
//! a small stack instead of bloating the parser. The parser drives this builder
//! from `w:tbl`/`w:tr`/`w:tc` events; finished paragraphs (and nested tables)
//! are routed into the innermost open cell via [`TableStack::push_block`].

use casual_doc_model::v1::{
    BlockNode, GridColumn, MAX_TABLE_DEPTH, Paragraph, ParagraphProperties, Table, TableCell,
    TableCellProperties, TableRow, VerticalMerge,
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
    cells: Vec<TableCell>,
    cell: Option<CellBuilder>,
}

/// A table being accumulated.
struct TableBuilder {
    id: NodeId,
    grid: Vec<GridColumn>,
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

impl TableStack {
    /// Whether a table is currently open.
    pub(crate) fn is_active(&self) -> bool {
        !self.stack.is_empty()
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
        if let Some(row) = self.stack.last_mut().and_then(|table| table.row.as_mut()) {
            if let Some(cell) = row.cell.take() {
                row.cells.push(TableCell {
                    id: cell.id,
                    properties: cell.properties,
                    blocks: cell.blocks,
                });
            }
        }
        Ok(())
    }

    /// Closes the innermost open row, committing it to its table. Returns `false`
    /// when the row had no cells (caller reports it), so a degenerate row never
    /// produces an invalid model.
    pub(crate) fn close_row(&mut self) -> bool {
        if let Some(table) = self.stack.last_mut() {
            if let Some(row) = table.row.take() {
                if row.cells.is_empty() {
                    return false;
                }
                table.rows.push(TableRow {
                    id: row.id,
                    cells: row.cells,
                });
            }
        }
        true
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
            rows: table.rows,
        })
    }
}
