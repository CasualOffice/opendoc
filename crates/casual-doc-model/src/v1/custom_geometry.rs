// SPDX-License-Identifier: Apache-2.0

//! DrawingML shape geometry as DATA: the `a:custGeom` a document authors, and —
//! through [`crate::v1::preset_shape`] — the 187 ECMA-376 preset geometries, which
//! are the same structure read from the standard's own table.
//!
//! # One structure for both, on purpose
//!
//! ECMA-376 defines every preset shape (`a:prstGeom@prst="star5"`) as an
//! `a:custGeom` written in advance: adjust defaults (`avLst`), guide formulas
//! (`gdLst`), adjust handles (`ahLst`), a text rectangle (`rect`) and a path list
//! (`pathLst`). So a preset is not a kind of shape and a freeform another; a preset
//! is a recipe the standard supplies and a freeform is one the document supplies.
//! Modelling them as one type is what lets ONE engine
//! ([`crate::v1::GeometryProgram`]) draw both, and is the structure `docs/119` §3
//! measured in the market leader and §6 adopted.
//!
//! # Coordinates are values or guide NAMES
//!
//! Every coordinate, radius and angle in the grammar is an `ST_AdjCoordinate` /
//! `ST_AdjAngle`: either a literal or the name of a guide. [`GeometryValue`] holds
//! either, so a geometry round-trips exactly as authored — evaluating names at
//! import would bake the box size the file happened to be opened at into a shape
//! that is meant to scale.
//!
//! # Bounds
//!
//! Every list is bounded so a hostile package cannot turn one shape into unbounded
//! work. The guide bound is set by the standard's own largest preset (`star32`,
//! 242 guides) because Word writes a preset's FULL definition into `a:custGeom`
//! when an author uses Edit Points on it, and refusing such a shape would refuse a
//! routine Word document.

use serde::{Deserialize, Deserializer, Serialize};

use super::{MAX_EMU, MAX_SHAPE_GUIDE_NAME_BYTES, PointEmu, ShapeAdjustment};

/// Maximum path commands retained for one custom shape geometry, summed over all
/// of its paths (`a:custGeom/a:pathLst/a:path/*`). A bound, not a fidelity target:
/// the largest preset has 70, a hand-drawn freeform is tens of points, and the cap
/// stops a hostile package from turning one shape into an unbounded vertex list
/// (`docs/119` §6).
pub const MAX_SHAPE_PATH_COMMANDS: usize = 1024;

/// Maximum `a:path` elements in one geometry. The largest preset has 6.
pub const MAX_SHAPE_PATHS: usize = 64;

/// Maximum `a:gdLst` guides in one geometry. The largest preset (`star32`) has
/// 242, and Word writes a preset's whole guide list when it converts it to a
/// freeform, so the bound must admit it.
pub const MAX_SHAPE_GUIDES: usize = 256;

/// Maximum adjust handles (`a:ahLst`) in one geometry. The largest preset has 5.
pub const MAX_SHAPE_HANDLES: usize = 64;

/// Maximum connection sites (`a:cxnLst`) in one geometry. The largest preset has
/// 16.
pub const MAX_SHAPE_CONNECTIONS: usize = 64;

/// One coordinate, radius or angle of a DrawingML geometry (`ST_AdjCoordinate` /
/// `ST_AdjAngle`): a literal, or the name of a guide the engine resolves.
///
/// Serialized untagged — a JSON number or a JSON string — so a literal-only path
/// keeps the shape a snapshot written before guide names existed already had.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum GeometryValue {
    /// A literal: EMU for a coordinate or radius, 60000ths of a degree for an
    /// angle.
    Literal(i64),
    /// The name of a guide (`a:gd@name`), a built-in variable (`w`, `hd2`,
    /// `cd4`, …), or an adjust value.
    Guide(String),
}

