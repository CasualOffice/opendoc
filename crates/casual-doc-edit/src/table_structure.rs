//! Structural table edits through the ONE grid map (`docs/171` §3).
//!
//! Every function here is a pure `Table -> Table` transform that resolves "which
//! cells, which spans" through [`TableGridMap`], never through a cell index:
//! a row's `cells[i]` is grid column `i` only until some cell carries
//! `w:gridSpan`, some row starts after `w:gridBefore`, or a `w:vMerge`
//! continuation sits under a merged cell. The facade (`casual-doc-wasm`) commits
//! the result as an operation; these functions decide what the result is.
//!
//! The rules are Word's and Google Docs', and where the two differ the choice is
//! stated on the function:
//!
//! | Edit | Rule |
//! | --- | --- |
//! | Insert row | The new row copies the anchor row's spans. A vertically merged cell the new row lands INSIDE (between its first and last row) grows by one row; one it lands outside of is not touched. |
//! | Delete rows | A merged cell whose top row is deleted keeps its content and properties: they move to its first surviving row, which becomes the new top. |
//! | Insert column | A cell that straddles the insertion line (a horizontal merge across it) grows by one column; every other row gets a new cell there. |
//! | Delete columns | A cell partly inside the deleted range shrinks; one wholly inside is removed. |
//! | Merge | Any rectangle, expanded until it contains whole merged cells. Each non-empty cell's content moves into the merged cell, in reading order. |
//! | Unmerge | A merged cell becomes one cell per grid cell it covered; the content stays in the top-left. |
//! | Move rows / columns | The unit moved is a BAND: the rows (columns) that no merged cell crosses out of. A merged cell is never torn. |
//!
//! After every edit, [`normalize_vertical_merges`] removes the two degenerate
//! states an edit can leave behind — a `restart` nothing continues, and a
//! `continue` that joins nothing — so no edit can write a `w:vMerge` that a
//! reader of the file would see differently from this editor.
//!
//! Complexity: every function is O(cells of the table) plus at most two grid
//! builds; none touches the document. `table_grid_cell_visits` measures it.

use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, GridColumn, Paragraph, ParagraphProperties, Table, TableCell, TableCellProperties,
    TableGridMap, TableRow, TableWidth, VerticalMerge,
};

use crate::refused;

/// Mints a fresh node id, or `None` when the id space is exhausted.
pub type MintId<'a> = &'a mut dyn FnMut() -> Option<NodeId>;

const EXHAUSTED: &str = "id space exhausted";

fn next(ids: MintId<'_>) -> Result<NodeId, String> {
    ids().ok_or_else(|| EXHAUSTED.to_owned())
}

fn empty_paragraph(ids: MintId<'_>) -> Result<BlockNode, String> {
    Ok(BlockNode::Paragraph(Paragraph {
        id: next(ids)?,
        properties: ParagraphProperties::default().into(),
        inlines: Vec::new(),
    }))
}

/// A cell's properties with everything that belongs to ITS position or history
/// removed: span, vertical merge, width, and tracked-change records. What a new
/// cell inherits from the cell it was made beside — its borders, shading,
/// alignment and margins — is what is left, as Word copies it.
fn formatting_of(properties: &TableCellProperties) -> TableCellProperties {
    TableCellProperties {
        grid_span: None,
        vertical_merge: None,
        width: None,
        cell_revision: None,
        cell_merge: None,
        prop_change: None,
        ..properties.clone()
    }
}

fn fresh_cell(ids: MintId<'_>, properties: TableCellProperties) -> Result<TableCell, String> {
    Ok(TableCell {
        id: next(ids)?,
        properties,
        blocks: vec![empty_paragraph(ids)?],
    })
}

/// Whether a cell holds anything a reader would lose: any block that is not an
/// empty paragraph.
fn has_content(cell: &TableCell) -> bool {
    cell.blocks.iter().any(|block| match block {
        BlockNode::Paragraph(paragraph) => !paragraph.inlines.is_empty(),
        _ => true,
    })
}

/// Removes the degenerate vertical-merge states an edit can leave: a
/// `continue` that joins no merge above it becomes an ordinary cell (its content
/// stays visible, as layout already shows it), and a `restart` that nothing
/// continues becomes an ordinary cell. **O(cells).**
pub fn normalize_vertical_merges(table: &mut Table) {
    let grid = TableGridMap::build(table);
    for (r, row) in table.rows.iter_mut().enumerate() {
        for (c, cell) in row.cells.iter_mut().enumerate() {
            let Some(slot) = grid.cell(r, c) else {
                continue;
            };
            match cell.properties.vertical_merge {
                Some(VerticalMerge::Continue) if !slot.continuation => {
                    cell.properties.vertical_merge = None;
                }
                Some(VerticalMerge::Restart) if grid.region(slot.region).row_count() == 1 => {
                    cell.properties.vertical_merge = None;
                }
                _ => {}
            }
        }
    }
}

