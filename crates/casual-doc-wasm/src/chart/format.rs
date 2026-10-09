//! Chart **formatting**: text fonts, axis titles and number formats, each
//! series' colour, line, number format, trendline and error bars, and the
//! combination charts and secondary axes a per-series chart type makes
//! (`docs/155` §19).
//!
//! # The interaction, from the competitive standard
//!
//! Word's *Format Chart Area ▸ Text Options* and Home-tab font controls on a
//! selected chart element; *Format Data Series* (fill and line, dash type,
//! width, "Plot Series On ▸ Secondary Axis"); *Chart Elements ▸ Trendline /
//! Error Bars / Axis Titles*; *Change Chart Type ▸ Combo*, which gives every
//! series its own type and a secondary-axis box. ONLYOFFICE's chart Advanced
//! Settings carries the same controls (`ChartSettingsDlg.js`: per-series type
//! and secondary axis in the Combo tab, axis titles and number formats in the
//! axis tabs). Every control here is one of those, and every one rides the
//! same single write (`setChartData`'s optional `format`), so a formatting
//! change is one undo step through the same choke point as a data edit.
//!
//! # Shadows: the model wins once a reader edits
//!
//! An imported chart carries Word's formatting verbatim (`docs/155` §17). An
//! edit to a modelled value drops the verbatim copy it would contradict: a font
//! edit drops the container's carried `c:txPr` (and a title's carried `c:tx`,
//! whose runs hold their own sizes — the text itself is modelled and kept); a
//! series colour, width or dash drops its carried `c:spPr`; a trendline or
//! error-bar edit drops any carried `c:trendline` / `c:errBars`.
//!
//! # Plot order
//!
//! A series is addressed by its position in **plot order** — sorted by `c:order`,
//! ties in document order — across every group. That is the order Word lists
//! series in (legend, Select Data) and the order the data grid shows columns
//! in, and it is stable when a series moves to another group, which document
//! order is not.
//!
//! # Complexity
//!
//! Everything here is O(series × log series + axes + carried fragments) in the
//! ONE chart edited. Nothing walks the document.

use serde::{Deserialize, Serialize};

use casual_doc_edit::refusal;
use casual_doc_model::v1::{
    Axis, AxisKind, AxisPosition, BarGrouping, CHART_FONT_SIZE_RANGE, Chart, ChartContainer,
    ChartFont, ChartGroup, ChartGroupKind, ChartLine, ChartText, ChartTitle, ChartValue, ChartXml,
    Color, DashStyle, Definitions, ErrorBarDirection, ErrorBarType, ErrorBars, ErrorValueType,
    Grouping, MAX_CHART_AXES, MAX_CHART_GROUPS, MAX_CHART_NUMBER_BYTES, MAX_CHART_TEXT_BYTES,
    MAX_CHART_TYPEFACE_BYTES, RgbColor, Series, ThemeColor, ThemeColorRef, Trendline,
    TrendlineKind, chart_child_rank,
};

use super::{CHART_GALLERY, chart_kind_token, check_name, is_horizontal, keep_carried, row_count};
use crate::{chart_group_for_kind, unpainted_chart_refusal};

/// The chart-type tokens a series may take inside a combination chart.
///
/// Word's Combo dialog mixes the column, line and area families on shared
/// category axes. Pie and doughnut colour by point and have no axes, scatter
/// plots numeric x values the shared category axis cannot, and a horizontal
/// bar swaps the axes every other family plots against — Word refuses each
/// of those in a combination ("Some chart types cannot be combined with other
/// chart types"), and so does this.
pub(crate) const COMBINABLE: [&str; 10] = [
    "column",
    "column-stacked",
    "column-percent",
    "line",
    "line-markers",
    "line-stacked",
    "line-percent",
    "area",
    "area-stacked",
    "area-percent",
];

/// The dash tokens a series line may take — `a:prstDash`'s eleven presets, in
/// the order Word's Dash type menu lists them.
pub(crate) const DASHES: [&str; 11] = [
    "solid",
    "dot",
    "dash",
    "lgDash",
    "dashDot",
    "lgDashDot",
    "lgDashDotDot",
    "sysDash",
    "sysDot",
    "sysDashDot",
    "sysDashDotDot",
];

/// The number-format presets the editor offers: Word's Number category list
/// (General, Number, Currency, Percentage, Scientific) in its common forms.
/// Any other format code may still be typed; these are suggestions.
pub(crate) const NUMBER_FORMATS: [&str; 10] = [
    "General",
    "0",
    "0.00",
    "#,##0",
    "#,##0.00",
    "0%",
    "0.00%",
    "$#,##0.00",
    "€#,##0.00",
    "0.00E+00",
];

/// A series line's weight when the file declares none, in points — the
/// renderer's own default (`casual_doc_layout::chart`'s `SERIES_LINE_WIDTH`,
/// 30 twips), so the panel shows the weight the page paints. Word 2016 writes
/// an explicit 2.25 pt on every line series it creates, which this reads back
/// as written.
const DEFAULT_SERIES_LINE_PT: f64 = 1.5;

/// EMU per point.
const EMU_PER_POINT: f64 = 12_700.0;

/// The colour chart text is painted in when nothing declares one: Word 2016's
/// `tx1` at 65% luminance + 35% offset over the Office theme, which is the
/// renderer's `ChartStyle::text`.
const DEFAULT_TEXT_COLOR: &str = "#595959";

/// One text element's font, as the panel's font controls show it.
///
/// Every field is the EFFECTIVE value — what the page paints — so the panel
/// never has to know the inheritance rule (element, then chart, then Word's
/// default for that element).
#[derive(Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FontView {
    /// The face name, with a theme reference (`+mn-lt`, `+mj-lt`) resolved to
    /// the document theme's face (Calibri / Calibri Light when it has none).
    pub(crate) typeface: String,
    /// Points.
    pub(crate) size: f64,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    /// `#RRGGBB`.
    pub(crate) color: String,
    /// Whether the element exists (a chart without a title has no title font
    /// to edit).
    pub(crate) present: bool,
}

