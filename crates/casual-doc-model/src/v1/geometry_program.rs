// SPDX-License-Identifier: Apache-2.0

//! The DrawingML geometry engine: the guide formula language (`a:gd@fmla`,
//! ECMA-376 Part 1 §20.1.9.11), compiled once and evaluated against a box, plus
//! the path grammar (§20.1.9.15–20) resolved to absolute points with every
//! elliptical arc converted to cubic Béziers.
//!
//! # The named prior art
//!
//! Three textbook pieces, and nothing invented:
//!
//! 1. **An interpreter over a data table.** The 187 preset geometries are data in
//!    the standard (`docs/156` §4.6); this engine evaluates them the way it
//!    evaluates an authored `a:custGeom`, so a preset is a recipe and not a code
//!    path (`docs/119` §6).
//! 2. **Compile once, evaluate many** — symbol resolution at load time. Formulas
//!    are parsed and every name is bound to a slot index when a geometry is
//!    COMPILED; evaluating against a box then does arithmetic over an array, with
//!    no string work and no name lookup. A preset compiles once per process, so a
//!    page of forty stars parses `star5` once.
//! 3. **Elliptical arc to cubic Bézier, one segment per quarter turn** — the same
//!    approximation SVG renderers, cairo and Skia use, with control handles at
//!    `k = (4/3)·tan(Δ/4)` along the tangents. `casual-doc-layout::arc` uses it
//!    for circles; an ellipse is that arc under a non-uniform scale, which is
//!    exactly how it is computed here.
//!
//! # Units
//!
//! Lengths are whatever unit the caller's box is in (the layout engine passes the
//! shape's extent in EMU, so an authored literal means what the file meant).
//! Angles are **1/60000 of a degree**, the DrawingML convention, for both the
//! angle operands of `sin`/`cos`/`tan` and the result of `at2`.
//!
//! # Degenerate values, and refusal
//!
//! A zero divisor yields zero and a negative root yields zero (see
//! `Op::apply` for the measurement behind that); only a value that is still not
//! finite — an overflow — makes the engine REFUSE the evaluation, and the caller
//! then paints the bounding rectangle it always has. An infinity would be a shape
//! nobody chose. The whole standard table is evaluated at ordinary, flat and
//! extreme-adjust boxes in the tests, so the policy is held by measurement.

use std::collections::HashMap;

use super::{AdjustHandle, CustomGeometry, PathFill, ShapeAdjustment, ShapePathCommand};

/// The largest number of quarter-turn segments one arc may expand to. An arc
/// sweeping more than a full turn retraces itself, so the sweep is clamped to one
/// turn and a hostile `@swAng` cannot make the output unbounded.
const MAX_ARC_SEGMENTS: usize = 4;

/// Degrees-per-unit for DrawingML's 1/60000-degree angles.
const ANGLE_UNITS_PER_DEGREE: f64 = 60_000.0;

fn to_radians(angle: f64) -> f64 {
    (angle / ANGLE_UNITS_PER_DEGREE).to_radians()
}

fn from_radians(radians: f64) -> f64 {
    radians.to_degrees() * ANGLE_UNITS_PER_DEGREE
}

/// A built-in variable (§20.1.9.11's table), resolved against the box at
/// evaluation time.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Builtin {
    Width,
    Height,
    Shortest,
    Longest,
    HalfWidth,
    HalfHeight,
    /// `wd<n>` — the width over `n`.
    WidthOver(u32),
    /// `hd<n>`.
    HeightOver(u32),
    /// `ssd<n>`.
    ShortestOver(u32),
}

impl Builtin {
    /// The built-in a name denotes, or `None`. The angle constants and the
    /// left/top edges are constants, so they are returned as one.
    fn parse(name: &str) -> Option<Operand> {
        Some(match name {
            "w" | "wd1" | "r" => Operand::Builtin(Self::Width),
            "h" | "hd1" | "b" => Operand::Builtin(Self::Height),
            "ss" | "ssd1" => Operand::Builtin(Self::Shortest),
            "ls" => Operand::Builtin(Self::Longest),
            "hc" => Operand::Builtin(Self::HalfWidth),
            "vc" => Operand::Builtin(Self::HalfHeight),
            "l" | "t" => Operand::Const(0.0),
            // Angle constants, in 1/60000 degree. A quarter turn is 5 400 000.
            "cd1" => Operand::Const(21_600_000.0),
            "cd2" => Operand::Const(10_800_000.0),
            "cd4" => Operand::Const(5_400_000.0),
            "cd8" => Operand::Const(2_700_000.0),
            "3cd4" => Operand::Const(16_200_000.0),
            "3cd8" => Operand::Const(8_100_000.0),
            "5cd8" => Operand::Const(13_500_000.0),
            "7cd8" => Operand::Const(18_900_000.0),
            _ => {
                // `wd<n>` / `hd<n>` / `ssd<n>`: parsed rather than tabulated, so an
                // unusual-but-legal divisor resolves instead of silently becoming
                // an unknown name.
                let (make, digits): (fn(u32) -> Self, &str) =
                    if let Some(rest) = name.strip_prefix("ssd") {
                        (Self::ShortestOver, rest)
                    } else if let Some(rest) = name.strip_prefix("wd") {
                        (Self::WidthOver, rest)
                    } else if let Some(rest) = name.strip_prefix("hd") {
                        (Self::HeightOver, rest)
                    } else {
                        return None;
                    };
                if !digits.bytes().all(|byte| byte.is_ascii_digit()) {
                    return None;
                }
                let divisor = digits.parse::<u32>().ok().filter(|n| *n > 0)?;
                Operand::Builtin(make(divisor))
            }
        })
    }

    fn value(self, width: f64, height: f64) -> f64 {
        let shortest = width.min(height);
        match self {
            Self::Width => width,
            Self::Height => height,
            Self::Shortest => shortest,
            Self::Longest => width.max(height),
            Self::HalfWidth => width / 2.0,
            Self::HalfHeight => height / 2.0,
            Self::WidthOver(n) => width / f64::from(n),
            Self::HeightOver(n) => height / f64::from(n),
            Self::ShortestOver(n) => shortest / f64::from(n),
        }
    }
}

/// One compiled operand: a constant, a built-in, or a slot (an adjust value or
/// guide defined earlier).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Operand {
    Const(f64),
    Builtin(Builtin),
    Slot(u16),
}

