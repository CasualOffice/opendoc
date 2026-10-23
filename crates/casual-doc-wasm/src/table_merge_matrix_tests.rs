// SPDX-License-Identifier: Apache-2.0

//! The merged-cell operation matrix (`docs/171` §2): every table operation,
//! driven through the facade a host calls, against every merged-cell family in
//! `fixtures/generated/merged-tables.docx`, on every visible cell.
//!
//! Each run is classified, not just pass/fail:
//!
//! - `ok` — the operation applied, the result is a well-formed grid (below), the
//!   content it should keep is still there, undo restores the table exactly,
//!   redo re-applies it exactly, and the merge structure survives a DOCX save and
//!   reopen;
//! - `refused` — the operation declined with a reason and changed nothing;
//! - `CORRUPT(...)` — it applied and left a table no user could have meant: a row
//!   that no longer covers the grid, a `w:vMerge` continuation that joins
//!   nothing, lost text, an undo that does not restore, or a round trip that
//!   changes the merges.
//!
//! Well-formed means, read through the ONE grid map
//! (`casual_doc_model::v1::TableGridMap`): every row covers exactly the grid's
//! columns (cells plus `w:gridBefore`/`w:gridAfter`), and every continuation cell
//! joined the region above it.
//!
//! `the_merged_cell_matrix_has_no_corruption` is the guard; it prints the whole
//! matrix, which is how the before/after inventory in `docs/171` was measured.

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use casual_doc_model::NodeId;
use casual_doc_model::v1::{BlockNode, InlineNode, Table, TableGridMap, VerticalMerge};

use crate::{WasmDocument, first_paragraph_of_cell, open_document, write_document};

const FIXTURE: &[u8] = include_bytes!("../../../fixtures/generated/merged-tables.docx");

/// The fixture's tables, in document order, with the label each is filed under.
const LABELS: [&str; 7] = [
    "H-MERGE",
    "V-MERGE",
    "COMBINED",
    "EDGES",
    "RAGGED",
    "NESTED-OUTER",
    "NESTED-INNER",
];

fn open() -> WasmDocument {
    open_document(FIXTURE).expect("the merged-tables fixture opens")
}

/// Every table of the fixture, as (label, table id), nested ones included.
fn tables(d: &WasmDocument) -> Vec<(&'static str, NodeId)> {
    let ids = table_ids(d);
    assert_eq!(
        ids.len(),
        LABELS.len(),
        "the fixture holds one table per family"
    );
    LABELS.iter().copied().zip(ids).collect()
}

/// Every table id of `d` in document order, nested ones included.
fn table_ids(d: &WasmDocument) -> Vec<NodeId> {
    fn walk(blocks: &[BlockNode], out: &mut Vec<NodeId>) {
        for block in blocks {
            if let BlockNode::Table(table) = block {
                out.push(table.id);
                for row in &table.rows {
                    for cell in &row.cells {
                        walk(&cell.blocks, out);
                    }
                }
            }
        }
    }
    let mut ids = Vec::new();
    walk(d.document.body(), &mut ids);
    ids
}

fn table(d: &WasmDocument, id: NodeId) -> Table {
    casual_doc_edit::find_table(&d.document, id)
        .expect("the table survives")
        .clone()
}

fn cell_text(cell: &casual_doc_model::v1::TableCell) -> String {
    let mut out = String::new();
    for block in &cell.blocks {
        if let BlockNode::Paragraph(paragraph) = block {
            for inline in &paragraph.inlines {
                if let InlineNode::Run(run) = inline {
                    out.push_str(&run.text);
                }
            }
        }
    }
    out
}

/// The merge structure of a table, ignoring ids: per row, per cell, its span,
/// its vertical-merge role and its text. What a DOCX round trip must keep.
fn shape(table: &Table) -> Vec<Vec<(u32, &'static str, String)>> {
    table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| {
                    let role = match cell.properties.vertical_merge {
                        Some(VerticalMerge::Restart) => "restart",
                        Some(VerticalMerge::Continue) => "continue",
                        None => "",
                    };
                    (
                        cell.properties.grid_span.unwrap_or(1),
                        role,
                        cell_text(cell),
                    )
                })
                .collect()
        })
        .collect()
}

