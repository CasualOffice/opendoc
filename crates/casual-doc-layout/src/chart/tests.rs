// SPDX-License-Identifier: Apache-2.0

//! Chart formatting guards (`docs/155` §19): fonts, axis titles, dashes,
//! number formats, trendlines and error bars, on synthetic charts built here.

use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    ChartCoverage, ChartText, ChartTitle, DataLabels, ErrorBarDirection, ErrorBarType, ErrorBars,
    ErrorValueType, Legend, PlotArea, RgbColor, Trendline, TrendlineKind,
};

use super::analysis::{self, Fit};
use super::*;
use crate::text::{Decoration, FontId, Glyph, GlyphRun};

/// Font-id bit the test shaper sets for bold, so a test can read it back.
const BOLD_BIT: u32 = 1;
/// Font-id bit for italic.
const ITALIC_BIT: u32 = 2;

/// A deterministic shaper: one glyph per char (glyph id = the char, so the
/// text can be read back), an advance of half an em, ascent 0.8 em, descent
/// 0.2 em, weight and slant in the font id. Every call is logged.
fn fake_label(text: &str, style: &ChartTextStyle) -> Option<ChartLabel> {
    let em = (style.size / 5) as i32; // hundredths of a point -> twips
    let advance = Twip((em / 2).max(1));
    let glyphs: Vec<Glyph> = text
        .chars()
        .enumerate()
        .map(|(index, ch)| Glyph {
            id: ch as u32,
            advance,
            cluster: index as u32,
            is_whitespace: ch == ' ',
        })
        .collect();
    let width = Twip(advance.raw() * glyphs.len() as i32);
    let ascent = Twip(em * 4 / 5);
    let descent = Twip(em / 5);
    Some(ChartLabel {
        runs: vec![GlyphRun {
            is_marker: false,
            is_leader: false,
            node: None,
            font: FontId(
                if style.bold { BOLD_BIT } else { 0 } | if style.italic { ITALIC_BIT } else { 0 },
            ),
            size: Twip(em),
            ascent,
            descent,
            character_scale_percent: 100,
            // Deliberately NOT the style colour: the chart must paint it.
            color: [1, 2, 3, 255],
            origin: Point::new(Twip::ZERO, Twip::ZERO),
            bidi_level: 0,
            decoration: Decoration::default(),
            highlight: None,
            shading: None,
            glyphs,
        }],
        width,
        ascent,
        descent,
    })
}

/// The text a run spells (glyph ids are chars under [`fake_label`]).
fn run_text(run: &GlyphRun) -> String {
    run.glyphs
        .iter()
        .filter_map(|glyph| char::from_u32(glyph.id))
        .collect()
}

fn number_range(values: &[&str]) -> DataRange {
    DataRange {
        formula: None,
        point_count: values.len() as u32,
        points: values
            .iter()
            .enumerate()
            .map(|(i, v)| (i as u32, ChartValue::Number((*v).to_owned())))
            .collect(),
        number_format: None,
    }
}

fn series(values: &[&str]) -> Series {
    Series {
        values: number_range(values),
        ..Series::default()
    }
}

fn title(text: &str) -> ChartTitle {
    ChartTitle {
        text: Some(ChartText {
            text: text.to_owned(),
            formula: None,
        }),
        ..ChartTitle::default()
    }
}

