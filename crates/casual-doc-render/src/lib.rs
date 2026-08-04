//! CPU rasterization backend for the OpenDoc display list.
//!
//! Executes a [`casual_doc_layout::display::DisplayList`] onto a `tiny-skia`
//! pixmap: rectangles are filled/stroked directly, and glyph runs are rasterized
//! by extracting each glyph's outline from the *same* font face the shaper used
//! (via `skrifa`), so shaping and rendering agree exactly. This is the first of
//! the pluggable backends (`43-…` §5.2 — the WASM canvas and GPU/`vello` backends
//! consume the same display list). The display list is in device-independent
//! twips; the device scale (DPI × zoom) is applied here, at paint.
//!
//! Glyph outlines require the font bytes for a [`FontId`]; a [`GlyphSource`]
//! supplies them, keeping this crate independent of how fonts are resolved
//! (`P1C-002`).

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::io::Cursor;

use casual_doc_layout::display::{DisplayList, PaintItem};
// Kept on a separate `use` line (anti-conflict): the shape fill/outline/geometry
// display types the shape paint path consumes (`Color` aliased to avoid clashing
// with `tiny_skia::Color`).
use casual_doc_layout::display::{
    Color as DisplayColor, Fill, Gradient, GradientKind, ShapeGeometry, ShapeOutline,
};
// Separate `use` line to minimize import-block merge conflicts.
use casual_doc_layout::display::ShapeTransform;
use casual_doc_layout::font_registry::{DynFace, FontRegistry};
use casual_doc_layout::text::{FontId, GlyphRun};
use casual_doc_layout::units::{Point, Rect};
use casual_doc_model::v1::{CROP_FULL, CropRect};
// Kept on a separate `use` line (anti-conflict): the dash/line-end model types the
// shape paint path consumes.
use casual_doc_model::v1::{DashStyle, LineEnd, LineEndKind, LineEndSize};
use skrifa::instance::{LocationRef, Size};
use skrifa::metrics::Metrics;
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, GlyphId, MetadataProvider};
use tiny_skia::{
    Color, FillRule, FilterQuality, GradientStop as SkGradientStop, IntRect, IntSize,
    LinearGradient, Mask, Paint, PathBuilder, Pixmap, PixmapPaint, Point as SkPoint,
    RadialGradient, Rect as SkRect, Shader, SpreadMode, Stroke, StrokeDash, Transform,
};

/// Secure per-image decode defaults from `docs/21-PARSER-LIMITS.md`.
const MAX_DECODED_IMAGE_DIMENSION: u32 = 32_768;
const MAX_DECODED_IMAGE_PIXELS: u64 = 100_000_000;
const MAX_DECODED_IMAGE_BYTES: u64 = MAX_DECODED_IMAGE_PIXELS * 4;

/// Supplies the raw font bytes (and face index) for a [`FontId`] so the renderer
/// can extract glyph outlines from the exact face the shaper used.
pub trait GlyphSource {
    /// The font file bytes for `font`, or `None` if unknown (glyphs are skipped).
    fn font_data(&self, font: FontId) -> Option<&[u8]>;

    /// The face index within the file for `font` — `0` for a single-face
    /// `.ttf`/`.otf`, nonzero for a face inside a `.ttc`/`.otc` collection (many OS
    /// CJK fonts ship as collections). Defaults to `0`; only sources that serve
    /// collection files (the system/host fallback path) override it.
    fn face_index(&self, _font: FontId) -> u32 {
        0
    }
}

/// Supplies the raw encoded bytes for an inline image, keyed by the display list's
/// media key (the package part name) — the image analogue of [`GlyphSource`]. The
/// bytes live in the package (`word/media/*`); the pipeline/example serves them
/// here, keeping this crate independent of how the package is opened.
pub trait MediaSource {
    /// The encoded image bytes for `media` (its package part name), or `None` if
    /// unknown (the image renders nothing — bytes present but undecodable
    /// instead render a visible placeholder box, not a blank gap).
    fn media_bytes(&self, media: &str) -> Option<&[u8]>;
}

/// Errors from rendering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderError {
    /// The requested pixmap size was zero or too large.
    InvalidSize,
    /// PNG encoding failed.
    PngEncode,
}

/// A render target — an RGBA pixmap. Wraps a `tiny-skia` [`Pixmap`].
#[derive(Clone, Debug)]
pub struct Surface {
    pixmap: Pixmap,
}

impl Surface {
    /// Creates a `width × height` surface filled with white.
    pub fn new(width: u32, height: u32) -> Result<Self, RenderError> {
        Self::with_background(width, height, [255, 255, 255])
    }

    /// Creates a `width × height` surface filled with the given opaque sRGB
    /// color — the document's page background (`w:background`). Painted behind
    /// the whole page before the display list.
    pub fn with_background(
        width: u32,
        height: u32,
        [r, g, b]: [u8; 3],
    ) -> Result<Self, RenderError> {
        let mut pixmap = Pixmap::new(width, height).ok_or(RenderError::InvalidSize)?;
        pixmap.fill(Color::from_rgba8(r, g, b, 255));
        Ok(Self { pixmap })
    }

    /// The raw premultiplied-RGBA pixels.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        self.pixmap.data()
    }

    /// Encodes the surface as a PNG.
    pub fn encode_png(&self) -> Result<Vec<u8>, RenderError> {
        self.pixmap.encode_png().map_err(|_| RenderError::PngEncode)
    }
}

/// Renders `list` onto `surface`, applying the device scale `dpi` (device pixels
/// per inch × zoom) to the twip-space display list. Glyph outlines are taken from
/// `fonts`; a glyph whose font is unknown is skipped. Inline-image bytes are taken
/// from `media`; an image whose media is unknown renders nothing, while one with
/// bytes present but an undecodable format (e.g. EMF/WMF) renders a visible
/// placeholder box instead.
pub fn render(
    list: &DisplayList,
    surface: &mut Surface,
    dpi: f32,
    fonts: &dyn GlyphSource,
    media: &dyn MediaSource,
) {
    // A stack of clip masks: each entry is the *effective* clip (the intersection
    // of every enclosing `PushClip` rectangle). While non-empty, its top is
    // passed to every paint call so content outside an `exact`-height row's clip
    // rect is not drawn.
    let mut clip_stack: Vec<Mask> = Vec::new();
    for item in &list.items {
        match item {
            PaintItem::Rect { rect, fill, stroke } => {
                if let Some(path) = rect_path(*rect, dpi) {
                    paint_path(
                        surface,
                        &path,
                        fill.as_ref(),
                        stroke.as_ref(),
                        clip_stack.last(),
                    );
                }
            }
            PaintItem::Ellipse { rect, fill, stroke } => {
                if let Some(path) = ellipse_path(*rect, dpi) {
                    paint_path(
                        surface,
                        &path,
                        fill.as_ref(),
                        stroke.as_ref(),
                        clip_stack.last(),
                    );
                }
            }
            PaintItem::RoundedRect {
                rect,
                radius,
                fill,
                stroke,
            } => {
                if let Some(path) = rounded_rect_path(*rect, *radius, dpi) {
                    paint_path(
                        surface,
                        &path,
                        fill.as_ref(),
                        stroke.as_ref(),
                        clip_stack.last(),
                    );
                }
            }
            PaintItem::Polygon {
                points,
                fill,
                stroke,
            } => {
                if let Some(path) = polygon_path(points, dpi) {
                    paint_path(
                        surface,
                        &path,
                        fill.as_ref(),
                        stroke.as_ref(),
                        clip_stack.last(),
                    );
                }
            }
            PaintItem::Shape {
                geometry,
                fill,
                stroke,
                head_end,
                tail_end,
                transform,
            } => {
                render_shape(
                    surface,
                    geometry,
                    fill.as_ref(),
                    stroke.as_ref(),
                    head_end.as_ref(),
                    tail_end.as_ref(),
                    dpi,
                    clip_stack.last(),
                    object_transform(transform.as_ref(), dpi),
                );
            }
            PaintItem::Glyphs { run } => {
                render_glyph_run(run, surface, dpi, fonts, clip_stack.last());
            }
            // Push the clip rectangle, intersecting it with any enclosing clip so
            // nested clips compose. Coordinates are twips, scaled here like rects.
            PaintItem::PushClip(rect) => {
                if let Some(mask) = build_clip_mask(&surface.pixmap, *rect, dpi, clip_stack.last())
                {
                    clip_stack.push(mask);
                } else if let Some(parent) = clip_stack.last() {
                    // Degenerate rect (unreachable for a valid surface): inherit
                    // the parent clip so the stack stays balanced with `PopClip`.
                    let inherited = parent.clone();
                    clip_stack.push(inherited);
                }
            }
            PaintItem::PopClip => {
                clip_stack.pop();
            }
            PaintItem::Image {
                media: id,
                rect,
                crop,
                transform,
            } => {
                render_image(
                    id,
                    *rect,
                    crop.as_ref(),
                    surface,
                    dpi,
                    media,
                    clip_stack.last(),
                    object_transform(transform.as_ref(), dpi),
                );
            }
            PaintItem::Line { from, to, stroke } => {
                let clip = clip_stack.last();
                let mut paint = Paint::default();
                paint.set_color_rgba8(
                    stroke.color.r,
                    stroke.color.g,
                    stroke.color.b,
                    stroke.color.a,
                );
                paint.anti_alias = true;
                let mut builder = PathBuilder::new();
                builder.move_to(from.x.to_device_px(dpi), from.y.to_device_px(dpi));
                builder.line_to(to.x.to_device_px(dpi), to.y.to_device_px(dpi));
                if let Some(path) = builder.finish() {
                    surface.pixmap.stroke_path(
                        &path,
                        &paint,
                        &Stroke {
                            width: stroke.width,
                            ..Stroke::default()
                        },
                        Transform::identity(),
                        clip,
                    );
                }
            }
        }
    }
}