impl Operand {
    fn value(self, width: f64, height: f64, slots: &[f64]) -> f64 {
        match self {
            Self::Const(value) => value,
            Self::Builtin(builtin) => builtin.value(width, height),
            // A slot is only ever compiled to an index already defined, and the
            // slot vector is sized to the program, so this cannot be out of range;
            // `NAN` rather than a panic keeps the engine total regardless.
            Self::Slot(index) => slots.get(usize::from(index)).copied().unwrap_or(f64::NAN),
        }
    }
}

/// The seventeen opcodes of the formula language.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Op {
    Val,
    Abs,
    Sqrt,
    Max,
    Min,
    At2,
    Sin,
    Cos,
    Tan,
    MulDiv,
    AddSub,
    AddDiv,
    IfElse,
    Pin,
    Mod,
    Cat2,
    Sat2,
}

impl Op {
    fn parse(token: &str) -> Option<(Self, usize)> {
        Some(match token {
            "val" => (Self::Val, 1),
            "abs" => (Self::Abs, 1),
            "sqrt" => (Self::Sqrt, 1),
            "max" => (Self::Max, 2),
            "min" => (Self::Min, 2),
            "at2" => (Self::At2, 2),
            "sin" => (Self::Sin, 2),
            "cos" => (Self::Cos, 2),
            "tan" => (Self::Tan, 2),
            "*/" => (Self::MulDiv, 3),
            "+-" => (Self::AddSub, 3),
            "+/" => (Self::AddDiv, 3),
            "?:" => (Self::IfElse, 3),
            "pin" => (Self::Pin, 3),
            "mod" => (Self::Mod, 3),
            "cat2" => (Self::Cat2, 3),
            "sat2" => (Self::Sat2, 3),
            _ => return None,
        })
    }

    /// Applies the opcode. `None` only for a non-finite result.
    ///
    /// A ZERO DIVISOR yields `0`, and the root of a negative number yields `0`.
    /// Both are decisions, measured rather than assumed: refusing them instead
    /// sank the whole outline of 21 legal adjust settings in the standard table
    /// (`parallelogram` at `adj = 0` divides by its own zero offset in a guide that
    /// only positions the TEXT rectangle) and of 153 flat-box cases, all of which
    /// then painted as rectangles. At a zero divisor the guide is describing a
    /// degenerate part of a degenerate shape, and zero is the extent it has; a
    /// negative root is float round-off at a tangency (`circularArrow`'s
    /// `sqrt (1 - u7)`), whose true value is zero.
    fn apply(self, x: f64, y: f64, z: f64) -> Option<f64> {
        let value = match self {
            Self::Val => x,
            Self::Abs => x.abs(),
            Self::Sqrt => x.max(0.0).sqrt(),
            Self::Max => x.max(y),
            Self::Min => x.min(y),
            // `at2 x y` is arctan(y / x) — operand order is x then y, which is the
            // reverse of `f64::atan2`'s receiver/argument order.
            Self::At2 => from_radians(y.atan2(x)),
            Self::Sin => x * to_radians(y).sin(),
            Self::Cos => x * to_radians(y).cos(),
            Self::Tan => x * to_radians(y).tan(),
            Self::MulDiv => {
                if z == 0.0 {
                    0.0
                } else {
                    x * y / z
                }
            }
            Self::AddSub => x + y - z,
            Self::AddDiv => {
                if z == 0.0 {
                    0.0
                } else {
                    (x + y) / z
                }
            }
            Self::IfElse => {
                if x > 0.0 {
                    y
                } else {
                    z
                }
            }
            // `pin x y z` clamps the MIDDLE operand into `[x, z]`.
            Self::Pin => {
                if y < x {
                    x
                } else if y > z {
                    z
                } else {
                    y
                }
            }
            Self::Mod => (x * x + y * y + z * z).sqrt(),
            Self::Cat2 => x * z.atan2(y).cos(),
            Self::Sat2 => x * z.atan2(y).sin(),
        };
        value.is_finite().then_some(value)
    }
}

/// One compiled formula.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Formula {
    op: Op,
    args: [Operand; 3],
}

impl Formula {
    fn evaluate(self, width: f64, height: f64, slots: &[f64]) -> Option<f64> {
        let [x, y, z] = self.args.map(|arg| arg.value(width, height, slots));
        self.op.apply(x, y, z)
    }
}

/// Why a geometry did not compile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GeometryError {
    /// A formula is not in the language: an unknown opcode, or the wrong number
    /// of operands. Carries the guide name.
    Formula(String),
    /// A name that is neither a literal, a built-in, nor a guide or adjust value
    /// defined before it.
    UnknownName(String),
    /// A path that does not begin with a move, so its first segment has no start.
    PathWithoutMove,
    /// More slots than the engine indexes (65 535); far beyond the model's
    /// bounds, so only a geometry built around validation reaches it.
    TooManyGuides,
}

impl core::fmt::Display for GeometryError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Formula(name) => write!(f, "guide {name:?} is not a formula in the language"),
            Self::UnknownName(name) => write!(f, "{name:?} names no guide defined before it"),
            Self::PathWithoutMove => f.write_str("a path does not begin with a move"),
            Self::TooManyGuides => f.write_str("too many guides"),
        }
    }
}

impl std::error::Error for GeometryError {}

/// Name → slot bindings while compiling. A later definition shadows an earlier
/// one, which is the "each guide sees those before it" rule when a name repeats.
#[derive(Default)]
struct Scope<'a> {
    names: HashMap<&'a str, u16>,
}

impl<'a> Scope<'a> {
    fn bind(&mut self, name: &'a str, slot: usize) -> Result<(), GeometryError> {
        let slot = u16::try_from(slot).map_err(|_| GeometryError::TooManyGuides)?;
        self.names.insert(name, slot);
        Ok(())
    }

