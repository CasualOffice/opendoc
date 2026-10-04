// SPDX-License-Identifier: Apache-2.0

//! The DrawingML preset shape geometries (`a:prstGeom@prst`), resolved from the
//! committed ECMA-376 table rather than hand-written per shape.
//!
//! # What this replaces
//!
//! `ShapeGeometry` types 22 of the 187 presets, each with its own hand-coded vertex
//! list, and everything else painted as its bounding rectangle — so a Word arrow,
//! callout, banner or flowchart symbol drew as a box. `119` §6 named the way out: a
//! path is the primitive and a preset is a **recipe**, read as data and evaluated by
//! one interpreter, so the 165th preset costs no more code than the 23rd.
//!
//! # Where the data comes from
//!
//! `shape_preset_table.txt`, generated from `fixtures/spec/presetShapeDefinitions.xml`
//! by `casual-doc-ooxml`'s `generate_preset_table` example. That input is ECMA-376's
//! own normative preset data, vendored from Apache POI's Apache-2.0 redistribution;
//! `fixtures/spec/README.md` records the provenance and why ONLYOFFICE's AGPL
//! transcription of the same presets is deliberately not the source.
//!
//! # How much of it resolves today
//!
//! All 187. 124 use only `moveTo`/`lnTo`/`cubicBezTo`/`quadBezTo`/`close`, which the
//! path primitive already paints; the other 63 also use `a:arcTo`, and **zero** need
//! anything beyond that. `a:arcTo` is evaluated here into cubic Bézier segments
//! (§20.1.9.4), so the whole table draws and nothing falls back to its bounding
//! rectangle for want of an opcode.
//!
//! A row the table cannot read at all is still refused rather than dropped: a preset
//! drawn with one segment missing is a different shape, and it would look deliberate.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use casual_doc_model::v1::ShapeAdjustment;

use crate::display::PathCommand;
use crate::shape_guide::{GuideBox, Resolved};
use crate::units::{Point, Rect, Twip, twip_rounded};

/// The committed table, derived from the vendored specification data.
const TABLE: &str = include_str!("shape_preset_table.txt");

