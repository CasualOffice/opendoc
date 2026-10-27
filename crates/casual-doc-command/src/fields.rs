// SPDX-License-Identifier: Apache-2.0

//! Field lowerings the editor and agents share (`docs/181` §4.2, step 4).
//!
//! The editor's `insertField` and every `insert.field.<kind>` menu item call
//! [`insert_field`]; an agent's `insert_field` edit calls it with the caret its
//! anchor resolves to.

use casual_doc_edit::table_structure::MintId;
use casual_doc_edit::{CommonField, Operation, Pos};

/// The field kinds a person or an agent can insert, in menu order.
pub const KINDS: [&str; 6] = ["page", "numpages", "date", "time", "filename", "author"];

/// The common field `kind` names (case-insensitive), caching `result` as the
/// display value of the clock and context kinds; `page` and `numpages`
/// recompute at pagination and ignore it. O(1).
///
/// # Errors
/// When `kind` is not one of [`KINDS`].
pub fn common_field(kind: &str, result: String) -> Result<CommonField, String> {
    match kind.to_ascii_lowercase().as_str() {
        "page" => Ok(CommonField::Page),
        "numpages" => Ok(CommonField::NumPages),
        "date" => Ok(CommonField::Date {
            format: None,
            result,
        }),
        "time" => Ok(CommonField::Time {
            format: None,
            result,
        }),
        "filename" => Ok(CommonField::FileName { result }),
        "author" => Ok(CommonField::Author { result }),
        other => Err(format!("unknown field kind: {other}")),
    }
}

/// Inserts the common field `kind` at `at`. One `InsertField`, its field and
/// cached-result run ids minted in that order. O(1).
///
/// # Errors
/// An unknown kind, or ids running out.
pub fn insert_field(
    at: Pos,
    kind: &str,
    result: String,
    ids: MintId<'_>,
) -> Result<Vec<Operation>, String> {
    let common = common_field(kind, result)?;
    let field_id = ids().ok_or_else(|| "id space exhausted".to_owned())?;
    let result_id = ids().ok_or_else(|| "id space exhausted".to_owned())?;
    Ok(vec![Operation::InsertField {
        at,
        field: Box::new(common.build(field_id, result_id)),
    }])
}
