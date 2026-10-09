// SPDX-License-Identifier: Apache-2.0

//! Chart composition — turning a typed [`Chart`] projection into the box-local
//! paint primitives a chart box carries (`docs/155` §9 increment 5, ADR-050
//! part 4, `docs/105` FID-R-08).
//!
//! # What this is
//!
//! `casual_doc_model::v1::Chart` is a **read projection of a retained part**
//! (`docs/155` §6.1): the chart's XML is byte-preserved and re-emitted verbatim,
//! and this module never writes anything back. It reads the projection's cached
//! data tables — the cache *is* the data, which is what Word and ONLYOFFICE's
//! document editor both draw from (`docs/155` §3.2) — and produces a list of
//! [`ChartPrimitive`]s positioned relative to the chart box's own top-left
//! corner. Composition translates them into page space, exactly as it does for an
//! inline text box.
//!
//! Nothing here opens the embedded workbook, parses a `c:f` formula, or resolves
//! a cell reference. `docs/155` §5.2 is the line that stops a document editor
//! growing a spreadsheet engine.
//!
//! # What draws
//!
//! `docs/155` §7.3 split delivery by primitive rather than by family. Bar,
//! column, line (including `c:smooth`), area and scatter (tier 1A) use the
//! rectangle, straight-line and polyline primitives the display list has. Pie
//! and doughnut (tier 1B) needed an arc between two radii; they draw through the
//! shared sector geometry in [`crate::arc`] (`docs/155` §7.4) rather than a
//! many-sided polygon fan, which `docs/119` §6 named the wrong axis.
//! [`is_drawable`] is the single place that decides, and a chart with no drawable
//! group yields no primitives at all — so the caller keeps its reported
//! placeholder rather than painting an empty frame.
//!
//! A smooth series *is* drawn, as a sampled polyline. That is not an
//! approximation standing in for a curve: `c:smooth` is a function evaluated at
//! points, and ten straight segments per interval is what the shipping market
//! leader draws (`docs/155` §3.3 — their true-Bézier variants are switched off
//! behind `//TODO … draws incorrectly`).
//!
//! # Complexity
//!
//! [`compose_chart`] is O(points in the chart + labels shaped), and the point
//! count is bounded by `MAX_CHART_DATA_POINTS` per range. It is **per visible
//! chart** work and is independent of document size, so it does not enter the
//! O(1)-per-interaction budget (`docs/107` §4, `SKILL` §8). Every by-id lookup is
//! hoisted out of its loop: a group's axes are resolved once per group, never per
//! point.

use casual_doc_model::v1::{
    Axis, AxisKind, AxisOrientation, AxisPosition, BarDirection, BarGrouping, Chart, ChartGroup,
    ChartGroupKind, ChartValue, Color, DataLabelPosition, DataRange, DisplayBlanks, Grouping,
    LegendPosition, ScatterStyle, Series, TickLabelPosition, TickMark,
};

// Own line (anti-conflict): the shared arc/sector geometry (`docs/155` §7.4).
use crate::arc::{FULL_TURN, sector};
use crate::text::{ChartPrimitive, ChartStroke, GlyphRun};
use crate::units::{Point, Rect, Size, Twip};

/// Padding (twips) between the chart-space border and its contents — ~6 pt.
const PADDING: Twip = Twip(120);

/// Gap (twips) between two stacked bands of chart furniture — ~3 pt.
const BAND_GAP: Twip = Twip(60);

/// Hairline width (twips) for the chart-space border, gridlines, axis lines and
/// tick marks — Word's ~0.5 pt, the same weight [`crate::compose`] uses for a
/// bar-tab rule and a column separator.
const HAIRLINE: Twip = Twip(10);

/// Width (twips) of a series line — ~1.5 pt, Word's default for a line chart.
const SERIES_LINE_WIDTH: Twip = Twip(30);

/// Diameter (twips) of a series marker — ~5 pt, Word's default marker size.
const MARKER_SIZE: Twip = Twip(100);

/// Length (twips) of a major tick mark.
const TICK_LENGTH: Twip = Twip(60);

/// Gap (twips) between a tick label and the axis it labels.
const LABEL_GAP: Twip = Twip(40);

/// Side (twips) of a legend key swatch.
const LEGEND_KEY: Twip = Twip(140);

/// Gap (twips) between a legend key and its label, and between legend entries.
const LEGEND_GAP: Twip = Twip(80);

/// Target number of major intervals on a value axis. Word's own auto-scaling
/// aims for a comparable count; the exact figure only has to be stable.
const TARGET_INTERVALS: f64 = 5.0;

/// Straight segments a smoothed interval is sampled into (`docs/155` §3.3 — the
/// count ONLYOFFICE's shipping `calculateSplineLine` uses).
const SMOOTH_SAMPLES: usize = 10;

/// The chart-space background.
const BACKGROUND: [u8; 4] = [255, 255, 255, 255];

/// The chart-space border, gridline and axis-line colour — Word's light grey
/// chart furniture.
const FURNITURE: [u8; 4] = [191, 191, 191, 255];

/// The axis-line colour, a shade darker than a gridline so the axis reads as the
/// frame and the gridlines as the grid.
const AXIS_LINE: [u8; 4] = [137, 137, 137, 255];

/// A shaped single-line chart label: the glyph runs with origins relative to the
/// label's own left edge on its baseline, plus the metrics needed to place it.
///
/// Produced by the caller, because shaping needs the document's font resolver and
/// style cascade and this module deliberately depends on neither.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChartLabel {
    /// The shaped runs, origins relative to the label's left edge on its
    /// baseline.
    pub runs: Vec<GlyphRun>,
    /// The label's total advance width.
    pub width: Twip,
    /// Height above the baseline.
    pub ascent: Twip,
    /// Depth below the baseline.
    pub descent: Twip,
}

impl ChartLabel {
    /// The label's full line height.
    #[must_use]
    pub fn height(&self) -> Twip {
        self.ascent + self.descent
    }
}

/// The colours a chart resolves its series and text against.
///
/// Series colours come from the **document theme** (`docs/155` §12 Q-D): a chart
/// carries its own `colors1.xml` palette, but that part is out of scope and
/// preserved, and the theme already exists in the model. A series with an
/// explicit solid `c:spPr` fill overrides the accent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChartStyle {
    /// `a:accent1`..`a:accent6`, in order — the default series colour cycle.
    pub accents: [[u8; 4]; 6],
    /// The colour chart text (title, tick labels, legend, data labels) is painted
    /// in.
    pub text: [u8; 4],
}

impl Default for ChartStyle {
    /// Word's default Office theme accents and body text colour, for a document
    /// that declares no `a:clrScheme`.
    fn default() -> Self {
        Self {
            accents: [
                [0x44, 0x72, 0xC4, 0xFF],
                [0xED, 0x7D, 0x31, 0xFF],
                [0xA5, 0xA5, 0xA5, 0xFF],
                [0xFF, 0xC0, 0x00, 0xFF],
                [0x5B, 0x9B, 0xD5, 0xFF],
                [0x70, 0xAD, 0x47, 0xFF],
            ],
            text: [0x59, 0x59, 0x59, 0xFF],
        }
    }
}

/// Resolves an authored chart colour to RGBA without a document theme: an explicit
/// RGB is itself, and anything theme-dependent is black — the same answer the
/// document's own resolver gives when no `a:clrScheme` is declared.
///
/// A caller with a theme passes its own resolver to [`compose_chart`] instead, so
/// this module holds no second copy of the `w:themeColor` rule (`SKILL` §8:
/// prefer one mechanism over two).
#[must_use]
pub fn resolve_literal_color(color: Color) -> [u8; 4] {
    match color {
        Color::Rgb(rgb) => [rgb.r, rgb.g, rgb.b, 255],
        Color::Auto | Color::Theme(_) => [0, 0, 0, 255],
    }
}

/// Everything the drawers need to colour a series: the document's style, the
/// per-series default accent cycle, and the authored-colour resolver.
///
/// One borrow struct rather than three parameters threaded through every drawer —
/// which is also what keeps them under clippy's argument ceiling.
struct Paint<'a> {
    style: &'a ChartStyle,
    palette: &'a [[u8; 4]],
    colors: &'a dyn Fn(Color) -> [u8; 4],
}

impl Paint<'_> {
    /// A series' fill: its explicit solid `c:spPr` fill, else the accent for its
    /// position in the chart.
    fn series(&self, series: &Series, index: usize) -> [u8; 4] {
        series.fill.map_or_else(
            || self.palette[index % self.palette.len().max(1)],
            self.colors,
        )
    }

    /// One pie or doughnut **point's** fill.
    ///
    /// Pie and doughnut colour by point rather than by series — the format's
    /// `c:varyColors` default for these two families, and what Word draws whether
    /// or not the element is written. Indexed off the theme accents directly
    /// rather than off `palette`, which is sized by series count and would be
    /// length 1 for the single-series pie that is the common case.
    ///
    /// The model carries no `c:dPt` per-point override, so an authored
    /// point-specific fill is not honoured yet; it is retained in the part and
    /// re-emitted verbatim.
    fn point(&self, index: usize) -> [u8; 4] {
        self.style.accents[index % self.style.accents.len()]
    }

    /// A series' line colour: its explicit `a:ln` solid fill, else its fill.
    fn line(&self, series: &Series, index: usize) -> [u8; 4] {
        series
            .line
            .and_then(|line| line.color)
            .map_or_else(|| self.series(series, index), self.colors)
    }
}

