//! The backend-neutral display list — the single stable seam between layout and
//! rendering.
//!
//! Layout produces a [`DisplayList`] of ordered [`PaintItem`]s; a
//! `casual-doc-render` backend (CPU raster, WASM canvas, or GPU) executes it. No
//! backend type appears here, and the list is serializable so it can be golden-
//! tested and, later, shipped across a boundary. Coordinates are in device
//! pixels (the device scale has already been applied when the list was built).

use casual_doc_model::v1::{CropRect, DashStyle, LineEnd};
// Own `use` line (anti-conflict), matching the convention in `anchor.rs`: the
// outline geometry `a:ln` carries beyond colour, width and a preset dash.
use casual_doc_model::v1::{DashStop, LineCap, LineJoin};
// Own `use` line (anti-conflict): `a:ln@cmpd`, the multi-line outline form.
use casual_doc_model::v1::CompoundLine;
// Own `use` line (anti-conflict): a path gradient's family is DrawingML's own
// `a:path@path` token, so the model's enum is reused rather than mirrored.
use casual_doc_model::v1::GradientPath;
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
    ///
    /// This is the gradient a path gradient collapses to when the file states no
    /// `a:path@path` token at all — it is NOT what `a:path path="circle"` means, and
    /// the two were the same value until [`GradientKind::Path`] existed.
    Radial,
    /// A **path gradient** (`a:path`): the stops run outward from a focus rectangle
    /// inside the shape to the shape's own bounding box.
    ///
    /// Not a linear gradient and not a plain radial one. DrawingML has two gradient
    /// families and this is the second: `a:lin` sweeps along an axis, while `a:path`
    /// expands from `a:fillToRect` along contours whose *shape* is named by
    /// `a:path@path` — circles, rectangles, or the shape's own outline.
    Path {
        /// Which contour family the stops follow (`a:path@path`).
        path: GradientPath,
        /// Where the FIRST stop sits (`a:path/a:fillToRect`), as fractions of the
        /// shape's bounding box.
        focus: GradientFocus,
    },
}

/// Where a path gradient's first stop sits inside the shape's bounding box:
/// `a:path/a:fillToRect`, resolved from `ST_Percentage` to fractions of the box.
///
/// Each field is an **inset from its own edge**, so the focus rectangle spans
/// `left ..= 1.0 - right` horizontally and `top ..= 1.0 - bottom` vertically. The
/// common authored value is `0.5` on all four edges — a focus *point* at the box
/// centre — and an edge may be negative, which places the focus outside the box
/// (`RelativeRect` keeps negatives for exactly that reason).
///
/// Fractions rather than `RelativeRect`'s per-100000 integers because a backend
/// should not have to know DrawingML's percentage unit, which is the same reason
/// `AnchorShadow` resolves `a:outerShdw`'s polar offset in layout.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct GradientFocus {
    /// Inset from the box's left edge, as a fraction of its width.
    pub left: f32,
    /// Inset from the box's top edge, as a fraction of its height.
    pub top: f32,
    /// Inset from the box's right edge, as a fraction of its width.
    pub right: f32,
    /// Inset from the box's bottom edge, as a fraction of its height.
    pub bottom: f32,
}

impl GradientFocus {
    /// The focus point at the centre of the box — all four edges inset by half.
    ///
    /// This is what an **absent** `a:fillToRect` resolves to, and the choice is a
    /// decision rather than a default. ECMA-376's own default is the identity rect
    /// (every edge zero), which makes the focus the whole bounding box and the
    /// gradient therefore zero-extent: the first stop's colour, flat, across the
    /// shape. Painting that is the "flat colour that looks deliberate" outcome
    /// `docs/119` §6 rejects for an unsupported construct, and it is also what the
    /// collapse this replaces already did. Every producer that writes `a:path` writes
    /// an explicit `a:fillToRect` with it, so the degenerate reading is unreachable
    /// in practice; centring keeps a path gradient looking like a gradient and
    /// matches what `GradientKind::Radial` drew for the same markup before.
    pub const CENTER: Self = Self {
        left: 0.5,
        top: 0.5,
        right: 0.5,
        bottom: 0.5,
    };
}

