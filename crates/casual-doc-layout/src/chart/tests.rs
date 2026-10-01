// SPDX-License-Identifier: Apache-2.0

//! Unit-level geometry tests for [`super`]: chart composition driven straight
//! from a hand-built projection, with no package, importer or shaper involved.
//!
//! The division of labour with `casual-doc-render/tests/chart_paint.rs` is
//! deliberate. That file proves a chart survives a real `.docx` and reaches the
//! display list; it therefore tests one fixture. These tests cover the families
//! and attributes **no committed fixture exercises** — horizontal bars, stacked
//! and percent-stacked grouping, areas, scatters with explicit `c:xVal`,
//! `c:smooth`, a `maxMin` axis, and the three values of `c:dispBlanksAs` — which
//! would otherwise be code with no guard over it at all.
//!
//! Every assertion here is on geometry derived from the cached data, so the
//! shaper is a stand-in (see `fake_shaper`) and no coordinate depends on which
//! fonts the machine has installed.
use std::collections::BTreeMap;

use super::*;
use crate::text::{Decoration, FontId};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{ChartCoverage, DataLabels, PlotArea};

/// The box every test composes into: 6in x 3.5in, the authored extent of
/// `fixtures/generated/chart.docx`.
const BOX: Size = Size {
    width: Twip(8_640),
    height: Twip(5_040),
};

/// A stand-in label shaper: one run, with an advance proportional to the
/// text's length.
///
/// Deliberately **not** the real shaper. These tests assert geometry derived
/// from the cached data, and the real shaper would make every coordinate
/// depend on which faces the machine happens to have installed. The one thing
/// this closure must do faithfully is reserve a width, because the axis
/// gutters are measured from it.
fn fake_shaper(text: &str) -> Option<ChartLabel> {
    if text.is_empty() {
        return None;
    }
    Some(ChartLabel {
        runs: vec![GlyphRun {
            is_marker: false,
            is_leader: false,
            node: None,
            font: FontId(0),
            size: Twip(180),
            ascent: Twip(140),
            descent: Twip(40),
            character_scale_percent: 100,
            color: [0, 0, 0, 255],
            origin: Point::new(Twip::ZERO, Twip::ZERO),
            bidi_level: 0,
            decoration: Decoration::default(),
            highlight: None,
            shading: None,
            glyphs: Vec::new(),
        }],
        width: Twip(120 * text.chars().count() as i32),
        ascent: Twip(140),
        descent: Twip(40),
    })
}

/// Composes `chart` into [`BOX`] with the default style and no document theme.
fn compose(chart: &Chart) -> Vec<ChartPrimitive> {
    let style = ChartStyle::default();
    let mut shape: fn(&str) -> Option<ChartLabel> = fake_shaper;
    compose_chart(chart, BOX, &style, &resolve_literal_color, &mut shape)
}

/// A `DataRange` over verbatim lexical numbers. `None` is a **blank** cache
/// entry — a category the range declares but holds no `c:pt` for — which is
/// the thing `c:dispBlanksAs` governs.
fn range(values: &[Option<&str>]) -> DataRange {
    DataRange {
        formula: Some("Sheet1!$B$2:$B$9".to_owned()),
        point_count: values.len() as u32,
        points: values
            .iter()
            .enumerate()
            .filter_map(|(i, v)| v.map(|v| (i as u32, ChartValue::Number(v.to_owned()))))
            .collect(),
        number_format: None,
    }
}

fn series_over(values: &[Option<&str>]) -> Series {
    Series {
        values: range(values),
        ..Series::default()
    }
}