/// Decodes an inline image's bytes and blits them, scaled, into `rect` (twips,
/// scaled to device pixels here) under the current `clip`. Two distinct
/// no-media-drawn cases are handled differently (`docs/55` §8):
///
/// - **No bytes at all** (`media.media_bytes` misses): there is genuinely
///   nothing to show, so nothing is painted.
/// - **Bytes present but the format isn't decodable** by the PNG/JPEG-only
///   path below (e.g. an EMF/WMF vector metafile — real metafile decoding
///   remains future work, `P1F-28`): the picture's box is fully known even
///   though its content isn't, so a visible placeholder (a bordered box with
///   a diagonal cross, the "broken image" convention) is painted in `rect`
///   instead of a silent blank gap — the same never-silently-drop spirit as
///   `casual-doc-layout`'s `.notdef` glyph box.
///
/// A degenerate box (zero/negative device size) renders nothing in either
/// case; there is no area to paint into.
#[allow(clippy::too_many_arguments)]
fn render_image(
    media_id: &str,
    rect: Rect,
    crop: Option<&CropRect>,
    surface: &mut Surface,
    dpi: f32,
    media: &dyn MediaSource,
    clip: Option<&Mask>,
    transform: Transform,
) {
    let Some(bytes) = media.media_bytes(media_id) else {
        return;
    };
    let Some(source) = decode_to_pixmap(bytes) else {
        render_undecodable_placeholder(rect, surface, dpi, clip, transform);
        return;
    };
    // A crop (`a:srcRect`) selects a sub-rectangle of the SOURCE pixels; that
    // visible region is what scales to fill the destination box. Extract it first
    // so the scale below maps only the cropped source into `rect`. An identity or
    // degenerate crop leaves the whole source in play.
    let source = match crop.filter(|crop| !crop.is_identity()) {
        Some(crop) => match crop_pixmap(&source, crop) {
            Some(cropped) => cropped,
            None => return,
        },
        None => source,
    };
    let (src_w, src_h) = (source.width() as f32, source.height() as f32);
    if src_w <= 0.0 || src_h <= 0.0 {
        return;
    }
    let dx = rect.origin.x.to_device_px(dpi);
    let dy = rect.origin.y.to_device_px(dpi);
    let dw = rect.size.width.to_device_px(dpi);
    let dh = rect.size.height.to_device_px(dpi);
    if dw <= 0.0 || dh <= 0.0 {
        return;
    }
    // Scale the source pixmap to the destination box, then translate to its
    // top-left; `draw_pixmap` maps pixmap space through this transform. The
    // object transform (rotation/flip about the box center) is applied AFTER the
    // placement, so the picture rotates in page space.
    let placement = Transform::from_row(dw / src_w, 0.0, 0.0, dh / src_h, dx, dy);
    let paint = PixmapPaint {
        quality: FilterQuality::Bilinear,
        ..PixmapPaint::default()
    };
    surface.pixmap.draw_pixmap(
        0,
        0,
        source.as_ref(),
        &paint,
        transform.pre_concat(placement),
        clip,
    );
}

/// Extracts the visible source sub-rectangle described by an `a:srcRect` crop
/// from `source`. Each edge fraction is in [`CROP_FULL`]-relative units
/// (thousandths of a percent); the visible region is
/// `[left, CROP_FULL - right] x [top, CROP_FULL - bottom]` of the source,
/// clamped to the pixmap and to at least one pixel. Returns `None` when the crop
/// leaves no visible area (nothing to paint).
fn crop_pixmap(source: &Pixmap, crop: &CropRect) -> Option<Pixmap> {
    let w = source.width() as i64;
    let h = source.height() as i64;
    let full = i64::from(CROP_FULL);
    let axis = |size: i64, near: i32, far: i32| -> Option<(i32, u32)> {
        // Pixel offsets of the visible span; the near edge hides `near` of the
        // dimension, the far edge hides `far`, leaving at least one pixel.
        let start = (size * i64::from(near.max(0)) / full).clamp(0, size);
        let end = (size - size * i64::from(far.max(0)) / full).clamp(0, size);
        let start = start.min(end.saturating_sub(1)).max(0);
        let len = (end - start).max(1).min(size - start);
        (len > 0).then_some((start as i32, len as u32))
    };
    let (x, cw) = axis(w, crop.left, crop.right)?;
    let (y, ch) = axis(h, crop.top, crop.bottom)?;
    let sub = IntRect::from_xywh(x, y, cw, ch)?;
    source.clone_rect(sub)
}

/// The placeholder's stroke color: a neutral mid-gray, visible against a white
/// page without being mistaken for real ink (glyphs/borders are usually
/// darker or a document color).
const PLACEHOLDER_STROKE: (u8, u8, u8, u8) = (150, 150, 150, 255);

/// Paints a visible, deterministic "unsupported image" placeholder filling
/// `rect`: a stroked border box with a diagonal cross, the conventional
/// "broken image" glyph. Used when media bytes are present but not decodable
/// by [`decode_to_pixmap`] (e.g. EMF/WMF), so a reader sees "there was a
/// picture here" rather than a blank gap. A degenerate device box paints
/// nothing (mirrors the decoded-image path).
fn render_undecodable_placeholder(
    rect: Rect,
    surface: &mut Surface,
    dpi: f32,
    clip: Option<&Mask>,
    transform: Transform,
) {
    let dx = rect.origin.x.to_device_px(dpi);
    let dy = rect.origin.y.to_device_px(dpi);
    let dw = rect.size.width.to_device_px(dpi);
    let dh = rect.size.height.to_device_px(dpi);
    if dw <= 0.0 || dh <= 0.0 {
        return;
    }
    let mut paint = Paint::default();
    let (r, g, b, a) = PLACEHOLDER_STROKE;
    paint.set_color_rgba8(r, g, b, a);
    paint.anti_alias = true;
    let stroke = Stroke {
        width: (dw.min(dh) * 0.02).max(1.0),
        ..Stroke::default()
    };

    // The border.
    if let Some(border) = SkRect::from_xywh(dx, dy, dw, dh) {
        let mut builder = PathBuilder::new();
        builder.push_rect(border);
        if let Some(path) = builder.finish() {
            surface
                .pixmap
                .stroke_path(&path, &paint, &stroke, transform, clip);
        }
    }

    // The diagonal cross, corner to corner.
    let mut cross = PathBuilder::new();
    cross.move_to(dx, dy);
    cross.line_to(dx + dw, dy + dh);
    cross.move_to(dx + dw, dy);
    cross.line_to(dx, dy + dh);
    if let Some(path) = cross.finish() {
        surface
            .pixmap
            .stroke_path(&path, &paint, &stroke, transform, clip);
    }
}

/// Decodes PNG/JPEG bytes to a premultiplied-RGBA `tiny-skia` pixmap.
///
/// Dimensions are read and bounded before the full decode, and the decoder gets
/// a matching allocation ceiling. Unsupported, corrupt, zero-sized, or
/// over-budget images are skipped without allocating the RGBA output.
fn decode_to_pixmap(bytes: &[u8]) -> Option<Pixmap> {
    let format = image::guess_format(bytes).ok()?;
    let (width, height) = image::ImageReader::with_format(Cursor::new(bytes), format)
        .into_dimensions()
        .ok()?;
    if !image_dimensions_within_limits(width, height) {
        return None;
    }

    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODED_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_DECODED_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_DECODED_IMAGE_BYTES);
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits);
    let image = reader.decode().ok()?;
    if !image_dimensions_within_limits(image.width(), image.height()) {
        return None;
    }
    let rgba = image.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut data = rgba.into_raw();
    // `Pixmap::from_vec` expects premultiplied alpha; the decoder yields straight
    // (non-premultiplied) RGBA, so premultiply each channel by its alpha.
    for px in data.chunks_exact_mut(4) {
        let a = u16::from(px[3]);
        px[0] = (u16::from(px[0]) * a / 255) as u8;
        px[1] = (u16::from(px[1]) * a / 255) as u8;
        px[2] = (u16::from(px[2]) * a / 255) as u8;
    }
    Pixmap::from_vec(data, IntSize::from_wh(w, h)?)
}

fn image_dimensions_within_limits(width: u32, height: u32) -> bool {
    width > 0
        && height > 0
        && width <= MAX_DECODED_IMAGE_DIMENSION
        && height <= MAX_DECODED_IMAGE_DIMENSION
        && u64::from(width)
            .checked_mul(u64::from(height))
            .is_some_and(|pixels| pixels <= MAX_DECODED_IMAGE_PIXELS)
}

/// Builds the effective clip mask for a `PushClip(rect)`: the rectangle painted
/// into an 8-bit alpha mask, intersected with the enclosing clip (`parent`) so
/// nested clips compose. Returns `None` only for a degenerate (zero-area) rect.
fn build_clip_mask(pixmap: &Pixmap, rect: Rect, dpi: f32, parent: Option<&Mask>) -> Option<Mask> {
    let path = rect_path(rect, dpi)?;
    match parent {
        Some(parent) => {
            let mut mask = parent.clone();
            mask.intersect_path(&path, FillRule::Winding, true, Transform::identity());
            Some(mask)
        }
        None => {
            let mut mask = Mask::new(pixmap.width(), pixmap.height())?;
            mask.fill_path(&path, FillRule::Winding, true, Transform::identity());
            Some(mask)
        }
    }
}

/// Rasterizes one glyph run by outlining each glyph from its face, clipped to
/// `clip` (the current clip mask, if any).
fn render_glyph_run(
    run: &GlyphRun,
    surface: &mut Surface,
    dpi: f32,
    fonts: &dyn GlyphSource,
    clip: Option<&Mask>,
) {
    let Some(bytes) = fonts.font_data(run.font) else {
        return;
    };
    // A system/host fallback face may live inside a `.ttc` collection, so select
    // the exact face by index; bundled single-face files report index 0.
    let Ok(font) = FontRef::from_index(bytes, fonts.face_index(run.font)) else {
        return;
    };
    let outlines = font.outline_glyphs();
    let size_px = run.size.to_device_px(dpi);
    let scale_x = f32::from(run.character_scale_percent) / 100.0;
    let start_x = run.origin.x.to_device_px(dpi);
    let mut pen_x = start_x;
    let baseline_y = run.origin.y.to_device_px(dpi);

    let mut builder = PathBuilder::new();
    let mut wrote_any = false;
    for glyph in &run.glyphs {
        if let Some(outline) = outlines.get(GlyphId::new(glyph.id)) {
            let mut pen = GlyphPen {
                builder: &mut builder,
                origin_x: pen_x,
                baseline_y,
                scale_x,
            };
            let settings = DrawSettings::unhinted(Size::new(size_px), LocationRef::default());
            if outline.draw(settings, &mut pen).is_ok() {
                wrote_any = true;
            }
        }
        pen_x += glyph.advance.to_device_px(dpi);
    }

    if wrote_any && let Some(path) = builder.finish() {
        let mut paint = Paint::default();
        paint.set_color_rgba8(run.color[0], run.color[1], run.color[2], run.color[3]);
        paint.anti_alias = true;
        surface.pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            clip,
        );
    }

    // Underline / strikethrough (`w:u`/`w:strike`): drawn as thin filled rects in
    // the run color, spanning the run's total advance, over the glyphs. Positions
    // and thickness come from the face's own metrics, falling back to size-derived
    // defaults for faces that omit them.
    if run.decoration.underline || run.decoration.strikethrough || run.decoration.double_strike {
        let advance = pen_x - start_x;
        if advance > 0.0 {
            let metrics = Metrics::new(&font, Size::new(size_px), LocationRef::default());
            if run.decoration.underline {
                // `offset` is the top of the decoration measured up from the
                // baseline; a device y grows downward, so subtract it. Underlines
                // sit below the baseline (a negative offset → positive y).
                let (offset, thickness) = metrics
                    .underline
                    .map(|d| (d.offset, d.thickness))
                    .unwrap_or((-size_px * 0.12, size_px * 0.06));
                // `w:u@color` colors the underline independently of the text; it
                // falls back to the run color when unset (`auto`).
                let underline_color = run.decoration.underline_color.unwrap_or(run.color);
                draw_underline(
                    surface,
                    clip,
                    start_x,
                    baseline_y - offset,
                    advance,
                    thickness,
                    underline_color,
                    run.decoration.underline_style,
                );
            }
            if run.decoration.strikethrough || run.decoration.double_strike {
                let (offset, thickness) = metrics
                    .strikeout
                    .map(|d| (d.offset, d.thickness))
                    .unwrap_or((size_px * 0.26, size_px * 0.06));
                // Double strike-through (`w:dstrike`) draws two parallel lines
                // straddling the single strike position; a single strike draws one.
                let ys: &[f32] = if run.decoration.double_strike {
                    &[offset + thickness, offset - thickness]
                } else {
                    &[offset]
                };
                for line_offset in ys {
                    draw_decoration(
                        surface,
                        clip,
                        start_x,
                        baseline_y - line_offset,
                        advance,
                        thickness,
                        run.color,
                    );
                }
            }
        }
    }
}