/// Re-derives every cell's preferred width (`w:tcW`) from the grid columns it
/// covers, wherever the grid states all of them. A cell whose width is a
/// percentage or `auto` is left alone: it never claimed a twip width. Without
/// this, a span change leaves a cell asking for the width it had before.
/// **O(cells).**
pub fn sync_cell_widths(table: &mut Table) {
    let grid = TableGridMap::build(table);
    let widths: Vec<Option<i32>> = table.grid.iter().map(|g| g.width_twips).collect();
    for (r, row) in table.rows.iter_mut().enumerate() {
        for (c, cell) in row.cells.iter_mut().enumerate() {
            let Some(slot) = grid.cell(r, c) else {
                continue;
            };
            let twip_or_unset = cell
                .properties
                .width
                .is_none_or(|w| w.dxa_twips().is_some());
            if !twip_or_unset {
                continue;
            }
            let total: Option<i32> = widths
                .get(slot.first_column..slot.end_column())
                .and_then(|columns| columns.iter().copied().sum());
            if let Some(total) = total {
                cell.properties.width = Some(TableWidth::dxa(total));
            }
        }
    }
}

fn finish(mut table: Table) -> Table {
    normalize_vertical_merges(&mut table);
    sync_cell_widths(&mut table);
    table
}

/// Inserts a row above (`below == false`) or below the row `row`, returning the
/// table and the new row's index.
///
/// The new row copies the anchor row's spans and cell formatting, empty. A
/// vertically merged cell the new row lands INSIDE — strictly after its first
/// row and no later than its last — grows to cover the new row (the new cell
/// there is a continuation); a merged cell the new row lands above or below is
/// untouched. That is Word's and ONLYOFFICE's rule; the old code copied the
/// anchor row's `w:vMerge` verbatim, so a row inserted below the top of a merge
/// started a second merge and tore the first.
///
/// # Errors
/// When `row` is not a row of the table, or ids run out.
pub fn insert_row(
    table: &Table,
    row: usize,
    below: bool,
    ids: MintId<'_>,
) -> Result<(Table, usize), String> {
    let template = table
        .rows
        .get(row)
        .ok_or_else(|| "row is outside the table".to_owned())?;
    let grid = TableGridMap::build(table);
    let at = if below { row + 1 } else { row };
    let mut cells = Vec::with_capacity(template.cells.len());
    for (c, cell) in template.cells.iter().enumerate() {
        let slot = grid
            .cell(row, c)
            .ok_or_else(|| "row is outside the table".to_owned())?;
        let region = grid.region(slot.region);
        let inside = region.first_row < at && at <= region.last_row;
        let mut properties = formatting_of(&cell.properties);
        properties.grid_span = cell.properties.grid_span;
        properties.width = cell.properties.width;
        if inside {
            properties.vertical_merge = Some(VerticalMerge::Continue);
        }
        cells.push(fresh_cell(ids, properties)?);
    }
    let mut properties = template.properties.clone();
    properties.row_revision = None;
    properties.prop_change = None;
    let mut out = table.clone();
    out.rows.insert(
        at,
        TableRow {
            id: next(ids)?,
            properties,
            cells,
        },
    );
    Ok((finish(out), at))
}

/// Deletes rows `first..=last`.
///
/// A merged cell whose TOP row is deleted while it continues below the range
/// keeps its content and its properties: both move to its first surviving row,
/// which becomes the merge's new top. ONLYOFFICE discards the content there
/// (`docs/171` §2); losing the one cell of a merged band a reader can see is
/// not a structural side effect anyone asks for.
///
/// # Errors
/// When the range is empty, outside the table, or every row (delete the table
/// instead).
pub fn delete_rows(table: &Table, first: usize, last: usize) -> Result<Table, String> {
    if first > last || last >= table.rows.len() {
        return Err("row is outside the table".to_owned());
    }
    if first == 0 && last + 1 == table.rows.len() {
        return Err(refused!(
            "table.delete-every-row",
            "These are all the rows of the table. Delete the table instead."
        )
        .to_owned());
    }
    let grid = TableGridMap::build(table);
    let mut out = table.clone();
    for region in grid.regions() {
        let anchored_here = (first..=last).contains(&region.first_row);
        if !anchored_here || region.last_row <= last {
            continue;
        }
        let survivor = last + 1;
        let Some(slot) = grid.cell_at(survivor, region.first_column) else {
            continue;
        };
        let anchor = table.rows[region.first_row].cells[region.anchor_cell].clone();
        let cell = &mut out.rows[survivor].cells[slot.cell];
        let mut properties = anchor.properties.clone();
        properties.vertical_merge = Some(VerticalMerge::Restart);
        cell.properties = properties;
        cell.blocks = anchor.blocks;
    }
    out.rows.drain(first..=last);
    Ok(finish(out))
}