impl GeometryValue {
    /// Reads one attribute token: an integer is a literal, anything else non-empty
    /// and within [`MAX_SHAPE_GUIDE_NAME_BYTES`] is a guide name.
    ///
    /// `None` for an empty or over-long token — a coordinate the model could not
    /// round-trip, which the importer refuses rather than truncates.
    ///
    /// Complexity: O(token length).
    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        if let Ok(value) = token.parse::<i64>() {
            return Some(Self::Literal(value));
        }
        (!token.is_empty()
            && token.len() <= MAX_SHAPE_GUIDE_NAME_BYTES
            && !token.chars().any(char::is_whitespace))
        .then(|| Self::Guide(token.to_owned()))
    }

    /// The literal value, or `None` for a guide name.
    #[must_use]
    pub fn literal(&self) -> Option<i64> {
        match self {
            Self::Literal(value) => Some(*value),
            Self::Guide(_) => None,
        }
    }

    /// The attribute token this value is written back as.
    #[must_use]
    pub fn token(&self) -> String {
        match self {
            Self::Literal(value) => value.to_string(),
            Self::Guide(name) => name.clone(),
        }
    }

    /// Whether the value is inside the model's bounds: a literal within
    /// `±MAX_EMU`, or a non-empty guide name within
    /// [`MAX_SHAPE_GUIDE_NAME_BYTES`].
    fn in_bounds(&self) -> bool {
        match self {
            Self::Literal(value) => (-MAX_EMU..=MAX_EMU).contains(value),
            Self::Guide(name) => !name.is_empty() && name.len() <= MAX_SHAPE_GUIDE_NAME_BYTES,
        }
    }
}

impl From<i64> for GeometryValue {
    fn from(value: i64) -> Self {
        Self::Literal(value)
    }
}

/// A point of a geometry (`a:pt`, `a:pos`): two [`GeometryValue`]s.
///
/// Serialized as `x`/`y`; the `xEmu`/`yEmu` spelling of the literal-only form
/// that preceded guide names is still read.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GeometryPoint {
    /// The x coordinate.
    #[serde(alias = "xEmu")]
    pub x: GeometryValue,
    /// The y coordinate.
    #[serde(alias = "yEmu")]
    pub y: GeometryValue,
}

impl GeometryPoint {
    /// A literal point.
    #[must_use]
    pub fn literal(x: i64, y: i64) -> Self {
        Self {
            x: GeometryValue::Literal(x),
            y: GeometryValue::Literal(y),
        }
    }

    /// The point as literal EMU, or `None` if either coordinate is a guide name.
    #[must_use]
    pub fn as_literal(&self) -> Option<PointEmu> {
        Some(PointEmu {
            x_emu: self.x.literal()?,
            y_emu: self.y.literal()?,
        })
    }
}

impl From<PointEmu> for GeometryPoint {
    fn from(point: PointEmu) -> Self {
        Self::literal(point.x_emu, point.y_emu)
    }
}

/// How one path of a geometry is filled (`a:path@fill`, `ST_PathFillMode`).
///
/// The four shading modes are how the standard draws the visible faces of a
/// 3-D-looking preset (the top of `can`, the sides of `cube`, the fold of
/// `foldedCorner`) in the shape's own colour, darker or lighter.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PathFill {
    /// The shape's fill (`norm`, the default).
    #[default]
    Norm,
    /// Not filled (`none`): an outline-only path such as a callout leader.
    None,
    /// The shape's fill, lightened (`lighten`).
    Lighten,
    /// The shape's fill, lightened less (`lightenLess`).
    LightenLess,
    /// The shape's fill, darkened (`darken`).
    Darken,
    /// The shape's fill, darkened less (`darkenLess`).
    DarkenLess,
}

impl PathFill {
    /// The `ST_PathFillMode` token.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Norm => "norm",
            Self::None => "none",
            Self::Lighten => "lighten",
            Self::LightenLess => "lightenLess",
            Self::Darken => "darken",
            Self::DarkenLess => "darkenLess",
        }
    }

    /// The mode a `ST_PathFillMode` token names, or `None` for an unknown one.
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        Some(match token {
            "norm" => Self::Norm,
            "none" => Self::None,
            "lighten" => Self::Lighten,
            "lightenLess" => Self::LightenLess,
            "darken" => Self::Darken,
            "darkenLess" => Self::DarkenLess,
            _ => return None,
        })
    }

    fn is_norm(&self) -> bool {
        *self == Self::Norm
    }
}