/// Whether a chart group's family is drawn by this module.
///
/// The one place the drawable boundary is decided. **Every tier-1 family is now
/// drawn**: tier 1A's bar, column, line (including `c:smooth`), area and scatter,
/// and tier 1B's pie and doughnut, which needed the arc that
/// [`crate::arc::sector`] builds from the path primitive the shapes lane landed.
/// The families `docs/155` §4.3 puts out of scope never reach a
/// [`ChartGroupKind`] at all, so there is nothing for this to refuse — it is
/// kept, exhaustive and total, because the next family added to the model must
/// make a deliberate choice here rather than defaulting into the drawers.
///
/// O(1).
#[must_use]
pub fn is_drawable(kind: ChartGroupKind) -> bool {
    match kind {
        ChartGroupKind::Bar { .. }
        | ChartGroupKind::Line { .. }
        | ChartGroupKind::Area { .. }
        | ChartGroupKind::Scatter { .. }
        | ChartGroupKind::Pie { .. }
        | ChartGroupKind::Doughnut { .. } => true,
    }
}

/// Whether a family colours by **point** rather than by series.
///
/// `c:varyColors` is the explicit element, but pie and doughnut behave as though
/// it were set whether or not a producer writes it — a single-series pie with one
/// colour would be a solid disc. One predicate so the fills, the legend keys and
/// anything added later cannot disagree about which they key off.
///
/// O(1).
fn colors_by_point(kind: ChartGroupKind) -> bool {
    matches!(
        kind,
        ChartGroupKind::Pie { .. } | ChartGroupKind::Doughnut { .. }
    )
}

/// Whether any of `chart`'s groups can be drawn at all.
///
/// The caller's gate: `false` means keep the placeholder, because a frame with
/// nothing in it is less honest than a labelled placeholder (`docs/155` §6.2 —
/// the omitted-and-preserved path).
///
/// Complexity: O(groups), which is bounded by `MAX_CHART_GROUPS`.
#[must_use]
pub fn has_drawable_content(chart: &Chart) -> bool {
    chart
        .plot_area
        .groups
        .iter()
        .any(|group| is_drawable(group.kind) && !group.series.is_empty())
}

/// Composes one chart projection into box-local paint primitives.
///
/// `size` is the box the chart is drawn into — the authored `wp:extent`, which is
/// now reserved rather than ignored. `shape` shapes one short label; it may
/// decline (returning `None`), in which case that label is simply not painted and
/// the geometry it would have reserved is not reserved either, so a shaper that
/// cannot shape text yields a chart with no labels rather than a misaligned one.
///
/// Returns an empty list when nothing is drawable, which is the signal to keep
/// the placeholder — see [`has_drawable_content`].
///
/// Complexity: O(points in the chart + labels shaped). Per visible chart, not per
/// document.
#[must_use]
pub fn compose_chart(
    chart: &Chart,
    size: Size,
    style: &ChartStyle,
    colors: &dyn Fn(Color) -> [u8; 4],
    shape: &mut dyn FnMut(&str) -> Option<ChartLabel>,
) -> Vec<ChartPrimitive> {
    if size.width.raw() <= 0 || size.height.raw() <= 0 || !has_drawable_content(chart) {
        return Vec::new();
    }

    let mut out = Vec::new();
    let full = Rect::new(Point::new(Twip::ZERO, Twip::ZERO), size);
    out.push(ChartPrimitive::Rect {
        rect: full,
        fill: Some(BACKGROUND),
        stroke: Some(ChartStroke {
            color: FURNITURE,
            width: HAIRLINE,
        }),
    });

    let mut area = inset(full, PADDING);
    if area.size.width.raw() <= 0 || area.size.height.raw() <= 0 {
        return out;
    }

    // Series colours are assigned in document order across the WHOLE chart, so a
    // combo's line group continues the palette rather than restarting it.
    let accents = series_colors(chart, style);
    let paint = Paint {
        style,
        palette: &accents,
        colors,
    };

    area = place_title(chart, area, style, shape, &mut out);
    area = place_legend(chart, area, &paint, shape, &mut out);

    let plot = reserve_axis_gutters(chart, area, style, shape);
    if plot.size.width.raw() <= 0 || plot.size.height.raw() <= 0 {
        return out;
    }

    let categories = category_count(chart);
    let mut color = 0usize;
    // Gridlines first (behind the series), then the series, then the axes and
    // their labels on top — Word's own paint order.
    draw_gridlines(chart, plot, categories, &mut out);
    for group in &chart.plot_area.groups {
        let count = group.series.len();
        if is_drawable(group.kind) {
            draw_group(
                group,
                &GroupGeometry {
                    plot,
                    categories,
                    scale: group_scale(chart, group),
                    first_color: color,
                },
                &paint,
                shape,
                &mut out,
            );
        }
        color += count;
    }
    draw_axes(chart, plot, categories, style, shape, &mut out);
    out
}

/// The derived geometry one chart group is drawn against. A struct rather than
/// six parameters so the drawers stay under clippy's argument ceiling and so the
/// value scale is resolved once per group instead of once per series.
struct GroupGeometry {
    /// The plot rectangle, box-local.
    plot: Rect,
    /// How many category slots the plot is divided into.
    categories: usize,
    /// The group's value axis scale.
    scale: Scale,
    /// The palette index of this group's first series.
    first_color: usize,
}

/// A value axis' resolved numeric range and major interval.
#[derive(Clone, Copy, Debug)]
struct Scale {
    /// The value at the axis origin.
    min: f64,
    /// The value at the axis end.
    max: f64,
    /// The major interval, used for gridlines and tick labels.
    step: f64,
    /// Whether the axis runs `maxMin` (`c:scaling/c:orientation`).
    reversed: bool,
}

impl Scale {
    /// Where `value` falls on the axis, as `0.0` at [`Scale::min`] and `1.0` at
    /// [`Scale::max`], honouring a reversed orientation.
    fn fraction(self, value: f64) -> f64 {
        let span = self.max - self.min;
        if span.abs() < f64::EPSILON {
            return 0.0;
        }
        let raw = (value - self.min) / span;
        if self.reversed { 1.0 - raw } else { raw }
    }

    /// The major tick values, from [`Scale::min`] to [`Scale::max`] inclusive.
    ///
    /// Bounded: a step small enough to produce an unbounded sequence would have
    /// been rejected by [`nice_scale`], and the loop is capped regardless so a
    /// producer-supplied `c:min`/`c:max` cannot make this run long.
    fn ticks(self) -> Vec<f64> {
        let mut ticks = Vec::new();
        if self.step <= 0.0 || !self.step.is_finite() {
            return ticks;
        }
        let count = ((self.max - self.min) / self.step).round() as i64;
        if count <= 0 {
            return ticks;
        }
        for i in 0..=count.min(64) {
            ticks.push(self.min + self.step * i as f64);
        }
        ticks
    }
}

/// Shrinks `rect` by `by` on all four sides.
fn inset(rect: Rect, by: Twip) -> Rect {
    Rect::new(
        Point::new(rect.origin.x + by, rect.origin.y + by),
        Size::new(
            Twip((rect.size.width.raw() - 2 * by.raw()).max(0)),
            Twip((rect.size.height.raw() - 2 * by.raw()).max(0)),
        ),
    )
}

/// Translates `label`'s runs so its left edge sits at `left` on baseline `baseline`,
/// recolours them to the chart's text colour, and appends them.
fn place_label(label: &ChartLabel, left: Twip, baseline: Twip, out: &mut Vec<ChartPrimitive>) {
    for run in &label.runs {
        let mut placed = run.clone();
        placed.origin = Point::new(left + run.origin.x, baseline + run.origin.y);
        out.push(ChartPrimitive::Text { run: placed });
    }
}

/// Places the chart title across the top of `area`, centred, and returns the area
/// below it.
///
/// A title that is overlaid (`c:overlay`) or suppressed (`c:autoTitleDeleted`)
/// reserves nothing, which is what both attributes mean.
fn place_title(
    chart: &Chart,
    area: Rect,
    style: &ChartStyle,
    shape: &mut dyn FnMut(&str) -> Option<ChartLabel>,
    out: &mut Vec<ChartPrimitive>,
) -> Rect {
    if chart.auto_title_deleted {
        return area;
    }
    let Some(title) = chart.title.as_ref() else {
        return area;
    };
    if title.overlay {
        return area;
    }
    let Some(text) = title.text.as_ref().map(|text| text.text.as_str()) else {
        return area;
    };
    let Some(label) = shape_colored(text, style.text, shape) else {
        return area;
    };
    let left = Twip(area.origin.x.raw() + ((area.size.width.raw() - label.width.raw()) / 2).max(0));
    place_label(&label, left, area.origin.y + label.ascent, out);
    let used = label.height() + BAND_GAP;
    Rect::new(
        Point::new(area.origin.x, area.origin.y + used),
        Size::new(
            area.size.width,
            Twip((area.size.height.raw() - used.raw()).max(0)),
        ),
    )
}

/// One legend entry: a series name and the colour its key swatch is filled with.
struct LegendEntry {
    label: ChartLabel,
    color: [u8; 4],
}