/// Inserts one grid column at `boundary` (0 = before the first column,
/// `columns` = after the last), of width `width`.
///
/// A cell that straddles the boundary — a horizontal merge across it — grows by
/// one column, so the new column sits inside the merged cell: Google Docs' rule.
/// Word instead lets the rows fall out of alignment; a grid that stays a grid is
/// the better reading of "insert a column", and it is what keeps every later
/// column edit well defined. Every other row gets a new empty cell at the
/// boundary, formatted like the cell beside it (the left one, or the right one
/// at the leading edge); a row whose `w:gridBefore`/`w:gridAfter` gap contains
/// the boundary grows that gap instead.
///
/// # Errors
/// When `boundary` is past the last column, or ids run out.
pub fn insert_column(
    table: &Table,
    boundary: usize,
    width: Option<i32>,
    ids: MintId<'_>,
) -> Result<Table, String> {
    let grid = TableGridMap::build(table);
    let columns = grid.column_count();
    if boundary > columns {
        return Err("column is outside the table".to_owned());
    }
    let mut out = table.clone();
    if out.grid.len() < columns {
        out.grid.resize(columns, GridColumn { width_twips: None });
    }
    out.grid.insert(boundary, GridColumn { width_twips: width });
    for (r, row) in out.rows.iter_mut().enumerate() {
        let (before, _) = grid.row_skips(r);
        let slots = grid.row(r);
        let row_end = slots.last().map_or(before, |slot| slot.end_column());
        if let Some(straddling) = slots
            .iter()
            .find(|slot| slot.first_column < boundary && boundary < slot.end_column())
        {
            let cell = &mut row.cells[straddling.cell];
            cell.properties.grid_span = Some(straddling.span as u32 + 1);
            continue;
        }
        if boundary < before {
            row.properties.grid_before = Some(before as u32 + 1);
            continue;
        }
        if boundary > row_end {
            let after = row.properties.grid_after.unwrap_or(0);
            row.properties.grid_after = Some(after + 1);
            continue;
        }
        let index = slots
            .iter()
            .position(|slot| slot.first_column >= boundary)
            .unwrap_or(slots.len());
        let beside = index
            .checked_sub(1)
            .and_then(|i| row.cells.get(i))
            .or_else(|| row.cells.get(index));
        let properties = beside.map_or_else(TableCellProperties::default, |cell| {
            formatting_of(&cell.properties)
        });
        row.cells.insert(index, fresh_cell(ids, properties)?);
    }
    Ok(finish(out))
}

/// Deletes grid columns `first..=last`.
///
/// A cell wholly inside the range is removed; a cell partly inside shrinks by
/// the columns it loses; a `w:gridBefore`/`w:gridAfter` gap shrinks by its
/// overlap. A row left with no cell at all is removed with its columns. Every
/// row of a vertically merged cell covers the same columns, so all of them
/// change the same way and no merge is torn.
///
/// # Errors
/// When the range is empty, outside the table, or every column (delete the
/// table instead), or would leave no row.
pub fn delete_columns(table: &Table, first: usize, last: usize) -> Result<Table, String> {
    let grid = TableGridMap::build(table);
    let columns = grid.column_count();
    if first > last || last >= columns {
        return Err("column is outside the table".to_owned());
    }
    if first == 0 && last + 1 == columns {
        return Err(refused!(
            "table.delete-every-column",
            "These are all the columns of the table. Delete the table instead."
        )
        .to_owned());
    }
    let overlap = |start: usize, end: usize| -> usize {
        let lo = start.max(first);
        let hi = end.min(last + 1);
        hi.saturating_sub(lo)
    };
    let mut out = table.clone();
    if out.grid.len() > last {
        out.grid.drain(first..=last);
    } else if out.grid.len() > first {
        out.grid.truncate(first);
    }
    for (r, row) in out.rows.iter_mut().enumerate() {
        let (before, after) = grid.row_skips(r);
        let slots = grid.row(r);
        let row_end = slots.last().map_or(before, |slot| slot.end_column());
        let mut kept = Vec::with_capacity(row.cells.len());
        for (cell, slot) in std::mem::take(&mut row.cells).into_iter().zip(slots) {
            let lost = overlap(slot.first_column, slot.end_column());
            if lost >= slot.span {
                continue;
            }
            let mut cell = cell;
            let span = slot.span - lost;
            cell.properties.grid_span = (span > 1).then_some(span as u32);
            kept.push(cell);
        }
        row.cells = kept;
        let before = before - overlap(0, before);
        let after = after - overlap(row_end, row_end + after);
        row.properties.grid_before = (before > 0).then_some(before as u32);
        row.properties.grid_after = (after > 0).then_some(after as u32);
    }
    out.rows.retain(|row| !row.cells.is_empty());
    if out.rows.is_empty() {
        return Err(refused!(
            "table.delete-every-column",
            "These are all the columns of the table. Delete the table instead."
        )
        .to_owned());
    }
    Ok(finish(out))
}

/// A rectangle of grid cells, inclusive on both axes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GridRect {
    /// First row.
    pub first_row: usize,
    /// Last row (inclusive).
    pub last_row: usize,
    /// First grid column.
    pub first_column: usize,
    /// Last grid column (inclusive).
    pub last_column: usize,
}

