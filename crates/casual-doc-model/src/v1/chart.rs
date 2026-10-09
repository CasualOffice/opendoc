//! The typed DrawingML chart projection (`docs/155` §8, ADR-050).
//!
//! # What this is, and what it deliberately is not
//!
//! A [`Chart`] is a **read projection of a retained package part, not a
//! replacement for it** (`docs/155` §6.1). `word/charts/chart1.xml` and
//! everything reachable from it are byte-preserved by the import side-table and
//! re-emitted verbatim on save; this model is a derived read index over those
//! bytes so a layout consumer can draw the chart without parsing XML, and so a
//! compatibility report can name what inside the part was not understood. The
//! projection is never the authority for the file's contents.
//!
//! Two consequences are load-bearing and enforced rather than intended:
//!
//! - A chart whose projection is [`ChartCoverage::Partial`] **may not be
//!   regenerated** — see [`ChartCoverage::permits_regeneration`]. Rewriting a
//!   part we only partly understood would silently drop whatever we did not
//!   model (a trendline, a data table, a gradient fill).
//! - The embedded workbook named by `c:externalData` is a **typed pointer**
//!   ([`Chart::external_data`]), never bytes and never opened. The cached data
//!   tables in the part are the rendering truth, which is what Word itself and
//!   ONLYOFFICE's document editor both draw from (`docs/155` §3.2, §5.2).
//!   Reading the workbook is how a document editor grows a spreadsheet engine by
//!   accident; `opencalc` is a separate product.
//!
//! # No floating point
//!
//! The v1 model holds **zero** `f64`/`f32` and `Definitions` derives `Eq`, so a
//! cached chart number is carried in its verbatim lexical form as
//! [`ChartValue::Number`] with [`ChartValue::as_f64`] for consumers. That is not
//! a workaround for the `Eq` derive: a byte-faithful rewrite of a chart part must
//! re-emit the spelling the producer wrote, and `4.30` and `4.3` are the same
//! number and different documents. Axis bounds ([`Axis::minimum`],
//! [`Axis::maximum`]) follow the same rule.

use serde::{Deserialize, Serialize};

use super::{Color, EmbeddedPart};
use crate::NodeId;

/// Chart groups a plot area may hold. A combo chart beyond this is not a
/// document chart (`docs/155` §8.4).
pub const MAX_CHART_GROUPS: usize = 16;
/// Series a single chart group may hold.
pub const MAX_CHART_SERIES_PER_GROUP: usize = 256;
/// Cached data points a single [`DataRange`] may hold.
pub const MAX_CHART_DATA_POINTS: usize = 32_768;
/// Axes a plot area may hold: primary and secondary on both axes, with slack.
pub const MAX_CHART_AXES: usize = 8;
/// Byte ceiling on a [`ChartValue::Number`]'s lexical form.
pub const MAX_CHART_NUMBER_BYTES: usize = 64;
/// Byte ceiling on a [`ChartValue::Text`] and on any cached label or format code.
pub const MAX_CHART_TEXT_BYTES: usize = 1_024;
/// Byte ceiling on a verbatim `c:f` formula string — the same ceiling
/// `EmbeddedKind::Other`'s uri already uses.
pub const MAX_CHART_FORMULA_BYTES: usize = 2_048;
/// Ceiling on the verbatim XML one chart carries in [`ChartXml`] fragments,
/// summed over every container. A real Word chart carries a few kilobytes of
/// formatting; this bounds what a hostile part can make the model hold.
pub const MAX_CHART_RETAINED_BYTES: usize = 1_048_576;

/// How much of the source chart part the projection captured.
///
/// This is the gate on regeneration, not a quality score: `Partial` means the
/// part carries at least one construct this model does not represent, so the
/// only correct way to write it back is to copy the retained bytes.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChartCoverage {
    /// Every construct in the source part is represented in this projection.
    #[default]
    Complete,
    /// At least one construct in the source part is not represented. The part's
    /// retained bytes remain the authority (`docs/155` §6.1).
    Partial,
}

