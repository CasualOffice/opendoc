// SPDX-License-Identifier: Apache-2.0

//! Table-structure lowerings the editor and agents share (`docs/181` §4.2,
//! step 1).
//!
//! The merged-cell rules live in `casual_doc_edit::table_structure` (`docs/171`);
//! what lives here is the choice of operations: one row inserted or deleted and
//! nothing else changed is the fine-grained `InsertRow`/`DeleteRow` a concurrent
//! edit transforms against, and anything more (a merge the rule grew or
//! repaired) is the whole-table `ReplaceTable`. Before this module the editor
//! made that choice inside its wasm method and the AI layer skipped both the
//! rule and the choice, so an agent's row edit through a vertical merge tore
//! the merge a person's edit kept.

use casual_doc_edit::table_structure::{self, GridRect, MintId};
use casual_doc_edit::{Operation, refused};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, BorderEdge, CellVerticalAlignment, Document, GridColumn, Paragraph,
    ParagraphProperties, RgbColor, Table, TableCell, TableCellProperties, TableGridMap,
    TableProperties, TableRow, TableRowProperties, TableWidth,
};
use casual_doc_selection::TableCellSelection;

/// A lowered row edit.
#[derive(Clone, Debug)]
pub struct RowEdit {
    /// The operations, applied in order as one step.
    pub operations: Vec<Operation>,
    /// The table after the edit.
    pub table: Table,
    /// The row the caret belongs in afterwards: the new row for an insert, the
    /// row now at the deleted position (else the one before it) for a delete.
    pub row: usize,
}

/// Inserts an empty row above (`below == false`) or below row `row`.
/// O(cells of the table).
///
/// # Errors
/// When `row` is not a row of `table`, or ids run out.
pub fn insert_row(
    table: &Table,
    row: usize,
    below: bool,
    ids: MintId<'_>,
) -> Result<RowEdit, String> {
    let (replacement, at) = table_structure::insert_row(table, row, below, ids)?;
    let mut without = replacement.clone();
    without.rows.remove(at);
    let operations = if without == *table {
        vec![Operation::InsertRow {
            table: table.id,
            index: u32::try_from(at).map_err(|_| "the table is too long".to_owned())?,
            row: Box::new(replacement.rows[at].clone()),
        }]
    } else {
        vec![Operation::ReplaceTable {
            table: table.id,
            replacement: Box::new(replacement.clone()),
        }]
    };
    Ok(RowEdit {
        operations,
        table: replacement,
        row: at,
    })
}

/// Inserts an empty row so that it lands at index `index` (0 = above the
/// first row, `rows` = below the last), formatted like the row it follows (the
/// first row when `index` is 0). O(cells of the table).
///
/// # Errors
/// When `index` is past the end of `table`, or ids run out.
pub fn insert_row_at(table: &Table, index: usize, ids: MintId<'_>) -> Result<RowEdit, String> {
    if index > table.rows.len() {
        return Err("The row index is past the end of the table.".to_owned());
    }
    if index == 0 {
        insert_row(table, 0, false, ids)
    } else {
        insert_row(table, index - 1, true, ids)
    }
}

/// Deletes row `row`. A merged cell anchored in it that continues below keeps
/// its content and properties on its first surviving row. O(cells of the
/// table).
///
/// # Errors
/// When `row` is not a row of `table`, or it is the table's only row.
pub fn delete_row(table: &Table, row: usize) -> Result<RowEdit, String> {
    let replacement = table_structure::delete_rows(table, row, row)?;
    let mut expected = table.clone();
    expected.rows.remove(row);
    let operations = if expected == replacement {
        vec![Operation::DeleteRow {
            table: table.id,
            index: u32::try_from(row).map_err(|_| "the table is too long".to_owned())?,
        }]
    } else {
        vec![Operation::ReplaceTable {
            table: table.id,
            replacement: Box::new(replacement.clone()),
        }]
    };
    let row = row.min(replacement.rows.len().saturating_sub(1));
    Ok(RowEdit {
        operations,
        table: replacement,
        row,
    })
}