/// Why `table` is not a well-formed grid, or `None` when it is.
fn malformed(table: &Table) -> Option<String> {
    let grid = TableGridMap::build(table);
    for r in 0..grid.row_count() {
        let (before, after) = grid.row_skips(r);
        let cells = grid.row(r);
        let covered = before + cells.iter().map(|c| c.span).sum::<usize>() + after;
        if covered != grid.column_count() {
            return Some(format!(
                "row {r} covers {covered} of {} columns",
                grid.column_count()
            ));
        }
        for (c, cell) in table.rows[r].cells.iter().enumerate() {
            if cell.properties.vertical_merge == Some(VerticalMerge::Continue)
                && !grid.cell(r, c).is_some_and(|slot| slot.continuation)
            {
                return Some(format!("row {r} cell {c} is an orphan continuation"));
            }
        }
    }
    None
}

/// The texts operation `name` on visible cell (`row`, `cell`) may legitimately
/// remove: for "delete row", every cell lying wholly inside that row; for
/// "delete column", every cell lying wholly inside the target's columns;
/// nothing for any other operation.
fn deleted_texts(table: &Table, name: &str, row: usize, cell: usize) -> Vec<String> {
    let grid = TableGridMap::build(table);
    let Some(target) = grid.region_of(row, cell).copied() else {
        return Vec::new();
    };
    grid.regions()
        .iter()
        .filter(|region| match name {
            "delete row" => region.first_row == row && region.last_row == row,
            "delete column" => {
                region.first_column >= target.first_column
                    && region.last_column <= target.last_column
            }
            _ => false,
        })
        .map(|region| cell_text(&table.rows[region.first_row].cells[region.anchor_cell]))
        .filter(|text| !text.is_empty())
        .collect()
}

/// A merged cell of `before` (found by its text) that is smaller in `after`.
fn shrunk_region(before: &Table, after: &Table) -> Option<String> {
    let regions = |table: &Table| -> Vec<(String, usize, usize)> {
        let grid = TableGridMap::build(table);
        grid.regions()
            .iter()
            .map(|region| {
                let text = cell_text(&table.rows[region.first_row].cells[region.anchor_cell]);
                (text, region.row_count(), region.column_count())
            })
            .filter(|(text, _, _)| !text.is_empty())
            .collect()
    };
    let after = regions(after);
    for (text, rows, columns) in regions(before) {
        let Some((_, r, c)) = after.iter().find(|(t, _, _)| t.contains(text.as_str())) else {
            continue;
        };
        if *r < rows || *c < columns {
            return Some(format!("{text} shrank from {rows}x{columns} to {r}x{c}"));
        }
    }
    None
}

/// The non-empty texts of `table`, each once.
fn texts(table: &Table) -> Vec<String> {
    let mut out: Vec<String> = table
        .rows
        .iter()
        .flat_map(|row| row.cells.iter().map(cell_text))
        .filter(|text| !text.is_empty())
        .collect();
    out.sort();
    out
}

/// An operation the matrix drives: its name, and what it does to the visible
/// cell at (row, cell index) of the table. `Ok(false)` means "not applicable
/// here" (e.g. unmerge on an unmerged cell), which is reported as `-`.
type Op = fn(&mut WasmDocument, &Table, usize, usize) -> Result<bool, String>;

fn para(table: &Table, row: usize, cell: usize) -> String {
    first_paragraph_of_cell(&table.rows[row].cells[cell])
        .expect("every cell holds a paragraph")
        .to_string()
}

/// The visible cell to the right of (row, cell) in grid terms, as (row, cell).
fn neighbour(table: &Table, row: usize, cell: usize, down: bool) -> Option<(usize, usize)> {
    let grid = TableGridMap::build(table);
    let region = *grid.region_of(row, cell)?;
    let (r, col) = if down {
        (region.last_row + 1, region.first_column)
    } else {
        (region.first_row, region.last_column + 1)
    };
    let slot = grid.cell_at(r, col)?;
    grid.anchor_of(slot.row, slot.cell)
}

