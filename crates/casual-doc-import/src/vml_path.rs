//! VML path geometry: the `path` attribute of a `v:shape` (or of the
//! `v:shapetype` it names, or of a `v:path@v` child), evaluated against the
//! shape's `v:formulas` and `adj` values into absolute path commands.
//!
//! # Why this exists
//!
//! Until this module, a `v:shape` was drawn as the outline of its bounding box
//! (`docs/109` HF-392). That is the right fallback and the wrong default: the
//! shapes Word and the PDF-to-Word converters write are frames, callout borders,
//! arrows and connectors whose geometry IS the path. The path is translated
//! into the same `CustomGeometry` an authored DrawingML `a:custGeom` produces,
//! so it is drawn by the one geometry pipeline the renderer already has — the
//! normalising translation `docs/08` ADR-072 records, not a second renderer.
//!
//! # What is evaluated
//!
//! ECMA-376 Part 4 §14.1.2.19 (`path`) and §14.1.2.6 (`v:f`):
//!
//! * commands `m l c x e t r v nf ns at ar wa wr qx qy qb` — move, line, cubic
//!   curve, close, end, their relative forms, the no-fill / no-stroke subpath
//!   flags, the four arc forms (converted to cubic Béziers here, so the model
//!   holds only literal commands), elliptical quadrants and quadratic B-splines;
//! * operands that are literals or `@n` formula references;
//! * every `v:f` operator the standard defines (`val sum product mid abs min
//!   max if mod atan2 sin cos cosatan2 sinatan2 sqrt sumangle ellipse tan`),
//!   with `#n` adjust references and the named shape properties.
//!
//! What is NOT evaluated is refused rather than guessed: the `ae`/`al` angle
//! ellipse forms and the `h*` rounded-corner extensions. A refusal returns
//! `None`, and the caller keeps the bounding-box fallback and reports the shape
//! as shown approximately.
//!
//! # Complexity
//!
//! O(tokens + formulas) per shape. Formula references point backwards only
//! (a formula can name only an earlier one), so evaluation is a single forward
//! pass with no recursion and no cycle to guard against. Command and formula
//! counts are bounded ([`MAX_COMMANDS`], [`MAX_FORMULAS`]) so adversarial input
//! cannot grow the output without limit.

use std::f64::consts::PI;

/// Ceiling on path commands produced for one shape; past it the path is
/// refused (and the shape falls back to its box). Matches the model's own
/// `MAX_SHAPE_PATH_COMMANDS`, so a translated path always validates.
pub(crate) const MAX_COMMANDS: usize = 1024;

/// Ceiling on `v:f` formulas evaluated for one shape.
pub(crate) const MAX_FORMULAS: usize = 256;

/// One absolute path command, in the shape's coordinate space with the
/// `coordorigin` already subtracted (so `0,0` is the box's top-left corner and
/// `width,height` its bottom-right).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VmlPathCommand {
    /// Start a new subpath at a point.
    MoveTo(f64, f64),
    /// A straight segment to a point.
    LineTo(f64, f64),
    /// A cubic Bézier: two control points, then the end point.
    CubicTo([f64; 6]),
    /// Close the current subpath.
    Close,
}

/// One run of commands sharing a fill and stroke switch (`nf`/`ns` apply until
/// the next `e`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VmlSubpath {
    /// Whether this run is filled (`false` after `nf`).
    pub fill: bool,
    /// Whether this run is stroked (`false` after `ns`).
    pub stroke: bool,
    /// The commands, beginning with a [`VmlPathCommand::MoveTo`].
    pub commands: Vec<VmlPathCommand>,
}

/// A shape's evaluated path: its coordinate space and its subpaths.
#[derive(Clone, Debug, PartialEq)]
pub struct VmlGeometry {
    /// The coordinate-space width (`coordsize` x).
    pub width: i64,
    /// The coordinate-space height (`coordsize` y).
    pub height: i64,
    /// The subpaths, in order. Never empty.
    pub subpaths: Vec<VmlSubpath>,
}