/// Fills a thin horizontal decoration line (underline/strikethrough): a rect from
/// `x` spanning `advance` wide, centered on `y_center`, `thickness` (min 1px) tall.
fn draw_decoration(
    surface: &mut Surface,
    clip: Option<&Mask>,
    x: f32,
    y_center: f32,
    advance: f32,
    thickness: f32,
    color: [u8; 4],
) {
    let h = thickness.max(1.0);
    let Some(rect) = SkRect::from_xywh(x, y_center - h / 2.0, advance, h) else {
        return;
    };
    let mut builder = PathBuilder::new();
    builder.push_rect(rect);
    let Some(path) = builder.finish() else {
        return;
    };
    let mut paint = Paint::default();
    paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
    paint.anti_alias = true;
    surface.pixmap.fill_path(
        &path,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        clip,
    );
}

/// Draws an underline in its `w:u@val` line style. Single/double/thick are drawn
/// exactly; dotted/dashed/dot-dash as patterned segments; wave and words are
/// approximated by a single line (a true sine wave / per-word gapping is a
/// follow-up), so they still underline rather than disappearing.
#[allow(clippy::too_many_arguments)]
fn draw_underline(
    surface: &mut Surface,
    clip: Option<&Mask>,
    x: f32,
    y_center: f32,
    advance: f32,
    thickness: f32,
    color: [u8; 4],
    style: casual_doc_model::v1::UnderlineStyle,
) {
    use casual_doc_model::v1::UnderlineStyle;
    let line = |surface: &mut Surface, x: f32, w: f32, y: f32, t: f32| {
        draw_decoration(surface, clip, x, y, w, t, color);
    };
    // A repeating on/off pattern of segments (dotted/dashed) across the advance.
    let dashed = |surface: &mut Surface, on: f32, off: f32| {
        let mut cursor = 0.0_f32;
        while cursor < advance {
            let seg = on.min(advance - cursor);
            if seg > 0.0 {
                line(surface, x + cursor, seg, y_center, thickness);
            }
            cursor += on + off;
        }
    };
    match style {
        UnderlineStyle::Single | UnderlineStyle::Wavy | UnderlineStyle::Words => {
            line(surface, x, advance, y_center, thickness);
        }
        UnderlineStyle::Double => {
            let gap = (thickness * 2.0).max(1.5);
            line(surface, x, advance, y_center - gap / 2.0, thickness);
            line(surface, x, advance, y_center + gap / 2.0, thickness);
        }
        UnderlineStyle::Thick => {
            line(surface, x, advance, y_center, (thickness * 2.0).max(1.5));
        }
        UnderlineStyle::Dotted => dashed(surface, thickness.max(1.0), thickness * 1.6),
        UnderlineStyle::Dashed => dashed(surface, thickness * 4.0, thickness * 3.0),
        UnderlineStyle::DotDash => {
            // Alternating dash then dot, each followed by a gap.
            let (dash, dot, gap) = (thickness * 4.0, thickness.max(1.0), thickness * 2.5);
            let period = dash + gap + dot + gap;
            let mut base = 0.0_f32;
            while base < advance {
                let d = dash.min(advance - base);
                if d > 0.0 {
                    line(surface, x + base, d, y_center, thickness);
                }
                let dot_start = base + dash + gap;
                if dot_start < advance {
                    let d2 = dot.min(advance - dot_start);
                    line(surface, x + dot_start, d2, y_center, thickness);
                }
                base += period;
            }
        }
    }
}

/// A `skrifa` outline pen that appends glyph contours to a `tiny-skia` path,
/// translating font space (origin at the glyph, y-up) into device space
/// (`origin_x`, `baseline_y`, y-down).
struct GlyphPen<'a> {
    builder: &'a mut PathBuilder,
    origin_x: f32,
    baseline_y: f32,
    scale_x: f32,
}

impl GlyphPen<'_> {
    fn map(&self, x: f32, y: f32) -> (f32, f32) {
        (self.origin_x + x * self.scale_x, self.baseline_y - y)
    }
}

impl OutlinePen for GlyphPen<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        let (px, py) = self.map(x, y);
        self.builder.move_to(px, py);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let (px, py) = self.map(x, y);
        self.builder.line_to(px, py);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let (cx, cy) = self.map(cx0, cy0);
        let (px, py) = self.map(x, y);
        self.builder.quad_to(cx, cy, px, py);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (c0x, c0y) = self.map(cx0, cy0);
        let (c1x, c1y) = self.map(cx1, cy1);
        let (px, py) = self.map(x, y);
        self.builder.cubic_to(c0x, c0y, c1x, c1y, px, py);
    }

    fn close(&mut self) {
        self.builder.close();
    }
}

/// Builds a `tiny-skia` rectangle path from a twip rect at the device scale.
fn rect_path(rect: Rect, dpi: f32) -> Option<tiny_skia::Path> {
    let x = rect.origin.x.to_device_px(dpi);
    let y = rect.origin.y.to_device_px(dpi);
    let w = rect.size.width.to_device_px(dpi);
    let h = rect.size.height.to_device_px(dpi);
    let mut builder = PathBuilder::new();
    builder.push_rect(tiny_skia::Rect::from_xywh(x, y, w.max(0.0), h.max(0.0))?);
    builder.finish()
}

fn ellipse_path(rect: Rect, dpi: f32) -> Option<tiny_skia::Path> {
    let x = rect.origin.x.to_device_px(dpi);
    let y = rect.origin.y.to_device_px(dpi);
    let width = rect.size.width.to_device_px(dpi).max(0.0);
    let height = rect.size.height.to_device_px(dpi).max(0.0);
    PathBuilder::from_oval(SkRect::from_xywh(x, y, width, height)?)
}

fn polygon_path(points: &[Point], dpi: f32) -> Option<tiny_skia::Path> {
    let (first, rest) = points.split_first()?;
    if rest.len() < 2 {
        return None;
    }
    let mut builder = PathBuilder::new();
    builder.move_to(first.x.to_device_px(dpi), first.y.to_device_px(dpi));
    for point in rest {
        builder.line_to(point.x.to_device_px(dpi), point.y.to_device_px(dpi));
    }
    builder.close();
    builder.finish()
}

fn rounded_rect_path(
    rect: Rect,
    radius: casual_doc_layout::units::Twip,
    dpi: f32,
) -> Option<tiny_skia::Path> {
    let x = rect.origin.x.to_device_px(dpi);
    let y = rect.origin.y.to_device_px(dpi);
    let width = rect.size.width.to_device_px(dpi).max(0.0);
    let height = rect.size.height.to_device_px(dpi).max(0.0);
    let radius = radius
        .to_device_px(dpi)
        .max(0.0)
        .min(width / 2.0)
        .min(height / 2.0);
    let right = x + width;
    let bottom = y + height;
    let mut builder = PathBuilder::new();
    builder.move_to(x + radius, y);
    builder.line_to(right - radius, y);
    builder.quad_to(right, y, right, y + radius);
    builder.line_to(right, bottom - radius);
    builder.quad_to(right, bottom, right - radius, bottom);
    builder.line_to(x + radius, bottom);
    builder.quad_to(x, bottom, x, bottom - radius);
    builder.line_to(x, y + radius);
    builder.quad_to(x, y, x + radius, y);
    builder.close();
    builder.finish()
}

fn paint_path(
    surface: &mut Surface,
    path: &tiny_skia::Path,
    fill: Option<&casual_doc_layout::display::Color>,
    stroke: Option<&casual_doc_layout::display::Stroke>,
    clip: Option<&Mask>,
) {
    if let Some(color) = fill {
        let mut paint = Paint::default();
        paint.set_color_rgba8(color.r, color.g, color.b, color.a);
        paint.anti_alias = true;
        surface
            .pixmap
            .fill_path(path, &paint, FillRule::Winding, Transform::identity(), clip);
    }
    if let Some(stroke) = stroke {
        let mut paint = Paint::default();
        paint.set_color_rgba8(
            stroke.color.r,
            stroke.color.g,
            stroke.color.b,
            stroke.color.a,
        );
        paint.anti_alias = true;
        surface.pixmap.stroke_path(
            path,
            &paint,
            &Stroke {
                width: stroke.width,
                ..Stroke::default()
            },
            Transform::identity(),
            clip,
        );
    }
}

/// Paints a floating DrawingML shape: its geometry filled (solid or gradient) and
/// stroked (with a preset dash pattern), plus start/end arrowheads for a line.
#[allow(clippy::too_many_arguments)]
/// Builds the device-space affine transform for an object transform: a rotation
/// and/or flips about the object's center (`a:xfrm`). Identity when the
/// descriptor is absent or a no-op, so the common unrotated path is unchanged.
///
/// DrawingML applies `flipH`/`flipV` before `rot`, both about the center, so the
/// matrix is `T(c) · R(rot) · Flip · T(-c)`.
fn object_transform(transform: Option<&ShapeTransform>, dpi: f32) -> Transform {
    let Some(t) = transform else {
        return Transform::identity();
    };
    if t.rotation == 0 && !t.flip_h && !t.flip_v {
        return Transform::identity();
    }
    let cx = t.center.x.to_device_px(dpi);
    let cy = t.center.y.to_device_px(dpi);
    let angle_deg = t.rotation as f32 / 60_000.0;
    let sx = if t.flip_h { -1.0 } else { 1.0 };
    let sy = if t.flip_v { -1.0 } else { 1.0 };
    Transform::from_translate(cx, cy)
        .pre_concat(Transform::from_rotate(angle_deg))
        .pre_concat(Transform::from_scale(sx, sy))
        .pre_concat(Transform::from_translate(-cx, -cy))
}