/// A lowered table edit other than a row edit.
#[derive(Clone, Debug)]
pub struct TableEdit {
    /// The operations, applied in order as one step.
    pub operations: Vec<Operation>,
    /// The paragraph the caret belongs in afterwards, when the edit names one.
    pub caret: Option<NodeId>,
}

/// The first paragraph directly inside `cell`. O(blocks of the cell).
#[must_use]
pub fn first_paragraph_of_cell(cell: &TableCell) -> Option<NodeId> {
    cell.blocks.iter().find_map(|block| match block {
        BlockNode::Paragraph(paragraph) => Some(paragraph.id),
        _ => None,
    })
}

fn mint(ids: MintId<'_>) -> Result<NodeId, String> {
    ids().ok_or_else(|| "id space exhausted".to_owned())
}

/// `count` empty cells with default properties, each holding one empty
/// paragraph; per cell the cell id is minted before its paragraph's. O(count).
///
/// # Errors
/// When ids run out.
pub fn empty_cells(count: usize, ids: MintId<'_>) -> Result<Vec<TableCell>, String> {
    let mut cells = Vec::with_capacity(count);
    for _ in 0..count {
        let id = mint(ids)?;
        let paragraph = mint(ids)?;
        cells.push(TableCell {
            id,
            properties: TableCellProperties::default(),
            blocks: vec![BlockNode::Paragraph(Paragraph {
                id: paragraph,
                properties: ParagraphProperties::default().into(),
                inlines: Vec::new(),
            })],
        });
    }
    Ok(cells)
}

/// Inserts an empty column at grid boundary `boundary` (0 = before the first
/// column), as wide as grid column `beside`. A regular table gets the
/// fine-grained `InsertColumn` a concurrent edit transforms against, with fresh
/// default cells; a table with merges gets the `docs/171` rule
/// (`table_structure::insert_column`: a straddling merge grows) as a
/// `ReplaceTable`. The caret goes to the new cell in grid row `row`.
/// O(cells of the table).
///
/// # Errors
/// When `boundary` is past the grid, or ids run out.
pub fn insert_column(
    table: &Table,
    row: usize,
    boundary: usize,
    beside: usize,
    ids: MintId<'_>,
) -> Result<TableEdit, String> {
    let grid = TableGridMap::build(table);
    if boundary > grid.column_count() {
        return Err("The column index is past the end of the table.".to_owned());
    }
    let width = table.grid.get(beside).and_then(|column| column.width_twips);
    if grid.is_regular() {
        let cells = empty_cells(table.rows.len(), ids)?;
        let caret = cells.get(row).and_then(first_paragraph_of_cell);
        return Ok(TableEdit {
            operations: vec![Operation::InsertColumn {
                table: table.id,
                index: u32::try_from(boundary).map_err(|_| "the table is too wide".to_owned())?,
                width,
                cells,
            }],
            caret,
        });
    }
    let replacement = table_structure::insert_column(table, boundary, width, ids)?;
    let caret = TableGridMap::build(&replacement)
        .cell_at(row, boundary)
        .and_then(|slot| replacement.rows[slot.row].cells.get(slot.cell))
        .and_then(first_paragraph_of_cell);
    Ok(TableEdit {
        operations: vec![Operation::ReplaceTable {
            table: table.id,
            replacement: Box::new(replacement),
        }],
        caret,
    })
}

/// Inserts an empty column beside the cell at physical (`row`, `cell`): right
/// of all of it when `after`, else left of it, as wide as the grid column it
/// is beside. What the editor's Insert Column Left/Right does. O(cells of the
/// table).
///
/// # Errors
/// When the cell is not in the table, or ids run out.
pub fn insert_column_beside(
    table: &Table,
    row: usize,
    cell: usize,
    after: bool,
    ids: MintId<'_>,
) -> Result<TableEdit, String> {
    let slot = *TableGridMap::build(table)
        .cell(row, cell)
        .ok_or_else(|| "caret is not inside a table".to_owned())?;
    let (boundary, beside) = if after {
        (slot.end_column(), slot.last_column())
    } else {
        (slot.first_column, slot.first_column)
    };
    insert_column(table, row, boundary, beside, ids)
}

