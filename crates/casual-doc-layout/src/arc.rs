// Copyright The opendoc Authors
// SPDX-License-Identifier: Apache-2.0

//! Circular arcs and annular sectors as [`PathCommand`] lists.
//!
//! # The established approach, named before inventing one (`SKILL` §8)
//!
//! A circle is not a polynomial, so every vector pipeline approximates it. There
//! are exactly three answers in common use, and this module takes the third:
//!
//! 1. **A dedicated arc command in the path, flattened by the backend.** What
//!    `docs/155` §7.4 asked for, and what DrawingML itself writes (`a:arcTo`).
//!    Rejected *here* only because [`PathCommand`] is the display-list mirror of
//!    `casual_doc_model::v1::ShapePathCommand`; adding a fifth variant on one
//!    side diverges the mirror, and the model is held by another lane. The
//!    conversion in this module is exactly what such a backend would do anyway
//!    (`ArcTo.js:216` in the market leader's engine flattens once, centrally,
//!    for the same reason).
//! 2. **A flattened polyline at a chord tolerance.** Rejected by `docs/155` §7.3
//!    and `docs/119` §6: a many-sided polygon fan is a *second* curve mechanism
//!    living beside the Bézier one the path primitive already has, and the vertex
//!    count becomes a tuning knob that the golden harness then pins.
//! 3. **Cubic Bézier approximation, one segment per quadrant.** The textbook
//!    result: for a sweep `Δ`, the control handles leave the endpoints along the
//!    tangents at distance `k = (4/3)·tan(Δ/4)·r`. This is what SVG renderers,
//!    PostScript, cairo and Skia all do for an elliptical arc, and
//!    `docs/155` §7.4 pre-authorised it in as many words: *"If the primitive ends
//!    up Bézier-only, charts will use the standard four-segment-per-quadrant
//!    approximation and say so in code."* It is saying so.
//!
//! So this module adds **no new display-list primitive at all**. An arc is a
//! *constructor over the existing path primitive*, which is why `docs/155` §7.1's
//! "there is no arc, no quadratic and no cubic anywhere" no longer describes the
//! tree: the shapes lane landed [`PathCommand::CubicTo`] and
//! [`PathCommand::QuadTo`], and the backend's `command_path` already walks them.
//!
//! # Accuracy
//!
//! One cubic per 90° has a maximum radial error of about `2.7e-4 · r`
//! ([`MAX_RADIAL_ERROR_RATIO`]). On a 3-inch pie (`r ≈ 2160` twips) that is under
//! 0.6 twip — below the 1-twip resolution the display list stores points at, so
//! the approximation is finer than the coordinate space can record. There is
//! therefore no tolerance to tune and none is exposed.
//!
//! # Angles
//!
//! Angles are **60000ths of a degree** (DrawingML's `ST_Angle`, the unit
//! [`crate::display::ShapeTransform::rotation`] already uses), measured
//! **clockwise from three o'clock** — DrawingML's own convention, so the preset
//! and `a:custGeom` evaluators can reuse this without a second frame of
//! reference. A chart's `c:firstSliceAng` is measured from twelve o'clock
//! instead; that offset belongs to the chart drawer, not here.
//!
//! Because the display list's y axis points down, a numerically increasing angle
//! sweeps clockwise on screen, which is the direction both DrawingML and a pie
//! chart lay slices out in.
//!
//! # Complexity
//!
//! Every function is O(segments) = O(1 + |sweep| / 90°), bounded by 4 commands
//! for a full turn. Nothing here touches the document.

use crate::display::PathCommand;
use crate::units::{Point, Twip};

/// A full turn in 60000ths of a degree.
pub const FULL_TURN: i32 = 360 * 60_000;

/// The largest sweep one cubic segment is allowed to approximate: a quadrant.
///
/// Not a tuning knob — see [`MAX_RADIAL_ERROR_RATIO`]. A quadrant is the standard
/// subdivision because the error at 90° is already below this coordinate space's
/// resolution, and because 4 segments close a full turn exactly.
const MAX_SEGMENT_ANGLE: i32 = FULL_TURN / 4;

/// The maximum radial error of a one-cubic-per-quadrant arc, as a fraction of the
/// radius.
///
/// Published as a constant rather than prose so the guard that checks it cites
/// the same number the doc comment does.
pub const MAX_RADIAL_ERROR_RATIO: f64 = 2.8e-4;

/// Converts an `ST_Angle` to radians.
fn radians(angle: i32) -> f64 {
    f64::from(angle) / 60_000.0 * core::f64::consts::PI / 180.0
}