/// A one-group chart over `series`, with a bottom category axis and a left
/// value axis — the shape every family below shares.
fn chart_of(kind: ChartGroupKind, series: Vec<Series>, blanks: DisplayBlanks) -> Chart {
    Chart {
        object: NodeId::from_parts(1, 1).unwrap(),
        coverage: ChartCoverage::Complete,
        title: None,
        auto_title_deleted: true,
        plot_area: PlotArea {
            groups: vec![ChartGroup {
                kind,
                series,
                axis_ids: vec![1, 2],
                vary_colors: false,
            }],
            axes: vec![
                Axis {
                    id: 1,
                    kind: AxisKind::Category,
                    position: Some(AxisPosition::Bottom),
                    ..Axis::default()
                },
                Axis {
                    id: 2,
                    kind: AxisKind::Value,
                    position: Some(AxisPosition::Left),
                    ..Axis::default()
                },
            ],
        },
        legend: None,
        plot_visible_only: true,
        display_blanks_as: blanks,
        vary_colors: false,
        external_data: None,
    }
}

fn line_kind() -> ChartGroupKind {
    ChartGroupKind::Line {
        grouping: Grouping::Standard,
        marker: false,
    }
}

fn column_kind(grouping: BarGrouping) -> ChartGroupKind {
    ChartGroupKind::Bar {
        direction: BarDirection::Column,
        grouping,
        gap_width: 150,
        overlap: -27,
    }
}

fn area_kind() -> ChartGroupKind {
    ChartGroupKind::Area {
        grouping: Grouping::Standard,
    }
}

/// Every open polyline (a line series), as its vertex list.
fn polylines(out: &[ChartPrimitive]) -> Vec<Vec<Point>> {
    out.iter()
        .filter_map(|primitive| match primitive {
            ChartPrimitive::Path {
                points,
                closed: false,
                ..
            } => Some(points.clone()),
            _ => None,
        })
        .collect()
}

/// Every closed polygon (an area band), as its vertex list.
fn polygons(out: &[ChartPrimitive]) -> Vec<Vec<Point>> {
    out.iter()
        .filter_map(|primitive| match primitive {
            ChartPrimitive::Path {
                points,
                closed: true,
                ..
            } => Some(points.clone()),
            _ => None,
        })
        .collect()
}

/// Every filled rectangle except the chart background, which
/// [`compose_chart`] always emits first. With no legend declared, what is left
/// is exactly the bars.
fn bars(out: &[ChartPrimitive]) -> Vec<Rect> {
    out.iter()
        .skip(1)
        .filter_map(|primitive| match primitive {
            ChartPrimitive::Rect {
                rect,
                fill: Some(_),
                ..
            } => Some(*rect),
            _ => None,
        })
        .collect()
}

/// Every series marker's centre.
fn marker_centres(out: &[ChartPrimitive]) -> Vec<Point> {
    out.iter()
        .filter_map(|primitive| match primitive {
            ChartPrimitive::Ellipse { rect, .. } => Some(Point::new(
                Twip(rect.origin.x.raw() + rect.size.width.raw() / 2),
                Twip(rect.origin.y.raw() + rect.size.height.raw() / 2),
            )),
            _ => None,
        })
        .collect()
}

fn text_count(out: &[ChartPrimitive]) -> usize {
    out.iter()
        .filter(|primitive| matches!(primitive, ChartPrimitive::Text { .. }))
        .count()
}

// ---- `c:dispBlanksAs` -------------------------------------------------
//
// The attribute was read by the importer and stored on the projection, and
// then had no effect on the page: `gap` was hard-coded at the one call site
// that consulted it, so `zero` and `span` both drew a hole. These three tests
// are the SAME chart under the three values, and they must disagree.

#[test]
fn a_gap_blank_breaks_the_series_line_in_two() {
    let chart = chart_of(
        line_kind(),
        vec![series_over(&[
            Some("1"),
            Some("2"),
            None,
            Some("3"),
            Some("4"),
        ])],
        DisplayBlanks::Gap,
    );
    let lines = polylines(&compose(&chart));
    assert_eq!(
        lines.len(),
        2,
        "`gap` leaves a hole, so the series is two polylines, got {lines:?}"
    );
    assert_eq!(
        (lines[0].len(), lines[1].len()),
        (2, 2),
        "two points before the hole and two after, got {lines:?}"
    );
}