impl ChartCoverage {
    /// Whether a writer may regenerate the chart part from this projection.
    ///
    /// `docs/155` §6.1 consequence 1: a chart whose projection is `Partial`
    /// cannot be made dirty, because regenerating it would drop the constructs
    /// the projection does not hold without saying so.
    #[must_use]
    pub const fn permits_regeneration(self) -> bool {
        matches!(self, Self::Complete)
    }
}

/// One cached data point value.
///
/// A number is kept in its **verbatim lexical form** — the text of `c:v` — and
/// parsed to `f64` by the consumer through [`ChartValue::as_f64`], never stored
/// as a float. See this module's header for the two reasons.
/// Adjacently tagged (`content = "value"`), not internally tagged like `Color`:
/// an internally-tagged enum cannot carry a newtype variant whose payload is a
/// string, and serde fails at *serialization* time rather than at compile time —
/// which is how the snapshot round-trip guard below earned its keep.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ChartValue {
    /// A `c:numCache` value, verbatim (`<= MAX_CHART_NUMBER_BYTES` bytes).
    Number(String),
    /// A `c:strCache` value (`<= MAX_CHART_TEXT_BYTES` bytes).
    Text(String),
    /// A cache entry the producer declared with no value.
    Blank,
}

impl ChartValue {
    /// This value as an `f64`, for a consumer that plots it.
    ///
    /// `None` for [`ChartValue::Text`] and [`ChartValue::Blank`], and for a
    /// `Number` whose lexical form does not parse — a cache is producer-supplied
    /// input, so "the string is there but it is not a number" is a state a
    /// renderer must handle rather than a state to panic on.
    ///
    /// Complexity: O(bytes in the value), which is bounded by
    /// [`MAX_CHART_NUMBER_BYTES`].
    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(text) => text.trim().parse::<f64>().ok().filter(|value| {
                // A cache may spell a value `NaN` or `1e400`; neither is a point
                // a renderer can place, and admitting one would propagate it into
                // axis scaling. Refusing here keeps that at the boundary.
                value.is_finite()
            }),
            Self::Text(_) | Self::Blank => None,
        }
    }

    /// Whether this value is [`ChartValue::Blank`].
    #[must_use]
    pub const fn is_blank(&self) -> bool {
        matches!(self, Self::Blank)
    }
}

/// A cached data range — **the cache is the data** (`docs/155` §5.2).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DataRange {
    /// `c:f`, verbatim and opaque.
    ///
    /// Stored so a regenerated part re-emits it unchanged, and for no other
    /// purpose: it is never parsed, never evaluated, and never resolved against a
    /// workbook. Reading it as a formula is how this product would grow a
    /// spreadsheet engine by accident (`docs/155` §5.2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formula: Option<String>,
    /// `c:ptCount` — the producer's declared length, which may exceed
    /// `points.len()` when the cache is sparse.
    pub point_count: u32,
    /// `c:pt`, in `idx` order, from `c:numCache`/`c:strCache`. Each entry is the
    /// point's `idx` and its value; the `idx` is kept rather than implied by
    /// position because a cache may be sparse.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<(u32, ChartValue)>,
    /// `c:formatCode`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number_format: Option<String>,
}

/// Cached chart text: a title, an axis title, or a series name.
///
/// `c:tx` is either a `c:strRef` (a formula plus a one-cell cache) or a `c:rich`
/// DrawingML text body. Both reduce to the same two facts for a consumer — the
/// string to paint, and the formula to re-emit — so one type covers both and a
/// consumer need not care which spelling the producer used.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChartText {
    /// The cached text, with the paragraph and run structure of a `c:rich` body
    /// flattened away (`docs/155` §4.2 admits cached rich text only).
    pub text: String,
    /// The `c:strRef/c:f` this text came from, verbatim, when it came from one.
    /// Never parsed — see [`DataRange::formula`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formula: Option<String>,
}

