// SPDX-License-Identifier: Apache-2.0

//! Trendlines and error bars — the series analysis Word draws over a chart's
//! series (`c:trendline`, `c:errBars`; `docs/155` §19).
//!
//! # What is drawn
//!
//! - **Trendlines**: linear, exponential, logarithmic, power and polynomial
//!   (order 2..6) least-squares fits, and a moving average, over the series'
//!   plotted points — `x` is the 1-based category index on a category chart (what
//!   Excel regresses against) and the `c:xVal` on a scatter chart. Forward and
//!   backward projection and a fixed intercept (linear, exponential, polynomial)
//!   are honoured. The line is sampled and clipped to the plot rectangle;
//!   `c:dispEq` / `c:dispRSqr` print the equation and R² beside the line's end.
//! - **Error bars**: fixed value, percentage, standard deviation (centred on the
//!   series mean, as Excel draws it), standard error and custom plus/minus
//!   ranges; both/plus/minus; end caps unless `c:noEndCap`; `x`-direction bars on
//!   a scatter series.
//!
//! # What is not
//!
//! Stacked groups draw neither: Excel offers no trendline on a stacked chart, and
//! a stacked error bar would have to ride the stack — reported as a gap, not
//! drawn wrong. Pie and doughnut have no axes to regress against. A fit that is
//! undefined for the data (an exponential over a non-positive value, a
//! logarithm over a non-positive `x`, a singular polynomial system) is skipped,
//! which is what Excel does — it refuses the trendline rather than drawing a
//! wrong one.
//!
//! # Complexity
//!
//! O(points) per trendline or error-bar set, except a polynomial fit, which is
//! O(points x order + order³) with order at most 6. A trendline is drawn from at
//! most [`TREND_SAMPLES`] samples whatever the data size. Nothing here is
//! per-document work.

use casual_doc_model::v1::{
    ErrorBarDirection, ErrorBarType, ErrorBars, ErrorValueType, Trendline, TrendlineKind,
};

use super::{
    ChartGroup, ChartGroupKind, ChartLabel, ChartLine, ChartPrimitive, ChartStroke, DashStyle,
    GroupGeometry, LabelShaper, Paint, Point, Rect, Scale, Series, Twip, dense_numbers,
    group_is_stacked, line_width, parse_axis_bound, place_label, range_len, round_angle,
    scatter_x_scale, shape_colored,
};

/// Samples a curved trendline is drawn from, across its whole domain.
pub(super) const TREND_SAMPLES: usize = 64;

/// Word's default trendline weight (`a:ln@w="19050"`, 1.5 pt).
const TRENDLINE_WIDTH: Twip = Twip(30);

/// Word's default error-bar weight (`a:ln@w="9525"`, 0.75 pt).
const ERROR_BAR_WIDTH: Twip = Twip(15);

/// Word's default error-bar colour: `tx1` at 65% luminance, a dark grey.
const ERROR_BAR_COLOR: [u8; 4] = [0x59, 0x59, 0x59, 0xFF];

/// Half the length of an error bar's end cap (the cap is ~5 pt across).
const CAP_HALF: f64 = 50.0;

/// Gap (twips) between a trendline's end and its equation label.
const TREND_LABEL_GAP: i32 = 40;

/// The largest projection honoured, as a multiple of the data's own span —
/// a hostile `c:forward="1e300"` cannot push the samples to infinity.
const MAX_PROJECTION_SPANS: f64 = 1_000.0;

/// Draws every trendline and error-bar set of `group`'s series.
///
/// Complexity: see the module documentation.
pub(super) fn draw_series_analysis(
    group: &ChartGroup,
    geometry: &GroupGeometry,
    paint: &Paint<'_>,
    shape: &mut LabelShaper<'_>,
    out: &mut Vec<ChartPrimitive>,
) {
    let Some(map) = PlotMap::for_group(group, geometry) else {
        return;
    };
    for (index, series) in group.series.iter().enumerate() {
        if series.trendlines.is_empty() && series.error_bars.is_empty() {
            continue;
        }
        let indexed = series_points(group, geometry, series);
        let points: Vec<(f64, f64)> = indexed.iter().map(|(_, point)| *point).collect();
        let color = paint.line(series, geometry.first_color + index);
        for trendline in &series.trendlines {
            draw_trendline(trendline, &points, &map, color, paint, shape, out);
        }
        for bars in &series.error_bars {
            draw_error_bars(bars, &indexed, &map, paint, out);
        }
    }
}

/// How data coordinates map onto the plot rectangle for one group.
#[derive(Clone, Copy)]
pub(super) struct PlotMap {
    plot: Rect,
    x: XMap,
    y: Scale,
}

