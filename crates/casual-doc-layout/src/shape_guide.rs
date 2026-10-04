// SPDX-License-Identifier: Apache-2.0

//! The DrawingML shape guide formula language (`a:gd@fmla`, ECMA-376 Part 1
//! §20.1.9.11), and the variable environment a formula is evaluated in.
//!
//! # Why this exists
//!
//! A shape's adjustment list (`a:avLst`) and guide list (`a:gdLst`) hold named
//! values, each either a literal or a small expression over the shape's box. Before
//! this module, layout read only the literal form `val N` and silently fell back to
//! a documented default for anything else — so a shape whose `adj` is `*/ h 1 2`
//! drew with the *default* proportions rather than the authored ones, with no
//! report, because nothing in the pipeline knew a formula had been passed over.
//!
//! # The named prior art, and the part that is NOT prior art
//!
//! This is an **interpreter over a tiny expression language**: a fixed opcode table,
//! up to three operands, no nesting, no precedence. There is nothing to invent, and
//! deliberately nothing clever here — the whole point (`119` §6, `109` FID-L-04) is
//! that the 187 preset geometries are *data* evaluated by one interpreter rather
//! than 187 hand-written vertex lists.
//!
//! ONLYOFFICE was read as a **behavioural reference only**, to confirm the opcode
//! set and the built-in variable names a real implementation must recognise. Their
//! code is AGPL-3.0-only and none of it is reproduced here: the semantics below come
//! from ECMA-376, and the preset definition table they hand-transcribed is
//! explicitly NOT copied — its provenance is an open owner decision (`156` §11 Q6).
//!
//! # Units
//!
//! Lengths are whatever unit the caller's box is in (layout passes twips). Angles
//! are **1/60000 of a degree**, the DrawingML convention, for both the angle
//! operands of `sin`/`cos`/`tan` and the result of `at2`.

use casual_doc_model::v1::ShapeAdjustment;

// Why there is no guide cap here.
//
// There was one — 32, mirroring the model's `MAX_SHAPE_ADJUSTMENTS` — and it was in
// the wrong layer twice over. For AUTHORED guides it is redundant: import already
// refuses a shape with more than that, so nothing unbounded reaches layout. And for
// the committed preset table it is simply wrong: 28 of the 187 ECMA-376 presets
// declare more than 32 guides and `gear9` declares 244, so the cap silently refused
// nine arc-free presets that resolve perfectly well. The guides-per-preset ceiling is
// asserted against the table in `crate::shape_preset`, where the data is.
//
// Both callers are therefore bounded by construction: authored guides at import, the
// table by being committed.

/// The variable environment a guide formula resolves names against.
///
/// Coordinates are in the shape's **own** space — `l`/`t` are zero and `r`/`b` are
/// the extents — because that is what ECMA-376's preset definitions are written in.
/// This is deliberately not the absolute page box: a formula is a property of the
/// shape, not of where the shape happens to sit, and evaluating `r` as a page
/// coordinate would scale every authored proportion by the page offset.
#[derive(Clone, Copy, Debug)]
pub struct GuideBox {
    width: f64,
    height: f64,
}

impl GuideBox {
    /// The environment for a shape of `width` × `height`, in the caller's unit.
    #[must_use]
    pub fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }

    /// The value of a built-in variable name, or `None` if it is not one.
    ///
    /// The fraction families (`wd2`…`wd32`, `hd2`…, `ssd2`…) are parsed rather than
    /// tabulated, so an unusual-but-legal divisor resolves instead of silently
    /// becoming an unknown name.
    fn builtin(self, name: &str) -> Option<f64> {
        let w = self.width;
        let h = self.height;
        let ss = w.min(h);
        match name {
            "w" | "wd1" => return Some(w),
            "h" | "hd1" => return Some(h),
            "ss" | "ssd1" => return Some(ss),
            "ls" => return Some(w.max(h)),
            "l" | "t" => return Some(0.0),
            "r" => return Some(w),
            "b" => return Some(h),
            "hc" => return Some(w / 2.0),
            "vc" => return Some(h / 2.0),
            // Angle constants, in 1/60000 degree. A quarter turn is 5 400 000.
            "cd1" => return Some(21_600_000.0),
            "cd2" => return Some(10_800_000.0),
            "cd4" => return Some(5_400_000.0),
            "cd8" => return Some(2_700_000.0),
            "3cd4" => return Some(16_200_000.0),
            "3cd8" => return Some(8_100_000.0),
            "5cd8" => return Some(13_500_000.0),
            "7cd8" => return Some(18_900_000.0),
            _ => {}
        }
        // `wd<n>` / `hd<n>` / `ssd<n>`: the named extent divided by n.
        let (base, digits) = if let Some(rest) = name.strip_prefix("ssd") {
            (ss, rest)
        } else if let Some(rest) = name.strip_prefix("wd") {
            (w, rest)
        } else if let Some(rest) = name.strip_prefix("hd") {
            (h, rest)
        } else {
            return None;
        };
        let divisor: f64 = digits.parse::<u32>().ok().filter(|n| *n > 0)?.into();
        Some(base / divisor)
    }
}

