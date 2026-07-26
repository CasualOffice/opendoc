//! Device-independent geometry.
//!
//! Layout computes entirely in [`Twip`]s (1/1440 inch, the DOCX unit) so that a
//! document paginates identically regardless of the output device. The device
//! scale (DPI × zoom) is applied only when a [`crate::display`] list is built or
//! painted — never during layout — which is what keeps pagination deterministic
//! across native, WASM, and print (`00-README.md` determinism constraint).

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
    fn twip_arithmetic() {
        assert_eq!(Twip(30) + Twip(12), Twip(42));
        assert_eq!(Twip(30) - Twip(42), Twip(-12));
        assert_eq!(Twip::from_points(1), Twip(20));
    }
}