/// Places the legend on `area`'s declared edge and returns the area left for the
/// plot.
///
/// Only the drawable groups contribute entries: a legend key for a group this
/// build does not paint would describe nothing on the page.
///
/// **A pie or doughnut legend names CATEGORIES, not series.** That is not a
/// special case bolted on: these two families colour by point, so the legend has
/// to key the same thing the fills key, or every swatch would be the wrong
/// colour. Word and ONLYOFFICE both list categories here.
///
/// Complexity: O(series + categories), with the category labels gathered once for
/// the whole chart rather than per group — `category_labels` walks the series
/// list, so calling it inside the group loop would be the by-id-lookup-in-a-loop
/// shape `SKILL` §8 forbids.
fn place_legend(
    chart: &Chart,
    area: Rect,
    paint: &Paint<'_>,
    shape: &mut dyn FnMut(&str) -> Option<ChartLabel>,
    out: &mut Vec<ChartPrimitive>,
) -> Rect {
    let Some(legend) = chart.legend.as_ref() else {
        return area;
    };
    if legend.overlay {
        return area;
    }
    let mut entries: Vec<LegendEntry> = Vec::new();
    let mut color = 0usize;
    // Hoisted: one walk for the whole chart, not one per group.
    let by_point = chart
        .plot_area
        .groups
        .iter()
        .any(|group| colors_by_point(group.kind) && !group.series.is_empty());
    if by_point {
        for (point, name) in category_labels(chart).into_iter().enumerate() {
            if let Some(label) = shape_colored(&name, paint.style.text, shape) {
                entries.push(LegendEntry {
                    label,
                    color: paint.point(point),
                });
            }
        }
    }
    for group in &chart.plot_area.groups {
        for series in &group.series {
            if is_drawable(group.kind)
                && !colors_by_point(group.kind)
                && let Some(name) = series.name.as_ref().map(|name| name.text.as_str())
                && let Some(label) = shape_colored(name, paint.style.text, shape)
            {
                entries.push(LegendEntry {
                    label,
                    color: paint.series(series, color),
                });
            }
            color += 1;
        }
    }
    if entries.is_empty() {
        return area;
    }

    match legend.position {
        LegendPosition::Bottom | LegendPosition::Top => {
            place_horizontal_legend(&entries, area, legend.position, out)
        }
        LegendPosition::Left | LegendPosition::Right | LegendPosition::TopRight => {
            place_vertical_legend(&entries, area, legend.position, out)
        }
    }
}

/// A single-row legend along the top or bottom edge, centred.
fn place_horizontal_legend(
    entries: &[LegendEntry],
    area: Rect,
    position: LegendPosition,
    out: &mut Vec<ChartPrimitive>,
) -> Rect {
    let height = entries
        .iter()
        .map(|entry| entry.label.height().max(LEGEND_KEY))
        .max()
        .unwrap_or(LEGEND_KEY);
    let total: i32 = entries
        .iter()
        .map(|entry| LEGEND_KEY.raw() + LEGEND_GAP.raw() / 2 + entry.label.width.raw())
        .sum::<i32>()
        + LEGEND_GAP.raw() * (entries.len().saturating_sub(1) as i32);
    let band_top = if position == LegendPosition::Top {
        area.origin.y
    } else {
        Twip(area.bottom().raw() - height.raw())
    };
    let mut x = Twip(area.origin.x.raw() + ((area.size.width.raw() - total) / 2).max(0));
    for entry in entries {
        draw_legend_entry(entry, Point::new(x, band_top), height, out);
        x = Twip(x.raw() + LEGEND_KEY.raw() + LEGEND_GAP.raw() / 2 + entry.label.width.raw())
            + LEGEND_GAP;
    }
    let used = height + BAND_GAP;
    let remaining = Twip((area.size.height.raw() - used.raw()).max(0));
    let top = if position == LegendPosition::Top {
        area.origin.y + used
    } else {
        area.origin.y
    };
    Rect::new(
        Point::new(area.origin.x, top),
        Size::new(area.size.width, remaining),
    )
}

/// A stacked legend along the left or right edge.
fn place_vertical_legend(
    entries: &[LegendEntry],
    area: Rect,
    position: LegendPosition,
    out: &mut Vec<ChartPrimitive>,
) -> Rect {
    let row = entries
        .iter()
        .map(|entry| entry.label.height().max(LEGEND_KEY))
        .max()
        .unwrap_or(LEGEND_KEY);
    let width = entries
        .iter()
        .map(|entry| Twip(LEGEND_KEY.raw() + LEGEND_GAP.raw() / 2 + entry.label.width.raw()))
        .max()
        .unwrap_or(LEGEND_KEY);
    let on_left = position == LegendPosition::Left;
    let band_left = if on_left {
        area.origin.x
    } else {
        Twip(area.right().raw() - width.raw())
    };
    let stack = Twip(row.raw() * entries.len() as i32);
    let mut y = Twip(area.origin.y.raw() + ((area.size.height.raw() - stack.raw()) / 2).max(0));
    for entry in entries {
        draw_legend_entry(entry, Point::new(band_left, y), row, out);
        y = y + row;
    }
    let used = width + BAND_GAP;
    let left = if on_left {
        area.origin.x + used
    } else {
        area.origin.x
    };
    Rect::new(
        Point::new(left, area.origin.y),
        Size::new(
            Twip((area.size.width.raw() - used.raw()).max(0)),
            area.size.height,
        ),
    )
}

/// A key swatch and its label, vertically centred in a `row`-tall slot.
fn draw_legend_entry(entry: &LegendEntry, origin: Point, row: Twip, out: &mut Vec<ChartPrimitive>) {
    let key_top = Twip(origin.y.raw() + ((row.raw() - LEGEND_KEY.raw()) / 2).max(0));
    out.push(ChartPrimitive::Rect {
        rect: Rect::new(
            Point::new(origin.x, key_top),
            Size::new(LEGEND_KEY, LEGEND_KEY),
        ),
        fill: Some(entry.color),
        stroke: None,
    });
    let baseline = Twip(origin.y.raw() + ((row.raw() - entry.label.height().raw()) / 2).max(0))
        + entry.label.ascent;
    place_label(
        &entry.label,
        Twip(origin.x.raw() + LEGEND_KEY.raw() + LEGEND_GAP.raw() / 2),
        baseline,
        out,
    );
}

/// Shrinks `area` by the gutters the axes' tick labels and tick marks need, and
/// returns the plot rectangle.
///
/// Measures the widest value-axis label on each vertical edge and the tallest
/// category label along the bottom, so a secondary axis on the right reserves its
/// own gutter and the plot does not overrun it.
fn reserve_axis_gutters(
    chart: &Chart,
    area: Rect,
    style: &ChartStyle,
    shape: &mut dyn FnMut(&str) -> Option<ChartLabel>,
) -> Rect {
    let mut left = Twip::ZERO;
    let mut right = Twip::ZERO;
    let mut bottom = Twip::ZERO;
    let mut top = Twip::ZERO;

    for axis in &chart.plot_area.axes {
        if axis.deleted || axis.tick_label_position == TickLabelPosition::None {
            continue;
        }
        let position = axis_position(axis);
        let extent = match axis.kind {
            AxisKind::Value => {
                let scale = axis_scale(chart, axis);
                scale
                    .ticks()
                    .iter()
                    .filter_map(|value| shape_colored(&format_value(*value), style.text, shape))
                    .map(|label| label.width)
                    .max()
                    .unwrap_or(Twip::ZERO)
            }
            AxisKind::Category | AxisKind::Date => category_labels(chart)
                .iter()
                .filter_map(|text| shape_colored(text, style.text, shape))
                .map(|label| label.height())
                .max()
                .unwrap_or(Twip::ZERO),
        };
        if extent.raw() <= 0 {
            continue;
        }
        let gutter = extent + LABEL_GAP + TICK_LENGTH;
        match position {
            AxisPosition::Left => left = left.max(gutter),
            AxisPosition::Right => right = right.max(gutter),
            AxisPosition::Bottom => bottom = bottom.max(gutter),
            AxisPosition::Top => top = top.max(gutter),
        }
    }

    Rect::new(
        Point::new(area.origin.x + left, area.origin.y + top),
        Size::new(
            Twip((area.size.width.raw() - left.raw() - right.raw()).max(0)),
            Twip((area.size.height.raw() - top.raw() - bottom.raw()).max(0)),
        ),
    )
}

/// An axis' drawing edge: its declared `c:axPos`, else the conventional edge for
/// its kind (a category axis along the bottom, a value axis up the left).
fn axis_position(axis: &Axis) -> AxisPosition {
    axis.position.unwrap_or(match axis.kind {
        AxisKind::Value => AxisPosition::Left,
        AxisKind::Category | AxisKind::Date => AxisPosition::Bottom,
    })
}

