//! The normalized grid map of a table: grid coordinates to the cell that owns
//! them (`docs/171` §3).
//!
//! A table's model is OOXML-shaped: rows of *physical* cells, where a cell covers
//! `w:gridSpan` grid columns, a row may skip `w:gridBefore` columns before its
//! first cell and `w:gridAfter` after its last, and a vertically merged cell is a
//! `w:vMerge="restart"` cell followed, in the rows below, by `w:vMerge` (continue)
//! cells over the same grid columns. What the user sees is different: one
//! rectangle per merged cell, laid on a shared grid of columns.
//!
//! Every table feature has to translate between the two, and until this module
//! each did it with its own arithmetic — `cells[i]` taken as grid column `i`, a
//! span sum in one place, a vertical-merge chain scanned by *cell index* in
//! another. Each worked on a regular table and broke on a merged one, in a
//! different way. This is the **one** translation (the pattern is the HTML
//! table model's "forming a table" algorithm: a grid of slots, each slot
//! pointing at the cell that owns it). Layout, selection, navigation and every
//! structural edit read it, so they agree by construction.
//!
//! The vertical-merge rule is layout's (`casual-doc-layout` `flow.rs`,
//! `resolve_vertical_merge_roles`), because layout decides what is painted: a
//! continuation joins the merge above it only when the cell directly above
//! covers **exactly the same grid columns** and is itself a restart or a joined
//! continuation. Any other continuation is an orphan and stands alone, its
//! content visible, so a malformed file degrades to unmerged cells rather than to
//! a rectangle that is not on screen.
//!
//! No dense `rows × columns` array is ever allocated: a `w:gridSpan` may legally
//! be 16384, and a dense grid of that width would be a memory hazard on a hostile
//! file. Everything is per-cell, so building the map is O(cells) time and memory
//! and is O(1) in DOCUMENT size — it is given the table.

use std::cell::Cell;

use super::{Table, VerticalMerge};

thread_local! {
    static GRID_CELL_VISITS: Cell<u64> = const { Cell::new(0) };
}

/// How many cells [`TableGridMap::build`] has read on this thread since the last
/// [`reset_table_grid_cell_visits`].
///
/// The complexity meter for every table operation that reads the grid: a guard
/// runs an operation on tables of `n` and `2n` rows and asserts the count
/// doubles rather than quadruples, which a millisecond threshold cannot do
/// (`SKILL` section 8).
#[must_use]
pub fn table_grid_cell_visits() -> u64 {
    GRID_CELL_VISITS.with(Cell::get)
}

/// Zeroes the [`table_grid_cell_visits`] counter for this thread.
pub fn reset_table_grid_cell_visits() {
    GRID_CELL_VISITS.with(|visits| visits.set(0));
}

/// One physical cell's footprint in the shared grid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GridCell {
    /// Row index of this cell.
    pub row: usize,
    /// Index of this cell in its row's `cells`.
    pub cell: usize,
    /// First grid column covered.
    pub first_column: usize,
    /// Grid columns covered, at least 1.
    pub span: usize,
    /// Index of the merged region this cell belongs to (see
    /// [`TableGridMap::region`]).
    pub region: usize,
    /// Whether this cell is a `w:vMerge` continuation that joined the region
    /// above: it is covered by the merged cell and paints nothing of its own.
    pub continuation: bool,
}

impl GridCell {
    /// The last grid column covered (inclusive).
    #[must_use]
    pub const fn last_column(&self) -> usize {
        self.first_column + self.span - 1
    }

    /// One past the last grid column covered.
    #[must_use]
    pub const fn end_column(&self) -> usize {
        self.first_column + self.span
    }

    /// Whether this cell covers grid column `column`.
    #[must_use]
    pub const fn covers(&self, column: usize) -> bool {
        self.first_column <= column && column < self.end_column()
    }
}

