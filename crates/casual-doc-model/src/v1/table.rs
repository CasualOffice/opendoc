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

/// One border edge (`w:top`/`w:start`/`w:bottom`/`w:end`/`w:insideH`/`w:insideV`).
/// The `style` token is retained verbatim (the `ST_Border` list is ~180 values
/// incl. art borders — a producer-facing vocabulary kept opaque, like other
/// producer-specific tokens); the editable size/color/space are typed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BorderEdge {
    /// Line style token (`w:val`), non-empty, at most 32 bytes.
    pub style: String,
    /// Line width in eighth-points (`w:sz`; `0..=1024`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_eighth_points: Option<u32>,
    /// Line color (`w:color`), explicit sRGB only; `auto`/theme reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<RgbColor>,
    /// Padding between border and text in points (`w:space`; `0..=31`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub space_points: Option<u32>,
}

/// A border set (`w:tblBorders` / `w:tcBorders`); any subset of edges.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableBorders {
    /// Top edge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top: Option<BorderEdge>,
    /// Leading (start) edge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<BorderEdge>,
    /// Bottom edge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bottom: Option<BorderEdge>,
    /// Trailing (end) edge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<BorderEdge>,
    /// Inside horizontal edges (`w:insideH`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inside_h: Option<BorderEdge>,
    /// Inside vertical edges (`w:insideV`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inside_v: Option<BorderEdge>,
}

impl TableBorders {
    /// Whether no edge is set (serializes to nothing).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Cell content margins (`w:tblCellMar` / `w:tcMar`), dxa twips (`0..=31_680`).
/// Non-`dxa` (`pct`/`nil`) margin types are reported.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CellMargins {
    /// Top margin.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_twips: Option<i32>,
    /// Leading (start) margin.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_twips: Option<i32>,
    /// Bottom margin.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bottom_twips: Option<i32>,
    /// Trailing (end) margin.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_twips: Option<i32>,
}

impl CellMargins {
    /// Whether no margin is set (serializes to nothing).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
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

/// Floating-table overlap behavior (`w:tblOverlap/@w:val`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TableOverlap {
    /// Never overlap other floating tables (`w:val="never"`).
    Never,
    /// May overlap (`w:val="overlap"`).
    Overlap,
}

/// The reference frame a floating table's position is measured from
/// (`w:tblpPr/@w:horzAnchor` `ST_HAnchor` and `@w:vertAnchor` `ST_VAnchor`,
/// which share the same value set).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TableAnchor {
    /// Anchored to the surrounding text (`w:val="text"`).
    Text,
    /// Anchored to the page margin (`w:val="margin"`).
    Margin,
    /// Anchored to the page edge (`w:val="page"`).
    Page,
}

/// Named relative horizontal alignment of a floating table within its anchor
/// (`w:tblpPr/@w:tblpXSpec`, `ST_XAlign`). The named form is mutually exclusive
/// with an absolute `tbl_px_twips` offset and takes precedence when both appear.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TableXAlign {
    /// Flush to the leading edge (`left`).
    Left,
    /// Centered (`center`).
    Center,
    /// Flush to the trailing edge (`right`).
    Right,
    /// Inside edge relative to the page binding (`inside`).
    Inside,
    /// Outside edge relative to the page binding (`outside`).
    Outside,
}

/// Named relative vertical alignment of a floating table within its anchor
/// (`w:tblpPr/@w:tblpYSpec`, `ST_YAlign`). The named form is mutually exclusive
/// with an absolute `tbl_py_twips` offset and takes precedence when both appear.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TableYAlign {
    /// In line with the surrounding text (`inline`).
    Inline,
    /// Flush to the top (`top`).
    Top,
    /// Centered (`center`).
    Center,
    /// Flush to the bottom (`bottom`).
    Bottom,
    /// Inside edge relative to the page binding (`inside`).
    Inside,
    /// Outside edge relative to the page binding (`outside`).
    Outside,
}