/// One command of a geometry path (`a:pathLst/a:path/*`).
///
/// The whole ECMA-376 path grammar (§20.1.9): move, line, elliptical arc,
/// quadratic and cubic Bézier, close. Coordinates are [`GeometryValue`]s, so a
/// command can name guides; the engine resolves them.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ShapePathCommand {
    /// Start a subpath at a point (`a:moveTo/a:pt`).
    MoveTo {
        /// The point, in the path's own coordinate space.
        point: GeometryPoint,
    },
    /// Draw a straight segment to a point (`a:lnTo/a:pt`).
    LineTo {
        /// The point, in the path's own coordinate space.
        point: GeometryPoint,
    },
    /// Draw an elliptical arc from the current point (`a:arcTo`).
    ///
    /// The current point lies on the ellipse at `start_angle`; the arc sweeps
    /// `swing_angle` (positive = clockwise on the page, whose y axis points down).
    /// Both angles are 60000ths of a degree and are VISUAL angles — the direction
    /// of the point from the ellipse's centre — not the ellipse's parametric angle;
    /// the standard's own presets (`pie`, `chord`) compute their arc endpoints that
    /// way, so the engine converts.
    #[serde(rename_all = "camelCase")]
    ArcTo {
        /// The ellipse's horizontal radius (`@wR`).
        width_radius: GeometryValue,
        /// The ellipse's vertical radius (`@hR`).
        height_radius: GeometryValue,
        /// Where on the ellipse the current point lies (`@stAng`).
        start_angle: GeometryValue,
        /// How far the arc sweeps (`@swAng`).
        swing_angle: GeometryValue,
    },
    /// Draw a cubic Bézier (`a:cubicBezTo`): two control points, then the
    /// endpoint, in the authored order.
    CubicBezTo {
        /// The control point leaving the previous endpoint.
        control1: GeometryPoint,
        /// The control point entering `point`.
        control2: GeometryPoint,
        /// The curve's endpoint.
        point: GeometryPoint,
    },
    /// Draw a quadratic Bézier (`a:quadBezTo`): one control point, then the
    /// endpoint.
    ///
    /// Kept distinct from [`ShapePathCommand::CubicBezTo`] rather than promoted on
    /// import, so the authored command round-trips as itself.
    QuadBezTo {
        /// The single control point.
        control: GeometryPoint,
        /// The curve's endpoint.
        point: GeometryPoint,
    },
    /// Close the subpath back to its starting point (`a:close`).
    Close,
}

impl ShapePathCommand {
    /// Whether this command draws, as opposed to only moving the pen or closing.
    ///
    /// A method rather than a `matches!` at the call site because that call site
    /// once tested for `LineTo` specifically, which silently rejected every
    /// curve-only geometry the moment curves existed.
    #[must_use]
    pub fn is_segment(&self) -> bool {
        matches!(
            self,
            Self::LineTo { .. }
                | Self::ArcTo { .. }
                | Self::CubicBezTo { .. }
                | Self::QuadBezTo { .. }
        )
    }

    /// Every point the command names, control points included, in authored
    /// order. An arc names no point (its geometry is radii and angles), so it
    /// yields none.
    pub fn points(&self) -> impl Iterator<Item = &GeometryPoint> + '_ {
        let (a, b, c) = match self {
            Self::MoveTo { point } | Self::LineTo { point } => (Some(point), None, None),
            Self::CubicBezTo {
                control1,
                control2,
                point,
            } => (Some(control1), Some(control2), Some(point)),
            Self::QuadBezTo { control, point } => (Some(control), Some(point), None),
            Self::ArcTo { .. } | Self::Close => (None, None, None),
        };
        a.into_iter().chain(b).chain(c)
    }

    /// Every value the command names: point coordinates, and an arc's radii and
    /// angles. Validation goes through this so nothing a command carries escapes
    /// the bounds check.
    pub fn values(&self) -> impl Iterator<Item = &GeometryValue> + '_ {
        let arc = match self {
            Self::ArcTo {
                width_radius,
                height_radius,
                start_angle,
                swing_angle,
            } => vec![width_radius, height_radius, start_angle, swing_angle],
            _ => Vec::new(),
        };
        self.points()
            .flat_map(|point| [&point.x, &point.y])
            .chain(arc)
    }
}