/// The point at `angle` on the circle of radius `radius` about `center`.
fn on_circle(center: Point, radius: f64, angle: f64) -> Point {
    Point::new(
        Twip(round_to_twip(
            f64::from(center.x.raw()) + radius * angle.cos(),
        )),
        Twip(round_to_twip(
            f64::from(center.y.raw()) + radius * angle.sin(),
        )),
    )
}

/// Rounds to the nearest whole twip, saturating rather than wrapping — a
/// producer-supplied radius can be absurd and must not become a negative
/// coordinate by overflow.
fn round_to_twip(value: f64) -> i32 {
    if value.is_nan() {
        return 0;
    }
    let rounded = value.round();
    if rounded >= f64::from(i32::MAX) {
        i32::MAX
    } else if rounded <= f64::from(i32::MIN) {
        i32::MIN
    } else {
        // Exact: `rounded` is integral and inside i32.
        #[allow(clippy::cast_possible_truncation)]
        {
            rounded as i32
        }
    }
}

/// Appends the cubic segments of a circular arc to `out`, assuming the pen is
/// already at the arc's start point.
///
/// Subdivides `sweep` into equal segments of at most [`MAX_SEGMENT_ANGLE`] and
/// emits one [`PathCommand::CubicTo`] each, with the handle length
/// `k = (4/3)·tan(Δ/4)·r` that makes the curve touch the circle at both
/// endpoints and at the midpoint.
///
/// A zero `sweep` appends nothing: there is no arc, and emitting a degenerate
/// segment would put a visible stroke join where the document has no geometry.
///
/// Complexity: O(1 + |sweep| / 90°).
fn append_arc(out: &mut Vec<PathCommand>, center: Point, radius: f64, start: i32, sweep: i32) {
    if sweep == 0 || radius <= 0.0 {
        return;
    }
    // `div_ceil` on the magnitude, so a 91° sweep becomes two 45.5° segments
    // rather than a quadrant plus a sliver: equal segments keep the error even.
    let segments = sweep
        .unsigned_abs()
        .div_ceil(MAX_SEGMENT_ANGLE.unsigned_abs())
        .max(1);
    // Angles stay in f64 from here: rounding the step to a whole angle unit and
    // then multiplying it back out would leave the final endpoint short of the
    // true end angle, and a pie slice's rim would then not meet the next
    // slice's. The endpoints are rounded, the angles are not.
    let begin_angle = radians(start);
    let end_angle = radians(start.saturating_add(sweep));
    let step = (end_angle - begin_angle) / f64::from(segments);
    // The tangent handle length for one segment (Δ = `step`).
    let handle = 4.0 / 3.0 * (step / 4.0).tan() * radius;
    for index in 0..segments {
        let from = begin_angle + step * f64::from(index);
        // Pin the last segment to the exact end angle so accumulated
        // floating-point drift cannot move the slice boundary.
        let to = if index + 1 == segments {
            end_angle
        } else {
            begin_angle + step * f64::from(index + 1)
        };
        let begin = on_circle(center, radius, from);
        let end = on_circle(center, radius, to);
        // Tangents of (r·cos θ, r·sin θ) are (−sin θ, cos θ), unit length.
        out.push(PathCommand::CubicTo {
            control1: Point::new(
                Twip(round_to_twip(
                    f64::from(begin.x.raw()) - handle * from.sin(),
                )),
                Twip(round_to_twip(
                    f64::from(begin.y.raw()) + handle * from.cos(),
                )),
            ),
            control2: Point::new(
                Twip(round_to_twip(f64::from(end.x.raw()) + handle * to.sin())),
                Twip(round_to_twip(f64::from(end.y.raw()) - handle * to.cos())),
            ),
            point: end,
        });
    }
}

