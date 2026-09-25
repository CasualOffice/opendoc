//! The backend-neutral display list — the single stable seam between layout and
//! rendering.
//!
//! Layout produces a [`DisplayList`] of ordered [`PaintItem`]s; a
//! `casual-doc-render` backend (CPU raster, WASM canvas, or GPU) executes it. No
//! backend type appears here, and the list is serializable so it can be golden-
//! tested and, later, shipped across a boundary. Coordinates are in device
//! pixels (the device scale has already been applied when the list was built).

use casual_doc_model::v1::{CropRect, DashStyle, LineEnd};
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

/// A shape fill: a flat color (`a:solidFill`) or a gradient (`a:gradFill`).
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Fill {
    /// A single flat color.
    Solid(Color),
    /// A multi-stop gradient.
    Gradient(Gradient),
}

/// A gradient fill: ordered stops plus the sweep geometry.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Gradient {
    /// The stops in paint order (at least one; a well-formed gradient has two).
    pub stops: Vec<GradientStop>,
    /// The gradient geometry (linear sweep or radial).
    pub kind: GradientKind,
}

/// One gradient stop: a position along the gradient axis and the color there.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct GradientStop {
    /// The stop position in `0.0..=1.0` (start..end).
    pub position: f32,
    /// The resolved stop color.
    pub color: Color,
}

/// The geometry of a gradient fill.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum GradientKind {
    /// A linear sweep at `angle_deg` clockwise from the positive x-axis.
    Linear {
        /// The sweep angle in degrees, clockwise from +x.
        angle_deg: f32,
    },
    /// A radial/concentric gradient centered on the shape.
    Radial,
}

/// The outline of a floating DrawingML shape: a resolved color, a device-pixel
/// width, and a preset dash pattern (`a:ln > a:prstDash`).
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct ShapeOutline {
    /// The outline color.
    pub color: Color,
    /// The outline width in device pixels.
    pub width: f32,
    /// The preset dash pattern (`DashStyle::Solid` = an unbroken line).
    pub dash: DashStyle,
}

/// The default for a serialized [`ShapeGeometry::Polygon`] that predates the
/// `closed` field: every polygon that could be written then was closed.
fn closed_polygon() -> bool {
    true
}

/// The geometry primitive of a painted [`PaintItem::Shape`].
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ShapeGeometry {
    /// A rectangle fitted to `rect`.
    Rect {
        /// The rectangle in device-pixel-scaled twips.
        rect: Rect,
    },
    /// An ellipse fitted to `rect`.
    Ellipse {
        /// The ellipse bounding rectangle.
        rect: Rect,
    },
    /// A rounded rectangle fitted to `rect`.
    RoundedRect {
        /// The bounding rectangle.
        rect: Rect,
        /// The corner radius in twips.
        radius: Twip,
    },
    /// A polyline in path order, closed (a polygon) or open.
    Polygon {
        /// The vertices in path order.
        points: Vec<Point>,
        /// Whether the last vertex joins back to the first. `false` strokes an
        /// open path — an unclosed `a:custGeom` (docs/119). Defaults to `true`
        /// so a display list serialized before this field still deserializes as
        /// the closed polygon it was.
        #[serde(default = "closed_polygon")]
        closed: bool,
    },
    /// A straight line / connector.
    Line {
        /// The start point.
        from: Point,
        /// The end point.
        to: Point,
    },
}

/// An affine transform applied to a [`PaintItem::Shape`] or [`PaintItem::Image`]
/// about the object's own center: a clockwise rotation and/or axis flips
/// (`a:xfrm@rot` / `@flipH` / `@flipV`).
///
/// Stored as integers and booleans so the display list stays deterministic; the
/// backend derives the floating-point matrix (rotation about the center, flips)
/// at paint time. `None` on a paint item means the identity (unrotated,
/// unflipped) transform.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ShapeTransform {
    /// Clockwise rotation in 60000ths of a degree (`a:xfrm@rot`).
    #[serde(default, skip_serializing_if = "is_zero_rotation")]
    pub rotation: i32,
    /// Horizontal flip about the center (`a:xfrm@flipH`).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub flip_h: bool,
    /// Vertical flip about the center (`a:xfrm@flipV`).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub flip_v: bool,
    /// The center of rotation/flip — the object rect's center, in the same
    /// device-scaled twips as the rest of the list.
    pub center: Point,
}

