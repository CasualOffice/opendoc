// SPDX-License-Identifier: Apache-2.0

//! Making an even-odd geometry paint the same under the non-zero rule
//! (`docs/182` gap G-2).
//!
//! # Why this exists
//!
//! A PDF fills with either rule (`f` / `f*`); DrawingML has no fill-rule
//! attribute at all — `CT_Path2D` (ECMA-376 Part 1 §20.1.9.15) carries `w`, `h`,
//! `fill`, `stroke` and `extrusionOk` and nothing else, and the standard does not
//! say which rule a consumer applies. So a DOCX writer cannot *state* even-odd.
//! What it can do is write a geometry for which the question does not arise.
//!
//! # The pattern: re-orient a laminar family of simple contours
//!
//! The two rules disagree only where the path winds around a point two or more
//! times. When a path's subpaths are simple closed curves that neither cross nor
//! touch each other, they nest as a tree (a laminar family), and every point lies
//! inside a CHAIN of them. Orienting each subpath by the parity of its nesting
//! depth — even depth one way, odd depth the other — makes the winding number of
//! every point 0 or ±1, and then non-zero and even-odd paint exactly the same
//! pixels. That is the classic orientation normalisation font tools apply to
//! TrueType contours (outer clockwise, holes counter-clockwise), and it is what
//! [`CustomGeometry::to_non_zero`] does.
//!
//! When the precondition fails — two subpaths cross, a subpath crosses itself,
//! or a coordinate is a guide name whose value depends on the box — no
//! orientation achieves it, and the function says why instead of guessing. The
//! DOCX writer then writes the geometry as authored and REPORTS the fill rule as
//! not carried (`docx.export.custom_geometry.even_odd_fill`).
//!
//! # Precision
//!
//! Curves are tested as their flattening at [`CURVE_STEPS`] chords. A crossing
//! between two curves narrower than that chordal error is not seen; the region
//! it would mis-wind is correspondingly thin. Lines — which is what nearly every
//! PDF vector page is made of — are tested exactly on their EMU integer points.

use super::{CustomGeometry, FillRule, GeometryPoint, PathFill, ShapePath, ShapePathCommand};

/// Chords per Bézier when testing for crossings and containment.
pub const CURVE_STEPS: usize = 8;

/// Why [`CustomGeometry::to_non_zero`] could not produce an equivalent
/// non-zero geometry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FillRuleRewriteError {
    /// A coordinate names a guide, or a path uses `a:arcTo`: the outline depends
    /// on the box it is evaluated at, so its windings cannot be fixed once.
    NotLiteral,
    /// Two subpaths of one path cross or touch, or one crosses itself: some
    /// region is wound twice under any orientation.
    Intersecting,
    /// An open, stroked subpath would have to be reversed, which would move its
    /// arrowheads and dash phase to the other end.
    OpenStrokedSubpath,
}

impl FillRuleRewriteError {
    /// A stable identifier for reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotLiteral => "not-literal",
            Self::Intersecting => "intersecting",
            Self::OpenStrokedSubpath => "open-stroked-subpath",
        }
    }
}

impl CustomGeometry {
    /// The same geometry filled [`FillRule::NonZero`], painting exactly the
    /// pixels this one paints under its own rule.
    ///
    /// A non-zero geometry is returned unchanged. An even-odd one has each
    /// filled path's subpaths re-oriented by nesting parity (module docs); a
    /// path that is not filled (`fill="none"`) is left as authored, because the
    /// rule does not apply to it.
    ///
    /// Complexity: O(E²) in a path's flattened edges, E ≤ `CURVE_STEPS` ×
    /// `MAX_SHAPE_PATH_COMMANDS` — bounded, so O(1) in document size. Called by a
    /// writer once per even-odd shape, never per keystroke.
    ///
    /// # Errors
    ///
    /// [`FillRuleRewriteError`] when no orientation makes the two rules agree.
    pub fn to_non_zero(&self) -> Result<Self, FillRuleRewriteError> {
        if self.fill_rule == FillRule::NonZero {
            return Ok(self.clone());
        }
        let mut out = self.clone();
        out.fill_rule = FillRule::NonZero;
        for path in &mut out.paths {
            if path.fill != PathFill::None {
                path.commands = rewound(path)?;
            }
        }
        Ok(out)
    }
}

type Pt = (i64, i64);

#[derive(Clone, Copy)]
enum Seg {
    Line(Pt),
    Quad(Pt, Pt),
    Cubic(Pt, Pt, Pt),
}

