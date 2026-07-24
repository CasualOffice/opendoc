//! Table block structure: the shared grid, rows, cells, cell-merge geometry,
//! and recursive cell content.

use serde::{Deserialize, Serialize};

use super::BlockNode;
use crate::NodeId;

/// Maximum table nesting depth enforced by validation. A root-level table is
/// depth 1; a table inside one of its cells is depth 2. Import caps at the same
/// value so authored and imported documents share one bound.
pub const MAX_TABLE_DEPTH: u32 = 32;

/// One column in a table's shared grid (`w:gridCol`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GridColumn {
    /// Column width in twips, if declared (`0..=31_680`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width_twips: Option<i32>,
}

/// A cell's vertical-merge role (`w:vMerge`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerticalMerge {
    /// The top cell of a vertically merged run (`w:val="restart"`).
    Restart,
    /// A continued cell of a vertically merged run (`<w:vMerge/>`).
    Continue,
}

/// Typed table-cell properties. An empty value serializes to `{}`.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableCellProperties {
    /// Horizontal merge span in grid columns (`w:gridSpan`; `1..=16384`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grid_span: Option<u32>,
    /// Vertical merge role (`w:vMerge`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vertical_merge: Option<VerticalMerge>,
    /// Cell width in twips when the source width type is `dxa` (`0..=31_680`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width_twips: Option<i32>,
}

/// A table cell holding recursive block content.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableCell {
    /// Stable cell identity.
    pub id: NodeId,
    /// Cell properties (always present; empty is `{}`).
    pub properties: TableCellProperties,
    /// Nested block content (non-empty; paragraphs and nested tables).
    pub blocks: Vec<BlockNode>,
}

/// A table row.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableRow {
    /// Stable row identity.
    pub id: NodeId,
    /// Ordered cells (non-empty).
    pub cells: Vec<TableCell>,
}

/// A table: a shared column grid and ordered rows.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Table {
    /// Stable table identity.
    pub id: NodeId,
    /// The shared column grid (`w:tblGrid`); may be empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grid: Vec<GridColumn>,
    /// Ordered rows (non-empty).
    pub rows: Vec<TableRow>,
}