/// One chart element this projection does not model, carried **verbatim** on
/// the container it was read from, so a regenerated part keeps it.
///
/// The named pattern is round-tripping unknown content (the same reason
/// `Definitions::format_scheme_xml` exists): a typed projection that dropped
/// whatever it did not model could never be the authority over a real file —
/// a default Word chart carries axis and legend formatting (`c:spPr`,
/// `c:txPr`), `c:lang`, `c:crosses`, `c:lblAlgn` and a style choice none of
/// which the chart's DATA depends on, and refusing to rewrite the chart for
/// their sake is what made every Word chart read-only (`docs/155` §17).
///
/// `name` is the element's local name, which is what a writer uses to put it
/// back in its schema position. A fragment whose name is also one the
/// projection models (`spPr` on a series, `majorGridlines` on an axis, `tx`
/// on a title) **shadows** the generated element: the writer emits the
/// verbatim one while the model still says the element is present, and an edit
/// that changes the modelled value drops the fragment so the edit wins.
///
/// Prefixes inside `xml` resolve against [`Chart::namespaces`] or against
/// declarations inside the fragment itself.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChartXml {
    /// The element's local name (`spPr`, `txPr`, `crosses`, `AlternateContent`).
    pub name: String,
    /// The element, verbatim, from its `<` to its closing `>`.
    pub xml: String,
}

/// A chart container whose children have a schema order, for placing a
/// [`ChartXml`] fragment back where it came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChartContainer {
    /// `c:chartSpace` (`CT_ChartSpace`).
    Space,
    /// `c:chart` (`CT_Chart`).
    Chart,
    /// `c:plotArea` (`CT_PlotArea`).
    PlotArea,
    /// A chart group, by family (`CT_BarChart`, `CT_LineChart`, …).
    Group(ChartGroupKind),
    /// A series, by its group's family (`CT_BarSer`, `CT_LineSer`, …).
    Series(ChartGroupKind),
    /// An axis, by kind (`CT_CatAx`, `CT_ValAx`, `CT_DateAx`).
    Axis(AxisKind),
    /// `c:legend` (`CT_Legend`).
    Legend,
    /// `c:title` (`CT_Title`).
    Title,
}