/// The outline of a floating DrawingML shape: a resolved color, a device-pixel
/// width, a dash pattern, and the end/corner geometry `a:ln` carries.
///
/// # Why cap, join and the authored dash are here and not defaulted
///
/// They were defaulted away: the raster backend built its stroke as
/// `Stroke { width, dash, ..default() }`, so every outline drew with a butt cap and a
/// miter join whatever the file said. A round-capped dotted border drew as square
/// dots — close enough to look intentional, which is what made it survive.
///
/// `custom_dash` outranks `dash` when non-empty, for the same reason a custom
/// geometry outranks a preset: `a:custDash` IS the pattern the author stated, and
/// `a:prstDash` is only present when they picked from the gallery.
///
/// # What `a:ln` carries that is deliberately absent
///
/// `@algn` (`ST_PenAlignment`). `algn="in"` puts the pen wholly inside the outline,
/// which moves the *path* inward by half the line width — a different geometry, not a
/// stroke parameter, and one no rasterizer here can produce because neither
/// `tiny-skia` nor PDF offers a path offset. Faking it by narrowing the stroke, or by
/// insetting only the rectangular cases, would make a shape's painted extent agree
/// with Word for a rectangle and disagree for every `a:custGeom` — worse than one
/// honest centred stroke plus the importer's finding. It stays modeled, round-tripped
/// and reported.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ShapeOutline {
    /// The outline color.
    pub color: Color,
    /// The outline width in device pixels.
    pub width: f32,
    /// The preset dash pattern (`DashStyle::Solid` = an unbroken line).
    pub dash: DashStyle,
    /// `a:ln@cap` — how a dash and an open end terminate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap: Option<LineCap>,
    /// `a:ln`'s join child — how two segments meet at a corner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join: Option<LineJoin>,
    /// `a:custDash` as dash/space pairs in 1/1000 of a percent of the line width,
    /// which is the unit `ST_PositivePercentage` uses. Empty means none authored.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_dash: Vec<DashStop>,
    /// `a:ln@cmpd` — how many parallel lines the outline draws as. `None` and
    /// `Some(CompoundLine::Single)` both mean one line, which is DrawingML's
    /// default; what each other value paints is [`ShapeOutline::compound_paint`]'s
    /// decision, not a backend's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compound: Option<CompoundLine>,
}

/// The default miter limit, as the ratio of a corner's miter length to the stroke
/// width past which the corner degenerates into a bevel.
///
/// 4 because that is `tiny-skia`'s default, Skia's, and SVG's `stroke-miterlimit`
/// initial value. PDF's default is **10**, which is why the PDF backend emits an
/// explicit `M` rather than relying on the viewer's initial graphics state: left
/// implicit, the same unstated-`a:miter` corner would bevel in the raster output and
/// stay sharp in the PDF.
pub const DEFAULT_MITER_LIMIT: f32 = 4.0;

/// What one `a:ln@cmpd` value paints, decided once here so the raster and PDF
/// backends cannot disagree about it.
///
/// A compound outline's lines are concentric about the path, so each **symmetric**
/// pair of them is the region between two stroke widths — an annulus — and needs no
/// path offsetting to express. The asymmetric forms do, which is why they are
/// [`CompoundPaint::Unsupported`] rather than approximated.
#[derive(Clone, Debug, PartialEq)]
pub enum CompoundPaint {
    /// One plain stroke of the modeled width: `sng`, or no `@cmpd` at all.
    Single,
    /// Concentric bands, each `(knockout, outer)` as a pair of **stroke widths** in
    /// device pixels. A band is the area a stroke of `outer` covers minus the area a
    /// stroke of `knockout` covers, so a `knockout` of zero is a plain centred
    /// stroke of `outer` and anything larger is a symmetric pair of lines.
    ///
    /// Ordered outside in.
    Bands(Vec<(f32, f32)>),
    /// Modeled, carried, re-emitted — and deliberately not painted.
    Unsupported,
}

