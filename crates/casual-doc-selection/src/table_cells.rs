//! Rectangular table-cell selection: the grid geometry and the merged-cell
//! expansion rule (`docs/141` §4.3, row TBL-16).
//!
//! A [`TableCellSelection`] is an anchor cell plus a focus cell — the cell a drag
//! started in and the cell it is currently over. The rectangle they describe is
//! **derived on demand** by [`TableCellSelection::normalised`] and never stored,
//! so a grid edit cannot leave it stale.
//!
//! Two rules live here rather than in a facade, because the merge operation and
//! the selection have to agree by construction (`docs/141` §4.3.1):
//!
//! 1. **Cell index is not grid column.** A row's `cells[i]` starts at grid column
//!    `i` only until some cell carries `w:gridSpan`. [`cell_grid_start`] is the
//!    conversion, and it is the same function the merge path uses.
//! 2. **A rectangle is expanded until it contains whole cells.** A rectangle whose
//!    edge clips a horizontally or vertically merged cell grows to contain that
//!    cell, which can pull in further merged cells, so the growth iterates to a
//!    fixed point. This is what Word and Google Docs both do, and it is what makes
//!    "merge the selection" well defined for every rectangle a user can drag.
//!
//! Everything in this module is O(1) in DOCUMENT size: it is given the table.
//! Per-call cost is stated on each item, because extending a selection happens on
//! pointer-move (`docs/107` §4).

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use casual_doc_model::NodeId;
use casual_doc_model::v1::{Table, TableRow, VerticalMerge};

/// The grid column a row's `cells[cell_index]` starts at, summing the
/// `w:gridSpan` of every cell before it. A cell index past the end of the row
/// clamps to the row's total width, so this never panics on a ragged grid.
///
/// This moved down from the wasm facade (`docs/141` §4.3.1): the merge operation
/// and [`TableCellSelection::normalised`] must convert cell indices to grid
/// columns the same way, and two implementations of one rule diverge.
///
/// Complexity: O(cell_index) — a sum over the cells to the left, no document
/// access at all.
#[must_use]
pub fn cell_grid_start(row: &TableRow, cell_index: usize) -> usize {
    row.cells[..cell_index.min(row.cells.len())]
        .iter()
        .map(|cell| span_of(cell.properties.grid_span))
        .sum()
}

/// The grid width of one cell: `w:gridSpan` when declared, at least 1.
fn span_of(grid_span: Option<u32>) -> usize {
    grid_span.unwrap_or(1).max(1) as usize
}

/// One cell's footprint in the shared grid.
#[derive(Clone, Copy, Debug)]
struct Slot {
    row: usize,
    grid_start: usize,
    span: usize,
    cell: NodeId,
    region: usize,
}

impl Slot {
    /// The last grid column this cell covers (inclusive).
    const fn grid_end(&self) -> usize {
        self.grid_start + self.span - 1
    }
}

/// The inclusive rectangle a set of cells covers, in grid columns.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Rect {
    first_row: usize,
    last_row: usize,
    first_column: usize,
    last_column: usize,
}

impl Rect {
    fn union(self, other: Self) -> Self {
        Self {
            first_row: self.first_row.min(other.first_row),
            last_row: self.last_row.max(other.last_row),
            first_column: self.first_column.min(other.first_column),
            last_column: self.last_column.max(other.last_column),
        }
    }

    const fn contains(self, other: Self) -> bool {
        self.first_row <= other.first_row
            && self.last_row >= other.last_row
            && self.first_column <= other.first_column
            && self.last_column >= other.last_column
    }
}

/// A merged cell as the user sees it: the rectangle covered by one cell together
/// with every `w:vMerge` continuation below it. An unmerged cell is a 1x1 region.
type Region = Rect;

/// The shared grid of one table: every cell's footprint, and the merged region it
/// belongs to.
///
/// Built once per query. Complexity: O(cells) time and memory — one pass per row
/// plus a per-row map keyed by grid column, so no dense `rows x columns` array is
/// ever allocated (a `w:gridSpan` may legally be 16384, and a dense grid of that
/// width would be a memory hazard on a hostile file).
struct TableGrid {
    rows: Vec<Vec<Slot>>,
    regions: Vec<Region>,
    columns: usize,
}