/// Runs `f`, turning a panic (a `JsValue` built off-wasm is one) into `Err`.
fn guarded<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let out = catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| Err("refused (threw)".into()));
    std::panic::set_hook(hook);
    out
}

fn ops() -> Vec<(&'static str, Op)> {
    vec![
        ("insert row above", |d, t, r, c| {
            guarded(|| {
                d.insert_row(&para(t, r, c), false)
                    .map(|_| true)
                    .map_err(|_| "refused".into())
            })
        }),
        ("insert row below", |d, t, r, c| {
            guarded(|| {
                d.insert_row(&para(t, r, c), true)
                    .map(|_| true)
                    .map_err(|_| "refused".into())
            })
        }),
        ("delete row", |d, t, r, c| {
            guarded(|| {
                d.delete_row(&para(t, r, c))
                    .map(|_| true)
                    .map_err(|_| "refused".into())
            })
        }),
        ("insert column left", |d, t, r, c| {
            guarded(|| {
                d.insert_column(&para(t, r, c), false)
                    .map(|_| true)
                    .map_err(|_| "refused".into())
            })
        }),
        ("insert column right", |d, t, r, c| {
            guarded(|| {
                d.insert_column(&para(t, r, c), true)
                    .map(|_| true)
                    .map_err(|_| "refused".into())
            })
        }),
        ("delete column", |d, t, r, c| {
            guarded(|| {
                d.delete_column(&para(t, r, c))
                    .map(|_| true)
                    .map_err(|_| "refused".into())
            })
        }),
        ("merge right", |d, t, r, c| {
            let Some((nr, nc)) = neighbour(t, r, c, false) else {
                return Ok(false);
            };
            d.merge_table_cell_range_inner(&para(t, r, c), &para(t, nr, nc))
                .map(|_| true)
                .map_err(|_| "refused".into())
        }),
        ("merge down", |d, t, r, c| {
            let Some((nr, nc)) = neighbour(t, r, c, true) else {
                return Ok(false);
            };
            d.merge_table_cell_range_inner(&para(t, r, c), &para(t, nr, nc))
                .map(|_| true)
                .map_err(|_| "refused".into())
        }),
        ("unmerge", |d, t, r, c| {
            let grid = TableGridMap::build(t);
            if !grid
                .region_of(r, c)
                .is_some_and(|region| region.is_merged())
            {
                return Ok(false);
            }
            guarded(|| {
                d.split_merged_cell(&para(t, r, c), Some(0), Some(0))
                    .map(|_| true)
                    .map_err(|_| "refused".into())
            })
        }),
        ("move row down", |d, t, r, c| {
            if r + 1 >= t.rows.len() {
                return Ok(false);
            }
            d.move_table_row_inner(&para(t, r, c), r as u32, r as u32 + 1)
                .map(|_| true)
                .map_err(|_| "refused".into())
        }),
        ("move column right", |d, t, r, c| {
            let grid = TableGridMap::build(t);
            let slot = *grid.cell(r, c).expect("cell");
            if slot.end_column() >= grid.column_count() {
                return Ok(false);
            }
            d.move_table_column_inner(
                &para(t, r, c),
                slot.first_column as u32,
                slot.first_column as u32 + 1,
            )
            .map(|_| true)
            .map_err(|_| "refused".into())
        }),
        ("set column width", |d, t, r, c| {
            guarded(|| {
                d.set_table_column_width(&para(t, r, c), 2_000)
                    .map(|_| true)
                    .map_err(|_| "refused".into())
            })
        }),
        ("drag column border", |d, t, r, c| {
            let grid = TableGridMap::build(t);
            let slot = *grid.cell(r, c).expect("cell");
            if slot.end_column() >= grid.column_count() {
                return Ok(false);
            }
            d.move_table_column_boundary_inner(
                &para(t, r, c),
                slot.last_column() as u32,
                180,
                "border",
            )
            .map(|_| true)
            .map_err(|_| "refused".into())
        }),
    ]
}