/// Inserts an empty column so it lands at grid column `index` (0 = left of the
/// first, the column count = right of the last), as wide as its left
/// neighbour (the first column when `index` is 0). O(cells of the table).
///
/// # Errors
/// As [`insert_column`].
pub fn insert_column_at(table: &Table, index: usize, ids: MintId<'_>) -> Result<TableEdit, String> {
    insert_column(table, 0, index, index.saturating_sub(1), ids)
}

/// Deletes grid columns `first..=last`, refusing the table's last column. A
/// regular table losing one column gets the fine-grained `DeleteColumn`; any
/// other case the `docs/171` rule's `ReplaceTable`. The caret goes to the cell
/// now at `first` in row `row` (else the one before it).
/// O(cells of the table).
///
/// # Errors
/// When the range is outside the table, or is every column.
pub fn delete_columns(
    table: &Table,
    row: usize,
    first: usize,
    last: usize,
) -> Result<TableEdit, String> {
    let regular = TableGridMap::build(table).is_regular();
    let replacement = table_structure::delete_columns(table, first, last)?;
    let new_grid = TableGridMap::build(&replacement);
    let row = row.min(replacement.rows.len().saturating_sub(1));
    let column = first.min(new_grid.column_count().saturating_sub(1));
    let caret = new_grid
        .cell_at(row, column)
        .or_else(|| new_grid.row(row).last())
        .and_then(|slot| new_grid.anchor_of(slot.row, slot.cell))
        .and_then(|(r, c)| replacement.rows[r].cells.get(c))
        .and_then(first_paragraph_of_cell);
    let operations = if regular && first == last {
        vec![Operation::DeleteColumn {
            table: table.id,
            index: u32::try_from(first).map_err(|_| "the table is too wide".to_owned())?,
        }]
    } else {
        vec![Operation::ReplaceTable {
            table: table.id,
            replacement: Box::new(replacement),
        }]
    };
    Ok(TableEdit { operations, caret })
}

/// Deletes every grid column the cell at physical (`row`, `cell`) covers — all
/// of a merged cell, as Word does. O(cells of the table).
///
/// # Errors
/// As [`delete_columns`], or when the cell is not in the table.
pub fn delete_column_of_cell(table: &Table, row: usize, cell: usize) -> Result<TableEdit, String> {
    let grid = TableGridMap::build(table);
    let slot = *grid
        .cell(row, cell)
        .ok_or_else(|| "caret is not inside a table".to_owned())?;
    let region = *grid.region(slot.region);
    delete_columns(table, row, region.first_column, region.last_column)
}

/// The grid rectangle between two physical cells, grown until it contains
/// every merged cell it touches (`casual_doc_selection`'s rule, the one a
/// dragged selection uses). O(cells of the table).
///
/// # Errors
/// When either cell is not in the table.
pub fn cell_rect(
    table: &Table,
    from: (usize, usize),
    to: (usize, usize),
) -> Result<GridRect, String> {
    let cell = |(row, cell): (usize, usize)| {
        table
            .rows
            .get(row)
            .and_then(|r| r.cells.get(cell))
            .map(|c| c.id)
            .ok_or_else(|| "That table has no such cell.".to_owned())
    };
    let range = TableCellSelection::new(table.id, cell(from)?, cell(to)?)
        .normalised(table)
        .map_err(|error| error.to_string())?;
    Ok(GridRect {
        first_row: range.first_row(),
        last_row: range.last_row(),
        first_column: range.first_column(),
        last_column: range.last_column(),
    })
}

/// Merges grid rectangle `rect` (already grown to whole merged cells) into one
/// cell, its content moved into the merged cell in reading order
/// (`table_structure::merge_rect`, `docs/171`). The caret goes to the merged
/// cell. O(cells of the table).
///
/// # Errors
/// When the rectangle is one cell, outside the table, or crosses a row gap.
pub fn merge(table: &Table, rect: GridRect, ids: MintId<'_>) -> Result<TableEdit, String> {
    if rect.first_row == rect.last_row && rect.first_column == rect.last_column {
        return Err(refused!(
            "table.merge-too-few-cells",
            "Select at least two cells to merge."
        )
        .to_owned());
    }
    let (replacement, (row, cell)) = table_structure::merge_rect(table, rect, ids)?;
    let caret = replacement.rows[row]
        .cells
        .get(cell)
        .and_then(first_paragraph_of_cell);
    Ok(TableEdit {
        operations: vec![Operation::ReplaceTable {
            table: table.id,
            replacement: Box::new(replacement),
        }],
        caret,
    })
}