/// A chart of one group over a bottom category axis (id 1) and a left value
/// axis (id 2).
fn chart(kind: ChartGroupKind, series: Vec<Series>) -> Chart {
    Chart {
        font: None,
        chart_retained: Default::default(),
        namespaces: Default::default(),
        space_retained: Default::default(),
        object: NodeId::from_parts(1, 1).unwrap(),
        coverage: ChartCoverage::Partial,
        title: None,
        auto_title_deleted: false,
        plot_area: PlotArea {
            retained: Default::default(),
            groups: vec![ChartGroup {
                retained: Default::default(),
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
        display_blanks_as: DisplayBlanks::Gap,
        vary_colors: false,
        external_data: None,
        dirty: false,
    }
}

const COLUMN: ChartGroupKind = ChartGroupKind::Bar {
    direction: BarDirection::Column,
    grouping: BarGrouping::Clustered,
    gap_width: 150,
    overlap: 0,
};

const LINE: ChartGroupKind = ChartGroupKind::Line {
    grouping: Grouping::Standard,
    marker: false,
};

const SCATTER: ChartGroupKind = ChartGroupKind::Scatter {
    style: ScatterStyle::LineMarker,
};

/// A scatter chart: x on axis 1 (bottom), y on axis 2 (left).
fn scatter(points: &[(&str, &str)]) -> Chart {
    let mut chart = chart(
        SCATTER,
        vec![Series {
            x_values: Some(number_range(
                &points.iter().map(|(x, _)| *x).collect::<Vec<_>>(),
            )),
            values: number_range(&points.iter().map(|(_, y)| *y).collect::<Vec<_>>()),
            ..Series::default()
        }],
    );
    chart.plot_area.axes[0].kind = AxisKind::Value;
    chart
}

const BOX: Size = Size {
    width: Twip(8_640),
    height: Twip(5_040),
};

/// Composes `chart` into the standard box with the test shaper; returns the
/// primitives and every (text, style) the shaper was asked for.
fn draw(chart: &Chart) -> (Vec<ChartPrimitive>, Vec<(String, ChartTextStyle)>) {
    let mut log = Vec::new();
    let mut shape = |text: &str, style: &ChartTextStyle| {
        log.push((text.to_owned(), style.clone()));
        fake_label(text, style)
    };
    let primitives = compose_chart(
        chart,
        BOX,
        &ChartStyle::default(),
        &resolve_literal_color,
        &mut shape,
    );
    (primitives, log)
}

/// Every upright text run, with its text.
fn texts(primitives: &[ChartPrimitive]) -> Vec<(String, GlyphRun)> {
    primitives
        .iter()
        .filter_map(|primitive| match primitive {
            ChartPrimitive::Text { run } => Some((run_text(run), run.clone())),
            _ => None,
        })
        .collect()
}

/// The x of the left value axis line (the plot's left edge).
fn left_axis_x(primitives: &[ChartPrimitive]) -> i32 {
    primitives
        .iter()
        .find_map(|primitive| match primitive {
            ChartPrimitive::Line { from, to, stroke }
                if from.x == to.x
                    && stroke.color == AXIS_LINE
                    && to.y.raw() - from.y.raw() > 1000 =>
            {
                Some(from.x.raw())
            }
            _ => None,
        })
        .expect("a left axis line")
}

/// The y of the bottom category axis line (the plot's bottom edge).
fn bottom_axis_y(primitives: &[ChartPrimitive]) -> i32 {
    primitives
        .iter()
        .find_map(|primitive| match primitive {
            ChartPrimitive::Line { from, to, stroke }
                if from.y == to.y
                    && stroke.color == AXIS_LINE
                    && to.x.raw() - from.x.raw() > 1000 =>
            {
                Some(from.y.raw())
            }
            _ => None,
        })
        .expect("a bottom axis line")
}

/// Every stroked path's stroke, in paint order.
fn path_strokes(primitives: &[ChartPrimitive]) -> Vec<(ChartStroke, Vec<Point>)> {
    primitives
        .iter()
        .filter_map(|primitive| match primitive {
            ChartPrimitive::Path {
                commands,
                stroke: Some(stroke),
                fill: None,
                ..
            } => Some((
                *stroke,
                commands
                    .iter()
                    .filter_map(|command| match command {
                        crate::display::PathCommand::MoveTo { point }
                        | crate::display::PathCommand::LineTo { point } => Some(*point),
                        _ => None,
                    })
                    .collect(),
            )),
            _ => None,
        })
        .collect()
}

// ------------------------------------------------------------------ fonts

#[test]
fn an_axis_font_size_changes_its_tick_labels_and_the_gutter_they_reserve() {
    let base = chart(COLUMN, vec![series(&["10", "20", "30"])]);
    let (plain, _) = draw(&base);
    let mut big = base.clone();
    big.plot_area.axes[1].font = Some(ChartFont {
        size: Some(1_800),
        ..ChartFont::default()
    });
    let (large, log) = draw(&big);

    let tick = |primitives: &[ChartPrimitive]| {
        texts(primitives)
            .into_iter()
            .find(|(text, _)| text == "30")
            .expect("a 30 tick label")
            .1
    };
    assert_eq!(tick(&plain).size, Twip(180), "9 pt default");
    assert_eq!(tick(&large).size, Twip(360), "18 pt from the axis font");
    assert!(
        left_axis_x(&large) > left_axis_x(&plain),
        "wider labels reserve a wider gutter: {} vs {}",
        left_axis_x(&large),
        left_axis_x(&plain)
    );
    // The category axis did not ask for 18 pt and does not get it.
    let category = log
        .iter()
        .find(|(text, _)| text == "1")
        .expect("category 1");
    assert_eq!(category.1.size, LABEL_TEXT_SIZE);
}

#[test]
fn bold_italic_and_colour_reach_the_glyph_run_and_the_title_defaults_to_14_pt() {
    let mut chart = chart(COLUMN, vec![series(&["1", "2"])]);
    chart.title = Some(ChartTitle {
        font: Some(ChartFont {
            bold: Some(true),
            color: Some(Color::Rgb(RgbColor { r: 200, g: 0, b: 0 })),
            ..ChartFont::default()
        }),
        ..title("Revenue")
    });
    chart.font = Some(ChartFont {
        italic: Some(true),
        typeface: Some("+mj-lt".to_owned()),
        ..ChartFont::default()
    });
    let (primitives, log) = draw(&chart);
    let (_, run) = texts(&primitives)
        .into_iter()
        .find(|(text, _)| text == "Revenue")
        .expect("the title is painted");
    assert_eq!(
        run.font,
        FontId(BOLD_BIT | ITALIC_BIT),
        "bold + inherited italic"
    );
    assert_eq!(run.color, [200, 0, 0, 255], "the title's own colour");
    assert_eq!(run.size, Twip(280), "14 pt default title");
    let (_, style) = log.iter().find(|(text, _)| text == "Revenue").unwrap();
    assert_eq!(
        style.typeface.as_deref(),
        Some("+mj-lt"),
        "the chart-space theme typeface is handed to the shaper"
    );
    // A tick label inherits the chart's italic but not the title's bold or red.
    let (_, tick) = texts(&primitives)
        .into_iter()
        .find(|(text, _)| text == "2")
        .expect("a tick label");
    assert_eq!(tick.font, FontId(ITALIC_BIT));
    assert_eq!(tick.color, ChartStyle::default().text);
}

#[test]
fn the_legend_takes_its_own_font_over_the_charts() {
    let mut chart = chart(COLUMN, vec![series(&["1", "2"])]);
    chart.plot_area.groups[0].series[0].name = Some(ChartText {
        text: "North".to_owned(),
        formula: None,
    });
    chart.legend = Some(Legend {
        font: Some(ChartFont {
            size: Some(1_200),
            ..ChartFont::default()
        }),
        ..Legend::default()
    });
    chart.font = Some(ChartFont {
        bold: Some(true),
        ..ChartFont::default()
    });
    let (primitives, _) = draw(&chart);
    let (_, run) = texts(&primitives)
        .into_iter()
        .find(|(text, _)| text == "North")
        .expect("legend entry");
    assert_eq!(run.size, Twip(240));
    assert_eq!(run.font, FontId(BOLD_BIT));
}

// ------------------------------------------------------------- axis titles

#[test]
fn axis_titles_are_placed_and_reserved_and_a_vertical_one_is_turned() {
    let base = chart(COLUMN, vec![series(&["1", "2", "3"])]);
    let (plain, _) = draw(&base);
    let mut titled = base.clone();
    titled.plot_area.axes[0].title = Some(title("Quarter"));
    titled.plot_area.axes[1].title = Some(title("Units"));
    let (primitives, log) = draw(&titled);

    let (_, horizontal) = texts(&primitives)
        .into_iter()
        .find(|(text, _)| text == "Quarter")
        .expect("the category-axis title is upright text");
    assert!(
        horizontal.origin.y.raw() > bottom_axis_y(&primitives),
        "below the axis"
    );
    assert!(
        bottom_axis_y(&primitives) < bottom_axis_y(&plain),
        "the plot shrank to make room for it"
    );
    assert!(
        left_axis_x(&primitives) > left_axis_x(&plain),
        "the vertical title reserves width"
    );
    let rotated: Vec<_> = primitives
        .iter()
        .filter_map(|primitive| match primitive {
            ChartPrimitive::RotatedText {
                runs,
                center,
                quarter_turns,
            } => Some((runs.clone(), *center, *quarter_turns)),
            _ => None,
        })
        .collect();
    assert_eq!(rotated.len(), 1, "one rotated title: {rotated:?}");
    let (runs, center, turns) = &rotated[0];
    assert_eq!(*turns, -1, "a left axis title reads bottom-to-top");
    assert_eq!(run_text(&runs[0]), "Units");
    assert!(center.x.raw() < left_axis_x(&primitives));
    assert!(
        !texts(&primitives).iter().any(|(text, _)| text == "Units"),
        "the vertical title is not ALSO painted upright"
    );
    let (_, style) = log.iter().find(|(text, _)| text == "Units").unwrap();
    assert_eq!(style.size, AXIS_TITLE_TEXT_SIZE);
}

#[test]
fn a_right_hand_axis_title_turns_the_other_way() {
    let mut chart = chart(COLUMN, vec![series(&["1", "2"])]);
    chart.plot_area.axes[1].position = Some(AxisPosition::Right);
    chart.plot_area.axes[1].title = Some(title("Secondary"));
    let (primitives, _) = draw(&chart);
    let turns: Vec<i8> = primitives
        .iter()
        .filter_map(|primitive| match primitive {
            ChartPrimitive::RotatedText { quarter_turns, .. } => Some(*quarter_turns),
            _ => None,
        })
        .collect();
    assert_eq!(turns, vec![1]);
}

#[test]
fn a_deleted_axis_draws_no_title() {
    let mut chart = chart(COLUMN, vec![series(&["1", "2"])]);
    chart.plot_area.axes[1].title = Some(title("Units"));
    chart.plot_area.axes[1].deleted = true;
    let (primitives, log) = draw(&chart);
    assert!(!log.iter().any(|(text, _)| text == "Units"));
    assert!(
        !primitives
            .iter()
            .any(|primitive| matches!(primitive, ChartPrimitive::RotatedText { .. }))
    );
}

// ------------------------------------------------------------------ dashes

#[test]
fn a_dashed_series_line_carries_its_dash() {
    let mut chart = chart(LINE, vec![series(&["1", "3", "2"])]);
    chart.plot_area.groups[0].series[0].line = Some(ChartLine {
        dash: Some(DashStyle::Dash),
        ..ChartLine::default()
    });
    let (primitives, _) = draw(&chart);
    let strokes = path_strokes(&primitives);
    assert_eq!(strokes.len(), 1, "{strokes:?}");
    assert_eq!(strokes[0].0.dash, DashStyle::Dash);
    // Furniture stays solid.
    assert!(primitives.iter().all(|primitive| match primitive {
        ChartPrimitive::Line { stroke, .. } => stroke.dash == DashStyle::Solid,
        _ => true,
    }));
}

// ---------------------------------------------------------- number formats

#[test]
fn value_axis_labels_use_the_axis_format_then_the_series_format() {
    let mut chart = chart(COLUMN, vec![series(&["1000", "2500"])]);
    chart.plot_area.axes[1].number_format = Some("$#,##0".to_owned());
    let (primitives, _) = draw(&chart);
    let labels: Vec<String> = texts(&primitives).into_iter().map(|(t, _)| t).collect();
    assert!(labels.contains(&"$2,500".to_owned()), "{labels:?}");

    // General on the axis: the series' cached formatCode is used (sourceLinked).
    chart.plot_area.axes[1].number_format = Some("General".to_owned());
    chart.plot_area.groups[0].series[0].values.number_format = Some("0.0".to_owned());
    let (primitives, _) = draw(&chart);
    let labels: Vec<String> = texts(&primitives).into_iter().map(|(t, _)| t).collect();
    assert!(labels.contains(&"2500.0".to_owned()), "{labels:?}");
}

#[test]
fn data_labels_use_the_series_format() {
    let mut chart = chart(COLUMN, vec![series(&["0.25", "0.5"])]);
    chart.plot_area.groups[0].series[0].values.number_format = Some("0%".to_owned());
    chart.plot_area.groups[0].series[0].data_labels = Some(DataLabels {
        show_value: true,
        ..DataLabels::default()
    });
    let (primitives, _) = draw(&chart);
    let labels: Vec<String> = texts(&primitives).into_iter().map(|(t, _)| t).collect();
    assert!(labels.contains(&"25%".to_owned()), "{labels:?}");
    assert!(labels.contains(&"50%".to_owned()), "{labels:?}");
}

#[test]
fn a_percent_stacked_axis_prints_percent() {
    let chart = chart(
        ChartGroupKind::Bar {
            direction: BarDirection::Column,
            grouping: BarGrouping::PercentStacked,
            gap_width: 150,
            overlap: 100,
        },
        vec![series(&["1", "3"]), series(&["3", "1"])],
    );
    let (primitives, _) = draw(&chart);
    let labels: Vec<String> = texts(&primitives).into_iter().map(|(t, _)| t).collect();
    assert!(labels.contains(&"100%".to_owned()), "{labels:?}");
    assert!(labels.contains(&"0%".to_owned()), "{labels:?}");
}

// -------------------------------------------------------------- trendlines

fn points(xs: &[f64], f: impl Fn(f64) -> f64) -> Vec<(f64, f64)> {
    xs.iter().map(|x| (*x, f(*x))).collect()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6 * b.abs().max(1.0)
}

#[test]
fn a_linear_fit_through_known_points_hits_the_expected_value() {
    let data = points(&[1.0, 2.0, 3.0, 4.0], |x| 2.0 * x + 1.0);
    let result = analysis::fit(TrendlineKind::Linear, 2, None, &data).expect("fits");
    assert!(close(result.fit.eval(5.0).unwrap(), 11.0), "{result:?}");
    assert!(close(result.r_squared, 1.0));
    assert_eq!(result.fit.equation(), "y = 2x + 1");
    // A fixed intercept is honoured.
    let pinned = analysis::fit(TrendlineKind::Linear, 2, Some(0.0), &data).unwrap();
    assert!(close(pinned.fit.eval(0.0).unwrap(), 0.0), "{pinned:?}");
}

#[test]
fn a_second_order_polynomial_fits_a_parabola_exactly() {
    let data = points(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], |x| {
        3.0 * x * x - 2.0 * x + 1.0
    });
    let result = analysis::fit(TrendlineKind::Polynomial, 2, None, &data).expect("fits");
    let Fit::Polynomial { coef, .. } = &result.fit else {
        panic!("{result:?}")
    };
    assert!(
        close(coef[0], 1.0) && close(coef[1], -2.0) && close(coef[2], 3.0),
        "{coef:?}"
    );
    assert!(close(result.r_squared, 1.0));
    assert_eq!(result.fit.equation(), "y = 3x² - 2x + 1");
    // Order 6 over wide x stays solvable (scaled normal equations).
    let wide = points(
        &[
            1000.0, 1100.0, 1200.0, 1300.0, 1400.0, 1500.0, 1600.0, 1700.0,
        ],
        |x| x * 0.5,
    );
    assert!(analysis::fit(TrendlineKind::Polynomial, 6, None, &wide).is_some());
    // Too few distinct points: singular, skipped.
    let flat = [(1.0, 1.0), (1.0, 2.0), (1.0, 3.0)];
    assert!(analysis::fit(TrendlineKind::Polynomial, 2, None, &flat).is_none());
}

#[test]
fn exponential_logarithmic_and_power_fits_recover_their_curves() {
    let xs = [1.0, 2.0, 3.0, 4.0, 5.0];
    let exp = analysis::fit(
        TrendlineKind::Exponential,
        2,
        None,
        &points(&xs, |x| 2.0 * (0.3 * x).exp()),
    )
    .unwrap();
    assert!(
        close(exp.fit.eval(6.0).unwrap(), 2.0 * 1.8f64.exp()),
        "{exp:?}"
    );
    let log = analysis::fit(
        TrendlineKind::Logarithmic,
        2,
        None,
        &points(&xs, |x| 1.5 * x.ln() + 2.0),
    )
    .unwrap();
    assert!(close(log.fit.eval(10.0).unwrap(), 1.5 * 10f64.ln() + 2.0));
    let power = analysis::fit(
        TrendlineKind::Power,
        2,
        None,
        &points(&xs, |x| 3.0 * x.powf(0.5)),
    )
    .unwrap();
    assert!(close(power.fit.eval(9.0).unwrap(), 9.0));
}

#[test]
fn an_exponential_trendline_over_a_non_positive_value_is_skipped() {
    let data = [(1.0, 1.0), (2.0, 0.0), (3.0, 4.0)];
    assert!(analysis::fit(TrendlineKind::Exponential, 2, None, &data).is_none());
    let negative = [(1.0, 1.0), (2.0, -2.0), (3.0, 4.0)];
    assert!(analysis::fit(TrendlineKind::Exponential, 2, None, &negative).is_none());
    assert!(analysis::fit(TrendlineKind::Power, 2, None, &negative).is_none());
    let log_x = [(-1.0, 1.0), (2.0, 2.0), (3.0, 4.0)];
    assert!(analysis::fit(TrendlineKind::Logarithmic, 2, None, &log_x).is_none());
    let mut chart = chart(COLUMN, vec![series(&["1", "0", "4"])]);
    chart.plot_area.groups[0].series[0].trendlines = vec![Trendline {
        kind: TrendlineKind::Exponential,
        ..Trendline::default()
    }];
    let (primitives, _) = draw(&chart);
    assert!(
        path_strokes(&primitives).is_empty(),
        "no trendline is drawn for an undefined fit"
    );
}

#[test]
fn the_solver_pivots_past_a_zero_leading_entry() {
    // x = 3, y = 2, written so the first pivot is zero: elimination without
    // row exchange divides by it.
    let mut system = vec![vec![0.0, 1.0, 2.0], vec![1.0, 0.0, 3.0]];
    let solution = analysis::solve(&mut system).expect("solvable with pivoting");
    assert!(
        close(solution[0], 3.0) && close(solution[1], 2.0),
        "{solution:?}"
    );
    let mut singular = vec![vec![1.0, 2.0, 1.0], vec![2.0, 4.0, 2.0]];
    assert!(analysis::solve(&mut singular).is_none());
}

#[test]
fn a_moving_average_has_one_point_per_window() {
    let data = points(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], |x| x * 2.0);
    let average = analysis::moving_average(&data, 3);
    assert_eq!(average.len(), 4);
    assert_eq!(average[0], (3.0, 4.0));
    assert_eq!(average[3], (6.0, 10.0));
    assert!(analysis::moving_average(&data, 10).is_empty());
}

#[test]
fn a_trendline_is_drawn_dotted_in_the_series_colour_with_its_labels() {
    let mut chart = chart(LINE, vec![series(&["3", "5", "7", "9"])]);
    chart.plot_area.groups[0].series[0].trendlines = vec![Trendline {
        kind: TrendlineKind::Linear,
        display_equation: true,
        display_r_squared: true,
        ..Trendline::default()
    }];
    let (primitives, _) = draw(&chart);
    let strokes = path_strokes(&primitives);
    let trend: Vec<_> = strokes
        .iter()
        .filter(|(stroke, _)| stroke.dash == DashStyle::SystemDot)
        .collect();
    assert_eq!(trend.len(), 1, "{strokes:?}");
    let series_line = strokes
        .iter()
        .find(|(stroke, _)| stroke.dash == DashStyle::Solid)
        .expect("the series line");
    assert_eq!(trend[0].0.color, series_line.0.color);
    // y = 2x + 1 over categories 1..4 passes exactly through every data point,
    // so its ends coincide with the series line's ends.
    let (line_points, trend_points) = (&series_line.1, &trend[0].1);
    assert!((trend_points[0].y.raw() - line_points[0].y.raw()).abs() <= 2);
    assert!((trend_points[1].y.raw() - line_points[3].y.raw()).abs() <= 2);
    let labels: Vec<String> = texts(&primitives).into_iter().map(|(t, _)| t).collect();
    assert!(labels.contains(&"y = 2x + 1".to_owned()), "{labels:?}");
    assert!(labels.contains(&"R² = 1.0000".to_owned()), "{labels:?}");
}

#[test]
fn a_forward_projection_extends_the_trendline() {
    let mut chart = scatter(&[("1", "1"), ("2", "2"), ("3", "3"), ("10", "10")]);
    let line = Trendline {
        kind: TrendlineKind::Linear,
        ..Trendline::default()
    };
    chart.plot_area.groups[0].series[0].trendlines = vec![line.clone()];
    let (plain, _) = draw(&chart);
    chart.plot_area.groups[0].series[0].trendlines = vec![Trendline {
        backward: Some("0.5".to_owned()),
        ..line
    }];
    let (projected, _) = draw(&chart);
    let start = |primitives: &[ChartPrimitive]| {
        path_strokes(primitives)
            .into_iter()
            .find(|(stroke, _)| stroke.dash == DashStyle::SystemDot)
            .expect("trendline")
            .1[0]
            .x
            .raw()
    };
    assert!(start(&projected) < start(&plain));
}

// -------------------------------------------------------------- error bars

fn bars(value_type: ErrorValueType, value: &str) -> ErrorBars {
    ErrorBars {
        value_type,
        value: Some(value.to_owned()),
        ..ErrorBars::default()
    }
}

fn indexed(values: &[f64]) -> Vec<(usize, f64)> {
    values.iter().copied().enumerate().collect()
}

#[test]
fn error_bar_lengths_follow_each_value_type() {
    let values = indexed(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]);
    let spans = analysis::error_spans(&bars(ErrorValueType::FixedValue, "1.5"), &values);
    assert!(close(spans[0].low, 0.5) && close(spans[0].high, 3.5));

    let spans = analysis::error_spans(&bars(ErrorValueType::Percentage, "10"), &values);
    assert!(
        close(spans[7].low, 8.1) && close(spans[7].high, 9.9),
        "{spans:?}"
    );

    // Sample standard deviation of the series is sqrt(32/7); Excel centres the
    // bar on the MEAN (5), the same span at every point.
    let deviation = (32.0f64 / 7.0).sqrt();
    let spans = analysis::error_spans(&bars(ErrorValueType::StandardDeviation, "2"), &values);
    for span in &spans {
        assert!(close(span.low, 5.0 - 2.0 * deviation), "{span:?}");
        assert!(close(span.high, 5.0 + 2.0 * deviation), "{span:?}");
    }

    let error = (32.0f64 / 56.0).sqrt();
    let spans = analysis::error_spans(&bars(ErrorValueType::StandardError, ""), &values);
    assert!(close(spans[1].low, 4.0 - error) && close(spans[1].high, 4.0 + error));

    let custom = ErrorBars {
        value_type: ErrorValueType::Custom,
        plus: Some(number_range(&["1", "2"])),
        minus: Some(number_range(&["0.5"])),
        ..ErrorBars::default()
    };
    let spans = analysis::error_spans(&custom, &indexed(&[10.0, 20.0]));
    assert!(close(spans[0].low, 9.5) && close(spans[0].high, 11.0));
    assert!(
        close(spans[1].low, 20.0) && close(spans[1].high, 22.0),
        "missing minus is 0"
    );

    let plus_only = ErrorBars {
        bar_type: ErrorBarType::Plus,
        ..bars(ErrorValueType::FixedValue, "1")
    };
    let spans = analysis::error_spans(&plus_only, &indexed(&[3.0]));
    assert!(close(spans[0].low, 3.0) && close(spans[0].high, 4.0));
    assert!(!spans[0].low_drawn && spans[0].high_drawn);
    let minus_only = ErrorBars {
        bar_type: ErrorBarType::Minus,
        ..bars(ErrorValueType::FixedValue, "1")
    };
    let spans = analysis::error_spans(&minus_only, &indexed(&[3.0]));
    assert!(close(spans[0].low, 2.0) && close(spans[0].high, 3.0));
}