/// The data-x mapping.
#[derive(Clone, Copy)]
enum XMap {
    /// A category chart: `x` is the 1-based category index, centred in its slot.
    /// `horizontal` is a bar chart, whose categories run down the plot and whose
    /// values run across it.
    Category { slot: f64, horizontal: bool },
    /// A scatter chart: `x` is a value on the x axis' scale.
    Scatter(Scale),
}

impl PlotMap {
    /// The map for a drawable, unstacked, axis-bearing group, else `None`.
    fn for_group(group: &ChartGroup, geometry: &GroupGeometry) -> Option<Self> {
        if group_is_stacked(group) {
            return None;
        }
        let plot = geometry.plot;
        let categories = geometry.categories.max(1) as i32;
        let x = match group.kind {
            ChartGroupKind::Bar { direction, .. } => {
                let horizontal = direction == super::BarDirection::Bar;
                let span = if horizontal {
                    plot.size.height.raw()
                } else {
                    plot.size.width.raw()
                };
                XMap::Category {
                    slot: f64::from(span / categories),
                    horizontal,
                }
            }
            ChartGroupKind::Line { .. } | ChartGroupKind::Area { .. } => XMap::Category {
                slot: f64::from(plot.size.width.raw() / categories),
                horizontal: false,
            },
            ChartGroupKind::Scatter { .. } => XMap::Scatter(scatter_x_scale(group)),
            ChartGroupKind::Pie { .. } | ChartGroupKind::Doughnut { .. } => return None,
        };
        Some(Self {
            plot,
            x,
            y: geometry.scale,
        })
    }

    /// A data point's box-local position, unclamped (it may lie outside the
    /// plot; the caller clips).
    fn point(&self, x: f64, y: f64) -> (f64, f64) {
        let left = f64::from(self.plot.origin.x.raw());
        let top = f64::from(self.plot.origin.y.raw());
        let width = f64::from(self.plot.size.width.raw());
        let height = f64::from(self.plot.size.height.raw());
        match self.x {
            XMap::Category {
                slot,
                horizontal: false,
            } => (
                left + slot * (x - 0.5),
                top + height * (1.0 - self.y.fraction(y)),
            ),
            XMap::Category {
                slot,
                horizontal: true,
            } => (left + width * self.y.fraction(y), top + slot * (x - 0.5)),
            XMap::Scatter(scale) => (
                left + width * scale.fraction(x),
                top + height * (1.0 - self.y.fraction(y)),
            ),
        }
    }

    /// Whether the group is a scatter group (the only one with an x direction).
    fn is_scatter(&self) -> bool {
        matches!(self.x, XMap::Scatter(_))
    }
}

/// A series' plotted `(x, y)` data points, in order, blanks skipped, each with
/// its cache index (which a custom error-bar range is indexed by).
///
/// O(points).
pub(super) fn series_points(
    group: &ChartGroup,
    geometry: &GroupGeometry,
    series: &Series,
) -> Vec<(usize, (f64, f64))> {
    if matches!(group.kind, ChartGroupKind::Scatter { .. }) {
        let count = range_len(&series.values).max(series.x_values.as_ref().map_or(0, range_len));
        let ys = dense_numbers(&series.values, count);
        let xs = series
            .x_values
            .as_ref()
            .map(|range| dense_numbers(range, count));
        return (0..count)
            .filter_map(|i| {
                let y = ys[i]?;
                let x = match xs.as_ref() {
                    Some(values) => values[i]?,
                    None => i as f64 + 1.0,
                };
                Some((i, (x, y)))
            })
            .collect();
    }
    dense_numbers(&series.values, geometry.categories)
        .into_iter()
        .enumerate()
        .filter_map(|(i, y)| Some((i, (i as f64 + 1.0, y?))))
        .collect()
}

// ---------------------------------------------------------------- trendlines

/// A fitted regression, evaluable at any `x` in its domain.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Fit {
    /// `y = a + b·x`.
    Linear { a: f64, b: f64 },
    /// `y = c·e^(b·x)`.
    Exponential { c: f64, b: f64 },
    /// `y = a + b·ln(x)`.
    Logarithmic { a: f64, b: f64 },
    /// `y = c·x^b`.
    Power { c: f64, b: f64 },
    /// `y = Σ coef[k]·x^k`, ascending powers — the printed form. Evaluated in
    /// the centred, scaled variable it was solved in (`t = (x - shift) /
    /// scale`, `y = Σ normalized[k]·t^k`), which keeps an order-6 curve over x
    /// in the thousands accurate.
    Polynomial {
        coef: Vec<f64>,
        shift: f64,
        scale: f64,
        normalized: Vec<f64>,
    },
}