/// A cell as the user sees it: the rectangle covered by one physical cell
/// together with every `w:vMerge` continuation that joined it. An unmerged cell
/// is a one-row region of its own span.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GridRegion {
    /// First row (the row of the anchor cell).
    pub first_row: usize,
    /// Last row (inclusive).
    pub last_row: usize,
    /// First grid column.
    pub first_column: usize,
    /// Last grid column (inclusive).
    pub last_column: usize,
    /// Index, in `first_row`'s `cells`, of the anchor: the physical cell that
    /// holds the merged cell's content and properties.
    pub anchor_cell: usize,
    /// Whether the anchor declared `w:vMerge="restart"` (so a continuation below
    /// may join it). A restart nothing joined is still a one-row region.
    pub restart: bool,
}

impl GridRegion {
    /// Rows covered, at least 1.
    #[must_use]
    pub const fn row_count(&self) -> usize {
        self.last_row - self.first_row + 1
    }

    /// Grid columns covered, at least 1.
    #[must_use]
    pub const fn column_count(&self) -> usize {
        self.last_column - self.first_column + 1
    }

    /// Whether this region is more than one grid cell — the question "is this
    /// cell merged".
    #[must_use]
    pub const fn is_merged(&self) -> bool {
        self.last_row > self.first_row || self.last_column > self.first_column
    }

    /// Whether the region covers grid cell (`row`, `column`).
    #[must_use]
    pub const fn contains(&self, row: usize, column: usize) -> bool {
        self.first_row <= row
            && row <= self.last_row
            && self.first_column <= column
            && column <= self.last_column
    }
}

/// The normalized grid of one table (see the module documentation).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TableGridMap {
    rows: Vec<Vec<GridCell>>,
    /// Per row: `w:gridBefore` and `w:gridAfter` as read.
    skips: Vec<(usize, usize)>,
    regions: Vec<GridRegion>,
    columns: usize,
}

impl TableGridMap {
    /// Reads `table` into its grid.
    ///
    /// Never fails: an empty row contributes no cells, a span of 0 counts as 1,
    /// and an orphan continuation stands alone (module documentation).
    ///
    /// Complexity: O(cells) time and memory, and no document access.
    #[must_use]
    pub fn build(table: &Table) -> Self {
        let mut rows: Vec<Vec<GridCell>> = Vec::with_capacity(table.rows.len());
        let mut skips = Vec::with_capacity(table.rows.len());
        let mut regions: Vec<GridRegion> = Vec::new();
        let mut columns = table.grid.len();
        // The previous row's cells, sorted by first column, for the
        // vertical-merge join (an exact grid-range match).
        let mut above: Vec<GridCell> = Vec::new();
        for (r, row) in table.rows.iter().enumerate() {
            let before = row.properties.grid_before.unwrap_or(0) as usize;
            let after = row.properties.grid_after.unwrap_or(0) as usize;
            skips.push((before, after));
            let mut start = before;
            let mut here = Vec::with_capacity(row.cells.len());
            for (c, cell) in row.cells.iter().enumerate() {
                let span = cell.properties.grid_span.unwrap_or(1).max(1) as usize;
                let continues = cell.properties.vertical_merge == Some(VerticalMerge::Continue);
                let joined = continues
                    .then(|| exact_match(&above, start, span))
                    .flatten()
                    .filter(|above| {
                        let region = &regions[above.region];
                        region.restart && region.last_row + 1 == r
                    });
                let (region, continuation) = if let Some(above) = joined {
                    regions[above.region].last_row = r;
                    (above.region, true)
                } else {
                    regions.push(GridRegion {
                        first_row: r,
                        last_row: r,
                        first_column: start,
                        last_column: start + span - 1,
                        anchor_cell: c,
                        restart: cell.properties.vertical_merge == Some(VerticalMerge::Restart),
                    });
                    (regions.len() - 1, false)
                };
                here.push(GridCell {
                    row: r,
                    cell: c,
                    first_column: start,
                    span,
                    region,
                    continuation,
                });
                start = start.saturating_add(span);
            }
            GRID_CELL_VISITS.with(|visits| {
                visits.set(visits.get().saturating_add(row.cells.len() as u64));
            });
            columns = columns.max(start.saturating_add(after));
            above.clone_from(&here);
            rows.push(here);
        }
        Self {
            rows,
            skips,
            regions,
            columns,
        }
    }