fn is_zero_rotation(rotation: &i32) -> bool {
    *rotation == 0
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
        /// The rotation/flip applied about the image's center (`a:xfrm`), if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        transform: Option<ShapeTransform>,
        /// The picture's opacity (`a:alphaModFix`), in 1000ths of a percent.
        /// `None` is fully opaque — which is what an absent `a:alphaModFix`
        /// means, and how Word writes every picture that is not a watermark.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        opacity: Option<u32>,
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
    /// A floating DrawingML shape (`wps:wsp`/`wps:cxnSp`): a geometry primitive
    /// with a gradient-or-solid fill, a dashable outline, and — for a line /
    /// connector — optional start/end arrowheads. Kept distinct from the flat
    /// [`PaintItem::Rect`]/[`PaintItem::Ellipse`]/… (used for shading, borders,
    /// and table furniture) so those stay a simple solid-color seam.
    Shape {
        /// The geometry to fill/stroke.
        geometry: ShapeGeometry,
        /// The fill (solid or gradient), if filled.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fill: Option<Fill>,
        /// The outline (color + width + dash), if stroked.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stroke: Option<ShapeOutline>,
        /// The start (`a:headEnd`) arrowhead — only meaningful for a line.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        head_end: Option<LineEnd>,
        /// The end (`a:tailEnd`) arrowhead — only meaningful for a line.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tail_end: Option<LineEnd>,
        /// The rotation/flip applied about the shape's center (`a:xfrm`), if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        transform: Option<ShapeTransform>,
    },
    /// Push a clip rectangle; subsequent items are clipped until [`PaintItem::PopClip`].
    PushClip(Rect),
    /// Pop the most recent clip.
    PopClip,
    /// Push a LAYER: subsequent items are composited as one group, optionally
    /// through an affine transform and a non-normal blend, until
    /// [`PaintItem::PopLayer`]. Nests, composing outermost first, exactly like
    /// [`PaintItem::PushClip`].
    ///
    /// This is how ROTATED TEXT is expressed. A glyph run carries no angle of its
    /// own, and giving it one would have meant a rotation field on the one paint
    /// item that is constructed in a dozen places, for the sake of the single
    /// object that needs it. A bracket around a group also matches what the thing
    /// actually is: a watermark is one rotated object whose parts — the words, or
    /// the picture — are neither individually angled nor individually blended.
    ///
    /// The BLEND is what lets a watermark sit on top of the page without hiding
    /// anything. A normal-blended stamp has to choose: behind the content, where an
    /// opaque table fill erases it, or in front, where it dims the text. Multiplied,
    /// it does not have to choose — see [`LayerBlend::Multiply`].
    ///
    /// Introduced for the watermark (`109` OO-006). `docs/105` FID-L-08 (vertical
    /// and rotated text) is the other caller this seam is waiting for; it is not
    /// implemented by this existing, and no flow content emits a layer yet.
    PushLayer {
        /// Rotation/flip about a centre point, or `None` for an upright layer.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        transform: Option<ShapeTransform>,
        /// How the layer composites onto what is already painted.
        #[serde(default)]
        blend: LayerBlend,
    },
    /// Pop the most recent layer.
    PopLayer,
}

/// How a [`PaintItem::PushLayer`] group composites onto the page beneath it.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LayerBlend {
    /// Ordinary source-over: the layer covers what is beneath it.
    #[default]
    Normal,
    /// Multiply the layer with what is beneath it.
    ///
    /// This is the compositing model of ink on paper, and it is what makes a
    /// watermark behave like one. Multiplying by a value never brightens, so:
    ///
    /// - over white paper, a light grey stamp shows as light grey;
    /// - over an opaque table fill or a picture, it shows as a darkening of that
    ///   fill — it is not erased by it, which normal blending underneath cannot
    ///   achieve at all;
    /// - over black text, `0 x anything = 0`, so **the text is returned
    ///   unchanged**. A multiplied stamp painted on top of the page provably
    ///   cannot make a word harder to read, which is the whole reason it may be
    ///   painted on top.
    Multiply,
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