/// The five text elements' fonts.
#[derive(Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FontsView {
    pub(crate) chart: FontView,
    pub(crate) title: FontView,
    pub(crate) legend: FontView,
    pub(crate) horizontal_axis: FontView,
    pub(crate) vertical_axis: FontView,
}

/// A series' trendline, as the panel shows it.
#[derive(Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TrendlineView {
    /// `none`, or a `c:trendlineType` token.
    pub(crate) kind: &'static str,
    /// A polynomial's degree (2 when not polynomial: the value the control
    /// starts at).
    pub(crate) order: u8,
    /// A moving average's window (2 when not a moving average).
    pub(crate) period: u32,
    pub(crate) equation: bool,
    pub(crate) r_squared: bool,
}

/// A series' value error bars, as the panel shows them.
#[derive(Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ErrorBarsView {
    /// `none`, or a `c:errValType` token.
    pub(crate) kind: &'static str,
    /// `c:val`, verbatim, or empty.
    pub(crate) value: String,
    /// `both`, `plus` or `minus`.
    #[serde(rename = "type")]
    pub(crate) bar_type: &'static str,
}

/// One series' formatting, as the Format Data Series controls show it.
#[derive(Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SeriesFormatView {
    pub(crate) name: String,
    /// The [`CHART_GALLERY`] token of the group the series is plotted in.
    pub(crate) kind: &'static str,
    /// Plotted against the secondary axes.
    pub(crate) secondary: bool,
    /// `#RRGGBB`: the explicit colour, or the theme accent it paints with.
    pub(crate) color: String,
    /// Points, for the line families; 0 when the family has no series line.
    pub(crate) line_width: f64,
    /// A [`DASHES`] token.
    pub(crate) dash: &'static str,
    /// Whether the family draws a series line (line, scatter).
    pub(crate) has_line: bool,
    /// The values' `c:formatCode`, or empty for General / source-linked.
    pub(crate) number_format: String,
    pub(crate) trendline: TrendlineView,
    pub(crate) error_bars: ErrorBarsView,
    pub(crate) admits_trendline: bool,
    pub(crate) admits_error_bars: bool,
}

/// One element's font change. Absent fields are left; `""` / `0` inherit.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub(crate) struct FontPatch {
    typeface: Option<String>,
    size: Option<f64>,
    bold: Option<bool>,
    italic: Option<bool>,
    color: Option<String>,
}

/// The five elements' font changes.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub(crate) struct FontsPatch {
    chart: Option<FontPatch>,
    title: Option<FontPatch>,
    legend: Option<FontPatch>,
    horizontal_axis: Option<FontPatch>,
    vertical_axis: Option<FontPatch>,
}

/// A trendline change: `kind` `none` removes it.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub(crate) struct TrendlinePatch {
    kind: String,
    order: Option<u8>,
    period: Option<u32>,
    equation: Option<bool>,
    r_squared: Option<bool>,
}

/// An error-bar change: `kind` `none` removes them.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub(crate) struct ErrorBarsPatch {
    kind: String,
    value: Option<String>,
    #[serde(rename = "type")]
    bar_type: Option<String>,
}

/// One series' changes, addressed by plot order.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub(crate) struct SeriesFormatPatch {
    index: usize,
    kind: Option<String>,
    secondary: Option<bool>,
    color: Option<String>,
    line_width: Option<f64>,
    dash: Option<String>,
    number_format: Option<String>,
    trendline: Option<TrendlinePatch>,
    error_bars: Option<ErrorBarsPatch>,
}

/// Which text element a font belongs to, for its Word default size.
#[derive(Clone, Copy)]
enum FontRole {
    Chart,
    Title,
    Furniture,
}

/// Word 2016's default chart text size for an element, in points: 10 pt for
/// the chart space (DrawingML's chart text default), 14 pt for the chart
/// title, 9 pt for the legend and the axis labels (the `sz="1400"` and
/// `sz="900"` its default chart style writes). O(1).
const fn default_size_pt(role: FontRole) -> f64 {
    match role {
        FontRole::Chart => 10.0,
        FontRole::Title => 14.0,
        FontRole::Furniture => 9.0,
    }
}

/// `color` as the page paints it, `#RRGGBB`. O(1).
pub(crate) fn hex(definitions: &Definitions, color: Color) -> String {
    let [r, g, b, _] = casual_doc_layout::flow::chart_color_rgba(definitions, color);
    format!("#{r:02X}{g:02X}{b:02X}")
}

/// A face name with a theme reference resolved: `+mn-*` to the theme's minor
/// (body) Latin face, `+mj-*` to its major (heading) one, and Word's Office
/// theme faces when the document declares no font scheme. The East Asian and
/// complex-script references resolve to the Latin face too: the font box shows
/// one name, and it is the Latin one Word shows. O(1).
fn resolve_typeface(face: &str, definitions: &Definitions) -> String {
    let pick = |major: bool| {
        definitions
            .font_scheme
            .as_ref()
            .map(|scheme| {
                if major {
                    &scheme.major.latin.typeface
                } else {
                    &scheme.minor.latin.typeface
                }
            })
            .filter(|face| !face.is_empty())
            .map_or_else(
                || if major { "Calibri Light" } else { "Calibri" }.to_owned(),
                Clone::clone,
            )
    };
    match face {
        "+mn-lt" | "+mn-ea" | "+mn-cs" => pick(false),
        "+mj-lt" | "+mj-ea" | "+mj-cs" => pick(true),
        other => other.to_owned(),
    }
}

/// The effective font of one element. O(1).
fn font_view(
    own: Option<&ChartFont>,
    base: Option<&ChartFont>,
    role: FontRole,
    present: bool,
    definitions: &Definitions,
) -> FontView {
    let effective = own.map_or_else(|| base.cloned().unwrap_or_default(), |own| own.over(base));
    FontView {
        typeface: resolve_typeface(
            effective.typeface.as_deref().unwrap_or("+mn-lt"),
            definitions,
        ),
        size: effective
            .size
            .map_or(default_size_pt(role), |size| f64::from(size) / 100.0),
        bold: effective.bold.unwrap_or(false),
        italic: effective.italic.unwrap_or(false),
        color: effective.color.map_or_else(
            || DEFAULT_TEXT_COLOR.to_owned(),
            |color| hex(definitions, color),
        ),
        present,
    }
}