/// The ECMA-376 Part 1 child sequence of `container`, by local name.
///
/// This is the one place the order is written down: the importer carries only
/// an element its container's sequence names (anything else stays an
/// unconsumed construct), and the writer emits each carried element at its
/// position in the same list — Word discards a chart part whose children are
/// out of order, so a fragment written in the wrong place is silent data loss.
///
/// Two pseudo-names hold the plot area's lists: `*groups` and `*axes`.
/// `AlternateContent` sits where `c:style` does, which is where Word writes
/// the `mc:AlternateContent` that wraps it.
///
/// O(1).
#[must_use]
pub const fn chart_child_order(container: ChartContainer) -> &'static [&'static str] {
    match container {
        ChartContainer::Space => &[
            "date1904",
            "lang",
            "roundedCorners",
            "AlternateContent",
            "style",
            "clrMapOvr",
            "pivotSource",
            "protection",
            "chart",
            "spPr",
            "txPr",
            "externalData",
            "printSettings",
            "userShapes",
            "extLst",
        ],
        ChartContainer::Chart => &[
            "title",
            "autoTitleDeleted",
            "pivotFmts",
            "view3D",
            "floor",
            "sideWall",
            "backWall",
            "plotArea",
            "legend",
            "plotVisOnly",
            "dispBlanksAs",
            "showDLblsOverMax",
            "extLst",
        ],
        ChartContainer::PlotArea => &["layout", "*groups", "*axes", "dTable", "spPr", "extLst"],
        ChartContainer::Group(kind) => match kind {
            ChartGroupKind::Bar { .. } => &[
                "barDir",
                "grouping",
                "varyColors",
                "ser",
                "dLbls",
                "gapWidth",
                "overlap",
                "serLines",
                "axId",
                "extLst",
            ],
            ChartGroupKind::Line { .. } => &[
                "grouping",
                "varyColors",
                "ser",
                "dLbls",
                "dropLines",
                "hiLowLines",
                "upDownBars",
                "marker",
                "smooth",
                "axId",
                "extLst",
            ],
            ChartGroupKind::Area { .. } => &[
                "grouping",
                "varyColors",
                "ser",
                "dLbls",
                "dropLines",
                "axId",
                "extLst",
            ],
            ChartGroupKind::Pie { .. } => {
                &["varyColors", "ser", "dLbls", "firstSliceAng", "extLst"]
            }
            ChartGroupKind::Doughnut { .. } => &[
                "varyColors",
                "ser",
                "dLbls",
                "firstSliceAng",
                "holeSize",
                "extLst",
            ],
            ChartGroupKind::Scatter { .. } => &[
                "scatterStyle",
                "varyColors",
                "ser",
                "dLbls",
                "axId",
                "extLst",
            ],
        },
        ChartContainer::Series(kind) => match kind {
            ChartGroupKind::Bar { .. } => &[
                "idx",
                "order",
                "tx",
                "spPr",
                "invertIfNegative",
                "pictureOptions",
                "dPt",
                "dLbls",
                "trendline",
                "errBars",
                "cat",
                "val",
                "shape",
                "extLst",
            ],
            ChartGroupKind::Line { .. } => &[
                "idx",
                "order",
                "tx",
                "spPr",
                "marker",
                "dPt",
                "dLbls",
                "trendline",
                "errBars",
                "cat",
                "val",
                "smooth",
                "extLst",
            ],
            ChartGroupKind::Area { .. } => &[
                "idx",
                "order",
                "tx",
                "spPr",
                "pictureOptions",
                "dPt",
                "dLbls",
                "trendline",
                "errBars",
                "cat",
                "val",
                "extLst",
            ],
            ChartGroupKind::Pie { .. } | ChartGroupKind::Doughnut { .. } => &[
                "idx",
                "order",
                "tx",
                "spPr",
                "explosion",
                "dPt",
                "dLbls",
                "cat",
                "val",
                "extLst",
            ],
            ChartGroupKind::Scatter { .. } => &[
                "idx",
                "order",
                "tx",
                "spPr",
                "marker",
                "dPt",
                "dLbls",
                "trendline",
                "errBars",
                "xVal",
                "yVal",
                "smooth",
                "extLst",
            ],
        },
        ChartContainer::Axis(kind) => match kind {
            AxisKind::Category => &[
                "axId",
                "scaling",
                "delete",
                "axPos",
                "majorGridlines",
                "minorGridlines",
                "title",
                "numFmt",
                "majorTickMark",
                "minorTickMark",
                "tickLblPos",
                "spPr",
                "txPr",
                "crossAx",
                "crosses",
                "crossesAt",
                "auto",
                "lblAlgn",
                "lblOffset",
                "tickLblSkip",
                "tickMarkSkip",
                "noMultiLvlLbl",
                "extLst",
            ],
            AxisKind::Value => &[
                "axId",
                "scaling",
                "delete",
                "axPos",
                "majorGridlines",
                "minorGridlines",
                "title",
                "numFmt",
                "majorTickMark",
                "minorTickMark",
                "tickLblPos",
                "spPr",
                "txPr",
                "crossAx",
                "crosses",
                "crossesAt",
                "crossBetween",
                "majorUnit",
                "minorUnit",
                "dispUnits",
                "extLst",
            ],
            AxisKind::Date => &[
                "axId",
                "scaling",
                "delete",
                "axPos",
                "majorGridlines",
                "minorGridlines",
                "title",
                "numFmt",
                "majorTickMark",
                "minorTickMark",
                "tickLblPos",
                "spPr",
                "txPr",
                "crossAx",
                "crosses",
                "crossesAt",
                "auto",
                "lblOffset",
                "baseTimeUnit",
                "majorUnit",
                "majorTimeUnit",
                "minorUnit",
                "minorTimeUnit",
                "extLst",
            ],
        },
        ChartContainer::Legend => &[
            "legendPos",
            "legendEntry",
            "layout",
            "overlay",
            "spPr",
            "txPr",
            "extLst",
        ],
        ChartContainer::Title => &["tx", "layout", "overlay", "spPr", "txPr", "extLst"],
    }
}