impl TableGrid {
    /// Reads `table` into slots and merged regions.
    ///
    /// The vertical-merge rule: a cell whose `w:vMerge` is `Continue` joins the
    /// region of the cell that starts at the **same grid column** in the row
    /// directly above. Any other alignment (an irregular grid, or a `Continue`
    /// with nothing above it) starts a new region, so a malformed merge degrades
    /// to unmerged cells rather than to a wrong rectangle.
    fn build(table: &Table) -> Result<Self, CellSelectionError> {
        if table.rows.is_empty() {
            return Err(CellSelectionError::EmptyTable { table: table.id });
        }
        let mut rows: Vec<Vec<Slot>> = Vec::with_capacity(table.rows.len());
        let mut regions: Vec<Region> = Vec::new();
        let mut columns = 0usize;
        // Grid column -> region id, for the row above only.
        let mut above: BTreeMap<usize, usize> = BTreeMap::new();
        for (r, row) in table.rows.iter().enumerate() {
            if row.cells.is_empty() {
                return Err(CellSelectionError::EmptyRow {
                    table: table.id,
                    row: r,
                });
            }
            let mut here: BTreeMap<usize, usize> = BTreeMap::new();
            let mut slots: Vec<Slot> = Vec::with_capacity(row.cells.len());
            let mut grid_start = 0usize;
            for cell in &row.cells {
                let span = span_of(cell.properties.grid_span);
                let continues = cell.properties.vertical_merge == Some(VerticalMerge::Continue);
                let footprint = Rect {
                    first_row: r,
                    last_row: r,
                    first_column: grid_start,
                    last_column: grid_start + span - 1,
                };
                let region = match (continues, above.get(&grid_start)) {
                    (true, Some(&id)) => {
                        regions[id] = regions[id].union(footprint);
                        id
                    }
                    _ => {
                        regions.push(footprint);
                        regions.len() - 1
                    }
                };
                here.insert(grid_start, region);
                slots.push(Slot {
                    row: r,
                    grid_start,
                    span,
                    cell: cell.id,
                    region,
                });
                grid_start = grid_start.saturating_add(span);
            }
            columns = columns.max(grid_start);
            rows.push(slots);
            above = here;
        }
        Ok(Self {
            rows,
            regions,
            columns,
        })
    }

    /// The slot of cell `cell`, or `None` when the cell is not in this table.
    ///
    /// Complexity: O(cells).
    fn slot_of(&self, cell: NodeId) -> Option<Slot> {
        self.rows
            .iter()
            .flatten()
            .find(|slot| slot.cell == cell)
            .copied()
    }
}

/// A rectangular block of table cells, as the user dragged it: `anchor` is the
/// cell the gesture started in, `focus` the cell it is currently over. Both are
/// **cell** ids; a host holding a paragraph id resolves it first (the wasm facade
/// uses `casual_doc_edit::locate_cell`, which already finds the innermost table).
///
/// The rectangle is derived by [`normalised`](Self::normalised) on demand, never
/// stored, so an edit to the grid cannot leave a stale rectangle behind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TableCellSelection {
    table: NodeId,
    anchor: NodeId,
    focus: NodeId,
}

impl TableCellSelection {
    /// A selection of the cells between `anchor` and `focus` in table `table`.
    ///
    /// Construction does not validate — the table is not supplied here — so every
    /// refusal comes out of [`normalised`](Self::normalised) with a reason.
    #[must_use]
    pub const fn new(table: NodeId, anchor: NodeId, focus: NodeId) -> Self {
        Self {
            table,
            anchor,
            focus,
        }
    }

    /// The table this selection is inside.
    #[must_use]
    pub const fn table(&self) -> NodeId {
        self.table
    }

    /// The cell the gesture started in.
    #[must_use]
    pub const fn anchor(&self) -> NodeId {
        self.anchor
    }

