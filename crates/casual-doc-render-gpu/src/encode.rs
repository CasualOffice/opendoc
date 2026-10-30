//! The display list as `vello_common` drawing commands.
//!
//! One walk over a [`DisplayList`], in list order, onto a [`SparseCanvas`]: the
//! backend-neutral drawing surface both `vello_hybrid` (GPU) and `vello_cpu`
//! implement. The walk is the only place a paint item's meaning lives on this
//! side; the painters only rasterize.
//!
//! **Native families** — everything that costs time on a real page (`docs/architecture/179`
//! §3: glyphs are 85–97 % of a page's raster) — are encoded as vector commands:
//! outline glyph runs and their single/double/thick/patterned/wavy underlines
//! and strikes, the outline text effects, rectangles, ellipses, rounded
//! rectangles, polygons, lines, DrawingML shapes with solid or gradient fills and
//! dashed outlines, clips, and layers with a transform and a multiply blend.
//!
//! **Fallback families** are rasterized by the reference renderer itself into a
//! tile and composited as an image (module `tile`): a glyph run that holds a
//! colour (COLR/CBDT/sbix) glyph, a shape with arrowheads, and every picture.
//! They are rare on a page, and a tile is the reference's own pixels, so parity
//! for them is exact rather than "within tolerance".
//!
//! The geometry rules are the CPU raster's, copied rather than approximated:
//! the hairline snap on filled rectangles, unhinted outlines at the run's device
//! size, the face's own underline and strike metrics, the size-derived effect
//! offsets. Where this backend draws differently it says so at the site.
//!
//! **Complexity.** O(items + glyphs) per page, one pass, no lookup by id.

use casual_doc_layout::display::{
    Color as DisplayColor, DisplayList, Fill, Gradient, GradientKind, LayerBlend, PaintItem,
    PathCommand, ShapeGeometry, ShapeOutline, ShapeTransform,
};
use casual_doc_layout::text::{Glyph, GlyphRun};
use casual_doc_layout::units::{Point, Rect, Twip};
use casual_doc_model::v1::FillRule;
use casual_doc_model::v1::{DashStyle, UnderlineStyle};
use casual_doc_render::{GlyphSource, MediaSource};
use skrifa::instance::{LocationRef, Size};
use skrifa::metrics::Metrics;
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, GlyphId, MetadataProvider};
use vello_common::kurbo::{Affine, BezPath, Ellipse, Rect as KRect, Shape, Stroke};
use vello_common::paint::{Image, ImageSource, PaintType};
use vello_common::peniko::{
    self, BlendMode, Color, ColorStop, Compose, Extend, ImageQuality, ImageSampler, Mix,
};
use vello_common::pixmap::Pixmap;

use crate::tile::{self, Tile};

/// The drawing surface the encoder writes to: the subset of the `vello_common`
/// render-context API that `vello_hybrid::Scene` and `vello_cpu::RenderContext`
/// share. A painter implements it by forwarding.
pub trait SparseCanvas {
    /// Sets the transform for the following paths.
    fn set_transform(&mut self, transform: Affine);
    /// Sets the paint for the following fills and strokes.
    fn set_paint(&mut self, paint: PaintType);
    /// Sets the paint's own transform (an image's placement).
    fn set_paint_transform(&mut self, transform: Affine);
    /// Resets the paint transform to identity.
    fn reset_paint_transform(&mut self);
    /// Sets the stroke for [`SparseCanvas::stroke_path`].
    fn set_stroke(&mut self, stroke: Stroke);
    /// Sets the winding rule for the following fills (`vello_common`'s
    /// `peniko::Fill`). The encoder sets it around each even-odd fill and
    /// restores [`peniko::Fill::NonZero`] straight after, so every other fill
    /// and every clip path stays non-zero (`docs/182` G-2).
    fn set_fill_rule(&mut self, rule: peniko::Fill);
    /// Fills `path` with the current paint and fill rule.
    fn fill_path(&mut self, path: &BezPath);
    /// Strokes `path` with the current paint and stroke.
    fn stroke_path(&mut self, path: &BezPath);
    /// Fills `rect` with the current paint.
    fn fill_rect(&mut self, rect: &KRect);
    /// Opens a layer clipped to `path`.
    fn push_clip_layer(&mut self, path: &BezPath);
    /// Opens a layer composited with `blend`.
    fn push_blend_layer(&mut self, blend: BlendMode);
    /// Closes the most recent layer.
    fn pop_layer(&mut self);
    /// Makes `pixmap` drawable and returns its source. A GPU painter uploads it
    /// into its atlas; a CPU painter registers it.
    fn register_image(&mut self, pixmap: Pixmap) -> ImageSource;
}