fn default_true() -> bool {
    true
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde's skip_serializing_if signature
fn is_true(value: &bool) -> bool {
    *value
}

/// One path of a geometry (`a:pathLst/a:path`): an ordered command list in its
/// own coordinate space, plus how it is painted.
///
/// [`width_emu`](Self::width_emu) / [`height_emu`](Self::height_emu) are
/// `a:path@w` / `@h`. Per ECMA-376 Part 1 §20.1.9.15 they default to `0`, and the
/// default is meaningful: a **positive** value is the extent of the path's own
/// coordinate space, so a coordinate maps to the shape box by `x / width_emu`;
/// **zero** means the coordinates are in the shape's own space (EMU, or guide
/// values computed from its extent) and do NOT rescale. The two axes are
/// independent — the loan-agreement rules in `docs/119` set `@w` and omit `@h`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShapePath {
    /// `a:path@w`: the path coordinate space's width, or `0` for shape space.
    #[serde(default)]
    pub width_emu: i64,
    /// `a:path@h`: the path coordinate space's height, or `0` for shape space.
    #[serde(default)]
    pub height_emu: i64,
    /// `a:path@fill`: how this path is filled.
    #[serde(default, skip_serializing_if = "PathFill::is_norm")]
    pub fill: PathFill,
    /// `a:path@stroke`: whether this path is outlined (default `true`).
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub stroke: bool,
    /// `a:path@extrusionOk`: whether a 3-D extrusion may use this path (default
    /// `true`). Carried for round-trip only; nothing here extrudes.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub extrusion_ok: bool,
    /// The commands, in path order. Always starts with a
    /// [`ShapePathCommand::MoveTo`].
    pub commands: Vec<ShapePathCommand>,
}

impl ShapePath {
    /// A path with the ECMA-376 attribute defaults (`fill="norm"`, stroked,
    /// extrusion allowed) in shape space.
    #[must_use]
    pub fn new(commands: Vec<ShapePathCommand>) -> Self {
        Self {
            width_emu: 0,
            height_emu: 0,
            fill: PathFill::Norm,
            stroke: true,
            extrusion_ok: true,
            commands,
        }
    }
}

/// A geometry's text rectangle (`a:rect`): the box text is laid out in, as four
/// edges that may name guides.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GeometryRect {
    /// The left edge (`@l`).
    pub left: GeometryValue,
    /// The top edge (`@t`).
    pub top: GeometryValue,
    /// The right edge (`@r`).
    pub right: GeometryValue,
    /// The bottom edge (`@b`).
    pub bottom: GeometryValue,
}

impl GeometryRect {
    /// `l t r b` — the whole shape box, which is what Word writes on every
    /// freeform and what an absent `a:rect` means.
    #[must_use]
    pub fn is_whole_box(&self) -> bool {
        let named = |value: &GeometryValue, name: &str| matches!(value, GeometryValue::Guide(guide) if guide == name);
        named(&self.left, "l")
            && named(&self.top, "t")
            && named(&self.right, "r")
            && named(&self.bottom, "b")
    }
}

/// One adjust handle (`a:ahLst/a:ahXY` or `a:ahPolar`): the drag affordance that
/// edits a shape's adjust values. `ahLst` is the ADJUST-HANDLE list — not
/// arrowheads (`docs/119` §1).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AdjustHandle {
    /// A handle that moves in x and/or y (`a:ahXY`), each axis driving one
    /// adjust value within an optional range.
    #[serde(rename_all = "camelCase")]
    Xy {
        /// The adjust value the x position drives (`@gdRefX`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        guide_x: Option<String>,
        /// The lowest value it may take (`@minX`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min_x: Option<GeometryValue>,
        /// The highest value it may take (`@maxX`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_x: Option<GeometryValue>,
        /// The adjust value the y position drives (`@gdRefY`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        guide_y: Option<String>,
        /// The lowest value it may take (`@minY`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min_y: Option<GeometryValue>,
        /// The highest value it may take (`@maxY`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_y: Option<GeometryValue>,
        /// Where the handle is drawn (`a:pos`).
        position: GeometryPoint,
    },
    /// A handle that moves in radius and/or angle (`a:ahPolar`).
    #[serde(rename_all = "camelCase")]
    Polar {
        /// The adjust value the radius drives (`@gdRefR`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        guide_radius: Option<String>,
        /// The lowest value it may take (`@minR`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min_radius: Option<GeometryValue>,
        /// The highest value it may take (`@maxR`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_radius: Option<GeometryValue>,
        /// The adjust value the angle drives (`@gdRefAng`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        guide_angle: Option<String>,
        /// The lowest value it may take (`@minAng`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min_angle: Option<GeometryValue>,
        /// The highest value it may take (`@maxAng`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_angle: Option<GeometryValue>,
        /// Where the handle is drawn (`a:pos`).
        position: GeometryPoint,
    },
}