    /// The cell the gesture is currently over.
    #[must_use]
    pub const fn focus(&self) -> NodeId {
        self.focus
    }

    /// The inclusive grid rectangle this selection covers, expanded until it
    /// contains every merged cell it touches.
    ///
    /// The expansion, step by step:
    ///
    /// 1. Start from the **bounding box of the two cells' own footprints** — rows
    ///    `min..max`, grid columns `min(anchor.start, focus.start)` to
    ///    `max(anchor.end, focus.end)`. `gridSpan` is already honoured here, which
    ///    is why [`cell_grid_start`] has to live beside this.
    /// 2. Scan every cell in the rectangle's rows whose grid footprint **overlaps**
    ///    the rectangle's columns. For each, take the merged region it belongs to
    ///    (itself, when it is not merged) and union the region into the rectangle
    ///    if the rectangle does not already contain it.
    /// 3. A union can admit new rows and new columns, whose cells were not scanned,
    ///    and those cells can be merged too — so **repeat from 2 until a pass grows
    ///    nothing**. That is the fixed point, and the post-condition it establishes
    ///    is the one the merge operation needs: *every merged region that
    ///    intersects the rectangle is wholly inside it*.
    ///
    /// Growth is monotone and bounded by the table, so the fixed point is reached:
    /// a pass that changes nothing ends the loop, and any other pass moves at least
    /// one of the four edges, of which at most `rows - 1 + columns - 1` moves exist.
    /// A defensive cap on the pass count converts an impossible non-convergence
    /// into [`CellSelectionError::DidNotConverge`] rather than a hang.
    ///
    /// Complexity: O(cells) to build the grid, then O(cells) per pass. **One pass
    /// on a regular grid and two on any ordinary merge layout**; the worst case is
    /// `O((rows + columns) * cells)`, which needs a staircase of merges arranged so
    /// that every pass admits exactly one new row or column. That is superlinear
    /// and is stated rather than hidden — but it is O(1) in document size, and the
    /// tables this runs on are the ones a user drags across (a 20x5 table is 100
    /// cells and one pass).
    ///
    /// # Errors
    /// [`CellSelectionError::CellNotInTable`] when `anchor` or `focus` is not a
    /// cell of `table` — a range that cannot be acted on refuses with a reason
    /// rather than silently selecting something else; [`CellSelectionError::EmptyTable`]
    /// / [`CellSelectionError::EmptyRow`] for a degenerate grid.
    pub fn normalised(&self, table: &Table) -> Result<CellRange, CellSelectionError> {
        if table.id != self.table {
            return Err(CellSelectionError::WrongTable {
                expected: self.table,
                given: table.id,
            });
        }
        let grid = TableGrid::build(table)?;
        let anchor = grid
            .slot_of(self.anchor)
            .ok_or(CellSelectionError::CellNotInTable {
                table: table.id,
                cell: self.anchor,
            })?;
        let focus = grid
            .slot_of(self.focus)
            .ok_or(CellSelectionError::CellNotInTable {
                table: table.id,
                cell: self.focus,
            })?;

        let requested = Rect {
            first_row: anchor.row.min(focus.row),
            last_row: anchor.row.max(focus.row),
            first_column: anchor.grid_start.min(focus.grid_start),
            last_column: anchor.grid_end().max(focus.grid_end()),
        };
        let mut rect = requested;
        let cap = grid.rows.len() + grid.columns + 2;
        let mut passes = 0usize;
        loop {
            passes += 1;
            if passes > cap {
                return Err(CellSelectionError::DidNotConverge { table: table.id });
            }
            let mut grew = false;
            for r in rect.first_row..=rect.last_row {
                for slot in &grid.rows[r] {
                    if slot.grid_start > rect.last_column || slot.grid_end() < rect.first_column {
                        continue;
                    }
                    let region = grid.regions[slot.region];
                    if !rect.contains(region) {
                        rect = rect.union(region);
                        grew = true;
                    }
                }
            }
            if !grew {
                break;
            }
        }

        // One id per merged region — a merged cell is one cell to the user, and a
        // cell format must be applied to it once. The fixed point guarantees every
        // region here is wholly inside the rectangle, so scanning the rectangle's
        // rows top to bottom meets each region at its own top-left cell first.
        let mut cells = Vec::new();
        let mut seen = vec![false; grid.regions.len()];
        for r in rect.first_row..=rect.last_row {
            for slot in &grid.rows[r] {
                if slot.grid_start < rect.first_column || slot.grid_end() > rect.last_column {
                    continue;
                }
                if !seen[slot.region] {
                    seen[slot.region] = true;
                    cells.push(slot.cell);
                }
            }
        }

        Ok(CellRange {
            first_row: rect.first_row,
            last_row: rect.last_row,
            first_column: rect.first_column,
            last_column: rect.last_column,
            expanded: rect != requested,
            passes,
            cells,
        })
    }
}