    /// Resolves one token: a literal, then a built-in, then a defined name.
    ///
    /// Built-ins before names, so a geometry cannot redefine `w` out from under
    /// its own path; the standard's presets never do.
    fn operand(&self, token: &str) -> Result<Operand, GeometryError> {
        if token
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_digit() || matches!(byte, b'-' | b'+' | b'.'))
            && let Ok(literal) = token.parse::<f64>()
            && literal.is_finite()
        {
            return Ok(Operand::Const(literal));
        }
        if let Some(builtin) = Builtin::parse(token) {
            return Ok(builtin);
        }
        self.names
            .get(token)
            .map(|slot| Operand::Slot(*slot))
            .ok_or_else(|| GeometryError::UnknownName(token.to_owned()))
    }

    /// Compiles one formula.
    ///
    /// Operands BEYOND the opcode's arity are ignored rather than refused, and
    /// that is the standard's own requirement, not leniency: ECMA-376's preset
    /// annex writes `+- xH 0 dxB 0` — four operands to a three-operand opcode —
    /// in `circularArrow`, `leftCircularArrow` and `leftRightCircularArrow`, and a
    /// producer that embeds those definitions writes the same formula into a
    /// document's `a:custGeom`. Refusing it would refuse three standard shapes.
    /// Too FEW operands is still refused: there is no value to supply.
    fn formula(&self, name: &str, text: &str) -> Result<Formula, GeometryError> {
        let mut tokens = text.split_whitespace();
        let (op, arity) = tokens
            .next()
            .and_then(Op::parse)
            .ok_or_else(|| GeometryError::Formula(name.to_owned()))?;
        let mut args = [Operand::Const(0.0); 3];
        let mut count = 0_usize;
        for token in tokens.take(arity) {
            args[count] = self.operand(token)?;
            count += 1;
        }
        if count != arity {
            return Err(GeometryError::Formula(name.to_owned()));
        }
        Ok(Formula { op, args })
    }

    fn value(&self, value: &super::GeometryValue) -> Result<Operand, GeometryError> {
        match value {
            super::GeometryValue::Literal(literal) => {
                // An i64 literal is bounded by validation to ±MAX_EMU, well inside
                // f64's exact integer range.
                #[allow(clippy::cast_precision_loss)]
                Ok(Operand::Const(*literal as f64))
            }
            super::GeometryValue::Guide(name) => self.operand(name),
        }
    }

    fn point(&self, point: &super::GeometryPoint) -> Result<[Operand; 2], GeometryError> {
        Ok([self.value(&point.x)?, self.value(&point.y)?])
    }
}

/// One compiled path command.
#[derive(Clone, Debug, PartialEq)]
enum Command {
    Move([Operand; 2]),
    Line([Operand; 2]),
    /// `wR hR stAng swAng`.
    Arc([Operand; 4]),
    Quad([Operand; 4]),
    Cubic([Operand; 6]),
    Close,
}

#[derive(Clone, Debug, PartialEq)]
struct CompiledPath {
    width: f64,
    height: f64,
    fill: PathFill,
    stroke: bool,
    commands: Vec<Command>,
}

#[derive(Clone, Debug, PartialEq)]
struct CompiledHandle {
    polar: bool,
    guides: [Option<String>; 2],
    bounds: [(Option<Operand>, Option<Operand>); 2],
    position: [Operand; 2],
}

/// A geometry compiled for evaluation: every formula parsed and every name bound
/// to a slot.
///
/// Built once per geometry — once per process for a preset — and evaluated per
/// box with [`GeometryProgram::evaluate`].
#[derive(Clone, Debug, PartialEq)]
pub struct GeometryProgram {
    adjust_names: Vec<String>,
    adjust_defaults: Vec<Formula>,
    guides: Vec<Formula>,
    paths: Vec<CompiledPath>,
    text_rect: Option<[Operand; 4]>,
    handles: Vec<CompiledHandle>,
}

/// A point resolved into the shape's own space (the caller's unit; `0,0` is the
/// shape's top-left).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedPoint {
    /// x, increasing to the right.
    pub x: f64,
    /// y, increasing downward.
    pub y: f64,
}

/// One resolved path command. Arcs have already become [`ResolvedCommand::CubicTo`]s.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ResolvedCommand {
    /// Start a subpath.
    MoveTo(ResolvedPoint),
    /// A straight segment.
    LineTo(ResolvedPoint),
    /// A quadratic Bézier.
    QuadTo {
        /// The control point.
        control: ResolvedPoint,
        /// The endpoint.
        point: ResolvedPoint,
    },
    /// A cubic Bézier.
    CubicTo {
        /// The control point leaving the current point.
        control1: ResolvedPoint,
        /// The control point entering `point`.
        control2: ResolvedPoint,
        /// The endpoint.
        point: ResolvedPoint,
    },
    /// Close the current subpath.
    Close,
}

/// One resolved path, with how it is painted.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedPath {
    /// How the path is filled.
    pub fill: PathFill,
    /// Whether the path is outlined.
    pub stroke: bool,
    /// The commands, in path order, in the shape's own space.
    pub commands: Vec<ResolvedCommand>,
}

/// A resolved rectangle in the shape's own space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedRect {
    /// Left edge.
    pub left: f64,
    /// Top edge.
    pub top: f64,
    /// Right edge.
    pub right: f64,
    /// Bottom edge.
    pub bottom: f64,
}

/// A geometry evaluated against one box.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedGeometry {
    /// The paths, in paint order.
    pub paths: Vec<ResolvedPath>,
    /// The text rectangle; the whole box when the geometry declares none.
    pub text_rect: ResolvedRect,
}

/// One adjust handle evaluated against a box: where it is drawn, which adjust
/// values it drives, and their ranges.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedHandle<'a> {
    /// `true` for an `a:ahPolar` (radius/angle), `false` for an `a:ahXY`.
    pub polar: bool,
    /// The adjust values driven, `(x or radius, y or angle)`.
    pub guides: [Option<&'a str>; 2],
    /// Each driven value's `(min, max)`, where declared.
    pub bounds: [(Option<f64>, Option<f64>); 2],
    /// Where the handle is drawn, in the shape's own space.
    pub position: ResolvedPoint,
}

