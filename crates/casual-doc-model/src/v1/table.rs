//! Table block structure: the shared grid, rows, cells, cell-merge geometry,
//! and recursive cell content.

use serde::{Deserialize, Serialize};

use super::{Alignment, BlockNode, MarkRevision, PropChange, RgbColor, StyleId, ThemeColor};
use crate::NodeId;

/// Maximum table nesting depth enforced by validation. A root-level table is
/// depth 1; a table inside one of its cells is depth 2. Import caps at the same
/// value so authored and imported documents share one bound.
pub const MAX_TABLE_DEPTH: u32 = 32;

/// The unit a preferred table or cell width is expressed in (`w:tblW`/`w:tcW`
/// `@w:type`, `ST_TblWidth`, ECMA-376 §17.18.90).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WidthType {
    /// Absolute width in twips (`dxa`). The default when a width is present but
    /// its type is omitted.
    #[default]
    Dxa,
    /// Percentage of the reference width, in fiftieths of a percent (`pct`);
    /// `5000` is 100%.
    Pct,
    /// Width chosen automatically by the layout algorithm (`auto`).
    Auto,
    /// No preferred width (`nil`).
    Nil,
}

/// A preferred table or cell width (`w:tblW`/`w:tcW`, `CT_TblWidth`). The `value`
/// is interpreted per `width_type`: twips for `dxa`, fiftieths of a percent for
/// `pct` (`5000` = 100%), and carried as `0` for `auto`/`nil`, which express no
/// magnitude. Carrying the type (rather than assuming `dxa`) lets AutoFit-to-window
/// (`pct`) and content-sized (`auto`) widths round-trip.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableWidth {
    /// Width magnitude (`w:w`): twips for `dxa`, fiftieths of a percent for `pct`,
    /// `0` for `auto`/`nil`.
    pub value: i32,
    /// The unit `value` is measured in (`w:type`).
    pub width_type: WidthType,
}

impl TableWidth {
    /// Constructs an absolute (`dxa`) width in twips.
    #[must_use]
    pub fn dxa(twips: i32) -> Self {
        Self {
            value: twips,
            width_type: WidthType::Dxa,
        }
    }

    /// Constructs a percentage (`pct`) width in fiftieths of a percent.
    #[must_use]
    pub fn pct(fiftieths: i32) -> Self {
        Self {
            value: fiftieths,
            width_type: WidthType::Pct,
        }
    }

    /// The absolute width in twips, when (and only when) this is a `dxa` width.
    /// `pct`/`auto`/`nil` widths carry no twip magnitude and yield `None`, so
    /// layout that only understands absolute widths ignores them as before.
    #[must_use]
    pub fn dxa_twips(&self) -> Option<i32> {
        (self.width_type == WidthType::Dxa).then_some(self.value)
    }

    /// Whether `value` lies in the domain of its `width_type`: twips
    /// (`0..=31_680`) for `dxa`, fiftieths of a percent (`0..=5_000`, i.e.
    /// `0..=100%`) for `pct`, and exactly `0` for the magnitude-less `auto`/`nil`.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        match self.width_type {
            WidthType::Dxa => (0..=31_680).contains(&self.value),
            WidthType::Pct => (0..=5_000).contains(&self.value),
            WidthType::Auto | WidthType::Nil => self.value == 0,
        }
    }
}

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
    /// Theme background fill (`w:themeFill`, with optional `w:themeFillTint`/
    /// `w:themeFillShade`), a palette slot resolved by the consumer. Word emits
    /// this without a duplicate concrete `w:fill`, so it is modeled separately.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_fill: Option<ThemeColor>,
}

impl Shading {
    /// Whether this shading carries no modeled value (serializes to nothing).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fill.is_none() && self.theme_fill.is_none()
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

/// Row/cell conditional-format selector (`w:cnfStyle`, `CT_Cnf`, ECMA-376
/// §17.4.7). Each flag marks the row or cell as belonging to a table-style
/// region, selecting which `w:tblStylePr` override (see `TableStyleOverride`)
/// formats it. The twelve flags are the `ST_Cnf` bit positions, in bitmask
/// order. All-false serializes to nothing (omitted); the value is only carried
/// as `Some` when at least one flag is set.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CnfStyle {
    /// First (header) row (`firstRow`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub first_row: bool,
    /// Last (total) row (`lastRow`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub last_row: bool,
    /// First column (`firstColumn`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub first_column: bool,
    /// Last column (`lastColumn`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub last_column: bool,
    /// Odd vertical band (`oddVBand`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub odd_v_band: bool,
    /// Even vertical band (`evenVBand`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub even_v_band: bool,
    /// Odd horizontal band (`oddHBand`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub odd_h_band: bool,
    /// Even horizontal band (`evenHBand`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub even_h_band: bool,
    /// Top-left (north-west) corner cell (`firstRowFirstColumn`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub first_row_first_column: bool,
    /// Top-right (north-east) corner cell (`firstRowLastColumn`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub first_row_last_column: bool,
    /// Bottom-left (south-west) corner cell (`lastRowFirstColumn`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub last_row_first_column: bool,
    /// Bottom-right (south-east) corner cell (`lastRowLastColumn`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub last_row_last_column: bool,
}