impl AdjustHandle {
    /// Where the handle is drawn.
    #[must_use]
    pub fn position(&self) -> &GeometryPoint {
        match self {
            Self::Xy { position, .. } | Self::Polar { position, .. } => position,
        }
    }

    /// The adjust values this handle drives, in `(x or radius, y or angle)`
    /// order.
    #[must_use]
    pub fn guides(&self) -> [Option<&str>; 2] {
        match self {
            Self::Xy {
                guide_x, guide_y, ..
            } => [guide_x.as_deref(), guide_y.as_deref()],
            Self::Polar {
                guide_radius,
                guide_angle,
                ..
            } => [guide_radius.as_deref(), guide_angle.as_deref()],
        }
    }

    /// The handle's range bounds, in `(min, max)` pairs per axis.
    #[must_use]
    pub fn bounds(&self) -> [(Option<&GeometryValue>, Option<&GeometryValue>); 2] {
        match self {
            Self::Xy {
                min_x,
                max_x,
                min_y,
                max_y,
                ..
            } => [
                (min_x.as_ref(), max_x.as_ref()),
                (min_y.as_ref(), max_y.as_ref()),
            ],
            Self::Polar {
                min_radius,
                max_radius,
                min_angle,
                max_angle,
                ..
            } => [
                (min_radius.as_ref(), max_radius.as_ref()),
                (min_angle.as_ref(), max_angle.as_ref()),
            ],
        }
    }
}

/// One connection site (`a:cxnLst/a:cxn`): where a connector may attach, and the
/// direction it leaves in. Carried for round-trip; nothing routes connectors yet.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionSite {
    /// The direction a connector leaves the site (`@ang`).
    pub angle: GeometryValue,
    /// The site (`a:pos`).
    pub position: GeometryPoint,
}

/// A complete DrawingML geometry: `a:custGeom` as authored, or a preset's
/// definition read from the ECMA-376 table.
///
/// The adjust values (`a:avLst`) are deliberately NOT here: a shape keeps them
/// beside its geometry (`GroupShape::adjustments`) whether the geometry is a
/// preset or custom, because for a preset they are the shape's own overrides of
/// the definition's defaults and for a custom geometry they are its defaults —
/// one list either way.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CustomGeometry {
    /// The guide formulas (`a:gdLst/a:gd`), in definition order — each sees the
    /// ones before it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guides: Vec<ShapeAdjustment>,
    /// The adjust handles (`a:ahLst`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub handles: Vec<AdjustHandle>,
    /// The connection sites (`a:cxnLst`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connections: Vec<ConnectionSite>,
    /// The text rectangle (`a:rect`), when the geometry declares one. Kept as
    /// authored, including the whole-box `l t r b` Word writes, so a save writes
    /// back exactly what was read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_rect: Option<GeometryRect>,
    /// The paths (`a:pathLst/a:path`), painted in order.
    pub paths: Vec<ShapePath>,
}

/// The current snapshot form of [`CustomGeometry`].
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CustomGeometryCurrent {
    #[serde(default)]
    guides: Vec<ShapeAdjustment>,
    #[serde(default)]
    handles: Vec<AdjustHandle>,
    #[serde(default)]
    connections: Vec<ConnectionSite>,
    #[serde(default)]
    text_rect: Option<GeometryRect>,
    paths: Vec<ShapePath>,
}

/// Either snapshot form: the current one, or the single-path object a snapshot
/// written before multiple paths and guides existed holds in the same field.
#[derive(Deserialize)]
#[serde(untagged)]
enum CustomGeometryWire {
    Current(CustomGeometryCurrent),
    SinglePath(ShapePath),
}