impl Fit {
    /// The fitted value at `x`, or `None` outside the fit's domain or when it
    /// is not finite. O(order).
    pub(super) fn eval(&self, x: f64) -> Option<f64> {
        let y = match self {
            Self::Linear { a, b } => a + b * x,
            Self::Exponential { c, b } => c * (b * x).exp(),
            Self::Logarithmic { a, b } => {
                if x <= 0.0 {
                    return None;
                }
                a + b * x.ln()
            }
            Self::Power { c, b } => {
                if x <= 0.0 {
                    return None;
                }
                c * x.powf(*b)
            }
            Self::Polynomial {
                shift,
                scale,
                normalized,
                ..
            } => {
                let t = (x - shift) / scale;
                normalized.iter().rev().fold(0.0, |acc, k| acc * t + k)
            }
        };
        y.is_finite().then_some(y)
    }

    /// The equation as Excel prints it (`y = 1.2x + 0.5`). O(order).
    pub(super) fn equation(&self) -> String {
        match self {
            Self::Linear { a, b } => format!("y = {}{}", term(*b, "x", true), constant(*a)),
            Self::Exponential { c, b } => {
                format!("y = {}e^{}", coefficient(*c), coefficient(*b) + "x")
            }
            Self::Logarithmic { a, b } => {
                format!("y = {}{}", term(*b, "ln(x)", true), constant(*a))
            }
            Self::Power { c, b } => format!("y = {}x^{}", coefficient(*c), coefficient(*b)),
            Self::Polynomial { coef, .. } => {
                let mut text = String::from("y = ");
                let mut first = true;
                for power in (1..coef.len()).rev() {
                    let value = coef[power];
                    if round_significant(value) == 0.0 {
                        continue;
                    }
                    let unit = if power == 1 {
                        "x".to_owned()
                    } else {
                        format!("x{}", superscript(power))
                    };
                    text.push_str(&term(value, &unit, first));
                    first = false;
                }
                let c0 = coef.first().copied().unwrap_or(0.0);
                if first {
                    text.push_str(&coefficient(c0));
                } else {
                    text.push_str(&constant(c0));
                }
                text
            }
        }
    }
}

/// A fit plus its coefficient of determination.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct FitResult {
    pub(super) fit: Fit,
    pub(super) r_squared: f64,
}

/// Fits `kind` over `points`, or `None` when the fit is undefined for them.
///
/// `intercept` is a fixed y intercept (honoured for linear, exponential and
/// polynomial, as Excel offers it). O(points x order + order³).
pub(super) fn fit(
    kind: TrendlineKind,
    order: u8,
    intercept: Option<f64>,
    points: &[(f64, f64)],
) -> Option<FitResult> {
    if points.len() < 2 {
        return None;
    }
    match kind {
        TrendlineKind::Linear => {
            let (a, b) = linear(points, intercept)?;
            let r_squared = r_squared(points, intercept.is_some(), |x| a + b * x);
            Some(FitResult {
                fit: Fit::Linear { a, b },
                r_squared,
            })
        }
        TrendlineKind::Exponential => {
            if points.iter().any(|(_, y)| *y <= 0.0) {
                return None;
            }
            let logs: Vec<(f64, f64)> = points.iter().map(|(x, y)| (*x, y.ln())).collect();
            let fixed = intercept.filter(|c| *c > 0.0).map(f64::ln);
            let (a, b) = linear(&logs, fixed)?;
            let r_squared = r_squared(&logs, fixed.is_some(), |x| a + b * x);
            Some(FitResult {
                fit: Fit::Exponential { c: a.exp(), b },
                r_squared,
            })
        }
        TrendlineKind::Logarithmic => {
            if points.iter().any(|(x, _)| *x <= 0.0) {
                return None;
            }
            let logs: Vec<(f64, f64)> = points.iter().map(|(x, y)| (x.ln(), *y)).collect();
            let (a, b) = linear(&logs, None)?;
            let r_squared = r_squared(&logs, false, |x| a + b * x);
            Some(FitResult {
                fit: Fit::Logarithmic { a, b },
                r_squared,
            })
        }
        TrendlineKind::Power => {
            if points.iter().any(|(x, y)| *x <= 0.0 || *y <= 0.0) {
                return None;
            }
            let logs: Vec<(f64, f64)> = points.iter().map(|(x, y)| (x.ln(), y.ln())).collect();
            let (a, b) = linear(&logs, None)?;
            let r_squared = r_squared(&logs, false, |x| a + b * x);
            Some(FitResult {
                fit: Fit::Power { c: a.exp(), b },
                r_squared,
            })
        }
        TrendlineKind::Polynomial => {
            let fit = polynomial(points, usize::from(order.clamp(2, 6)), intercept)?;
            let r_squared = r_squared(points, intercept.is_some(), |x| {
                fit.eval(x).unwrap_or(f64::NAN)
            });
            Some(FitResult { fit, r_squared })
        }
        // A moving average is not a fitted function; see `moving_average`.
        TrendlineKind::MovingAverage => None,
    }
}