impl ShapeOutline {
    /// The miter limit this outline's join asks for, as a stroke-width ratio.
    ///
    /// `a:miter@lim` is an `ST_PositivePercentage`, i.e. 1/1000 of a percent, so
    /// `lim="800000"` is 800% and a ratio of 8. The floor is 1: a miter cannot be
    /// shorter than the stroke is wide, and a rasterizer reads a sub-1 limit as
    /// "always bevel", so clamping states that intent instead of relying on it.
    ///
    /// Any other join — and an `a:miter` with no `@lim` — gets
    /// [`DEFAULT_MITER_LIMIT`]. A round or bevel join ignores the limit entirely;
    /// returning the default rather than `None` keeps both backends writing one
    /// value unconditionally, which is what stops PDF's own default of 10 leaking in.
    ///
    /// Complexity: O(1).
    #[must_use]
    pub fn miter_limit(&self) -> f32 {
        match self.join {
            Some(LineJoin::Miter { limit: Some(limit) }) => (limit as f32 / 100_000.0).max(1.0),
            _ => DEFAULT_MITER_LIMIT,
        }
    }

    /// How `a:ln@cmpd` decomposes for this outline's width.
    ///
    /// # Why only `dbl` paints
    ///
    /// `dbl` is two lines of equal weight, and the only reading of "equal" that fits
    /// a stated total width is three equal bands — line, gap, line, each a third of
    /// the width. That is what a `double` border means in CSS and ODF too, so the
    /// geometry is not invented here.
    ///
    /// `thickThin`, `thinThick` and `tri` are **not** painted, and the reason is not
    /// effort. ECMA-376 states no proportions for them, so a renderer has to invent
    /// both the weights and, for the two asymmetric forms, which side the thick line
    /// is on — and "which side" is only defined for a closed path, so an open
    /// connector has no answer at all. A thick line drawn on the wrong side of a
    /// shape's outline is a worse output than one honest line plus a loss report
    /// (`SKILL` §9.4), so these return [`CompoundPaint::Unsupported`] and the
    /// importer's finding stands.
    ///
    /// Complexity: O(1) — at most one band is produced.
    #[must_use]
    pub fn compound_paint(&self) -> CompoundPaint {
        match self.compound {
            None | Some(CompoundLine::Single) => CompoundPaint::Single,
            // Three equal thirds: the visible lines are the outermost and innermost
            // third, which is the area a `width` stroke covers minus the area a
            // `width / 3` stroke covers.
            Some(CompoundLine::Double) => {
                CompoundPaint::Bands(vec![(self.width / 3.0, self.width)])
            }
            Some(CompoundLine::ThickThin | CompoundLine::ThinThick | CompoundLine::Triple) => {
                CompoundPaint::Unsupported
            }
        }
    }
}

/// One command of a resolved shape path, in the same device-scaled twips as the
/// rest of the list.
///
/// This is the display-list form of `casual_doc_model::v1::ShapePathCommand`, with
/// coordinates already resolved to page-local twips so a backend only scales them.
/// Curves live here because both `a:custGeom` and DrawingML's preset table need
/// them (`119` §6, `109` FID-G-02); a straight polyline is just a command list that
/// happens to contain none.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum PathCommand {
    /// Start a subpath at a point (`a:moveTo`).
    MoveTo {
        /// The subpath's first point.
        point: Point,
    },
    /// A straight segment to a point (`a:lnTo`).
    LineTo {
        /// The segment's endpoint.
        point: Point,
    },
    /// A cubic Bézier (`a:cubicBezTo`): two control points, then the endpoint.
    CubicTo {
        /// The control point leaving the previous endpoint.
        control1: Point,
        /// The control point entering `point`.
        control2: Point,
        /// The curve's endpoint.
        point: Point,
    },
    /// A quadratic Bézier (`a:quadBezTo`): one control point, then the endpoint.
    ///
    /// Kept distinct from [`PathCommand::CubicTo`] rather than promoted at
    /// construction so the authored command survives into the display list; a
    /// backend without a quadratic operator (PDF) promotes it itself.
    QuadTo {
        /// The single control point.
        control: Point,
        /// The curve's endpoint.
        point: Point,
    },
}

impl PathCommand {
    /// The point the path arrives at after this command.
    #[must_use]
    pub fn endpoint(self) -> Point {
        match self {
            Self::MoveTo { point }
            | Self::LineTo { point }
            | Self::CubicTo { point, .. }
            | Self::QuadTo { point, .. } => point,
        }
    }