/// `name`'s position in `container`'s sequence, or `None` when the container
/// does not admit it. O(sequence length), at most 25.
#[must_use]
pub fn chart_child_rank(container: ChartContainer, name: &str) -> Option<usize> {
    chart_child_order(container)
        .iter()
        .position(|child| *child == name)
}

/// A chart title (`c:title`).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChartTitle {
    /// The cached title text, when the producer wrote one. `None` for a title
    /// element that carries only formatting (Word writes one when the title is
    /// auto-generated from the single series name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<ChartText>,
    /// `c:overlay` — whether the title is drawn over the plot area.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub overlay: bool,
    /// Children carried verbatim ([`ChartXml`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retained: Vec<ChartXml>,
}

/// Where a legend sits relative to the plot area (`c:legendPos`).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LegendPosition {
    /// `b`.
    #[default]
    Bottom,
    /// `l`.
    Left,
    /// `r`.
    Right,
    /// `t`.
    Top,
    /// `tr`.
    TopRight,
}

/// A chart legend (`c:legend`).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Legend {
    /// `c:legendPos`.
    #[serde(default)]
    pub position: LegendPosition,
    /// `c:overlay` — whether the legend is drawn over the plot area.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub overlay: bool,
    /// Children carried verbatim ([`ChartXml`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retained: Vec<ChartXml>,
}

/// How blank cache entries are plotted (`c:dispBlanksAs`).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DisplayBlanks {
    /// `gap` — leave a hole.
    #[default]
    Gap,
    /// `zero` — plot as zero.
    Zero,
    /// `span` — bridge the gap with the line.
    Span,
}

/// A bar/column chart's bar direction (`c:barDir`).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BarDirection {
    /// `col` — vertical bars.
    #[default]
    Column,
    /// `bar` — horizontal bars.
    Bar,
}

/// A bar/column chart's grouping (`c:grouping`).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BarGrouping {
    /// `clustered`.
    #[default]
    Clustered,
    /// `stacked`.
    Stacked,
    /// `percentStacked`.
    PercentStacked,
    /// `standard` — the schema admits it on a bar group; Word draws it clustered.
    Standard,
}

/// A line/area chart's grouping (`c:grouping`).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Grouping {
    /// `standard`.
    #[default]
    Standard,
    /// `stacked`.
    Stacked,
    /// `percentStacked`.
    PercentStacked,
}

/// A scatter chart's style (`c:scatterStyle`).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ScatterStyle {
    /// `none`.
    None,
    /// `line`.
    Line,
    /// `lineMarker`.
    #[default]
    LineMarker,
    /// `marker`.
    Marker,
    /// `smooth`.
    Smooth,
    /// `smoothMarker`.
    SmoothMarker,
}

/// Which plotting family a chart group draws, with the family's own settings.
///
/// The family is on the **group**, never on the [`Chart`]: a `c:plotArea` holds a
/// list of groups, so a bar+line combo is the absence of a restriction rather
/// than a feature, and a chart-type enum on the chart is the modeling mistake
/// that makes combo and secondary-axis support two separate projects later
/// (`docs/155` §4.2).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ChartGroupKind {
    /// `c:barChart`.
    Bar {
        /// `c:barDir`.
        direction: BarDirection,
        /// `c:grouping`.
        grouping: BarGrouping,
        /// `c:gapWidth`, a percentage of bar width (`0..=500`).
        gap_width: u16,
        /// `c:overlap`, a percentage (`-100..=100`).
        overlap: i16,
    },
    /// `c:lineChart`.
    Line {
        /// `c:grouping`.
        grouping: Grouping,
        /// `c:marker` — whether series markers are drawn.
        marker: bool,
    },
    /// `c:areaChart`.
    Area {
        /// `c:grouping`.
        grouping: Grouping,
    },
    /// `c:pieChart`.
    Pie {
        /// `c:firstSliceAng`, degrees clockwise from twelve o'clock (`0..=360`).
        first_slice_angle: u16,
    },
    /// `c:doughnutChart`.
    Doughnut {
        /// `c:firstSliceAng`, degrees clockwise from twelve o'clock (`0..=360`).
        first_slice_angle: u16,
        /// `c:holeSize`, a percentage of the outer radius (`1..=90`).
        hole_size: u8,
    },
    /// `c:scatterChart`.
    Scatter {
        /// `c:scatterStyle`.
        style: ScatterStyle,
    },
}