/// What one operation on one cell did.
fn classify(op: Op, name: &str, table_id: NodeId, row: usize, cell: usize) -> String {
    let mut d = open();
    let before = table(&d, table_id);
    let before_texts = texts(&before);
    match op(&mut d, &before, row, cell) {
        Ok(false) => return "-".into(),
        Err(_) => {
            return if shape(&table(&d, table_id)) == shape(&before) {
                "refused".into()
            } else {
                "CORRUPT(refusal changed the table)".into()
            };
        }
        Ok(true) => {}
    }
    let after = table(&d, table_id);
    if let Some(why) = malformed(&after) {
        return format!("CORRUPT({why})");
    }
    if let Err(error) = d.document.validate() {
        return format!("CORRUPT(invalid document: {error:?})");
    }
    // Content: an insert, a move, a resize or a merge keeps every text (a merge
    // concatenates them into one cell); a delete may drop only what it deleted.
    let after_texts = texts(&after);
    let joined = after_texts.concat();
    let deletes = name.starts_with("delete");
    // What a delete may lose: the merged cells lying WHOLLY inside the deleted
    // row (or columns). A merged cell that continues past them keeps its text.
    let deletable = deleted_texts(&before, name, row, cell);
    if before_texts
        .iter()
        .filter(|text| !deletable.contains(text))
        .any(|text| !joined.contains(text.as_str()))
    {
        return "CORRUPT(lost text)".into();
    }
    let _ = after_texts;
    // Merges: no operation but a merge, an unmerge or a delete may make a
    // merged cell SMALLER. A row inserted below the top of a three-row merge
    // must not leave it one row tall.
    if !deletes
        && name != "unmerge"
        && let Some(why) = shrunk_region(&before, &after)
    {
        return format!("CORRUPT({why})");
    }
    if guarded(|| d.undo().map_err(|_| "undo".to_owned())).is_err() || table(&d, table_id) != before
    {
        return "CORRUPT(undo does not restore)".into();
    }
    if guarded(|| d.redo().map_err(|_| "redo".to_owned())).is_err()
        || shape(&table(&d, table_id)) != shape(&after)
    {
        return "CORRUPT(redo does not re-apply)".into();
    }
    let bytes = write_document(&d.document, d.resources.as_map()).expect("saves");
    let reopened = open_document(&bytes).expect("reopens");
    // The same table of the reopened file is the one at the same document-order
    // index (ids are not preserved by a save).
    let index = table_ids(&d).iter().position(|id| *id == table_id);
    let round_tripped = index
        .and_then(|i| table_ids(&reopened).get(i).copied())
        .map(|id| table(&reopened, id));
    if round_tripped.as_ref().map(shape) != Some(shape(&after)) {
        return "CORRUPT(DOCX round trip changes the merges)".into();
    }
    "ok".into()
}

/// Runs the whole matrix and returns, per (table, operation), how many visible
/// cells ended in each outcome.
fn matrix() -> BTreeMap<(usize, &'static str, usize), BTreeMap<String, Vec<String>>> {
    let d = open();
    let mut out = BTreeMap::new();
    for (ti, (_, table_id)) in tables(&d).into_iter().enumerate() {
        let t = table(&d, table_id);
        let grid = TableGridMap::build(&t);
        for (oi, (name, op)) in ops().into_iter().enumerate() {
            let entry: &mut BTreeMap<String, Vec<String>> = out.entry((ti, name, oi)).or_default();
            for region in grid.regions() {
                let (r, c) = (region.first_row, region.anchor_cell);
                let outcome = classify(op, name, table_id, r, c);
                entry
                    .entry(outcome)
                    .or_default()
                    .push(cell_text(&t.rows[r].cells[c]));
            }
        }
    }
    out
}