/// The normalised result of a [`TableCellSelection`]: an inclusive rectangle in
/// grid columns, whether it had to grow, and the cells inside it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CellRange {
    first_row: usize,
    last_row: usize,
    first_column: usize,
    last_column: usize,
    expanded: bool,
    passes: usize,
    cells: Vec<NodeId>,
}

impl CellRange {
    /// First row index (inclusive).
    #[must_use]
    pub const fn first_row(&self) -> usize {
        self.first_row
    }

    /// Last row index (inclusive).
    #[must_use]
    pub const fn last_row(&self) -> usize {
        self.last_row
    }

    /// First grid column (inclusive) — a GRID column, not a cell index.
    #[must_use]
    pub const fn first_column(&self) -> usize {
        self.first_column
    }

    /// Last grid column (inclusive) — a GRID column, not a cell index.
    #[must_use]
    pub const fn last_column(&self) -> usize {
        self.last_column
    }

    /// Whether the rectangle grew beyond the bounding box of the anchor and focus
    /// cells' own footprints, because it clipped a merged cell. Dragging *into* a
    /// merged cell is therefore not an expansion — the focus cell's footprint
    /// already covers it. The UI says so rather than appearing to select more than the
    /// gesture covered (`docs/141` §4.3.5).
    #[must_use]
    pub const fn expanded(&self) -> bool {
        self.expanded
    }

    /// Rows covered.
    #[must_use]
    pub const fn row_count(&self) -> usize {
        self.last_row - self.first_row + 1
    }

    /// Grid columns covered.
    #[must_use]
    pub const fn column_count(&self) -> usize {
        self.last_column - self.first_column + 1
    }

    /// One cell id per merged region in the rectangle, in row-major order of each
    /// region's top-left cell. A merged cell appears **once**: applying a cell
    /// format twice to one merged cell is two identical edits, and Word and Docs
    /// both count a merged cell as one cell.
    #[must_use]
    pub fn cells(&self) -> &[NodeId] {
        &self.cells
    }

    /// Whether the rectangle is a single cell — merging refuses on this, and the
    /// caller's reason is *"Select two or more cells before merging"*.
    #[must_use]
    pub fn is_single_cell(&self) -> bool {
        self.cells.len() < 2
    }

    /// How many expansion passes the fixed point took: 1 when nothing was clipped.
    /// Exposed for the complexity guard, which asserts the rule converges rather
    /// than timing it.
    #[must_use]
    pub const fn passes(&self) -> usize {
        self.passes
    }
}

/// Why a table-cell range could not be produced. Every variant names what the
/// caller should say: no range is ever silently narrowed or widened.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CellSelectionError {
    /// The supplied table is not the one the selection belongs to.
    WrongTable {
        /// The table the selection names.
        expected: NodeId,
        /// The table that was supplied.
        given: NodeId,
    },
    /// An endpoint is not a cell of this table.
    CellNotInTable {
        /// The table searched.
        table: NodeId,
        /// The endpoint that did not resolve.
        cell: NodeId,
    },
    /// The table has no rows.
    EmptyTable {
        /// The degenerate table.
        table: NodeId,
    },
    /// A row of the table has no cells.
    EmptyRow {
        /// The table holding the degenerate row.
        table: NodeId,
        /// The 0-based row index.
        row: usize,
    },
    /// The merged-cell expansion did not reach a fixed point within the bound its
    /// own monotonicity implies. Unreachable by construction; reported rather than
    /// looped so a grid this code has not anticipated refuses instead of hanging.
    DidNotConverge {
        /// The table whose expansion did not settle.
        table: NodeId,
    },
}