/// A series line (`c:spPr/a:ln`), solid-colour only (`docs/155` §4.3).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChartLine {
    /// The `a:solidFill` colour, when the producer declared one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
    /// `a:ln@w`, the stroke width in EMU.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width_emu: Option<u32>,
    /// `a:ln/a:noFill` — the series is drawn with no line at all. Distinct from
    /// an absent colour, which means "use the theme's".
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub no_fill: bool,
}

/// Where a data label sits relative to its point (`c:dLblPos`).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DataLabelPosition {
    /// `bestFit`.
    #[default]
    BestFit,
    /// `b`.
    Bottom,
    /// `ctr`.
    Center,
    /// `inBase`.
    InsideBase,
    /// `inEnd`.
    InsideEnd,
    /// `l`.
    Left,
    /// `outEnd`.
    OutsideEnd,
    /// `r`.
    Right,
    /// `t`.
    Top,
}

/// Data-label settings (`c:dLbls`).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DataLabels {
    /// `c:showVal`.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub show_value: bool,
    /// `c:showCatName`.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub show_category_name: bool,
    /// `c:showSerName`.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub show_series_name: bool,
    /// `c:showPercent`.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub show_percent: bool,
    /// `c:dLblPos`, when the producer declared one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<DataLabelPosition>,
}

/// One data series (`c:ser`).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Series {
    /// `c:idx`.
    pub index: u32,
    /// `c:order`.
    pub order: u32,
    /// `c:tx` — the cached series name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<ChartText>,
    /// `c:cat` — the cached category labels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub categories: Option<DataRange>,
    /// `c:val`, or `c:yVal` for a scatter series.
    #[serde(default)]
    pub values: DataRange,
    /// `c:xVal` — a scatter series' x values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x_values: Option<DataRange>,
    /// The solid `c:spPr` fill colour. Gradient, picture and pattern fills are
    /// out of scope and reported `degraded` (`docs/155` §4.3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Color>,
    /// The series line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<ChartLine>,
    /// `c:smooth` — a smoothed line. Drawn as a sampled polyline, which is what
    /// the market leader ships (`docs/155` §3.3, §7.3).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub smooth: bool,
    /// `c:dLbls`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_labels: Option<DataLabels>,
    /// Children carried verbatim ([`ChartXml`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retained: Vec<ChartXml>,
}

/// A chart group: one plotting family and the series drawn with it (`docs/155`
/// §4.2).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChartGroup {
    /// Which family this group draws.
    pub kind: ChartGroupKind,
    /// The group's series, in declaration order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub series: Vec<Series>,
    /// `c:axId`, in declaration order — the axes in [`PlotArea::axes`] this group
    /// is plotted against. A group naming a second value axis *is* a secondary
    /// axis; nothing else is needed to represent one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub axis_ids: Vec<u32>,
    /// `c:varyColors` — colour each point rather than each series.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub vary_colors: bool,
    /// Children carried verbatim ([`ChartXml`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retained: Vec<ChartXml>,
}

/// Which kind of axis an [`Axis`] is.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AxisKind {
    /// `c:catAx`.
    #[default]
    Category,
    /// `c:valAx`.
    Value,
    /// `c:dateAx`.
    Date,
}

/// Which edge of the plot area an axis is drawn on (`c:axPos`).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AxisPosition {
    /// `b`.
    #[default]
    Bottom,
    /// `l`.
    Left,
    /// `r`.
    Right,
    /// `t`.
    Top,
}