#[test]
fn a_span_blank_carries_the_line_straight_across_the_hole() {
    let chart = chart_of(
        line_kind(),
        vec![series_over(&[
            Some("1"),
            Some("2"),
            None,
            Some("3"),
            Some("4"),
        ])],
        DisplayBlanks::Span,
    );
    let lines = polylines(&compose(&chart));
    assert_eq!(
        lines.len(),
        1,
        "`span` bridges the hole, so the series is ONE polyline, got {lines:?}"
    );
    assert_eq!(
        lines[0].len(),
        4,
        "the four cached points, with no vertex invented for the blank"
    );
}

#[test]
fn a_zero_blank_plots_the_hole_on_the_value_axis_zero() {
    let chart = chart_of(
        line_kind(),
        vec![series_over(&[
            Some("1"),
            Some("2"),
            None,
            Some("3"),
            Some("4"),
        ])],
        DisplayBlanks::Zero,
    );
    let lines = polylines(&compose(&chart));
    assert_eq!(lines.len(), 1, "`zero` is a plotted point, not a hole");
    assert_eq!(lines[0].len(), 5, "five categories, five vertices");
    // Y grows downwards, every cached value is positive and the derived scale's
    // minimum is zero, so the blank's vertex is the lowest of the five.
    let blank = lines[0][2].y.raw();
    assert!(
        lines[0].iter().all(|point| point.y.raw() <= blank),
        "the blank sits at the value-axis zero, below every real point: {:?}",
        lines[0]
    );
}

#[test]
fn a_gap_blank_leaves_a_hole_in_an_area_band_rather_than_a_shortcut() {
    // The same rule one family over. An area that carried its edges across the
    // hole would draw a straight shortcut over the missing category — which is
    // `span`'s behaviour, not `gap`'s.
    let chart = chart_of(
        area_kind(),
        vec![series_over(&[
            Some("1"),
            Some("2"),
            None,
            Some("3"),
            Some("4"),
        ])],
        DisplayBlanks::Gap,
    );
    let bands = polygons(&compose(&chart));
    assert_eq!(
        bands.len(),
        2,
        "`gap` splits the band in two, got {bands:?}"
    );
    assert!(
        bands.iter().all(|band| band.len() == 4),
        "each band closes over its own two categories, got {bands:?}"
    );
}

#[test]
fn a_span_blank_keeps_an_area_band_whole() {
    let chart = chart_of(
        area_kind(),
        vec![series_over(&[
            Some("1"),
            Some("2"),
            None,
            Some("3"),
            Some("4"),
        ])],
        DisplayBlanks::Span,
    );
    let bands = polygons(&compose(&chart));
    assert_eq!(bands.len(), 1, "`span` keeps one band, got {bands:?}");
    assert_eq!(
        bands[0].len(),
        8,
        "four upper vertices and the four under them"
    );
}

// ---- stacked geometry -------------------------------------------------

#[test]
fn a_marker_on_a_stacked_line_sits_on_the_line_it_marks() {
    // The marker drawer used to re-derive its own y from the series' RAW
    // value, ignoring the stack base the polyline was drawn against, so the
    // second series' markers floated off its own line.
    let chart = chart_of(
        ChartGroupKind::Line {
            grouping: Grouping::Stacked,
            marker: true,
        },
        vec![
            series_over(&[Some("2"), Some("2"), Some("2")]),
            series_over(&[Some("3"), Some("3"), Some("3")]),
        ],
        DisplayBlanks::Gap,
    );
    let out = compose(&chart);
    let lines = polylines(&out);
    assert_eq!(lines.len(), 2, "one polyline per series, got {lines:?}");
    let centres = marker_centres(&out);
    assert_eq!(centres.len(), 6, "three markers on each of two series");
    // Every marker centre coincides with a vertex of one of the two polylines.
    // That is the guarantee — "a marker sits on its line" — rather than the
    // mechanism, so it stays true however the stack comes to be computed.
    for centre in &centres {
        let on_a_line = lines.iter().flatten().any(|vertex| {
            (vertex.x.raw() - centre.x.raw()).abs() <= 1
                && (vertex.y.raw() - centre.y.raw()).abs() <= 1
        });
        assert!(
            on_a_line,
            "marker at {centre:?} is on neither series' line: {lines:?}"
        );
    }
    // And the two series really are stacked, so the assertion above is not
    // passing because both lines sit in the same place.
    assert_ne!(
        lines[0][0].y, lines[1][0].y,
        "the second series is stacked above the first"
    );
}