/// Least-squares `y = a + b·x`, or through a fixed intercept. O(points).
fn linear(points: &[(f64, f64)], intercept: Option<f64>) -> Option<(f64, f64)> {
    let n = points.len() as f64;
    if let Some(c) = intercept {
        let sxx: f64 = points.iter().map(|(x, _)| x * x).sum();
        let sxy: f64 = points.iter().map(|(x, y)| x * (y - c)).sum();
        if sxx.abs() < f64::EPSILON {
            return None;
        }
        let b = sxy / sxx;
        return (b.is_finite()).then_some((c, b));
    }
    let mx = points.iter().map(|(x, _)| x).sum::<f64>() / n;
    let my = points.iter().map(|(_, y)| y).sum::<f64>() / n;
    let sxx: f64 = points.iter().map(|(x, _)| (x - mx) * (x - mx)).sum();
    let sxy: f64 = points.iter().map(|(x, y)| (x - mx) * (y - my)).sum();
    if sxx.abs() < f64::EPSILON * n.max(1.0) {
        return None;
    }
    let b = sxy / sxx;
    let a = my - b * mx;
    (a.is_finite() && b.is_finite()).then_some((a, b))
}

/// A least-squares polynomial fit.
///
/// The normal equations are formed in a centred, scaled variable
/// `t = (x - m) / s` (`m` the mean x — or 0 with a fixed intercept, which pins
/// the curve at x = 0 — and `s` the largest |x - m|), so an order-6 fit over x
/// in the thousands neither squares 1e3 twelve times nor solves a Vandermonde
/// system on a sliver of [0, 1]. The printed coefficients are expanded back to
/// powers of `x` (binomial expansion, O(order²)). Solved by Gaussian
/// elimination with partial pivoting; `None` when singular (fewer distinct x
/// than unknowns). O(points x order + order³).
fn polynomial(points: &[(f64, f64)], order: usize, intercept: Option<f64>) -> Option<Fit> {
    let first = usize::from(intercept.is_some());
    let unknowns = order + 1 - first;
    if points.len() < unknowns {
        return None;
    }
    let shift = if intercept.is_some() {
        0.0
    } else {
        points.iter().map(|(x, _)| x).sum::<f64>() / points.len() as f64
    };
    let scale = points
        .iter()
        .map(|(x, _)| (x - shift).abs())
        .fold(0.0f64, f64::max);
    if !(scale.is_finite() && scale > 0.0) {
        return None;
    }
    let c = intercept.unwrap_or(0.0);
    // Power sums Σ t^k for k in 0..=2·order, and Σ t^k·(y - c) for k in 0..=order.
    let mut sums = vec![0.0f64; 2 * order + 1];
    let mut rhs_sums = vec![0.0f64; order + 1];
    for (x, y) in points {
        let t = (x - shift) / scale;
        let mut power = 1.0;
        for (k, sum) in sums.iter_mut().enumerate() {
            *sum += power;
            if k <= order {
                rhs_sums[k] += power * (y - c);
            }
            power *= t;
        }
    }
    let mut matrix: Vec<Vec<f64>> = (0..unknowns)
        .map(|row| {
            let mut line: Vec<f64> = (0..unknowns)
                .map(|col| sums[row + first + col + first])
                .collect();
            line.push(rhs_sums[row + first]);
            line
        })
        .collect();
    let solution = solve(&mut matrix)?;
    let mut normalized = vec![0.0f64; order + 1];
    normalized[0] = c;
    for (index, value) in solution.into_iter().enumerate() {
        normalized[index + first] = value;
    }
    // Expand Σ n_k ((x - m)/s)^k into powers of x.
    let mut coef = vec![0.0f64; order + 1];
    for (k, n_k) in normalized.iter().enumerate() {
        let factor = n_k / scale.powi(k as i32);
        let mut binomial = 1.0f64;
        for (j, slot) in coef.iter_mut().enumerate().take(k + 1) {
            // C(k, j) · (-m)^(k - j) contributes to x^j.
            *slot += factor * binomial * (-shift).powi((k - j) as i32);
            binomial = binomial * (k - j) as f64 / (j + 1) as f64;
        }
    }
    (coef
        .iter()
        .chain(&normalized)
        .all(|value| value.is_finite()))
    .then_some(Fit::Polynomial {
        coef,
        shift,
        scale,
        normalized,
    })
}

