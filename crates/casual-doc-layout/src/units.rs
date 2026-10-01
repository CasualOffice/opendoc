//! Device-independent geometry.
//!
//! Layout computes entirely in [`Twip`]s (1/1440 inch, the DOCX unit) so that a
//! document paginates identically regardless of the output device. The device
//! scale (DPI × zoom) is applied only when a [`crate::display`] list is built or
//! painted — never during layout — which is what keeps pagination deterministic
//! across native, WASM, and print (`00-README.md` determinism constraint).
//!
//! # EMU, and the one rounding rule
//!
//! DrawingML authors geometry in EMU (1/914400 inch), so an authored coordinate
//! crosses into layout through one of the converters below. They are collected
//! here because the same conversion previously existed as three private copies in
//! `flow`, `anchor`, and `document_layout`, which is how two different rounding
//! rules came to be in use at once without anyone choosing between them: the
//! integer copies truncated toward zero, while the one the group affine used
//! rounded.
//!
//! **They now all round half away from zero**, which is the accurate choice and
//! not a preference. EMU is exact and twips are our approximation, so the only
//! goal is to minimise error against the authored coordinate: rounding errs by at
//! most half a twip, truncation by up to a whole one. Nothing in ECMA-376 asks for
//! truncation — it was an artifact of integer division.
//!
//! Two defects went with it. The rules disagreed by up to a whole twip (1000 EMU
//! truncated to 1 and rounded to 2), so a group child and a directly anchored
//! sibling authored at the same EMU landed a twip apart. And because an edge is
//! `offset + extent` and each term lost up to a twip *toward zero*, a computed
//! right or bottom edge could fall short of the authored one, so shapes authored
//! to abut could seam.
//!
//! One thing was never wrong and is asserted so it stays that way: truncation
//! toward zero was already mirror-symmetric (`±1000 EMU → ±1 twip`), because Rust
//! integer division truncates rather than floors. Rounding half **away from zero**
//! preserves that symmetry; rounding half-up or half-to-even would not.
//!
//! The integer converters round in integer arithmetic, via
//! `div_round_away_from_zero`, rather than by going through `f64`. Not for
//! precision: `MAX_EMU` is 2.7e13 and `f64` represents integers exactly to 9.0e15,
//! so a float round-trip would in fact be exact across the whole permitted EMU
//! range. The reason is narrower — an integer input deserves integer arithmetic, it
//! removes a float round-trip from a path that runs per shape, and it makes the
//! rounding rule readable at the point of definition instead of implied by
//! `f64::round`'s convention.

use serde::{Deserialize, Serialize};

/// Twips per inch (1 twip = 1/1440 in). A point is 20 twips; a pixel at 96 dpi
/// is 15 twips.
pub const TWIPS_PER_INCH: i32 = 1_440;

/// A length in twips (1/1440 inch). Signed so offsets and overflow can be
/// negative; layout sizes are expected to be non-negative.
#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Deserialize, Serialize,
)]
#[serde(transparent)]
pub struct Twip(pub i32);

impl Twip {
    /// Zero length.
    pub const ZERO: Self = Self(0);

    /// A length from a whole number of points (1 pt = 20 twips).
    #[must_use]
    pub const fn from_points(points: i32) -> Self {
        Self(points * 20)
    }

    /// The raw twip count.
    #[must_use]
    pub const fn raw(self) -> i32 {
        self.0
    }

    /// Whether this length is exactly zero (used to skip serializing default
    /// paint metadata).
    #[must_use]
    pub const fn is_zero(&self) -> bool {
        self.0 == 0
    }

    /// Converts to device pixels at the given scale (device pixels per inch,
    /// i.e. dpi × zoom). Rounded to the nearest pixel.
    #[must_use]
    pub fn to_device_px(self, dpi: f32) -> f32 {
        (self.0 as f32) * dpi / (TWIPS_PER_INCH as f32)
    }
}

impl core::ops::Add for Twip {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl core::ops::Sub for Twip {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

/// A point in twip space. `y` grows downward (screen convention).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct Point {
    /// Horizontal offset.
    pub x: Twip,
    /// Vertical offset (downward-positive).
    pub y: Twip,
}

impl Point {
    /// A point from raw twip coordinates.
    #[must_use]
    pub const fn new(x: Twip, y: Twip) -> Self {
        Self { x, y }
    }
}

/// A size in twip space; width and height are expected non-negative.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct Size {
    /// Width.
    pub width: Twip,
    /// Height.
    pub height: Twip,
}