impl GeometryProgram {
    /// Compiles a geometry whose adjust values are `adjustments`: parses every
    /// formula and binds every name.
    ///
    /// For a preset, `adjustments` is the definition's own `avLst` — the defaults
    /// — and a shape's authored values are passed later to
    /// [`evaluate`](Self::evaluate) as overrides. For an authored `a:custGeom`,
    /// they are the shape's `avLst` and nothing overrides them.
    ///
    /// Complexity: O(g + c + h) in the geometry's guides, path commands and
    /// handles; each name binds through a hash map.
    ///
    /// # Errors
    ///
    /// [`GeometryError`] for a formula outside the language, a name that is not
    /// defined before its use, or a path that does not begin with a move.
    pub fn compile(
        adjustments: &[ShapeAdjustment],
        geometry: &CustomGeometry,
    ) -> Result<Self, GeometryError> {
        let mut scope = Scope::default();
        let mut adjust_names = Vec::with_capacity(adjustments.len());
        let mut adjust_defaults = Vec::with_capacity(adjustments.len());
        for (slot, adjustment) in adjustments.iter().enumerate() {
            adjust_defaults.push(scope.formula(&adjustment.name, &adjustment.formula)?);
            adjust_names.push(adjustment.name.clone());
            scope.bind(&adjustment.name, slot)?;
        }
        let mut guides = Vec::with_capacity(geometry.guides.len());
        for (index, guide) in geometry.guides.iter().enumerate() {
            guides.push(scope.formula(&guide.name, &guide.formula)?);
            scope.bind(&guide.name, adjustments.len() + index)?;
        }
        let mut paths = Vec::with_capacity(geometry.paths.len());
        for path in &geometry.paths {
            if !matches!(path.commands.first(), Some(ShapePathCommand::MoveTo { .. })) {
                return Err(GeometryError::PathWithoutMove);
            }
            let mut commands = Vec::with_capacity(path.commands.len());
            for command in &path.commands {
                commands.push(match command {
                    ShapePathCommand::MoveTo { point } => Command::Move(scope.point(point)?),
                    ShapePathCommand::LineTo { point } => Command::Line(scope.point(point)?),
                    ShapePathCommand::ArcTo {
                        width_radius,
                        height_radius,
                        start_angle,
                        swing_angle,
                    } => Command::Arc([
                        scope.value(width_radius)?,
                        scope.value(height_radius)?,
                        scope.value(start_angle)?,
                        scope.value(swing_angle)?,
                    ]),
                    ShapePathCommand::QuadBezTo { control, point } => {
                        let [cx, cy] = scope.point(control)?;
                        let [x, y] = scope.point(point)?;
                        Command::Quad([cx, cy, x, y])
                    }
                    ShapePathCommand::CubicBezTo {
                        control1,
                        control2,
                        point,
                    } => {
                        let [ax, ay] = scope.point(control1)?;
                        let [bx, by] = scope.point(control2)?;
                        let [x, y] = scope.point(point)?;
                        Command::Cubic([ax, ay, bx, by, x, y])
                    }
                    ShapePathCommand::Close => Command::Close,
                });
            }
            // Path-space extents are validated to `0..=MAX_EMU`, inside f64's
            // exact integer range.
            #[allow(clippy::cast_precision_loss)]
            paths.push(CompiledPath {
                width: path.width_emu as f64,
                height: path.height_emu as f64,
                fill: path.fill,
                stroke: path.stroke,
                commands,
            });
        }
        let text_rect = match &geometry.text_rect {
            Some(rect) => Some([
                scope.value(&rect.left)?,
                scope.value(&rect.top)?,
                scope.value(&rect.right)?,
                scope.value(&rect.bottom)?,
            ]),
            None => None,
        };
        let mut handles = Vec::with_capacity(geometry.handles.len());
        for handle in &geometry.handles {
            let bound =
                |value: Option<&super::GeometryValue>| -> Result<Option<Operand>, GeometryError> {
                    value.map(|value| scope.value(value)).transpose()
                };
            let [first, second] = handle.bounds();
            handles.push(CompiledHandle {
                polar: matches!(handle, AdjustHandle::Polar { .. }),
                guides: handle.guides().map(|guide| guide.map(str::to_owned)),
                bounds: [
                    (bound(first.0)?, bound(first.1)?),
                    (bound(second.0)?, bound(second.1)?),
                ],
                position: scope.point(handle.position())?,
            });
        }
        Ok(Self {
            adjust_names,
            adjust_defaults,
            guides,
            paths,
            text_rect,
            handles,
        })
    }

    /// The adjust values this geometry declares, in definition order.
    pub fn adjust_names(&self) -> impl Iterator<Item = &str> + '_ {
        self.adjust_names.iter().map(String::as_str)
    }

    /// Evaluates every adjust value and guide for a `width` × `height` box.
    ///
    /// An adjust value named in `overrides` is taken from there; one whose
    /// formula cannot be evaluated (or that names a value this geometry does not
    /// declare) keeps the default, which is a shape someone chose. Overrides may
    /// name built-ins and adjust values defined before them.
    fn slots(&self, width: f64, height: f64, overrides: &[ShapeAdjustment]) -> Option<Vec<f64>> {
        let mut slots = Vec::with_capacity(self.adjust_names.len() + self.guides.len());
        for (index, (name, default)) in self
            .adjust_names
            .iter()
            .zip(&self.adjust_defaults)
            .enumerate()
        {
            let authored = overrides
                .iter()
                .rev()
                .find(|adjustment| adjustment.name == *name)
                .and_then(|adjustment| {
                    let mut scope = Scope::default();
                    for (slot, earlier) in self.adjust_names[..index].iter().enumerate() {
                        scope.bind(earlier, slot).ok()?;
                    }
                    scope
                        .formula(name, &adjustment.formula)
                        .ok()?
                        .evaluate(width, height, &slots)
                });
            let value = match authored {
                Some(value) => value,
                None => default.evaluate(width, height, &slots)?,
            };
            slots.push(value);
        }
        for guide in &self.guides {
            let value = guide.evaluate(width, height, &slots)?;
            slots.push(value);
        }
        Some(slots)
    }

    /// Resolves the geometry for a `width` × `height` box (in the caller's unit),
    /// with `overrides` replacing adjust values by name.
    ///
    /// Returns `None` when a formula has no answer at this box — a division by
    /// zero, the root of a negative — rather than inventing one; the caller paints
    /// the bounding rectangle instead.
    ///
    /// Complexity: O(g + c) in the program's guides and commands — at most four
    /// cubic segments per arc — so O(1) in document size, with no string work.
    #[must_use]
    pub fn evaluate(
        &self,
        width: f64,
        height: f64,
        overrides: &[ShapeAdjustment],
    ) -> Option<ResolvedGeometry> {
        if !(width.is_finite() && height.is_finite()) {
            return None;
        }
        let slots = self.slots(width, height, overrides)?;
        let value = |operand: Operand| operand.value(width, height, &slots);
        let mut paths = Vec::with_capacity(self.paths.len());
        for path in &self.paths {
            paths.push(resolve_path(path, width, height, &value)?);
        }
        let text_rect = match self.text_rect {
            Some([left, top, right, bottom]) => ResolvedRect {
                left: value(left),
                top: value(top),
                right: value(right),
                bottom: value(bottom),
            },
            None => ResolvedRect {
                left: 0.0,
                top: 0.0,
                right: width,
                bottom: height,
            },
        };
        let finite = [
            text_rect.left,
            text_rect.top,
            text_rect.right,
            text_rect.bottom,
        ]
        .iter()
        .all(|edge| edge.is_finite());
        finite.then_some(ResolvedGeometry { paths, text_rect })
    }

    /// The adjust handles evaluated for a `width` × `height` box, in declaration
    /// order — what an editor draws as the yellow drag handles.
    ///
    /// Complexity: as [`evaluate`](Self::evaluate), plus O(h) in the handles.
    #[must_use]
    pub fn handles(
        &self,
        width: f64,
        height: f64,
        overrides: &[ShapeAdjustment],
    ) -> Option<Vec<ResolvedHandle<'_>>> {
        let slots = self.slots(width, height, overrides)?;
        let value = |operand: Operand| operand.value(width, height, &slots);
        self.handles
            .iter()
            .map(|handle| {
                let position = ResolvedPoint {
                    x: value(handle.position[0]),
                    y: value(handle.position[1]),
                };
                (position.x.is_finite() && position.y.is_finite()).then(|| ResolvedHandle {
                    polar: handle.polar,
                    guides: [handle.guides[0].as_deref(), handle.guides[1].as_deref()],
                    bounds: handle
                        .bounds
                        .map(|(min, max)| (min.map(value), max.map(value))),
                    position,
                })
            })
            .collect()
    }
}