/// Solves an augmented `n x (n+1)` system in place by Gaussian elimination with
/// partial pivoting; `None` when a pivot vanishes. O(n³).
pub(super) fn solve(matrix: &mut [Vec<f64>]) -> Option<Vec<f64>> {
    let n = matrix.len();
    let magnitude = matrix
        .iter()
        .flat_map(|row| row.iter().take(n))
        .fold(0.0f64, |acc, value| acc.max(value.abs()))
        .max(f64::MIN_POSITIVE);
    for col in 0..n {
        let pivot =
            (col..n).max_by(|a, b| matrix[*a][col].abs().total_cmp(&matrix[*b][col].abs()))?;
        if matrix[pivot][col].abs() <= magnitude * 1e-12 {
            return None;
        }
        matrix.swap(col, pivot);
        let (upper, lower) = matrix.split_at_mut(col + 1);
        let pivot_row = &upper[col];
        for row in lower.iter_mut() {
            let factor = row[col] / pivot_row[col];
            for (target, value) in row.iter_mut().zip(pivot_row.iter()).skip(col) {
                *target -= factor * value;
            }
        }
    }
    let mut solution = vec![0.0f64; n];
    for row in (0..n).rev() {
        let tail: f64 = (row + 1..n).map(|k| matrix[row][k] * solution[k]).sum();
        solution[row] = (matrix[row][n] - tail) / matrix[row][row];
    }
    solution
        .iter()
        .all(|value| value.is_finite())
        .then_some(solution)
}

/// The coefficient of determination of `model` over `points`; uncentred when
/// the intercept was fixed, as Excel computes it. O(points).
fn r_squared(points: &[(f64, f64)], fixed_intercept: bool, model: impl Fn(f64) -> f64) -> f64 {
    let n = points.len() as f64;
    let mean = if fixed_intercept {
        0.0
    } else {
        points.iter().map(|(_, y)| y).sum::<f64>() / n
    };
    let total: f64 = points.iter().map(|(_, y)| (y - mean).powi(2)).sum();
    let residual: f64 = points.iter().map(|(x, y)| (y - model(*x)).powi(2)).sum();
    if !residual.is_finite() {
        return 0.0;
    }
    if total <= f64::EPSILON {
        return if residual <= f64::EPSILON { 1.0 } else { 0.0 };
    }
    1.0 - residual / total
}

/// A moving average over `period` points: one output point per window, at the
/// window's last `x` — `points.len() - period + 1` points, or none when the
/// period exceeds the data. O(points) with a running sum.
pub(super) fn moving_average(points: &[(f64, f64)], period: u32) -> Vec<(f64, f64)> {
    let period = (period.clamp(2, 255)) as usize;
    if points.len() < period {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(points.len() - period + 1);
    let mut sum: f64 = points[..period].iter().map(|(_, y)| y).sum();
    out.push((points[period - 1].0, sum / period as f64));
    for i in period..points.len() {
        sum += points[i].1 - points[i - period].1;
        out.push((points[i].0, sum / period as f64));
    }
    out
}

/// Draws one trendline over `points`.
#[allow(clippy::too_many_arguments)] // one series' trendline; every input is used
fn draw_trendline(
    trendline: &Trendline,
    points: &[(f64, f64)],
    map: &PlotMap,
    series_color: [u8; 4],
    paint: &Paint<'_>,
    shape: &mut LabelShaper<'_>,
    out: &mut Vec<ChartPrimitive>,
) {
    if trendline.line.is_some_and(|line| line.no_fill) || points.len() < 2 {
        return;
    }
    let stroke = analysis_stroke(
        trendline.line,
        series_color,
        TRENDLINE_WIDTH,
        DashStyle::SystemDot,
        paint,
    );
    let (samples, result) = if trendline.kind == TrendlineKind::MovingAverage {
        (moving_average(points, trendline.period.unwrap_or(2)), None)
    } else {
        let intercept = trendline.intercept.as_deref().and_then(parse_axis_bound);
        let Some(result) = fit(
            trendline.kind,
            trendline.order.unwrap_or(2),
            intercept,
            points,
        ) else {
            return;
        };
        (sample_fit(&result.fit, trendline, points), Some(result))
    };
    let screen: Vec<(f64, f64)> = samples.iter().map(|(x, y)| map.point(*x, *y)).collect();
    let runs = clip_polyline(&screen, map.plot);
    let end = runs.last().and_then(|run| run.last().copied());
    for run in runs {
        out.push(ChartPrimitive::polyline(&run, false, None, Some(stroke)));
    }
    if let (Some(result), Some(end)) = (result, end) {
        let mut lines = Vec::new();
        if trendline.display_equation {
            lines.push(result.fit.equation());
        }
        if trendline.display_r_squared {
            lines.push(format!("R² = {:.4}", result.r_squared));
        }
        draw_trend_labels(&lines, end, map.plot, paint, shape, out);
    }
}

/// The trendline's samples across its data domain plus projection: two for a
/// straight line, [`TREND_SAMPLES`] for a curve. O(samples x order).
fn sample_fit(fit: &Fit, trendline: &Trendline, points: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let (lo, hi) = points
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), (x, _)| {
            (lo.min(*x), hi.max(*x))
        });
    let span = (hi - lo).abs().max(1.0);
    let projection = |value: Option<&String>| {
        value
            .and_then(|text| parse_axis_bound(text))
            .map_or(0.0, |value| value.clamp(0.0, span * MAX_PROJECTION_SPANS))
    };
    let lo = lo - projection(trendline.backward.as_ref());
    let hi = hi + projection(trendline.forward.as_ref());
    let count = if matches!(fit, Fit::Linear { .. }) {
        2
    } else {
        TREND_SAMPLES
    };
    (0..count)
        .filter_map(|i| {
            let x = lo + (hi - lo) * i as f64 / (count - 1) as f64;
            fit.eval(x).map(|y| (x, y))
        })
        .collect()
}

