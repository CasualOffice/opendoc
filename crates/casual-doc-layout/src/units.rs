//! Device-independent geometry.
//!
//! Layout computes entirely in [`Twip`]s (1/1440 inch, the DOCX unit) so that a
//! document paginates identically regardless of the output device. The device
//! scale (DPI × zoom) is applied only when a [`crate::display`] list is built or
//! painted — never during layout — which is what keeps pagination deterministic
//! across native, WASM, and print (`00-README.md` determinism constraint).
//!
//! # EMU, and the two rounding rules
//!
//! DrawingML authors geometry in EMU (1/914400 inch), so an authored coordinate
//! crosses into layout through one of the converters below. They are collected
//! here because the same conversion previously existed as three private copies in
//! `flow`, `anchor`, and `document_layout`, which is how two different rounding
//! rules came to be in use at once.
//!
//! [`emu_to_twip_extent`] and [`emu_to_twip_offset`] **truncate** toward zero;
//! [`emu_to_twip_rounded`] **rounds** to nearest. That divergence is preserved
//! deliberately so that collecting the copies here changed no geometry, and it is
//! a real defect while it lasts, in two ways that are worth stating precisely
//! because a plausible-sounding third way is false.
//!
//! 1. **The two rules disagree by up to a whole twip.** 1000 EMU truncates to 1
//!    twip and rounds to 2. A group child, whose rect comes from the rounding
//!    path, and a directly anchored sibling authored at the same EMU therefore
//!    land a twip apart.
//! 2. **Truncation shrinks magnitudes, and an edge is a sum of two of them.** An
//!    offset and an extent each lose up to a twip toward zero, so a right or
//!    bottom edge computed as `offset + extent` can fall up to two twips short of
//!    the authored edge, and shapes authored to abut can show a seam. Rounding
//!    errs by at most half a twip in either direction.
//!
//! What is **not** wrong: truncation toward zero is mirror-symmetric
//! (`±1000 EMU → ±1 twip`), so a shape and its mirror land symmetrically. Only
//! floor division would break that, and Rust integer division does not floor.
//!
//! Converging on one rule moves committed goldens and is therefore a separate,
//! deliberate change (`156` §6 row 0.6).

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

/// Truncating EMU → twips for a non-negative extent, clamped to `[0, i32::MAX]`.
///
/// Truncates toward zero. See the module's rounding note for why this differs
/// from [`emu_to_twip_rounded`].
#[must_use]
pub fn emu_to_twip_extent(emu: i64) -> Twip {
    Twip((emu / EMU_PER_TWIP).clamp(0, i64::from(i32::MAX)) as i32)
}

/// Truncating EMU → twips for a signed offset, which may be negative because a
/// float can overhang its reference edge.
///
/// Truncates toward zero, so a negative offset is biased toward the origin. See
/// the module's rounding note.
#[must_use]
pub fn emu_to_twip_offset(emu: i64) -> Twip {
    Twip((emu / EMU_PER_TWIP).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
}

/// Rounding EMU → twips for a value produced by a group affine, which is
/// evaluated in `f64` because `a:ext`/`a:chExt` scaling is not integral.
///
/// Rounds to nearest, unlike [`emu_to_twip_extent`] and [`emu_to_twip_offset`].
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
    fn emu_to_twip_extent_truncates_and_clamps_a_negative_to_zero() {
        assert_eq!(emu_to_twip_extent(635), Twip(1)); // exactly one twip
        assert_eq!(emu_to_twip_extent(1000), Twip(1)); // 1.5748 truncates down
        assert_eq!(emu_to_twip_extent(634), Twip(0)); // under a twip vanishes
        assert_eq!(emu_to_twip_extent(-1000), Twip(0)); // an extent is non-negative
    }

    #[test]
    fn emu_to_twip_offset_truncates_toward_zero_and_is_mirror_symmetric() {
        assert_eq!(emu_to_twip_offset(1000), Twip(1));
        assert_eq!(emu_to_twip_offset(-1000), Twip(-1));
        // The property the module note calls out as NOT broken: truncation toward
        // zero sends a value and its negation to mirrored twips. Floor division
        // would break this; Rust integer division does not floor.
        for emu in [1_i64, 634, 635, 953, 1000, 91_440] {
            assert_eq!(
                emu_to_twip_offset(-emu),
                Twip(-emu_to_twip_offset(emu).raw()),
                "asymmetry at {emu} EMU"
            );
        }
    }

    #[test]
    fn emu_to_twip_rounded_rounds_to_nearest() {
        assert_eq!(emu_to_twip_rounded(635.0), Twip(1));
        assert_eq!(emu_to_twip_rounded(1000.0), Twip(2)); // 1.5748 rounds up
        assert_eq!(emu_to_twip_rounded(952.0), Twip(1)); // 1.4992 rounds down
        assert_eq!(emu_to_twip_rounded(-1000.0), Twip(-2));
    }

    #[test]
    fn the_two_rounding_rules_still_disagree_by_a_whole_twip() {
        // This PINS A KNOWN DEFECT (`156` §6 row 0.6): the group-affine path
        // rounds while the direct path truncates, so one authored EMU reaches
        // layout as two different twip values depending on which path it took.
        //
        // If this test fails, the rules have been converged. That is the intended
        // fix and not a regression — but it is a geometry change, so retiring this
        // test means reviewing the committed goldens that move with it.
        assert_eq!(emu_to_twip_offset(1000), Twip(1));
        assert_eq!(emu_to_twip_rounded(1000.0), Twip(2));
        assert_ne!(emu_to_twip_offset(1000), emu_to_twip_rounded(1000.0));
    }

    #[test]
    fn summing_truncated_terms_loses_a_twip_against_the_authored_edge() {
        // The module note's second defect. An edge is `offset + extent`, and each
        // term truncates toward zero independently, so the computed edge can fall
        // short of the edge the author wrote. This is why shapes meant to abut can
        // show a seam.
        let offset_emu = 1000_i64; // 1.5748 twips
        let extent_emu = 1000_i64; // 1.5748 twips
        let computed = emu_to_twip_offset(offset_emu).raw() + emu_to_twip_extent(extent_emu).raw();
        let authored = (offset_emu + extent_emu) / EMU_PER_TWIP; // 3.1496 -> 3
        assert_eq!(authored, 3);
        assert_eq!(
            computed, 2,
            "summing truncated terms should lose a twip here"
        );
    }

    #[test]
    fn twip_arithmetic() {
        assert_eq!(Twip(30) + Twip(12), Twip(42));
        assert_eq!(Twip(30) - Twip(42), Twip(-12));
        assert_eq!(Twip::from_points(1), Twip(20));
    }
}