    /// Number of rows.
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Number of grid columns: the declared grid, or the widest row when a row
    /// reaches past it.
    #[must_use]
    pub const fn column_count(&self) -> usize {
        self.columns
    }

    /// The cells of row `row`, in order. Empty for a row index past the end.
    #[must_use]
    pub fn row(&self, row: usize) -> &[GridCell] {
        self.rows.get(row).map_or(&[], Vec::as_slice)
    }

    /// `w:gridBefore` and `w:gridAfter` of row `row`.
    #[must_use]
    pub fn row_skips(&self, row: usize) -> (usize, usize) {
        self.skips.get(row).copied().unwrap_or((0, 0))
    }

    /// The footprint of physical cell `cell` of row `row`.
    #[must_use]
    pub fn cell(&self, row: usize, cell: usize) -> Option<&GridCell> {
        self.rows.get(row)?.get(cell)
    }

    /// The physical cell of row `row` that covers grid column `column`, or `None`
    /// when the row does not reach it (a `w:gridBefore`/`w:gridAfter` gap).
    ///
    /// Complexity: O(log cells in the row).
    #[must_use]
    pub fn cell_at(&self, row: usize, column: usize) -> Option<&GridCell> {
        let cells = self.rows.get(row)?;
        let index = cells.partition_point(|cell| cell.end_column() <= column);
        cells.get(index).filter(|cell| cell.covers(column))
    }

    /// The merged region with index `region`.
    ///
    /// # Panics
    /// When `region` is not an index this map produced.
    #[must_use]
    pub fn region(&self, region: usize) -> &GridRegion {
        &self.regions[region]
    }

    /// Every merged region, in the order their anchors are met reading the
    /// table row by row, left to right — the order Tab visits cells.
    #[must_use]
    pub fn regions(&self) -> &[GridRegion] {
        &self.regions
    }

    /// The region physical cell (`row`, `cell`) belongs to.
    #[must_use]
    pub fn region_of(&self, row: usize, cell: usize) -> Option<&GridRegion> {
        self.cell(row, cell).map(|slot| &self.regions[slot.region])
    }

    /// The anchor (`row`, `cell index`) of the region physical cell (`row`,
    /// `cell`) belongs to: where the merged cell's content lives. A continuation
    /// resolves to the restart above it; every other cell to itself.
    #[must_use]
    pub fn anchor_of(&self, row: usize, cell: usize) -> Option<(usize, usize)> {
        self.region_of(row, cell)
            .map(|region| (region.first_row, region.anchor_cell))
    }

    /// Whether the table has no merge and no short row: every row covers every
    /// grid column with one-column cells. On such a table a cell index IS a grid
    /// column, which is the only case the per-row column operations encode.
    #[must_use]
    pub fn is_regular(&self) -> bool {
        self.columns > 0
            && self.rows.iter().zip(&self.skips).all(|(cells, skips)| {
                *skips == (0, 0) && cells.len() == self.columns && cells.iter().all(|c| c.span == 1)
            })
            && self.regions.iter().all(|region| !region.is_merged())
    }

    /// The region Tab moves to from physical cell (`row`, `cell`): the next (or
    /// previous) region in reading order, as its anchor. Continuations are never
    /// visited — they are covered by the merged cell above them, which is one
    /// cell to the user, as in Word and Google Docs.
    ///
    /// Complexity: O(regions).
    #[must_use]
    pub fn step(&self, row: usize, cell: usize, forward: bool) -> Option<(usize, usize)> {
        let here = self.cell(row, cell)?.region;
        let order = self.reading_order();
        let at = order.iter().position(|&region| region == here)?;
        let next = if forward {
            order.get(at + 1)
        } else {
            at.checked_sub(1).and_then(|i| order.get(i))
        }?;
        let region = &self.regions[*next];
        Some((region.first_row, region.anchor_cell))
    }

    /// Region indices in reading order: by anchor row, then anchor column.
    /// Regions are created in exactly that order, so this is the identity order;
    /// it is a method so the guarantee is stated where it is relied on.
    fn reading_order(&self) -> Vec<usize> {
        (0..self.regions.len()).collect()
    }