impl CnfStyle {
    /// Whether no flag is set (would serialize to nothing).
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
    /// Associated table style (`w:tblStyle@w:val`): the style whose defaults and
    /// `w:tblStylePr` conditional formatting this table draws from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_ref: Option<StyleId>,
    /// Render the table right-to-left, mirroring column order (`w:bidiVisual`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tbl_bidi_visual: bool,
    /// Table alignment (`w:jc`); start/center/end (justify reported at import).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alignment: Option<Alignment>,
    /// Preferred table width (`w:tblW`), typed by unit: absolute (`dxa`),
    /// percentage (`pct`, AutoFit-to-window), automatic (`auto`), or none (`nil`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<TableWidth>,
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
    /// Table-properties format-change revision (`w:tblPrChange`): the prior table
    /// properties plus author/date/id. Additive, omitted when absent; re-emitted
    /// as the last child of `w:tblPr`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prop_change: Option<PropChange<TableProperties>>,
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
// Not `Copy`: an optional `prop_change` owns a boxed prior snapshot.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableRowProperties {
    /// Conditional-format selector (`w:cnfStyle`): which table-style region
    /// formats this row. `None` when no flag is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conditional_format: Option<CnfStyle>,
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
    /// Tracked insertion/deletion of the whole row (`w:trPr > w:ins` / `w:del`).
    /// Additive, omitted when absent; re-emitted inside `w:trPr` before
    /// `w:trPrChange`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_revision: Option<MarkRevision>,
    /// Row-properties format-change revision (`w:trPrChange`): the prior row
    /// properties plus author/date/id. Additive, omitted when absent; re-emitted
    /// as the last child of `w:trPr`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prop_change: Option<PropChange<TableRowProperties>>,
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

/// A tracked cell merge's vertical-merge annotation (`ST_AnnotationVMerge`):
/// whether the cell is the continuation of, or the start (rest) of, a merged
/// vertical span under the tracked merge.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CellMergeAnnotation {
    /// A continued cell in the merged span (`cont`).
    Cont,
    /// The starting cell of the merged span (`rest`).
    Rest,
}

/// A tracked cell merge (`w:tcPr > w:cellMerge`, `CT_CellMergeTrackChange`): the
/// cell's vertical-merge role changed under tracked changes. Unlike a cell
/// insertion/deletion (a plain [`MarkRevision`]), a merge also records the
/// current and original vertical-merge annotations.
///
/// Author/date/id are retained as the producer wrote them (opaque, bounded),
/// mirroring [`MarkRevision`] and [`super::Revision`] metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CellMergeRevision {
    /// The revision author, if declared (non-empty, at most 255 bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// The revision date as written (ISO-8601 string), if declared (<= 64 bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// The producer's revision id (`w:id`) as written, if declared (<= 64 bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision_id: Option<String>,
    /// The cell's vertical-merge annotation after the merge (`w:vMerge`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vmerge: Option<CellMergeAnnotation>,
    /// The cell's original vertical-merge annotation (`w:vMergeOrig`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vmerge_orig: Option<CellMergeAnnotation>,
}

/// Typed table-cell properties. An empty value serializes to `{}`.
// Not `Copy`: `TableBorders` owns a `String` (a border style token).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableCellProperties {
    /// Conditional-format selector (`w:cnfStyle`): which table-style region
    /// formats this cell. `None` when no flag is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conditional_format: Option<CnfStyle>,
    /// Horizontal merge span in grid columns (`w:gridSpan`; `1..=16384`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grid_span: Option<u32>,
    /// Vertical merge role (`w:vMerge`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vertical_merge: Option<VerticalMerge>,
    /// Preferred cell width (`w:tcW`), typed by unit: absolute (`dxa`),
    /// percentage (`pct`), automatic (`auto`), or none (`nil`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<TableWidth>,
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
    /// Tracked insertion/deletion of the cell (`w:tcPr > w:cellIns` / `w:cellDel`).
    /// Additive, omitted when absent; re-emitted inside `w:tcPr` before
    /// `w:tcPrChange`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell_revision: Option<MarkRevision>,
    /// Tracked cell merge (`w:tcPr > w:cellMerge`). Additive, omitted when absent;
    /// re-emitted inside `w:tcPr` before `w:tcPrChange`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell_merge: Option<CellMergeRevision>,
    /// Cell-properties format-change revision (`w:tcPrChange`): the prior cell
    /// properties plus author/date/id. Additive, omitted when absent; re-emitted
    /// as the last child of `w:tcPr`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prop_change: Option<PropChange<TableCellProperties>>,
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
    /// Grid format-change revision (`w:tblGridChange`): the prior column grid
    /// plus its id (this change carries no author/date). Additive, omitted when
    /// absent; re-emitted as the last child of `w:tblGrid`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid_change: Option<PropChange<Vec<GridColumn>>>,
    /// Table properties (`w:tblPr`); additive, omitted when empty.
    #[serde(default, skip_serializing_if = "TableProperties::is_empty")]
    pub properties: TableProperties,
    /// Ordered rows (non-empty).
    pub rows: Vec<TableRow>,
}