/// Every text element's effective font. O(axes).
pub(crate) fn fonts_view(chart: &Chart, definitions: &Definitions) -> FontsView {
    let base = chart.font.as_ref();
    let axis = |horizontal| {
        let axis = role_axis_index(chart, horizontal).map(|index| &chart.plot_area.axes[index]);
        font_view(
            axis.and_then(|axis| axis.font.as_ref()),
            base,
            FontRole::Furniture,
            axis.is_some(),
            definitions,
        )
    };
    FontsView {
        chart: font_view(base, None, FontRole::Chart, true, definitions),
        title: font_view(
            chart.title.as_ref().and_then(|title| title.font.as_ref()),
            base,
            FontRole::Title,
            chart.title.is_some(),
            definitions,
        ),
        legend: font_view(
            chart
                .legend
                .as_ref()
                .and_then(|legend| legend.font.as_ref()),
            base,
            FontRole::Furniture,
            chart.legend.is_some(),
            definitions,
        ),
        horizontal_axis: axis(true),
        vertical_axis: axis(false),
    }
}

/// Every series as `(group, series)` indices, in plot order: by `c:order`,
/// ties in document order. O(series × log series).
pub(crate) fn plot_order(chart: &Chart) -> Vec<(usize, usize)> {
    let groups = &chart.plot_area.groups;
    let mut all: Vec<(usize, usize)> = groups
        .iter()
        .enumerate()
        .flat_map(|(group, g)| (0..g.series.len()).map(move |series| (group, series)))
        .collect();
    // Stable, so equal `c:order`s keep document order.
    all.sort_by_key(|&(group, series)| groups[group].series[series].order);
    all
}

/// The primary axis ids: those of the first group not plotted against a
/// right-hand (or top) value axis, else the first group's. O(groups × axes).
pub(crate) fn primary_axis_ids(chart: &Chart) -> Vec<u32> {
    let axes = &chart.plot_area.axes;
    let off_primary_edge = |group: &ChartGroup| {
        group.axis_ids.iter().any(|id| {
            axes.iter().any(|axis| {
                axis.id == *id
                    && axis.kind == AxisKind::Value
                    && matches!(axis.position, Some(AxisPosition::Right | AxisPosition::Top))
            })
        })
    };
    let groups = &chart.plot_area.groups;
    groups
        .iter()
        .find(|group| !group.axis_ids.is_empty() && !off_primary_edge(group))
        .or_else(|| groups.first())
        .map(|group| group.axis_ids.clone())
        .unwrap_or_default()
}

/// Whether `group` plots against axes other than the primary pair. O(ids).
fn is_secondary(group: &ChartGroup, primary: &[u32]) -> bool {
    !group.axis_ids.is_empty() && group.axis_ids != primary
}

/// The index of the PRIMARY axis in one role (horizontal or vertical), falling
/// back to any axis in that role. Shared by the view and the patch, so the
/// axis the panel shows is the axis an edit changes. O(axes).
pub(crate) fn role_axis_index(chart: &Chart, horizontal: bool) -> Option<usize> {
    let primary = primary_axis_ids(chart);
    let axes = &chart.plot_area.axes;
    axes.iter()
        .position(|axis| {
            (primary.is_empty() || primary.contains(&axis.id)) && is_horizontal(axis) == horizontal
        })
        .or_else(|| {
            axes.iter()
                .position(|axis| is_horizontal(axis) == horizontal)
        })
}

/// The dash token for a model dash. Exhaustive, so a new `DashStyle` is a
/// compile error here rather than a dash the panel cannot show. O(1).
const fn dash_token(dash: DashStyle) -> &'static str {
    match dash {
        DashStyle::Solid => "solid",
        DashStyle::Dot => "dot",
        DashStyle::Dash => "dash",
        DashStyle::LargeDash => "lgDash",
        DashStyle::DashDot => "dashDot",
        DashStyle::LargeDashDot => "lgDashDot",
        DashStyle::LargeDashDotDot => "lgDashDotDot",
        DashStyle::SystemDash => "sysDash",
        DashStyle::SystemDot => "sysDot",
        DashStyle::SystemDashDot => "sysDashDot",
        DashStyle::SystemDashDotDot => "sysDashDotDot",
    }
}

/// The model dash a token names. O(dashes).
fn dash_value(token: &str) -> Option<DashStyle> {
    [
        DashStyle::Solid,
        DashStyle::Dot,
        DashStyle::Dash,
        DashStyle::LargeDash,
        DashStyle::DashDot,
        DashStyle::LargeDashDot,
        DashStyle::LargeDashDotDot,
        DashStyle::SystemDash,
        DashStyle::SystemDot,
        DashStyle::SystemDashDot,
        DashStyle::SystemDashDotDot,
    ]
    .into_iter()
    .find(|dash| dash_token(*dash) == token)
}

/// `c:trendlineType` tokens. O(1).
const fn trendline_token(kind: TrendlineKind) -> &'static str {
    match kind {
        TrendlineKind::Linear => "linear",
        TrendlineKind::Exponential => "exp",
        TrendlineKind::Logarithmic => "log",
        TrendlineKind::Polynomial => "poly",
        TrendlineKind::Power => "power",
        TrendlineKind::MovingAverage => "movingAvg",
    }
}

/// `c:errValType` tokens. O(1).
const fn error_value_token(kind: ErrorValueType) -> &'static str {
    match kind {
        ErrorValueType::Custom => "cust",
        ErrorValueType::FixedValue => "fixedVal",
        ErrorValueType::Percentage => "percentage",
        ErrorValueType::StandardDeviation => "stdDev",
        ErrorValueType::StandardError => "stdErr",
    }
}