/// Prints the equation / R² lines just above-left of the trendline's end,
/// kept inside the plot.
fn draw_trend_labels(
    lines: &[String],
    end: Point,
    plot: Rect,
    paint: &Paint<'_>,
    shape: &mut LabelShaper<'_>,
    out: &mut Vec<ChartPrimitive>,
) {
    let labels: Vec<ChartLabel> = lines
        .iter()
        .filter_map(|line| shape_colored(line, &paint.data_text, shape))
        .collect();
    if labels.is_empty() {
        return;
    }
    let total: i32 = labels.iter().map(|label| label.height().raw()).sum();
    let mut top = (end.y.raw() - TREND_LABEL_GAP - total)
        .max(plot.origin.y.raw())
        .min(plot.bottom().raw() - total);
    for label in &labels {
        let left = (end.x.raw() - label.width.raw())
            .max(plot.origin.x.raw())
            .min(plot.right().raw() - label.width.raw());
        place_label(label, Twip(left), Twip(top + label.ascent.raw()), out);
        top += label.height().raw();
    }
}

// ---------------------------------------------------------------- error bars

/// One point's error bar in data units: the bar runs from `low` to `high`
/// about `at` (the point's other coordinate).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ErrorSpan {
    /// The value the bar starts at.
    pub(super) low: f64,
    /// The value the bar ends at.
    pub(super) high: f64,
    /// Whether the low end is a drawn end (gets a cap).
    pub(super) low_drawn: bool,
    /// Whether the high end is a drawn end.
    pub(super) high_drawn: bool,
}