/// Paints the major gridlines of every non-deleted axis that declares them.
///
/// A value axis draws one line per major tick. A category axis draws one at
/// each category boundary, both outer edges included — Word's vertical
/// gridlines on a column chart, its horizontal ones on a bar chart.
///
/// O(ticks + categories).
fn draw_gridlines(chart: &Chart, plot: Rect, categories: usize, out: &mut Vec<ChartPrimitive>) {
    let stroke = ChartStroke {
        color: FURNITURE,
        width: HAIRLINE,
    };
    for axis in &chart.plot_area.axes {
        if axis.deleted || !axis.major_gridlines {
            continue;
        }
        let vertical_axis = matches!(
            axis_position(axis),
            AxisPosition::Left | AxisPosition::Right
        );
        let fractions: Vec<f64> = match axis.kind {
            AxisKind::Value => {
                let scale = axis_scale(chart, axis);
                scale
                    .ticks()
                    .into_iter()
                    .map(|value| scale.fraction(value))
                    .collect()
            }
            AxisKind::Category | AxisKind::Date => {
                if categories == 0 {
                    continue;
                }
                (0..=categories)
                    .map(|edge| edge as f64 / categories as f64)
                    .collect()
            }
        };
        for fraction in fractions {
            let (from, to) = if vertical_axis {
                let y = along(plot.bottom(), plot.origin.y, fraction);
                (Point::new(plot.origin.x, y), Point::new(plot.right(), y))
            } else {
                let x = along(plot.origin.x, plot.right(), fraction);
                (Point::new(x, plot.origin.y), Point::new(x, plot.bottom()))
            };
            out.push(ChartPrimitive::Line { from, to, stroke });
        }
    }
}

/// Paints every non-deleted axis: its line along the plot edge, its major tick
/// marks, and its tick labels.
fn draw_axes(
    chart: &Chart,
    plot: Rect,
    categories: usize,
    style: &ChartStyle,
    shape: &mut dyn FnMut(&str) -> Option<ChartLabel>,
    out: &mut Vec<ChartPrimitive>,
) {
    let stroke = ChartStroke {
        color: AXIS_LINE,
        width: HAIRLINE,
    };
    for axis in &chart.plot_area.axes {
        if axis.deleted {
            continue;
        }
        let position = axis_position(axis);
        let (from, to) = match position {
            AxisPosition::Left => (
                Point::new(plot.origin.x, plot.origin.y),
                Point::new(plot.origin.x, plot.bottom()),
            ),
            AxisPosition::Right => (
                Point::new(plot.right(), plot.origin.y),
                Point::new(plot.right(), plot.bottom()),
            ),
            AxisPosition::Bottom => (
                Point::new(plot.origin.x, plot.bottom()),
                Point::new(plot.right(), plot.bottom()),
            ),
            AxisPosition::Top => (
                Point::new(plot.origin.x, plot.origin.y),
                Point::new(plot.right(), plot.origin.y),
            ),
        };
        out.push(ChartPrimitive::Line { from, to, stroke });
        match axis.kind {
            AxisKind::Value => {
                draw_value_axis_labels(chart, axis, plot, position, style, shape, out);
            }
            AxisKind::Category | AxisKind::Date => {
                draw_category_axis_labels(
                    chart, axis, plot, categories, position, style, shape, out,
                );
            }
        }
    }
}

/// A value axis' major tick marks and numeric labels.
fn draw_value_axis_labels(
    chart: &Chart,
    axis: &Axis,
    plot: Rect,
    position: AxisPosition,
    style: &ChartStyle,
    shape: &mut dyn FnMut(&str) -> Option<ChartLabel>,
    out: &mut Vec<ChartPrimitive>,
) {
    let scale = axis_scale(chart, axis);
    let on_right = position == AxisPosition::Right;
    let edge = if on_right {
        plot.right()
    } else {
        plot.origin.x
    };
    let show = axis.tick_label_position != TickLabelPosition::None;
    for value in scale.ticks() {
        let y = along(plot.bottom(), plot.origin.y, scale.fraction(value));
        if axis.major_tick_mark != TickMark::None {
            let (from, to) = tick_span(edge, on_right, axis.major_tick_mark);
            out.push(ChartPrimitive::Line {
                from: Point::new(from, y),
                to: Point::new(to, y),
                stroke: ChartStroke {
                    color: AXIS_LINE,
                    width: HAIRLINE,
                },
            });
        }
        if show && let Some(label) = shape_colored(&format_value(value), style.text, shape) {
            let left = if on_right {
                edge + TICK_LENGTH + LABEL_GAP
            } else {
                Twip(edge.raw() - TICK_LENGTH.raw() - LABEL_GAP.raw() - label.width.raw())
            };
            place_label(
                &label,
                left,
                Twip(y.raw() + label.ascent.raw() - label.height().raw() / 2),
                out,
            );
        }
    }
}

/// A category axis' tick labels, one centred under each category slot.
#[allow(clippy::too_many_arguments)]
fn draw_category_axis_labels(
    chart: &Chart,
    axis: &Axis,
    plot: Rect,
    categories: usize,
    position: AxisPosition,
    style: &ChartStyle,
    shape: &mut dyn FnMut(&str) -> Option<ChartLabel>,
    out: &mut Vec<ChartPrimitive>,
) {
    if axis.tick_label_position == TickLabelPosition::None || categories == 0 {
        return;
    }
    let labels = category_labels(chart);
    let slot = plot.size.width.raw() / categories.max(1) as i32;
    let baseline_top = if position == AxisPosition::Top {
        Twip(plot.origin.y.raw() - TICK_LENGTH.raw() - LABEL_GAP.raw())
    } else {
        plot.bottom() + TICK_LENGTH + LABEL_GAP
    };
    for (index, text) in labels.iter().enumerate().take(categories) {
        let Some(label) = shape_colored(text, style.text, shape) else {
            continue;
        };
        let centre = plot.origin.x.raw() + slot * index as i32 + slot / 2;
        let left = Twip(centre - label.width.raw() / 2);
        let baseline = if position == AxisPosition::Top {
            Twip(baseline_top.raw() - label.descent.raw())
        } else {
            baseline_top + label.ascent
        };
        place_label(&label, left, baseline, out);
    }
}

/// A tick mark's span along the axis-normal direction, honouring `c:majorTickMark`.
fn tick_span(edge: Twip, outward_is_right: bool, mark: TickMark) -> (Twip, Twip) {
    let sign = if outward_is_right { 1 } else { -1 };
    let out_end = Twip(edge.raw() + sign * TICK_LENGTH.raw());
    let in_end = Twip(edge.raw() - sign * TICK_LENGTH.raw());
    match mark {
        TickMark::Inside => (edge, in_end),
        TickMark::Cross => (in_end, out_end),
        // `None` is filtered by the caller; `Outside` is Word's default.
        TickMark::None | TickMark::Outside => (edge, out_end),
    }
}

/// Linear interpolation between two twip coordinates at `fraction`.
fn along(zero: Twip, one: Twip, fraction: f64) -> Twip {
    let span = f64::from(one.raw() - zero.raw());
    Twip(f64::from(zero.raw()).mul_add(1.0, span * fraction.clamp(-4.0, 4.0)) as i32)
}

/// Draws one chart group's series.
fn draw_group(
    group: &ChartGroup,
    geometry: &GroupGeometry,
    paint: &Paint<'_>,
    shape: &mut dyn FnMut(&str) -> Option<ChartLabel>,
    out: &mut Vec<ChartPrimitive>,
) {
    match group.kind {
        ChartGroupKind::Bar {
            direction,
            grouping,
            gap_width,
            overlap,
        } => draw_bars(
            group,
            geometry,
            BarShape {
                direction,
                grouping,
                gap_width,
                overlap,
            },
            paint,
            shape,
            out,
        ),
        ChartGroupKind::Line { grouping, marker } => {
            draw_lines(group, geometry, grouping, marker, paint, shape, out);
        }
        ChartGroupKind::Area { grouping } => {
            draw_areas(group, geometry, grouping, paint, out);
        }
        ChartGroupKind::Scatter { style: scatter } => {
            draw_scatter(group, geometry, scatter, paint, out);
        }
        ChartGroupKind::Pie { first_slice_angle } => draw_pie(
            group,
            geometry,
            PieShape {
                first_slice_angle,
                hole_percent: 0,
            },
            paint,
            shape,
            out,
        ),
        ChartGroupKind::Doughnut {
            first_slice_angle,
            hole_size,
        } => draw_pie(
            group,
            geometry,
            PieShape {
                first_slice_angle,
                hole_percent: hole_size,
            },
            paint,
            shape,
            out,
        ),
    }
}

/// A pie or doughnut group's geometry settings.
///
/// One struct for both families, because a doughnut *is* a pie with a hole
/// (`SKILL` §8: one mechanism, not two). A pie is `hole_percent: 0`, and nothing
/// below branches on the family again.
#[derive(Clone, Copy)]
struct PieShape {
    /// `c:firstSliceAng`, degrees clockwise from twelve o'clock.
    first_slice_angle: u16,
    /// `c:holeSize`, a percentage of the outer radius. Zero is a pie.
    hole_percent: u8,
}