/// What one encode did, for the benchmark and for a caller that wants to know
/// how much of a page went through the fallback.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EncodeStats {
    /// Paint items encoded as vector commands.
    pub native_items: usize,
    /// Paint items rasterized by the reference renderer into a tile.
    pub fallback_items: usize,
    /// Pixels in all fallback tiles.
    pub fallback_pixels: u64,
}

/// Encodes `list` at `dpi` onto `canvas`, whose size is `width × height` device
/// pixels. The page background is NOT painted here: the caller clears the
/// target to it, as `Surface::with_background` does on the CPU.
pub fn encode_display_list(
    list: &DisplayList,
    canvas: &mut dyn SparseCanvas,
    width: u32,
    height: u32,
    dpi: f32,
    fonts: &dyn GlyphSource,
    media: &dyn MediaSource,
) -> EncodeStats {
    let mut encoder = Encoder {
        canvas,
        width,
        height,
        dpi,
        fonts,
        media,
        transform: Affine::IDENTITY,
        stack: Vec::new(),
        stats: EncodeStats::default(),
    };
    for item in &list.items {
        encoder.item(item);
    }
    // An unbalanced list (a push with no pop) still closes every layer it opened:
    // the CPU raster treats the rest of the list as inside the bracket, and so
    // does this, up to the end.
    while let Some(open) = encoder.stack.pop() {
        encoder.close(open);
    }
    encoder.stats
}

/// One open bracket: what closing it undoes.
#[derive(Clone, Copy, Debug)]
enum Open {
    /// A clip layer.
    Clip,
    /// A transform/blend layer: whether a blend layer was pushed, and the
    /// transform to restore.
    Layer { pushed: bool, restore: Affine },
}

struct Encoder<'a> {
    canvas: &'a mut dyn SparseCanvas,
    width: u32,
    height: u32,
    dpi: f32,
    fonts: &'a dyn GlyphSource,
    media: &'a dyn MediaSource,
    transform: Affine,
    stack: Vec<Open>,
    stats: EncodeStats,
}