impl Size {
    /// A size from raw twip dimensions.
    #[must_use]
    pub const fn new(width: Twip, height: Twip) -> Self {
        Self { width, height }
    }
}

/// An axis-aligned rectangle in twip space (origin = top-left).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct Rect {
    /// Top-left corner.
    pub origin: Point,
    /// Extent.
    pub size: Size,
}

impl Rect {
    /// A rectangle from an origin and a size.
    #[must_use]
    pub const fn new(origin: Point, size: Size) -> Self {
        Self { origin, size }
    }

    /// The x-coordinate of the right edge.
    #[must_use]
    pub fn right(&self) -> Twip {
        self.origin.x + self.size.width
    }

    /// The y-coordinate of the bottom edge.
    #[must_use]
    pub fn bottom(&self) -> Twip {
        self.origin.y + self.size.height
    }

    /// Whether the point lies within the rectangle (inclusive of the top-left,
    /// exclusive of the bottom-right edge).
    #[must_use]
    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.origin.x
            && point.x < self.right()
            && point.y >= self.origin.y
            && point.y < self.bottom()
    }
}

/// EMU per twip: 914400 EMU/inch ÷ 1440 twips/inch = 635 exactly. The constant
/// divides without residue; only the value being converted can have one.
pub const EMU_PER_TWIP: i64 = 635;

/// Integer division rounding half **away from zero**, exactly.
///
/// `away from zero` is what keeps the conversion mirror-symmetric: a coordinate and
/// its negation map to mirrored twips, which `f64::round` also does but half-up and
/// half-to-even do not. `divisor` must be positive.
fn div_round_away_from_zero(dividend: i64, divisor: i64) -> i64 {
    debug_assert!(divisor > 0, "divisor must be positive");
    let quotient = dividend / divisor;
    let remainder = dividend % divisor;
    if remainder.unsigned_abs() * 2 >= divisor.unsigned_abs() {
        if dividend < 0 {
            quotient - 1
        } else {
            quotient + 1
        }
    } else {
        quotient
    }
}

/// EMU → twips for a non-negative extent, clamped to `[0, i32::MAX]`.
///
/// Rounds half away from zero, exactly — see the module's rounding note.
#[must_use]
pub fn emu_to_twip_extent(emu: i64) -> Twip {
    Twip(div_round_away_from_zero(emu, EMU_PER_TWIP).clamp(0, i64::from(i32::MAX)) as i32)
}

/// EMU → twips for a signed offset, which may be negative because a float can
/// overhang its reference edge.
///
/// Rounds half away from zero, exactly, so it agrees with
/// [`emu_to_twip_rounded`] on every integral input.
#[must_use]
pub fn emu_to_twip_offset(emu: i64) -> Twip {
    Twip(
        div_round_away_from_zero(emu, EMU_PER_TWIP).clamp(i64::from(i32::MIN), i64::from(i32::MAX))
            as i32,
    )
}

/// Rounds a twip value computed in `f64` (a group affine's output) to a whole
/// twip, half away from zero, clamped to the twip range.
///
/// The same rounding rule as the EMU converters above, for the case where the
/// value is already in twips and only the quantisation is left.
#[must_use]
pub fn twip_rounded(twips: f64) -> Twip {
    Twip(
        twips
            .round()
            .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32,
    )
}