#[test]
fn a_percent_stacked_group_fills_every_category_to_the_same_total() {
    let chart = chart_of(
        column_kind(BarGrouping::PercentStacked),
        vec![
            series_over(&[Some("1"), Some("3")]),
            series_over(&[Some("3"), Some("1")]),
        ],
        DisplayBlanks::Gap,
    );
    let rects = bars(&compose(&chart));
    assert_eq!(rects.len(), 4, "two series over two categories");
    // Stacked bars in one category share a left edge, so grouping by it sorts
    // the rectangles into columns without assuming where the plot starts.
    let mut columns: BTreeMap<i32, Vec<Rect>> = BTreeMap::new();
    for rect in rects {
        columns.entry(rect.origin.x.raw()).or_default().push(rect);
    }
    let totals: Vec<i32> = columns
        .values()
        .map(|column| column.iter().map(|rect| rect.size.height.raw()).sum())
        .collect();
    assert_eq!(
        columns.values().map(Vec::len).collect::<Vec<_>>(),
        vec![2, 2],
        "two bars stacked in each of two categories"
    );
    assert!(
        (totals[0] - totals[1]).abs() <= 2,
        "`percentStacked` normalises each category to 100, so the two columns \
         are the same height despite raw values of 1:3 and 3:1, got {totals:?}"
    );
}

// ---- family coverage --------------------------------------------------

#[test]
fn a_horizontal_bar_group_grows_its_bars_sideways() {
    // `c:barDir="bar"`: the category axis runs DOWN the plot and the bars grow
    // rightwards, so the bar for the larger value is the wider one and not the
    // taller one.
    let chart = chart_of(
        ChartGroupKind::Bar {
            direction: BarDirection::Bar,
            grouping: BarGrouping::Clustered,
            gap_width: 150,
            overlap: -27,
        },
        vec![series_over(&[Some("1"), Some("4")])],
        DisplayBlanks::Gap,
    );
    let mut rects = bars(&compose(&chart));
    rects.sort_by_key(|rect| rect.origin.y.raw());
    assert_eq!(rects.len(), 2, "one bar per category");
    assert!(
        rects[1].size.width.raw() > rects[0].size.width.raw() * 3,
        "the 4 is about four times as WIDE as the 1: {rects:?}"
    );
    assert!(
        (rects[0].size.height.raw() - rects[1].size.height.raw()).abs() <= 1,
        "both bars are one category slot tall: {rects:?}"
    );
    assert!(
        (rects[0].origin.x.raw() - rects[1].origin.x.raw()).abs() <= 1,
        "and they share a left edge on the value-axis zero: {rects:?}"
    );
}

#[test]
fn a_scatter_series_plots_its_x_values_rather_than_its_point_index() {
    let mut series = series_over(&[Some("1"), Some("2"), Some("3")]);
    // x values 1, 10, 100: a renderer plotting the point INDEX would space
    // these three evenly, and not doing that is the whole point of `c:xVal`.
    series.x_values = Some(range(&[Some("1"), Some("10"), Some("100")]));
    let chart = chart_of(
        ChartGroupKind::Scatter {
            style: ScatterStyle::LineMarker,
        },
        vec![series],
        DisplayBlanks::Gap,
    );
    let lines = polylines(&compose(&chart));
    assert_eq!(lines.len(), 1, "a line-marker scatter joins its points");
    let xs: Vec<i32> = lines[0].iter().map(|point| point.x.raw()).collect();
    assert_eq!(xs.len(), 3);
    let (near, far) = (xs[1] - xs[0], xs[2] - xs[1]);
    assert!(
        far > near * 5,
        "1 -> 10 -> 100 are not evenly spaced: gaps of {near} then {far}"
    );
}