    /// Whether this command draws a segment, as opposed to only moving the pen.
    #[must_use]
    pub fn is_segment(self) -> bool {
        !matches!(self, Self::MoveTo { .. })
    }

    /// Every point the command names, endpoint and controls alike.
    ///
    /// Control points are included deliberately: a Bézier lies inside the convex
    /// hull of its control polygon, so this is a sound over-approximation for the
    /// bounds a gradient extent and a clip need. A tight curve bound would be
    /// smaller but is not what either caller is asking for.
    pub fn points(self) -> impl Iterator<Item = Point> {
        let (a, b, c) = match self {
            Self::MoveTo { point } | Self::LineTo { point } => (point, None, None),
            Self::CubicTo {
                control1,
                control2,
                point,
            } => (control1, Some(control2), Some(point)),
            Self::QuadTo { control, point } => (control, Some(point), None),
        };
        core::iter::once(a).chain(b).chain(c)
    }
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
    /// A path in command order, closed (a filled figure) or open (stroked only).
    ///
    /// One primitive for every non-rectangular outline: a preset's hand-resolved
    /// vertex list and an authored `a:custGeom` are the same thing here, which is
    /// what `119` §6 means by "a path is the primitive and a preset is a recipe".
    /// It replaced a point-list-only `Polygon` variant; the display list has no
    /// persisted form, so nothing had to be migrated.
    Path {
        /// The commands in path order, beginning with a
        /// [`PathCommand::MoveTo`].
        commands: Vec<PathCommand>,
        /// Whether the figure joins back to its subpath start. `false` strokes an
        /// open path — an unclosed `a:custGeom` (`119`).
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
    /// Push a clip **path**; subsequent items are clipped to it until
    /// [`PaintItem::PopClip`], which pops either kind.
    ///
    /// # Why a rectangle was not enough
    ///
    /// A picture-filled shape (`a:blipFill` on a `wps:spPr`) paints its image
    /// clipped to the shape's own outline. With only a rectangular clip, a
    /// picture-filled ellipse or star could not be drawn at all — which is why both
    /// `docs/156` §6 row 0.3 (a shape's own `a:blipFill`) and row 0.2 (the style
    /// matrix's `a:blipFill` entry) were blocked on the same missing primitive
    /// rather than on any modelling work.
    ///
    /// Shares [`PaintItem::PopClip`] with the rectangular form deliberately: the
    /// backend keeps one clip stack, and two pop opcodes would let a mismatched pair
    /// unbalance it silently.
    PushClipPath {
        /// The outline, in the same device-scaled twips as the rest of the list.
        commands: Vec<PathCommand>,
        /// Whether the outline closes. An unclosed path still clips as if filled —
        /// a clip has no stroke — but the flag is carried so the geometry round-trips
        /// unchanged.
        closed: bool,
    },
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
        /// A drop shadow cast by everything in the layer, painted BEHIND it.
        ///
        /// On the layer rather than on each shape, because that is what makes it
        /// correct for free: the shadow is cast by the layer's combined silhouette,
        /// so a shape with an outline, a picture-filled shape and a text box all
        /// cast the shadow of what they actually paint rather than of their
        /// bounding box. A per-shape shadow would have to re-derive each one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        shadow: Option<LayerShadow>,
    },
    /// Pop the most recent layer.
    PopLayer,
}

/// A drop shadow cast by a [`PaintItem::PushLayer`] group (`a:outerShdw`).
///
/// In twips like every other length in this list, and for the same reason: the device
/// scale belongs to the backend, so a shadow authored once renders at every zoom. The
/// DrawingML knowledge is what has already been spent — `a:outerShdw`'s `@dist` and
/// `@dir` are polar, and layout converts them to a cartesian offset, so the backend
/// sees an offset, a radius and a colour and needs no effect vocabulary at all.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct LayerShadow {
    /// `a:outerShdw@blurRad` as a blur radius. Zero is a hard-edged offset copy,
    /// which is a legal shadow and not a no-op.
    pub blur: Twip,
    /// The horizontal offset, from `@dist` and `@dir`.
    pub offset_x: Twip,
    /// The vertical offset. Positive is DOWN, matching the page's y axis.
    pub offset_y: Twip,
    /// The shadow colour, with the effect's alpha already folded in.
    pub color: Color,
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