/// One path command of a preset definition, with coordinates still as authored
/// expression tokens.
#[derive(Clone, Copy, Debug)]
enum PresetCommand<'a> {
    Move([&'a str; 2]),
    Line([&'a str; 2]),
    Cubic([&'a str; 6]),
    Quad([&'a str; 4]),
    /// `a:arcTo`: `[wR, hR, stAng, swAng]`, in the element's own attribute order.
    /// The radii are lengths in the path's coordinate space; the angles are in
    /// 1/60000 of a degree.
    Arc([&'a str; 4]),
    Close,
    /// A table row whose arity matches no command.
    ///
    /// Carried rather than dropped, because dropping a segment silently changes the
    /// shape into one that looks deliberate: the preset it appears in refuses to
    /// resolve, and the caller keeps its reported bounding rectangle.
    Unreadable,
}

/// One `a:path` of a preset definition.
#[derive(Clone, Debug, Default)]
struct PresetPath<'a> {
    /// `a:path@w`/`@h`: the path's own coordinate space, or `None` when absent, in
    /// which case coordinates are already in the shape's box units.
    width: Option<f64>,
    height: Option<f64>,
    commands: Vec<PresetCommand<'a>>,
}

/// One preset's recipe: its adjust defaults, its computed guides, and its paths.
#[derive(Clone, Debug, Default)]
struct PresetDefinition<'a> {
    /// `a:avLst` defaults, in order. An authored `a:avLst` overrides by name.
    adjust: Vec<(&'a str, &'a str)>,
    /// `a:gdLst`, in order. Order is load-bearing: a guide may name an earlier one.
    guides: Vec<(&'a str, &'a str)>,
    paths: Vec<PresetPath<'a>>,
}

/// The parsed table, built once.
fn table() -> &'static BTreeMap<&'static str, PresetDefinition<'static>> {
    static TABLE_ONCE: OnceLock<BTreeMap<&'static str, PresetDefinition<'static>>> =
        OnceLock::new();
    TABLE_ONCE.get_or_init(parse_table)
}

/// Parses the committed table.
///
/// Lazily, once per process, and only when a document actually reaches a preset — a
/// document with no shapes never pays for it. Complexity: O(n) in the table, which is
/// a fixed 108 KB and independent of document size.
fn parse_table() -> BTreeMap<&'static str, PresetDefinition<'static>> {
    let mut presets: BTreeMap<&'static str, PresetDefinition<'static>> = BTreeMap::new();
    let mut token: Option<&'static str> = None;
    let mut current = PresetDefinition::default();
    for line in TABLE.lines() {
        let mut fields = line.split('\t');
        let Some(kind) = fields.next() else { continue };
        match kind {
            "P" => {
                if let Some(previous) = token.take() {
                    presets.insert(previous, core::mem::take(&mut current));
                }
                token = fields.next();
            }
            "A" | "G" => {
                if let (Some(name), Some(formula)) = (fields.next(), fields.next()) {
                    if kind == "A" {
                        current.adjust.push((name, formula));
                    } else {
                        current.guides.push((name, formula));
                    }
                }
            }
            "H" => {
                let width = fields.next().and_then(|value| value.parse().ok());
                let height = fields.next().and_then(|value| value.parse().ok());
                current.paths.push(PresetPath {
                    width,
                    height,
                    commands: Vec::new(),
                });
            }
            "M" | "L" | "C" | "Q" | "R" | "Z" => {
                // A command before any `H` belongs to an implicit path, which the
                // generator does not emit; guard rather than index out of bounds.
                let Some(path) = current.paths.last_mut() else {
                    continue;
                };
                let rest: Vec<&str> = fields.collect();
                let command = match (kind, rest.len()) {
                    ("M", 2) => PresetCommand::Move([rest[0], rest[1]]),
                    ("L", 2) => PresetCommand::Line([rest[0], rest[1]]),
                    ("C", 6) => {
                        PresetCommand::Cubic([rest[0], rest[1], rest[2], rest[3], rest[4], rest[5]])
                    }
                    ("Q", 4) => PresetCommand::Quad([rest[0], rest[1], rest[2], rest[3]]),
                    ("R", 4) => PresetCommand::Arc([rest[0], rest[1], rest[2], rest[3]]),
                    ("Z", 0) => PresetCommand::Close,
                    // A command whose arity does not match is not a command; dropping
                    // it would silently change the shape, so the whole preset is made
                    // undrawable instead.
                    _ => PresetCommand::Unreadable,
                };
                path.commands.push(command);
            }
            _ => {}
        }
    }
    if let Some(last) = token {
        presets.insert(last, current);
    }
    presets
}

/// How many presets the table carries. For guards, so the count is derived.
#[must_use]
pub fn preset_count() -> usize {
    table().len()
}

/// Whether a preset token is in the table at all.
#[must_use]
pub fn is_known(token: &str) -> bool {
    table().contains_key(token)
}

/// DrawingML angles are 1/60000 of a degree.
///
/// `shape_guide` holds the same constant for the angle operands of the formula
/// language; this one is for the angle attributes of `a:arcTo`, which are coordinate
/// tokens rather than formulas and so never pass through it.
const ANGLE_UNITS_PER_DEGREE: f64 = 60_000.0;

/// The largest sweep one cubic Bézier segment of an arc may cover: a quarter turn.
///
/// The error of the `k = 4/3·tan(Δ/4)` approximation grows steeply with Δ — about
/// 2.7 × 10⁻⁴ of the radius at 90°, which is a fifth of a twip on a page-wide shape,
/// and roughly 2 × 10⁻² at 180°. Beyond 180° a single cubic cannot even reach the
/// far side of the ellipse, so `pie`'s 270° default would draw as a bulge rather
/// than three quarters of an ellipse. Splitting is therefore not a refinement.
const MAX_ARC_SEGMENT: f64 = std::f64::consts::FRAC_PI_2;

/// The most full turns one `a:arcTo` may sweep before the preset is refused.
///
/// No preset in the table sweeps more than one turn; the bound is on *authored*
/// adjustments, which reach the angles through the guides and are not clamped by a
/// `pin` in every preset. It keeps the emitted segment count — and so the display
/// list — bounded by the specification's data rather than by an authored number.
const MAX_ARC_TURNS: f64 = 4.0;

/// One `a:arcTo` angle in radians.
fn arc_radians(angle: f64) -> f64 {
    (angle / ANGLE_UNITS_PER_DEGREE).to_radians()
}

/// The ellipse *parameter* of the point an `a:arcTo` angle names.
///
/// `stAng`/`swAng` are **geometric** angles — the direction from the centre to the
/// point — which on a non-circular ellipse is not the parameter `t` of
/// `(wR·cos t, hR·sin t)`. ECMA-376's own preset definitions settle which of the two
/// it is, and they are the reason this conversion exists rather than being skipped:
///
/// - `pie`, `arc` and `blockArc` compute the point they `moveTo` just before the arc
///   as `hc + wd2·cos(atan2(wd2·sin stAng, hd2·cos stAng))` — the `cat2`/`sat2`
///   opcode pair — which is this function written in the formula language.
/// - `circularArrow` goes the other way: it starts its inner arc at `xC,yC` and
///   passes `istAng = at2 sdxC sdyC`, the arctangent of that very point.
///
/// Reading `stAng` as the parameter instead places the centre off the shape's own
/// centre for every angle that is not a multiple of a quarter turn, which is why
/// the error hides: every literal angle in the table is such a multiple, and only
/// the guide-driven ones (and any authored adjustment) expose it. The wedge then
/// detaches from the radius drawn after it.
///
/// `tan t = (wR/hR)·tan θ` is the relation. It has period π, so the half-turn the
/// angle sits in is taken off first and added back, which keeps the result
/// continuous and strictly increasing over the whole real line — `atan2` alone
/// would fold a full turn down to nothing and a 360° sweep would draw no arc.
fn arc_parameter(angle: f64, w_r: f64, h_r: f64) -> f64 {
    let half_turns = (angle / std::f64::consts::PI).round();
    let base = angle - half_turns * std::f64::consts::PI;
    (w_r * base.sin()).atan2(h_r * base.cos()) + half_turns * std::f64::consts::PI
}

/// Appends one `a:arcTo` (ECMA-376 Part 1 §20.1.9.4) to `out` as cubic Bézier
/// segments, answering the pen position it leaves behind — or `None` to refuse the
/// whole preset.
///
/// `start` is the pen position in the path's **own** coordinate space, and `place`
/// scales a path-space point onto the shape's box exactly as it does for every other
/// command, so `a:path@w`/`@h` applies to an arc through the same code rather than a
/// parallel one. The radii are path-space lengths too, so the ellipse's aspect — and
/// with it `arc_parameter` — is computed before any scaling, which is what makes a
/// non-square path space come out right.
///
/// The centre is **derived from the pen**, not assumed: §20.1.9.4 says the start
/// angle "is locked to the last known pen position", so
/// `centre = start − (wR·cos t₀, hR·sin t₀)`. Taking the pen for the centre instead
/// is the classic `arcTo` defect — the arc then begins a radius away from the segment
/// before it and the outline tears open.
///
/// A negative `swAng` sweeps anticlockwise, which falls out of the arithmetic: the
/// parameter span is signed, the per-segment step inherits that sign, and
/// `tan(Δ/4)` turns negative with it so the control points lean the other way.
///
/// Complexity: O(1) — at most `4·MAX_ARC_TURNS` segments.
fn push_arc(
    out: &mut Vec<PathCommand>,
    start: (f64, f64),
    [w_r, h_r, st_ang, sw_ang]: [f64; 4],
    place: &impl Fn(f64, f64) -> Point,
) -> Option<(f64, f64)> {
    // A radius of zero has no extent, so there is no ellipse to walk and the pen
    // stays put. Emitting anything would mean dividing by it and handing the Bézier
    // maths a NaN, and refusing the preset over it would lose the rest of a shape
    // whose other segments are fine.
    if !(w_r > 0.0 && h_r > 0.0) {
        return Some(start);
    }
    let from = arc_parameter(arc_radians(st_ang), w_r, h_r);
    let span = arc_parameter(arc_radians(st_ang + sw_ang), w_r, h_r) - from;
    // A zero sweep covers no angle: one degenerate zero-length cubic in the display
    // list for every caller to step over, and `tan(0)` control points on top of
    // their endpoints. `donut`'s inner ring is four arcs, so this is a real row.
    if span == 0.0 {
        return Some(start);
    }
    if !span.is_finite() || span.abs() > MAX_ARC_TURNS * std::f64::consts::TAU {
        return None;
    }
    // `arc_radians` cannot represent a quarter turn exactly, so the ratio for a 90°
    // sweep lands either side of 1.0; without the tolerance the quarter-turn arcs
    // that make up `ellipse` and `donut` would each split in two over a rounding
    // error, doubling the display list for no change in shape.
    let count = (span.abs() / MAX_ARC_SEGMENT - 1e-9).ceil().max(1.0);
    let step = span / count;
    let centre = (start.0 - w_r * from.cos(), start.1 - h_r * from.sin());
    let at = |t: f64| (centre.0 + w_r * t.cos(), centre.1 + h_r * t.sin());
    // The tangent dP/dt. Its length carries the two radii separately, which is what
    // keeps the approximation elliptical instead of collapsing it to a circle.
    let tangent = |t: f64| (-w_r * t.sin(), h_r * t.cos());
    let handle = 4.0 / 3.0 * (step / 4.0).tan();
    let mut pen = start;
    for index in 0..count as usize {
        let t0 = from + step * index as f64;
        let t1 = t0 + step;
        let (p0, p1) = (at(t0), at(t1));
        let (d0, d1) = (tangent(t0), tangent(t1));
        out.push(PathCommand::CubicTo {
            control1: place(p0.0 + handle * d0.0, p0.1 + handle * d0.1),
            control2: place(p1.0 - handle * d1.0, p1.1 - handle * d1.1),
            point: place(p1.0, p1.1),
        });
        pen = p1;
    }
    Some(pen)
}

/// The resolved outline of a preset, in page-local twips, or `None` when this build
/// cannot draw it.
///
/// `None` means the caller keeps today's behaviour — the bounding rectangle, reported
/// — and happens when the token is unknown, when a coordinate does not resolve, or
/// when the table holds a row that is not a command. It never means "draw something
/// close". Every one of the 187 presets resolves for its own default adjustments.
///
/// `authored` is the shape's own `a:avLst`, which overrides the preset's defaults by
/// name; an authored guide may itself be a formula, which the evaluator handles.
///
/// Complexity: O(g + c) in the preset's guides and commands, both fixed per preset and
/// independent of document size.
#[must_use]
pub fn preset_outline(
    token: &str,
    authored: &[ShapeAdjustment],
    rect: Rect,
) -> Option<Vec<PathCommand>> {
    let definition = table().get(token)?;
    let shape = GuideBox::new(
        f64::from(rect.size.width.raw()),
        f64::from(rect.size.height.raw()),
    );
    // The adjust list first, with the shape's own values replacing the preset defaults
    // by name, then the computed guides — which is the order the definitions assume,
    // since a `gdLst` formula routinely names an `adj`.
    let mut pairs: Vec<(&str, &str)> =
        Vec::with_capacity(definition.adjust.len() + definition.guides.len() + authored.len());
    for (name, formula) in &definition.adjust {
        let override_formula = authored
            .iter()
            .find(|guide| guide.name == *name)
            .map(|guide| guide.formula.as_str());
        pairs.push((name, override_formula.unwrap_or(formula)));
    }
    // An authored guide the preset does not declare is still in scope: Word writes
    // extra `a:gd`s for some shapes, and a path may name one.
    for guide in authored {
        if !definition
            .adjust
            .iter()
            .any(|(name, _)| *name == guide.name.as_str())
        {
            pairs.push((guide.name.as_str(), guide.formula.as_str()));
        }
    }
    pairs.extend(definition.guides.iter().copied());
    let resolved = Resolved::new(pairs, shape);

    let mut out = Vec::new();
    for path in &definition.paths {
        // `@w`/`@h` give the path its own coordinate space, scaled onto the box; absent,
        // the coordinates are already in box units because the guides produced them
        // there. Getting this backwards collapses a shape to a corner, which is why the
        // two cases are separate rather than defaulting one to the other.
        let scale = |value: f64, space: Option<f64>, extent: Twip| -> f64 {
            match space {
                Some(space) if space > 0.0 => value * f64::from(extent.raw()) / space,
                _ => value,
            }
        };
        // Path space -> box space, for a value already resolved. Separate from the
        // token lookup because `a:arcTo` resolves four tokens that are not a point
        // and then produces points of its own.
        let place = |x: f64, y: f64| -> Point {
            Point::new(
                Twip(rect.origin.x.raw()) + twip_rounded(scale(x, path.width, rect.size.width)),
                Twip(rect.origin.y.raw()) + twip_rounded(scale(y, path.height, rect.size.height)),
            )
        };
        let pair = |x: &str, y: &str| -> Option<(f64, f64)> {
            Some((resolved.token(x)?, resolved.token(y)?))
        };
        // The pen, in path space and unrounded. `a:arcTo` needs it: its centre is
        // derived from the pen, so the arc has to see the coordinate the previous
        // command resolved to rather than the twip it was rounded to — rounding the
        // centre of a page-wide ellipse would move its far side by a twip.
        let mut pen: Option<(f64, f64)> = None;
        // Where `a:close` returns the pen. No preset in the table draws an arc after
        // a close today, but several callouts do draw a LINE after one, so the pen
        // genuinely survives a close and tracking it here is what keeps that true if
        // the data ever grows an arc there.
        let mut subpath_start: Option<(f64, f64)> = None;
        for command in &path.commands {
            match *command {
                PresetCommand::Move([x, y]) => {
                    let point = pair(x, y)?;
                    pen = Some(point);
                    subpath_start = Some(point);
                    out.push(PathCommand::MoveTo {
                        point: place(point.0, point.1),
                    });
                }
                PresetCommand::Line([x, y]) => {
                    let point = pair(x, y)?;
                    pen = Some(point);
                    out.push(PathCommand::LineTo {
                        point: place(point.0, point.1),
                    });
                }
                PresetCommand::Cubic([x1, y1, x2, y2, x3, y3]) => {
                    let (control1, control2, point) = (pair(x1, y1)?, pair(x2, y2)?, pair(x3, y3)?);
                    pen = Some(point);
                    out.push(PathCommand::CubicTo {
                        control1: place(control1.0, control1.1),
                        control2: place(control2.0, control2.1),
                        point: place(point.0, point.1),
                    });
                }
                PresetCommand::Quad([x1, y1, x2, y2]) => {
                    let (control, point) = (pair(x1, y1)?, pair(x2, y2)?);
                    pen = Some(point);
                    out.push(PathCommand::QuadTo {
                        control: place(control.0, control.1),
                        point: place(point.0, point.1),
                    });
                }
                PresetCommand::Arc([w_r, h_r, st_ang, sw_ang]) => {
                    // An arc before any `moveTo` has no pen to lock its start angle
                    // to, so there is no centre to derive and no shape to draw. The
                    // generator never emits one; refusing says so rather than
                    // inventing an origin.
                    let start = pen?;
                    let arc = [
                        resolved.token(w_r)?,
                        resolved.token(h_r)?,
                        resolved.token(st_ang)?,
                        resolved.token(sw_ang)?,
                    ];
                    pen = Some(push_arc(&mut out, start, arc, &place)?);
                }
                PresetCommand::Close => pen = subpath_start,
                PresetCommand::Unreadable => return None,
            }
        }
    }
    (!out.is_empty()).then_some(out)
}

/// Whether a preset's outline closes — any `a:close` in its paths.
#[must_use]
pub fn preset_is_closed(token: &str) -> bool {
    table().get(token).is_some_and(|definition| {
        definition
            .paths
            .iter()
            .flat_map(|path| path.commands.iter())
            .any(|command| matches!(command, PresetCommand::Close))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1000 × 500 twip box at the origin, so x and y scale differently and a shape
    /// that swapped them would show it.
    fn rect() -> Rect {
        Rect::new(
            Point::new(Twip(0), Twip(0)),
            crate::units::Size::new(Twip(1_000), Twip(500)),
        )
    }

    /// The on-curve endpoint of a command, whatever its kind.
    fn endpoint(command: &PathCommand) -> Point {
        match *command {
            PathCommand::MoveTo { point }
            | PathCommand::LineTo { point }
            | PathCommand::CubicTo { point, .. }
            | PathCommand::QuadTo { point, .. } => point,
        }
    }

    /// How far a point sits off the ellipse inscribed in [`rect`] — centred at
    /// (500, 250) with radii 500 and 250 — as a fraction of the radius in that
    /// direction. Zero means exactly on it. A twip of rounding on the short axis is
    /// 1/250 = 0.004, so anything under 0.01 is the rounding and nothing else.
    fn off_box_ellipse(point: Point) -> f64 {
        let x = (f64::from(point.x.raw()) - 500.0) / 500.0;
        let y = (f64::from(point.y.raw()) - 250.0) / 250.0;
        x.hypot(y) - 1.0
    }

    /// The cubic the evaluator should emit, written as the three points it carries.
    fn cubic(control1: (i32, i32), control2: (i32, i32), point: (i32, i32)) -> PathCommand {
        let at = |(x, y): (i32, i32)| Point::new(Twip(x), Twip(y));
        PathCommand::CubicTo {
            control1: at(control1),
            control2: at(control2),
            point: at(point),
        }
    }

    #[test]
    fn the_table_carries_every_preset_the_specification_defines() {
        // Derived from the committed table, not typed here: 187 is the count ECMA-376
        // Part 1 §20.1.9 defines, and the generator counted 186 until a name-based
        // filter that was swallowing the preset literally called `rect` came out.
        assert_eq!(preset_count(), 187);
        assert!(is_known("rect"), "the preset named like a child element");
        assert!(is_known("roundRect"));
        assert!(is_known("bentConnector3"));
        assert!(is_known("accentBorderCallout1"));
        assert!(!is_known("notAShape"));
    }

    #[test]
    fn the_guides_per_preset_ceiling_is_what_the_table_actually_holds() {
        // `shape_guide` used to cap evaluation at 32 guides, which silently refused
        // nine arc-free presets. This pins the real shape of the data so the cap cannot
        // quietly come back in the wrong place: `gear9` is the widest at 244, and 28
        // presets exceed 32.
        let widest = table()
            .iter()
            .map(|(token, definition)| (definition.adjust.len() + definition.guides.len(), *token))
            .max()
            .expect("the table is not empty");
        assert_eq!(widest, (244, "gear9"));
        let over_32 = table()
            .values()
            .filter(|definition| definition.adjust.len() + definition.guides.len() > 32)
            .count();
        assert_eq!(over_32, 28, "presets a 32-guide cap would have broken");
    }

    #[test]
    fn a_rectangle_preset_resolves_to_its_four_corners() {
        // `rect` is the simplest recipe: move to l,t then three lines and a close. It
        // is the one preset whose expected geometry needs no arithmetic, which makes it
        // the right canary for the whole resolve path.
        let outline = preset_outline("rect", &[], rect()).expect("rect resolves");
        assert_eq!(outline.len(), 4, "a move and three lines: {outline:?}");
        assert_eq!(
            outline[0],
            PathCommand::MoveTo {
                point: Point::new(Twip(0), Twip(0))
            }
        );
        assert_eq!(
            outline[1],
            PathCommand::LineTo {
                point: Point::new(Twip(1_000), Twip(0))
            },
            "r,t is the box's top-right"
        );
        assert_eq!(
            outline[2],
            PathCommand::LineTo {
                point: Point::new(Twip(1_000), Twip(500))
            }
        );
        assert!(preset_is_closed("rect"));
    }

    #[test]
    fn an_ellipse_is_four_quarter_turn_cubics_around_the_box_centre() {
        // `ellipse` is `M l,vc` and then four `a:arcTo wd2 hd2` of a quarter turn
        // each. In the 1000 × 500 box that is the ellipse centred at (500, 250) with
        // radii 500 and 250, walked clockwise in DrawingML's y-down space from its
        // leftmost point: left → top → right → bottom → back to left.
        //
        // Every number here is the arc maths rather than a recording. For a quarter
        // turn the handle is k = 4/3 · tan(90°/4) = 0.5522847, applied to the tangent
        // dP/dt = (−wR·sin t, hR·cos t). Leaving the leftmost point (t = 180°) that
        // tangent is (0, −250), so control1 = (0, 250 − 0.5522847 · 250) = (0, 111.93)
        // → (0, 112); arriving at the top (t = 270°) it is (500, 0), so
        // control2 = (500 − 0.5522847 · 500, 0) = (223.86, 0) → (224, 0).
        //
        // Two bugs are pinned by these literals. The first control point sharing the
        // move-to's x = 0 can only happen if the centre was DERIVED from the pen and
        // the start angle; take the pen for the centre and the first arc runs
        // (0, 250) → (0, 0) with a control at x = −500, outside the box. And the
        // endpoints alternating between the 500-radius and the 250-radius axes can
        // only happen if the two radii are carried separately; make the arc circular
        // and the top lands at (500, −250).
        let outline = preset_outline("ellipse", &[], rect()).expect("ellipse resolves");
        assert_eq!(
            outline,
            vec![
                PathCommand::MoveTo {
                    point: Point::new(Twip(0), Twip(250))
                },
                cubic((0, 112), (224, 0), (500, 0)),
                cubic((776, 0), (1_000, 112), (1_000, 250)),
                cubic((1_000, 388), (776, 500), (500, 500)),
                cubic((224, 500), (0, 388), (0, 250)),
            ],
            "four quarter turns, clockwise from the left"
        );
        // The outline comes back to where it started, which is what a detached arc
        // cannot do, and `a:close` still reaches the caller.
        assert_eq!(endpoint(&outline[4]), endpoint(&outline[0]));
        assert!(preset_is_closed("ellipse"));
    }

    #[test]
    fn a_sweep_wider_than_a_quarter_turn_is_split_rather_than_approximated() {
        // `pie`'s defaults are stAng 0 and enAng 270°, so its single `a:arcTo` sweeps
        // 270° — three quarter turns. A cubic Bézier cannot describe one: it cannot
        // leave the right of the ellipse, curl past the bottom and the left, and
        // arrive at the top. So the sweep is split at 90° and the three segments'
        // endpoints are exactly the axis points the parameter passes through: bottom
        // (500, 500), left (0, 250), top (500, 0). Approximate the whole sweep with
        // one cubic and this outline is three commands instead of five, and a bulge
        // instead of a wedge.
        let outline = preset_outline("pie", &[], rect()).expect("pie resolves");
        assert_eq!(
            outline,
            vec![
                PathCommand::MoveTo {
                    point: Point::new(Twip(1_000), Twip(250))
                },
                cubic((1_000, 388), (776, 500), (500, 500)),
                cubic((224, 500), (0, 388), (0, 250)),
                cubic((0, 112), (224, 0), (500, 0)),
                // `L hc vc`: the wedge's radius back to the centre.
                PathCommand::LineTo {
                    point: Point::new(Twip(500), Twip(250))
                },
            ],
            "three cubics for 270°, then the radius"
        );
        for command in &outline[1..4] {
            let off = off_box_ellipse(endpoint(command));
            assert!(off.abs() < 0.01, "{command:?} is {off} off the ellipse");
        }
    }

    #[test]
    fn a_negative_swing_angle_sweeps_the_other_way() {
        // `donut` is two full ellipses: the outer one swept clockwise in four `+cd4`
        // arcs, the inner one ANTICLOCKWISE in four `-5400000` arcs, which is how the
        // hole is cut. A sign error still produces a plausible closed ring, so the
        // direction has to be asserted on a point only one direction reaches.
        //
        // With the default `adj` of 25000 and ss = 500, dr = 125, so the inner
        // ellipse has radii 375 and 125 about (500, 250) and starts at (125, 250) —
        // its leftmost point, t = 180°. Sweeping −90° arrives at t = 90°, which in
        // y-down space is the BOTTOM, (500, 375). Sweeping +90° would arrive at
        // t = 270°, the top, (500, 125). Those are the two answers, 250 twips apart.
        let outline = preset_outline("donut", &[], rect()).expect("donut resolves");
        assert_eq!(
            outline.len(),
            10,
            "two move-tos and eight cubics: {outline:?}"
        );
        assert_eq!(
            outline[5],
            PathCommand::MoveTo {
                point: Point::new(Twip(125), Twip(250))
            },
            "the inner ring starts at its own leftmost point"
        );
        assert_eq!(
            outline[6..],
            [
                cubic((125, 319), (293, 375), (500, 375)),
                cubic((707, 375), (875, 319), (875, 250)),
                cubic((875, 181), (707, 125), (500, 125)),
                cubic((293, 125), (125, 181), (125, 250)),
            ],
            "the inner ring runs left → bottom → right → top, the reverse of the outer"
        );
        // The outer ring is the same ellipse as `ellipse`, so the two rings turn
        // opposite ways: outer left → top, inner left → bottom.
        assert_eq!(endpoint(&outline[1]), Point::new(Twip(500), Twip(0)));
    }

    #[test]
    fn a_quarter_turn_stays_one_segment_through_the_rounding() {
        // The per-segment split count is a ratio that floating point cannot be relied
        // on to land exactly on a whole number, and the error goes the expensive way.
        // In a PORTRAIT 500 × 1000 box, `donut`'s inner ring has radii 125 and 375,
        // and two of its four quarter turns come out at span/90° =
        // 1.0000000000000002 — just over one segment. Without the tolerance in the
        // count those two each split in two, so the ring is six cubics rather than
        // four: the painted shape is identical, which is why only the command count
        // can notice, and why the landscape box the other guards use does not.
        let portrait = Rect::new(
            Point::new(Twip(0), Twip(0)),
            crate::units::Size::new(Twip(500), Twip(1_000)),
        );
        let outline = preset_outline("donut", &[], portrait).expect("donut resolves");
        assert_eq!(
            outline.len(),
            10,
            "two move-tos and two rings of four quarter turns: {outline:?}"
        );
    }

    #[test]
    fn an_arc_angle_is_the_geometric_one_not_the_ellipse_parameter() {
        // The subtlety that only shows on a non-right angle. `a:arcTo`'s `stAng` is
        // the GEOMETRIC angle from the centre, not the parameter t of
        // (wR·cos t, hR·sin t); on this 2:1 ellipse the two differ by up to 18.4°.
        // The preset data settles which it is: `pie` computes the point it moves to
        // just before the arc with the `cat2`/`sat2` pair, which is that conversion
        // written in the formula language.
        //
        // Authoring stAng = 45° (2 700 000) gives
        // t = atan2(500·sin 45°, 250·cos 45°) = atan 2 = 63.4349°, so the move-to is
        // (500 + 500·cos t, 250 + 250·sin t) = (723.61, 473.61) → (724, 474). The
        // sweep runs on to the unchanged enAng of 270°, whose own parameter is
        // exactly 270°, so the last arc point is the top of the box ellipse, (500, 0),
        // and the wedge closes on the centre.
        //
        // Read `stAng` as the parameter instead and the centre comes out at
        // (370.05, 296.83) rather than (500, 250): the arc is then an ellipse that is
        // not the shape's own, its last point is (370, 47), and the `L hc vc` radius
        // no longer touches it.
        let outline = preset_outline(
            "pie",
            &[ShapeAdjustment {
                name: "adj1".to_owned(),
                formula: "val 2700000".to_owned(),
            }],
            rect(),
        )
        .expect("pie resolves with an authored start angle");
        assert_eq!(
            outline[0],
            PathCommand::MoveTo {
                point: Point::new(Twip(724), Twip(474))
            },
            "the geometric 45° point, not the 45° parameter point"
        );
        assert_eq!(outline.len(), 5, "206.57° is three cubics: {outline:?}");
        assert_eq!(
            endpoint(&outline[3]),
            Point::new(Twip(500), Twip(0)),
            "the sweep ends on the top of the shape's OWN ellipse"
        );
        // Every on-curve point of the arc is on that ellipse, which only holds when
        // the centre was derived through the same conversion.
        for command in &outline[0..4] {
            let off = off_box_ellipse(endpoint(command));
            assert!(off.abs() < 0.01, "{command:?} is {off} off the ellipse");
        }
        assert_eq!(
            endpoint(&outline[4]),
            Point::new(Twip(500), Twip(250)),
            "and the radius still lands on the centre the arc was built around"
        );
    }

    #[test]
    fn an_arc_is_scaled_by_the_paths_own_coordinate_space() {
        // `cloud` is the only family carrying both an `a:path@w`/`@h` and arcs: 43200
        // × 43200 path units, with the arc radii in those units too. The radii must
        // therefore be scaled by the same per-axis factors as the coordinates — and
        // the two factors differ here, 1000/43200 against 500/43200. Leave the radii
        // in path space and the first arc alone is tens of times the box.
        let outline = preset_outline("cloud", &[], rect()).expect("cloud resolves");
        let arcs = outline
            .iter()
            .filter(|command| matches!(command, PathCommand::CubicTo { .. }))
            .count();
        assert!(arcs >= 11, "cloud is drawn from arcs: {arcs}");
        let (mut left, mut top) = (i32::MAX, i32::MAX);
        let (mut right, mut bottom) = (i32::MIN, i32::MIN);
        for command in &outline {
            let point = endpoint(command);
            left = left.min(point.x.raw());
            right = right.max(point.x.raw());
            top = top.min(point.y.raw());
            bottom = bottom.max(point.y.raw());
        }
        // The on-curve points fill the box on BOTH axes and overshoot it by at most a
        // twip, which is the preset's own data rather than the arc maths. Scale the
        // height radii by the width factor instead of their own — the "the arc is
        // circular" mistake — and the vertical extent comes out twice the box; leave
        // the radii in path space and it is tens of times the box.
        assert!(
            (-4..=1_004).contains(&left) && (-4..=1_004).contains(&right),
            "the horizontal extent is {left}..{right}, not ~0..1000"
        );
        assert!(
            (-4..=504).contains(&top) && (-4..=504).contains(&bottom),
            "the vertical extent is {top}..{bottom}, not ~0..500"
        );
        assert!(
            right - left > 950,
            "the box is filled across: {left}..{right}"
        );
        assert!(bottom - top > 475, "and down it: {top}..{bottom}");
    }

    #[test]
    fn an_unknown_token_resolves_to_nothing() {
        assert_eq!(preset_outline("notAShape", &[], rect()), None);
    }

    #[test]
    fn an_authored_adjust_value_overrides_the_preset_default() {
        // `roundRect`'s radius comes from `adj`, default 16667. Authoring a different
        // value must move the geometry; if the override were ignored the two outlines
        // would be identical, which is exactly the silent failure this catches. It is
        // drawn with an arc per corner, so it only became testable here once `arcTo`
        // resolved — this guard used to assert both were refused.
        let default = preset_outline("roundRect", &[], rect()).expect("roundRect resolves");
        let authored = preset_outline(
            "roundRect",
            &[ShapeAdjustment {
                name: "adj".to_owned(),
                formula: "val 50000".to_owned(),
            }],
            rect(),
        )
        .expect("still resolves with an authored adjust");
        assert_eq!(
            default.len(),
            authored.len(),
            "the same recipe, so the same commands"
        );
        assert_ne!(
            default, authored,
            "a bigger corner radius must move the corners"
        );

        // `parallelogram` is adjustable and arc-free, so it also shows that the 124
        // which resolved before arcs still honour an override.
        let default = preset_outline("parallelogram", &[], rect()).expect("parallelogram resolves");
        let authored = preset_outline(
            "parallelogram",
            &[ShapeAdjustment {
                name: "adj".to_owned(),
                formula: "val 10000".to_owned(),
            }],
            rect(),
        )
        .expect("still resolves with an authored adjust");
        assert_ne!(
            default, authored,
            "an authored adj must change the outline, not be ignored"
        );
    }

    #[test]
    fn an_arc_free_preset_still_emits_exactly_one_command_per_authored_row() {
        // The 124 presets that already resolved must not have moved. Checked over the
        // whole arc-free subset rather than spot-checked, and derived from the table:
        // a preset with no `R` row must emit exactly one command per non-`close` row,
        // which is what the evaluator did before the pen-tracking rewrite and is the
        // invariant that rewrite could most easily have broken.
        let mut checked = 0_usize;
        for (token, definition) in table() {
            let rows: Vec<PresetCommand<'_>> = definition
                .paths
                .iter()
                .flat_map(|path| path.commands.iter().copied())
                .collect();
            if rows
                .iter()
                .any(|command| matches!(command, PresetCommand::Arc(_)))
            {
                continue;
            }
            let expected = rows
                .iter()
                .filter(|command| !matches!(command, PresetCommand::Close))
                .count();
            let outline = preset_outline(token, &[], rect()).expect("an arc-free preset resolves");
            assert_eq!(outline.len(), expected, "{token} changed shape");
            checked += 1;
        }
        assert_eq!(
            checked, 124,
            "the arc-free presets, counted from the table rather than typed here"
        );
    }

    #[test]
    fn every_preset_in_the_table_resolves_and_the_count_is_the_measured_one() {
        // The number that scopes this work, and the reason it is derived by walking
        // the table rather than written down: it read 124 resolved / 63 refused while
        // `a:arcTo` was a parameterless marker, and `arcTo` landing is what moves it
        // to 187/0. Nothing here may be hand-maintained — a hand-written count is how
        // a false claim survives in this repository.
        let mut ok = 0_usize;
        let mut refused: Vec<&str> = Vec::new();
        for token in table().keys() {
            if preset_outline(token, &[], rect()).is_some() {
                ok += 1;
            } else {
                refused.push(token);
            }
        }
        assert_eq!(refused, Vec::<&str>::new(), "every preset must draw");
        assert_eq!(ok, preset_count());
        assert_eq!(ok, 187);
    }

    #[test]
    fn a_zero_sweep_and_a_zero_radius_draw_nothing_rather_than_a_degenerate_segment() {
        // Both are legal and both cover no area. A zero-length cubic would be a
        // command every backend has to step over for no paint, and the centre and
        // handle arithmetic would be dividing by the thing that is zero. The pen must
        // stay where the previous command left it, so what follows is unaffected.
        let place = |x: f64, y: f64| Point::new(twip_rounded(x), twip_rounded(y));
        let mut out = Vec::new();
        assert_eq!(
            push_arc(&mut out, (7.0, 9.0), [500.0, 250.0, 0.0, 0.0], &place),
            Some((7.0, 9.0)),
            "a zero sweep leaves the pen alone"
        );
        assert_eq!(
            push_arc(&mut out, (7.0, 9.0), [0.0, 250.0, 0.0, 5_400_000.0], &place),
            Some((7.0, 9.0)),
            "a zero width radius has no ellipse to walk"
        );
        assert_eq!(
            push_arc(&mut out, (7.0, 9.0), [500.0, 0.0, 0.0, 5_400_000.0], &place),
            Some((7.0, 9.0)),
            "nor a zero height radius"
        );
        assert!(
            out.is_empty(),
            "and none of them emitted a segment: {out:?}"
        );
        // A full turn is NOT degenerate, even though it starts and ends on the same
        // point: `atan2` alone would fold it to a zero span and draw nothing.
        assert!(
            push_arc(
                &mut out,
                (7.0, 9.0),
                [500.0, 250.0, 0.0, 21_600_000.0],
                &place
            )
            .is_some()
        );
        assert_eq!(out.len(), 4, "a full turn is four quarter turns: {out:?}");
    }

    #[test]
    fn an_absurd_sweep_is_refused_rather_than_filling_the_display_list() {
        // The angles come from guides, so an authored adjustment can reach them. A
        // sweep of a thousand turns traces the same ellipse a thousand times; drawing
        // it would put four thousand cubics in the display list for a shape
        // indistinguishable from one turn, so the preset is refused and the caller
        // keeps its reported bounding rectangle.
        let place = |x: f64, y: f64| Point::new(twip_rounded(x), twip_rounded(y));
        let mut out = Vec::new();
        assert_eq!(
            push_arc(
                &mut out,
                (0.0, 0.0),
                [500.0, 250.0, 0.0, 21_600_000.0 * 1_000.0],
                &place
            ),
            None
        );
        assert!(out.is_empty());
    }
}