/// Unmerges the merged cell physical cell (`row`, `cell`) belongs to, from any
/// of its rows (`table_structure::unmerge`, `docs/171`). The caret goes to the
/// merged cell's top-left. O(cells of the table).
///
/// # Errors
/// When the cell is not merged or not in the table.
pub fn unmerge(
    table: &Table,
    row: usize,
    cell: usize,
    ids: MintId<'_>,
) -> Result<TableEdit, String> {
    let (row, cell) = TableGridMap::build(table)
        .anchor_of(row, cell)
        .ok_or_else(|| "caret is not inside a table".to_owned())?;
    let replacement = table_structure::unmerge(table, row, cell, ids)?;
    let caret = replacement
        .rows
        .get(row)
        .and_then(|r| r.cells.get(cell))
        .and_then(first_paragraph_of_cell);
    Ok(TableEdit {
        operations: vec![Operation::ReplaceTable {
            table: table.id,
            replacement: Box::new(replacement),
        }],
        caret,
    })
}

/// Deletes table `table` whole. O(1).
#[must_use]
pub fn delete_table(table: NodeId) -> Vec<Operation> {
    vec![Operation::DeleteTable { table }]
}

/// The thin single black line a new table is drawn with, so it is visible.
fn grid_line() -> BorderEdge {
    BorderEdge {
        style: "single".to_owned(),
        size_eighth_points: Some(4),
        color: Some(RgbColor { r: 0, g: 0, b: 0 }),
        space_points: None,
        theme_color: None,
    }
}

/// Draws the thin single grid a new table gets on every edge of `properties`,
/// so a freshly made table is visible. O(1).
pub(crate) fn grid_borders(properties: &mut TableProperties) {
    let borders = &mut properties.borders;
    borders.top = Some(grid_line());
    borders.bottom = Some(grid_line());
    borders.start = Some(grid_line());
    borders.end = Some(grid_line());
    borders.inside_h = Some(grid_line());
    borders.inside_v = Some(grid_line());
}

/// The most rows and columns a new table may have.
pub const MAX_NEW_TABLE: (u32, u32) = (50, 20);

/// Inserts a fresh `rows`×`columns` table (clamped to 1..=50 by 1..=20) at
/// body index `index`, with equal columns spanning `content_width` twips and a
/// thin single grid on every edge. Ids are minted per cell (cell, then its
/// paragraph), then the row, and the table last. The caret goes to the first
/// cell. O(rows × columns).
///
/// # Errors
/// When ids run out or the body index overflows.
pub fn insert_table(
    index: usize,
    rows: u32,
    columns: u32,
    content_width: i32,
    ids: MintId<'_>,
) -> Result<TableEdit, String> {
    insert_table_in(None, index, rows, columns, content_width, ids)
}