impl fmt::Display for CellSelectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongTable { expected, given } => write!(
                formatter,
                "cell selection belongs to table {expected}, not {given}"
            ),
            Self::CellNotInTable { table, cell } => {
                write!(formatter, "cell {cell} is not in table {table}")
            }
            Self::EmptyTable { table } => write!(formatter, "table {table} has no rows"),
            Self::EmptyRow { table, row } => {
                write!(formatter, "row {row} of table {table} has no cells")
            }
            Self::DidNotConverge { table } => write!(
                formatter,
                "cell range in table {table} did not reach a stable rectangle"
            ),
        }
    }
}

impl Error for CellSelectionError {}

#[cfg(test)]
mod tests {
    use casual_doc_model::v1::{
        BlockNode, Paragraph, ParagraphProperties, Table, TableCell, TableCellProperties, TableRow,
    };
    use casual_doc_model::{IdGenerator, NodeId};

    use super::*;

    /// Builds a table from a per-row list of `(grid_span, vertical_merge)`, so a
    /// merge layout is written out literally in the test rather than produced by
    /// the merge operation. The oracle must not share code with the code under
    /// test: if the expansion and the fixture both derived the grid from
    /// `cell_grid_start`, a bug in it would agree with itself.
    struct Built {
        table: Table,
        /// `ids[row][cell_index]` — the cell ids, for naming endpoints.
        ids: Vec<Vec<NodeId>>,
    }

    fn build(rows: &[Vec<(u32, Option<VerticalMerge>)>]) -> Built {
        build_in(1, rows)
    }

    /// As [`build`], in its own id namespace, so two fixtures in one test do not
    /// mint the same node ids.
    fn build_in(namespace: u64, rows: &[Vec<(u32, Option<VerticalMerge>)>]) -> Built {
        let mut idgen = IdGenerator::new(namespace);
        let table_id = idgen.next_id().unwrap();
        let mut table_rows = Vec::new();
        let mut ids = Vec::new();
        for spec in rows {
            let mut cells = Vec::new();
            let mut row_ids = Vec::new();
            for (span, vmerge) in spec {
                let cell_id = idgen.next_id().unwrap();
                let para_id = idgen.next_id().unwrap();
                row_ids.push(cell_id);
                cells.push(TableCell {
                    id: cell_id,
                    properties: TableCellProperties {
                        grid_span: (*span > 1).then_some(*span),
                        vertical_merge: *vmerge,
                        ..TableCellProperties::default()
                    },
                    blocks: vec![BlockNode::Paragraph(Paragraph {
                        id: para_id,
                        properties: ParagraphProperties::default().into(),
                        inlines: Vec::new(),
                    })],
                });
            }
            ids.push(row_ids);
            table_rows.push(TableRow {
                id: idgen.next_id().unwrap(),
                properties: Default::default(),
                cells,
            });
        }
        Built {
            table: Table {
                id: table_id,
                grid: Vec::new(),
                grid_change: None,
                properties: Default::default(),
                rows: table_rows,
            },
            ids,
        }
    }

    /// A regular `rows x columns` grid, no merges.
    fn plain(rows: usize, columns: usize) -> Built {
        build(&vec![vec![(1, None); columns]; rows])
    }

    fn range(built: &Built, a: (usize, usize), f: (usize, usize)) -> CellRange {
        TableCellSelection::new(built.table.id, built.ids[a.0][a.1], built.ids[f.0][f.1])
            .normalised(&built.table)
            .unwrap()
    }