#[allow(clippy::too_many_arguments)]
fn render_shape(
    surface: &mut Surface,
    geometry: &ShapeGeometry,
    fill: Option<&Fill>,
    stroke: Option<&ShapeOutline>,
    head_end: Option<&LineEnd>,
    tail_end: Option<&LineEnd>,
    dpi: f32,
    clip: Option<&Mask>,
    transform: Transform,
) {
    // Build the geometry path and its device-pixel bounds (the gradient extent).
    let (path, bounds) = match geometry {
        ShapeGeometry::Rect { rect } => (rect_path(*rect, dpi), device_bounds(*rect, dpi)),
        ShapeGeometry::Ellipse { rect } => (ellipse_path(*rect, dpi), device_bounds(*rect, dpi)),
        ShapeGeometry::RoundedRect { rect, radius } => (
            rounded_rect_path(*rect, *radius, dpi),
            device_bounds(*rect, dpi),
        ),
        ShapeGeometry::Polygon { points } => {
            (polygon_path(points, dpi), polygon_bounds(points, dpi))
        }
        ShapeGeometry::Line { from, to } => {
            let mut builder = PathBuilder::new();
            builder.move_to(from.x.to_device_px(dpi), from.y.to_device_px(dpi));
            builder.line_to(to.x.to_device_px(dpi), to.y.to_device_px(dpi));
            (builder.finish(), None)
        }
    };
    let Some(path) = path else {
        return;
    };

    // Fill (solid or gradient), then stroke (dashable), then arrowheads.
    if let Some(fill) = fill {
        let mut paint = Paint::default();
        match fill {
            Fill::Solid(color) => {
                paint.set_color_rgba8(color.r, color.g, color.b, color.a);
            }
            Fill::Gradient(gradient) => {
                if let Some(bounds) = bounds
                    && let Some(shader) = gradient_shader(gradient, bounds)
                {
                    paint.shader = shader;
                } else if let Some(first) = fill_fallback_color(fill) {
                    paint.set_color_rgba8(first.0, first.1, first.2, first.3);
                }
            }
        }
        paint.anti_alias = true;
        surface
            .pixmap
            .fill_path(&path, &paint, FillRule::Winding, transform, clip);
    }

    if let Some(stroke) = stroke {
        let mut paint = Paint::default();
        paint.set_color_rgba8(
            stroke.color.r,
            stroke.color.g,
            stroke.color.b,
            stroke.color.a,
        );
        paint.anti_alias = true;
        let width = stroke.width.max(1.0);
        let sk_stroke = Stroke {
            width,
            dash: dash_pattern(stroke.dash, width),
            ..Stroke::default()
        };
        surface
            .pixmap
            .stroke_path(&path, &paint, &sk_stroke, transform, clip);

        // Arrowheads sit at the line's endpoints, oriented along the segment. The
        // endpoints ride the same transform so a rotated/flipped line keeps its
        // heads attached and correctly oriented.
        if let ShapeGeometry::Line { from, to } = geometry {
            let mut ends = [
                SkPoint::from_xy(from.x.to_device_px(dpi), from.y.to_device_px(dpi)),
                SkPoint::from_xy(to.x.to_device_px(dpi), to.y.to_device_px(dpi)),
            ];
            transform.map_points(&mut ends);
            let [a, b] = ends;
            if let Some(head) = head_end {
                // The head sits at the start, pointing back along `b -> a`.
                draw_arrowhead(surface, a, b, head, width, stroke.color, clip);
            }
            if let Some(tail) = tail_end {
                // The tail sits at the end, pointing along `a -> b`.
                draw_arrowhead(surface, b, a, tail, width, stroke.color, clip);
            }
        }
    }
}

/// The device-pixel bounding rectangle of a twip rect (for a gradient extent).
fn device_bounds(rect: Rect, dpi: f32) -> Option<SkRect> {
    SkRect::from_xywh(
        rect.origin.x.to_device_px(dpi),
        rect.origin.y.to_device_px(dpi),
        rect.size.width.to_device_px(dpi).max(0.0),
        rect.size.height.to_device_px(dpi).max(0.0),
    )
}

/// The device-pixel bounding rectangle of a polygon's vertices.
fn polygon_bounds(points: &[Point], dpi: f32) -> Option<SkRect> {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for point in points {
        let x = point.x.to_device_px(dpi);
        let y = point.y.to_device_px(dpi);
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    SkRect::from_xywh(
        min_x,
        min_y,
        (max_x - min_x).max(0.0),
        (max_y - min_y).max(0.0),
    )
}

/// The first gradient stop's color, used as a flat fallback when the gradient
/// extent is degenerate (a tiny-skia gradient needs a positive-size box).
fn fill_fallback_color(fill: &Fill) -> Option<(u8, u8, u8, u8)> {
    match fill {
        Fill::Solid(color) => Some((color.r, color.g, color.b, color.a)),
        Fill::Gradient(gradient) => gradient
            .stops
            .first()
            .map(|stop| (stop.color.r, stop.color.g, stop.color.b, stop.color.a)),
    }
}

/// Builds a tiny-skia gradient shader for `gradient` spanning `bounds`. A linear
/// gradient's endpoints are the box center projected along the sweep angle to the
/// box extent; a radial gradient is centered on the box. Returns `None` (the
/// caller falls back to the first stop) if the stops or box are degenerate.
fn gradient_shader(gradient: &Gradient, bounds: SkRect) -> Option<Shader<'static>> {
    let stops: Vec<SkGradientStop> = gradient
        .stops
        .iter()
        .map(|stop| {
            SkGradientStop::new(
                stop.position.clamp(0.0, 1.0),
                Color::from_rgba8(stop.color.r, stop.color.g, stop.color.b, stop.color.a),
            )
        })
        .collect();
    if stops.len() < 2 {
        return None;
    }
    let (cx, cy) = (
        bounds.x() + bounds.width() / 2.0,
        bounds.y() + bounds.height() / 2.0,
    );
    match gradient.kind {
        GradientKind::Linear { angle_deg } => {
            let radians = angle_deg.to_radians();
            let (dx, dy) = (radians.cos(), radians.sin());
            // Half-extent of the box projected onto the sweep direction.
            let half = (bounds.width() / 2.0 * dx).abs() + (bounds.height() / 2.0 * dy).abs();
            let half = half.max(0.5);
            let start = SkPoint::from_xy(cx - dx * half, cy - dy * half);
            let end = SkPoint::from_xy(cx + dx * half, cy + dy * half);
            LinearGradient::new(start, end, stops, SpreadMode::Pad, Transform::identity())
        }
        GradientKind::Radial => {
            let radius = (bounds.width().max(bounds.height()) / 2.0).max(0.5);
            RadialGradient::new(
                SkPoint::from_xy(cx, cy),
                SkPoint::from_xy(cx, cy),
                radius,
                stops,
                SpreadMode::Pad,
                Transform::identity(),
            )
        }
    }
}

/// The dash on/off array (device pixels) for a preset dash style at `width`, or
/// `None` for a solid line. Ratios follow the OOXML preset dash conventions,
/// scaled by the line width (with a floor so a hairline still dashes).
fn dash_pattern(dash: DashStyle, width: f32) -> Option<StrokeDash> {
    let unit = width.max(1.0);
    let ratios: &[f32] = match dash {
        DashStyle::Solid => return None,
        DashStyle::Dot | DashStyle::SystemDot => &[1.0, 3.0],
        DashStyle::Dash => &[4.0, 3.0],
        DashStyle::LargeDash => &[8.0, 3.0],
        DashStyle::DashDot => &[4.0, 3.0, 1.0, 3.0],
        DashStyle::LargeDashDot => &[8.0, 3.0, 1.0, 3.0],
        DashStyle::LargeDashDotDot => &[8.0, 3.0, 1.0, 3.0, 1.0, 3.0],
        DashStyle::SystemDash => &[3.0, 1.0],
        DashStyle::SystemDashDot => &[3.0, 1.0, 1.0, 1.0],
        DashStyle::SystemDashDotDot => &[3.0, 1.0, 1.0, 1.0, 1.0, 1.0],
    };
    let array: Vec<f32> = ratios.iter().map(|r| (r * unit).max(0.1)).collect();
    StrokeDash::new(array, 0.0)
}

/// Draws a filled arrowhead at `tip`, oriented along the direction `tip - other`
/// (the segment pointing outward from the shape toward `tip`), sized by the
/// line-end kind and its width/length size tokens (relative to the line width).
fn draw_arrowhead(
    surface: &mut Surface,
    tip: SkPoint,
    other: SkPoint,
    end: &LineEnd,
    width: f32,
    color: DisplayColor,
    clip: Option<&Mask>,
) {
    if matches!(end.kind, LineEndKind::None) {
        return;
    }
    let (dx, dy) = (tip.x - other.x, tip.y - other.y);
    let len = (dx * dx + dy * dy).sqrt();
    if len <= f32::EPSILON {
        return;
    }
    // Unit vectors along the segment (`ux`) and perpendicular to it (`px`).
    let (ux, uy) = (dx / len, dy / len);
    let (px, py) = (-uy, ux);
    let unit = width.max(1.0);
    let half_w = unit * size_factor(end.width) * 1.5;
    let length = unit * size_factor(end.length) * 3.0;
    // The base sits `length` back from the tip along the segment.
    let base = SkPoint::from_xy(tip.x - ux * length, tip.y - uy * length);
    let left = SkPoint::from_xy(base.x + px * half_w, base.y + py * half_w);
    let right = SkPoint::from_xy(base.x - px * half_w, base.y - py * half_w);

    let mut builder = PathBuilder::new();
    match end.kind {
        LineEndKind::Triangle | LineEndKind::Arrow => {
            builder.move_to(tip.x, tip.y);
            builder.line_to(left.x, left.y);
            builder.line_to(right.x, right.y);
            builder.close();
        }
        LineEndKind::Stealth => {
            // A concave "stealth" arrow: the base notches in toward the tip.
            let notch = SkPoint::from_xy(tip.x - ux * length * 0.6, tip.y - uy * length * 0.6);
            builder.move_to(tip.x, tip.y);
            builder.line_to(left.x, left.y);
            builder.line_to(notch.x, notch.y);
            builder.line_to(right.x, right.y);
            builder.close();
        }
        LineEndKind::Diamond => {
            let far = SkPoint::from_xy(tip.x - ux * length * 2.0, tip.y - uy * length * 2.0);
            builder.move_to(tip.x, tip.y);
            builder.line_to(left.x, left.y);
            builder.line_to(far.x, far.y);
            builder.line_to(right.x, right.y);
            builder.close();
        }
        LineEndKind::Oval => {
            let radius = half_w.max(length / 2.0);
            let center = SkPoint::from_xy(tip.x - ux * radius, tip.y - uy * radius);
            if let Some(rect) = SkRect::from_xywh(
                center.x - radius,
                center.y - radius,
                radius * 2.0,
                radius * 2.0,
            ) && let Some(oval) = PathBuilder::from_oval(rect)
            {
                fill_solid(surface, &oval, color, clip);
            }
            return;
        }
        LineEndKind::None => return,
    }
    if let Some(path) = builder.finish() {
        fill_solid(surface, &path, color, clip);
    }
}

/// Fills `path` with a flat color (used for arrowheads).
fn fill_solid(
    surface: &mut Surface,
    path: &tiny_skia::Path,
    color: DisplayColor,
    clip: Option<&Mask>,
) {
    let mut paint = Paint::default();
    paint.set_color_rgba8(color.r, color.g, color.b, color.a);
    paint.anti_alias = true;
    surface
        .pixmap
        .fill_path(path, &paint, FillRule::Winding, Transform::identity(), clip);
}

/// The size multiplier for a line-end width/length token (default medium).
fn size_factor(size: Option<LineEndSize>) -> f32 {
    match size {
        Some(LineEndSize::Small) => 0.75,
        None | Some(LineEndSize::Medium) => 1.0,
        Some(LineEndSize::Large) => 1.5,
    }
}

/// A [`GlyphSource`] backed by a single font blob for `FontId(0)` — the current
/// default face until the font resolver (`P1C-002`) supplies the full set.
#[derive(Clone, Copy, Debug)]
pub struct SingleFontSource<'a> {
    data: &'a [u8],
}

