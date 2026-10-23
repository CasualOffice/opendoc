//! The model half of the VML normalisation: pure functions from the neutral
//! VML intermediate (`crate::vml`) to the typed DrawingML model the renderer
//! already draws (`docs/08` ADR-072).
//!
//! `body.rs` owns the glue that needs the parser's state (ids, the media
//! table, the deferred text-box blocks); everything here is a total function
//! of its arguments, so it is tested without a document.

use casual_doc_model::v1::{
    CustomGeometry, DashStyle, GeometryPoint, GeometryProgram, LineEnd, LineEndKind, PathFill,
    ShapePath, ShapePathCommand, ShapeStroke,
};

use crate::vml::{VmlArrow, VmlDash, VmlGeometry, VmlPathCommand, VmlStroke};

/// An evaluated VML path as a DrawingML custom geometry: one `ShapePath` per
/// subpath, in the path's own coordinate space (`coordsize`), which the
/// renderer maps onto the shape's box exactly as it maps an `a:path@w/h`.
///
/// `None` when the result would not compile (an out-of-range coordinate, too
/// many commands): the importer attaches only a geometry the renderer can
/// draw, and the caller keeps its bounding-box fallback.
///
/// O(commands).
#[must_use]
pub(crate) fn custom_geometry(geometry: &VmlGeometry) -> Option<CustomGeometry> {
    // Coordinates are rounded to whole units of the coordinate space: VML's own
    // resolution, and what the model's literal geometry values hold.
    #[allow(clippy::cast_possible_truncation)]
    let point = |x: f64, y: f64| GeometryPoint::literal(x.round() as i64, y.round() as i64);
    let paths = geometry
        .subpaths
        .iter()
        .map(|subpath| ShapePath {
            width_emu: geometry.width,
            height_emu: geometry.height,
            fill: if subpath.fill {
                PathFill::Norm
            } else {
                PathFill::None
            },
            stroke: subpath.stroke,
            extrusion_ok: true,
            commands: subpath
                .commands
                .iter()
                .map(|command| match *command {
                    VmlPathCommand::MoveTo(x, y) => ShapePathCommand::MoveTo { point: point(x, y) },
                    VmlPathCommand::LineTo(x, y) => ShapePathCommand::LineTo { point: point(x, y) },
                    VmlPathCommand::CubicTo(c) => ShapePathCommand::CubicBezTo {
                        control1: point(c[0], c[1]),
                        control2: point(c[2], c[3]),
                        point: point(c[4], c[5]),
                    },
                    VmlPathCommand::Close => ShapePathCommand::Close,
                })
                .collect(),
        })
        .collect();
    let custom = CustomGeometry {
        paths,
        ..CustomGeometry::default()
    };
    (custom.check(&[]).is_ok() && GeometryProgram::compile(&[], &custom).is_ok()).then_some(custom)
}

/// A VML dash as the DrawingML preset dash with the same rhythm. VML's `short*`
/// family is DrawingML's `sys*` family (dashes measured in line widths), and
/// `long*` is `lg*`.
#[must_use]
pub(crate) const fn dash(dash: VmlDash) -> DashStyle {
    match dash {
        VmlDash::ShortDash => DashStyle::SystemDash,
        VmlDash::ShortDot => DashStyle::SystemDot,
        VmlDash::ShortDashDot => DashStyle::SystemDashDot,
        VmlDash::ShortDashDotDot => DashStyle::SystemDashDotDot,
        VmlDash::Dot => DashStyle::Dot,
        VmlDash::Dash => DashStyle::Dash,
        VmlDash::LongDash => DashStyle::LargeDash,
        VmlDash::DashDot => DashStyle::DashDot,
        VmlDash::LongDashDot => DashStyle::LargeDashDot,
        VmlDash::LongDashDotDot => DashStyle::LargeDashDotDot,
    }
}

/// A VML arrowhead as a DrawingML line end of the same shape.
#[must_use]
pub(crate) const fn arrow(arrow: VmlArrow) -> LineEnd {
    LineEnd {
        kind: match arrow {
            VmlArrow::Block => LineEndKind::Triangle,
            VmlArrow::Classic => LineEndKind::Stealth,
            VmlArrow::Open => LineEndKind::Arrow,
            VmlArrow::Oval => LineEndKind::Oval,
            VmlArrow::Diamond => LineEndKind::Diamond,
        },
        width: None,
        length: None,
    }
}

/// Carries a VML stroke's dash and arrowheads onto an already-resolved model
/// stroke. `reversed` swaps the ends, for a line whose flips run it backwards.
pub(crate) fn apply_stroke_details(stroke: &mut ShapeStroke, vml: &VmlStroke, reversed: bool) {
    stroke.dash = vml.dash.map(dash);
    let (head, tail) = (vml.start_arrow.map(arrow), vml.end_arrow.map(arrow));
    (stroke.head_end, stroke.tail_end) = if reversed { (tail, head) } else { (head, tail) };
}

/// The flips that make DrawingML's top-left-to-bottom-right line run from
/// `from` to `to`: `(flip_h, flip_v)`. A line running up and right is
/// vertically flipped, down and left horizontally, up and left both.
#[must_use]
pub(crate) const fn line_flips(from: (i64, i64), to: (i64, i64)) -> (bool, bool) {
    let leftward = to.0 < from.0;
    let upward = to.1 < from.1;
    match (leftward, upward) {
        (false, false) => (false, false),
        (true, false) => (true, false),
        (false, true) => (false, true),
        (true, true) => (true, true),
    }
}

/// A style `rotation` (degrees clockwise) as the model's 60000ths of a degree,
/// normalised to one turn. `None` for no rotation.
#[must_use]
pub(crate) fn rotation(degrees: Option<f64>) -> Option<i32> {
    let degrees = degrees.filter(|value| value.is_finite())?.rem_euclid(360.0);
    #[allow(clippy::cast_possible_truncation)]
    let angle = (degrees * 60_000.0).round() as i32;
    (angle != 0 && angle != 21_600_000).then_some(angle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vml::parse_vml_pict;

    #[test]
    fn a_closed_path_becomes_a_compiling_custom_geometry() {
        let drawing = &parse_vml_pict(
            r#"<w:pict><v:shape style="position:absolute;margin-left:0;margin-top:0;width:100pt;height:50pt" coordsize="200,100" path="m0,0l200,0,200,100,0,100xe"/></w:pict>"#,
        )[0];
        let geometry = drawing.geometry.as_ref().expect("the path evaluates");
        let custom = custom_geometry(geometry).expect("compiles");
        assert_eq!(custom.paths.len(), 1);
        assert_eq!(custom.paths[0].width_emu, 200);
        assert_eq!(custom.paths[0].commands.len(), 5);
    }

    #[test]
    fn line_flips_cover_all_four_directions() {
        assert_eq!(line_flips((0, 0), (10, 10)), (false, false));
        assert_eq!(line_flips((10, 0), (0, 10)), (true, false));
        assert_eq!(line_flips((0, 10), (10, 0)), (false, true));
        assert_eq!(line_flips((10, 10), (0, 0)), (true, true));
    }

    #[test]
    fn rotation_is_normalised_and_zero_is_none() {
        assert_eq!(rotation(Some(90.0)), Some(5_400_000));
        assert_eq!(rotation(Some(-90.0)), Some(16_200_000));
        assert_eq!(rotation(Some(360.0)), None);
        assert_eq!(rotation(None), None);
    }
}