#[test]
fn a_smooth_series_is_sampled_rather_than_drawn_as_straight_segments() {
    let mut series = series_over(&[Some("1"), Some("4"), Some("2"), Some("5")]);
    series.smooth = true;
    let chart = chart_of(line_kind(), vec![series], DisplayBlanks::Gap);
    let lines = polylines(&compose(&chart));
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0].len() > 4 * SMOOTH_SAMPLES / 2,
        "`c:smooth` is sampled, not drawn as four straight segments, got {} \
         vertices",
        lines[0].len()
    );
}

#[test]
fn a_reversed_value_axis_puts_the_larger_value_lower() {
    let series = vec![series_over(&[Some("1"), Some("4")])];
    let upright = chart_of(line_kind(), series.clone(), DisplayBlanks::Gap);
    let mut reversed = chart_of(line_kind(), series, DisplayBlanks::Gap);
    reversed.plot_area.axes[1].orientation = AxisOrientation::MaxMin;
    let up = polylines(&compose(&upright))[0].clone();
    let down = polylines(&compose(&reversed))[0].clone();
    assert!(
        up[1].y.raw() < up[0].y.raw(),
        "upright: the 4 is above the 1, got {up:?}"
    );
    assert!(
        down[1].y.raw() > down[0].y.raw(),
        "`maxMin`: the 4 is BELOW the 1, got {down:?}"
    );
}

// ---- data labels, across every family that can carry them -------------

#[test]
fn every_point_family_paints_the_data_labels_its_series_asks_for() {
    // Bars and lines honoured `c:dLbls`; areas and scatters silently dropped
    // it. A capability present on one family and missing on the next is the
    // defect class `SKILL` §10 is about, so all four are asserted together and
    // a fifth family cannot quietly omit it.
    let plain = series_over(&[Some("1"), Some("2"), Some("3")]);
    let mut scatter_series = plain.clone();
    scatter_series.x_values = Some(range(&[Some("1"), Some("2"), Some("3")]));

    for (name, kind, series) in [
        ("bar", column_kind(BarGrouping::Clustered), plain.clone()),
        ("line", line_kind(), plain.clone()),
        ("area", area_kind(), plain),
        (
            "scatter",
            ChartGroupKind::Scatter {
                style: ScatterStyle::Marker,
            },
            scatter_series,
        ),
    ] {
        let mut labelled = series.clone();
        labelled.data_labels = Some(DataLabels {
            show_value: true,
            ..DataLabels::default()
        });
        let with = text_count(&compose(&chart_of(
            kind,
            vec![labelled],
            DisplayBlanks::Gap,
        )));
        let without = text_count(&compose(&chart_of(kind, vec![series], DisplayBlanks::Gap)));
        assert_eq!(
            with - without,
            3,
            "a {name} series with `c:showVal` paints one label per point: \
             {with} text primitives with labels against {without} without"
        );
    }
}

// ---- the tier boundary ------------------------------------------------

#[test]
fn a_pie_group_draws_nothing_because_there_is_no_arc_primitive() {
    // Tier 1B. Asserted here as well as at the paint tier because this is the
    // decision point: a polygon fan slipped in as "close enough" would show up
    // as a closed polygon with many vertices.
    for kind in [
        ChartGroupKind::Pie {
            first_slice_angle: 0,
        },
        ChartGroupKind::Doughnut {
            first_slice_angle: 0,
            hole_size: 50,
        },
    ] {
        assert!(!is_drawable(kind), "{kind:?} is tier 1B");
        let chart = chart_of(
            kind,
            vec![series_over(&[Some("1"), Some("2"), Some("3")])],
            DisplayBlanks::Gap,
        );
        assert!(
            !has_drawable_content(&chart),
            "{kind:?} alone yields no drawable content, so the caller keeps \
             the placeholder"
        );
        assert!(
            compose(&chart).is_empty(),
            "{kind:?} composes to nothing rather than to an empty frame"
        );
    }
}