/// One evaluated guide: its name and its value.
#[derive(Clone, Copy, Debug)]
pub struct Guide<'a> {
    name: &'a str,
    value: f64,
}

/// A shape's guides, evaluated in order, ready to resolve coordinate tokens against.
///
/// Exists because a preset's path coordinates are not values but TOKENS — `l`, `r`,
/// `x1`, `19098` — each either a literal, a built-in, or one of the guides evaluated
/// here. Resolving them one at a time against a re-evaluated list would be O(g) per
/// coordinate; evaluating once and resolving against the result is O(1) per coordinate
/// after an O(g) build.
#[derive(Debug)]
pub struct Resolved<'a> {
    guides: Vec<Guide<'a>>,
    shape: GuideBox,
}

impl<'a> Resolved<'a> {
    /// Evaluates `pairs` — `(name, formula)` in authored order — against `shape`.
    ///
    /// Each formula sees the built-ins plus the guides before it, which is the ordering
    /// ECMA-376's preset definitions depend on. An unreadable formula is skipped, so a
    /// later guide naming it is unreadable too rather than silently zero.
    pub fn new(pairs: impl IntoIterator<Item = (&'a str, &'a str)>, shape: GuideBox) -> Self {
        let mut guides: Vec<Guide<'a>> = Vec::new();
        for (name, formula) in pairs {
            if let Some(value) = evaluate(formula, shape, &guides) {
                guides.push(Guide { name, value });
            }
        }
        Self { guides, shape }
    }

    /// The value of a named guide, or `None` when absent or unreadable.
    #[must_use]
    pub fn value(&self, name: &str) -> Option<f64> {
        self.guides
            .iter()
            .rev()
            .find(|guide| guide.name == name)
            .map(|guide| guide.value)
    }

    /// One coordinate token: a literal, a built-in, or a guide defined here.
    #[must_use]
    pub fn token(&self, token: &str) -> Option<f64> {
        operand(token, self.shape, &self.guides)
    }
}

/// Evaluates a shape's guide list in order, each formula seeing the built-ins plus
/// every guide defined *before* it — which is the order dependency ECMA-376's
/// preset definitions rely on.
///
/// A guide whose formula cannot be read is **skipped rather than defaulted to
/// zero**: zero is a legal value that would silently reshape the geometry, whereas
/// a missing name leaves the caller's documented default in place.
///
/// Complexity: O(g) in the shape's guides, which the caller bounds.
fn evaluate_all<'a>(guides: &'a [ShapeAdjustment], shape: GuideBox) -> Resolved<'a> {
    Resolved::new(
        guides
            .iter()
            .map(|guide| (guide.name.as_str(), guide.formula.as_str())),
        shape,
    )
}

/// The value of the guide named `name`, or `None` when it is absent or unreadable.
///
/// Complexity: O(g) over the shape's own guides, which import bounds.
#[must_use]
pub fn guide_value(guides: &[ShapeAdjustment], name: &str, shape: GuideBox) -> Option<f64> {
    evaluate_all(guides, shape).value(name)
}