/// [`insert_table`] into `container` (`None` = the body, else the table cell
/// or content control whose blocks receive it): how a table is nested inside a
/// cell (`docs/153` `table.nested`). `content_width` is then the cell's own
/// content width (`table_tools::cell_content_width`). O(rows x columns).
///
/// # Errors
/// When ids run out or the index overflows.
pub fn insert_table_in(
    container: Option<NodeId>,
    index: usize,
    rows: u32,
    columns: u32,
    content_width: i32,
    ids: MintId<'_>,
) -> Result<TableEdit, String> {
    let rows = rows.clamp(1, MAX_NEW_TABLE.0) as usize;
    let columns = columns.clamp(1, MAX_NEW_TABLE.1) as usize;
    let count = i32::try_from(columns).unwrap_or(1);
    let width = content_width.max(count) / count;
    let grid = (0..columns)
        .map(|_| GridColumn {
            width_twips: Some(width),
        })
        .collect();
    let mut table_rows = Vec::with_capacity(rows);
    for _ in 0..rows {
        let mut cells = Vec::with_capacity(columns);
        for _ in 0..columns {
            let id = mint(ids)?;
            let paragraph = mint(ids)?;
            cells.push(TableCell {
                id,
                properties: TableCellProperties {
                    width: Some(TableWidth::dxa(width)),
                    ..TableCellProperties::default()
                },
                blocks: vec![BlockNode::Paragraph(Paragraph {
                    id: paragraph,
                    properties: ParagraphProperties::default().into(),
                    inlines: Vec::new(),
                })],
            });
        }
        table_rows.push(TableRow {
            id: mint(ids)?,
            properties: TableRowProperties::default(),
            cells,
        });
    }
    let mut properties = TableProperties::default();
    grid_borders(&mut properties);
    let table = Table {
        id: mint(ids)?,
        grid,
        grid_change: None,
        properties,
        rows: table_rows,
    };
    let caret = table
        .rows
        .first()
        .and_then(|row| row.cells.first())
        .and_then(first_paragraph_of_cell);
    Ok(TableEdit {
        operations: vec![Operation::InsertTable {
            container,
            index: u32::try_from(index).map_err(|_| "the body is too long".to_owned())?,
            table: Box::new(table),
        }],
        caret,
    })
}

/// The body index right after the top-level block holding paragraph `node`
/// (at any depth), else the end of the body: where a new table goes. O(body).
#[must_use]
pub fn body_index_after(document: &Document, node: NodeId) -> usize {
    let blocks = document.body();
    blocks
        .iter()
        .position(|block| holds_paragraph(block, node))
        .map_or(blocks.len(), |index| index + 1)
}

/// Whether `block` holds paragraph `node`, at any depth. O(block).
#[must_use]
pub fn holds_paragraph(block: &BlockNode, node: NodeId) -> bool {
    match block {
        BlockNode::Paragraph(paragraph) => paragraph.id == node,
        BlockNode::Table(table) => table.rows.iter().any(|row| {
            row.cells
                .iter()
                .any(|cell| cell.blocks.iter().any(|b| holds_paragraph(b, node)))
        }),
        BlockNode::Sdt(sdt) => sdt.blocks.iter().any(|b| holds_paragraph(b, node)),
        BlockNode::AltChunk(_) => false,
    }
}

/// Every cell of the rectangle between two physical cells, grown to whole
/// merged cells (each merged cell once), with its current properties: the
/// cells a cell-format command applies to. O(cells of the table) per cell.
///
/// # Errors
/// When either cell is not in the table.
pub fn range_cells(
    table: &Table,
    from: (usize, usize),
    to: (usize, usize),
) -> Result<Vec<(NodeId, TableCellProperties)>, String> {
    let cell = |(row, cell): (usize, usize)| {
        table
            .rows
            .get(row)
            .and_then(|r| r.cells.get(cell))
            .map(|c| c.id)
            .ok_or_else(|| "That table has no such cell.".to_owned())
    };
    let range = TableCellSelection::new(table.id, cell(from)?, cell(to)?)
        .normalised(table)
        .map_err(|error| error.to_string())?;
    Ok(range
        .cells()
        .iter()
        .filter_map(|id| {
            table
                .rows
                .iter()
                .flat_map(|row| row.cells.iter())
                .find(|candidate| candidate.id == *id)
                .map(|found| (found.id, found.properties.clone()))
        })
        .collect())
}

/// One cell-property change for every cell named, as one step: the editor's
/// cell-format commands and an agent's share it. O(cells named).
#[must_use]
pub fn set_cell_properties(
    cells: Vec<(NodeId, TableCellProperties)>,
    change: impl Fn(&mut TableCellProperties),
) -> Vec<Operation> {
    cells
        .into_iter()
        .map(|(cell, mut properties)| {
            change(&mut properties);
            Operation::SetTableCellProperties {
                cell,
                properties: Box::new(properties),
            }
        })
        .collect()
}

/// A cell vertical alignment by name: `center`, `bottom`, anything else top.
#[must_use]
pub fn cell_vertical_alignment(name: &str) -> CellVerticalAlignment {
    match name {
        "center" => CellVerticalAlignment::Center,
        "bottom" => CellVerticalAlignment::Bottom,
        _ => CellVerticalAlignment::Top,
    }
}