/// `c:errBarType` tokens. O(1).
const fn error_type_token(kind: ErrorBarType) -> &'static str {
    match kind {
        ErrorBarType::Both => "both",
        ErrorBarType::Plus => "plus",
        ErrorBarType::Minus => "minus",
    }
}

/// Whether a family draws a series LINE (and so has a width and a dash). O(1).
pub(crate) const fn has_line(kind: ChartGroupKind) -> bool {
    matches!(
        kind,
        ChartGroupKind::Line { .. } | ChartGroupKind::Scatter { .. }
    )
}

/// Whether a family's series take a trendline: the schema admits one
/// (`CT_PieSer` has no place for it) and the family is not stacked — Word
/// greys Trendline out for stacked and 100% stacked charts, where a fit through
/// a cumulative value describes no series. O(sequence).
pub(crate) fn admits_trendline(kind: ChartGroupKind) -> bool {
    let stacked = match kind {
        ChartGroupKind::Bar { grouping, .. } => {
            matches!(grouping, BarGrouping::Stacked | BarGrouping::PercentStacked)
        }
        ChartGroupKind::Line { grouping, .. } | ChartGroupKind::Area { grouping } => {
            matches!(grouping, Grouping::Stacked | Grouping::PercentStacked)
        }
        _ => false,
    };
    !stacked && chart_child_rank(ChartContainer::Series(kind), "trendline").is_some()
}

/// Whether a family's series take error bars (the schema decides: bar, line,
/// area and scatter do; pie and doughnut do not). O(sequence).
pub(crate) fn admits_error_bars(kind: ChartGroupKind) -> bool {
    chart_child_rank(ChartContainer::Series(kind), "errBars").is_some()
}

/// The value-direction error bars of a series: those with no direction or a
/// `y` one (a scatter series may also carry `x` bars, which stay as they are).
fn value_bars(series: &Series) -> Option<&ErrorBars> {
    series
        .error_bars
        .iter()
        .find(|bars| bars.direction != Some(ErrorBarDirection::X))
}

/// Every series' formatting, in plot order. O(series × log series).
pub(crate) fn series_views(chart: &Chart, definitions: &Definitions) -> Vec<SeriesFormatView> {
    let groups = &chart.plot_area.groups;
    let primary = primary_axis_ids(chart);
    // The renderer's accent cycle runs in DOCUMENT order across groups
    // (`casual_doc_layout::chart::series_colors`), so the automatic colour is
    // read off that position, not the plot position.
    let mut document_position = Vec::new();
    for (group, g) in groups.iter().enumerate() {
        for series in 0..g.series.len() {
            document_position.push((group, series));
        }
    }
    const ACCENTS: [ThemeColorRef; 6] = [
        ThemeColorRef::Accent1,
        ThemeColorRef::Accent2,
        ThemeColorRef::Accent3,
        ThemeColorRef::Accent4,
        ThemeColorRef::Accent5,
        ThemeColorRef::Accent6,
    ];
    plot_order(chart)
        .into_iter()
        .map(|(group, index)| {
            let g = &groups[group];
            let series = &g.series[index];
            let line = has_line(g.kind);
            let explicit = if line {
                series.line.and_then(|line| line.color).or(series.fill)
            } else {
                series.fill
            };
            let position = document_position
                .iter()
                .position(|at| *at == (group, index))
                .unwrap_or(0);
            let color = explicit.unwrap_or(Color::Theme(ThemeColor {
                slot: ACCENTS[position % ACCENTS.len()],
                theme_tint: None,
                theme_shade: None,
            }));
            let trendline = series.trendlines.first();
            let bars = value_bars(series);
            SeriesFormatView {
                name: series
                    .name
                    .as_ref()
                    .map(|name| name.text.clone())
                    .unwrap_or_default(),
                kind: chart_kind_token(g.kind),
                secondary: is_secondary(g, &primary),
                color: hex(definitions, color),
                line_width: if line {
                    series
                        .line
                        .and_then(|line| line.width_emu)
                        .map_or(DEFAULT_SERIES_LINE_PT, |emu| f64::from(emu) / EMU_PER_POINT)
                } else {
                    0.0
                },
                dash: series
                    .line
                    .and_then(|line| line.dash)
                    .map_or("solid", dash_token),
                has_line: line,
                number_format: series.values.number_format.clone().unwrap_or_default(),
                trendline: trendline.map_or(
                    TrendlineView {
                        kind: "none",
                        order: 2,
                        period: 2,
                        equation: false,
                        r_squared: false,
                    },
                    |line| TrendlineView {
                        kind: trendline_token(line.kind),
                        order: line.order.unwrap_or(2),
                        period: line.period.unwrap_or(2),
                        equation: line.display_equation,
                        r_squared: line.display_r_squared,
                    },
                ),
                error_bars: bars.map_or(
                    ErrorBarsView {
                        kind: "none",
                        value: String::new(),
                        bar_type: "both",
                    },
                    |bars| ErrorBarsView {
                        kind: error_value_token(bars.value_type),
                        value: bars.value.clone().unwrap_or_default(),
                        bar_type: error_type_token(bars.bar_type),
                    },
                ),
                admits_trendline: admits_trendline(g.kind),
                admits_error_bars: admits_error_bars(g.kind),
            }
        })
        .collect()
}

/// `#RRGGBB` as a model colour, `""` as `None` (automatic), anything else a
/// refusal naming the text. O(1).
fn parse_color(text: &str, what: &str) -> Result<Option<Color>, String> {
    if text.is_empty() {
        return Ok(None);
    }
    let digits = text
        .strip_prefix('#')
        .filter(|digits| digits.len() == 6 && digits.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let Some(digits) = digits else {
        return Err(refusal::marked(
            "chart.color",
            &format!("“{text}” is not a colour for {what}; use the form #RRGGBB."),
        ));
    };
    let channel = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).unwrap_or(0);
    Ok(Some(Color::Rgb(RgbColor {
        r: channel(0),
        g: channel(2),
        b: channel(4),
    })))
}