/// Every point's error span for `bars` over `values` (the measured coordinate
/// per point, in point order), Excel's rules:
///
/// - fixed value: `±value` about each point;
/// - percentage: `±value% · |v|` about each point;
/// - standard deviation: `mean ± value · s` (sample s), the SAME span at every
///   point — Excel centres it on the series mean, not on the point;
/// - standard error: `±SE` about each point, `SE = sqrt(Σ(v-m)² / (n(n-1)))`;
/// - custom: `+plus[i]`, `-minus[i]`, read from the cached ranges by index.
///
/// O(points).
pub(super) fn error_spans(bars: &ErrorBars, values: &[(usize, f64)]) -> Vec<ErrorSpan> {
    let n = values.len();
    if n == 0 {
        return Vec::new();
    }
    let parameter = bars
        .value
        .as_deref()
        .and_then(parse_axis_bound)
        .map(f64::abs);
    let mean = values.iter().map(|(_, v)| v).sum::<f64>() / n as f64;
    let squares: f64 = values.iter().map(|(_, v)| (v - mean).powi(2)).sum();
    let (plus_values, minus_values) = if bars.value_type == ErrorValueType::Custom {
        let count = values.iter().map(|(i, _)| i + 1).max().unwrap_or(0);
        (
            bars.plus.as_ref().map(|range| dense_numbers(range, count)),
            bars.minus.as_ref().map(|range| dense_numbers(range, count)),
        )
    } else {
        (None, None)
    };
    let pick = |range: &Option<Vec<Option<f64>>>, index: usize| {
        range
            .as_ref()
            .and_then(|values| values.get(index).copied().flatten())
            .map_or(0.0, f64::abs)
    };
    values
        .iter()
        .map(|(index, value)| {
            let (centre, plus, minus) = match bars.value_type {
                ErrorValueType::FixedValue => {
                    let length = parameter.unwrap_or(1.0);
                    (*value, length, length)
                }
                ErrorValueType::Percentage => {
                    let length = value.abs() * parameter.unwrap_or(5.0) / 100.0;
                    (*value, length, length)
                }
                ErrorValueType::StandardDeviation => {
                    let deviation = if n > 1 {
                        (squares / (n as f64 - 1.0)).sqrt()
                    } else {
                        0.0
                    };
                    let length = parameter.unwrap_or(1.0) * deviation;
                    (mean, length, length)
                }
                ErrorValueType::StandardError => {
                    let error = if n > 1 {
                        (squares / (n as f64 * (n as f64 - 1.0))).sqrt()
                    } else {
                        0.0
                    };
                    (*value, error, error)
                }
                ErrorValueType::Custom => (
                    *value,
                    pick(&plus_values, *index),
                    pick(&minus_values, *index),
                ),
            };
            let (plus, minus) = match bars.bar_type {
                ErrorBarType::Both => (plus, minus),
                ErrorBarType::Plus => (plus, 0.0),
                ErrorBarType::Minus => (0.0, minus),
            };
            ErrorSpan {
                low: centre - minus,
                high: centre + plus,
                low_drawn: bars.bar_type != ErrorBarType::Plus,
                high_drawn: bars.bar_type != ErrorBarType::Minus,
            }
        })
        .collect()
}

/// Draws one error-bar set over a series' points.
fn draw_error_bars(
    bars: &ErrorBars,
    points: &[(usize, (f64, f64))],
    map: &PlotMap,
    paint: &Paint<'_>,
    out: &mut Vec<ChartPrimitive>,
) {
    if bars.line.is_some_and(|line| line.no_fill) {
        return;
    }
    let along_x = bars.direction == Some(ErrorBarDirection::X);
    // Only a scatter series has an x value to be wrong about.
    if along_x && !map.is_scatter() {
        return;
    }
    let stroke = analysis_stroke(
        bars.line,
        ERROR_BAR_COLOR,
        ERROR_BAR_WIDTH,
        DashStyle::Solid,
        paint,
    );
    let measured: Vec<(usize, f64)> = points
        .iter()
        .map(|(i, (x, y))| (*i, if along_x { *x } else { *y }))
        .collect();
    let spans = error_spans(bars, &measured);
    for ((_, (x, y)), span) in points.iter().zip(spans) {
        let (from, to) = if along_x {
            (map.point(span.low, *y), map.point(span.high, *y))
        } else {
            (map.point(*x, span.low), map.point(*x, span.high))
        };
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let length = dx.hypot(dy);
        if !length.is_finite() || length < 1.0 {
            continue;
        }
        push_clipped(&[from, to], map.plot, stroke, out);
        if bars.no_end_cap {
            continue;
        }
        // The cap is perpendicular to the bar, whichever way the bar runs.
        let (nx, ny) = (-dy / length * CAP_HALF, dx / length * CAP_HALF);
        for (end, drawn) in [(from, span.low_drawn), (to, span.high_drawn)] {
            if drawn {
                push_clipped(
                    &[(end.0 - nx, end.1 - ny), (end.0 + nx, end.1 + ny)],
                    map.plot,
                    stroke,
                    out,
                );
            }
        }
    }
}

/// A trendline's or error bar's stroke: its own `a:ln` colour, width and dash,
/// else the given defaults.
fn analysis_stroke(
    line: Option<ChartLine>,
    default_color: [u8; 4],
    default_width: Twip,
    default_dash: DashStyle,
    paint: &Paint<'_>,
) -> ChartStroke {
    ChartStroke {
        color: line
            .and_then(|line| line.color)
            .map_or(default_color, paint.colors),
        width: line_width(line, default_width),
        dash: line.and_then(|line| line.dash).unwrap_or(default_dash),
    }
}

/// Clips a polyline and appends each surviving run.
fn push_clipped(
    points: &[(f64, f64)],
    plot: Rect,
    stroke: ChartStroke,
    out: &mut Vec<ChartPrimitive>,
) {
    for run in clip_polyline(points, plot) {
        out.push(ChartPrimitive::polyline(&run, false, None, Some(stroke)));
    }
}