impl Encoder<'_> {
    fn px(&self, twip: Twip) -> f64 {
        f64::from(twip.to_device_px(self.dpi))
    }

    fn point(&self, point: Point) -> (f64, f64) {
        (self.px(point.x), self.px(point.y))
    }

    fn item(&mut self, item: &PaintItem) {
        match item {
            PaintItem::PushClip(rect) => {
                // The CPU clip is the hairline-snapped rect painted anti-aliased
                // into a mask; a clip layer of the same path is the same coverage.
                let path = self.rect_path(*rect).unwrap_or_default();
                // Inside a transformed layer the reference clips the upright
                // content and then moves the whole layer, which is the clip
                // path drawn through the same transform.
                self.canvas.set_transform(self.transform);
                self.canvas.push_clip_layer(&path);
                self.stack.push(Open::Clip);
            }
            PaintItem::PopClip => {
                // Only a CLIP closes here. A stray pop with no clip open is
                // ignored, as the CPU raster ignores a pop on an empty stack.
                if matches!(self.stack.last(), Some(Open::Clip)) {
                    self.stack.pop();
                    self.canvas.pop_layer();
                }
            }
            PaintItem::PushLayer { transform, blend } => {
                let pushed = matches!(blend, LayerBlend::Multiply);
                if pushed {
                    self.canvas
                        .push_blend_layer(BlendMode::new(Mix::Multiply, Compose::SrcOver));
                }
                let restore = self.transform;
                // The CPU raster paints the layer upright off screen and
                // composites it through the transform (bilinear). Transforming the
                // vectors instead is the same geometry, drawn sharper; the parity
                // tolerance covers the resampling difference.
                self.transform = restore * object_transform(transform.as_ref(), self.dpi);
                self.stack.push(Open::Layer { pushed, restore });
            }
            PaintItem::PopLayer => {
                if matches!(self.stack.last(), Some(Open::Layer { .. })) {
                    let open = self.stack.pop().expect("checked");
                    self.close(open);
                }
            }
            PaintItem::Rect { rect, fill, stroke } => {
                self.native();
                if let Some(path) = self.rect_path(*rect) {
                    self.paint_path(&path, fill.as_ref(), stroke.as_ref());
                }
            }
            PaintItem::Ellipse { rect, fill, stroke } => {
                self.native();
                if let Some(path) = self.ellipse_path(*rect) {
                    self.paint_path(&path, fill.as_ref(), stroke.as_ref());
                }
            }
            PaintItem::RoundedRect {
                rect,
                radius,
                fill,
                stroke,
            } => {
                self.native();
                if let Some(path) = self.rounded_rect_path(*rect, *radius) {
                    self.paint_path(&path, fill.as_ref(), stroke.as_ref());
                }
            }
            PaintItem::Polygon {
                points,
                fill,
                stroke,
            } => {
                self.native();
                if let Some(path) = self.polygon_path(points) {
                    self.paint_path(&path, fill.as_ref(), stroke.as_ref());
                }
            }
            PaintItem::Line { from, to, stroke } => {
                self.native();
                let mut path = BezPath::new();
                path.move_to(self.point(*from));
                path.line_to(self.point(*to));
                self.canvas.set_transform(self.transform);
                self.canvas.set_paint(solid(stroke.color).into());
                self.canvas.set_stroke(Stroke::new(f64::from(stroke.width)));
                self.canvas.stroke_path(&path);
            }
            PaintItem::Shape {
                geometry,
                fill,
                stroke,
                head_end,
                tail_end,
                transform,
            } => {
                let has_heads = stroke.is_some() && (head_end.is_some() || tail_end.is_some());
                if has_heads {
                    // Arrowheads are the reference's: the whole shape goes
                    // through the tile so the head sits on its own line.
                    self.fallback(item);
                } else {
                    self.native();
                    let local = object_transform(transform.as_ref(), self.dpi);
                    self.shape(geometry, fill.as_ref(), stroke.as_ref(), local);
                }
            }
            PaintItem::Glyphs { run } => {
                if self.run_needs_fallback(run) {
                    self.fallback(item);
                } else {
                    self.native();
                    self.glyph_run(run);
                }
            }
            // The blur is the reference renderer's (`casual_doc_render`'s
            // shadow raster), through the same per-item tile an image takes.
            PaintItem::Image { .. } | PaintItem::Shadow { .. } => self.fallback(item),
        }
    }

    fn close(&mut self, open: Open) {
        match open {
            Open::Clip => self.canvas.pop_layer(),
            Open::Layer { pushed, restore } => {
                if pushed {
                    self.canvas.pop_layer();
                }
                self.transform = restore;
            }
        }
    }

    fn native(&mut self) {
        self.stats.native_items += 1;
    }

    /// Rasterizes `item` through the reference renderer into a tile and draws
    /// the tile where it belongs.
    fn fallback(&mut self, item: &PaintItem) {
        self.stats.fallback_items += 1;
        let Some(Tile {
            pixmap,
            x,
            y,
            nearest,
        }) = tile::rasterize(
            item,
            self.width,
            self.height,
            self.dpi,
            self.fonts,
            self.media,
        )
        else {
            return;
        };
        let (w, h) = (f64::from(pixmap.width()), f64::from(pixmap.height()));
        self.stats.fallback_pixels += u64::from(pixmap.width()) * u64::from(pixmap.height());
        let source = self.canvas.register_image(pixmap);
        let image = Image {
            image: source,
            sampler: ImageSampler {
                x_extend: Extend::Pad,
                y_extend: Extend::Pad,
                // A tile placed on whole pixels is copied texel for texel; one
                // placed at a fractional offset (a zoom that is not a multiple
                // of 96 dpi) is resampled, as any image would be.
                quality: if nearest {
                    ImageQuality::Low
                } else {
                    ImageQuality::Medium
                },
                alpha: 1.0,
            },
        };
        self.canvas.set_transform(self.transform);
        self.canvas.set_paint(PaintType::Image(image));
        self.canvas.set_paint_transform(Affine::translate((x, y)));
        self.canvas.fill_rect(&KRect::new(x, y, x + w, y + h));
        self.canvas.reset_paint_transform();
    }

    fn paint_path(
        &mut self,
        path: &BezPath,
        fill: Option<&DisplayColor>,
        stroke: Option<&casual_doc_layout::display::Stroke>,
    ) {
        self.canvas.set_transform(self.transform);
        if let Some(color) = fill {
            self.canvas.set_paint(solid(*color).into());
            self.canvas.fill_path(path);
        }
        if let Some(stroke) = stroke {
            self.canvas.set_paint(solid(stroke.color).into());
            // The CPU stroke width is device pixels, unscaled by dpi.
            self.canvas.set_stroke(Stroke::new(f64::from(stroke.width)));
            self.canvas.stroke_path(path);
        }
    }

    fn shape(
        &mut self,
        geometry: &ShapeGeometry,
        fill: Option<&Fill>,
        stroke: Option<&ShapeOutline>,
        local: Affine,
    ) {
        // The one geometry that can state a rule: a `CustomGeometry` with
        // `FillRule::EvenOdd` (a PDF `f*` import, `docs/182` G-2). Every other
        // primitive is one contour, where the rules agree.
        let even_odd = matches!(
            geometry,
            ShapeGeometry::Path {
                fill_rule: FillRule::EvenOdd,
                ..
            }
        );
        let (path, bounds) = match geometry {
            ShapeGeometry::Rect { rect } => (self.rect_path(*rect), self.device_bounds(*rect)),
            ShapeGeometry::Ellipse { rect } => {
                (self.ellipse_path(*rect), self.device_bounds(*rect))
            }
            ShapeGeometry::RoundedRect { rect, radius } => (
                self.rounded_rect_path(*rect, *radius),
                self.device_bounds(*rect),
            ),
            ShapeGeometry::Path {
                commands, closed, ..
            } => (
                self.command_path(commands, *closed),
                self.command_path(commands, *closed)
                    .map(|path| path.bounding_box()),
            ),
            ShapeGeometry::Line { from, to } => {
                let mut path = BezPath::new();
                path.move_to(self.point(*from));
                path.line_to(self.point(*to));
                (Some(path), None)
            }
        };
        let Some(path) = path else {
            return;
        };
        self.canvas.set_transform(self.transform * local);
        if let Some(fill) = fill {
            let paint = match fill {
                Fill::Solid(color) => Some(PaintType::Solid(solid(*color))),
                Fill::Gradient(gradient) => bounds
                    .filter(|b| b.width() > 0.0 && b.height() > 0.0)
                    .and_then(|b| gradient_paint(gradient, b))
                    .or_else(|| {
                        gradient
                            .stops
                            .first()
                            .map(|stop| PaintType::Solid(solid(stop.color)))
                    }),
            };
            if let Some(paint) = paint {
                self.canvas.set_paint(paint);
                if even_odd {
                    self.canvas.set_fill_rule(peniko::Fill::EvenOdd);
                }
                self.canvas.fill_path(&path);
                if even_odd {
                    self.canvas.set_fill_rule(peniko::Fill::NonZero);
                }
            }
        }
        if let Some(stroke) = stroke {
            let width = f64::from(stroke.width.max(1.0));
            let mut kstroke = Stroke::new(width);
            if let Some(dashes) = dash_pattern(stroke.dash, width) {
                kstroke = kstroke.with_dashes(0.0, dashes);
            }
            self.canvas.set_paint(solid(stroke.color).into());
            self.canvas.set_stroke(kstroke);
            self.canvas.stroke_path(&path);
        }
    }

    /// Whether the reference must paint this run: a colour glyph (COLR, CBDT,
    /// sbix) anywhere in it. Checked per glyph, exactly as the CPU raster
    /// decides per glyph which path to take.
    fn run_needs_fallback(&self, run: &GlyphRun) -> bool {
        let Some(bytes) = self.fonts.font_data(run.font) else {
            return false;
        };
        let Ok(font) = FontRef::from_index(bytes, self.fonts.face_index(run.font)) else {
            return false;
        };
        let size = Size::new(run.size.to_device_px(self.dpi));
        let colour = font.color_glyphs();
        let strikes = font.bitmap_strikes();
        run.glyphs.iter().any(|glyph| {
            let id = GlyphId::new(glyph.id);
            colour.get(id).is_some() || strikes.glyph_for_size(size, id).is_some()
        })
    }

    fn glyph_run(&mut self, run: &GlyphRun) {
        let Some(bytes) = self.fonts.font_data(run.font) else {
            return;
        };
        let Ok(font) = FontRef::from_index(bytes, self.fonts.face_index(run.font)) else {
            return;
        };
        let outlines = font.outline_glyphs();
        let size_px = run.size.to_device_px(self.dpi);
        let scale_x = f64::from(run.character_scale_percent) / 100.0;
        let start_x = f64::from(run.origin.x.to_device_px(self.dpi));
        let baseline_y = f64::from(run.origin.y.to_device_px(self.dpi));
        let mut pen_x = start_x;
        let mut path = BezPath::new();
        for glyph in &run.glyphs {
            if let Some(outline) = outlines.get(GlyphId::new(glyph.id)) {
                let mut pen = KurboPen {
                    path: &mut path,
                    origin_x: pen_x,
                    baseline_y,
                    scale_x,
                };
                let settings = DrawSettings::unhinted(Size::new(size_px), LocationRef::default());
                let _ = outline.draw(settings, &mut pen);
            }
            pen_x += f64::from(glyph.advance.to_device_px(self.dpi));
        }

        self.canvas.set_transform(self.transform);
        if !path.elements().is_empty() {
            let effects = run.decoration.effects;
            let offset = f64::from((size_px / 24.0).max(0.5));
            let [r, g, b, a] = run.color;
            if effects.shadow {
                self.canvas
                    .set_paint(Color::from_rgba8(0, 0, 0, a / 2).into());
                self.canvas
                    .set_transform(self.transform * Affine::translate((offset, offset)));
                self.canvas.fill_path(&path);
                self.canvas.set_transform(self.transform);
            }
            if effects.emboss || effects.imprint {
                let direction = if effects.imprint { -1.0 } else { 1.0 };
                self.canvas
                    .set_paint(Color::from_rgba8(255, 255, 255, a).into());
                self.canvas.set_transform(
                    self.transform * Affine::translate((-offset * direction, -offset * direction)),
                );
                self.canvas.fill_path(&path);
                self.canvas
                    .set_paint(Color::from_rgba8(0, 0, 0, a / 2).into());
                self.canvas.set_transform(
                    self.transform * Affine::translate((offset * direction, offset * direction)),
                );
                self.canvas.fill_path(&path);
                self.canvas.set_transform(self.transform);
            }
            self.canvas.set_paint(Color::from_rgba8(r, g, b, a).into());
            if effects.outline {
                self.canvas
                    .set_stroke(Stroke::new(f64::from((size_px / 32.0).max(0.5))));
                self.canvas.stroke_path(&path);
            } else {
                self.canvas.fill_path(&path);
            }
        }

        let decoration = &run.decoration;
        if !(decoration.underline || decoration.strikethrough || decoration.double_strike) {
            return;
        }
        let advance = pen_x - start_x;
        if advance <= 0.0 {
            return;
        }
        let metrics = Metrics::new(&font, Size::new(size_px), LocationRef::default());
        let size = f64::from(size_px);
        if decoration.underline {
            let (offset, thickness) = metrics.underline.map_or((-size * 0.12, size * 0.06), |d| {
                (f64::from(d.offset), f64::from(d.thickness))
            });
            let color = decoration.underline_color.unwrap_or(run.color);
            self.underline(
                start_x,
                baseline_y - offset,
                advance,
                thickness,
                color,
                decoration.underline_style,
                &run.glyphs,
            );
        }
        if decoration.strikethrough || decoration.double_strike {
            let (offset, thickness) = metrics.strikeout.map_or((size * 0.26, size * 0.06), |d| {
                (f64::from(d.offset), f64::from(d.thickness))
            });
            let lines: &[f64] = if decoration.double_strike {
                &[offset + thickness, offset - thickness]
            } else {
                &[offset]
            };
            for line in lines {
                self.decoration_line(start_x, baseline_y - line, advance, thickness, run.color);
            }
        }
    }

    /// A thin filled band centred on `y_center`, at least one pixel tall — the
    /// CPU raster's `draw_decoration`.
    fn decoration_line(
        &mut self,
        x: f64,
        y_center: f64,
        advance: f64,
        thickness: f64,
        color: [u8; 4],
    ) {
        let h = thickness.max(1.0);
        if advance <= 0.0 {
            return;
        }
        let [r, g, b, a] = color;
        self.canvas.set_paint(Color::from_rgba8(r, g, b, a).into());
        self.canvas.fill_rect(&KRect::new(
            x,
            y_center - h / 2.0,
            x + advance,
            y_center + h / 2.0,
        ));
    }

    /// The CPU raster's `draw_underline`, style for style.
    #[allow(clippy::too_many_arguments)] // mirrors the reference's signature
    fn underline(
        &mut self,
        x: f64,
        y_center: f64,
        advance: f64,
        thickness: f64,
        color: [u8; 4],
        style: UnderlineStyle,
        glyphs: &[Glyph],
    ) {
        // The reference floors a pattern's step, so a face that reports a zero
        // underline thickness cannot spin the loop forever.
        const MIN_PATTERN_STEP: f64 = 0.5;
        let step_of = |raw: f64| {
            if raw.is_finite() {
                raw.max(MIN_PATTERN_STEP)
            } else {
                MIN_PATTERN_STEP
            }
        };
        match style {
            UnderlineStyle::Single => self.decoration_line(x, y_center, advance, thickness, color),
            UnderlineStyle::Words => {
                let mut cursor = 0.0_f64;
                let mut word_start = None;
                let mut spans = Vec::new();
                for glyph in glyphs {
                    if glyph.is_whitespace {
                        if let Some(start) = word_start.take()
                            && cursor > start
                        {
                            spans.push((start, cursor - start));
                        }
                    } else if word_start.is_none() {
                        word_start = Some(cursor);
                    }
                    cursor += f64::from(glyph.advance.to_device_px(self.dpi));
                }
                if let Some(start) = word_start
                    && cursor > start
                {
                    spans.push((start, cursor - start));
                }
                for (offset, width) in spans {
                    self.decoration_line(x + offset, y_center, width, thickness, color);
                }
            }
            UnderlineStyle::Wavy => {
                let amplitude = (thickness * 1.5).max(1.0);
                let period = (thickness * 6.0).max(4.0);
                let step = (period / 8.0).clamp(0.75, 2.0);
                let wave_y =
                    |px: f64| y_center + amplitude * (std::f64::consts::TAU * px / period).sin();
                let mut path = BezPath::new();
                path.move_to((x, wave_y(0.0)));
                let mut px = step;
                while px < advance {
                    path.line_to((x + px, wave_y(px)));
                    px += step;
                }
                path.line_to((x + advance, wave_y(advance)));
                let [r, g, b, a] = color;
                self.canvas.set_paint(Color::from_rgba8(r, g, b, a).into());
                self.canvas.set_stroke(Stroke::new(thickness.max(1.0)));
                self.canvas.stroke_path(&path);
            }
            UnderlineStyle::Double => {
                let gap = (thickness * 2.0).max(1.5);
                self.decoration_line(x, y_center - gap / 2.0, advance, thickness, color);
                self.decoration_line(x, y_center + gap / 2.0, advance, thickness, color);
            }
            UnderlineStyle::Thick => {
                self.decoration_line(x, y_center, advance, (thickness * 2.0).max(1.5), color);
            }
            UnderlineStyle::Dotted | UnderlineStyle::Dashed => {
                let (on, off) = if style == UnderlineStyle::Dotted {
                    (thickness.max(1.0), thickness * 1.6)
                } else {
                    (thickness * 4.0, thickness * 3.0)
                };
                let step = step_of(on + off);
                let mut cursor = 0.0_f64;
                while cursor < advance {
                    let seg = on.min(advance - cursor);
                    if seg > 0.0 {
                        self.decoration_line(x + cursor, y_center, seg, thickness, color);
                    }
                    cursor += step;
                }
            }
            UnderlineStyle::DotDash => {
                let (dash, dot, gap) = (thickness * 4.0, thickness.max(1.0), thickness * 2.5);
                let period = step_of(dash + gap + dot + gap);
                let mut base = 0.0_f64;
                while base < advance {
                    let d = dash.min(advance - base);
                    if d > 0.0 {
                        self.decoration_line(x + base, y_center, d, thickness, color);
                    }
                    let dot_start = base + dash + gap;
                    if dot_start < advance {
                        let d2 = dot.min(advance - dot_start);
                        self.decoration_line(x + dot_start, y_center, d2, thickness, color);
                    }
                    base += period;
                }
            }
        }
    }

    /// The CPU raster's `rect_path`, hairline snap included: a positive extent
    /// under one device pixel is drawn one pixel wide, so a 0.25 pt border stays
    /// visible at screen resolution.
    fn rect_path(&self, rect: Rect) -> Option<BezPath> {
        let x = self.px(rect.origin.x);
        let y = self.px(rect.origin.y);
        let w = hairline_snap(self.px(rect.size.width));
        let h = hairline_snap(self.px(rect.size.height));
        (w > 0.0 && h > 0.0).then(|| KRect::new(x, y, x + w, y + h).to_path(0.1))
    }

    fn device_bounds(&self, rect: Rect) -> Option<KRect> {
        let x = self.px(rect.origin.x);
        let y = self.px(rect.origin.y);
        Some(KRect::new(
            x,
            y,
            x + self.px(rect.size.width).max(0.0),
            y + self.px(rect.size.height).max(0.0),
        ))
    }

    fn ellipse_path(&self, rect: Rect) -> Option<BezPath> {
        let bounds = self.device_bounds(rect)?;
        (bounds.width() > 0.0 && bounds.height() > 0.0)
            .then(|| Ellipse::from_rect(bounds).to_path(0.1))
    }

    /// The CPU raster's quadratic-cornered rounded rectangle, not kurbo's
    /// circular one, so a corner is the same curve on both backends.
    fn rounded_rect_path(&self, rect: Rect, radius: Twip) -> Option<BezPath> {
        let x = self.px(rect.origin.x);
        let y = self.px(rect.origin.y);
        let width = self.px(rect.size.width).max(0.0);
        let height = self.px(rect.size.height).max(0.0);
        if width <= 0.0 || height <= 0.0 {
            return None;
        }
        let radius = self.px(radius).max(0.0).min(width / 2.0).min(height / 2.0);
        let (right, bottom) = (x + width, y + height);
        let mut path = BezPath::new();
        path.move_to((x + radius, y));
        path.line_to((right - radius, y));
        path.quad_to((right, y), (right, y + radius));
        path.line_to((right, bottom - radius));
        path.quad_to((right, bottom), (right - radius, bottom));
        path.line_to((x + radius, bottom));
        path.quad_to((x, bottom), (x, bottom - radius));
        path.line_to((x, y + radius));
        path.quad_to((x, y), (x + radius, y));
        path.close_path();
        Some(path)
    }

    fn polygon_path(&self, points: &[Point]) -> Option<BezPath> {
        let (first, rest) = points.split_first()?;
        if rest.len() < 2 {
            return None;
        }
        let mut path = BezPath::new();
        path.move_to(self.point(*first));
        for point in rest {
            path.line_to(self.point(*point));
        }
        path.close_path();
        Some(path)
    }

    fn command_path(&self, commands: &[PathCommand], closed: bool) -> Option<BezPath> {
        let (first, rest) = commands.split_first()?;
        let segments = rest.iter().filter(|command| command.is_segment()).count();
        if segments < if closed { 2 } else { 1 } {
            return None;
        }
        let mut path = BezPath::new();
        path.move_to(self.point(first.endpoint()?));
        for command in rest {
            match *command {
                PathCommand::MoveTo { point } => path.move_to(self.point(point)),
                PathCommand::LineTo { point } => path.line_to(self.point(point)),
                PathCommand::CubicTo {
                    control1,
                    control2,
                    point,
                } => path.curve_to(
                    self.point(control1),
                    self.point(control2),
                    self.point(point),
                ),
                PathCommand::QuadTo { control, point } => {
                    path.quad_to(self.point(control), self.point(point));
                }
                PathCommand::Close => path.close_path(),
            }
        }
        if closed {
            path.close_path();
        }
        Some(path)
    }
}