/// A number format code: `""` clears it (General / source-linked); otherwise
/// bounded by the model's text ceiling and free of control characters, which
/// no format code holds and an XML attribute cannot carry. O(text).
pub(crate) fn number_format_value(text: &str, what: &str) -> Result<Option<String>, String> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    if text.len() > MAX_CHART_TEXT_BYTES || text.chars().any(char::is_control) {
        return Err(refusal::marked(
            "chart.number-format",
            &format!(
                "That is not a number format {what} can use: a format code is at most \
                 {MAX_CHART_TEXT_BYTES} bytes and holds no control characters."
            ),
        ));
    }
    Ok(Some(text.to_owned()))
}

/// Applies one font change to `slot`. `Ok(true)` when the patch named any
/// field — the caller then drops the carried formatting it supersedes.
/// O(1).
fn apply_font(slot: &mut Option<ChartFont>, patch: &FontPatch, what: &str) -> Result<bool, String> {
    let mut font = slot.clone().unwrap_or_default();
    let mut touched = false;
    if let Some(face) = &patch.typeface {
        touched = true;
        let face = face.trim();
        font.typeface = if face.is_empty() {
            None
        } else if face.len() > MAX_CHART_TYPEFACE_BYTES || face.chars().any(char::is_control) {
            return Err(refusal::marked(
                "chart.font-typeface",
                &format!(
                    "That is not a font name {what} can use: a face name is at most \
                     {MAX_CHART_TYPEFACE_BYTES} bytes and holds no control characters."
                ),
            ));
        } else {
            Some(face.to_owned())
        };
    }
    if let Some(size) = patch.size {
        touched = true;
        font.size = if size == 0.0 {
            None
        } else {
            let hundredths = (size * 100.0).round();
            let valid = size.is_finite()
                && hundredths >= f64::from(*CHART_FONT_SIZE_RANGE.start())
                && hundredths <= f64::from(*CHART_FONT_SIZE_RANGE.end());
            if !valid {
                return Err(refusal::marked(
                    "chart.font-size",
                    &format!("{what} can be 1 to 4000 points; {size} is outside that."),
                ));
            }
            // In range, so the cast is exact.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Some(hundredths as u32)
        };
    }
    if let Some(bold) = patch.bold {
        touched = true;
        font.bold = Some(bold);
    }
    if let Some(italic) = patch.italic {
        touched = true;
        font.italic = Some(italic);
    }
    if let Some(color) = &patch.color {
        touched = true;
        font.color = parse_color(color, what)?;
    }
    if touched {
        *slot = (!font.is_empty()).then_some(font);
    }
    Ok(touched)
}

/// Drops the carried fragments named `names`. O(fragments).
fn drop_carried(fragments: &mut Vec<ChartXml>, names: &[&str]) {
    fragments.retain(|fragment| !names.contains(&fragment.name.as_str()));
}

/// Applies every element's font change, dropping each edited container's
/// carried `c:txPr` (and a title's carried rich `c:tx`). An element the chart
/// does not have is refused rather than created: a font is not a title.
///
/// Complexity: O(axes + carried fragments).
pub(crate) fn apply_fonts(next: &mut Chart, fonts: &FontsPatch) -> Result<(), String> {
    if let Some(patch) = &fonts.chart
        && apply_font(&mut next.font, patch, "the chart text")?
    {
        drop_carried(&mut next.space_retained, &["txPr"]);
    }
    if let Some(patch) = &fonts.title {
        let title = next.title.as_mut().ok_or_else(|| {
            refusal::marked(
                "chart.no-title",
                "This chart has no title, so it has no title font. Add a title first.",
            )
        })?;
        if apply_font(&mut title.font, patch, "the chart title")? {
            drop_carried(&mut title.retained, &["txPr", "tx"]);
        }
    }
    if let Some(patch) = &fonts.legend {
        let legend = next.legend.as_mut().ok_or_else(|| {
            refusal::marked(
                "chart.no-legend",
                "This chart has no legend, so it has no legend font. Show the legend first.",
            )
        })?;
        if apply_font(&mut legend.font, patch, "the legend")? {
            drop_carried(&mut legend.retained, &["txPr"]);
        }
    }
    for (patch, horizontal, role) in [
        (&fonts.horizontal_axis, true, "horizontal"),
        (&fonts.vertical_axis, false, "vertical"),
    ] {
        let Some(patch) = patch else {
            continue;
        };
        let index = role_axis_index(next, horizontal).ok_or_else(|| {
            refusal::marked("chart.no-axis", &format!("This chart has no {role} axis."))
        })?;
        let axis = &mut next.plot_area.axes[index];
        if apply_font(&mut axis.font, patch, &format!("the {role} axis labels"))? {
            drop_carried(&mut axis.retained, &["txPr"]);
        }
    }
    Ok(())
}

/// Sets or removes an axis title. `""` removes it; a changed text drops the
/// title's carried rich `c:tx` (whose runs spell the old words), keeping its
/// font and overlay. O(text + fragments).
pub(crate) fn apply_axis_title(axis: &mut Axis, text: &str, role: &str) -> Result<(), String> {
    // An axis title is carried as a fragment by an importer that did not model
    // it; the model decides now, so a verbatim copy never doubles it.
    drop_carried(&mut axis.retained, &["title"]);
    if text.trim().is_empty() {
        axis.title = None;
        return Ok(());
    }
    check_name(text, &format!("The {role} axis title"))?;
    let previous = axis.title.take();
    let same = previous
        .as_ref()
        .and_then(|title| title.text.as_ref())
        .is_some_and(|old| old.text == text && old.formula.is_none());
    axis.title = Some(ChartTitle {
        text: Some(ChartText {
            text: text.to_owned(),
            formula: None,
        }),
        overlay: previous.as_ref().is_some_and(|title| title.overlay),
        font: previous.as_ref().and_then(|title| title.font.clone()),
        retained: previous.map_or_else(Vec::new, |title| {
            keep_carried(
                &title.retained,
                ChartContainer::Title,
                if same { &[] } else { &["tx"] },
            )
        }),
    });
    Ok(())
}