impl Seg {
    const fn end(self) -> Pt {
        match self {
            Self::Line(end) | Self::Quad(_, end) | Self::Cubic(_, _, end) => end,
        }
    }
}

/// One subpath: where it starts, its segments, and the command range it came
/// from so an unchanged subpath is copied verbatim.
struct Sub {
    explicit_move: bool,
    start: Pt,
    segs: Vec<Seg>,
    closed: bool,
    commands: std::ops::Range<usize>,
}

enum Chunk {
    Sub(Sub),
    /// A command that belongs to no subpath (a stray `close`), kept as written.
    Verbatim(usize),
}

fn literal(point: &GeometryPoint) -> Result<Pt, FillRuleRewriteError> {
    point
        .as_literal()
        .map(|p| (p.x_emu, p.y_emu))
        .ok_or(FillRuleRewriteError::NotLiteral)
}

fn split(commands: &[ShapePathCommand]) -> Result<Vec<Chunk>, FillRuleRewriteError> {
    let mut chunks = Vec::new();
    let mut current: Option<Sub> = None;
    let mut last_start: Option<Pt> = None;
    for (index, command) in commands.iter().enumerate() {
        let seg = match command {
            ShapePathCommand::MoveTo { point } => {
                if let Some(sub) = current.take() {
                    chunks.push(Chunk::Sub(sub));
                }
                let start = literal(point)?;
                last_start = Some(start);
                current = Some(Sub {
                    explicit_move: true,
                    start,
                    segs: Vec::new(),
                    closed: false,
                    commands: index..index + 1,
                });
                continue;
            }
            ShapePathCommand::Close => {
                match current.take() {
                    Some(mut sub) => {
                        sub.closed = true;
                        sub.commands.end = index + 1;
                        last_start = Some(sub.start);
                        chunks.push(Chunk::Sub(sub));
                    }
                    None => chunks.push(Chunk::Verbatim(index)),
                }
                continue;
            }
            ShapePathCommand::ArcTo { .. } => return Err(FillRuleRewriteError::NotLiteral),
            ShapePathCommand::LineTo { point } => Seg::Line(literal(point)?),
            ShapePathCommand::QuadBezTo { control, point } => {
                Seg::Quad(literal(control)?, literal(point)?)
            }
            ShapePathCommand::CubicBezTo {
                control1,
                control2,
                point,
            } => Seg::Cubic(literal(control1)?, literal(control2)?, literal(point)?),
        };
        // A segment after `close` with no `moveTo` starts a new subpath at the
        // closed one's start, as in PostScript, PDF and SVG.
        let sub = match current.as_mut() {
            Some(sub) => sub,
            None => current.insert(Sub {
                explicit_move: false,
                start: last_start.ok_or(FillRuleRewriteError::NotLiteral)?,
                segs: Vec::new(),
                closed: false,
                commands: index..index,
            }),
        };
        sub.segs.push(seg);
        sub.commands.end = index + 1;
    }
    if let Some(sub) = current {
        chunks.push(Chunk::Sub(sub));
    }
    Ok(chunks)
}

#[allow(clippy::cast_precision_loss)] // EMU are bounded by MAX_EMU, exact in f64
fn f(point: Pt) -> (f64, f64) {
    (point.0 as f64, point.1 as f64)
}

/// The subpath as a closed polygon (the closing edge is implicit).
#[allow(clippy::cast_precision_loss)] // CURVE_STEPS is tiny
fn flatten(sub: &Sub) -> Vec<(f64, f64)> {
    let mut points = vec![f(sub.start)];
    let mut from = sub.start;
    for &seg in &sub.segs {
        match seg {
            Seg::Line(end) => points.push(f(end)),
            Seg::Quad(c, end) => {
                let (p0, p1, p2) = (f(from), f(c), f(end));
                for step in 1..=CURVE_STEPS {
                    let t = step as f64 / CURVE_STEPS as f64;
                    let u = 1.0 - t;
                    points.push((
                        u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0,
                        u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1,
                    ));
                }
            }
            Seg::Cubic(c1, c2, end) => {
                let (p0, p1, p2, p3) = (f(from), f(c1), f(c2), f(end));
                for step in 1..=CURVE_STEPS {
                    let t = step as f64 / CURVE_STEPS as f64;
                    let u = 1.0 - t;
                    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                    points.push((
                        a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0,
                        a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1,
                    ));
                }
            }
        }
        from = seg.end();
    }
    points.dedup();
    if points.len() > 1 && points.first() == points.last() {
        points.pop();
    }
    points
}