/// The inputs a path is evaluated against.
pub(crate) struct PathInputs<'a> {
    /// `coordorigin`.
    pub origin: (i64, i64),
    /// `coordsize`; both positive.
    pub size: (i64, i64),
    /// The `v:f@eqn` strings, in order.
    pub formulas: &'a [String],
    /// The effective adjust values (`adj`, defaulted from the shapetype).
    pub adjust: &'a [i64],
    /// The shape's box in EMU, for the `emuWidth`-family named values.
    pub box_emu: (i64, i64),
    /// Whether the shape is filled / stroked, for `hasFill` / `hasStroke`.
    pub filled: bool,
    /// See `filled`.
    pub stroked: bool,
}

/// Evaluates a VML path. `None` when the path uses a construct this module
/// refuses, references a formula it cannot evaluate, is empty, or exceeds
/// [`MAX_COMMANDS`].
#[must_use]
pub(crate) fn evaluate_path(path: &str, inputs: &PathInputs<'_>) -> Option<VmlGeometry> {
    if inputs.size.0 <= 0 || inputs.size.1 <= 0 {
        return None;
    }
    let formulas = evaluate_formulas(inputs)?;
    let tokens = tokenize(path)?;
    let mut builder = PathBuilder::new(inputs.origin);
    let mut index = 0;
    let mut command: Option<String> = None;
    while index < tokens.len() {
        // Operands are read greedily after a command; a command letter ends the
        // run. A number with no preceding command is malformed.
        match &tokens[index] {
            Token::Command(name) => {
                command = Some(name.clone());
                index += 1;
                let operands = take_operands(&tokens, &mut index, &formulas)?;
                builder.apply(command.as_deref()?, &operands)?;
            }
            Token::Value(_) => {
                // A run of operands after a repeating command (`l` pairs past the
                // first) is consumed by `take_operands` above; reaching one here
                // means operands with no command at all.
                let _ = command.as_ref()?;
                return None;
            }
        }
        if builder.commands_len() > MAX_COMMANDS {
            return None;
        }
    }
    let subpaths = builder.finish();
    (!subpaths.is_empty()).then_some(VmlGeometry {
        width: inputs.size.0,
        height: inputs.size.1,
        subpaths,
    })
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Command(String),
    /// A literal (`Some`) or a formula reference resolved later; an empty
    /// operand between commas is a literal zero.
    Value(Operand),
}

#[derive(Clone, Debug, PartialEq)]
enum Operand {
    Literal(f64),
    Formula(usize),
}

/// Splits a path into commands and operands. Commands are runs of letters;
/// operands are signed integers or `@n`; commas and whitespace separate, and two
/// commas in a row stand for a zero.
fn tokenize(path: &str) -> Option<Vec<Token>> {
    let bytes = path.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    // A comma with nothing before it (another comma or a command) stands for an
    // omitted zero operand, and so does a comma with nothing after it before
    // the next command: `l1769,,1779,x` is `l1769,0,1779,0 x`.
    let mut pending_comma = false;
    let mut after_command = false;
    while i < bytes.len() {
        let byte = bytes[i];
        if byte.is_ascii_alphabetic() {
            if pending_comma {
                tokens.push(Token::Value(Operand::Literal(0.0)));
            }
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                i += 1;
            }
            // Command letters run together (`xe`, `nfns`): split them into the
            // known command names greedily.
            split_commands(&path[start..i], &mut tokens)?;
            pending_comma = false;
            after_command = true;
        } else if byte == b',' {
            if pending_comma || after_command {
                tokens.push(Token::Value(Operand::Literal(0.0)));
            }
            pending_comma = true;
            after_command = false;
            i += 1;
        } else if byte.is_ascii_whitespace() {
            i += 1;
        } else if byte == b'@' {
            let start = i + 1;
            i = start;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            let n: usize = path[start..i].parse().ok()?;
            tokens.push(Token::Value(Operand::Formula(n)));
            pending_comma = false;
            after_command = false;
        } else if byte == b'-' || byte == b'+' || byte.is_ascii_digit() || byte == b'.' {
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            let value: f64 = path[start..i].parse().ok()?;
            tokens.push(Token::Value(Operand::Literal(value)));
            pending_comma = false;
            after_command = false;
        } else {
            return None;
        }
    }
    Some(tokens)
}