/// Rounding EMU → twips for a value produced by a group affine, which is
/// evaluated in `f64` because `a:ext`/`a:chExt` scaling is not integral.
///
/// Rounds half away from zero, agreeing with [`emu_to_twip_extent`] and
/// [`emu_to_twip_offset`] on every integral input.
#[must_use]
pub fn emu_to_twip_rounded(emu: f64) -> Twip {
    Twip(
        (emu / EMU_PER_TWIP as f64)
            .round()
            .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_conversion_uses_the_device_scale() {
        // One inch = 1440 twips; at 96 dpi that is 96 device pixels.
        assert_eq!(Twip(TWIPS_PER_INCH).to_device_px(96.0), 96.0);
        // Zoomed 2x (192 effective dpi) doubles it.
        assert_eq!(Twip(TWIPS_PER_INCH).to_device_px(192.0), 192.0);
        // A point is 20 twips = 1/72 inch.
        assert!((Twip::from_points(72).to_device_px(96.0) - 96.0).abs() < 1e-3);
    }

    #[test]
    fn rect_contains_is_half_open() {
        let rect = Rect::new(
            Point::new(Twip(100), Twip(200)),
            Size::new(Twip(50), Twip(60)),
        );
        assert!(rect.contains(Point::new(Twip(100), Twip(200)))); // top-left inclusive
        assert!(rect.contains(Point::new(Twip(149), Twip(259))));
        assert!(!rect.contains(Point::new(Twip(150), Twip(260)))); // bottom-right exclusive
        assert!(!rect.contains(Point::new(Twip(99), Twip(200))));
    }

    #[test]
    fn emu_to_twip_extent_rounds_and_clamps_a_negative_to_zero() {
        assert_eq!(emu_to_twip_extent(635), Twip(1)); // exactly one twip
        assert_eq!(emu_to_twip_extent(1000), Twip(2)); // 1.5748 rounds up
        assert_eq!(emu_to_twip_extent(634), Twip(1)); // 0.9984 rounds up, no longer vanishes
        assert_eq!(emu_to_twip_extent(-1000), Twip(0)); // an extent is non-negative
    }

    #[test]
    fn emu_to_twip_offset_rounds_and_is_mirror_symmetric() {
        assert_eq!(emu_to_twip_offset(1000), Twip(2));
        assert_eq!(emu_to_twip_offset(-1000), Twip(-2));
        // Rounding half AWAY FROM ZERO is what preserves this. Half-up or
        // half-to-even would send a value and its negation to unmirrored twips.
        for emu in [1_i64, 317, 318, 634, 635, 953, 1000, 91_440, 914_400] {
            assert_eq!(
                emu_to_twip_offset(-emu),
                Twip(-emu_to_twip_offset(emu).raw()),
                "asymmetry at {emu} EMU"
            );
        }
    }

    #[test]
    fn rounding_lands_exactly_on_the_half_twip_boundary() {
        // Half a twip is 317.5 EMU, which is not an integer, so the boundary is
        // between 317 and 318 and no input is ever an exact tie.
        assert_eq!(emu_to_twip_extent(317), Twip(0)); // 0.4992
        assert_eq!(emu_to_twip_extent(318), Twip(1)); // 0.5008
        assert_eq!(emu_to_twip_extent(952), Twip(1)); // 1.4992
        assert_eq!(emu_to_twip_extent(953), Twip(2)); // 1.5008
    }

    #[test]
    fn the_integer_and_float_converters_now_agree_on_every_integral_input() {
        // This is the convergence guard, and it replaces one that asserted the two
        // rules DISAGREED. The group affine evaluates in f64 and reaches layout via
        // `emu_to_twip_rounded`; a directly anchored sibling arrives through
        // `emu_to_twip_offset`. They must not quantise one authored EMU differently.
        for emu in [
            -914_400_i64,
            -1000,
            -635,
            -318,
            -317,
            0,
            317,
            318,
            635,
            1000,
            91_440,
            914_400,
            12_192_000,
        ] {
            assert_eq!(
                emu_to_twip_offset(emu),
                emu_to_twip_rounded(emu as f64),
                "integer and float paths disagree at {emu} EMU"
            );
        }
    }

    #[test]
    fn independently_rounded_terms_can_still_miss_the_authored_edge() {
        // Converging the rules did NOT fix this, and claiming otherwise would be
        // wrong: an edge is `offset + extent`, each term is quantised on its own, so
        // the computed edge can still differ from quantising the authored edge once.
        // Rounding only halves the per-term error; the only real fix is to compute
        // edges in EMU and convert once, which is a wider change than row 0.6.
        let offset_emu = 1000_i64; // 1.5748 twips -> 2
        let extent_emu = 1000_i64; // 1.5748 twips -> 2
        let computed = emu_to_twip_offset(offset_emu).raw() + emu_to_twip_extent(extent_emu).raw();
        let authored = emu_to_twip_offset(offset_emu + extent_emu).raw(); // 3.1496 -> 3
        assert_eq!(computed, 4);
        assert_eq!(authored, 3);
        assert_ne!(
            computed, authored,
            "double quantisation is still observable"
        );
    }

    #[test]
    fn twip_arithmetic() {
        assert_eq!(Twip(30) + Twip(12), Twip(42));
        assert_eq!(Twip(30) - Twip(42), Twip(-12));
        assert_eq!(Twip::from_points(1), Twip(20));
    }
}