#[test]
fn error_bars_draw_a_bar_and_two_caps_per_point_unless_capless() {
    let error_paths = |chart: &Chart| {
        path_strokes(&draw(chart).0)
            .into_iter()
            .filter(|(stroke, _)| stroke.color == [0x59, 0x59, 0x59, 0xFF])
            .collect::<Vec<_>>()
    };
    let mut chart = chart(COLUMN, vec![series(&["10", "20", "21"])]);
    chart.plot_area.groups[0].series[0].error_bars = vec![bars(ErrorValueType::FixedValue, "2")];
    let drawn = error_paths(&chart);
    assert_eq!(drawn.len(), 9, "3 bars + 6 caps: {drawn:?}");
    // A column's bar is vertical, its caps horizontal.
    assert_eq!(drawn[0].1[0].x, drawn[0].1[1].x);
    assert_eq!(drawn[1].1[0].y, drawn[1].1[1].y);
    chart.plot_area.groups[0].series[0].error_bars[0].no_end_cap = true;
    assert_eq!(error_paths(&chart).len(), 3);
}

#[test]
fn x_error_bars_run_horizontally_on_a_scatter_series_only() {
    let mut chart = scatter(&[("1", "1"), ("2", "4"), ("3", "9")]);
    chart.plot_area.groups[0].series[0].error_bars = vec![ErrorBars {
        direction: Some(ErrorBarDirection::X),
        no_end_cap: true,
        ..bars(ErrorValueType::FixedValue, "0.5")
    }];
    let (primitives, _) = draw(&chart);
    let drawn: Vec<_> = path_strokes(&primitives)
        .into_iter()
        .filter(|(stroke, _)| stroke.color == [0x59, 0x59, 0x59, 0xFF])
        .collect();
    assert_eq!(drawn.len(), 3);
    assert!(drawn.iter().all(|(_, points)| points[0].y == points[1].y));

    let mut column = self::chart(COLUMN, vec![series(&["1", "2"])]);
    column.plot_area.groups[0].series[0].error_bars =
        chart.plot_area.groups[0].series[0].error_bars.clone();
    let (primitives, _) = draw(&column);
    assert!(
        !path_strokes(&primitives)
            .iter()
            .any(|(stroke, _)| stroke.color == [0x59, 0x59, 0x59, 0xFF]),
        "a column series has no x value to put an x error bar on"
    );
}