impl<'a> SingleFontSource<'a> {
    /// Wraps font bytes served for `FontId(0)`.
    #[must_use]
    pub fn new(data: &'a [u8]) -> Self {
        Self { data }
    }
}

impl GlyphSource for SingleFontSource<'_> {
    fn font_data(&self, font: FontId) -> Option<&[u8]> {
        (font == FontId(0)).then_some(self.data)
    }
}

/// A [`GlyphSource`] over the layout crate's bundled face set — serves the right
/// Roboto face (regular/bold/italic/bold-italic) for a [`FontId`], matching the
/// face the shaper resolved.
#[derive(Clone, Copy, Debug, Default)]
pub struct BundledFontSource;

impl GlyphSource for BundledFontSource {
    fn font_data(&self, font: FontId) -> Option<&[u8]> {
        Some(casual_doc_layout::fonts::face_bytes(font))
    }
}

/// A [`GlyphSource`] over both the bundled faces *and* the shaper's dynamic
/// [`FontRegistry`] — the seam that lets the renderer rasterize the same
/// system-resolved (native `system-fonts`) or host-registered (e.g. browser
/// network-fetched Noto CJK) faces the shaper shaped with. A bundled [`FontId`] is
/// served from the bundled table; a dynamic id (a `.notdef`-avoiding fallback the
/// shaper interned) is served, with its face index, from a snapshot of the
/// registry taken at construction.
#[derive(Clone, Debug, Default)]
pub struct RegistryFontSource {
    /// `FontId.0` → the interned face, snapshotted from the registry.
    dynamic: HashMap<u32, DynFace>,
}

impl RegistryFontSource {
    /// Snapshots `registry`'s dynamic faces so this source can serve their bytes.
    /// Take it *after* shaping (or the render pass), when every fallback face the
    /// document needs has been interned.
    #[must_use]
    pub fn new(registry: &FontRegistry) -> Self {
        let dynamic = registry
            .snapshot()
            .into_iter()
            .map(|(id, face)| (id.0, face))
            .collect();
        Self { dynamic }
    }
}

impl GlyphSource for RegistryFontSource {
    fn font_data(&self, font: FontId) -> Option<&[u8]> {
        // A dynamic (system/host) face is served from the snapshot; anything else
        // is a bundled id served from the bundled table (Roboto for an unknown id).
        if let Some(face) = self.dynamic.get(&font.0) {
            Some(face.bytes.as_slice())
        } else {
            Some(casual_doc_layout::fonts::face_bytes(font))
        }
    }

    fn face_index(&self, font: FontId) -> u32 {
        self.dynamic.get(&font.0).map_or(0, |face| face.index)
    }
}

/// A [`MediaSource`] that serves no media — every lookup misses, so inline images
/// render nothing. The default for callers with no embedded pictures.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoMediaSource;

impl MediaSource for NoMediaSource {
    fn media_bytes(&self, _media: &str) -> Option<&[u8]> {
        None
    }
}

/// A [`MediaSource`] backed by an in-memory map from media key (package part name)
/// to encoded bytes — built by the pipeline/example from the package's
/// `word/media` parts (mirroring how fonts are served).
#[derive(Clone, Debug, Default)]
pub struct MapMediaSource {
    parts: HashMap<String, Vec<u8>>,
}

impl MapMediaSource {
    /// An empty media source.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers the encoded bytes for a media key (its package part name).
    pub fn insert(&mut self, media: impl Into<String>, bytes: Vec<u8>) {
        self.parts.insert(media.into(), bytes);
    }
}

impl MediaSource for MapMediaSource {
    fn media_bytes(&self, media: &str) -> Option<&[u8]> {
        self.parts.get(media).map(Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use casual_doc_layout::compose::compose_paragraph;
    use casual_doc_layout::fonts::ROBOTO_REGULAR;
    use casual_doc_layout::model::{ModelPos, ModelRange};
    use casual_doc_layout::shape::ParleyShaper;
    use casual_doc_layout::text::{Decoration, LineConstraints, LineShaper, StyledRun};
    use casual_doc_layout::units::{Point, Twip};
    use casual_doc_model::NodeId;

    fn dark_pixel_count(surface: &Surface) -> usize {
        // Count clearly non-white pixels (glyph ink).
        surface
            .data()
            .chunks_exact(4)
            .filter(|px| px[0] < 200 || px[1] < 200 || px[2] < 200)
            .count()
    }

    #[test]
    fn renders_shaped_text_to_non_blank_pixels() {
        // End to end: shape a paragraph, compose a display list, rasterize it,
        // and confirm real glyph ink landed on the page (not a blank canvas).
        let shaper = ParleyShaper::new();
        let node = NodeId::from_parts(1, 1).unwrap();
        let layout = shaper.shape_paragraph(
            &[StyledRun {
                text: "Hello, opendoc!".into(),
                requested_family: None,
                font: FontId(0),
                size: Twip::from_points(24),
                character_scale_percent: 100,
                bold: false,
                italic: false,
                letter_spacing: Twip::ZERO,
                color: [0, 0, 0, 255],
                decoration: Decoration::default(),
                highlight: None,
                shading: None,
                baseline_shift: Twip::ZERO,
            }],
            LineConstraints {
                max_width: Twip::from_points(500),
                ..LineConstraints::default()
            },
            ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 0)),
        );
        let list = compose_paragraph(
            &layout,
            Point::new(Twip::from_points(12), Twip::from_points(24)),
        );

        let mut surface = Surface::new(400, 100).unwrap();
        let fonts = SingleFontSource::new(ROBOTO_REGULAR);
        render(&list, &mut surface, 96.0, &fonts, &NoMediaSource);

        let ink = dark_pixel_count(&surface);
        assert!(
            ink > 200,
            "shaped text rasterized to glyph ink (got {ink} dark px)"
        );
        // The PNG encodes.
        assert!(!surface.encode_png().unwrap().is_empty());
    }

    #[test]
    fn unknown_font_is_skipped_not_panicked() {
        let mut surface = Surface::new(50, 50).unwrap();
        let run = GlyphRun {
            font: FontId(9),
            size: Twip::from_points(12),
            character_scale_percent: 100,
            color: [0, 0, 0, 255],
            origin: Point::new(Twip::ZERO, Twip::from_points(12)),
            bidi_level: 0,
            decoration: Decoration::default(),
            highlight: None,
            shading: None,
            glyphs: vec![casual_doc_layout::text::Glyph {
                id: 5,
                advance: Twip::from_points(6),
                cluster: 0,
            }],
            is_marker: false,
        };
        let mut list = DisplayList::new();
        list.push(PaintItem::Glyphs { run });
        let fonts = SingleFontSource::new(ROBOTO_REGULAR); // only serves FontId(0)
        render(&list, &mut surface, 96.0, &fonts, &NoMediaSource); // FontId(9) unknown -> skipped
        assert_eq!(dark_pixel_count(&surface), 0, "unknown font paints nothing");
    }

    #[test]
    fn a_push_clip_prevents_painting_outside_the_clip_rect() {
        use casual_doc_layout::display::Color as DisplayColor;
        use casual_doc_layout::units::{Rect, Size};

        // At 1440 dpi, one twip maps to exactly one device pixel, so the clip and
        // fill rects are stated directly in pixels.
        let dpi = 1440.0;
        let mut surface = Surface::new(100, 100).unwrap();

        let clip = Rect::new(Point::new(Twip(0), Twip(0)), Size::new(Twip(40), Twip(40)));
        let full = Rect::new(
            Point::new(Twip(0), Twip(0)),
            Size::new(Twip(100), Twip(100)),
        );

        // A full-page black fill, clipped to the top-left 40x40 rect: content
        // beyond the clip (as an `exact`-height row emits) must not be drawn.
        let mut list = DisplayList::new();
        list.push(PaintItem::PushClip(clip));
        list.push(PaintItem::Rect {
            rect: full,
            fill: Some(DisplayColor::BLACK),
            stroke: None,
        });
        list.push(PaintItem::PopClip);

        let fonts = SingleFontSource::new(ROBOTO_REGULAR);
        render(&list, &mut surface, dpi, &fonts, &NoMediaSource);

        let pixel = |x: usize, y: usize| {
            let i = (y * 100 + x) * 4;
            let px = &surface.data()[i..i + 4];
            [px[0], px[1], px[2], px[3]]
        };
        // Inside the clip: painted black.
        let inside = pixel(10, 10);
        assert!(
            inside[0] < 40 && inside[1] < 40 && inside[2] < 40,
            "content inside the clip is painted (got {inside:?})"
        );
        // Outside the clip but inside the fill rect: left as background (white).
        assert_eq!(
            pixel(60, 60),
            [255, 255, 255, 255],
            "content beyond the clip rect is not drawn"
        );
    }

    #[test]
    fn a_popped_clip_no_longer_restricts_painting() {
        use casual_doc_layout::display::Color as DisplayColor;
        use casual_doc_layout::units::{Rect, Size};

        let dpi = 1440.0;
        let mut surface = Surface::new(100, 100).unwrap();
        let clip = Rect::new(Point::new(Twip(0), Twip(0)), Size::new(Twip(40), Twip(40)));
        let full = Rect::new(
            Point::new(Twip(0), Twip(0)),
            Size::new(Twip(100), Twip(100)),
        );

        // After the clip is popped, a subsequent fill paints unrestricted.
        let mut list = DisplayList::new();
        list.push(PaintItem::PushClip(clip));
        list.push(PaintItem::PopClip);
        list.push(PaintItem::Rect {
            rect: full,
            fill: Some(DisplayColor::BLACK),
            stroke: None,
        });

        let fonts = SingleFontSource::new(ROBOTO_REGULAR);
        render(&list, &mut surface, dpi, &fonts, &NoMediaSource);

        let i = (60 * 100 + 60) * 4;
        let px = &surface.data()[i..i + 4];
        assert!(
            px[0] < 40 && px[1] < 40 && px[2] < 40,
            "with the clip popped, the whole rect is painted (got {:?})",
            &px[..4]
        );
    }

    #[test]
    fn ellipse_and_rounded_rectangle_leave_their_bounding_corners_unpainted() {
        use casual_doc_layout::display::Color as DisplayColor;
        use casual_doc_layout::units::{Rect, Size};

        // At 1440 dpi, one twip is one device pixel, making the geometry and
        // sampled pixels exact and platform-independent.
        let dpi = 1440.0;
        let mut list = DisplayList::new();
        list.push(PaintItem::Ellipse {
            rect: Rect::new(
                Point::new(Twip(10), Twip(10)),
                Size::new(Twip(40), Twip(40)),
            ),
            fill: Some(DisplayColor::rgb(200, 20, 20)),
            stroke: None,
        });
        list.push(PaintItem::RoundedRect {
            rect: Rect::new(
                Point::new(Twip(70), Twip(10)),
                Size::new(Twip(40), Twip(40)),
            ),
            radius: Twip(12),
            fill: Some(DisplayColor::rgb(20, 80, 200)),
            stroke: None,
        });

        let mut surface = Surface::new(120, 60).unwrap();
        render(
            &list,
            &mut surface,
            dpi,
            &SingleFontSource::new(ROBOTO_REGULAR),
            &NoMediaSource,
        );

        assert_eq!(
            pixel_at(&surface, 120, 10, 10),
            [255, 255, 255, 255],
            "an ellipse does not fill its rectangular corner"
        );
        assert_eq!(pixel_at(&surface, 120, 30, 30), [200, 20, 20, 255]);
        assert_eq!(
            pixel_at(&surface, 120, 70, 10),
            [255, 255, 255, 255],
            "a rounded rectangle does not fill its rectangular corner"
        );
        assert_eq!(pixel_at(&surface, 120, 90, 30), [20, 80, 200, 255]);
    }