/// An axis' value direction (`c:scaling/c:orientation`).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AxisOrientation {
    /// `minMax` — the usual direction.
    #[default]
    MinMax,
    /// `maxMin` — reversed.
    MaxMin,
}

/// Tick-mark style on an axis (`c:majorTickMark`, `c:minorTickMark`).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TickMark {
    /// `none`.
    None,
    /// `in`.
    Inside,
    /// `out` — the Word default.
    #[default]
    Outside,
    /// `cross`.
    Cross,
}

/// Where an axis' tick labels are drawn (`c:tickLblPos`).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TickLabelPosition {
    /// `nextTo`.
    #[default]
    NextTo,
    /// `high`.
    High,
    /// `low`.
    Low,
    /// `none`.
    None,
}

/// One chart axis (`c:catAx`, `c:valAx`, `c:dateAx`).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Axis {
    /// `c:axId` — the identity a [`ChartGroup::axis_ids`] entry names.
    pub id: u32,
    /// Which kind of axis this is.
    #[serde(default)]
    pub kind: AxisKind,
    /// `c:axPos`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<AxisPosition>,
    /// `c:delete` — the axis exists in the model but is not drawn.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub deleted: bool,
    /// `c:scaling/c:orientation`.
    #[serde(default)]
    pub orientation: AxisOrientation,
    /// `c:scaling/c:min`, in its **verbatim lexical form** — the same no-float
    /// rule as [`ChartValue::Number`]. Parse it with
    /// [`ChartValue::as_f64`] through [`ChartValue::Number`], or with the
    /// consumer's own parser.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum: Option<String>,
    /// `c:scaling/c:max`, verbatim. See [`Axis::minimum`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum: Option<String>,
    /// `c:majorGridlines`.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub major_gridlines: bool,
    /// `c:minorGridlines`.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub minor_gridlines: bool,
    /// `c:majorTickMark`.
    #[serde(default)]
    pub major_tick_mark: TickMark,
    /// `c:minorTickMark`.
    #[serde(default)]
    pub minor_tick_mark: TickMark,
    /// `c:tickLblPos`.
    #[serde(default)]
    pub tick_label_position: TickLabelPosition,
    /// `c:numFmt@formatCode`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number_format: Option<String>,
    /// `c:crossAx` — the axis this one crosses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cross_axis_id: Option<u32>,
    /// Children carried verbatim ([`ChartXml`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retained: Vec<ChartXml>,
}

/// The plot area (`c:plotArea`): a list of chart groups and a list of axes.
///
/// Both are lists for the same reason, and it is the central modeling decision
/// of `docs/155` §4.2: a combo chart is more than one group, and a secondary axis
/// is one more axis that a group names. Neither is a feature to add later.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlotArea {
    /// The chart groups, in declaration order. More than one **is** a combo
    /// chart.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<ChartGroup>,
    /// The axes, each with its own [`Axis::id`], in declaration order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub axes: Vec<Axis>,
    /// Children carried verbatim ([`ChartXml`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retained: Vec<ChartXml>,
}