// --------------------------------------------------------------- degenerate

#[test]
fn degenerate_series_never_panic() {
    let every_trend = [
        TrendlineKind::Linear,
        TrendlineKind::Exponential,
        TrendlineKind::Logarithmic,
        TrendlineKind::Polynomial,
        TrendlineKind::Power,
        TrendlineKind::MovingAverage,
    ];
    let every_error = [
        ErrorValueType::Custom,
        ErrorValueType::FixedValue,
        ErrorValueType::Percentage,
        ErrorValueType::StandardDeviation,
        ErrorValueType::StandardError,
    ];
    let datasets: [&[&str]; 5] = [
        &[],
        &["NaN", "abc", "1e400"],
        &["5"],
        &["0", "0", "0"],
        &["-1", "1e300", "-1e300", "2"],
    ];
    for kind in [COLUMN, LINE, SCATTER] {
        for values in datasets {
            let mut series = series(values);
            series.trendlines = every_trend
                .iter()
                .map(|kind| Trendline {
                    kind: *kind,
                    order: Some(6),
                    period: Some(255),
                    forward: Some("1e308".to_owned()),
                    backward: Some("NaN".to_owned()),
                    intercept: Some("-3".to_owned()),
                    display_equation: true,
                    display_r_squared: true,
                    ..Trendline::default()
                })
                .collect();
            series.error_bars = every_error
                .iter()
                .map(|value_type| ErrorBars {
                    value_type: *value_type,
                    value: Some("1e308".to_owned()),
                    plus: Some(number_range(&["NaN"])),
                    ..ErrorBars::default()
                })
                .collect();
            series.values.number_format = Some("[$€-x]0.00E+00;;".to_owned());
            let mut chart = chart(kind, vec![series]);
            chart.plot_area.axes[1].title = Some(title(""));
            chart.plot_area.axes[0].title = Some(title("x"));
            let (primitives, _) = draw(&chart);
            // Every emitted path is finite and inside the box (clipped).
            for (_, points) in path_strokes(&primitives) {
                for point in points {
                    assert!(point.x.raw() >= 0 && point.x.raw() <= BOX.width.raw());
                    assert!(point.y.raw() >= 0 && point.y.raw() <= BOX.height.raw());
                }
            }
        }
    }
}