    #[test]
    fn angular_polygons_leave_bounding_corners_unpainted() {
        use casual_doc_layout::display::Color as DisplayColor;

        let dpi = 1440.0;
        let mut list = DisplayList::new();
        for (points, color) in [
            (
                vec![
                    Point::new(Twip(30), Twip(10)),
                    Point::new(Twip(50), Twip(50)),
                    Point::new(Twip(10), Twip(50)),
                ],
                DisplayColor::rgb(200, 20, 20),
            ),
            (
                vec![
                    Point::new(Twip(70), Twip(10)),
                    Point::new(Twip(110), Twip(50)),
                    Point::new(Twip(70), Twip(50)),
                ],
                DisplayColor::rgb(20, 160, 60),
            ),
            (
                vec![
                    Point::new(Twip(150), Twip(10)),
                    Point::new(Twip(170), Twip(30)),
                    Point::new(Twip(150), Twip(50)),
                    Point::new(Twip(130), Twip(30)),
                ],
                DisplayColor::rgb(20, 80, 200),
            ),
        ] {
            list.push(PaintItem::Polygon {
                points,
                fill: Some(color),
                stroke: None,
            });
        }

        let mut surface = Surface::new(180, 60).unwrap();
        render(
            &list,
            &mut surface,
            dpi,
            &SingleFontSource::new(ROBOTO_REGULAR),
            &NoMediaSource,
        );

        for (x, y) in [(10, 10), (110, 10), (130, 10)] {
            assert_eq!(
                pixel_at(&surface, 180, x, y),
                [255, 255, 255, 255],
                "polygon corner ({x}, {y}) stays outside the silhouette"
            );
        }
        assert_eq!(pixel_at(&surface, 180, 30, 35), [200, 20, 20, 255]);
        assert_eq!(pixel_at(&surface, 180, 80, 35), [20, 160, 60, 255]);
        assert_eq!(pixel_at(&surface, 180, 150, 30), [20, 80, 200, 255]);
    }

    /// Surface dimensions used by the decoration tests.
    const DECO_W: u32 = 300;
    const DECO_H: u32 = 140;

