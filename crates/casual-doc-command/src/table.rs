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

use casual_doc_edit::Operation;
use casual_doc_edit::table_structure::{self, MintId};
use casual_doc_model::v1::Table;

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