/// A gallery token as its `'static` spelling, or the unpainted-family
/// refusal. O(gallery).
fn gallery_token(token: &str) -> Result<&'static str, String> {
    CHART_GALLERY
        .iter()
        .copied()
        .find(|known| *known == token)
        .ok_or_else(|| unpainted_chart_refusal(token))
}

/// The secondary axis pair's ids — `[category, value]` — found or created.
///
/// Found: a value axis on the right that is not primary, and the axis it
/// crosses. Created: Word's own secondary pair — a deleted category axis at the
/// bottom and a value axis on the right that crosses at the maximum
/// (`c:crosses max`, carried the way an imported Word axis carries its
/// `c:crosses autoZero`) — with ids no axis holds.
///
/// O(axes).
fn ensure_secondary_axes(next: &mut Chart, primary: &[u32]) -> Result<Vec<u32>, String> {
    let axes = &next.plot_area.axes;
    let found = axes.iter().find_map(|axis| {
        let secondary_value = axis.kind == AxisKind::Value
            && axis.position == Some(AxisPosition::Right)
            && !primary.contains(&axis.id);
        let partner = axis.cross_axis_id?;
        (secondary_value && axes.iter().any(|other| other.id == partner))
            .then_some(vec![partner, axis.id])
    });
    if let Some(ids) = found {
        return Ok(ids);
    }
    if axes.len() + 2 > MAX_CHART_AXES {
        return Err(refusal::marked(
            "chart.too-many-axes",
            &format!(
                "A chart can hold at most {MAX_CHART_AXES} axes, so it has no room for a secondary pair."
            ),
        ));
    }
    let top = axes.iter().map(|axis| axis.id).max().unwrap_or(0);
    let (category, value) = match (top.checked_add(1), top.checked_add(2)) {
        (Some(category), Some(value)) => (category, value),
        // Ids at the top of the range: take the lowest two free ones instead.
        _ => {
            let mut free = (1..).filter(|id| !axes.iter().any(|axis| axis.id == *id));
            (free.next().unwrap_or(1), free.next().unwrap_or(2))
        }
    };
    next.plot_area.axes.push(Axis {
        id: category,
        kind: AxisKind::Category,
        position: Some(AxisPosition::Bottom),
        deleted: true,
        cross_axis_id: Some(value),
        ..Axis::default()
    });
    next.plot_area.axes.push(Axis {
        id: value,
        kind: AxisKind::Value,
        position: Some(AxisPosition::Right),
        cross_axis_id: Some(category),
        retained: vec![ChartXml {
            name: "crosses".to_owned(),
            xml: r#"<c:crosses val="max"/>"#.to_owned(),
        }],
        ..Axis::default()
    });
    Ok(vec![category, value])
}

/// Removes the axes `before` plotted against that no group of `next` does —
/// the secondary pair once no series is on it. An axis no group named BEFORE
/// the edit is left alone: it is the producer's, not this edit's to drop.
/// O(groups × ids + axes).
pub(crate) fn prune_orphaned_axes(before: &Chart, next: &mut Chart) {
    let named = |chart: &Chart| {
        chart
            .plot_area
            .groups
            .iter()
            .flat_map(|group| group.axis_ids.iter().copied())
            .collect::<std::collections::BTreeSet<u32>>()
    };
    let was = named(before);
    let now = named(next);
    next.plot_area
        .axes
        .retain(|axis| !was.contains(&axis.id) || now.contains(&axis.id));
}

/// A series moved into `kind`'s family keeps only what that family's series
/// admits: carried fragments, trendlines, error bars and a label position.
/// O(fragments).
fn fit_series_to(series: &mut Series, kind: ChartGroupKind) {
    series.retained = keep_carried(&series.retained, ChartContainer::Series(kind), &[]);
    if !admits_trendline(kind) {
        series.trendlines.clear();
    }
    if !admits_error_bars(kind) {
        series.error_bars.clear();
    }
    if let Some(labels) = series.data_labels.as_mut()
        && let Some(position) = labels.position
        && !super::label_positions(kind).contains(&super::label_position_token(Some(position)))
    {
        labels.position = None;
    }
}