#[test]
fn the_merged_cell_matrix_has_no_corruption() {
    let matrix = matrix();
    let mut report = String::from("table | operation | outcomes (cells)\n");
    let mut corrupt = Vec::new();
    for ((ti, name, _), outcomes) in &matrix {
        let summary: Vec<String> = outcomes
            .iter()
            .map(|(outcome, cells)| format!("{outcome} x{} [{}]", cells.len(), cells.join(",")))
            .collect();
        report.push_str(&format!(
            "{} | {name} | {}\n",
            LABELS[*ti],
            summary.join("; ")
        ));
        for (outcome, cells) in outcomes {
            if outcome.starts_with("CORRUPT") {
                corrupt.push(format!("{} {name}: {outcome} on {cells:?}", LABELS[*ti]));
            }
        }
    }
    // The matrix is the inventory `docs/171` publishes; set TABLE_MATRIX_OUT to
    // write it, rather than printing on every run.
    if let Ok(path) = std::env::var("TABLE_MATRIX_OUT") {
        std::fs::write(path, &report).expect("writes the matrix");
    }
    assert!(
        corrupt.is_empty(),
        "corrupting operations:\n{}",
        corrupt.join("\n")
    );
}

/// Tab visits every VISIBLE cell of every fixture table exactly once, in
/// reading order, and never a covered `w:vMerge` continuation; Shift+Tab walks
/// the same cells back. Mutation: step by cell index again in
/// `table_cell_anchor` and the V-MERGE walk enters the hidden continuation.
#[test]
fn tab_visits_every_visible_cell_once_in_every_family() {
    let d = open();
    for (label, table_id) in tables(&d) {
        let t = table(&d, table_id);
        // Expected straight from the file, NOT from the grid map under test: every
        // cell that is not a `w:vMerge` continuation, row by row (the fixture has
        // no orphan continuations).
        let anchors: Vec<String> = t
            .rows
            .iter()
            .enumerate()
            .flat_map(|(r, row)| {
                row.cells
                    .iter()
                    .enumerate()
                    .filter(|(_, cell)| {
                        cell.properties.vertical_merge != Some(VerticalMerge::Continue)
                    })
                    .map(move |(c, _)| (r, c))
            })
            .map(|(r, c)| para(&t, r, c))
            .collect();
        let mut at = anchors[0].clone();
        let mut forward = vec![at.clone()];
        while let Ok(caret) = guarded(|| d.move_table_cell(&at, true).map_err(|_| "end".to_owned()))
        {
            at = caret.node;
            forward.push(at.clone());
            assert!(forward.len() <= anchors.len(), "{label}: Tab does not end");
        }
        assert_eq!(forward, anchors, "{label}: Tab order");
        let mut back = vec![at.clone()];
        while let Ok(caret) = guarded(|| {
            d.move_table_cell(&at, false)
                .map_err(|_| "start".to_owned())
        }) {
            at = caret.node;
            back.push(at.clone());
            assert!(
                back.len() <= anchors.len(),
                "{label}: Shift+Tab does not end"
            );
        }
        back.reverse();
        assert_eq!(back, anchors, "{label}: Shift+Tab order");
    }
}

/// Selecting a column selects every visible cell that covers it — a merged
/// cell once — on every family, and selecting a row likewise. It returned
/// nothing for a column of any merged table.
#[test]
fn row_and_column_selection_cover_merged_cells_in_every_family() {
    let d = open();
    for (label, table_id) in tables(&d) {
        let t = table(&d, table_id);
        let grid = TableGridMap::build(&t);
        for region in grid.regions() {
            let node = para(&t, region.first_row, region.anchor_cell);
            for mode in ["row", "column"] {
                let selected = d.table_selection_anchor_nodes(&node, mode);
                assert!(
                    selected.contains(&node),
                    "{label}: selecting the {mode} of {node} includes it: {selected:?}"
                );
                let mut unique = selected.clone();
                unique.sort();
                unique.dedup();
                assert_eq!(unique.len(), selected.len(), "{label}: each cell once");
            }
            let column = d.table_selection_anchor_nodes(&node, "column");
            let expected = grid.regions_in_columns(region.first_column, region.last_column);
            assert_eq!(column.len(), expected.len(), "{label}: the whole column");
        }
    }
}
