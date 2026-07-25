//! Table block structure: the shared grid, rows, cells, cell-merge geometry,
//! and recursive cell content.

use serde::{Deserialize, Serialize};

use super::{Alignment, BlockNode, RgbColor};
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

/// Cell background shading (`w:shd`). Only the background fill is modeled; a
/// non-`clear`/`nil` pattern or non-`auto` pattern color is reported at import so
/// no visible shading semantics are silently lost.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Shading {
    /// Background fill (`w:fill`), explicit sRGB; `auto` yields `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<RgbColor>,
}

impl Shading {
    /// Whether this shading carries no modeled value (serializes to nothing).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fill.is_none()
    }
}

/// Table layout algorithm (`w:tblLayout/@w:type`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TableLayout {
    /// Fixed column widths.
    Fixed,
    /// Auto-fit to contents.
    Autofit,
}

/// Conditional-formatting look flags (`w:tblLook`). All-false serializes to
/// nothing (omitted).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableLook {
    /// Apply first-row conditional formatting.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub first_row: bool,
    /// Apply last-row conditional formatting.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub last_row: bool,
    /// Apply first-column conditional formatting.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub first_column: bool,
    /// Apply last-column conditional formatting.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub last_column: bool,
    /// Suppress horizontal row banding.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_h_band: bool,
    /// Suppress vertical column banding.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_v_band: bool,
}

impl TableLook {
    /// Whether no look flag is set (serializes to nothing).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Typed table properties (`w:tblPr`). An empty value serializes to nothing.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableProperties {
    /// Table alignment (`w:jc`); start/center/end (justify reported at import).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alignment: Option<Alignment>,
    /// Preferred table width in twips, `dxa` only (`w:tblW`; `0..=31_680`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width_twips: Option<i32>,
    /// Layout algorithm (`w:tblLayout`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<TableLayout>,
    /// Conditional-format look flags (`w:tblLook`).
    #[serde(default, skip_serializing_if = "TableLook::is_empty")]
    pub look: TableLook,
    /// Table background shading (`w:shd`).
    #[serde(default, skip_serializing_if = "Shading::is_empty")]
    pub shading: Shading,
}

impl TableProperties {
    /// Whether no property is set (serializes to nothing).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// A table row's height rule (`w:trHeight/@w:hRule`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HeightRule {
    /// Height determined by content.
    Auto,
    /// At least the given height.
    AtLeast,
    /// Exactly the given height.
    Exact,
}

/// A table row's height (`w:trHeight`).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RowHeight {
    /// Height in twips (`w:val`; `0..=31_680`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_twips: Option<u32>,
    /// Height rule (`w:hRule`); absent means `auto`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<HeightRule>,
}

impl RowHeight {
    /// Whether no height value is set (serializes to nothing).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Typed table-row properties (`w:trPr`). An empty value serializes to nothing.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableRowProperties {
    /// Row height (`w:trHeight`).
    #[serde(default, skip_serializing_if = "RowHeight::is_empty")]
    pub height: RowHeight,
    /// Keep the whole row on one page (`w:cantSplit`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cant_split: bool,
    /// Repeat as a header row across pages (`w:tblHeader`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub header: bool,
}

impl TableRowProperties {
    /// Whether no property is set (serializes to nothing).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Cell vertical text alignment (`w:vAlign`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CellVerticalAlignment {
    /// Align to the top.
    Top,
    /// Center vertically.
    Center,
    /// Align to the bottom.
    Bottom,
}

/// Cell text flow direction (`w:textDirection`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TextDirection {
    /// Left-to-right, top-to-bottom.
    LrTb,
    /// Top-to-bottom, right-to-left (vertical).
    TbRl,
    /// Bottom-to-top, left-to-right (vertical).
    BtLr,
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
    /// Cell background shading (`w:shd`).
    #[serde(default, skip_serializing_if = "Shading::is_empty")]
    pub shading: Shading,
    /// Vertical text alignment (`w:vAlign`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vertical_alignment: Option<CellVerticalAlignment>,
    /// Suppress text wrapping (`w:noWrap`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_wrap: bool,
    /// Cell text flow direction (`w:textDirection`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_direction: Option<TextDirection>,
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
    /// Row properties (`w:trPr`); additive, omitted when empty.
    #[serde(default, skip_serializing_if = "TableRowProperties::is_empty")]
    pub properties: TableRowProperties,
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
    /// Table properties (`w:tblPr`); additive, omitted when empty.
    #[serde(default, skip_serializing_if = "TableProperties::is_empty")]
    pub properties: TableProperties,
    /// Ordered rows (non-empty).
    pub rows: Vec<TableRow>,
}