/// Merges the rectangle `rect` into one cell, returning the table and the
/// merged cell's (row, cell index).
///
/// `rect` must already contain every merged cell it touches —
/// `casual_doc_selection::TableCellSelection::normalised` produces exactly that
/// — so the merge may run on a table that already has merges, which the old
/// path refused outright. Each non-empty cell's content moves into the merged
/// cell in reading order, as Word does; empty cells contribute nothing, so
/// merging a filled cell with blank ones leaves no stray empty paragraphs. The
/// merged cell takes the top-left cell's formatting.
///
/// # Errors
/// When `rect` is a single cell, is outside the table, or a row of it has a
/// gap (a `w:gridBefore`/`w:gridAfter` column) inside the rectangle.
pub fn merge_rect(
    table: &Table,
    rect: GridRect,
    ids: MintId<'_>,
) -> Result<(Table, (usize, usize)), String> {
    let grid = TableGridMap::build(table);
    if rect.first_row > rect.last_row
        || rect.first_column > rect.last_column
        || rect.last_row >= grid.row_count()
        || rect.last_column >= grid.column_count()
    {
        return Err("selection is outside the table".to_owned());
    }
    let not_rectangular = || {
        refused!(
            "table.range-not-rectangular",
            "A cell range has to be a full rectangle of cells, and this selection is not one."
        )
        .to_owned()
    };
    // Per row: the run of cells that tiles the rectangle's columns exactly.
    let mut runs = Vec::with_capacity(rect.last_row - rect.first_row + 1);
    for r in rect.first_row..=rect.last_row {
        let slots = grid.row(r);
        let start = slots
            .iter()
            .position(|slot| slot.first_column == rect.first_column)
            .ok_or_else(not_rectangular)?;
        let end = slots
            .iter()
            .position(|slot| slot.last_column() == rect.last_column)
            .ok_or_else(not_rectangular)?;
        if end < start {
            return Err(not_rectangular());
        }
        runs.push((start, end));
    }
    let mut inside: Vec<usize> = runs
        .iter()
        .enumerate()
        .flat_map(|(offset, &(start, end))| {
            grid.row(rect.first_row + offset)[start..=end]
                .iter()
                .map(|slot| slot.region)
        })
        .collect();
    inside.sort_unstable();
    inside.dedup();
    // Every merged cell the rectangle touches must be wholly inside it: merging
    // half of a merged cell would tear the other half off silently.
    if inside.iter().any(|&region| {
        let region = grid.region(region);
        region.first_row < rect.first_row || region.last_row > rect.last_row
    }) {
        return Err(refused!(
            "table.range-clips-merged-cell",
            "This range cuts through a merged cell. Include the whole merged cell, or unmerge \
             it first."
        )
        .to_owned());
    }
    if inside.len() < 2 {
        return Err(refused!(
            "table.merge-too-few-cells",
            "Select at least two cells to merge."
        )
        .to_owned());
    }
    // Content in reading order: each region's anchor, top to bottom, left to
    // right (region indices ARE reading order).
    let mut blocks = Vec::new();
    for &region in &inside {
        let region = grid.region(region);
        let anchor = &table.rows[region.first_row].cells[region.anchor_cell];
        if has_content(anchor) {
            blocks.extend(anchor.blocks.iter().cloned());
        }
    }
    let top_left = &table.rows[rect.first_row].cells[runs[0].0];
    if blocks.is_empty() {
        blocks.clone_from(&top_left.blocks);
    }
    let mut properties = formatting_of(&top_left.properties);
    let span = rect.last_column - rect.first_column + 1;
    properties.grid_span = (span > 1).then_some(span as u32);
    let tall = rect.last_row > rect.first_row;
    let mut out = table.clone();
    for (offset, &(start, end)) in runs.iter().enumerate() {
        let r = rect.first_row + offset;
        let row = &mut out.rows[r];
        let mut kept = row.cells[start].clone();
        kept.properties = properties.clone();
        kept.properties.vertical_merge = match (tall, offset) {
            (false, _) => None,
            (true, 0) => Some(VerticalMerge::Restart),
            (true, _) => Some(VerticalMerge::Continue),
        };
        kept.blocks = if offset == 0 {
            blocks.clone()
        } else {
            vec![empty_paragraph(ids)?]
        };
        row.cells.splice(start..=end, std::iter::once(kept));
    }
    let caret = (rect.first_row, runs[0].0);
    Ok((finish(out), caret))
}

/// Unmerges the merged cell that physical cell (`row`, `cell`) belongs to —
/// from ANY of its rows, not only the top-left: a merged cell is one cell to the
/// reader, wherever the caret sits in it. It becomes one cell per grid cell it
/// covered; the content stays in the top-left, the others are empty and carry
/// its formatting.
///
/// # Errors
/// When the cell is not merged, or is outside the table.
pub fn unmerge(table: &Table, row: usize, cell: usize, ids: MintId<'_>) -> Result<Table, String> {
    let grid = TableGridMap::build(table);
    let region = *grid
        .region_of(row, cell)
        .ok_or_else(|| "cell is outside the table".to_owned())?;
    if !region.is_merged() {
        return Err(refused!(
            "table.unmerge-not-merged",
            "This cell is not merged, so there is nothing to unmerge."
        )
        .to_owned());
    }
    let formatting =
        formatting_of(&table.rows[region.first_row].cells[region.anchor_cell].properties);
    let mut out = table.clone();
    for r in region.first_row..=region.last_row {
        let slot = *grid
            .cell_at(r, region.first_column)
            .ok_or_else(|| "merged row is outside the table".to_owned())?;
        let mut first = out.rows[r].cells[slot.cell].clone();
        first.properties = formatting.clone();
        if r != region.first_row {
            first.blocks = vec![empty_paragraph(ids)?];
        }
        let mut cells = vec![first];
        for _ in 1..region.column_count() {
            cells.push(fresh_cell(ids, formatting.clone())?);
        }
        out.rows[r].cells.splice(slot.cell..=slot.cell, cells);
    }
    Ok(finish(out))
}

/// The row bands of `table`: maximal runs of rows that no vertically merged
/// cell crosses out of. A row in no merge is a band of its own. **O(cells).**
#[must_use]
pub fn row_bands(table: &Table) -> Vec<(usize, usize)> {
    let grid = TableGridMap::build(table);
    let mut bottom_of = vec![0usize; grid.row_count()];
    for (r, bottom) in bottom_of.iter_mut().enumerate() {
        *bottom = r;
    }
    for region in grid.regions() {
        let bottom = &mut bottom_of[region.first_row];
        *bottom = (*bottom).max(region.last_row);
    }
    let mut bands = Vec::new();
    let mut r = 0;
    while r < grid.row_count() {
        let start = r;
        let mut end = bottom_of[r];
        while r < end {
            r += 1;
            end = end.max(bottom_of[r]);
        }
        bands.push((start, end));
        r = end + 1;
    }
    bands
}