/// Regroups the plot area so each series sits in the group its `(type,
/// secondary)` names, groups in order of first appearance in plot order.
///
/// A group that already exists under the same key keeps its own family
/// settings (gap width, overlap) and carried formatting; a new one takes the
/// gallery's defaults. Secondary groups plot against the secondary pair
/// ([`ensure_secondary_axes`]); the pair is removed once nothing plots on it.
///
/// Complexity: O(series × groups + axes).
fn regroup(next: &mut Chart, targets: &[(&'static str, bool)]) -> Result<(), String> {
    let before = next.clone();
    let order = plot_order(next);
    let primary = primary_axis_ids(next);
    let secondary_ids = if targets.iter().any(|(_, secondary)| *secondary) {
        ensure_secondary_axes(next, &primary)?
    } else {
        Vec::new()
    };
    let old = std::mem::take(&mut next.plot_area.groups);
    let mut keys: Vec<(&'static str, bool)> = Vec::new();
    let mut groups: Vec<ChartGroup> = Vec::new();
    for (&(group, index), &key) in order.iter().zip(targets) {
        let at = if let Some(at) = keys.iter().position(|known| *known == key) {
            at
        } else {
            let reuse = old
                .iter()
                .find(|g| chart_kind_token(g.kind) == key.0 && is_secondary(g, &primary) == key.1);
            let kind = match reuse {
                Some(g) => g.kind,
                None => {
                    chart_group_for_kind(key.0).ok_or_else(|| unpainted_chart_refusal(key.0))?
                }
            };
            groups.push(ChartGroup {
                kind,
                series: Vec::new(),
                axis_ids: if key.1 {
                    secondary_ids.clone()
                } else {
                    primary.clone()
                },
                vary_colors: reuse.is_some_and(|g| g.vary_colors),
                retained: reuse.map_or_else(Vec::new, |g| {
                    keep_carried(&g.retained, ChartContainer::Group(kind), &[])
                }),
            });
            keys.push(key);
            groups.len() - 1
        };
        let mut series = old[group].series[index].clone();
        if old[group].kind != groups[at].kind {
            fit_series_to(&mut series, groups[at].kind);
        }
        groups[at].series.push(series);
    }
    if groups.len() > MAX_CHART_GROUPS {
        return Err(refusal::marked(
            "chart.too-many-groups",
            &format!("A chart can combine at most {MAX_CHART_GROUPS} chart types."),
        ));
    }
    next.vary_colors = false;
    next.plot_area.groups = groups;
    prune_orphaned_axes(&before, next);
    Ok(())
}

/// The trendline a patch describes for `series`, or `None` for `none`.
/// O(points).
fn trendline_for(
    series: &Series,
    patch: &TrendlinePatch,
    name: &str,
) -> Result<Option<Trendline>, String> {
    let kind = match patch.kind.as_str() {
        "none" => return Ok(None),
        "linear" => TrendlineKind::Linear,
        "exp" => TrendlineKind::Exponential,
        "log" => TrendlineKind::Logarithmic,
        "poly" => TrendlineKind::Polynomial,
        "power" => TrendlineKind::Power,
        "movingAvg" => TrendlineKind::MovingAverage,
        other => {
            return Err(refusal::marked(
                "chart.trendline-unknown",
                &format!(
                    "A trendline can be linear, exponential, logarithmic, polynomial, power \
                     or a moving average; there is no {other} trendline."
                ),
            ));
        }
    };
    // The previous line's own formatting (its colour, name, label) is kept.
    let mut line = series.trendlines.first().cloned().unwrap_or_default();
    line.kind = kind;
    line.order = None;
    line.period = None;
    if kind == TrendlineKind::Polynomial {
        let order = patch.order.unwrap_or(2);
        if !(2..=6).contains(&order) {
            return Err(refusal::marked(
                "chart.trendline-order",
                &format!("A polynomial trendline's order is 2 to 6; {order} is outside that."),
            ));
        }
        line.order = Some(order);
    }
    if kind == TrendlineKind::MovingAverage {
        let points = u32::try_from(row_count(std::iter::once(series))).unwrap_or(u32::MAX);
        let ceiling = points.min(255);
        let period = patch.period.unwrap_or(2);
        if ceiling < 2 || !(2..=ceiling).contains(&period) {
            return Err(refusal::marked(
                "chart.trendline-period",
                &format!(
                    "A moving average of “{name}” can span 2 to {ceiling} points; {period} is \
                     outside that."
                ),
            ));
        }
        line.period = Some(period);
        // Word offers neither an equation nor R² for a moving average: it is
        // not a fitted function.
        line.display_equation = false;
        line.display_r_squared = false;
    } else {
        if let Some(equation) = patch.equation {
            line.display_equation = equation;
        }
        if let Some(r_squared) = patch.r_squared {
            line.display_r_squared = r_squared;
        }
    }
    Ok(Some(line))
}

/// The value error bars a patch describes for `series`, or `None` for `none`.
/// O(1).
fn error_bars_for(
    series: &Series,
    patch: &ErrorBarsPatch,
    scatter: bool,
) -> Result<Option<ErrorBars>, String> {
    let (value_type, default) = match patch.kind.as_str() {
        "none" => return Ok(None),
        // Excel's and Word's defaults for each amount.
        "fixedVal" => (ErrorValueType::FixedValue, Some("1")),
        "percentage" => (ErrorValueType::Percentage, Some("5")),
        "stdDev" => (ErrorValueType::StandardDeviation, Some("1")),
        "stdErr" => (ErrorValueType::StandardError, None),
        "cust" => {
            return Err(refusal::marked(
                "chart.error-bars-custom",
                "Custom error bars take a value per point from a worksheet range, which \
                 this editor cannot set yet; pick a fixed value, a percentage, a standard \
                 deviation or the standard error.",
            ));
        }
        other => {
            return Err(refusal::marked(
                "chart.error-bars-unknown",
                &format!("There are no {other} error bars."),
            ));
        }
    };
    let bar_type = match patch.bar_type.as_deref() {
        None | Some("both") => ErrorBarType::Both,
        Some("plus") => ErrorBarType::Plus,
        Some("minus") => ErrorBarType::Minus,
        Some(other) => {
            return Err(refusal::marked(
                "chart.error-bars-unknown",
                &format!("Error bars can show both directions, plus or minus; not {other}."),
            ));
        }
    };
    let value = match default {
        None => None,
        Some(default) => {
            let text = patch
                .value
                .as_deref()
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .unwrap_or(default);
            let parsed = ChartValue::Number(text.to_owned()).as_f64();
            if text.len() > MAX_CHART_NUMBER_BYTES || !parsed.is_some_and(|value| value >= 0.0) {
                return Err(refusal::marked(
                    "chart.error-bars-value",
                    &format!("An error amount is a number of zero or more; “{text}” is not."),
                ));
            }
            Some(text.to_owned())
        }
    };
    let mut bars = value_bars(series).cloned().unwrap_or_default();
    bars.direction = scatter.then_some(ErrorBarDirection::Y);
    bars.value_type = value_type;
    bars.bar_type = bar_type;
    bars.value = value;
    bars.plus = None;
    bars.minus = None;
    Ok(Some(bars))
}

/// Applies one series' formatting, with `kind` the family it will be plotted
/// in once regrouped. O(points + fragments).
fn format_series(
    series: &mut Series,
    kind: ChartGroupKind,
    patch: &SeriesFormatPatch,
) -> Result<(), String> {
    let name = series.name.as_ref().map_or_else(
        || format!("series {}", patch.index + 1),
        |name| name.text.clone(),
    );
    let line_family = has_line(kind);
    if (patch.line_width.is_some() || patch.dash.is_some()) && !line_family {
        return Err(refusal::marked(
            "chart.no-line",
            &format!(
                "A {} series has no series line, so it has no line width or dash.",
                chart_kind_token(kind)
            ),
        ));
    }
    let mut shape_changed = false;
    if let Some(text) = &patch.color {
        let color = parse_color(text, &format!("“{name}”"))?;
        series.fill = color;
        if line_family || color.is_none() {
            let mut line = series.line.unwrap_or_default();
            line.color = if line_family { color } else { None };
            if color.is_some() {
                line.no_fill = false;
            }
            series.line = (line != ChartLine::default()).then_some(line);
        }
        shape_changed = true;
    }
    if let Some(points) = patch.line_width {
        if !points.is_finite() || !(0.25..=20.0).contains(&points) {
            return Err(refusal::marked(
                "chart.line-width",
                &format!("A series line can be 0.25 to 20 points wide; {points} is outside that."),
            ));
        }
        let mut line = series.line.unwrap_or_default();
        // In range, so the cast is exact.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let emu = (points * EMU_PER_POINT).round() as u32;
        line.width_emu = Some(emu);
        line.no_fill = false;
        series.line = Some(line);
        shape_changed = true;
    }
    if let Some(token) = &patch.dash {
        let dash = dash_value(token).ok_or_else(|| {
            refusal::marked(
                "chart.dash-unknown",
                &format!("There is no {token} dash type."),
            )
        })?;
        let mut line = series.line.unwrap_or_default();
        line.dash = Some(dash);
        line.no_fill = false;
        series.line = Some(line);
        shape_changed = true;
    }
    if shape_changed {
        drop_carried(&mut series.retained, &["spPr"]);
    }
    if let Some(format) = &patch.number_format {
        series.values.number_format =
            number_format_value(format, &format!("the values of “{name}”"))?;
    }
    if let Some(trendline) = &patch.trendline {
        if trendline.kind != "none" && !admits_trendline(kind) {
            return Err(refusal::marked(
                "chart.no-trendline",
                &format!(
                    "A {} chart cannot show a trendline: the fit would run through stacked \
                     totals or slices, not through “{name}”.",
                    chart_kind_token(kind)
                ),
            ));
        }
        series.trendlines = trendline_for(series, trendline, &name)?
            .into_iter()
            .collect();
        drop_carried(&mut series.retained, &["trendline"]);
    }
    if let Some(bars) = &patch.error_bars {
        if bars.kind != "none" && !admits_error_bars(kind) {
            return Err(refusal::marked(
                "chart.no-error-bars",
                &format!("A {} chart cannot show error bars.", chart_kind_token(kind)),
            ));
        }
        let scatter = matches!(kind, ChartGroupKind::Scatter { .. });
        let replacement = error_bars_for(series, bars, scatter)?;
        // The value bars are replaced; a scatter series' x bars are not this
        // control's and stay.
        series
            .error_bars
            .retain(|bars| bars.direction == Some(ErrorBarDirection::X));
        series.error_bars.extend(replacement);
        drop_carried(&mut series.retained, &["errBars"]);
    }
    Ok(())
}

/// Applies the per-series changes: formatting first (checked against the
/// family each series will be plotted in), then — when any series changes
/// type or axis — the regrouping that makes a combination chart.
///
/// Complexity: O(series × (log series + groups) + axes + fragments).
pub(crate) fn apply_series(next: &mut Chart, patches: &[SeriesFormatPatch]) -> Result<(), String> {
    let order = plot_order(next);
    let primary = primary_axis_ids(next);
    let current: Vec<(&'static str, bool)> = order
        .iter()
        .map(|&(group, _)| {
            let g = &next.plot_area.groups[group];
            (chart_kind_token(g.kind), is_secondary(g, &primary))
        })
        .collect();
    let mut targets = current.clone();
    for patch in patches {
        let Some(target) = targets.get_mut(patch.index) else {
            return Err(refusal::marked(
                "chart.no-series",
                &format!(
                    "This chart has {} series, so there is no series {}.",
                    order.len(),
                    patch.index + 1
                ),
            ));
        };
        if let Some(kind) = &patch.kind {
            target.0 = gallery_token(kind)?;
        }
        if let Some(secondary) = patch.secondary {
            target.1 = secondary;
        }
    }
    let mut keys: Vec<(&'static str, bool)> = Vec::new();
    for key in &targets {
        if !keys.contains(key) {
            keys.push(*key);
        }
    }
    let combined = keys.len() > 1 || targets.iter().any(|(_, secondary)| *secondary);
    for ((token, _), (was, _)) in targets.iter().zip(&current) {
        // A series changes type only between combinable families (both ends
        // plot on the same category/value pair); a whole-chart change of
        // family is the chart type control's, which moves the axes with it.
        let allowed = if token == was {
            !combined || COMBINABLE.contains(token)
        } else {
            COMBINABLE.contains(token) && COMBINABLE.contains(was)
        };
        if !allowed {
            return Err(refusal::marked(
                "chart.not-combinable",
                &format!(
                    "A {token} series cannot share a chart with another chart type or a \
                     secondary axis. Combination charts mix column, line and area series."
                ),
            ));
        }
    }
    if !targets.is_empty() && targets.iter().all(|(_, secondary)| *secondary) {
        return Err(refusal::marked(
            "chart.all-secondary",
            "At least one series has to stay on the primary axis.",
        ));
    }
    for patch in patches {
        let (group, index) = order[patch.index];
        let target = targets[patch.index].0;
        let kind = if target == chart_kind_token(next.plot_area.groups[group].kind) {
            next.plot_area.groups[group].kind
        } else {
            chart_group_for_kind(target).ok_or_else(|| unpainted_chart_refusal(target))?
        };
        format_series(&mut next.plot_area.groups[group].series[index], kind, patch)?;
    }
    if targets != current {
        regroup(next, &targets)?;
    }
    Ok(())
}