fn hairline_snap(extent: f64) -> f64 {
    if extent > 0.0 && extent < 1.0 {
        1.0
    } else {
        extent.max(0.0)
    }
}

fn solid(color: DisplayColor) -> Color {
    Color::from_rgba8(color.r, color.g, color.b, color.a)
}

/// The rotation/flip about the object's centre (`a:xfrm`), as the CPU raster's
/// `object_transform`: `T(c) · R(rot) · Flip · T(-c)`.
pub(crate) fn object_transform(transform: Option<&ShapeTransform>, dpi: f32) -> Affine {
    let Some(t) = transform else {
        return Affine::IDENTITY;
    };
    if t.rotation == 0 && !t.flip_h && !t.flip_v {
        return Affine::IDENTITY;
    }
    let cx = f64::from(t.center.x.to_device_px(dpi));
    let cy = f64::from(t.center.y.to_device_px(dpi));
    let angle = (f64::from(t.rotation) / 60_000.0).to_radians();
    let sx = if t.flip_h { -1.0 } else { 1.0 };
    let sy = if t.flip_v { -1.0 } else { 1.0 };
    Affine::translate((cx, cy))
        * Affine::rotate(angle)
        * Affine::scale_non_uniform(sx, sy)
        * Affine::translate((-cx, -cy))
}