/// The column bands of `table`: maximal runs of grid columns that no cell
/// crosses out of. **O(cells).**
#[must_use]
pub fn column_bands(table: &Table) -> Vec<(usize, usize)> {
    let grid = TableGridMap::build(table);
    let columns = grid.column_count();
    let mut bands = Vec::new();
    let mut start = 0;
    for boundary in 0..columns {
        if boundary + 1 == columns || !grid.straddles(boundary) {
            bands.push((start, boundary));
            start = boundary + 1;
        }
    }
    bands
}

/// Where the band containing `from` lands when the user asks for the row (or
/// column) at `from` to move to index `to` — the index it would take if it
/// moved alone, which is what a keyboard step or a drop line reports.
///
/// The drop SNAPS to a band boundary instead of refusing: asking a merged band
/// to move one step down moves it below the next band, and a drop inside
/// another merged cell lands just past that merged cell, in the direction of
/// travel. The answer is the band's first index in the table WITHOUT the band,
/// which is what [`move_row_band`] and [`move_column_band`] take.
///
/// # Errors
/// When the band is already at the end it is moving towards, or `from` is in no
/// band.
pub fn band_target(bands: &[(usize, usize)], from: usize, to: usize) -> Result<usize, String> {
    let at = bands
        .iter()
        .position(|(a, b)| (*a..=*b).contains(&from))
        .ok_or_else(|| "outside the table".to_owned())?;
    let (first, last) = bands[at];
    let len = last - first + 1;
    let containing = |index: usize| {
        bands
            .iter()
            .copied()
            .find(|(a, b)| (*a..=*b).contains(&index))
    };
    if to > from {
        let after = if to <= last {
            bands
                .get(at + 1)
                .map(|(_, b)| *b)
                .ok_or_else(|| "already at the end".to_owned())?
        } else {
            containing(to).map_or(to, |(_, b)| b)
        };
        Ok(after + 1 - len)
    } else {
        if to >= first {
            return at
                .checked_sub(1)
                .map(|i| bands[i].0)
                .ok_or_else(|| "already at the start".to_owned());
        }
        Ok(containing(to).map_or(to, |(a, _)| a))
    }
}

fn band_refusal_row() -> String {
    refused!(
        "table.move-would-split-merge",
        "That spot is inside a merged cell, so the rows cannot be moved there. Drop them \
         above or below the merged cell."
    )
    .to_owned()
}

/// Moves the row band containing row `from` so that it starts at row index `to`
/// of the result — the whole merged band moves together, so a drag on one row
/// of a vertically merged cell moves every row of that cell and tears nothing.
/// Returns the table and the moved band's new (first, last) rows.
///
/// # Errors
/// When `to` would put the band inside another band (inside a merged cell), or
/// either index is outside the table.
pub fn move_row_band(
    table: &Table,
    from: usize,
    to: usize,
) -> Result<(Table, (usize, usize)), String> {
    let bands = row_bands(table);
    let (a, b) = *bands
        .iter()
        .find(|(a, b)| (*a..=*b).contains(&from))
        .ok_or_else(|| "row is outside the table".to_owned())?;
    let len = b - a + 1;
    let remaining = table.rows.len() - len;
    if to > remaining {
        return Err("row is outside the table".to_owned());
    }
    // Valid destinations are band starts in the table without the band, or its
    // end: a boundary between two remaining bands.
    let mut starts: Vec<usize> = bands
        .iter()
        .filter(|band| **band != (a, b))
        .map(|(start, _)| if *start > b { start - len } else { *start })
        .collect();
    starts.push(remaining);
    if !starts.contains(&to) {
        return Err(band_refusal_row());
    }
    let mut out = table.clone();
    let moved: Vec<TableRow> = out.rows.drain(a..=b).collect();
    out.rows.splice(to..to, moved);
    Ok((finish(out), (to, to + len - 1)))
}