/// Resolves one compiled path into shape space.
fn resolve_path(
    path: &CompiledPath,
    width: f64,
    height: f64,
    value: &impl Fn(Operand) -> f64,
) -> Option<ResolvedPath> {
    // A positive `@w`/`@h` is the path's own coordinate space, scaled onto the
    // box; zero means the coordinates are already in shape space (`docs/119` §3).
    let scale_x = if path.width > 0.0 {
        width / path.width
    } else {
        1.0
    };
    let scale_y = if path.height > 0.0 {
        height / path.height
    } else {
        1.0
    };
    let to_shape = |x: f64, y: f64| ResolvedPoint {
        x: x * scale_x,
        y: y * scale_y,
    };
    let mut commands = Vec::with_capacity(path.commands.len());
    // The pen and the subpath start, in PATH space: an arc is computed there and
    // only its resulting points are scaled, which is exact for Béziers under an
    // axis-aligned scale.
    let mut pen = (0.0_f64, 0.0_f64);
    let mut start = pen;
    for command in &path.commands {
        match command {
            Command::Move([x, y]) => {
                pen = (value(*x), value(*y));
                start = pen;
                commands.push(ResolvedCommand::MoveTo(to_shape(pen.0, pen.1)));
            }
            Command::Line([x, y]) => {
                pen = (value(*x), value(*y));
                commands.push(ResolvedCommand::LineTo(to_shape(pen.0, pen.1)));
            }
            Command::Arc([wr, hr, start_angle, swing]) => {
                pen = arc_to(
                    &mut commands,
                    pen,
                    [value(*wr), value(*hr)],
                    [value(*start_angle), value(*swing)],
                    &to_shape,
                );
            }
            Command::Quad([cx, cy, x, y]) => {
                let control = to_shape(value(*cx), value(*cy));
                pen = (value(*x), value(*y));
                commands.push(ResolvedCommand::QuadTo {
                    control,
                    point: to_shape(pen.0, pen.1),
                });
            }
            Command::Cubic([ax, ay, bx, by, x, y]) => {
                let control1 = to_shape(value(*ax), value(*ay));
                let control2 = to_shape(value(*bx), value(*by));
                pen = (value(*x), value(*y));
                commands.push(ResolvedCommand::CubicTo {
                    control1,
                    control2,
                    point: to_shape(pen.0, pen.1),
                });
            }
            Command::Close => {
                pen = start;
                commands.push(ResolvedCommand::Close);
            }
        }
    }
    let finite = commands.iter().all(|command| {
        let points: &[ResolvedPoint] = match command {
            ResolvedCommand::MoveTo(point) | ResolvedCommand::LineTo(point) => {
                core::slice::from_ref(point)
            }
            ResolvedCommand::QuadTo { control, point } => &[*control, *point],
            ResolvedCommand::CubicTo {
                control1,
                control2,
                point,
            } => &[*control1, *control2, *point],
            ResolvedCommand::Close => &[],
        };
        points
            .iter()
            .all(|point| point.x.is_finite() && point.y.is_finite())
    });
    finite.then_some(ResolvedPath {
        fill: path.fill,
        stroke: path.stroke,
        commands,
    })
}

/// The ellipse's PARAMETRIC angle for a VISUAL angle `theta` (both radians).
///
/// DrawingML's `@stAng`/`@swAng` are the direction of the point from the centre;
/// a point on the ellipse `(rx·cos t, ry·sin t)` lies in direction `theta` when
/// `tan t = (rx/ry)·tan theta`. The standard's own presets compute arc endpoints
/// this way (`pie`: `cat2 wd2 (cos hd2 stAng) (sin wd2 stAng)`), so this is the
/// standard's definition, not an interpretation.
///
/// Continuous and monotonic in `theta`: the visual and parametric angles always
/// lie in the same quadrant, so their difference is within a quarter turn and is
/// unwrapped against `theta` itself — which is what keeps a sweep of more than a
/// half turn from folding back.
fn parametric(theta: f64, rx: f64, ry: f64) -> f64 {
    let raw = (rx * theta.sin()).atan2(ry * theta.cos());
    let turn = core::f64::consts::TAU;
    let delta = raw - theta;
    theta + (delta - turn * (delta / turn).round())
}