#[test]
fn a_combo_of_a_drawable_and_a_tier_1b_group_draws_the_drawable_one() {
    // The partial case: most of a chart drawn beats a placeholder over a chart
    // we can mostly render, and the pie group must not poison it.
    let mut chart = chart_of(
        column_kind(BarGrouping::Clustered),
        vec![series_over(&[Some("1"), Some("2")])],
        DisplayBlanks::Gap,
    );
    chart.plot_area.groups.push(ChartGroup {
        kind: ChartGroupKind::Pie {
            first_slice_angle: 0,
        },
        series: vec![series_over(&[Some("5"), Some("5")])],
        axis_ids: vec![1, 2],
        vary_colors: false,
    });
    assert!(has_drawable_content(&chart));
    assert_eq!(
        bars(&compose(&chart)).len(),
        2,
        "the bar group's two bars, and nothing from the pie group"
    );
}

// ---- scale arithmetic -------------------------------------------------

#[test]
fn an_axis_rounds_outward_to_a_readable_step() {
    for (min, max, step) in [
        (0.0, 4.5, 1.0),
        (0.0, 100.0, 20.0),
        (0.0, 7.0, 2.0),
        (0.0, 0.0, 0.2),
    ] {
        let scale = nice_scale(min, max, false);
        assert!(
            (scale.step - step).abs() < 1e-9,
            "{min}..{max} wants a step of {step}, got {}",
            scale.step
        );
        assert!(
            scale.min <= min && scale.max >= max,
            "{min}..{max} must be contained by {}..{}",
            scale.min,
            scale.max
        );
    }
}

#[test]
fn a_non_finite_data_range_cannot_produce_a_non_finite_axis() {
    // The cache is producer-supplied text, so `ChartValue::as_f64` can hand
    // back anything a lexical double can express. An infinite axis would make
    // every `fraction` NaN and collapse the chart onto coordinate zero.
    let scale = nice_scale(f64::NEG_INFINITY, f64::INFINITY, false);
    assert!(scale.min.is_finite() && scale.max.is_finite() && scale.step > 0.0);
    assert!(scale.ticks().len() <= 65, "the tick loop stays bounded");
}

#[test]
fn an_axis_label_prints_a_whole_number_without_a_trailing_zero() {
    assert_eq!(format_value(4.0), "4");
    assert_eq!(format_value(4.5), "4.5");
    assert_eq!(format_value(-2.0), "-2");
    assert_eq!(format_value(f64::NAN), "");
}

// ---- complexity -------------------------------------------------------

#[test]
fn composition_cost_grows_with_the_point_count_and_not_faster() {
    // `SKILL` §8: guard the complexity by doubling n, not with a millisecond
    // threshold. `compose_chart` is documented O(points), so the primitive
    // count must roughly double when the points do — a per-point pass over
    // every other point would square it.
    let count = |n: usize| {
        let values: Vec<String> = (0..n).map(|i| format!("{}", i % 7 + 1)).collect();
        let borrowed: Vec<Option<&str>> = values.iter().map(|value| Some(value.as_str())).collect();
        compose(&chart_of(
            column_kind(BarGrouping::Clustered),
            vec![series_over(&borrowed)],
            DisplayBlanks::Gap,
        ))
        .len()
    };
    let (n, two_n) = (64, 128);
    let (small, large) = (count(n), count(two_n));
    let growth = large as f64 / small as f64;
    assert!(
        (1.5..=3.0).contains(&growth),
        "doubling {n} points to {two_n} must roughly double the primitives \
         ({small} -> {large}, x{growth:.2}); a quadratic would be x4 or worse"
    );
}