/// The command names, longest first so `nf` is not read as `n` + `f`.
const COMMANDS: [&str; 18] = [
    "nf", "ns", "at", "ar", "wa", "wr", "qx", "qy", "qb", "ae", "al", "m", "l", "c", "x", "e", "t",
    "r",
];

fn split_commands(run: &str, tokens: &mut Vec<Token>) -> Option<()> {
    let mut rest = run;
    while !rest.is_empty() {
        // `v` (relative curve) is the one single-letter command missing from the
        // table above, because the table is also the refusal list's spelling.
        if let Some(stripped) = rest.strip_prefix('v') {
            tokens.push(Token::Command("v".to_owned()));
            rest = stripped;
            continue;
        }
        let name = COMMANDS.iter().find(|name| rest.starts_with(**name))?;
        tokens.push(Token::Command((*name).to_owned()));
        rest = &rest[name.len()..];
    }
    Some(())
}

/// Reads the operands following a command, resolving formula references.
fn take_operands(tokens: &[Token], index: &mut usize, formulas: &[f64]) -> Option<Vec<f64>> {
    let mut operands = Vec::new();
    while let Some(Token::Value(operand)) = tokens.get(*index) {
        operands.push(match operand {
            Operand::Literal(value) => *value,
            Operand::Formula(n) => *formulas.get(*n)?,
        });
        *index += 1;
    }
    Some(operands)
}

/// Accumulates subpaths, tracking the current point for the relative forms.
struct PathBuilder {
    origin: (f64, f64),
    subpaths: Vec<VmlSubpath>,
    current: Option<VmlSubpath>,
    point: (f64, f64),
    fill: bool,
    stroke: bool,
}

impl PathBuilder {
    fn new(origin: (i64, i64)) -> Self {
        Self {
            origin: (origin.0 as f64, origin.1 as f64),
            subpaths: Vec::new(),
            current: None,
            point: (0.0, 0.0),
            fill: true,
            stroke: true,
        }
    }

    fn commands_len(&self) -> usize {
        self.subpaths
            .iter()
            .chain(self.current.as_ref())
            .map(|path| path.commands.len())
            .sum()
    }

    fn local(&self, x: f64, y: f64) -> (f64, f64) {
        (x - self.origin.0, y - self.origin.1)
    }

    fn push(&mut self, command: VmlPathCommand) {
        let current = self.current.get_or_insert_with(|| VmlSubpath {
            fill: self.fill,
            stroke: self.stroke,
            commands: Vec::new(),
        });
        // A segment with no preceding move starts at the current point.
        if current.commands.is_empty() && !matches!(command, VmlPathCommand::MoveTo(..)) {
            current
                .commands
                .push(VmlPathCommand::MoveTo(self.point.0, self.point.1));
        }
        current.commands.push(command);
    }

    fn move_to(&mut self, x: f64, y: f64) {
        // A move inside a run of commands starts a new subpath within the same
        // flag run, which the model represents as another command list.
        self.flush();
        self.point = (x, y);
        self.push(VmlPathCommand::MoveTo(x, y));
    }

    fn line_to(&mut self, x: f64, y: f64) {
        self.point = (x, y);
        self.push(VmlPathCommand::LineTo(x, y));
    }

    fn cubic_to(&mut self, c: [f64; 6]) {
        self.point = (c[4], c[5]);
        self.push(VmlPathCommand::CubicTo(c));
    }

    fn flush(&mut self) {
        if let Some(path) = self.current.take()
            && path.commands.len() > 1
        {
            self.subpaths.push(path);
        }
    }