    /// Renders "Hello" (no descenders below the baseline) with the given
    /// decoration and returns the surface together with the glyph baseline row (in
    /// device pixels — parley's baseline sits an ascent below the paragraph top).
    fn render_decorated(decoration: Decoration) -> (Surface, usize) {
        let shaper = ParleyShaper::new();
        let node = NodeId::from_parts(1, 1).unwrap();
        let origin = Point::new(Twip::from_points(6), Twip::from_points(20));
        let layout = shaper.shape_paragraph(
            &[StyledRun {
                text: "Hello".into(),
                requested_family: None,
                font: FontId(0),
                size: Twip::from_points(24),
                character_scale_percent: 100,
                bold: false,
                italic: false,
                letter_spacing: Twip::ZERO,
                color: [0, 0, 0, 255],
                decoration,
                highlight: None,
                shading: None,
                baseline_shift: Twip::ZERO,
            }],
            LineConstraints {
                max_width: Twip::from_points(500),
                ..LineConstraints::default()
            },
            ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 0)),
        );
        let baseline_twips = origin.y + layout.lines[0].runs[0].origin.y;
        let baseline_px = baseline_twips.to_device_px(96.0).round() as usize;
        let list = compose_paragraph(&layout, origin);
        let mut surface = Surface::new(DECO_W, DECO_H).unwrap();
        render(
            &list,
            &mut surface,
            96.0,
            &SingleFontSource::new(ROBOTO_REGULAR),
            &NoMediaSource,
        );
        (surface, baseline_px)
    }

    /// The number of dark pixels in the horizontal band `[y0, y1)`.
    fn band_dark(surface: &Surface, y0: usize, y1: usize) -> usize {
        let mut count = 0;
        for y in y0..y1 {
            for x in 0..DECO_W as usize {
                let i = (y * DECO_W as usize + x) * 4;
                let px = &surface.data()[i..i + 4];
                if px[0] < 200 || px[1] < 200 || px[2] < 200 {
                    count += 1;
                }
            }
        }
        count
    }

    /// The widest single-row dark-pixel count within the band `[y0, y1)` — a solid
    /// decoration line produces a row far denser than sparse glyph ink.
    fn max_row_dark(surface: &Surface, y0: usize, y1: usize) -> usize {
        (y0..y1)
            .map(|y| band_dark(surface, y, y + 1))
            .max()
            .unwrap_or(0)
    }

    #[test]
    fn underline_draws_a_line_below_the_baseline() {
        // The band a few pixels below the baseline is empty for plain "Hello"
        // (it has no descenders) but carries the underline when set.
        let (plain, baseline) = render_decorated(Decoration::default());
        let (underlined, _) = render_decorated(Decoration {
            underline: true,
            strikethrough: false,
            double_strike: false,
            underline_color: None,
            underline_style: casual_doc_model::v1::UnderlineStyle::Single,
        });
        let plain_below = band_dark(&plain, baseline + 2, baseline + 10);
        let under_below = band_dark(&underlined, baseline + 2, baseline + 10);
        assert_eq!(
            plain_below, 0,
            "a plain run paints nothing below the baseline"
        );
        assert!(
            under_below > 20,
            "the underline paints a line below the baseline (got {under_below} px)"
        );
    }

    #[test]
    fn strikethrough_draws_a_line_through_the_middle() {
        // The strike sits above the baseline, through the glyph mid; it makes one
        // row far denser than any row of the plain glyphs, and adds net ink.
        let (plain, baseline) = render_decorated(Decoration::default());
        let (struck, _) = render_decorated(Decoration {
            underline: false,
            strikethrough: true,
            double_strike: false,
            underline_color: None,
            underline_style: casual_doc_model::v1::UnderlineStyle::Single,
        });
        // Search the mid-band between the baseline and ~cap height above it.
        let (y0, y1) = (baseline.saturating_sub(20), baseline);
        let plain_row = max_row_dark(&plain, y0, y1);
        let struck_row = max_row_dark(&struck, y0, y1);
        assert!(
            struck_row > plain_row + 30,
            "strikethrough makes one dense horizontal row (struck {struck_row} vs plain {plain_row})"
        );
        assert!(
            dark_pixel_count(&struck) > dark_pixel_count(&plain),
            "the strike adds net ink over the plain glyphs"
        );
    }

    #[test]
    fn double_strike_draws_two_lines_more_ink_than_a_single_strike() {
        // `w:dstrike` draws two parallel lines straddling the strike position, so
        // it adds more ink than a single strike over the same glyphs.
        let (single, _) = render_decorated(Decoration {
            underline: false,
            strikethrough: true,
            double_strike: false,
            underline_color: None,
            underline_style: casual_doc_model::v1::UnderlineStyle::Single,
        });
        let (double, _) = render_decorated(Decoration {
            underline: false,
            strikethrough: false,
            double_strike: true,
            underline_color: None,
            underline_style: casual_doc_model::v1::UnderlineStyle::Single,
        });
        assert!(
            dark_pixel_count(&double) > dark_pixel_count(&single),
            "double strike ({}) lays down more ink than a single strike ({})",
            dark_pixel_count(&double),
            dark_pixel_count(&single),
        );
    }

    #[test]
    fn a_double_underline_draws_more_ink_than_a_single_underline() {
        use casual_doc_layout::text::Decoration;
        let single = render_decorated(Decoration {
            underline: true,
            ..Decoration::default()
        })
        .0;
        let double = render_decorated(Decoration {
            underline: true,
            underline_style: casual_doc_model::v1::UnderlineStyle::Double,
            ..Decoration::default()
        })
        .0;
        assert!(
            dark_pixel_count(&double) > dark_pixel_count(&single),
            "the double underline ({}) lays down more ink than a single ({})",
            dark_pixel_count(&double),
            dark_pixel_count(&single),
        );
    }

    #[test]
    fn a_plain_run_draws_no_decoration_line() {
        // A plain run has no ink below the baseline (no underline, no descenders).
        let (plain, baseline) = render_decorated(Decoration::default());
        assert_eq!(
            band_dark(&plain, baseline + 2, baseline + 12),
            0,
            "a plain run has no underline"
        );
    }

    /// Encodes a solid `w×h` image of `rgba` to `format` bytes, in memory.
    fn solid_image(w: u32, h: u32, rgba: [u8; 4], format: image::ImageFormat) -> Vec<u8> {
        let mut img = image::RgbaImage::new(w, h);
        for pixel in img.pixels_mut() {
            *pixel = image::Rgba(rgba);
        }
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut buf, format)
            .unwrap();
        buf.into_inner()
    }

    /// Reads a surface pixel `[r, g, b, a]` (surface is `w`-wide).
    fn pixel_at(surface: &Surface, w: usize, x: usize, y: usize) -> [u8; 4] {
        let i = (y * w + x) * 4;
        let d = &surface.data()[i..i + 4];
        [d[0], d[1], d[2], d[3]]
    }

    /// Renders a single `Image` paint item of `media_bytes` (keyed `"pic"`) into a
    /// 20×20-twip box at (10,10) on a 50×50 surface at 1440 dpi (1 twip = 1 px), so
    /// the box lands at device px [10,30)². Returns the surface.
    fn render_one_image(media_bytes: Vec<u8>) -> Surface {
        use casual_doc_layout::units::Size;
        let mut media = MapMediaSource::new();
        media.insert("pic", media_bytes);
        let rect = Rect::new(
            Point::new(Twip(10), Twip(10)),
            Size::new(Twip(20), Twip(20)),
        );
        let mut list = DisplayList::new();
        list.push(PaintItem::Image {
            media: "pic".to_owned(),
            rect,
            crop: None,
            transform: None,
        });
        let mut surface = Surface::new(50, 50).unwrap();
        render(&list, &mut surface, 1440.0, &BundledFontSource, &media);
        surface
    }

    /// Renders one `Image` paint item with a `crop` into the same box as
    /// [`render_one_image`], so a test can compare cropped vs uncropped pixels.
    fn render_one_image_cropped(media_bytes: Vec<u8>, crop: Option<CropRect>) -> Surface {
        use casual_doc_layout::units::Size;
        let mut media = MapMediaSource::new();
        media.insert("pic", media_bytes);
        let rect = Rect::new(
            Point::new(Twip(10), Twip(10)),
            Size::new(Twip(20), Twip(20)),
        );
        let mut list = DisplayList::new();
        list.push(PaintItem::Image {
            media: "pic".to_owned(),
            rect,
            crop,
            transform: None,
        });
        let mut surface = Surface::new(50, 50).unwrap();
        render(&list, &mut surface, 1440.0, &BundledFontSource, &media);
        surface
    }

    /// An `w×h` PNG whose left half is `left` and right half is `right`.
    fn split_image(w: u32, h: u32, left: [u8; 4], right: [u8; 4]) -> Vec<u8> {
        let mut img = image::RgbaImage::new(w, h);
        for (x, _y, pixel) in img.enumerate_pixels_mut() {
            *pixel = image::Rgba(if x < w / 2 { left } else { right });
        }
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    }

    #[test]
    fn a_crop_selects_only_the_visible_source_region() {
        // A 16×16 PNG: red left half, blue right half. Cropping away the left 50%
        // (`a:srcRect@l=50000`) must fill the whole box with the blue right half;
        // uncropped, the box shows both colors.
        let red = [220, 30, 30, 255];
        let blue = [30, 30, 220, 255];
        let bytes = split_image(16, 16, red, blue);

        let cropped = render_one_image_cropped(
            bytes.clone(),
            Some(CropRect {
                left: CROP_FULL / 2,
                top: 0,
                right: 0,
                bottom: 0,
            }),
        );
        // Sample near the left and right of the box (device px [10,30)²).
        let left_px = pixel_at(&cropped, 50, 13, 20);
        let right_px = pixel_at(&cropped, 50, 27, 20);
        assert!(
            left_px[2] > 150 && left_px[0] < 120,
            "the cropped box shows the blue right half at its left edge (got {left_px:?})"
        );
        assert!(
            right_px[2] > 150 && right_px[0] < 120,
            "and blue across to its right edge (got {right_px:?})"
        );

        // Uncropped, the same box's left edge is red, not blue — proving the crop
        // (not the box) changed which source pixels were sampled.
        let plain = render_one_image_cropped(bytes, None);
        let plain_left = pixel_at(&plain, 50, 13, 20);
        assert!(
            plain_left[0] > 150 && plain_left[2] < 120,
            "uncropped, the box's left edge shows the red left half (got {plain_left:?})"
        );
    }

    #[test]
    fn a_png_image_decodes_and_blits_pixels_into_its_rect() {
        // A solid-red 8×8 PNG, scaled into the 20×20-px box: ink inside, white out.
        let bytes = solid_image(8, 8, [210, 40, 40, 255], image::ImageFormat::Png);
        let surface = render_one_image(bytes);
        let inside = pixel_at(&surface, 50, 20, 20);
        assert!(
            inside[0] > 150 && inside[1] < 120 && inside[2] < 120,
            "the PNG's red ink lands inside the box (got {inside:?})"
        );
        assert_eq!(
            pixel_at(&surface, 50, 2, 2),
            [255, 255, 255, 255],
            "outside the image box stays background white"
        );
    }

    #[test]
    fn a_jpeg_image_decodes_and_blits_pixels_into_its_rect() {
        // JPEG is lossy, so assert a reddish, clearly non-white blit (not exact).
        let bytes = solid_image(16, 16, [200, 30, 30, 255], image::ImageFormat::Jpeg);
        let surface = render_one_image(bytes);
        let inside = pixel_at(&surface, 50, 20, 20);
        assert!(
            inside[0] > 130 && inside[0] > inside[2] + 40,
            "the JPEG's red ink lands inside the box (got {inside:?})"
        );
        assert_eq!(
            pixel_at(&surface, 50, 2, 2),
            [255, 255, 255, 255],
            "outside the image box stays background white"
        );
    }

    #[test]
    fn gif_bmp_tiff_images_decode_and_blit_pixels_into_their_rect() {
        // Office-authored DOCX commonly embeds these formats (pasted art, scanned
        // pages); they share the generic decode path and used to fall back to the
        // placeholder box. Each solid-red raster must now paint red ink inside the
        // box and leave the surrounding page white.
        for format in [
            image::ImageFormat::Gif,
            image::ImageFormat::Bmp,
            image::ImageFormat::Tiff,
        ] {
            let bytes = solid_image(8, 8, [210, 40, 40, 255], format);
            assert!(
                decode_to_pixmap(&bytes).is_some(),
                "{format:?} decodes to a pixmap"
            );
            let surface = render_one_image(bytes);
            let inside = pixel_at(&surface, 50, 20, 20);
            assert!(
                inside[0] > 150 && inside[0] > inside[2] + 40,
                "the {format:?} red ink lands inside the box (got {inside:?})"
            );
            assert_eq!(
                pixel_at(&surface, 50, 2, 2),
                [255, 255, 255, 255],
                "outside the {format:?} image box stays background white"
            );
        }
    }

    #[test]
    fn an_image_over_the_dimension_limit_is_rejected_before_decode_and_placeholdered() {
        // A very wide but shallow PNG keeps the regression fixture small while
        // proving that encoded byte size alone cannot admit an extreme raster:
        // the full decode never runs (the dimension check rejects it first),
        // but the bytes were present, so — like any other undecodable media —
        // it now paints the visible placeholder rather than a silent blank.
        let bytes = solid_image(32_769, 1, [200, 30, 30, 255], image::ImageFormat::Png);
        let surface = render_one_image(bytes);
        assert!(
            dark_pixel_count(&surface) > 0,
            "an image over the 32,768-pixel decode limit is rejected but still \
             placeholdered, not silently blank"
        );
    }

    #[test]
    fn decoded_image_dimension_and_pixel_boundaries_are_enforced() {
        assert!(image_dimensions_within_limits(32_768, 1));
        assert!(image_dimensions_within_limits(10_000, 10_000));
        assert!(!image_dimensions_within_limits(32_769, 1));
        assert!(!image_dimensions_within_limits(10_001, 10_000));
        assert!(!image_dimensions_within_limits(0, 1));
    }

    #[test]
    fn unknown_media_renders_nothing_and_does_not_panic() {
        // No source bytes at all (the media id isn't in the source): there is
        // genuinely nothing to show, so this stays a blank gap, unlike the
        // present-but-undecodable case below.
        use casual_doc_layout::units::Size;
        let rect = Rect::new(
            Point::new(Twip(10), Twip(10)),
            Size::new(Twip(20), Twip(20)),
        );
        let mut list = DisplayList::new();
        list.push(PaintItem::Image {
            media: "missing".to_owned(),
            rect,
            crop: None,
            transform: None,
        });
        let mut surface = Surface::new(50, 50).unwrap();
        render(
            &list,
            &mut surface,
            1440.0,
            &BundledFontSource,
            &NoMediaSource,
        );
        assert_eq!(
            dark_pixel_count(&surface),
            0,
            "unknown media (no bytes at all) paints nothing"
        );
    }

    #[test]
    fn undecodable_media_with_bytes_present_paints_a_visible_placeholder() {
        // Present but undecodable bytes (not PNG/JPEG, e.g. an EMF stub): the
        // box is fully known even though the content isn't, so a visible
        // placeholder is painted instead of a silent blank gap.
        use casual_doc_layout::units::Size;
        let rect = Rect::new(
            Point::new(Twip(10), Twip(10)),
            Size::new(Twip(20), Twip(20)),
        );
        let mut media = MapMediaSource::new();
        media.insert("junk", vec![1, 2, 3, 4, 5, 6, 7, 8]);
        let mut list = DisplayList::new();
        list.push(PaintItem::Image {
            media: "junk".to_owned(),
            rect,
            crop: None,
            transform: None,
        });
        let mut surface = Surface::new(50, 50).unwrap();
        render(&list, &mut surface, 1440.0, &BundledFontSource, &media);

        assert!(
            dark_pixel_count(&surface) > 0,
            "undecodable-but-present media paints a visible placeholder, not nothing"
        );
        // The rect is device px [10,30)^2 at this dpi; the placeholder's two
        // diagonals cross exactly at the box center.
        let center = pixel_at(&surface, 50, 20, 20);
        assert!(
            center[0] < 220 && center[1] < 220 && center[2] < 220,
            "the placeholder's diagonal cross lands on the box center (got {center:?})"
        );
        // Well outside the box: untouched background, same as before.
        assert_eq!(
            pixel_at(&surface, 50, 2, 2),
            [255, 255, 255, 255],
            "the placeholder stays within the image's own rect"
        );
    }

    /// End to end for the host-registered (populator 3) path: a font handed to the
    /// shaper at runtime gets a dynamic `FontId`, and the renderer rasterizes real
    /// ink for glyphs addressed by that id through the `RegistryFontSource` — the
    /// same seam a browser uses to feed network-fetched Noto CJK. Deterministic
    /// (no `system-fonts` feature needed).
    #[test]
    fn a_host_registered_face_rasterizes_through_the_registry_source() {
        use casual_doc_layout::font_registry::FontRegistry;

        let shaper = ParleyShaper::new();
        let host = shaper.register_font(ROBOTO_REGULAR.to_vec())[0];
        assert!(FontRegistry::is_dynamic(host), "host faces get dynamic ids");

        // Shape real text to obtain valid Roboto glyph ids, then re-address the run
        // at the host `FontId`: the registry source must fetch the host bytes.
        let node = NodeId::from_parts(1, 1).unwrap();
        let layout = shaper.shape_paragraph(
            &[StyledRun {
                text: "Hi".into(),
                requested_family: None,
                font: FontId(0),
                size: Twip::from_points(28),
                character_scale_percent: 100,
                bold: false,
                italic: false,
                letter_spacing: Twip::ZERO,
                color: [0, 0, 0, 255],
                decoration: Decoration::default(),
                highlight: None,
                shading: None,
                baseline_shift: Twip::ZERO,
            }],
            LineConstraints {
                max_width: Twip::from_points(500),
                ..LineConstraints::default()
            },
            ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 0)),
        );
        let mut list = DisplayList::new();
        for line_run in &layout.lines[0].runs {
            let mut run = line_run.clone();
            run.font = host;
            run.origin = Point::new(Twip::from_points(6), Twip::from_points(28));
            list.push(PaintItem::Glyphs { run });
        }

        let fonts = RegistryFontSource::new(&shaper.registry());
        let mut surface = Surface::new(200, 60).unwrap();
        render(&list, &mut surface, 96.0, &fonts, &NoMediaSource);
        assert!(
            dark_pixel_count(&surface) > 50,
            "the host-registered face rasterizes real ink via the registry source"
        );
    }

    /// End to end on the native path: a CJK run shapes to real (non-`.notdef`)
    /// glyphs via an OS fallback face, and rasterizes real ink — not the tofu box
    /// the bundled-only path produces. The native render crate enables the OS
    /// system-font source by default (see this crate's `Cargo.toml` target
    /// dependency), so this runs without an explicit feature flag; WASM is gated
    /// out. Whether the host has a CJK face is environment-dependent (a headless CI
    /// runner may have none); when no covering face is found the test returns early
    /// rather than failing, since there is then no real ink to observe (the
    /// coverage-gap behavior is asserted by the shaper's own
    /// `cjk_with_system_fonts_resolves_a_covering_face`).
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn cjk_renders_real_ink_with_system_fonts() {
        let shaper = ParleyShaper::new();
        let node = NodeId::from_parts(1, 1).unwrap();
        let layout = shaper.shape_paragraph(
            &[StyledRun {
                text: "中文字".into(),
                requested_family: None,
                font: FontId(0),
                size: Twip::from_points(32),
                character_scale_percent: 100,
                bold: false,
                italic: false,
                letter_spacing: Twip::ZERO,
                color: [0, 0, 0, 255],
                decoration: Decoration::default(),
                highlight: None,
                shading: None,
                baseline_shift: Twip::ZERO,
            }],
            LineConstraints {
                max_width: Twip::from_points(500),
                ..LineConstraints::default()
            },
            ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 0)),
        );
        let covered = layout.lines[0]
            .runs
            .iter()
            .flat_map(|r| &r.glyphs)
            .any(|g| g.id != 0);
        if !covered {
            // No CJK face installed in this environment: nothing real to rasterize.
            return;
        }
        let list = compose_paragraph(
            &layout,
            Point::new(Twip::from_points(6), Twip::from_points(32)),
        );
        let fonts = RegistryFontSource::new(&shaper.registry());
        let mut surface = Surface::new(240, 90).unwrap();
        render(&list, &mut surface, 96.0, &fonts, &NoMediaSource);
        assert!(
            dark_pixel_count(&surface) > 100,
            "the OS fallback face rasterizes real CJK ink (not blank / not tofu)"
        );
    }

    // --- Shape fill / outline / arrowhead rendering -----------------------------
    //
    // These sample exact device pixels at 1440 dpi (1 twip = 1 px), so the geometry
    // is platform-independent; they are still gated off the Windows CI job to match
    // the existing exact-geometry snapshot convention (shaping-free but conservative).

    use casual_doc_layout::display::{
        Color as ShapeColor, Fill as DisplayFill, Gradient, GradientKind, GradientStop,
        ShapeGeometry, ShapeOutline,
    };
    use casual_doc_layout::units::{Rect as UnitRect, Size};
    use casual_doc_model::v1::{DashStyle, LineEnd, LineEndKind};
    // Separate `use` line to minimize import-block merge conflicts.
    use casual_doc_layout::display::ShapeTransform;

    fn shape_surface(list: &DisplayList, w: u32, h: u32) -> Surface {
        let mut surface = Surface::new(w, h).unwrap();
        render(
            list,
            &mut surface,
            1440.0,
            &SingleFontSource::new(ROBOTO_REGULAR),
            &NoMediaSource,
        );
        surface
    }

    #[test]
    #[cfg_attr(target_os = "windows", ignore)]
    fn a_rotated_shape_paints_at_its_rotated_bounds() {
        // A 40x8 red rect at (30,10), rotated 90° about its center (50,14): it
        // becomes an 8x40 tall bar spanning x[46,54], y[-6,34]. A pixel deep in
        // the rotated (tall) bar is red; a pixel inside the UNrotated (wide) bar
        // but outside the rotated one is now background.
        let mut list = DisplayList::new();
        list.push(PaintItem::Shape {
            geometry: ShapeGeometry::Rect {
                rect: UnitRect::new(Point::new(Twip(30), Twip(10)), Size::new(Twip(40), Twip(8))),
            },
            fill: Some(DisplayFill::Solid(ShapeColor::rgb(220, 30, 30))),
            stroke: None,
            head_end: None,
            tail_end: None,
            transform: Some(ShapeTransform {
                rotation: 90 * 60_000,
                flip_h: false,
                flip_v: false,
                center: Point::new(Twip(50), Twip(14)),
            }),
        });
        let surface = shape_surface(&list, 100, 60);
        let inside_rotated = pixel_at(&surface, 100, 50, 30);
        assert!(
            inside_rotated[0] > 150 && inside_rotated[1] < 120 && inside_rotated[2] < 120,
            "the rotated bar paints red where only a 90°-rotated rect reaches (got {inside_rotated:?})"
        );
        let was_unrotated = pixel_at(&surface, 100, 30, 14);
        assert!(
            was_unrotated[0] > 200 && was_unrotated[1] > 200 && was_unrotated[2] > 200,
            "the unrotated footprint is now empty background (got {was_unrotated:?})"
        );
    }

    #[test]
    #[cfg_attr(target_os = "windows", ignore)]
    fn a_horizontally_flipped_image_swaps_left_and_right() {
        // A 16x16 PNG (red left, blue right) blitted into the 20x20 box at
        // (10,10), flipped horizontally about the box center (20,20): the box's
        // LEFT edge now shows blue and its RIGHT edge red.
        let red = [220, 30, 30, 255];
        let blue = [30, 30, 220, 255];
        let bytes = split_image(16, 16, red, blue);
        let mut media = MapMediaSource::new();
        media.insert("pic", bytes);
        let mut list = DisplayList::new();
        list.push(PaintItem::Image {
            media: "pic".to_owned(),
            rect: Rect::new(
                Point::new(Twip(10), Twip(10)),
                Size::new(Twip(20), Twip(20)),
            ),
            crop: None,
            transform: Some(ShapeTransform {
                rotation: 0,
                flip_h: true,
                flip_v: false,
                center: Point::new(Twip(20), Twip(20)),
            }),
        });
        let mut surface = Surface::new(50, 50).unwrap();
        render(&list, &mut surface, 1440.0, &BundledFontSource, &media);
        // The box spans device px [10,30)²; sample near its left and right edges.
        let left_px = pixel_at(&surface, 50, 13, 20);
        let right_px = pixel_at(&surface, 50, 27, 20);
        assert!(
            left_px[2] > 150 && left_px[0] < 120,
            "flipH shows the blue (originally right) half at the box's left edge (got {left_px:?})"
        );
        assert!(
            right_px[0] > 150 && right_px[2] < 120,
            "and the red (originally left) half at the box's right edge (got {right_px:?})"
        );
    }

    #[test]
    #[cfg_attr(target_os = "windows", ignore)]
    fn linear_gradient_fill_ramps_from_start_to_end_color() {
        // A 100x20 rect filled with a horizontal (angle 0°) red -> blue gradient.
        let mut list = DisplayList::new();
        list.push(PaintItem::Shape {
            geometry: ShapeGeometry::Rect {
                rect: UnitRect::new(Point::new(Twip(0), Twip(0)), Size::new(Twip(100), Twip(20))),
            },
            fill: Some(DisplayFill::Gradient(Gradient {
                stops: vec![
                    GradientStop {
                        position: 0.0,
                        color: ShapeColor::rgb(255, 0, 0),
                    },
                    GradientStop {
                        position: 1.0,
                        color: ShapeColor::rgb(0, 0, 255),
                    },
                ],
                kind: GradientKind::Linear { angle_deg: 0.0 },
            })),
            stroke: None,
            head_end: None,
            tail_end: None,
            transform: None,
        });
        let surface = shape_surface(&list, 100, 20);

        let left = pixel_at(&surface, 100, 4, 10);
        let right = pixel_at(&surface, 100, 95, 10);
        assert!(
            left[0] > 150 && left[2] < 100,
            "the gradient start is red (got {left:?})"
        );
        assert!(
            right[2] > 150 && right[0] < 100,
            "the gradient end is blue (got {right:?})"
        );
    }

    #[test]
    #[cfg_attr(target_os = "windows", ignore)]
    fn radial_gradient_fill_differs_center_from_edge() {
        // A 60x60 rect with a radial red-center -> blue-edge gradient.
        let mut list = DisplayList::new();
        list.push(PaintItem::Shape {
            geometry: ShapeGeometry::Rect {
                rect: UnitRect::new(Point::new(Twip(0), Twip(0)), Size::new(Twip(60), Twip(60))),
            },
            fill: Some(DisplayFill::Gradient(Gradient {
                stops: vec![
                    GradientStop {
                        position: 0.0,
                        color: ShapeColor::rgb(255, 0, 0),
                    },
                    GradientStop {
                        position: 1.0,
                        color: ShapeColor::rgb(0, 0, 255),
                    },
                ],
                kind: GradientKind::Radial,
            })),
            stroke: None,
            head_end: None,
            tail_end: None,
            transform: None,
        });
        let surface = shape_surface(&list, 60, 60);

        let center = pixel_at(&surface, 60, 30, 30);
        let edge = pixel_at(&surface, 60, 30, 2);
        assert!(center[0] > 150, "the radial center is red (got {center:?})");
        assert!(edge[2] > 150, "the radial edge is blue (got {edge:?})");
    }

    #[test]
    #[cfg_attr(target_os = "windows", ignore)]
    fn dashed_outline_leaves_gaps_a_solid_one_does_not() {
        // A horizontal line stroked with a `dash` pattern: it is painted where a dash
        // lands and blank in a gap, unlike a solid stroke.
        let line = |dash| {
            let mut list = DisplayList::new();
            list.push(PaintItem::Shape {
                geometry: ShapeGeometry::Line {
                    from: Point::new(Twip(0), Twip(10)),
                    to: Point::new(Twip(100), Twip(10)),
                },
                fill: None,
                stroke: Some(ShapeOutline {
                    color: ShapeColor::BLACK,
                    width: 2.0,
                    dash,
                }),
                head_end: None,
                tail_end: None,
                transform: None,
            });
            shape_surface(&list, 100, 20)
        };
        let dashed = line(DashStyle::Dash);
        let solid = line(DashStyle::Solid);

        // x=3 falls in the first "on" dash; x=11 falls in the first gap.
        assert!(
            pixel_at(&dashed, 100, 3, 10)[0] < 100,
            "the first dash is painted"
        );
        assert!(
            pixel_at(&dashed, 100, 11, 10)[0] > 200,
            "the first gap is blank"
        );
        assert!(
            pixel_at(&solid, 100, 11, 10)[0] < 100,
            "a solid stroke paints the same span with no gap"
        );
    }

    #[test]
    #[cfg_attr(target_os = "windows", ignore)]
    fn arrowhead_paints_beyond_the_thin_line_at_the_endpoint() {
        // A thin (2px) horizontal line with a large triangle tail arrowhead: the
        // arrowhead fills pixels off the line's own thickness near the endpoint.
        let mut list = DisplayList::new();
        list.push(PaintItem::Shape {
            geometry: ShapeGeometry::Line {
                from: Point::new(Twip(10), Twip(30)),
                to: Point::new(Twip(90), Twip(30)),
            },
            fill: None,
            stroke: Some(ShapeOutline {
                color: ShapeColor::BLACK,
                width: 2.0,
                dash: DashStyle::Solid,
            }),
            head_end: None,
            tail_end: Some(LineEnd {
                kind: LineEndKind::Triangle,
                width: Some(casual_doc_model::v1::LineEndSize::Large),
                length: Some(casual_doc_model::v1::LineEndSize::Large),
            }),
            transform: None,
        });
        let surface = shape_surface(&list, 100, 60);

        // A point inside the arrowhead triangle but above the 2px line body.
        assert!(
            pixel_at(&surface, 100, 85, 28)[0] < 100,
            "the arrowhead fills beyond the line thickness (got {:?})",
            pixel_at(&surface, 100, 85, 28)
        );
        // A control point well above the arrowhead stays blank.
        assert!(
            pixel_at(&surface, 100, 85, 18)[0] > 200,
            "outside the arrowhead stays blank"
        );
    }
}
