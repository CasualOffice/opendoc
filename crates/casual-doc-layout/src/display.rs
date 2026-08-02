//! The backend-neutral display list — the single stable seam between layout and
//! rendering.
//!
//! Layout produces a [`DisplayList`] of ordered [`PaintItem`]s; a
//! `casual-doc-render` backend (CPU raster, WASM canvas, or GPU) executes it. No
//! backend type appears here, and the list is serializable so it can be golden-
//! tested and, later, shipped across a boundary. Coordinates are in device
//! pixels (the device scale has already been applied when the list was built).

use casual_doc_model::v1::CropRect;
use serde::{Deserialize, Serialize};

use crate::text::GlyphRun;
use crate::units::{Point, Rect, Twip};

/// An 8-bit-per-channel straight-alpha sRGB color.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct Color {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
    /// Alpha channel (255 = opaque).
    pub a: u8,
}

impl Color {
    /// Opaque black.
    pub const BLACK: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    /// Opaque white.
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };

    /// An opaque color from RGB channels.
    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
}

/// A stroke style for outlined shapes.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct Stroke {
    /// Stroke color.
    pub color: Color,
    /// Stroke width in device pixels.
    pub width: f32,
}

/// One paint command. Items are painted in list order (painter's algorithm);
/// clips nest via [`PaintItem::PushClip`]/[`PaintItem::PopClip`].
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum PaintItem {
    /// A positioned run of glyphs (already shaped and placed by layout).
    Glyphs {
        /// The glyph run, in device pixels.
        run: GlyphRun,
    },
    /// A filled and/or stroked rectangle (borders, shading, table lines, caret,
    /// selection highlight).
    Rect {
        /// The rectangle, in device pixels.
        rect: Rect,
        /// Fill color, if filled.
        fill: Option<Color>,
        /// Stroke, if outlined.
        stroke: Option<Stroke>,
    },
    /// A filled and/or stroked ellipse fitted to `rect`.
    Ellipse {
        /// Ellipse bounding rectangle.
        rect: Rect,
        /// Fill color, if filled.
        fill: Option<Color>,
        /// Stroke, if outlined.
        stroke: Option<Stroke>,
    },
    /// A filled and/or stroked rounded rectangle.
    RoundedRect {
        /// Shape bounding rectangle.
        rect: Rect,
        /// Corner radius in twips.
        radius: Twip,
        /// Fill color, if filled.
        fill: Option<Color>,
        /// Stroke, if outlined.
        stroke: Option<Stroke>,
    },
    /// A filled and/or stroked closed polygon.
    Polygon {
        /// Vertices in path order, in page-local twips.
        points: Vec<Point>,
        /// Fill color, if filled.
        fill: Option<Color>,
        /// Stroke, if outlined.
        stroke: Option<Stroke>,
    },
    /// An image blit (a `Definitions.media` reference), placed in `rect`. The
    /// backend resolves the bytes; layout carries only the reference and box.
    Image {
        /// The media reference id (stringly to avoid a model dependency cycle
        /// here; resolved by the caller against `Definitions.media`).
        media: String,
        /// Destination rectangle, in device pixels.
        rect: Rect,
        /// The source-rectangle crop (`a:srcRect`), if the picture is cropped: the
        /// backend samples only the visible source sub-rectangle and scales it to
        /// fill `rect`. `None` = the whole source fills `rect` (`P1G-OBJ-MODEL`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        crop: Option<CropRect>,
    },
    /// A straight line / connector between two points (a floating DrawingML line
    /// shape or `wps:cxnSp` straight connector).
    Line {
        /// The line's start point, in device pixels.
        from: Point,
        /// The line's end point, in device pixels.
        to: Point,
        /// The line's stroke.
        stroke: Stroke,
    },
    /// Push a clip rectangle; subsequent items are clipped until [`PaintItem::PopClip`].
    PushClip(Rect),
    /// Pop the most recent clip.
    PopClip,
}

/// An ordered list of paint commands for one page (or one damage region during
/// incremental repaint). Executed by a rendering backend.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct DisplayList {
    /// The paint commands, in back-to-front order.
    pub items: Vec<PaintItem>,
}

impl DisplayList {
    /// An empty display list.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a paint command.
    pub fn push(&mut self, item: PaintItem) {
        self.items.push(item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_constructors() {
        assert_eq!(
            Color::rgb(1, 2, 3),
            Color {
                r: 1,
                g: 2,
                b: 3,
                a: 255
            }
        );
        assert_eq!(Color::BLACK.a, 255);
    }

    #[test]
    fn display_list_round_trips_through_json() {
        // The list is serializable so it can be golden-tested.
        let mut list = DisplayList::new();
        list.push(PaintItem::Rect {
            rect: Rect::default(),
            fill: Some(Color::WHITE),
            stroke: None,
        });
        let json = serde_json::to_string(&list).unwrap();
        let back: DisplayList = serde_json::from_str(&json).unwrap();
        assert_eq!(back.items.len(), 1);
    }
}