/// Resolves one operand: a literal, a built-in, or an earlier guide.
fn operand(token: &str, shape: GuideBox, resolved: &[Guide<'_>]) -> Option<f64> {
    if let Ok(literal) = token.parse::<f64>() {
        return literal.is_finite().then_some(literal);
    }
    if let Some(value) = shape.builtin(token) {
        return Some(value);
    }
    // Later definitions win, matching the "each guide sees those before it" rule
    // when a name is redefined.
    resolved
        .iter()
        .rev()
        .find(|guide| guide.name == token)
        .map(|guide| guide.value)
}

/// Degrees-per-unit for DrawingML's 1/60000-degree angles.
const ANGLE_UNITS_PER_DEGREE: f64 = 60_000.0;

fn to_radians(angle: f64) -> f64 {
    (angle / ANGLE_UNITS_PER_DEGREE).to_radians()
}

fn from_radians(radians: f64) -> f64 {
    radians.to_degrees() * ANGLE_UNITS_PER_DEGREE
}

/// Evaluates one `a:gd@fmla` formula.
///
/// Returns `None` for an unknown opcode, a wrong operand count, an unresolvable
/// name, or a result that is not finite — a division by zero included. Refusing is
/// the right answer rather than producing an infinity: the caller keeps the
/// documented default, which is a shape someone chose, where an infinity would be a
/// shape nobody chose.
///
/// # The one over-long formula the specification itself writes
///
/// No opcode takes more than three operands, but ECMA-376's own normative preset
/// data writes eight guides with four, always with a redundant trailing `0`:
/// `+- xH 0 dxB 0` in `circularArrow`, `leftCircularArrow` and
/// `leftRightCircularArrow` (`fixtures/spec/presetShapeDefinitions.xml`, e.g. line
/// 4363). The reading is not ambiguous — the guide directly above each one is
/// `+- xH dxG 0` with `dxG == dxB`, so the pair is the two arrowhead corners either
/// side of `xH` — and a trailing zero cannot change `x + y - z` whichever way it is
/// grouped. So a FOURTH operand is dropped when it is exactly zero, and refused
/// otherwise; without this the three circular-arrow presets cannot resolve at all,
/// for a reason that has nothing to do with the commands in their paths. A fifth
/// operand, or a non-zero fourth, is still not a formula in this language.
#[must_use]
fn evaluate(formula: &str, shape: GuideBox, resolved: &[Guide<'_>]) -> Option<f64> {
    let mut tokens = formula.split_whitespace();
    let op = tokens.next()?;
    let mut args = [0.0_f64; 4];
    let mut count = 0_usize;
    for token in tokens {
        if count == args.len() {
            // A fifth operand means this is not a formula in the language.
            return None;
        }
        args[count] = operand(token, shape, resolved)?;
        count += 1;
    }
    if count == 4 {
        if args[3] != 0.0 {
            return None;
        }
        count = 3;
    }
    let (x, y, z) = (args[0], args[1], args[2]);
    let value = match (op, count) {
        ("val", 1) => x,
        ("abs", 1) => x.abs(),
        ("sqrt", 1) => {
            if x < 0.0 {
                return None;
            }
            x.sqrt()
        }
        ("max", 2) => x.max(y),
        ("min", 2) => x.min(y),
        // `at2 x y` is arctan(y / x) — operand order is x then y, which is the
        // reverse of `f64::atan2`'s receiver/argument order.
        ("at2", 2) => from_radians(y.atan2(x)),
        ("sin", 2) => x * to_radians(y).sin(),
        ("cos", 2) => x * to_radians(y).cos(),
        ("tan", 2) => x * to_radians(y).tan(),
        ("*/", 3) => x * y / z,
        ("+-", 3) => x + y - z,
        ("+/", 3) => (x + y) / z,
        ("?:", 3) => {
            if x > 0.0 {
                y
            } else {
                z
            }
        }
        // `pin x y z` clamps the MIDDLE operand into `[x, z]`.
        ("pin", 3) => {
            if y < x {
                x
            } else if y > z {
                z
            } else {
                y
            }
        }
        ("mod", 3) => (x * x + y * y + z * z).sqrt(),
        ("cat2", 3) => x * z.atan2(y).cos(),
        ("sat2", 3) => x * z.atan2(y).sin(),
        _ => return None,
    };
    value.is_finite().then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adj(name: &str, formula: &str) -> ShapeAdjustment {
        ShapeAdjustment {
            name: name.to_owned(),
            formula: formula.to_owned(),
        }
    }

    /// A 200 × 100 shape, so `w`, `h`, `ss` and `ls` are all distinguishable — a box
    /// with equal sides could not tell `ss` from `ls` or `w` from `h`.
    fn shape() -> GuideBox {
        GuideBox::new(200.0, 100.0)
    }

    fn eval(formula: &str) -> Option<f64> {
        evaluate(formula, shape(), &[])
    }

    #[test]
    fn literal_and_unary_opcodes() {
        assert_eq!(eval("val 16667"), Some(16_667.0));
        assert_eq!(eval("val -5"), Some(-5.0));
        assert_eq!(eval("abs -7"), Some(7.0));
        assert_eq!(eval("sqrt 16"), Some(4.0));
        assert_eq!(eval("sqrt -1"), None, "a negative root is refused, not NaN");
    }

    #[test]
    fn the_three_operand_arithmetic_opcodes() {
        assert_eq!(eval("*/ 10 3 2"), Some(15.0));
        assert_eq!(eval("+- 10 3 2"), Some(11.0));
        assert_eq!(eval("+/ 10 2 3"), Some(4.0));
        assert_eq!(eval("mod 3 4 0"), Some(5.0), "a 3-4-5 triangle");
    }

    #[test]
    fn division_by_zero_is_refused_rather_than_infinite() {
        // The caller keeps its documented default, which is a shape someone chose.
        assert_eq!(eval("*/ 10 3 0"), None);
        assert_eq!(eval("+/ 10 2 0"), None);
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
        let quarter = 5_400_000.0;
        let sin = eval(&format!("sin 100 {quarter}")).unwrap();
        assert!((sin - 100.0).abs() < 1e-9, "sin(90 deg) = 1, got {sin}");
        let cos = eval(&format!("cos 100 {quarter}")).unwrap();
        assert!(cos.abs() < 1e-9, "cos(90 deg) = 0, got {cos}");
        // `at2 x y` is arctan(y/x): equal operands are 45 degrees.
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
        // `cat2 x y z` = x * cos(arctan(z / y)). With y = z the angle is 45 degrees.
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
    }

    #[test]
    fn the_box_edges_are_the_shapes_own_space_not_the_page() {
        // ECMA-376's preset definitions are written against a shape-local box, so
        // `l`/`t` are zero however far down the page the shape sits. Resolving them
        // as page coordinates would scale every authored proportion by the offset.
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
    fn an_unreadable_formula_is_refused_so_the_caller_keeps_its_default() {
        assert_eq!(eval(""), None);
        assert_eq!(eval("nope 1 2"), None, "unknown opcode");
        assert_eq!(eval("val"), None, "missing operand");
        assert_eq!(eval("val 1 2"), None, "too many operands for val");
        assert_eq!(eval("*/ 1 2"), None, "too few operands for */");
        assert_eq!(eval("val 1 2 3 4"), None, "beyond three operands");
        assert_eq!(eval("val mystery"), None, "unknown name");
    }

    #[test]
    fn the_specifications_own_redundant_fourth_operand_is_dropped() {
        // ECMA-376's normative preset data writes eight guides with one operand more
        // than any opcode takes, always a trailing zero: `+- xH 0 dxB 0`, in
        // `circularArrow`, `leftCircularArrow` and `leftRightCircularArrow`. Strict
        // arity refused them, which refused those three presets entirely — and for a
        // reason that has nothing to do with the arcs in their paths, so it hid
        // behind the arc refusal until arcs started drawing.
        //
        // The reading is not a guess: the guide immediately above each one is
        // `+- xH dxG 0` with `dxG` and `dxB` the same formula, so the pair is the two
        // arrowhead corners either side of `xH`, and a trailing zero cannot change
        // `x + y - z` however it is grouped.
        assert_eq!(
            eval("+- 10 0 3 0"),
            Some(7.0),
            "the specification's own shape"
        );
        assert_eq!(eval("+- 10 3 0 0"), Some(13.0));
        // A fourth operand that is NOT zero could change the answer, so it still means
        // this is not a formula in the language — as does a fifth.
        assert_eq!(eval("+- 10 0 3 1"), None, "not padding");
        assert_eq!(eval("+- 10 0 3 0 0"), None, "a fifth operand");
        assert_eq!(
            eval("*/ 10 3 2 5"),
            None,
            "and the same for the other opcodes"
        );
    }

    #[test]
    fn a_guide_sees_the_guides_defined_before_it() {
        let guides = [
            adj("half", "*/ h 1 2"),
            adj("quarter", "*/ half 1 2"),
            adj("sum", "+- half quarter 0"),
        ];
        let shape = shape();
        assert_eq!(guide_value(&guides, "half", shape), Some(50.0));
        assert_eq!(
            guide_value(&guides, "quarter", shape),
            Some(25.0),
            "a later guide resolves an earlier one by name"
        );
        assert_eq!(guide_value(&guides, "sum", shape), Some(75.0));
    }

    #[test]
    fn a_forward_reference_does_not_resolve() {
        // Order matters: a guide may only see those defined before it, so naming a
        // later one is unreadable rather than silently zero.
        let guides = [adj("early", "*/ late 1 2"), adj("late", "val 80")];
        assert_eq!(guide_value(&guides, "early", shape()), None);
        assert_eq!(guide_value(&guides, "late", shape()), Some(80.0));
    }

    #[test]
    fn an_unreadable_guide_is_skipped_rather_than_zeroed() {
        // Zero is a legal guide value that would silently reshape the geometry; a
        // missing name lets the caller keep its documented default instead.
        let guides = [adj("adj", "bogus 1 2 3")];
        assert_eq!(guide_value(&guides, "adj", shape()), None);
    }

    #[test]
    fn a_long_guide_list_is_evaluated_to_its_end() {
        // There is deliberately no cap here (see the module note). A 32-guide cap used
        // to live in this function and silently refused nine ECMA-376 presets, `gear9`
        // declaring 244 guides. Authored input is bounded at import instead.
        let guides: Vec<ShapeAdjustment> =
            (0..300).map(|i| adj(&format!("g{i}"), "val 1")).collect();
        assert_eq!(guide_value(&guides, "g0", shape()), Some(1.0));
        assert_eq!(
            guide_value(&guides, "g299", shape()),
            Some(1.0),
            "the last guide of a long list still evaluates"
        );
    }
}