    #[test]
    fn cell_grid_start_counts_spans_not_indices() {
        let built = build(&[vec![(1, None), (3, None), (1, None)]]);
        let row = &built.table.rows[0];
        assert_eq!(cell_grid_start(row, 0), 0);
        assert_eq!(cell_grid_start(row, 1), 1);
        assert_eq!(cell_grid_start(row, 2), 4);
        // Past the end clamps to the row's total width rather than panicking.
        assert_eq!(cell_grid_start(row, 99), 5);
    }

    #[test]
    fn a_dragged_rectangle_on_a_regular_grid_is_exactly_what_was_dragged() {
        let built = plain(4, 5);
        let r = range(&built, (0, 0), (2, 2));
        assert_eq!(
            (
                r.first_row(),
                r.last_row(),
                r.first_column(),
                r.last_column()
            ),
            (0, 2, 0, 2)
        );
        assert_eq!(r.cells().len(), 9);
        assert!(!r.expanded());
        assert_eq!(r.passes(), 1, "a regular grid must settle in one pass");
    }

    #[test]
    fn the_rectangle_is_the_same_whichever_corner_the_drag_started_in() {
        let built = plain(4, 5);
        let forward = range(&built, (0, 1), (2, 3));
        let backward = range(&built, (2, 3), (0, 1));
        assert_eq!(forward, backward);
    }

    #[test]
    fn a_horizontal_merge_at_the_edge_pulls_the_whole_merged_cell_in() {
        // Row 1 is | a | bcd (span 3) | e |. Dragging row 0 grid column 2 down-left
        // to row 1 column 0 asks for columns 0..=2, which cuts the span-3 cell in
        // half. (Dragging INTO the merged cell instead is not an expansion: the
        // focus cell's own footprint already covers it.)
        let built = build(&[
            vec![(1, None), (1, None), (1, None), (1, None), (1, None)],
            vec![(1, None), (3, None), (1, None)],
        ]);
        let r = range(&built, (0, 2), (1, 0));
        assert!(
            r.expanded(),
            "a clipped merged cell must expand the rectangle"
        );
        assert_eq!(
            (r.first_column(), r.last_column()),
            (0, 3),
            "the span-3 cell starts at grid column 1, so the rectangle must reach 3"
        );
        // 4 cells of row 0 (grid columns 0..=3) plus 2 regions of row 1.
        assert_eq!(r.cells().len(), 6);
    }

    #[test]
    fn a_vertical_merge_at_the_edge_pulls_the_whole_run_in() {
        // Column 1 is vertically merged across rows 0..2.
        let built = build(&[
            vec![(1, None), (1, Some(VerticalMerge::Restart)), (1, None)],
            vec![(1, None), (1, Some(VerticalMerge::Continue)), (1, None)],
            vec![(1, None), (1, Some(VerticalMerge::Continue)), (1, None)],
        ]);
        // Drag only the middle row, columns 0..1: the vMerge run must pull rows 0
        // and 2 in as well.
        let r = range(&built, (1, 0), (1, 1));
        assert!(r.expanded());
        assert_eq!((r.first_row(), r.last_row()), (0, 2));
        assert_eq!((r.first_column(), r.last_column()), (0, 1));
        // Column 0 contributes one cell per row; the merged column is ONE cell.
        assert_eq!(r.cells().len(), 4);
    }