/// Pie and doughnut: one sector per plotted point, sized by its share of the
/// series total.
///
/// # What is drawn
///
/// A pie plots its **first series only**, which is what the format means and what
/// Word and ONLYOFFICE both do — a `c:pieChart` with two series shows the first
/// and reports the rest. A doughnut plots **every** series as a concentric ring,
/// the first series innermost, splitting the band between the hole and the outer
/// radius equally. Both are the same loop with a different radial band, which is
/// why there is one function.
///
/// # Sweeps that close the circle exactly
///
/// Sweeps are taken from the **running cumulative total**, rounded once, and
/// differenced — not rounded per slice and summed. Per-slice rounding leaves up
/// to one angle unit of error per slice, so a nine-slice pie would not close; a
/// cumulative difference makes the last slice end exactly on the first slice's
/// start angle by construction, for any number of slices.
///
/// A point whose magnitude is zero contributes **no commands at all**
/// ([`crate::arc::sector`] returns an empty list for a zero sweep), so a
/// zero-valued category paints nothing rather than a hairline sliver on the slice
/// boundary.
///
/// # Complexity
///
/// O(points in the group), with a bounded constant per point: at most 10 path
/// commands. No by-id lookup happens inside the loop.
fn draw_pie(
    group: &ChartGroup,
    geometry: &GroupGeometry,
    pie: PieShape,
    paint: &Paint<'_>,
    shape: &mut dyn FnMut(&str) -> Option<ChartLabel>,
    out: &mut Vec<ChartPrimitive>,
) {
    let plot = geometry.plot;
    // A pie is circular, so it takes the largest circle the plot rectangle holds.
    let outer = Twip(plot.size.width.raw().min(plot.size.height.raw()) / 2);
    if outer.raw() <= 0 || group.series.is_empty() {
        return;
    }
    let centre = Point::new(
        Twip(plot.origin.x.raw() + plot.size.width.raw() / 2),
        Twip(plot.origin.y.raw() + plot.size.height.raw() / 2),
    );
    // `c:firstSliceAng` is measured from twelve o'clock; `crate::arc` measures
    // from three o'clock, DrawingML's own convention. One quarter turn converts.
    let start_angle = i32::from(pie.first_slice_angle)
        .saturating_mul(60_000)
        .saturating_sub(FULL_TURN / 4);

    let hole = Twip(outer.raw() * i32::from(pie.hole_percent.min(90)) / 100);
    // A pie draws its first series; a doughnut rings every series.
    let rings: &[Series] = if pie.hole_percent == 0 {
        &group.series[..1]
    } else {
        &group.series
    };
    let band = (outer.raw() - hole.raw()).max(1) / i32::try_from(rings.len()).unwrap_or(1).max(1);

    for (ring, series) in rings.iter().enumerate() {
        let index = i32::try_from(ring).unwrap_or(0);
        let ring_inner = Twip(hole.raw() + band * index);
        let ring_outer = Twip(hole.raw() + band * (index + 1));
        let count = range_len(&series.values).max(geometry.categories);
        let values = dense_numbers(&series.values, count);
        // Magnitudes: Word plots |value| in a pie, because a negative share of a
        // whole has no angle. The sign is not lost — it is still in the cache and
        // still re-emitted verbatim on export.
        let total: f64 = values.iter().flatten().map(|value| value.abs()).sum();
        if total <= 0.0 {
            continue;
        }
        let mut running = 0.0f64;
        let mut swept = 0i32;
        for (point, value) in values.iter().enumerate() {
            let magnitude = value.unwrap_or(0.0).abs();
            running += magnitude;
            // Round the CUMULATIVE fraction, then difference: the circle closes.
            let reached = round_angle(running / total * f64::from(FULL_TURN));
            let sweep = reached - swept;
            swept = reached;
            let commands = sector(
                centre,
                ring_outer,
                ring_inner,
                start_angle.saturating_add(swept - sweep),
                sweep,
            );
            if commands.is_empty() {
                continue;
            }
            let color = paint.point(point);
            out.push(ChartPrimitive::Path {
                commands,
                closed: true,
                fill: Some(color),
                stroke: Some(ChartStroke {
                    color: BACKGROUND,
                    width: HAIRLINE,
                }),
            });
            if show_value(series) {
                // At the band's mid-radius, on the slice's bisector — the only
                // place a label is inside its own slice for every slice width.
                let mid = f64::from(ring_inner.raw() + ring_outer.raw()) / 2.0;
                let bisector = f64::from(start_angle.saturating_add(swept - sweep / 2));
                let radians = bisector / 60_000.0 * core::f64::consts::PI / 180.0;
                let anchor = Rect::new(
                    Point::new(
                        Twip(centre.x.raw() + round_angle(mid * radians.cos())),
                        Twip(centre.y.raw() + round_angle(mid * radians.sin())),
                    ),
                    Size::new(Twip::ZERO, Twip::ZERO),
                );
                draw_data_label(
                    &format_value(magnitude),
                    anchor,
                    false,
                    // A slice label sits on the bisector at mid-radius, which is
                    // already its center; the zero-size anchor centres it.
                    Some(DataLabelPosition::Center),
                    paint.style,
                    shape,
                    out,
                );
            }
        }
    }
}

/// Rounds an angle or length to a whole unit, saturating rather than wrapping.
fn round_angle(value: f64) -> i32 {
    if value.is_nan() {
        return 0;
    }
    let rounded = value.round();
    if rounded >= f64::from(i32::MAX) {
        i32::MAX
    } else if rounded <= f64::from(i32::MIN) {
        i32::MIN
    } else {
        #[allow(clippy::cast_possible_truncation)]
        {
            rounded as i32
        }
    }
}

/// A bar group's geometry settings, grouped so [`draw_bars`] stays under the
/// argument ceiling.
#[derive(Clone, Copy)]
struct BarShape {
    direction: BarDirection,
    grouping: BarGrouping,
    gap_width: u16,
    overlap: i16,
}

/// Bars: one rectangle per plotted point, from the value-axis zero to the value.
///
/// Clustered bars share a category slot side by side; stacked bars accumulate,
/// and `percentStacked` normalises each category to 100.
fn draw_bars(
    group: &ChartGroup,
    geometry: &GroupGeometry,
    bars: BarShape,
    paint: &Paint<'_>,
    shape: &mut dyn FnMut(&str) -> Option<ChartLabel>,
    out: &mut Vec<ChartPrimitive>,
) {
    let plot = geometry.plot;
    let horizontal = bars.direction == BarDirection::Bar;
    let categories = geometry.categories.max(1);
    // The category axis runs along the plot's width for a column chart and down
    // its height for a bar chart; everything below is written once against
    // `slot`/`value_span` and switched at the two points it matters.
    let slot = if horizontal {
        plot.size.height.raw() / categories as i32
    } else {
        plot.size.width.raw() / categories as i32
    };
    let stacked = matches!(
        bars.grouping,
        BarGrouping::Stacked | BarGrouping::PercentStacked
    );
    let series_count = group.series.len().max(1);
    let (bar_width, step) = bar_metrics(slot, series_count, &bars, stacked);
    let values: Vec<Vec<Option<f64>>> = group
        .series
        .iter()
        .map(|series| dense_numbers(&series.values, geometry.categories))
        .collect();
    let normalised = if bars.grouping == BarGrouping::PercentStacked {
        Some(percent_totals(&values, geometry.categories))
    } else {
        None
    };

    for (index, series) in group.series.iter().enumerate() {
        let color = paint.series(series, geometry.first_color + index);
        let mut stack = vec![0.0f64; geometry.categories];
        if stacked {
            for prior in values.iter().take(index) {
                accumulate(&mut stack, prior, normalised.as_deref());
            }
        }
        for category in 0..geometry.categories {
            let Some(raw) = values[index][category] else {
                continue;
            };
            let value = match normalised.as_deref() {
                Some(totals) => scale_percent(raw, totals[category]),
                None => raw,
            };
            let base = if stacked { stack[category] } else { 0.0 };
            let (lo, hi) = (
                geometry.scale.fraction(base),
                geometry.scale.fraction(base + value),
            );
            let lead = if stacked {
                slot * category as i32 + (slot - bar_width) / 2
            } else {
                slot * category as i32
                    + (slot - step * (series_count as i32 - 1) - bar_width) / 2
                    + step * index as i32
            };
            let rect = if horizontal {
                let x0 = along(plot.origin.x, plot.right(), lo);
                let x1 = along(plot.origin.x, plot.right(), hi);
                Rect::new(
                    Point::new(
                        Twip(x0.raw().min(x1.raw())),
                        Twip(plot.origin.y.raw() + lead),
                    ),
                    Size::new(Twip((x1.raw() - x0.raw()).abs().max(1)), Twip(bar_width)),
                )
            } else {
                let y0 = along(plot.bottom(), plot.origin.y, lo);
                let y1 = along(plot.bottom(), plot.origin.y, hi);
                Rect::new(
                    Point::new(
                        Twip(plot.origin.x.raw() + lead),
                        Twip(y0.raw().min(y1.raw())),
                    ),
                    Size::new(Twip(bar_width), Twip((y1.raw() - y0.raw()).abs().max(1))),
                )
            };
            out.push(ChartPrimitive::Rect {
                rect,
                fill: Some(color),
                stroke: None,
            });
            if show_value(series) {
                draw_data_label(
                    &format_value(raw),
                    rect,
                    horizontal,
                    label_position(series),
                    paint.style,
                    shape,
                    out,
                );
            }
        }
    }
}

/// A bar's width and the step between two clustered series, from `c:gapWidth`
/// and `c:overlap`.
///
/// `gapWidth` is the gap between category clusters as a percentage of one bar's
/// width; `overlap` is the percentage of a bar's width that adjacent series
/// overlap. So a slot holds `n` bars less their overlaps plus one gap:
/// `slot = barWidth * (n - (n - 1) * overlap/100 + gapWidth/100)`.
fn bar_metrics(slot: i32, series: usize, shape: &BarShape, stacked: bool) -> (i32, i32) {
    let gap = f64::from(shape.gap_width.min(500)) / 100.0;
    if stacked {
        let width = f64::from(slot) / (1.0 + gap);
        return (width.round().max(1.0) as i32, 0);
    }
    let overlap = f64::from(shape.overlap.clamp(-100, 100)) / 100.0;
    let n = series as f64;
    let denominator = (n - (n - 1.0) * overlap + gap).max(0.5);
    let width = (f64::from(slot) / denominator).max(1.0);
    let step = width * (1.0 - overlap);
    (width.round().max(1.0) as i32, step.round() as i32)
}