    /// Applies one command and its operands.
    fn apply(&mut self, command: &str, operands: &[f64]) -> Option<()> {
        let chunks = |size: usize| -> Option<std::slice::Chunks<'_, f64>> {
            operands
                .len()
                .is_multiple_of(size)
                .then(|| operands.chunks(size))
        };
        match command {
            "m" => {
                let mut pairs = chunks(2)?;
                let first = pairs.next()?;
                let (x, y) = self.local(first[0], first[1]);
                self.move_to(x, y);
                for pair in pairs {
                    let (x, y) = self.local(pair[0], pair[1]);
                    self.line_to(x, y);
                }
            }
            "t" => {
                let mut pairs = chunks(2)?;
                let first = pairs.next()?;
                let (x, y) = (self.point.0 + first[0], self.point.1 + first[1]);
                self.move_to(x, y);
                for pair in pairs {
                    self.line_to(self.point.0 + pair[0], self.point.1 + pair[1]);
                }
            }
            "l" => {
                for pair in chunks(2)? {
                    let (x, y) = self.local(pair[0], pair[1]);
                    self.line_to(x, y);
                }
            }
            "r" => {
                for pair in chunks(2)? {
                    self.line_to(self.point.0 + pair[0], self.point.1 + pair[1]);
                }
            }
            "c" => {
                for six in chunks(6)? {
                    let (a, b) = self.local(six[0], six[1]);
                    let (c, d) = self.local(six[2], six[3]);
                    let (e, f) = self.local(six[4], six[5]);
                    self.cubic_to([a, b, c, d, e, f]);
                }
            }
            "v" => {
                for six in chunks(6)? {
                    let (px, py) = self.point;
                    self.cubic_to([
                        px + six[0],
                        py + six[1],
                        px + six[2],
                        py + six[3],
                        px + six[4],
                        py + six[5],
                    ]);
                }
            }
            "x" => {
                if self.current.is_some() {
                    self.push(VmlPathCommand::Close);
                }
            }
            "e" => {
                self.flush();
                self.fill = true;
                self.stroke = true;
            }
            "nf" => {
                self.flush();
                self.fill = false;
            }
            "ns" => {
                self.flush();
                self.stroke = false;
            }
            "at" | "ar" | "wa" | "wr" => {
                for eight in chunks(8)? {
                    self.arc(command, eight);
                }
            }
            "qx" | "qy" => {
                let mut x_first = command == "qx";
                for pair in chunks(2)? {
                    let (x, y) = self.local(pair[0], pair[1]);
                    self.quadrant(x, y, x_first);
                    x_first = !x_first;
                }
            }
            "qb" => self.quadratic_spline(operands)?,
            // `ae`/`al` and anything else: refused, so the caller falls back.
            _ => return None,
        }
        Some(())
    }

    /// The four arc forms: an ellipse inscribed in `l,t,r,b`, from the radial
    /// through `x1,y1` to the radial through `x2,y2`. `at`/`ar` run
    /// counter-clockwise, `wa`/`wr` clockwise (on screen, y down); `at`/`wa`
    /// draw a line from the current point to the arc's start, `ar`/`wr` start a
    /// new subpath there.
    fn arc(&mut self, command: &str, eight: &[f64]) {
        let (l, t) = self.local(eight[0], eight[1]);
        let (r, b) = self.local(eight[2], eight[3]);
        let (x1, y1) = self.local(eight[4], eight[5]);
        let (x2, y2) = self.local(eight[6], eight[7]);
        let (cx, cy) = ((l + r) / 2.0, (t + b) / 2.0);
        let (rx, ry) = (((r - l) / 2.0).abs(), ((b - t) / 2.0).abs());
        if rx == 0.0 || ry == 0.0 {
            return;
        }
        let angle = |x: f64, y: f64| ((y - cy) / ry).atan2((x - cx) / rx);
        let start = angle(x1, y1);
        let end = angle(x2, y2);
        let clockwise = matches!(command, "wa" | "wr");
        // Screen angles grow clockwise (y points down). Equal radials are a
        // full turn, which is how Word writes a whole ellipse as one arc.
        let mut sweep = if clockwise { end - start } else { start - end };
        sweep = sweep.rem_euclid(2.0 * PI);
        if sweep == 0.0 {
            sweep = 2.0 * PI;
        }
        if !clockwise {
            sweep = -sweep;
        }
        let begin = (cx + rx * start.cos(), cy + ry * start.sin());
        if matches!(command, "ar" | "wr") || self.current.is_none() {
            self.move_to(begin.0, begin.1);
        } else {
            self.line_to(begin.0, begin.1);
        }
        for segment in arc_to_cubics(cx, cy, rx, ry, start, sweep) {
            self.cubic_to(segment);
        }
    }

    /// An elliptical quadrant from the current point to `x,y`, leaving along
    /// x first (`qx`) or y first (`qy`).
    fn quadrant(&mut self, x: f64, y: f64, x_first: bool) {
        let (px, py) = self.point;
        let (dx, dy) = (x - px, y - py);
        let control = if x_first {
            [px + KAPPA * dx, py, x, y - KAPPA * dy]
        } else {
            [px, py + KAPPA * dy, x - KAPPA * dx, y]
        };
        self.cubic_to([control[0], control[1], control[2], control[3], x, y]);
    }

    /// A quadratic B-spline (`qb`): every point but the last is a control
    /// point; between two consecutive control points the curve passes through
    /// their midpoint (the TrueType convention the standard describes).
    fn quadratic_spline(&mut self, operands: &[f64]) -> Option<()> {
        if operands.len() < 4 || !operands.len().is_multiple_of(2) {
            return None;
        }
        let points: Vec<(f64, f64)> = operands
            .chunks(2)
            .map(|pair| self.local(pair[0], pair[1]))
            .collect();
        let (last, controls) = points.split_last()?;
        for (index, control) in controls.iter().enumerate() {
            let end = match controls.get(index + 1) {
                Some(next) => ((control.0 + next.0) / 2.0, (control.1 + next.1) / 2.0),
                None => *last,
            };
            let start = self.point;
            // Degree elevation: a quadratic with control Q is the cubic with
            // controls P0 + 2/3 (Q - P0) and P1 + 2/3 (Q - P1).
            self.cubic_to([
                start.0 + 2.0 / 3.0 * (control.0 - start.0),
                start.1 + 2.0 / 3.0 * (control.1 - start.1),
                end.0 + 2.0 / 3.0 * (control.0 - end.0),
                end.1 + 2.0 / 3.0 * (control.1 - end.1),
                end.0,
                end.1,
            ]);
        }
        Some(())
    }

    fn finish(mut self) -> Vec<VmlSubpath> {
        self.flush();
        self.subpaths
    }
}