/// Moves the column band containing grid column `from` so that it starts at
/// grid column `to` of the result. The band is the columns no cell crosses out
/// of, so a horizontally merged cell moves whole with its columns.
///
/// # Errors
/// When `to` is inside another band, either index is outside the table, or a
/// row is short of the band (a `w:gridBefore`/`w:gridAfter` gap), where a row
/// has no cells to move.
pub fn move_column_band(
    table: &Table,
    from: usize,
    to: usize,
) -> Result<(Table, (usize, usize)), String> {
    let grid = TableGridMap::build(table);
    let bands = column_bands(table);
    let (a, b) = *bands
        .iter()
        .find(|(a, b)| (*a..=*b).contains(&from))
        .ok_or_else(|| "column is outside the table".to_owned())?;
    let len = b - a + 1;
    let remaining = grid.column_count() - len;
    if to > remaining {
        return Err("column is outside the table".to_owned());
    }
    let mut starts: Vec<usize> = bands
        .iter()
        .filter(|band| **band != (a, b))
        .map(|(start, _)| if *start > b { start - len } else { *start })
        .collect();
    starts.push(remaining);
    if !starts.contains(&to) {
        return Err(refused!(
            "table.move-would-split-merge",
            "That spot is inside a merged cell, so the columns cannot be moved there. Drop \
             them beside the merged cell."
        )
        .to_owned());
    }
    for r in 0..grid.row_count() {
        if grid.row_skips(r) != (0, 0) {
            return Err(refused!(
                "table.move-ragged-row",
                "A row of this table is shorter than the others, so its columns cannot be \
                 moved. Fill the row out first."
            )
            .to_owned());
        }
    }
    let mut out = table.clone();
    if out.grid.len() == grid.column_count() {
        let moved: Vec<GridColumn> = out.grid.drain(a..=b).collect();
        out.grid.splice(to..to, moved);
    }
    for (r, row) in out.rows.iter_mut().enumerate() {
        let slots = grid.row(r);
        let start = slots
            .iter()
            .position(|slot| slot.first_column == a)
            .ok_or_else(|| "a merged cell crosses the band".to_owned())?;
        let end = slots
            .iter()
            .position(|slot| slot.last_column() == b)
            .ok_or_else(|| "a merged cell crosses the band".to_owned())?;
        let moved: Vec<TableCell> = row.cells.drain(start..=end).collect();
        // The destination, as a cell index of the row without the band.
        let target_column = to;
        let mut column = 0;
        let mut index = 0;
        for cell in &row.cells {
            if column >= target_column {
                break;
            }
            column += cell.properties.grid_span.unwrap_or(1).max(1) as usize;
            index += 1;
        }
        row.cells.splice(index..index, moved);
    }
    Ok((finish(out), (to, to + len - 1)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use casual_doc_model::v1::{
        InlineNode, Run, RunProperties, TableRowProperties, reset_table_grid_cell_visits,
        table_grid_cell_visits,
    };

    struct Ids(u64);
    impl Ids {
        fn mint(&mut self) -> Option<NodeId> {
            self.0 += 1;
            NodeId::new(u128::from(self.0)).ok()
        }
    }

    /// A cell spec: text, span, vMerge (0 none, 1 restart, 2 continue).
    type Spec<'a> = (&'a str, u32, u8);

    fn build(rows: &[&[Spec<'_>]], ids: &mut Ids) -> Table {
        let columns = rows
            .iter()
            .map(|row| row.iter().map(|c| c.1 as usize).sum::<usize>())
            .max()
            .unwrap_or(0);
        Table {
            id: ids.mint().unwrap(),
            grid: vec![
                GridColumn {
                    width_twips: Some(1_000)
                };
                columns
            ],
            grid_change: None,
            properties: Default::default(),
            rows: rows
                .iter()
                .map(|cells| TableRow {
                    id: ids.mint().unwrap(),
                    properties: TableRowProperties::default(),
                    cells: cells
                        .iter()
                        .map(|(text, span, v)| TableCell {
                            id: ids.mint().unwrap(),
                            properties: TableCellProperties {
                                grid_span: (*span > 1).then_some(*span),
                                vertical_merge: match v {
                                    1 => Some(VerticalMerge::Restart),
                                    2 => Some(VerticalMerge::Continue),
                                    _ => None,
                                },
                                ..Default::default()
                            },
                            blocks: vec![BlockNode::Paragraph(Paragraph {
                                id: ids.mint().unwrap(),
                                properties: Default::default(),
                                inlines: if text.is_empty() {
                                    Vec::new()
                                } else {
                                    vec![InlineNode::Run(Run {
                                        id: ids.mint().unwrap(),
                                        properties: RunProperties::default().into(),
                                        text: (*text).to_owned(),
                                    })]
                                },
                            })],
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    fn text(cell: &TableCell) -> String {
        cell.blocks
            .iter()
            .filter_map(|b| match b {
                BlockNode::Paragraph(p) => Some(
                    p.inlines
                        .iter()
                        .filter_map(|i| match i {
                            InlineNode::Run(r) => Some(r.text.clone()),
                            _ => None,
                        })
                        .collect::<String>(),
                ),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("|")
    }

    /// The table as text: per row, `text:span:role` per cell.
    fn dump(table: &Table) -> Vec<String> {
        table
            .rows
            .iter()
            .map(|row| {
                row.cells
                    .iter()
                    .map(|cell| {
                        let role = match cell.properties.vertical_merge {
                            Some(VerticalMerge::Restart) => "R",
                            Some(VerticalMerge::Continue) => "C",
                            None => "",
                        };
                        format!(
                            "{}:{}{}",
                            text(cell),
                            cell.properties.grid_span.unwrap_or(1),
                            role
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect()
    }

    /// A 3-row vertical merge in column 1 of a 4x2 table.
    fn vertical(ids: &mut Ids) -> Table {
        build(
            &[
                &[("a", 1, 0), ("M", 1, 1)],
                &[("b", 1, 0), ("", 1, 2)],
                &[("c", 1, 0), ("", 1, 2)],
                &[("d", 1, 0), ("e", 1, 0)],
            ],
            ids,
        )
    }

    #[test]
    fn a_row_inserted_inside_a_vertical_merge_extends_it_and_one_outside_does_not() {
        let mut ids = Ids(1_000);
        let t = vertical(&mut Ids(0));
        // Below the top row: inside the merge, which grows to four rows.
        let (below_top, at) = insert_row(&t, 0, true, &mut || ids.mint()).unwrap();
        assert_eq!(at, 1);
        assert_eq!(
            dump(&below_top),
            ["a:1 M:1R", ":1 :1C", "b:1 :1C", "c:1 :1C", "d:1 e:1"]
        );
        // Below the bottom row: outside it; the new cell is ordinary.
        let (below_bottom, _) = insert_row(&t, 2, true, &mut || ids.mint()).unwrap();
        assert_eq!(
            dump(&below_bottom),
            ["a:1 M:1R", "b:1 :1C", "c:1 :1C", ":1 :1", "d:1 e:1"]
        );
        // Above the top row: outside it.
        let (above_top, _) = insert_row(&t, 0, false, &mut || ids.mint()).unwrap();
        assert_eq!(dump(&above_top)[0], ":1 :1");
        assert_eq!(dump(&above_top)[1], "a:1 M:1R");
    }

    #[test]
    fn deleting_the_top_row_of_a_merge_moves_its_content_to_the_new_top() {
        let t = vertical(&mut Ids(0));
        let out = delete_rows(&t, 0, 0).unwrap();
        assert_eq!(dump(&out), ["b:1 M:1R", "c:1 :1C", "d:1 e:1"]);
        // Deleting all but one row of it leaves an ordinary cell, not a lone restart.
        let out = delete_rows(&t, 0, 1).unwrap();
        assert_eq!(dump(&out), ["c:1 M:1", "d:1 e:1"]);
        // Deleting a middle row shrinks it.
        let out = delete_rows(&t, 1, 1).unwrap();
        assert_eq!(dump(&out), ["a:1 M:1R", "c:1 :1C", "d:1 e:1"]);
        assert!(delete_rows(&t, 0, 3).is_err(), "every row is refused");
    }

    #[test]
    fn a_column_inserted_through_a_horizontal_merge_widens_it() {
        let mut ids = Ids(1_000);
        let t = build(
            &[
                &[("a", 1, 0), ("M", 2, 0)],
                &[("b", 1, 0), ("c", 1, 0), ("d", 1, 0)],
            ],
            &mut Ids(0),
        );
        // Boundary 2 is inside M (columns 1-2).
        let out = insert_column(&t, 2, Some(1_000), &mut || ids.mint()).unwrap();
        assert_eq!(dump(&out), ["a:1 M:3", "b:1 c:1 :1 d:1"]);
        assert_eq!(out.grid.len(), 4);
        // M's width follows its span.
        assert_eq!(
            out.rows[0].cells[1]
                .properties
                .width
                .and_then(|w| w.dxa_twips()),
            Some(3_000)
        );
        // Boundary 1 is M's leading edge: a new cell before it in row 0.
        let out = insert_column(&t, 1, Some(1_000), &mut || ids.mint()).unwrap();
        assert_eq!(dump(&out), ["a:1 :1 M:2", "b:1 :1 c:1 d:1"]);
    }

    #[test]
    fn a_column_inserted_beside_a_vertical_merge_keeps_it_whole() {
        let mut ids = Ids(1_000);
        let t = vertical(&mut Ids(0));
        let out = insert_column(&t, 1, Some(1_000), &mut || ids.mint()).unwrap();
        assert_eq!(
            dump(&out),
            ["a:1 :1 M:1R", "b:1 :1 :1C", "c:1 :1 :1C", "d:1 :1 e:1"]
        );
    }

    #[test]
    fn deleting_a_column_shrinks_a_merge_across_it_and_removes_cells_inside_it() {
        let t = build(
            &[
                &[("a", 1, 0), ("M", 2, 0)],
                &[("b", 1, 0), ("c", 1, 0), ("d", 1, 0)],
            ],
            &mut Ids(0),
        );
        let out = delete_columns(&t, 1, 1).unwrap();
        assert_eq!(dump(&out), ["a:1 M:1", "b:1 d:1"]);
        let out = delete_columns(&t, 1, 2).unwrap();
        assert_eq!(dump(&out), ["a:1", "b:1"]);
        assert!(delete_columns(&t, 0, 2).is_err());
        // A vertical merge in the deleted column goes with it, untorn.
        let v = vertical(&mut Ids(0));
        assert_eq!(
            dump(&delete_columns(&v, 1, 1).unwrap()),
            ["a:1", "b:1", "c:1", "d:1"]
        );
        assert_eq!(
            dump(&delete_columns(&v, 0, 0).unwrap()),
            ["M:1R", ":1C", ":1C", "e:1"]
        );
    }

    #[test]
    fn a_merge_runs_on_a_table_that_already_has_merges_and_keeps_every_text() {
        let mut ids = Ids(1_000);
        let t = vertical(&mut Ids(0));
        // Merge "a" and "b" (column 0, rows 0-1) beside the existing merge.
        let rect = GridRect {
            first_row: 0,
            last_row: 1,
            first_column: 0,
            last_column: 0,
        };
        let (out, caret) = merge_rect(&t, rect, &mut || ids.mint()).unwrap();
        assert_eq!(caret, (0, 0));
        assert_eq!(dump(&out), ["a|b:1R M:1R", ":1C :1C", "c:1 :1C", "d:1 e:1"]);
        // Merging the whole table: all content, reading order, one cell.
        let rect = GridRect {
            first_row: 0,
            last_row: 3,
            first_column: 0,
            last_column: 1,
        };
        let (out, _) = merge_rect(&t, rect, &mut || ids.mint()).unwrap();
        assert_eq!(dump(&out)[0], "a|M|b|c|d|e:2R");
        // A rectangle that clips a merged cell is refused (the selection expands
        // it first; this function never guesses).
        let rect = GridRect {
            first_row: 0,
            last_row: 0,
            first_column: 0,
            last_column: 1,
        };
        assert!(
            merge_rect(&t, rect, &mut || ids.mint()).is_err(),
            "the row-0 rectangle holds only the top of M"
        );
        let rect = GridRect {
            first_row: 1,
            last_row: 1,
            first_column: 0,
            last_column: 1,
        };
        assert!(
            merge_rect(&t, rect, &mut || ids.mint()).is_err(),
            "the row-1 rectangle cuts through M"
        );
    }

    #[test]
    fn unmerge_from_any_row_of_a_merged_cell_restores_its_grid_cells() {
        let mut ids = Ids(1_000);
        let t = build(
            &[&[("a", 1, 0), ("M", 2, 1)], &[("b", 1, 0), ("", 2, 2)]],
            &mut Ids(0),
        );
        // From the continuation row: the same merged cell.
        let out = unmerge(&t, 1, 1, &mut || ids.mint()).unwrap();
        assert_eq!(dump(&out), ["a:1 M:1 :1", "b:1 :1 :1"]);
        assert!(unmerge(&t, 0, 0, &mut || ids.mint()).is_err());
    }

    #[test]
    fn a_row_band_moves_a_merged_cell_whole() {
        let t = vertical(&mut Ids(0));
        assert_eq!(row_bands(&t), [(0, 2), (3, 3)]);
        // Moving row 1 (inside the band) moves the whole band below row "d".
        let (out, moved) = move_row_band(&t, 1, 1).unwrap();
        assert_eq!(moved, (1, 3));
        assert_eq!(dump(&out), ["d:1 e:1", "a:1 M:1R", "b:1 :1C", "c:1 :1C"]);
        // Dropping "d" inside the band is refused.
        assert!(move_row_band(&t, 3, 1).is_err());
        assert!(move_row_band(&t, 3, 0).is_ok());
    }

    #[test]
    fn a_band_drop_snaps_past_a_merged_cell_instead_of_refusing() {
        // Rows: band (0,2) is a merged cell; (3,3) and (4,4) are plain.
        let bands = [(0, 2), (3, 3), (4, 4)];
        // One step down from inside the merged band: below the next band.
        assert_eq!(band_target(&bands, 1, 2), Ok(1));
        // A plain row dropped inside the merged band lands above it (moving up).
        assert_eq!(band_target(&bands, 3, 1), Ok(0));
        // Moving the last row up one: above row 3.
        assert_eq!(band_target(&bands, 4, 3), Ok(3));
        // Nowhere to go: the merged band one step up from the top.
        assert!(band_target(&bands, 1, 0).is_err());
        // And down from inside the last band.
        assert!(band_target(&[(0, 1), (2, 4)], 3, 4).is_err());
    }

    #[test]
    fn a_column_band_moves_a_horizontal_merge_whole() {
        let t = build(
            &[
                &[("a", 1, 0), ("M", 2, 0)],
                &[("b", 1, 0), ("c", 1, 0), ("d", 1, 0)],
            ],
            &mut Ids(0),
        );
        assert_eq!(column_bands(&t), [(0, 0), (1, 2)]);
        let (out, moved) = move_column_band(&t, 2, 0).unwrap();
        assert_eq!(moved, (0, 1));
        assert_eq!(dump(&out), ["M:2 a:1", "c:1 d:1 b:1"]);
        assert!(
            move_column_band(&t, 0, 1).is_err(),
            "inside the merged band"
        );
    }

    /// Every structural edit is linear in the table: on a table twice as long it
    /// reads the grid twice as much — never quadratically (n vs 2n).
    #[test]
    fn structural_edits_are_linear_in_the_table() {
        let tall = |n: usize| {
            let mut rows: Vec<Vec<Spec<'_>>> = vec![vec![("a", 1, 0), ("M", 1, 1), ("z", 2, 0)]];
            for _ in 1..n {
                rows.push(vec![("b", 1, 0), ("", 1, 2), ("z", 2, 0)]);
            }
            let refs: Vec<&[Spec<'_>]> = rows.iter().map(Vec::as_slice).collect();
            build(&refs, &mut Ids(0))
        };
        let cost = |n: usize| {
            let t = tall(n);
            let mut ids = Ids(1_000_000);
            reset_table_grid_cell_visits();
            let _ = insert_row(&t, n / 2, true, &mut || ids.mint()).unwrap();
            let _ = delete_rows(&t, 0, 0).unwrap();
            let _ = insert_column(&t, 2, None, &mut || ids.mint()).unwrap();
            let _ = delete_columns(&t, 0, 0).unwrap();
            let rect = GridRect {
                first_row: 0,
                last_row: n - 1,
                first_column: 0,
                last_column: 0,
            };
            let _ = merge_rect(&t, rect, &mut || ids.mint()).unwrap();
            let _ = unmerge(&t, n - 1, 1, &mut || ids.mint()).unwrap();
            table_grid_cell_visits()
        };
        let (small, large) = (cost(200), cost(400));
        assert!(small > 0, "the meter sees the work");
        assert!(
            large <= small * 21 / 10,
            "doubling the rows should double the grid work: {small} -> {large}"
        );
    }
}