    #[test]
    fn expansion_iterates_to_a_fixed_point_across_a_vertical_and_a_horizontal_merge() {
        // A layout where ONE round of growth is provably not enough. Grid columns
        // across the top; `V` is a vertical merge of column 3, `S` a gridSpan of 3:
        //
        //           0     1     2     3     4
        //   row 0: |   |     |   | V restart |   |
        //   row 1: |   |     |   | V continue|   |
        //   row 2: |   | S ............ |        |
        //
        // Drag row 2 column 0 -> row 1 grid column 1. The dragged rectangle is
        // rows 1..=2 x columns 0..=1.
        //   pass 1 sees row 1 columns 0..=1 (nothing merged there) and row 2's
        //          span, which widens the columns to 0..=3;
        //   pass 2 re-reads row 1 with the WIDER band, meets the vertical merge at
        //          column 3, and grows the rows UP to 0..=2;
        //   pass 3 changes nothing and ends the loop.
        // Growing the columns is what exposed the row growth, which is exactly why
        // the rule iterates. Stopping after one round loses row 0 entirely.
        let built = build(&[
            vec![
                (1, None),
                (1, None),
                (1, None),
                (1, Some(VerticalMerge::Restart)),
                (1, None),
            ],
            vec![
                (1, None),
                (1, None),
                (1, None),
                (1, Some(VerticalMerge::Continue)),
                (1, None),
            ],
            vec![(1, None), (3, None), (1, None)],
        ]);
        let r = range(&built, (2, 0), (1, 1));
        assert!(r.expanded());
        assert_eq!(
            (
                r.first_row(),
                r.last_row(),
                r.first_column(),
                r.last_column()
            ),
            (0, 2, 0, 3),
            "the fixed point must contain the vertical merge the widened band exposed"
        );
        assert_eq!(
            r.passes(),
            3,
            "two rounds of growth plus the confirming pass; one round loses row 0"
        );
        // Rows 0..=2 x grid columns 0..=3, counting the vertical merge once:
        // row 0 gives 3 + the merged cell, row 1 gives 3, row 2 gives 1 + the span.
        assert_eq!(r.cells().len(), 9);
    }

    #[test]
    fn a_merged_cell_is_one_entry_however_many_rows_it_spans() {
        let built = build(&[
            vec![(2, Some(VerticalMerge::Restart))],
            vec![(2, Some(VerticalMerge::Continue))],
        ]);
        let r = range(&built, (0, 0), (1, 0));
        assert_eq!(r.cells().len(), 1);
        assert!(r.is_single_cell(), "one merged cell cannot be merged again");
    }

    #[test]
    fn a_single_cell_range_is_reported_as_single() {
        let built = plain(3, 3);
        let r = range(&built, (1, 1), (1, 1));
        assert!(r.is_single_cell());
        assert_eq!(r.cells().len(), 1);
        assert!(!r.expanded());
    }

    #[test]
    fn an_endpoint_outside_the_table_refuses_with_a_reason() {
        let built = plain(2, 2);
        let stranger = NodeId::new(0xdead_beef).unwrap();
        let error = TableCellSelection::new(built.table.id, built.ids[0][0], stranger)
            .normalised(&built.table)
            .unwrap_err();
        assert_eq!(
            error,
            CellSelectionError::CellNotInTable {
                table: built.table.id,
                cell: stranger
            }
        );
        assert!(error.to_string().contains("is not in table"));
    }

    #[test]
    fn the_wrong_table_refuses_rather_than_guessing() {
        let a = plain(2, 2);
        let b = build_in(2, &vec![vec![(1, None); 2]; 2]);
        let error = TableCellSelection::new(a.table.id, a.ids[0][0], a.ids[1][1])
            .normalised(&b.table)
            .unwrap_err();
        assert!(matches!(error, CellSelectionError::WrongTable { .. }));
    }

    #[test]
    fn a_continuation_with_nothing_above_it_degrades_to_an_unmerged_cell() {
        // A `Continue` in row 0 cannot join anything; it must not be treated as a
        // run reaching outside the table.
        let built = build(&[
            vec![(1, Some(VerticalMerge::Continue)), (1, None)],
            vec![(1, None), (1, None)],
        ]);
        let r = range(&built, (0, 0), (0, 0));
        assert_eq!((r.first_row(), r.last_row()), (0, 0));
        assert_eq!(r.cells().len(), 1);
    }

    #[test]
    fn a_misaligned_continuation_does_not_join_the_cell_above() {
        // Row 0 is | span 2 | 1 |; row 1 is | 1 | Continue at grid column 1 |.
        // The continuation starts at grid column 1, where row 0 has no cell start,
        // so it is its own region and must not drag row 0 in.
        let built = build(&[
            vec![(2, None), (1, None)],
            vec![(1, None), (1, Some(VerticalMerge::Continue)), (1, None)],
        ]);
        let r = range(&built, (1, 1), (1, 1));
        assert_eq!((r.first_row(), r.last_row()), (1, 1));
        assert!(!r.expanded());
    }
}