/// The cubic Bézier circle constant: a quarter ellipse's control points sit
/// this fraction of the radius along each tangent.
const KAPPA: f64 = 0.552_284_749_830_793_4;

/// Splits an elliptical arc into at most four cubic Béziers, each spanning no
/// more than a quarter turn (the standard approximation; error < 0.03%).
fn arc_to_cubics(cx: f64, cy: f64, rx: f64, ry: f64, start: f64, sweep: f64) -> Vec<[f64; 6]> {
    let count = (sweep.abs() / (PI / 2.0)).ceil().clamp(1.0, 4.0) as usize;
    let step = sweep / count as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let mut out = Vec::with_capacity(count);
    let mut angle = start;
    for _ in 0..count {
        let next = angle + step;
        let (s0, c0) = angle.sin_cos();
        let (s1, c1) = next.sin_cos();
        out.push([
            cx + rx * (c0 - k * s0),
            cy + ry * (s0 + k * c0),
            cx + rx * (c1 + k * s1),
            cy + ry * (s1 - k * c1),
            cx + rx * c1,
            cy + ry * s1,
        ]);
        angle = next;
    }
    out
}

/// VML angles in formulas are degrees in 16.16 fixed point.
const FD: f64 = 65536.0;

/// Evaluates the `v:f` list in order. `None` when a formula is malformed or
/// names something this module does not know — the path then cannot be
/// trusted, and the caller falls back.
fn evaluate_formulas(inputs: &PathInputs<'_>) -> Option<Vec<f64>> {
    if inputs.formulas.len() > MAX_FORMULAS {
        return None;
    }
    let mut results: Vec<f64> = Vec::with_capacity(inputs.formulas.len());
    for eqn in inputs.formulas {
        let mut parts = eqn.split_whitespace();
        let op = parts.next()?;
        let args: Vec<f64> = parts
            .map(|token| argument(token, inputs, &results))
            .collect::<Option<_>>()?;
        let arg = |i: usize| args.get(i).copied().unwrap_or(0.0);
        let (a, b, c) = (arg(0), arg(1), arg(2));
        let to_rad = |fd: f64| fd / FD * PI / 180.0;
        let value = match op {
            "val" => a,
            "sum" => a + b - c,
            "product" | "prod" => {
                if c == 0.0 {
                    0.0
                } else {
                    a * b / c
                }
            }
            "mid" => (a + b) / 2.0,
            "abs" => a.abs(),
            "min" => a.min(b),
            "max" => a.max(b),
            "if" => {
                if a > 0.0 {
                    b
                } else {
                    c
                }
            }
            "mod" => (a * a + b * b + c * c).sqrt(),
            "atan2" => b.atan2(a) * 180.0 / PI * FD,
            "sin" => a * to_rad(b).sin(),
            "cos" => a * to_rad(b).cos(),
            "tan" => a * to_rad(b).tan(),
            "cosatan2" => a * c.atan2(b).cos(),
            "sinatan2" => a * c.atan2(b).sin(),
            "sqrt" => a.max(0.0).sqrt(),
            "sumangle" => a + b * FD - c * FD,
            "ellipse" => {
                if b == 0.0 {
                    0.0
                } else {
                    c * (1.0 - (a / b) * (a / b)).max(0.0).sqrt()
                }
            }
            _ => return None,
        };
        results.push(if value.is_finite() { value } else { 0.0 });
    }
    Some(results)
}