    /// Every region that covers some cell of grid columns `first..=last`, in
    /// reading order — what "select this column" selects.
    ///
    /// Complexity: O(cells).
    #[must_use]
    pub fn regions_in_columns(&self, first: usize, last: usize) -> Vec<usize> {
        let mut seen = vec![false; self.regions.len()];
        let mut out = Vec::new();
        for cell in self.rows.iter().flatten() {
            if cell.first_column <= last && cell.last_column() >= first && !seen[cell.region] {
                seen[cell.region] = true;
                out.push(cell.region);
            }
        }
        out.sort_unstable();
        out
    }

    /// Every region that covers some cell of rows `first..=last`, in reading
    /// order — what "select this row" selects.
    ///
    /// Complexity: O(cells).
    #[must_use]
    pub fn regions_in_rows(&self, first: usize, last: usize) -> Vec<usize> {
        let mut out: Vec<usize> = (0..self.regions.len())
            .filter(|&i| {
                let region = &self.regions[i];
                region.first_row <= last && region.last_row >= first
            })
            .collect();
        out.sort_unstable();
        out
    }

    /// Whether some cell covers both sides of column boundary `boundary` (the
    /// line between grid columns `boundary` and `boundary + 1`).
    ///
    /// Complexity: O(rows × log cells per row).
    #[must_use]
    pub fn straddles(&self, boundary: usize) -> bool {
        (0..self.rows.len()).any(|row| {
            self.cell_at(row, boundary)
                .is_some_and(|cell| cell.last_column() > boundary)
        })
    }
}