impl<'de> Deserialize<'de> for CustomGeometry {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match CustomGeometryWire::deserialize(deserializer)? {
            CustomGeometryWire::Current(current) => Self {
                guides: current.guides,
                handles: current.handles,
                connections: current.connections,
                text_rect: current.text_rect,
                paths: current.paths,
            },
            CustomGeometryWire::SinglePath(path) => Self {
                paths: vec![path],
                ..Self::default()
            },
        })
    }
}

impl CustomGeometry {
    /// A geometry of one path and nothing else — what a hand-drawn freeform is.
    #[must_use]
    pub fn single_path(path: ShapePath) -> Self {
        Self {
            paths: vec![path],
            ..Self::default()
        }
    }

    /// Checks every bound the model places on a geometry, returning the name of
    /// the first property out of its domain.
    ///
    /// The structural bounds (counts, lengths, literal ranges, each path starting
    /// with a move) and then the semantic one: the geometry must COMPILE with the
    /// shape's `adjustments` — every name it uses must resolve and every formula
    /// must be in the language — because a geometry that cannot be evaluated
    /// would paint a rectangle in its place while claiming to be a freeform.
    ///
    /// Complexity: O(g + c) in the geometry's own guides and commands, both
    /// bounded, so O(1) in document size.
    ///
    /// # Errors
    ///
    /// The `ModelError` property name of the first violated bound.
    pub fn check(&self, adjustments: &[ShapeAdjustment]) -> Result<(), &'static str> {
        let bounded = |ok: bool, property: &'static str| if ok { Ok(()) } else { Err(property) };
        bounded(
            !self.paths.is_empty() && self.paths.len() <= MAX_SHAPE_PATHS,
            "group.shape.path.paths",
        )?;
        bounded(
            self.guides.len() <= MAX_SHAPE_GUIDES,
            "group.shape.path.guides",
        )?;
        bounded(
            self.handles.len() <= MAX_SHAPE_HANDLES,
            "group.shape.path.handles",
        )?;
        bounded(
            self.connections.len() <= MAX_SHAPE_CONNECTIONS,
            "group.shape.path.connections",
        )?;
        let commands: usize = self.paths.iter().map(|path| path.commands.len()).sum();
        bounded(
            commands <= MAX_SHAPE_PATH_COMMANDS,
            "group.shape.path.commands",
        )?;
        for guide in &self.guides {
            bounded(
                !guide.name.is_empty()
                    && guide.name.len() <= MAX_SHAPE_GUIDE_NAME_BYTES
                    && !guide.formula.is_empty()
                    && guide.formula.len() <= super::MAX_SHAPE_FORMULA_BYTES,
                "group.shape.path.guide",
            )?;
        }
        for path in &self.paths {
            bounded(
                matches!(path.commands.first(), Some(ShapePathCommand::MoveTo { .. })),
                "group.shape.path.commands.first",
            )?;
            bounded(
                (0..=MAX_EMU).contains(&path.width_emu) && (0..=MAX_EMU).contains(&path.height_emu),
                "group.shape.path.extent",
            )?;
            for value in path.commands.iter().flat_map(ShapePathCommand::values) {
                bounded(value.in_bounds(), "group.shape.path.point")?;
            }
        }
        let mut values: Vec<&GeometryValue> = Vec::new();
        for handle in &self.handles {
            values.extend([&handle.position().x, &handle.position().y]);
            for (min, max) in handle.bounds() {
                values.extend(min.into_iter().chain(max));
            }
            for guide in handle.guides().into_iter().flatten() {
                bounded(
                    !guide.is_empty() && guide.len() <= MAX_SHAPE_GUIDE_NAME_BYTES,
                    "group.shape.path.handle",
                )?;
            }
        }
        for site in &self.connections {
            values.extend([&site.angle, &site.position.x, &site.position.y]);
        }
        if let Some(rect) = &self.text_rect {
            values.extend([&rect.left, &rect.top, &rect.right, &rect.bottom]);
        }
        for value in values {
            bounded(value.in_bounds(), "group.shape.path.point")?;
        }
        bounded(
            super::GeometryProgram::compile(adjustments, self).is_ok(),
            "group.shape.path.formula",
        )
    }
}