/// Lines: a polyline through every plotted point, plus markers when the group
/// declares them.
#[allow(clippy::too_many_arguments)]
fn draw_lines(
    group: &ChartGroup,
    geometry: &GroupGeometry,
    grouping: Grouping,
    marker: bool,
    paint: &Paint<'_>,
    shape: &mut dyn FnMut(&str) -> Option<ChartLabel>,
    out: &mut Vec<ChartPrimitive>,
) {
    let plot = geometry.plot;
    let categories = geometry.categories.max(1);
    let slot = plot.size.width.raw() / categories as i32;
    let stacked = matches!(grouping, Grouping::Stacked | Grouping::PercentStacked);
    let values: Vec<Vec<Option<f64>>> = group
        .series
        .iter()
        .map(|series| dense_numbers(&series.values, geometry.categories))
        .collect();
    let normalised = if grouping == Grouping::PercentStacked {
        Some(percent_totals(&values, geometry.categories))
    } else {
        None
    };

    for (index, series) in group.series.iter().enumerate() {
        let color = paint.line(series, geometry.first_color + index);
        if series.line.is_some_and(|line| line.no_fill) && !marker {
            continue;
        }
        let mut stack = vec![0.0f64; geometry.categories];
        if stacked {
            for prior in values.iter().take(index) {
                accumulate(&mut stack, prior, normalised.as_deref());
            }
        }
        let mut points: Vec<Point> = Vec::new();
        for category in 0..geometry.categories {
            let plotted = plotted_value(values[index][category], DisplayBlanks::Gap);
            let Some(raw) = plotted else {
                // A gap ends the current polyline; the next point starts a new
                // one (`c:dispBlanksAs="gap"`).
                flush_polyline(&mut points, color, series, out);
                continue;
            };
            let value = match normalised.as_deref() {
                Some(totals) => scale_percent(raw, totals[category]),
                None => raw,
            };
            let base = if stacked { stack[category] } else { 0.0 };
            let x = Twip(plot.origin.x.raw() + slot * category as i32 + slot / 2);
            let y = along(
                plot.bottom(),
                plot.origin.y,
                geometry.scale.fraction(base + value),
            );
            points.push(Point::new(x, y));
            if show_value(series) {
                draw_data_label(
                    &format_value(raw),
                    Rect::new(Point::new(x, y), Size::new(Twip::ZERO, Twip::ZERO)),
                    false,
                    label_position(series),
                    paint.style,
                    shape,
                    out,
                );
            }
        }
        if series.smooth {
            points = sample_smooth(&points);
        }
        flush_polyline(&mut points, color, series, out);
        if marker {
            // Re-walk the plotted points for markers so a smoothed series puts a
            // marker on its DATA points and not on its sampled ones.
            draw_markers(group, geometry, index, color, out);
        }
    }
}

/// Appends the accumulated polyline, if it has at least two points, and clears it.
fn flush_polyline(
    points: &mut Vec<Point>,
    color: [u8; 4],
    series: &Series,
    out: &mut Vec<ChartPrimitive>,
) {
    if points.len() >= 2 && !series.line.is_some_and(|line| line.no_fill) {
        out.push(ChartPrimitive::polyline(
            &core::mem::take(points),
            false,
            None,
            Some(ChartStroke {
                color,
                width: series_line_width(series),
            }),
        ));
    } else {
        points.clear();
    }
}

/// A marker at each of one series' plotted category points.
fn draw_markers(
    group: &ChartGroup,
    geometry: &GroupGeometry,
    index: usize,
    color: [u8; 4],
    out: &mut Vec<ChartPrimitive>,
) {
    let Some(series) = group.series.get(index) else {
        return;
    };
    let plot = geometry.plot;
    let categories = geometry.categories.max(1);
    let slot = plot.size.width.raw() / categories as i32;
    let values = dense_numbers(&series.values, geometry.categories);
    for (category, value) in values.iter().enumerate() {
        let Some(raw) = value else { continue };
        let x = Twip(plot.origin.x.raw() + slot * category as i32 + slot / 2);
        let y = along(plot.bottom(), plot.origin.y, geometry.scale.fraction(*raw));
        push_marker(Point::new(x, y), color, out);
    }
}

/// A circular series marker centred on a point. An [`crate::display::PaintItem::Ellipse`]
/// already covers a round marker, so this needs no arc (`docs/155` §7.2).
fn push_marker(centre: Point, color: [u8; 4], out: &mut Vec<ChartPrimitive>) {
    let half = Twip(MARKER_SIZE.raw() / 2);
    out.push(ChartPrimitive::Ellipse {
        rect: Rect::new(
            Point::new(centre.x - half, centre.y - half),
            Size::new(MARKER_SIZE, MARKER_SIZE),
        ),
        fill: Some(color),
        stroke: Some(ChartStroke {
            color: BACKGROUND,
            width: HAIRLINE,
        }),
    });
}

/// Areas: a closed polygon from the value-axis zero up through the series' points
/// and back down.
fn draw_areas(
    group: &ChartGroup,
    geometry: &GroupGeometry,
    grouping: Grouping,
    paint: &Paint<'_>,
    out: &mut Vec<ChartPrimitive>,
) {
    let plot = geometry.plot;
    let categories = geometry.categories.max(1);
    let slot = plot.size.width.raw() / categories as i32;
    let stacked = matches!(grouping, Grouping::Stacked | Grouping::PercentStacked);
    let values: Vec<Vec<Option<f64>>> = group
        .series
        .iter()
        .map(|series| dense_numbers(&series.values, geometry.categories))
        .collect();
    let normalised = if grouping == Grouping::PercentStacked {
        Some(percent_totals(&values, geometry.categories))
    } else {
        None
    };
    let baseline = along(plot.bottom(), plot.origin.y, geometry.scale.fraction(0.0));

    for (index, series) in group.series.iter().enumerate() {
        let color = paint.series(series, geometry.first_color + index);
        let mut stack = vec![0.0f64; geometry.categories];
        if stacked {
            for prior in values.iter().take(index) {
                accumulate(&mut stack, prior, normalised.as_deref());
            }
        }
        let mut upper: Vec<Point> = Vec::new();
        let mut lower: Vec<Point> = Vec::new();
        for category in 0..geometry.categories {
            let Some(raw) = values[index][category] else {
                continue;
            };
            let value = match normalised.as_deref() {
                Some(totals) => scale_percent(raw, totals[category]),
                None => raw,
            };
            let base = if stacked { stack[category] } else { 0.0 };
            let x = Twip(plot.origin.x.raw() + slot * category as i32 + slot / 2);
            upper.push(Point::new(
                x,
                along(
                    plot.bottom(),
                    plot.origin.y,
                    geometry.scale.fraction(base + value),
                ),
            ));
            lower.push(Point::new(
                x,
                if stacked {
                    along(plot.bottom(), plot.origin.y, geometry.scale.fraction(base))
                } else {
                    baseline
                },
            ));
        }
        if upper.len() < 2 {
            continue;
        }
        lower.reverse();
        upper.extend(lower);
        out.push(ChartPrimitive::polyline(&upper, true, Some(color), None));
    }
}

/// Scatter: markers at `(xVal, yVal)`, optionally joined by straight connectors.
///
/// A scatter series' x values are plotted against the group's **first** value
/// axis and its y values against the second, which is what `c:axId` order means
/// for a scatter group.
fn draw_scatter(
    group: &ChartGroup,
    geometry: &GroupGeometry,
    scatter: ScatterStyle,
    paint: &Paint<'_>,
    out: &mut Vec<ChartPrimitive>,
) {
    let plot = geometry.plot;
    let joined = matches!(
        scatter,
        ScatterStyle::Line
            | ScatterStyle::LineMarker
            | ScatterStyle::Smooth
            | ScatterStyle::SmoothMarker
    );
    let marked = matches!(
        scatter,
        ScatterStyle::Marker
            | ScatterStyle::LineMarker
            | ScatterStyle::SmoothMarker
            | ScatterStyle::Smooth
    );
    let x_scale = scatter_x_scale(group);
    for (index, series) in group.series.iter().enumerate() {
        let color = paint.line(series, geometry.first_color + index);
        let count = range_len(&series.values).max(series.x_values.as_ref().map_or(0, range_len));
        let ys = dense_numbers(&series.values, count);
        let xs = series
            .x_values
            .as_ref()
            .map(|range| dense_numbers(range, count));
        let mut points: Vec<Point> = Vec::new();
        for i in 0..count {
            let Some(y) = ys[i] else { continue };
            // With no `c:xVal` a scatter series is plotted at 1, 2, 3 … which is
            // what Excel does for an omitted x range.
            let x = match xs.as_ref() {
                Some(values) => match values[i] {
                    Some(x) => x,
                    None => continue,
                },
                None => i as f64 + 1.0,
            };
            points.push(Point::new(
                along(plot.origin.x, plot.right(), x_scale.fraction(x)),
                along(plot.bottom(), plot.origin.y, geometry.scale.fraction(y)),
            ));
        }
        let smoothed = if series.smooth
            || matches!(scatter, ScatterStyle::Smooth | ScatterStyle::SmoothMarker)
        {
            sample_smooth(&points)
        } else {
            points.clone()
        };
        if joined && smoothed.len() >= 2 && !series.line.is_some_and(|line| line.no_fill) {
            out.push(ChartPrimitive::polyline(
                &smoothed,
                false,
                None,
                Some(ChartStroke {
                    color,
                    width: series_line_width(series),
                }),
            ));
        }
        if marked {
            for point in &points {
                push_marker(*point, color, out);
            }
        }
    }
}