/// Appends the cubic Béziers of one `a:arcTo` from `pen` and returns the arc's
/// endpoint, in path space.
///
/// `radii` are `[wR, hR]`; `angles` are `[stAng, swAng]` in 60000ths of a degree.
/// The sweep is clamped to one full turn, so the output is at most
/// [`MAX_ARC_SEGMENTS`] curves whatever the file says.
fn arc_to(
    out: &mut Vec<ResolvedCommand>,
    pen: (f64, f64),
    radii: [f64; 2],
    angles: [f64; 2],
    to_shape: &impl Fn(f64, f64) -> ResolvedPoint,
) -> (f64, f64) {
    let (rx, ry) = (radii[0].abs(), radii[1].abs());
    let turn = core::f64::consts::TAU;
    let start = to_radians(angles[0]);
    let swing = to_radians(angles[1]).clamp(-turn, turn);
    if rx == 0.0 && ry == 0.0 || swing == 0.0 || !(start.is_finite() && swing.is_finite()) {
        return pen;
    }
    let t0 = parametric(start, rx, ry);
    let t1 = parametric(start + swing, rx, ry);
    let sweep = t1 - t0;
    let center = (pen.0 - rx * t0.cos(), pen.1 - ry * t0.sin());
    let on = |t: f64| (center.0 + rx * t.cos(), center.1 + ry * t.sin());
    // At most a quarter turn per cubic; the count is a small exact integer.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let segments = ((sweep.abs() / (turn / 4.0)).ceil() as usize).clamp(1, MAX_ARC_SEGMENTS);
    #[allow(clippy::cast_precision_loss)]
    let step = sweep / segments as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    for index in 0..segments {
        #[allow(clippy::cast_precision_loss)]
        let a = t0 + step * index as f64;
        let b = a + step;
        let (ax, ay) = on(a);
        let (bx, by) = on(b);
        let control1 = (ax - k * rx * a.sin(), ay + k * ry * a.cos());
        let control2 = (bx + k * rx * b.sin(), by - k * ry * b.cos());
        out.push(ResolvedCommand::CubicTo {
            control1: to_shape(control1.0, control1.1),
            control2: to_shape(control2.0, control2.1),
            point: to_shape(bx, by),
        });
    }
    on(t1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::{GeometryPoint, GeometryRect, GeometryValue, ShapePath};

    fn adj(name: &str, formula: &str) -> ShapeAdjustment {
        ShapeAdjustment {
            name: name.to_owned(),
            formula: formula.to_owned(),
        }
    }

    /// Evaluates one formula as the only guide of a geometry over a 200 × 100 box,
    /// so `w`, `h`, `ss` and `ls` are all distinguishable — a box with equal sides
    /// could not tell `ss` from `ls` or `w` from `h`.
    fn eval(formula: &str) -> Option<f64> {
        let geometry = CustomGeometry {
            guides: vec![adj("g", formula)],
            paths: vec![ShapePath::new(vec![ShapePathCommand::MoveTo {
                point: GeometryPoint {
                    x: GeometryValue::Guide("g".to_owned()),
                    y: GeometryValue::Literal(0),
                },
            }])],
            ..CustomGeometry::default()
        };
        let program = GeometryProgram::compile(&[], &geometry).ok()?;
        let resolved = program.evaluate(200.0, 100.0, &[])?;
        match resolved.paths[0].commands[0] {
            ResolvedCommand::MoveTo(point) => Some(point.x),
            _ => None,
        }
    }

    #[test]
    fn literal_and_unary_opcodes() {
        assert_eq!(eval("val 16667"), Some(16_667.0));
        assert_eq!(eval("val -5"), Some(-5.0));
        assert_eq!(eval("abs -7"), Some(7.0));
        assert_eq!(eval("sqrt 16"), Some(4.0));
        assert_eq!(
            eval("sqrt -1"),
            Some(0.0),
            "a negative root is float round-off at a tangency, never NaN"
        );
    }

    #[test]
    fn the_three_operand_arithmetic_opcodes() {
        assert_eq!(eval("*/ 10 3 2"), Some(15.0));
        assert_eq!(eval("+- 10 3 2"), Some(11.0));
        assert_eq!(eval("+/ 10 2 3"), Some(4.0));
        assert_eq!(eval("mod 3 4 0"), Some(5.0), "a 3-4-5 triangle");
    }

    /// Never an infinity, and never a refusal of the whole shape either: the
    /// measurement on `Op::apply` is why.
    #[test]
    fn a_zero_divisor_yields_zero_rather_than_infinity() {
        assert_eq!(eval("*/ 10 3 0"), Some(0.0));
        assert_eq!(eval("+/ 10 2 0"), Some(0.0));
        assert_eq!(
            eval("*/ 1e300 1e300 1"),
            None,
            "an overflow is still refused: it is not a shape anyone chose"
        );
    }

    #[test]
    fn if_else_tests_the_first_operand_against_zero() {
        assert_eq!(eval("?: 1 10 20"), Some(10.0));
        assert_eq!(
            eval("?: 0 10 20"),
            Some(20.0),
            "zero is NOT greater than zero"
        );
        assert_eq!(eval("?: -1 10 20"), Some(20.0));
    }

    #[test]
    fn pin_clamps_the_middle_operand() {
        // Which operand is clamped is the thing to get right: `pin x y z` bounds y,
        // not x. Clamping the first would make the low bound the answer here.
        assert_eq!(eval("pin 10 5 20"), Some(10.0), "below the low bound");
        assert_eq!(eval("pin 10 15 20"), Some(15.0), "inside");
        assert_eq!(eval("pin 10 25 20"), Some(20.0), "above the high bound");
    }

    #[test]
    fn angles_are_sixty_thousandths_of_a_degree() {
        let sin = eval("sin 100 5400000").unwrap();
        assert!((sin - 100.0).abs() < 1e-9, "sin(90 deg) = 1, got {sin}");
        let cos = eval("cos 100 5400000").unwrap();
        assert!(cos.abs() < 1e-9, "cos(90 deg) = 0, got {cos}");
        let at2 = eval("at2 10 10").unwrap();
        assert!(
            (at2 - 2_700_000.0).abs() < 1.0,
            "arctan(1) = 45 deg = 2 700 000 units, got {at2}"
        );
    }

    #[test]
    fn at2_operand_order_is_x_then_y_not_atan2s() {
        // `f64::atan2` is called on y with x as the argument, so a naive
        // `x.atan2(y)` would give the complementary angle. These two must differ.
        let shallow = eval("at2 10 1").unwrap();
        let steep = eval("at2 1 10").unwrap();
        assert!(
            shallow < steep,
            "arctan(1/10) must be shallower than arctan(10/1): {shallow} vs {steep}"
        );
    }

    #[test]
    fn cat2_and_sat2_take_the_angle_from_their_second_and_third_operands() {
        let cat2 = eval("cat2 100 10 10").unwrap();
        assert!(
            (cat2 - 100.0 * std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-9,
            "got {cat2}"
        );
        let sat2 = eval("sat2 100 10 10").unwrap();
        assert!(
            (sat2 - 100.0 * std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-9,
            "got {sat2}"
        );
    }

    #[test]
    fn built_in_extents_and_their_fractions_resolve() {
        assert_eq!(eval("val w"), Some(200.0));
        assert_eq!(eval("val h"), Some(100.0));
        assert_eq!(eval("val ss"), Some(100.0), "shortest side");
        assert_eq!(eval("val ls"), Some(200.0), "longest side");
        assert_eq!(eval("val hc"), Some(100.0));
        assert_eq!(eval("val vc"), Some(50.0));
        assert_eq!(eval("val wd4"), Some(50.0));
        assert_eq!(eval("val hd2"), Some(50.0));
        assert_eq!(eval("val ssd8"), Some(12.5));
        // An unusual-but-legal divisor is parsed, not tabulated.
        assert_eq!(eval("val wd16"), Some(12.5));
        assert_eq!(eval("val wd0"), None, "a zero divisor names nothing");
    }

    #[test]
    fn the_box_edges_are_the_shapes_own_space_not_the_page() {
        assert_eq!(eval("val l"), Some(0.0));
        assert_eq!(eval("val t"), Some(0.0));
        assert_eq!(eval("val r"), Some(200.0));
        assert_eq!(eval("val b"), Some(100.0));
    }

    #[test]
    fn angle_constants_are_the_documented_turns() {
        assert_eq!(eval("val cd4"), Some(5_400_000.0), "a quarter turn");
        assert_eq!(eval("val cd2"), Some(10_800_000.0), "a half turn");
        assert_eq!(eval("val 3cd4"), Some(16_200_000.0), "three quarters");
    }

    #[test]
    fn an_unreadable_formula_does_not_compile() {
        for formula in ["", "nope 1 2", "val", "*/ 1 2", "val mystery", "val inf"] {
            assert_eq!(eval(formula), None, "{formula:?} must not evaluate");
        }
    }

    /// The standard's own annex writes `+- xH 0 dxB 0` in three circular-arrow
    /// presets: a fourth operand to a three-operand opcode. It must evaluate as
    /// the three operands it has, or those presets cannot be drawn at all.
    #[test]
    fn operands_beyond_the_arity_are_ignored_as_the_standards_annex_requires() {
        assert_eq!(eval("+- 10 0 3 0"), Some(7.0));
        assert_eq!(eval("val 1 2"), Some(1.0));
    }

    #[test]
    fn a_guide_sees_the_guides_defined_before_it_and_not_after() {
        let point = |name: &str| GeometryPoint {
            x: GeometryValue::Guide(name.to_owned()),
            y: GeometryValue::Literal(0),
        };
        let geometry = CustomGeometry {
            guides: vec![
                adj("half", "*/ h 1 2"),
                adj("quarter", "*/ half 1 2"),
                adj("sum", "+- half quarter 0"),
            ],
            paths: vec![ShapePath::new(vec![
                ShapePathCommand::MoveTo {
                    point: point("half"),
                },
                ShapePathCommand::LineTo {
                    point: point("quarter"),
                },
                ShapePathCommand::LineTo {
                    point: point("sum"),
                },
            ])],
            ..CustomGeometry::default()
        };
        let resolved = GeometryProgram::compile(&[], &geometry)
            .expect("compiles")
            .evaluate(200.0, 100.0, &[])
            .expect("evaluates");
        let xs: Vec<f64> = resolved.paths[0]
            .commands
            .iter()
            .filter_map(|command| match command {
                ResolvedCommand::MoveTo(point) | ResolvedCommand::LineTo(point) => Some(point.x),
                _ => None,
            })
            .collect();
        assert_eq!(xs, vec![50.0, 25.0, 75.0]);

        let forward = CustomGeometry {
            guides: vec![adj("early", "*/ late 1 2"), adj("late", "val 80")],
            ..geometry
        };
        assert_eq!(
            GeometryProgram::compile(&[], &forward),
            Err(GeometryError::UnknownName("late".to_owned())),
            "a forward reference is refused, not silently zero"
        );
    }

    #[test]
    fn an_authored_adjust_value_overrides_the_default_and_a_bad_one_does_not() {
        let geometry = CustomGeometry {
            guides: vec![adj("x", "*/ w adj 100000")],
            paths: vec![ShapePath::new(vec![ShapePathCommand::MoveTo {
                point: GeometryPoint {
                    x: GeometryValue::Guide("x".to_owned()),
                    y: GeometryValue::Literal(0),
                },
            }])],
            ..CustomGeometry::default()
        };
        let program =
            GeometryProgram::compile(&[adj("adj", "val 50000")], &geometry).expect("compiles");
        let x = |overrides: &[ShapeAdjustment]| match program
            .evaluate(200.0, 100.0, overrides)
            .expect("evaluates")
            .paths[0]
            .commands[0]
        {
            ResolvedCommand::MoveTo(point) => point.x,
            _ => f64::NAN,
        };
        assert_eq!(x(&[]), 100.0, "the default");
        assert_eq!(x(&[adj("adj", "val 25000")]), 50.0, "a literal override");
        assert_eq!(
            x(&[adj("adj", "*/ 100000 1 4")]),
            50.0,
            "a computed override is evaluated, not passed over"
        );
        assert_eq!(
            x(&[adj("adj", "bogus 1")]),
            100.0,
            "an unreadable override keeps the default rather than becoming zero"
        );
        assert_eq!(
            x(&[adj("other", "val 0")]),
            100.0,
            "an override of an undeclared value changes nothing"
        );
    }

    #[test]
    fn a_path_space_rescales_onto_the_box_and_shape_space_does_not() {
        let geometry = |w: i64, h: i64| CustomGeometry {
            paths: vec![ShapePath {
                width_emu: w,
                height_emu: h,
                ..ShapePath::new(vec![ShapePathCommand::MoveTo {
                    point: GeometryPoint::literal(10, 10),
                }])
            }],
            ..CustomGeometry::default()
        };
        let first = |geometry: CustomGeometry| match GeometryProgram::compile(&[], &geometry)
            .expect("compiles")
            .evaluate(200.0, 100.0, &[])
            .expect("evaluates")
            .paths[0]
            .commands[0]
        {
            ResolvedCommand::MoveTo(point) => (point.x, point.y),
            _ => (f64::NAN, f64::NAN),
        };
        assert_eq!(first(geometry(20, 20)), (100.0, 50.0), "scaled per axis");
        assert_eq!(first(geometry(0, 0)), (10.0, 10.0), "shape space as is");
        assert_eq!(
            first(geometry(20, 0)),
            (100.0, 10.0),
            "the axes are independent (docs/119's loan rules set @w and not @h)"
        );
    }

    /// A quarter of a circle of radius 100 starting at its LEFT point (0, 100):
    /// `stAng = cd2` (180°) puts the centre at (100, 100). On a page whose y axis
    /// points down, a positive swing is clockwise, so +90° climbs to the top
    /// (100, 0) and -90° descends to the bottom (100, 200).
    #[test]
    fn an_arc_starts_at_the_pen_and_a_positive_swing_is_clockwise() {
        let end = |swing: i64| {
            let geometry = CustomGeometry {
                paths: vec![ShapePath::new(vec![
                    ShapePathCommand::MoveTo {
                        point: GeometryPoint::literal(0, 100),
                    },
                    ShapePathCommand::ArcTo {
                        width_radius: 100.into(),
                        height_radius: 100.into(),
                        start_angle: GeometryValue::Guide("cd2".to_owned()),
                        swing_angle: swing.into(),
                    },
                ])],
                ..CustomGeometry::default()
            };
            let resolved = GeometryProgram::compile(&[], &geometry)
                .expect("compiles")
                .evaluate(200.0, 200.0, &[])
                .expect("evaluates");
            let ResolvedCommand::CubicTo { point, .. } = resolved.paths[0].commands[1] else {
                panic!("an arc becomes a cubic: {:?}", resolved.paths[0].commands);
            };
            point
        };
        let up = end(5_400_000);
        assert!(
            (up.x - 100.0).abs() < 1e-9 && up.y.abs() < 1e-9,
            "+90 degrees from the left point ends at the top: {up:?}"
        );
        let down = end(-5_400_000);
        assert!(
            (down.x - 100.0).abs() < 1e-9 && (down.y - 200.0).abs() < 1e-9,
            "-90 degrees ends at the bottom: {down:?}"
        );
    }

    /// The visual-to-parametric conversion is what makes a 45° arc on a wide
    /// ellipse end where the standard's own `pie` puts it — on the ray at 45°
    /// from the centre — and NOT at the parametric 45° point, which on a 2:1
    /// ellipse lies at about 26.6° visually.
    #[test]
    fn arc_angles_are_visual_not_parametric() {
        let geometry = CustomGeometry {
            paths: vec![ShapePath::new(vec![
                ShapePathCommand::MoveTo {
                    point: GeometryPoint::literal(400, 100),
                },
                ShapePathCommand::ArcTo {
                    width_radius: 200.into(),
                    height_radius: 100.into(),
                    start_angle: 0.into(),
                    swing_angle: 2_700_000.into(),
                },
            ])],
            ..CustomGeometry::default()
        };
        let resolved = GeometryProgram::compile(&[], &geometry)
            .expect("compiles")
            .evaluate(400.0, 200.0, &[])
            .expect("evaluates");
        let ResolvedCommand::CubicTo { point, .. } = resolved.paths[0].commands[1] else {
            panic!("an arc becomes a cubic");
        };
        let (dx, dy) = (point.x - 200.0, point.y - 100.0);
        let visual = dy.atan2(dx).to_degrees();
        assert!(
            (visual - 45.0).abs() < 1e-6,
            "the endpoint lies on the 45-degree ray: {visual} deg ({point:?})"
        );
        assert!(
            ((dx / 200.0).powi(2) + (dy / 100.0).powi(2) - 1.0).abs() < 1e-9,
            "and on the ellipse"
        );
    }

    #[test]
    fn a_whole_turn_is_four_cubics_and_a_hostile_sweep_is_clamped() {
        let geometry = |swing: i64| CustomGeometry {
            paths: vec![ShapePath::new(vec![
                ShapePathCommand::MoveTo {
                    point: GeometryPoint::literal(0, 50),
                },
                ShapePathCommand::ArcTo {
                    width_radius: 50.into(),
                    height_radius: 50.into(),
                    start_angle: GeometryValue::Guide("cd2".to_owned()),
                    swing_angle: swing.into(),
                },
            ])],
            ..CustomGeometry::default()
        };
        for swing in [21_600_000, 9_000_000_000_000] {
            let resolved = GeometryProgram::compile(&[], &geometry(swing))
                .expect("compiles")
                .evaluate(100.0, 100.0, &[])
                .expect("evaluates");
            assert_eq!(
                resolved.paths[0].commands.len(),
                1 + MAX_ARC_SEGMENTS,
                "swing {swing}: one move and at most four quarter-turn cubics"
            );
        }
    }

    #[test]
    fn the_text_rectangle_defaults_to_the_box_and_resolves_guides() {
        let mut geometry =
            CustomGeometry::single_path(ShapePath::new(vec![ShapePathCommand::MoveTo {
                point: GeometryPoint::literal(0, 0),
            }]));
        let program = GeometryProgram::compile(&[], &geometry).expect("compiles");
        assert_eq!(
            program
                .evaluate(200.0, 100.0, &[])
                .expect("evaluates")
                .text_rect,
            ResolvedRect {
                left: 0.0,
                top: 0.0,
                right: 200.0,
                bottom: 100.0
            }
        );
        geometry.text_rect = Some(GeometryRect {
            left: GeometryValue::Guide("wd4".to_owned()),
            top: GeometryValue::Guide("t".to_owned()),
            right: GeometryValue::Guide("hc".to_owned()),
            bottom: GeometryValue::Guide("vc".to_owned()),
        });
        let program = GeometryProgram::compile(&[], &geometry).expect("compiles");
        assert_eq!(
            program
                .evaluate(200.0, 100.0, &[])
                .expect("evaluates")
                .text_rect,
            ResolvedRect {
                left: 50.0,
                top: 0.0,
                right: 100.0,
                bottom: 50.0
            }
        );
    }

    #[test]
    fn a_path_must_begin_with_a_move() {
        let geometry =
            CustomGeometry::single_path(ShapePath::new(vec![ShapePathCommand::LineTo {
                point: GeometryPoint::literal(0, 0),
            }]));
        assert_eq!(
            GeometryProgram::compile(&[], &geometry),
            Err(GeometryError::PathWithoutMove)
        );
    }
}