/// A typed projection of one DrawingML chart part.
///
/// Resolves in `Definitions::charts`, keyed by [`ChartId`](crate::v1::ChartId).
/// Like every other definition in that table the chart does **not** carry its own
/// id: the map key is the identity, so an id field and a key that disagree are
/// unrepresentable rather than merely invalid. (`docs/155` §8.3 sketched an `id`
/// field; every v1 definition value omits one — `MediaReference`'s doc comment
/// states the rule — and the `FieldRangeId` comment in `v1::ids` is the same
/// argument. The deviation is deliberate.)
///
/// No `Default`: a projection with no anchor describes nothing, so there is no
/// meaningful default `Chart` and `NodeId` has none either.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Chart {
    /// The `EmbeddedObject` node this projects — a `NodeId` anchor, per
    /// `docs/45` invariant I3. Must resolve to an object whose kind is
    /// `EmbeddedKind::Chart`; a dangling projection is a `ModelError`, exactly as
    /// a dangling media reference is.
    pub object: NodeId,
    /// How much of the source part this projection captured. `Partial` forbids
    /// regeneration — see [`ChartCoverage::permits_regeneration`].
    #[serde(default)]
    pub coverage: ChartCoverage,
    /// `c:title` — cached text plus `c:overlay`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<ChartTitle>,
    /// `c:autoTitleDeleted`.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub auto_title_deleted: bool,
    /// `c:plotArea`.
    #[serde(default)]
    pub plot_area: PlotArea,
    /// `c:legend`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legend: Option<Legend>,
    /// `c:plotVisOnly` — plot visible cells only.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub plot_visible_only: bool,
    /// `c:dispBlanksAs`.
    #[serde(default)]
    pub display_blanks_as: DisplayBlanks,
    /// `c:varyColors` at chart-space level.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub vary_colors: bool,
    /// The embedded workbook named by `c:externalData`, when present: a part name
    /// and a relationship id, **never bytes** (`docs/45` I4, `docs/155` §5.2).
    /// Nothing in this crate or its consumers opens it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_data: Option<EmbeddedPart>,
    /// Whether an edit has made this projection the AUTHORITY over the chart's
    /// source part — `docs/155` §6.1's "a chart is dirty or it is not".
    ///
    /// `false` (every imported chart, until edited): export copies the retained
    /// part bytes and this projection is only a read index over them. `true`:
    /// export regenerates the part from this projection, and the retained bytes
    /// for the part — and for the embedded workbook it names, which a values-only
    /// workbook replaces — are superseded rather than written beside it. Only a
    /// `Complete` projection may be dirty; a writer still consults
    /// [`ChartCoverage::permits_regeneration`] and never trusts the bit alone.
    ///
    /// Provenance, not content: it is set by the data edit and cleared by
    /// nothing, because once a reader has changed a chart the source bytes no
    /// longer describe it.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub dirty: bool,
    /// The namespace declarations on the source `c:chartSpace`, as
    /// `(prefix, uri)`, so the prefixes inside carried fragments resolve when
    /// the part is regenerated.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub namespaces: Vec<(String, String)>,
    /// `c:chartSpace` children carried verbatim ([`ChartXml`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub space_retained: Vec<ChartXml>,
    /// `c:chart` children carried verbatim ([`ChartXml`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chart_retained: Vec<ChartXml>,
}

impl Chart {
    /// Total bytes of every [`ChartXml`] fragment this chart carries, across
    /// all its containers. O(fragments).
    #[must_use]
    pub fn carried_xml_bytes(&self) -> usize {
        let sum = |fragments: &[ChartXml]| fragments.iter().map(|f| f.xml.len()).sum::<usize>();
        sum(&self.space_retained)
            + sum(&self.chart_retained)
            + self.title.as_ref().map_or(0, |title| sum(&title.retained))
            + self
                .legend
                .as_ref()
                .map_or(0, |legend| sum(&legend.retained))
            + sum(&self.plot_area.retained)
            + self
                .plot_area
                .axes
                .iter()
                .map(|axis| sum(&axis.retained))
                .sum::<usize>()
            + self
                .plot_area
                .groups
                .iter()
                .map(|group| {
                    sum(&group.retained)
                        + group
                            .series
                            .iter()
                            .map(|series| sum(&series.retained))
                            .sum::<usize>()
                })
                .sum::<usize>()
    }

    /// Every [`DataRange`] this chart holds, in a stable order.
    ///
    /// Exists so a bounds check or a consumer enumerates ranges in one place
    /// rather than re-deriving which fields hold one — a series gained `x_values`
    /// for scatter, and a check written against `values` alone would silently
    /// stop covering half the data.
    ///
    /// Complexity: O(series in the chart); it borrows, and does not touch points.
    pub fn data_ranges(&self) -> impl Iterator<Item = &DataRange> {
        self.plot_area
            .groups
            .iter()
            .flat_map(|group| group.series.iter())
            .flat_map(|series| {
                [
                    Some(&series.values),
                    series.categories.as_ref(),
                    series.x_values.as_ref(),
                ]
            })
            .flatten()
    }
}