/// The CPU raster's gradient geometry: a linear sweep through the box centre
/// along the angle, spanning the box's projected half-extent; a radial one
/// centred on the box.
fn gradient_paint(gradient: &Gradient, bounds: KRect) -> Option<PaintType> {
    let stops: Vec<ColorStop> = gradient
        .stops
        .iter()
        .map(|stop| ColorStop::from((stop.position.clamp(0.0, 1.0), solid(stop.color))))
        .collect();
    if stops.len() < 2 {
        return None;
    }
    let center = bounds.center();
    let brush = match gradient.kind {
        GradientKind::Linear { angle_deg } => {
            let radians = f64::from(angle_deg).to_radians();
            let (dx, dy) = (radians.cos(), radians.sin());
            let half =
                ((bounds.width() / 2.0 * dx).abs() + (bounds.height() / 2.0 * dy).abs()).max(0.5);
            peniko::Gradient::new_linear(
                (center.x - dx * half, center.y - dy * half),
                (center.x + dx * half, center.y + dy * half),
            )
        }
        GradientKind::Radial => {
            let radius = (bounds.width().max(bounds.height()) / 2.0).max(0.5);
            peniko::Gradient::new_radial(center, radius as f32)
        }
    };
    Some(PaintType::Gradient(brush.with_stops(stops.as_slice())))
}