/// A floating (text-wrapped) table's position (`w:tblpPr`, `CT_TblPPr`,
/// ECMA-376 §17.4.58). A table carrying this is lifted out of the inline block
/// flow and positioned relative to its `horz_anchor`/`vert_anchor` frame, with
/// surrounding text wrapping around it at the `*_from_text_twips` distances.
///
/// Horizontal placement is either an absolute `tbl_px_twips` offset or a named
/// `x_spec` alignment (and likewise the vertical pair); the named form takes
/// precedence when both appear. Offsets are signed twips
/// (`ST_SignedTwipsMeasure`, `-31_680..=31_680`); from-text distances are
/// unsigned twips (`ST_TwipsMeasure`, `0..=31_680`). Bounds are enforced at
/// import.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableFloatPosition {
    /// Horizontal reference frame (`w:horzAnchor`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub horz_anchor: Option<TableAnchor>,
    /// Vertical reference frame (`w:vertAnchor`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vert_anchor: Option<TableAnchor>,
    /// Absolute horizontal offset from the anchor in signed twips (`w:tblpX`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tbl_px_twips: Option<i32>,
    /// Absolute vertical offset from the anchor in signed twips (`w:tblpY`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tbl_py_twips: Option<i32>,
    /// Named horizontal alignment (`w:tblpXSpec`); wins over `tbl_px_twips`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x_spec: Option<TableXAlign>,
    /// Named vertical alignment (`w:tblpYSpec`); wins over `tbl_py_twips`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y_spec: Option<TableYAlign>,
    /// Leading text-wrap distance in twips (`w:leftFromText`; `0..=31_680`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left_from_text_twips: Option<i32>,
    /// Trailing text-wrap distance in twips (`w:rightFromText`; `0..=31_680`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right_from_text_twips: Option<i32>,
    /// Top text-wrap distance in twips (`w:topFromText`; `0..=31_680`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_from_text_twips: Option<i32>,
    /// Bottom text-wrap distance in twips (`w:bottomFromText`; `0..=31_680`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bottom_from_text_twips: Option<i32>,
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
// Not `Copy`: `TableBorders` owns a `String` (a border style token).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
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
    /// Table borders (`w:tblBorders`).
    #[serde(default, skip_serializing_if = "TableBorders::is_empty")]
    pub borders: TableBorders,
    /// Default cell margins (`w:tblCellMar`).
    #[serde(default, skip_serializing_if = "CellMargins::is_empty")]
    pub cell_margins: CellMargins,
    /// Table indent from the leading margin in twips, `dxa` only (`w:tblInd`);
    /// may be negative.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indent_twips: Option<i32>,
    /// Default cell spacing in twips, `dxa` only (`w:tblCellSpacing`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell_spacing_twips: Option<i32>,
    /// Floating-overlap behavior (`w:tblOverlap`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overlap: Option<TableOverlap>,
    /// Floating-table position (`w:tblpPr`); lifts the table out of inline flow
    /// and positions it relative to an anchor frame with text wrapping around it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub float_position: Option<TableFloatPosition>,
    /// Accessibility caption (`w:tblCaption`); non-empty, at most 255 bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    /// Accessibility description (`w:tblDescription`); non-empty, at most 255
    /// bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
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
    /// Row alignment (`w:jc`); start/center/end (justify reported at import).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alignment: Option<Alignment>,
    /// Per-row default cell spacing in twips, `dxa` only (`w:tblCellSpacing`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell_spacing_twips: Option<i32>,
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
// Not `Copy`: `TableBorders` owns a `String` (a border style token).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
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
    /// Cell borders (`w:tcBorders`).
    #[serde(default, skip_serializing_if = "TableBorders::is_empty")]
    pub borders: TableBorders,
    /// Cell content margins (`w:tcMar`).
    #[serde(default, skip_serializing_if = "CellMargins::is_empty")]
    pub margins: CellMargins,
    /// Shrink text to fit the cell width (`w:tcFitText`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fit_text: bool,
    /// Hide the end-of-cell mark; affects auto-fit height (`w:hideMark`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hide_mark: bool,
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