/// A circular sector: a pie wedge when `inner_radius` is zero, an annular
/// (doughnut) sector when it is not.
///
/// **One mechanism, not two** (`SKILL` §8). A doughnut is a pie with a hole, so
/// the hole is a parameter and not a second function: the returned figure is a
/// *single closed contour* — outer arc forward, a radial segment inward, the
/// inner arc backward, and the closing radial segment — so the hole falls outside
/// the contour and is left empty under any fill rule. Nothing here depends on
/// winding, which is what makes a sector safe to hand to a backend that has not
/// stated its rule.
///
/// Returns an **empty** list, and therefore paints nothing, when:
///
/// - `outer_radius` is not positive — there is no circle;
/// - `sweep` is zero — a zero-valued pie slice. A zero slice must paint *nothing*,
///   not a hairline sliver at the slice boundary, which is what a naive
///   "always emit the radii" construction produces and what the guard in this
///   module's tests pins.
///
/// `inner_radius` is clamped into `0..outer_radius`; a hole at least as large as
/// the circle would leave no ring, so it degenerates to the largest ring that
/// still has area rather than inverting.
///
/// A `sweep` of a full turn or more has no radii to cut, so it yields the
/// complete circle (or, with a hole, the complete ring as a second reversed
/// subpath — the only case in this module that relies on the backend's nonzero
/// winding rule, which `casual-doc-render` uses at every `fill_path` call site).
///
/// Complexity: O(1 + |sweep| / 90°) — at most 10 commands.
#[must_use]
pub fn sector(
    center: Point,
    outer_radius: Twip,
    inner_radius: Twip,
    start: i32,
    sweep: i32,
) -> Vec<PathCommand> {
    if outer_radius.raw() <= 0 || sweep == 0 {
        return Vec::new();
    }
    let outer = f64::from(outer_radius.raw());
    // A hole must stay strictly inside the circle; `outer_radius - 1` twip is the
    // largest ring the coordinate space can still distinguish from a disc.
    let inner = f64::from(inner_radius.raw().clamp(0, outer_radius.raw() - 1));

    let mut out = Vec::new();
    if sweep.unsigned_abs() >= FULL_TURN.unsigned_abs() {
        let full = if sweep.is_negative() {
            -FULL_TURN
        } else {
            FULL_TURN
        };
        out.push(PathCommand::MoveTo {
            point: on_circle(center, outer, radians(start)),
        });
        append_arc(&mut out, center, outer, start, full);
        if inner > 0.0 {
            // Reversed, so nonzero winding subtracts the hole.
            out.push(PathCommand::MoveTo {
                point: on_circle(center, inner, radians(start)),
            });
            append_arc(&mut out, center, inner, start, -full);
        }
        return out;
    }

    let end = start.saturating_add(sweep);
    out.push(PathCommand::MoveTo {
        point: on_circle(center, outer, radians(start)),
    });
    append_arc(&mut out, center, outer, start, sweep);
    if inner > 0.0 {
        out.push(PathCommand::LineTo {
            point: on_circle(center, inner, radians(end)),
        });
        append_arc(&mut out, center, inner, end, -sweep);
    } else {
        out.push(PathCommand::LineTo { point: center });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Flattens a command list to the points it visits, sampling the *interior*
    /// of every segment — cubic and straight alike — so a geometric assertion is
    /// made against the figure the backend will fill, not against its vertices.
    ///
    /// Sampling straight segments matters and is not padding: an earlier version
    /// of this helper recorded only a `LineTo`'s endpoint, and two guards below
    /// then passed under the chord-fan mutation because a chord's endpoints lie
    /// exactly on the circle and only its middle does not (`SKILL` §4 — the
    /// mutation found the weak guard, which is what the mutation is for).
    ///
    /// Deliberately a *test* helper: production never flattens, because the
    /// backend does (`SKILL` §8 — one curve mechanism, and it is not here).
    fn flatten(commands: &[PathCommand], steps: usize) -> Vec<(f64, f64)> {
        let mut out = Vec::new();
        let mut pen = (0.0, 0.0);
        for command in commands {
            match *command {
                PathCommand::MoveTo { point } => {
                    pen = (f64::from(point.x.raw()), f64::from(point.y.raw()));
                    out.push(pen);
                }
                PathCommand::LineTo { point } => {
                    let to = (f64::from(point.x.raw()), f64::from(point.y.raw()));
                    for step in 1..=steps {
                        let t = step as f64 / steps as f64;
                        out.push((pen.0 + (to.0 - pen.0) * t, pen.1 + (to.1 - pen.1) * t));
                    }
                    pen = to;
                }
                PathCommand::CubicTo {
                    control1,
                    control2,
                    point,
                } => {
                    let p0 = pen;
                    let p1 = (f64::from(control1.x.raw()), f64::from(control1.y.raw()));
                    let p2 = (f64::from(control2.x.raw()), f64::from(control2.y.raw()));
                    let p3 = (f64::from(point.x.raw()), f64::from(point.y.raw()));
                    for step in 1..=steps {
                        let t = step as f64 / steps as f64;
                        let u = 1.0 - t;
                        let bez = |a: f64, b: f64, c: f64, d: f64| {
                            u * u * u * a
                                + 3.0 * u * u * t * b
                                + 3.0 * u * t * t * c
                                + t * t * t * d
                        };
                        out.push((bez(p0.0, p1.0, p2.0, p3.0), bez(p0.1, p1.1, p2.1, p3.1)));
                    }
                    pen = p3;
                }
                PathCommand::QuadTo { point, .. } => {
                    pen = (f64::from(point.x.raw()), f64::from(point.y.raw()));
                    out.push(pen);
                }
            }
        }
        out
    }

    const CENTER: Point = Point {
        x: Twip(5_000),
        y: Twip(5_000),
    };

    #[test]
    fn a_quarter_arc_stays_on_the_circle_within_the_published_error() {
        // The guarantee: every sampled point of the approximation is on the
        // circle to within MAX_RADIAL_ERROR_RATIO. Not "the path has N points".
        let radius = 2_000.0;
        let commands = sector(CENTER, Twip(2_000), Twip::ZERO, 0, FULL_TURN / 4);
        let tolerance = radius * MAX_RADIAL_ERROR_RATIO + 1.0; // +1 twip of rounding
        // The rim is everything but the closing radial segment back to the
        // centre; every sampled point of it must be ON the circle, in BOTH
        // directions. "Inside the circle" would be satisfied by a chord, which is
        // the approach this module rejects.
        let rim = &commands[..commands.len() - 1];
        let samples = flatten(rim, 24);
        assert!(
            samples.len() > 20,
            "the rim must be sampled, not just visited"
        );
        for (x, y) in &samples {
            let distance = (x - 5_000.0).hypot(y - 5_000.0);
            assert!(
                (distance - radius).abs() <= tolerance,
                "sampled rim point ({x}, {y}) is {distance} from the centre, off the \
                 {radius}-twip circle by more than {tolerance}"
            );
        }
        // And the arc really reaches the far end: 0deg is three o'clock, a
        // quarter turn clockwise is six o'clock.
        assert!(
            samples
                .iter()
                .any(|(x, y)| (*x - 5_000.0).abs() < 2.0 && (*y - 7_000.0).abs() < 2.0),
            "a quarter turn from three o'clock must reach six o'clock"
        );
    }

    #[test]
    fn three_equal_slices_close_the_circle_and_do_not_overlap() {
        // The guarantee, stated as the task framed it: three equal sweeps
        // together close the circle and do not overlap.
        let sweeps: Vec<i32> = (0..3).map(|_| FULL_TURN / 3).collect();
        let total: i32 = sweeps.iter().sum();
        assert_eq!(
            total, FULL_TURN,
            "three thirds must be a whole turn exactly"
        );

        let mut start = 0;
        let mut spans = Vec::new();
        for sweep in sweeps {
            let commands = sector(CENTER, Twip(2_000), Twip::ZERO, start, sweep);
            assert!(!commands.is_empty(), "a third of a circle must paint");
            spans.push((start, start + sweep));
            start += sweep;
        }
        // Adjacent: each slice begins exactly where the last ended, so there is
        // neither a gap nor an overlap anywhere.
        for pair in spans.windows(2) {
            assert_eq!(
                pair[0].1, pair[1].0,
                "slice boundaries must coincide: {:?} then {:?}",
                pair[0], pair[1]
            );
        }
        assert_eq!(
            spans.last().expect("three spans").1,
            FULL_TURN,
            "the last slice must land back at the start angle"
        );

        // And geometrically: the shared boundary point is the same point in both
        // slices, so no seam opens up through integer rounding.
        let first = sector(CENTER, Twip(2_000), Twip::ZERO, 0, FULL_TURN / 3);
        let second = sector(
            CENTER,
            Twip(2_000),
            Twip::ZERO,
            FULL_TURN / 3,
            FULL_TURN / 3,
        );
        let first_end = first
            .iter()
            .rev()
            .find(|command| matches!(command, PathCommand::CubicTo { .. }))
            .expect("an outer arc")
            .endpoint();
        let second_start = second.first().expect("a move").endpoint();
        assert_eq!(
            first_end, second_start,
            "slice 1's rim must end where slice 2's rim begins"
        );
    }

    #[test]
    fn a_zero_value_slice_paints_nothing_rather_than_a_sliver() {
        assert!(
            sector(CENTER, Twip(2_000), Twip::ZERO, 0, 0).is_empty(),
            "a zero sweep must yield no commands at all"
        );
        assert!(
            sector(CENTER, Twip(2_000), Twip(900), 0, 0).is_empty(),
            "a zero sweep must yield nothing for a doughnut either"
        );
    }

    #[test]
    fn a_doughnut_leaves_its_hole_empty() {
        // The guarantee: no part of the figure comes nearer the centre than the
        // hole radius. A wedge that reached the centre would fill the hole.
        let hole = 800.0;
        let commands = sector(CENTER, Twip(2_000), Twip(800), 0, FULL_TURN / 3);
        assert!(!commands.is_empty(), "a doughnut sector must paint");
        for (x, y) in flatten(&commands, 24) {
            let distance = (x - 5_000.0).hypot(y - 5_000.0);
            assert!(
                distance >= hole - 1.0,
                "({x}, {y}) is {distance} from the centre, inside the {hole}-twip hole"
            );
        }
        // And the pie built from the same call with a zero hole DOES reach the
        // centre, so the assertion above is discriminating rather than vacuous.
        let pie = sector(CENTER, Twip(2_000), Twip::ZERO, 0, FULL_TURN / 3);
        assert!(
            flatten(&pie, 24)
                .iter()
                .any(|(x, y)| (x - 5_000.0).hypot(y - 5_000.0) < 1.0),
            "a pie wedge must reach the centre"
        );
    }

    #[test]
    fn a_full_turn_is_a_circle_with_no_radial_seam() {
        let commands = sector(CENTER, Twip(2_000), Twip::ZERO, 0, FULL_TURN);
        assert!(
            !commands
                .iter()
                .any(|command| matches!(command, PathCommand::LineTo { .. })),
            "a whole-circle pie has no slice boundary to draw: {commands:?}"
        );
        // Four quadrant cubics and the opening move.
        assert_eq!(commands.len(), 5, "a full turn is four quadrants");
    }

    #[test]
    fn a_full_turn_doughnut_keeps_its_hole_as_a_reversed_subpath() {
        let commands = sector(CENTER, Twip(2_000), Twip(800), 0, FULL_TURN);
        let moves = commands
            .iter()
            .filter(|command| matches!(command, PathCommand::MoveTo { .. }))
            .count();
        assert_eq!(moves, 2, "a whole ring is two subpaths: {commands:?}");
        // The inner subpath runs the other way, so nonzero winding subtracts it.
        let inner: Vec<_> = flatten(&commands[5..], 24);
        for (x, y) in &inner {
            let distance = (x - 5_000.0).hypot(y - 5_000.0);
            assert!(
                (distance - 800.0).abs() <= 2.0,
                "the inner subpath must trace the hole: ({x}, {y}) is {distance} out"
            );
        }
    }

    #[test]
    fn a_hole_as_large_as_the_circle_degenerates_rather_than_inverting() {
        let commands = sector(CENTER, Twip(2_000), Twip(5_000), 0, FULL_TURN / 4);
        assert!(!commands.is_empty(), "an over-large hole must still paint");
        for (x, y) in flatten(&commands, 8) {
            let distance = (x - 5_000.0).hypot(y - 5_000.0);
            assert!(
                distance <= 2_001.0,
                "nothing may escape the outer radius: ({x}, {y}) at {distance}"
            );
        }
    }

    #[test]
    fn a_sweep_wider_than_a_quadrant_is_split_into_equal_segments() {
        // 120 degrees must not be a quadrant plus a 30-degree sliver: equal
        // segments keep the radial error even across the arc, which is the whole
        // reason the subdivision exists.
        let commands = sector(CENTER, Twip(2_000), Twip::ZERO, 0, FULL_TURN / 3);
        let cubics: Vec<_> = commands
            .iter()
            .filter(|command| matches!(command, PathCommand::CubicTo { .. }))
            .collect();
        assert_eq!(cubics.len(), 2, "120 degrees is two segments of 60");
        let tolerance = 2_000.0 * MAX_RADIAL_ERROR_RATIO + 1.0;
        for (x, y) in flatten(&commands[..=2], 32) {
            let distance = (x - 5_000.0).hypot(y - 5_000.0);
            assert!(
                (distance - 2_000.0).abs() <= tolerance,
                "({x}, {y}) is {distance} from the centre, off the 2000-twip rim by more than {tolerance}"
            );
        }
    }

    #[test]
    fn a_degenerate_radius_paints_nothing() {
        assert!(sector(CENTER, Twip::ZERO, Twip::ZERO, 0, FULL_TURN / 4).is_empty());
        assert!(sector(CENTER, Twip(-5), Twip::ZERO, 0, FULL_TURN / 4).is_empty());
    }
}