/// The x scale a scatter group's `c:xVal` ranges are plotted against, derived
/// from the cached x values.
fn scatter_x_scale(group: &ChartGroup) -> Scale {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    let mut any = false;
    for series in &group.series {
        if let Some(range) = series.x_values.as_ref() {
            for (_, value) in &range.points {
                if let Some(x) = value.as_f64() {
                    min = min.min(x);
                    max = max.max(x);
                    any = true;
                }
            }
        } else {
            min = min.min(1.0);
            max = max.max(range_len(&series.values).max(1) as f64);
            any = true;
        }
    }
    if !any {
        return Scale {
            min: 0.0,
            max: 1.0,
            step: 1.0,
            reversed: false,
        };
    }
    nice_scale(min, max, false)
}

/// A data label placed where its series' `c:dLblPos` asks.
///
/// `anchor` is the bar's rectangle, or a zero-size rectangle at a line or
/// scatter point. A bar honours Word's four bar positions — outside end (the
/// default), inside end, center and inside base — measured along the bar's own
/// direction; a point honours above (the default), below, left, right and
/// center. Any other position, or none, is the default for the shape.
fn draw_data_label(
    text: &str,
    anchor: Rect,
    horizontal: bool,
    position: Option<DataLabelPosition>,
    style: &ChartStyle,
    shape: &mut dyn FnMut(&str) -> Option<ChartLabel>,
    out: &mut Vec<ChartPrimitive>,
) {
    let Some(label) = shape_colored(text, style.text, shape) else {
        return;
    };
    let (w, h, ascent) = (label.width.raw(), label.height().raw(), label.ascent.raw());
    let gap = LABEL_GAP.raw();
    let (x0, y0) = (anchor.origin.x.raw(), anchor.origin.y.raw());
    let (aw, ah) = (anchor.size.width.raw(), anchor.size.height.raw());
    let centre_x = x0 + aw / 2 - w / 2;
    let centre_top = y0 + ah / 2 - h / 2;
    let is_point = aw == 0 && ah == 0;
    // (left, top) of the label box.
    let (left, top) = if is_point {
        match position {
            Some(DataLabelPosition::Bottom) => (centre_x, y0 + gap),
            Some(DataLabelPosition::Left) => (x0 - gap - w, centre_top),
            Some(DataLabelPosition::Right) => (x0 + gap, centre_top),
            Some(DataLabelPosition::Center) => (centre_x, centre_top),
            _ => (centre_x, y0 - gap - h),
        }
    } else if horizontal {
        // A horizontal bar grows rightwards from its base at `x0`.
        let x = match position {
            Some(DataLabelPosition::InsideEnd) => x0 + aw - gap - w,
            Some(DataLabelPosition::Center) => x0 + aw / 2 - w / 2,
            Some(DataLabelPosition::InsideBase) => x0 + gap,
            _ => x0 + aw + gap,
        };
        (x, centre_top)
    } else {
        // A column grows upwards from its base at `y0 + ah`.
        let y = match position {
            Some(DataLabelPosition::InsideEnd) => y0 + gap,
            Some(DataLabelPosition::Center) => y0 + ah / 2 - h / 2,
            Some(DataLabelPosition::InsideBase) => y0 + ah - gap - h,
            _ => y0 - gap - h,
        };
        (centre_x, y)
    };
    place_label(&label, Twip(left), Twip(top + ascent), out);
}

/// Where a series' labels sit, from its `c:dLbls`.
fn label_position(series: &Series) -> Option<DataLabelPosition> {
    series.data_labels.and_then(|labels| labels.position)
}

/// Whether a series' `c:dLbls` asks for its values to be printed.
fn show_value(series: &Series) -> bool {
    series
        .data_labels
        .is_some_and(|labels| labels.show_value || labels.show_percent)
}

/// Samples a polyline through a Catmull-Rom spline at [`SMOOTH_SAMPLES`] steps
/// per interval.
///
/// `c:smooth` is a function evaluated at points, not a shape outline, so a
/// sampled polyline is the honest representation and is what ONLYOFFICE's
/// shipping `calculateSplineLine` produces (`docs/155` §3.3, §7.3). When
/// [`crate::display::PathCommand`] gains an arc this becomes a real cubic; until
/// then it is not an approximation standing in for one.
///
/// Complexity: O(points × [`SMOOTH_SAMPLES`]).
fn sample_smooth(points: &[Point]) -> Vec<Point> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let at = |i: isize| -> Point {
        let last = points.len() as isize - 1;
        points[i.clamp(0, last) as usize]
    };
    let mut out = Vec::with_capacity(points.len() * SMOOTH_SAMPLES);
    out.push(points[0]);
    for i in 0..points.len() as isize - 1 {
        let (p0, p1, p2, p3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
        for step in 1..=SMOOTH_SAMPLES {
            let t = step as f64 / SMOOTH_SAMPLES as f64;
            out.push(Point::new(
                Twip(catmull_rom(
                    f64::from(p0.x.raw()),
                    f64::from(p1.x.raw()),
                    f64::from(p2.x.raw()),
                    f64::from(p3.x.raw()),
                    t,
                ) as i32),
                Twip(catmull_rom(
                    f64::from(p0.y.raw()),
                    f64::from(p1.y.raw()),
                    f64::from(p2.y.raw()),
                    f64::from(p3.y.raw()),
                    t,
                ) as i32),
            ));
        }
    }
    out
}

/// The uniform Catmull-Rom basis evaluated at `t` on one axis.
fn catmull_rom(p0: f64, p1: f64, p2: f64, p3: f64, t: f64) -> f64 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * ((2.0 * p1)
        + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
}

/// A series' stroke width: `a:ln@w` in EMU converted to twips, else Word's
/// default series weight.
fn series_line_width(series: &Series) -> Twip {
    series
        .line
        .and_then(|line| line.width_emu)
        .map_or(SERIES_LINE_WIDTH, |emu| {
            crate::units::emu_to_twip_extent(i64::from(emu)).max(HAIRLINE)
        })
}

/// Every series' default colour, in whole-chart document order, so a combo's
/// second group continues the accent cycle instead of restarting it.
///
/// Complexity: O(series in the chart).
fn series_colors(chart: &Chart, style: &ChartStyle) -> Vec<[u8; 4]> {
    let total: usize = chart
        .plot_area
        .groups
        .iter()
        .map(|group| group.series.len())
        .sum();
    (0..total.max(1))
        .map(|index| style.accents[index % style.accents.len()])
        .collect()
}

/// Shapes a label and recolours its runs to `color`.
///
/// The caller's shaper resolves fonts through the document's cascade, which paints
/// body text in the document's own colour; chart furniture is grey, so the colour
/// is applied here rather than threaded through the shaping closure.
fn shape_colored(
    text: &str,
    color: [u8; 4],
    shape: &mut dyn FnMut(&str) -> Option<ChartLabel>,
) -> Option<ChartLabel> {
    if text.is_empty() {
        return None;
    }
    let mut label = shape(text)?;
    if label.width.raw() <= 0 {
        return None;
    }
    for run in &mut label.runs {
        run.color = color;
    }
    Some(label)
}

/// A cached range's effective length: the producer's `c:ptCount` when it is at
/// least as long as the cache, else one past the highest cached index (a cache may
/// be sparse, and `c:ptCount` may understate it).
fn range_len(range: &DataRange) -> usize {
    let declared = range.point_count as usize;
    let highest = range
        .points
        .iter()
        .map(|(index, _)| *index as usize + 1)
        .max()
        .unwrap_or(0);
    declared.max(highest)
}

/// A cached numeric range densified to `count` slots, `None` where the cache has
/// no plottable number.
///
/// Uses [`ChartValue::as_f64`], which refuses `NaN` and an out-of-range
/// exponent, so a producer-supplied cache cannot leak a non-finite value into
/// axis scaling.
fn dense_numbers(range: &DataRange, count: usize) -> Vec<Option<f64>> {
    let mut out = vec![None; count];
    for (index, value) in &range.points {
        let slot = *index as usize;
        if slot < count {
            out[slot] = value.as_f64();
        }
    }
    out
}

/// A cached label range densified to `count` slots. A numeric category keeps its
/// verbatim lexical form, which is what a producer wrote and what Word prints.
fn dense_labels(range: &DataRange, count: usize) -> Vec<String> {
    let mut out = vec![String::new(); count];
    for (index, value) in &range.points {
        let slot = *index as usize;
        if slot < count {
            out[slot] = match value {
                ChartValue::Text(text) | ChartValue::Number(text) => text.clone(),
                ChartValue::Blank => String::new(),
            };
        }
    }
    out
}

/// How a blank cache entry is plotted (`c:dispBlanksAs`).
fn plotted_value(value: Option<f64>, blanks: DisplayBlanks) -> Option<f64> {
    match (value, blanks) {
        (Some(value), _) => Some(value),
        (None, DisplayBlanks::Zero) => Some(0.0),
        (None, DisplayBlanks::Gap | DisplayBlanks::Span) => None,
    }
}