/// The CPU raster's preset dash ratios, scaled by the line width.
fn dash_pattern(dash: DashStyle, width: f64) -> Option<Vec<f64>> {
    let unit = width.max(1.0);
    let ratios: &[f64] = match dash {
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
    Some(ratios.iter().map(|r| (r * unit).max(0.1)).collect())
}

/// A `skrifa` pen that appends glyph contours to a kurbo path, font space
/// (y-up, at the glyph origin) to device space (y-down, at the pen).
struct KurboPen<'a> {
    path: &'a mut BezPath,
    origin_x: f64,
    baseline_y: f64,
    scale_x: f64,
}

impl KurboPen<'_> {
    fn map(&self, x: f32, y: f32) -> (f64, f64) {
        (
            self.origin_x + f64::from(x) * self.scale_x,
            self.baseline_y - f64::from(y),
        )
    }
}

impl OutlinePen for KurboPen<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.map(x, y);
        self.path.move_to(p);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.map(x, y);
        self.path.line_to(p);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let c = self.map(cx0, cy0);
        let p = self.map(x, y);
        self.path.quad_to(c, p);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let c0 = self.map(cx0, cy0);
        let c1 = self.map(cx1, cy1);
        let p = self.map(x, y);
        self.path.curve_to(c0, c1, p);
    }

    fn close(&mut self) {
        self.path.close_path();
    }
}