fn edges(polygon: &[(f64, f64)]) -> Vec<((f64, f64), (f64, f64))> {
    if polygon.len() < 2 {
        return Vec::new();
    }
    (0..polygon.len())
        .map(|i| (polygon[i], polygon[(i + 1) % polygon.len()]))
        .collect()
}

fn cross(o: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
}

fn on_segment(a: (f64, f64), b: (f64, f64), p: (f64, f64)) -> bool {
    p.0 >= a.0.min(b.0) && p.0 <= a.0.max(b.0) && p.1 >= a.1.min(b.1) && p.1 <= a.1.max(b.1)
}

/// Whether two closed segments share any point (crossing, touching, or
/// overlapping collinearly).
fn segments_meet(a: (f64, f64), b: (f64, f64), c: (f64, f64), d: (f64, f64)) -> bool {
    let (d1, d2) = (cross(c, d, a), cross(c, d, b));
    let (d3, d4) = (cross(a, b, c), cross(a, b, d));
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }
    (d1 == 0.0 && on_segment(c, d, a))
        || (d2 == 0.0 && on_segment(c, d, b))
        || (d3 == 0.0 && on_segment(a, b, c))
        || (d4 == 0.0 && on_segment(a, b, d))
}

fn bounds(polygon: &[(f64, f64)]) -> (f64, f64, f64, f64) {
    polygon.iter().fold(
        (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ),
        |(x0, y0, x1, y1), &(x, y)| (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
    )
}

fn boxes_meet(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> bool {
    a.0 <= b.2 && b.0 <= a.2 && a.1 <= b.3 && b.1 <= a.3
}

/// Twice the signed area (shoelace).
fn signed_area(polygon: &[(f64, f64)]) -> f64 {
    edges(polygon)
        .iter()
        .map(|&(a, b)| a.0 * b.1 - b.0 * a.1)
        .sum()
}

/// Even-odd point-in-polygon by ray crossing.
fn contains(polygon: &[(f64, f64)], p: (f64, f64)) -> bool {
    let mut inside = false;
    for (a, b) in edges(polygon) {
        if (a.1 > p.1) != (b.1 > p.1) {
            let x = a.0 + (p.1 - a.1) * (b.0 - a.0) / (b.1 - a.1);
            if p.0 < x {
                inside = !inside;
            }
        }
    }
    inside
}

/// Whether a simple polygon's own edges cross (non-adjacent edges meet).
fn self_intersects(polygon: &[(f64, f64)]) -> bool {
    let edges = edges(polygon);
    let n = edges.len();
    for i in 0..n {
        for j in i + 1..n {
            let adjacent = j == i + 1 || (i == 0 && j == n - 1);
            if adjacent {
                continue;
            }
            if segments_meet(edges[i].0, edges[i].1, edges[j].0, edges[j].1) {
                return true;
            }
        }
    }
    false
}

fn polygons_meet(a: &[(f64, f64)], b: &[(f64, f64)]) -> bool {
    let b_edges = edges(b);
    edges(a)
        .iter()
        .any(|&(p, q)| b_edges.iter().any(|&(r, s)| segments_meet(p, q, r, s)))
}

fn point_command(point: Pt) -> GeometryPoint {
    GeometryPoint::literal(point.0, point.1)
}

/// The subpath drawn the other way round, as commands.
fn reversed(sub: &Sub) -> Vec<ShapePathCommand> {
    let mut ends = Vec::with_capacity(sub.segs.len() + 1);
    ends.push(sub.start);
    ends.extend(sub.segs.iter().map(|seg| seg.end()));
    let mut commands = vec![ShapePathCommand::MoveTo {
        point: point_command(*ends.last().unwrap_or(&sub.start)),
    }];
    for (index, seg) in sub.segs.iter().enumerate().rev() {
        let to = point_command(ends[index]);
        commands.push(match *seg {
            Seg::Line(_) => ShapePathCommand::LineTo { point: to },
            Seg::Quad(control, _) => ShapePathCommand::QuadBezTo {
                control: point_command(control),
                point: to,
            },
            Seg::Cubic(control1, control2, _) => ShapePathCommand::CubicBezTo {
                control1: point_command(control2),
                control2: point_command(control1),
                point: to,
            },
        });
    }
    if sub.closed {
        commands.push(ShapePathCommand::Close);
    }
    commands
}

fn rewound(path: &ShapePath) -> Result<Vec<ShapePathCommand>, FillRuleRewriteError> {
    let chunks = split(&path.commands)?;
    let subs: Vec<&Sub> = chunks
        .iter()
        .filter_map(|chunk| match chunk {
            Chunk::Sub(sub) if !sub.segs.is_empty() => Some(sub),
            _ => None,
        })
        .collect();
    let polygons: Vec<Vec<(f64, f64)>> = subs.iter().map(|sub| flatten(sub)).collect();
    let boxes: Vec<_> = polygons.iter().map(|polygon| bounds(polygon)).collect();
    for (i, polygon) in polygons.iter().enumerate() {
        if self_intersects(polygon) {
            return Err(FillRuleRewriteError::Intersecting);
        }
        for j in i + 1..polygons.len() {
            if boxes_meet(boxes[i], boxes[j]) && polygons_meet(polygon, &polygons[j]) {
                return Err(FillRuleRewriteError::Intersecting);
            }
        }
    }
    // Nesting depth: how many other subpaths contain this one. Disjoint simple
    // contours contain one another wholly or not at all, so one vertex decides.
    let mut reverse = Vec::with_capacity(subs.len());
    for (i, polygon) in polygons.iter().enumerate() {
        let probe = polygon[0];
        let depth = polygons
            .iter()
            .enumerate()
            .filter(|&(j, other)| j != i && contains(other, probe))
            .count();
        let area = signed_area(polygon);
        let wanted_positive = depth % 2 == 0;
        let flip = area != 0.0 && (area > 0.0) != wanted_positive;
        if flip && !subs[i].closed && path.stroke {
            return Err(FillRuleRewriteError::OpenStrokedSubpath);
        }
        reverse.push(flip);
    }
    let mut out = Vec::with_capacity(path.commands.len() + subs.len());
    let mut next = 0;
    for chunk in &chunks {
        match chunk {
            Chunk::Verbatim(index) => out.push(path.commands[*index].clone()),
            Chunk::Sub(sub) if sub.segs.is_empty() => {
                out.extend(path.commands[sub.commands.clone()].iter().cloned());
            }
            Chunk::Sub(sub) => {
                if reverse[next] {
                    out.extend(reversed(sub));
                } else {
                    if !sub.explicit_move {
                        out.push(ShapePathCommand::MoveTo {
                            point: point_command(sub.start),
                        });
                    }
                    out.extend(path.commands[sub.commands.clone()].iter().cloned());
                }
                next += 1;
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::GeometryValue;

    fn square(x: i64, y: i64, size: i64, clockwise: bool) -> Vec<ShapePathCommand> {
        let corners = if clockwise {
            [(x, y), (x + size, y), (x + size, y + size), (x, y + size)]
        } else {
            [(x, y), (x, y + size), (x + size, y + size), (x + size, y)]
        };
        let mut commands = vec![ShapePathCommand::MoveTo {
            point: GeometryPoint::literal(corners[0].0, corners[0].1),
        }];
        for &(cx, cy) in &corners[1..] {
            commands.push(ShapePathCommand::LineTo {
                point: GeometryPoint::literal(cx, cy),
            });
        }
        commands.push(ShapePathCommand::Close);
        commands
    }

    fn even_odd(commands: Vec<ShapePathCommand>) -> CustomGeometry {
        CustomGeometry::single_path(ShapePath::new(commands)).with_fill_rule(FillRule::EvenOdd)
    }

    /// The non-zero winding number of `p` around a lines-only path.
    fn winding(commands: &[ShapePathCommand], p: (f64, f64)) -> i32 {
        let mut total = 0;
        let mut polygon: Vec<(f64, f64)> = Vec::new();
        let mut flush = |polygon: &mut Vec<(f64, f64)>| {
            for (a, b) in edges(polygon) {
                if a.1 <= p.1 && b.1 > p.1 && cross(a, b, p) > 0.0 {
                    total += 1;
                } else if a.1 > p.1 && b.1 <= p.1 && cross(a, b, p) < 0.0 {
                    total -= 1;
                }
            }
            polygon.clear();
        };
        for command in commands {
            match command {
                ShapePathCommand::MoveTo { point } => {
                    flush(&mut polygon);
                    polygon.push(f(literal(point).unwrap()));
                }
                ShapePathCommand::LineTo { point } => polygon.push(f(literal(point).unwrap())),
                _ => {}
            }
        }
        flush(&mut polygon);
        total
    }

    /// Two squares wound the SAME way: under even-odd the inner one is a hole,
    /// under non-zero it is not. The rewrite must reverse the inner one so the
    /// non-zero result has the hole.
    #[test]
    fn a_same_wound_hole_is_reversed_so_non_zero_keeps_it() {
        let mut commands = square(0, 0, 100, true);
        commands.extend(square(25, 25, 50, true));
        let before = even_odd(commands);
        assert_eq!(winding(&before.paths[0].commands, (50.0, 50.0)).abs(), 2);

        let after = before.to_non_zero().expect("nested squares are rewritable");
        assert_eq!(after.fill_rule, FillRule::NonZero);
        let commands = &after.paths[0].commands;
        assert_eq!(winding(commands, (50.0, 50.0)), 0, "the centre is a hole");
        assert_eq!(
            winding(commands, (10.0, 10.0)).abs(),
            1,
            "the ring is filled"
        );
        assert_eq!(
            winding(commands, (200.0, 200.0)),
            0,
            "outside stays outside"
        );
    }

    /// Three levels: the middle contour flips, the innermost does not — every
    /// winding ends up 0 or ±1 and matches even-odd parity.
    #[test]
    fn nesting_parity_decides_each_orientation() {
        let mut commands = square(0, 0, 300, true);
        commands.extend(square(50, 50, 200, true));
        commands.extend(square(100, 100, 100, true));
        let after = even_odd(commands).to_non_zero().unwrap();
        let commands = &after.paths[0].commands;
        for (point, inside) in [
            ((25.0, 25.0), true),
            ((75.0, 75.0), false),
            ((150.0, 150.0), true),
        ] {
            assert_eq!(winding(commands, point) != 0, inside, "{point:?}");
            assert!(winding(commands, point).abs() <= 1, "{point:?}");
        }
    }

    #[test]
    fn crossing_subpaths_cannot_be_rewritten() {
        let mut commands = square(0, 0, 100, true);
        commands.extend(square(50, 50, 100, true));
        assert_eq!(
            even_odd(commands).to_non_zero(),
            Err(FillRuleRewriteError::Intersecting)
        );
    }

    #[test]
    fn a_guide_named_coordinate_cannot_be_rewritten() {
        let mut commands = square(0, 0, 100, true);
        commands[1] = ShapePathCommand::LineTo {
            point: GeometryPoint {
                x: GeometryValue::Guide("r".to_owned()),
                y: GeometryValue::Literal(0),
            },
        };
        assert_eq!(
            even_odd(commands).to_non_zero(),
            Err(FillRuleRewriteError::NotLiteral)
        );
    }

    #[test]
    fn a_non_zero_geometry_is_returned_unchanged() {
        let mut commands = square(0, 0, 100, true);
        commands.extend(square(25, 25, 50, true));
        let geometry = CustomGeometry::single_path(ShapePath::new(commands));
        assert_eq!(geometry.to_non_zero().unwrap(), geometry);
    }

    /// A reversed curve swaps its control points, so it is the same curve.
    #[test]
    fn a_reversed_cubic_keeps_its_shape() {
        let mut commands = square(0, 0, 1000, true);
        commands.extend([
            ShapePathCommand::MoveTo {
                point: GeometryPoint::literal(200, 500),
            },
            ShapePathCommand::CubicBezTo {
                control1: GeometryPoint::literal(200, 200),
                control2: GeometryPoint::literal(800, 200),
                point: GeometryPoint::literal(800, 500),
            },
            ShapePathCommand::LineTo {
                point: GeometryPoint::literal(500, 800),
            },
            ShapePathCommand::Close,
        ]);
        let after = even_odd(commands.clone()).to_non_zero().unwrap();
        let chunks = split(&after.paths[0].commands).unwrap();
        let Chunk::Sub(inner) = &chunks[1] else {
            panic!("two subpaths")
        };
        let Chunk::Sub(original) = &split(&commands).unwrap()[1] else {
            panic!("two subpaths")
        };
        let mut a = flatten(original);
        let mut b = flatten(inner);
        let norm = |p: &mut Vec<(f64, f64)>| {
            for q in p.iter_mut() {
                *q = ((q.0 * 1e6).round(), (q.1 * 1e6).round());
            }
            p.sort_by(|x, y| x.partial_cmp(y).unwrap());
        };
        norm(&mut a);
        norm(&mut b);
        assert_eq!(a, b, "the same points, traversed the other way");
        assert!(signed_area(&flatten(original)) * signed_area(&flatten(inner)) < 0.0);
    }
}