/// The cell of a row (sorted by first column) whose range is exactly
/// `[start, start + span)`.
fn exact_match(cells: &[GridCell], start: usize, span: usize) -> Option<GridCell> {
    let index = cells.partition_point(|cell| cell.first_column < start);
    cells
        .get(index)
        .filter(|cell| cell.first_column == start && cell.span == span)
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeId;
    use crate::v1::{
        BlockNode, Paragraph, TableCell, TableCellProperties, TableRow, TableRowProperties,
    };

    /// A cell spec: span, vMerge (0 none, 1 restart, 2 continue).
    fn table(rows: &[(usize, &[(u32, u8)])]) -> Table {
        let mut next = 1u64;
        let mut id = || {
            next += 1;
            NodeId::new(u128::from(next)).expect("id")
        };
        Table {
            id: id(),
            grid: Vec::new(),
            grid_change: None,
            properties: Default::default(),
            rows: rows
                .iter()
                .map(|(before, cells)| TableRow {
                    id: id(),
                    properties: TableRowProperties {
                        grid_before: (*before > 0).then_some(*before as u32),
                        ..Default::default()
                    },
                    cells: cells
                        .iter()
                        .map(|(span, v)| TableCell {
                            id: id(),
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
                                id: id(),
                                properties: Default::default(),
                                inlines: Vec::new(),
                            })],
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    #[test]
    fn a_regular_table_is_regular_and_its_cell_index_is_its_column() {
        let grid = TableGridMap::build(&table(&[(0, &[(1, 0), (1, 0)]), (0, &[(1, 0), (1, 0)])]));
        assert!(grid.is_regular());
        assert_eq!(grid.column_count(), 2);
        assert_eq!(grid.cell_at(1, 1).map(|c| c.cell), Some(1));
        assert_eq!(grid.regions().len(), 4);
    }

    #[test]
    fn a_span_moves_every_later_cell_right_and_cell_at_finds_the_owner() {
        let grid = TableGridMap::build(&table(&[(0, &[(1, 0), (2, 0), (1, 0)])]));
        assert!(!grid.is_regular());
        assert_eq!(grid.column_count(), 4);
        assert_eq!(grid.cell(0, 2).map(|c| c.first_column), Some(3));
        assert_eq!(grid.cell_at(0, 2).map(|c| c.cell), Some(1));
        assert!(grid.straddles(1));
        assert!(!grid.straddles(0));
        assert!(!grid.straddles(2));
    }

    #[test]
    fn grid_before_offsets_the_row_and_leaves_a_gap_no_cell_owns() {
        let grid = TableGridMap::build(&table(&[
            (0, &[(1, 0), (1, 0), (1, 0)]),
            (1, &[(1, 0), (1, 0)]),
        ]));
        assert_eq!(grid.cell_at(1, 0), None);
        assert_eq!(grid.cell_at(1, 1).map(|c| c.cell), Some(0));
        assert!(!grid.is_regular());
    }

    #[test]
    fn a_continuation_joins_only_an_exact_range_restart_directly_above() {
        // Column 1 restarts and continues twice; column 3's continuation has a
        // PLAIN cell above it, so it is an orphan standing alone.
        let grid = TableGridMap::build(&table(&[
            (0, &[(1, 0), (1, 1), (1, 0), (1, 0)]),
            (0, &[(1, 0), (1, 2), (1, 0), (1, 2)]),
            (0, &[(1, 0), (1, 2), (1, 0), (1, 0)]),
        ]));
        let merged = grid.region_of(2, 1).copied().expect("region");
        assert_eq!((merged.first_row, merged.last_row), (0, 2));
        assert!(grid.cell(2, 1).expect("cell").continuation);
        assert_eq!(grid.anchor_of(2, 1), Some((0, 1)));
        let orphan = grid.region_of(1, 3).copied().expect("region");
        assert!(!orphan.is_merged());
        assert!(!grid.cell(1, 3).expect("cell").continuation);
    }

    #[test]
    fn a_continuation_with_a_different_span_does_not_join() {
        let grid = TableGridMap::build(&table(&[
            (0, &[(2, 1), (1, 0)]),
            (0, &[(1, 2), (1, 0), (1, 0)]),
        ]));
        assert_eq!(grid.region_of(0, 0).expect("region").row_count(), 1);
        assert!(!grid.cell(1, 0).expect("cell").continuation);
    }

    #[test]
    fn tab_order_visits_each_merged_cell_once_at_its_anchor() {
        // r0: [a][b restart][c]; r1: [d][b cont][e]
        let grid = TableGridMap::build(&table(&[
            (0, &[(1, 0), (1, 1), (1, 0)]),
            (0, &[(1, 0), (1, 2), (1, 0)]),
        ]));
        let mut at = (0, 0);
        let mut visited = vec![at];
        while let Some(next) = grid.step(at.0, at.1, true) {
            visited.push(next);
            at = next;
        }
        assert_eq!(visited, vec![(0, 0), (0, 1), (0, 2), (1, 0), (1, 2)]);
        // Backwards from a continuation resolves through its region.
        assert_eq!(grid.step(1, 1, false), Some((0, 0)));
        assert_eq!(grid.step(1, 2, false), Some((1, 0)));
        assert_eq!(grid.step(1, 0, false), Some((0, 2)));
    }

    #[test]
    fn column_and_row_regions_include_a_merged_cell_once() {
        let grid = TableGridMap::build(&table(&[
            (0, &[(2, 1), (1, 0)]),
            (0, &[(2, 2), (1, 0)]),
            (0, &[(1, 0), (1, 0), (1, 0)]),
        ]));
        // Column 1 is covered by the 2x2 region (once) and r2c1.
        assert_eq!(grid.regions_in_columns(1, 1).len(), 2);
        assert_eq!(grid.regions_in_rows(1, 1).len(), 2);
    }

    /// Building the map is linear in the table: a table twice as long has twice
    /// the cells and twice the regions — no quadratic join (n vs 2n).
    #[test]
    fn building_is_linear_in_the_cells() {
        let make = |n: usize| {
            let mut rows: Vec<(usize, &[(u32, u8)])> = Vec::new();
            rows.push((0, &[(1, 1), (2, 0)]));
            for _ in 1..n {
                rows.push((0, &[(1, 2), (2, 0)]));
            }
            table(&rows)
        };
        let small = TableGridMap::build(&make(50));
        let large = TableGridMap::build(&make(100));
        assert_eq!(small.regions().len(), 51);
        assert_eq!(large.regions().len(), 101);
        assert_eq!(large.region_of(99, 0).map(|r| r.row_count()), Some(100));
    }
}