/// One formula argument: a literal, `#n` (an adjust value), `@n` (an earlier
/// formula) or a named shape property.
fn argument(token: &str, inputs: &PathInputs<'_>, results: &[f64]) -> Option<f64> {
    if let Some(n) = token.strip_prefix('#') {
        return Some(*inputs.adjust.get(n.parse::<usize>().ok()?)? as f64);
    }
    if let Some(n) = token.strip_prefix('@') {
        return results.get(n.parse::<usize>().ok()?).copied();
    }
    if let Ok(value) = token.parse::<f64>() {
        return Some(value);
    }
    let (w, h) = (inputs.size.0 as f64, inputs.size.1 as f64);
    let (ox, oy) = (inputs.origin.0 as f64, inputs.origin.1 as f64);
    let (ew, eh) = (inputs.box_emu.0 as f64, inputs.box_emu.1 as f64);
    // 1px at 96 dpi is 9525 EMU.
    let pixels = |emu: f64| emu / 9525.0;
    Some(match token {
        "width" => w,
        "height" => h,
        "xcenter" => ox + w / 2.0,
        "ycenter" => oy + h / 2.0,
        "xlimo" | "ylimo" => 0.0,
        "hasFill" => f64::from(u8::from(inputs.filled)),
        "hasStroke" | "lineDrawn" => f64::from(u8::from(inputs.stroked)),
        "pixelWidth" => pixels(ew),
        "pixelHeight" => pixels(eh),
        // A line is drawn one device pixel wide unless a weight says otherwise;
        // the weight is not an input here, so the hairline is the answer.
        "pixelLineWidth" => 1.0,
        "emuWidth" => ew,
        "emuHeight" => eh,
        "emuWidth2" => ew / 2.0,
        "emuHeight2" => eh / 2.0,
        "emuLineWidth" => 9525.0,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs<'a>(formulas: &'a [String], adjust: &'a [i64]) -> PathInputs<'a> {
        PathInputs {
            origin: (0, 0),
            size: (21600, 21600),
            formulas,
            adjust,
            box_emu: (914_400, 914_400),
            filled: true,
            stroked: true,
        }
    }

    #[test]
    fn a_closed_polygon_with_origin_and_omitted_operands() {
        let geometry = evaluate_path(
            "m1779,993l1769,993,1769,,1779,xe",
            &PathInputs {
                origin: (1769, 389),
                size: (8798, 1508),
                ..inputs(&[], &[])
            },
        )
        .expect("evaluates");
        assert_eq!(geometry.width, 8798);
        assert_eq!(
            geometry.subpaths[0].commands,
            vec![
                VmlPathCommand::MoveTo(10.0, 604.0),
                VmlPathCommand::LineTo(0.0, 604.0),
                // `1769,,` is `1769,0`: an omitted operand is zero, before the
                // origin is subtracted.
                VmlPathCommand::LineTo(0.0, -389.0),
                VmlPathCommand::LineTo(10.0, -389.0),
                VmlPathCommand::Close,
            ]
        );
    }

    #[test]
    fn nf_and_ns_split_subpaths_and_reset_at_e() {
        let geometry =
            evaluate_path("m0,0l10,0nsl10,10xem0,0l5,5e", &inputs(&[], &[])).expect("ok");
        let flags: Vec<(bool, bool)> = geometry
            .subpaths
            .iter()
            .map(|path| (path.fill, path.stroke))
            .collect();
        assert_eq!(flags, vec![(true, true), (true, false), (true, true)]);
    }

    #[test]
    fn formulas_resolve_adjust_and_earlier_results() {
        let formulas = vec!["val #0".to_owned(), "sum width 0 @0".to_owned()];
        let geometry =
            evaluate_path("m@0,0l@1,0,@1,@1xe", &inputs(&formulas, &[5400])).expect("evaluates");
        assert_eq!(
            geometry.subpaths[0].commands[1],
            VmlPathCommand::LineTo(16200.0, 0.0)
        );
    }

    #[test]
    fn a_full_clockwise_arc_is_four_cubics_ending_where_it_started() {
        let geometry =
            evaluate_path("wr0,0,100,100,100,50,100,50e", &inputs(&[], &[])).expect("ok");
        let commands = &geometry.subpaths[0].commands;
        assert_eq!(commands[0], VmlPathCommand::MoveTo(100.0, 50.0));
        assert_eq!(commands.len(), 5);
        let VmlPathCommand::CubicTo(last) = commands[4] else {
            panic!("an arc is drawn with cubics");
        };
        assert!((last[4] - 100.0).abs() < 1e-9 && (last[5] - 50.0).abs() < 1e-9);
        // Clockwise on screen from 3 o'clock passes 6 o'clock (y = 100) first.
        let VmlPathCommand::CubicTo(first) = commands[1] else {
            panic!("cubic");
        };
        assert!((first[5] - 100.0).abs() < 1e-9, "{first:?}");
    }

    #[test]
    fn refused_constructs_return_none() {
        assert!(evaluate_path("m0,0ae5,5,5,5,0,90e", &inputs(&[], &[])).is_none());
        assert!(evaluate_path("m@3,0l1,1e", &inputs(&[], &[])).is_none());
        assert!(evaluate_path("m0,0l1,1", &inputs(&["bogus 1".to_owned()], &[])).is_none());
        assert!(evaluate_path("", &inputs(&[], &[])).is_none());
    }

    #[test]
    fn relative_forms_and_quadrants_track_the_current_point() {
        let geometry = evaluate_path("t10,10r5,0qy20,20e", &inputs(&[], &[])).expect("ok");
        let commands = &geometry.subpaths[0].commands;
        assert_eq!(commands[0], VmlPathCommand::MoveTo(10.0, 10.0));
        assert_eq!(commands[1], VmlPathCommand::LineTo(15.0, 10.0));
        let VmlPathCommand::CubicTo(curve) = commands[2] else {
            panic!("a quadrant is a cubic");
        };
        assert_eq!((curve[4], curve[5]), (20.0, 20.0));
        // `qy` leaves vertically: the first control point keeps the start's x.
        assert_eq!(curve[0], 15.0);
    }
}