/// Clips a polyline (box-local, `f64`) to `rect`, splitting it where it leaves
/// and re-enters; a non-finite vertex breaks the line. Liang-Barsky per
/// segment. O(points).
pub(super) fn clip_polyline(points: &[(f64, f64)], rect: Rect) -> Vec<Vec<Point>> {
    let bounds = (
        f64::from(rect.origin.x.raw()),
        f64::from(rect.origin.y.raw()),
        f64::from(rect.right().raw()),
        f64::from(rect.bottom().raw()),
    );
    let to_point = |(x, y): (f64, f64)| Point::new(Twip(round_angle(x)), Twip(round_angle(y)));
    let mut runs: Vec<Vec<Point>> = Vec::new();
    let mut current: Vec<Point> = Vec::new();
    let flush = |current: &mut Vec<Point>, runs: &mut Vec<Vec<Point>>| {
        if current.len() >= 2 {
            runs.push(core::mem::take(current));
        } else {
            current.clear();
        }
    };
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if !(a.0.is_finite() && a.1.is_finite() && b.0.is_finite() && b.1.is_finite()) {
            flush(&mut current, &mut runs);
            continue;
        }
        match clip_segment(a, b, bounds) {
            Some((start, end, entered, left)) => {
                if entered || current.is_empty() {
                    flush(&mut current, &mut runs);
                    current.push(to_point(start));
                }
                current.push(to_point(end));
                if left {
                    flush(&mut current, &mut runs);
                }
            }
            None => flush(&mut current, &mut runs),
        }
    }
    flush(&mut current, &mut runs);
    runs
}

/// Liang-Barsky: the visible part of segment `a`-`b` inside `(x0, y0, x1, y1)`,
/// with whether its start was moved (it entered) and its end was moved (it
/// left). O(1).
#[allow(clippy::type_complexity)]
fn clip_segment(
    a: (f64, f64),
    b: (f64, f64),
    (x0, y0, x1, y1): (f64, f64, f64, f64),
) -> Option<((f64, f64), (f64, f64), bool, bool)> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let mut t0 = 0.0f64;
    let mut t1 = 1.0f64;
    for (p, q) in [
        (-dx, a.0 - x0),
        (dx, x1 - a.0),
        (-dy, a.1 - y0),
        (dy, y1 - a.1),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                t0 = t0.max(r);
            } else {
                t1 = t1.min(r);
            }
        }
    }
    if t0 > t1 {
        return None;
    }
    Some((
        (a.0 + t0 * dx, a.1 + t0 * dy),
        (a.0 + t1 * dx, a.1 + t1 * dy),
        t0 > 0.0,
        t1 < 1.0,
    ))
}

// ---------------------------------------------------------------- equation text

/// Rounds to four significant digits, Excel's default trendline-label
/// precision.
fn round_significant(value: f64) -> f64 {
    if value == 0.0 || !value.is_finite() {
        return 0.0;
    }
    let magnitude = value.abs().log10().floor() as i32;
    let factor = 10f64.powi(3 - magnitude);
    if !factor.is_finite() || factor == 0.0 {
        return value;
    }
    (value * factor).round() / factor
}

/// A coefficient printed at four significant digits, trailing zeros trimmed.
fn coefficient(value: f64) -> String {
    let rounded = round_significant(value);
    let text = format!("{rounded}");
    if text.contains('e') {
        format!("{rounded:.3e}")
    } else {
        text
    }
}

/// `coef·unit`, signed for its position: the leading term carries a bare `-`,
/// later ones ` + ` / ` - `. A unit coefficient prints as the bare unit.
fn term(value: f64, unit: &str, leading: bool) -> String {
    let rounded = round_significant(value);
    let magnitude = coefficient(rounded.abs());
    let body = if magnitude == "1" {
        unit.to_owned()
    } else {
        format!("{magnitude}{unit}")
    };
    match (leading, rounded < 0.0) {
        (true, false) => body,
        (true, true) => format!("-{body}"),
        (false, false) => format!(" + {body}"),
        (false, true) => format!(" - {body}"),
    }
}

/// A trailing constant term (` + 0.5`), or nothing when it rounds to zero.
fn constant(value: f64) -> String {
    let rounded = round_significant(value);
    if rounded == 0.0 {
        String::new()
    } else if rounded < 0.0 {
        format!(" - {}", coefficient(-rounded))
    } else {
        format!(" + {}", coefficient(rounded))
    }
}

/// A small power as a superscript digit (2..6 are all a polynomial needs).
fn superscript(power: usize) -> &'static str {
    match power {
        2 => "²",
        3 => "³",
        4 => "⁴",
        5 => "⁵",
        6 => "⁶",
        _ => "",
    }
}