/// Adds one series' contribution to a running stack.
fn accumulate(stack: &mut [f64], values: &[Option<f64>], totals: Option<&[f64]>) {
    for (index, slot) in stack.iter_mut().enumerate() {
        if let Some(Some(value)) = values.get(index) {
            *slot += match totals {
                Some(totals) => scale_percent(*value, totals[index]),
                None => *value,
            };
        }
    }
}

/// The per-category absolute totals a `percentStacked` group normalises against.
fn percent_totals(values: &[Vec<Option<f64>>], count: usize) -> Vec<f64> {
    let mut totals = vec![0.0f64; count];
    for series in values {
        for (index, value) in series.iter().enumerate() {
            if let Some(value) = value {
                totals[index] += value.abs();
            }
        }
    }
    totals
}

/// One value as a percentage of its category total; `0.0` for an empty category,
/// which is the only value that cannot mislead.
fn scale_percent(value: f64, total: f64) -> f64 {
    if total.abs() < f64::EPSILON {
        0.0
    } else {
        value / total * 100.0
    }
}

/// How many category slots the plot is divided into: the longest cached range in
/// any drawable non-scatter group.
///
/// Complexity: O(series in the chart); it reads declared counts and cached
/// indices, never the values.
fn category_count(chart: &Chart) -> usize {
    chart
        .plot_area
        .groups
        .iter()
        .filter(|group| {
            is_drawable(group.kind) && !matches!(group.kind, ChartGroupKind::Scatter { .. })
        })
        .flat_map(|group| group.series.iter())
        .map(|series| {
            range_len(&series.values).max(series.categories.as_ref().map_or(0, range_len))
        })
        .max()
        .unwrap_or(0)
}

/// The category labels, taken from the first drawable series that cached any.
///
/// Complexity: O(series + cached category points).
fn category_labels(chart: &Chart) -> Vec<String> {
    let count = category_count(chart);
    chart
        .plot_area
        .groups
        .iter()
        .filter(|group| is_drawable(group.kind))
        .flat_map(|group| group.series.iter())
        .find_map(|series| {
            series
                .categories
                .as_ref()
                .filter(|range| !range.points.is_empty())
                .map(|range| dense_labels(range, count))
        })
        .unwrap_or_else(|| (1..=count).map(|index| index.to_string()).collect())
}

/// The value-axis scale one chart group is plotted against.
///
/// The group's value axis is the **last** `c:axId` it names that resolves to a
/// value axis: Word writes the category axis id first, and for a scatter group
/// both ids are value axes with the y axis second. A group that names a second
/// value axis is plotted against it — that is all a secondary axis is
/// (`docs/155` §4.2).
///
/// Complexity: O(axes + series in the chart). Resolved once per group and carried,
/// never looked up per point.
fn group_scale(chart: &Chart, group: &ChartGroup) -> Scale {
    let axis = group
        .axis_ids
        .iter()
        .rev()
        .find_map(|id| {
            chart
                .plot_area
                .axes
                .iter()
                .find(|axis| axis.id == *id && axis.kind == AxisKind::Value)
        })
        .or_else(|| {
            chart
                .plot_area
                .axes
                .iter()
                .find(|axis| axis.kind == AxisKind::Value)
        });
    match axis {
        Some(axis) => axis_scale(chart, axis),
        None => derived_scale(chart, None, false),
    }
}

/// One value axis' scale: the producer's `c:min`/`c:max` where declared, else a
/// range derived from every series plotted against it.
///
/// Complexity: O(series in the chart + their cached points).
fn axis_scale(chart: &Chart, axis: &Axis) -> Scale {
    let reversed = axis.orientation == AxisOrientation::MaxMin;
    let mut scale = derived_scale(chart, Some(axis.id), reversed);
    // `c:min`/`c:max` are carried verbatim; parse them through the model's own
    // accessor rather than re-implementing the refusal of `NaN`/`1e400`.
    if let Some(min) = axis.minimum.as_deref().and_then(parse_axis_bound) {
        scale.min = min;
    }
    if let Some(max) = axis.maximum.as_deref().and_then(parse_axis_bound) {
        scale.max = max;
    }
    if scale.max <= scale.min {
        scale.max = scale.min + 1.0;
    }
    scale.step = nice_step(scale.min, scale.max);
    scale
}

/// A verbatim axis bound as a finite `f64`, through the model's accessor.
fn parse_axis_bound(text: &str) -> Option<f64> {
    ChartValue::Number(text.to_owned()).as_f64()
}

/// The data-derived scale for the series plotted against `axis_id` (or, with
/// `None`, every series in the chart).
fn derived_scale(chart: &Chart, axis_id: Option<u32>, reversed: bool) -> Scale {
    let mut min = 0.0f64;
    let mut max = 0.0f64;
    let mut any = false;
    let mut percent = false;
    for group in &chart.plot_area.groups {
        if !is_drawable(group.kind) {
            continue;
        }
        if let Some(id) = axis_id
            && !group.axis_ids.is_empty()
            && !group.axis_ids.contains(&id)
        {
            continue;
        }
        if group_is_percent_stacked(group) {
            percent = true;
            continue;
        }
        let stacked = group_is_stacked(group);
        let count = group
            .series
            .iter()
            .map(|series| range_len(&series.values))
            .max()
            .unwrap_or(0);
        let mut positive = vec![0.0f64; count];
        let mut negative = vec![0.0f64; count];
        for series in &group.series {
            for (index, value) in &series.values.points {
                let Some(value) = value.as_f64() else {
                    continue;
                };
                any = true;
                let slot = *index as usize;
                if stacked && slot < count {
                    if value >= 0.0 {
                        positive[slot] += value;
                    } else {
                        negative[slot] += value;
                    }
                } else {
                    min = min.min(value);
                    max = max.max(value);
                }
            }
        }
        for slot in 0..count {
            max = max.max(positive[slot]);
            min = min.min(negative[slot]);
        }
    }
    if percent && !any {
        return Scale {
            min: 0.0,
            max: 100.0,
            step: 20.0,
            reversed,
        };
    }
    if !any {
        return Scale {
            min: 0.0,
            max: 1.0,
            step: 1.0,
            reversed,
        };
    }
    nice_scale(min, max, reversed)
}

/// Whether a group stacks its series.
fn group_is_stacked(group: &ChartGroup) -> bool {
    match group.kind {
        ChartGroupKind::Bar { grouping, .. } => {
            matches!(grouping, BarGrouping::Stacked | BarGrouping::PercentStacked)
        }
        ChartGroupKind::Line { grouping, .. } | ChartGroupKind::Area { grouping } => {
            matches!(grouping, Grouping::Stacked | Grouping::PercentStacked)
        }
        _ => false,
    }
}

/// Whether a group normalises each category to 100%.
fn group_is_percent_stacked(group: &ChartGroup) -> bool {
    match group.kind {
        ChartGroupKind::Bar { grouping, .. } => grouping == BarGrouping::PercentStacked,
        ChartGroupKind::Line { grouping, .. } | ChartGroupKind::Area { grouping } => {
            grouping == Grouping::PercentStacked
        }
        _ => false,
    }
}

/// Rounds a data range outward to a readable axis: a `1`/`2`/`2.5`/`5`/`10`
/// times-a-power-of-ten major step, with the bounds snapped to it.
///
/// This is the textbook "nice numbers" axis rule rather than an invention; Word's
/// own auto-scaling produces the same family of steps.
fn nice_scale(min: f64, max: f64, reversed: bool) -> Scale {
    let (mut lo, mut hi) = (min.min(max), max.max(min));
    if !lo.is_finite() || !hi.is_finite() {
        return Scale {
            min: 0.0,
            max: 1.0,
            step: 1.0,
            reversed,
        };
    }
    if (hi - lo).abs() < f64::EPSILON {
        hi = lo + lo.abs().max(1.0);
    }
    let step = nice_step(lo, hi);
    lo = (lo / step).floor() * step;
    hi = (hi / step).ceil() * step;
    Scale {
        min: lo,
        max: hi,
        step,
        reversed,
    }
}

/// The major interval for a span, aiming at [`TARGET_INTERVALS`] intervals.
fn nice_step(min: f64, max: f64) -> f64 {
    let span = (max - min).abs();
    if span < f64::EPSILON || !span.is_finite() {
        return 1.0;
    }
    let raw = span / TARGET_INTERVALS;
    let magnitude = 10f64.powf(raw.log10().floor());
    let normalised = raw / magnitude;
    let factor = if normalised <= 1.0 {
        1.0
    } else if normalised <= 2.0 {
        2.0
    } else if normalised <= 2.5 {
        2.5
    } else if normalised <= 5.0 {
        5.0
    } else {
        10.0
    };
    (magnitude * factor).max(f64::MIN_POSITIVE)
}

/// Formats an axis or data-label number for display.
///
/// `c:numFmt`/`c:formatCode` is **not** applied: a format code is a whole
/// mini-language and it is reported as unmodelled rather than half-implemented
/// (`docs/155` §4.3, §14.5). This prints the value plainly, trimming a trailing
/// `.0`, which is what Word's `General` format does and what the fixture carries.
fn format_value(value: f64) -> String {
    if !value.is_finite() {
        return String::new();
    }
    if (value - value.round()).abs() < 1e-9 && value.abs() < 1e15 {
        return format!("{}", value.round() as i64);
    }
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}
