//! Transcribing one page's display list into a PDF content stream.
//!
//! This module is the heart of the editor↔PDF parity guarantee: it walks the
//! *same* [`DisplayList`] the raster backend walks, in the same back-to-front
//! order, and emits an operator for each [`PaintItem`]. It performs no layout,
//! no shaping and no metric adjustment — every coordinate on the page comes
//! from the display list the layout pass produced.
//!
//! # Coordinates
//!
//! Display-list coordinates are twips with the origin at the page's top-left
//! and `y` growing downward. PDF user space is points (1 pt = 20 twips) with
//! the origin at the bottom-left and `y` growing upward, so each point is
//! `(x / 20, page_height_pt - y / 20)`. No device scale is applied anywhere:
//! a PDF is resolution-free, which is the one deliberate divergence from the
//! raster backend's paint path (`docs/98` step 2).

use std::collections::BTreeMap;

use casual_doc_layout::display::Color;
use casual_doc_layout::display::DisplayList;
use casual_doc_layout::display::Fill;
use casual_doc_layout::display::Gradient;
use casual_doc_layout::display::GradientKind;
use casual_doc_layout::display::PaintItem;
use casual_doc_layout::display::ShapeGeometry;
use casual_doc_layout::display::ShapeOutline;
use casual_doc_layout::display::ShapeTransform;
use casual_doc_layout::display::Stroke;
use casual_doc_layout::text::GlyphRun;
use casual_doc_layout::units::Point;
use casual_doc_layout::units::Rect;
use casual_doc_layout::units::Twip;
use casual_doc_model::v1::CROP_FULL;
use casual_doc_model::v1::CropRect;
use casual_doc_model::v1::DashStyle;
use casual_doc_model::v1::LineEnd;
use casual_doc_model::v1::LineEndKind;
use casual_doc_model::v1::LineEndSize;
use casual_doc_model::v1::UnderlineStyle;
use skrifa::FontRef;
use skrifa::MetadataProvider;
use skrifa::instance::LocationRef;
use skrifa::instance::Size as FontSize;
use skrifa::metrics::Metrics;

use crate::font::FontError;
use crate::font::FontTable;
use crate::font::PdfFontSource;
use crate::picture::ImageTable;
use crate::picture::PdfMediaSource;
use crate::writer::Writer;
use crate::writer::channel;
use crate::writer::name as pdf_name;
use crate::writer::num;

/// Twips per PDF point.
const TWIPS_PER_POINT: f32 = 20.0;

/// Display-list stroke widths are device pixels at 96 DPI (`compose::stroke_px`);
/// a PDF point is 1/72 inch.
const PX96_TO_POINTS: f32 = 72.0 / 96.0;

/// The thinnest decoration rule this backend draws, in points.
///
/// The raster backend floors a decoration at one **device pixel**, which has no
/// meaning in a resolution-free PDF. Half a point is the equivalent floor at a
/// typical screen resolution and keeps a hairline underline visible at any zoom
/// instead of disappearing at 1:1.
const MIN_DECORATION_POINTS: f32 = 0.5;

/// The maximum number of segments a patterned (dotted / dashed / wavy)
/// decoration is drawn with. The pattern period is derived from font metrics,
/// and a face may report a zero or absurd underline thickness, so the loop is
/// bounded where the value is read rather than trusted (the raster backend
/// carries the same guard for the same reason).
const MAX_PATTERN_SEGMENTS: usize = 4096;

/// A page to transcribe: its trim size and the display list layout produced for
/// it.
#[derive(Clone, Copy, Debug)]
pub struct PdfPage<'a> {
    /// The page width in twips.
    pub width: Twip,
    /// The page height in twips.
    pub height: Twip,
    /// The page's paint commands, in back-to-front order.
    pub list: &'a DisplayList,
}

/// Accumulates the file-wide resources while pages are transcribed one by one.
pub(crate) struct Transcriber<'a> {
    pub(crate) fonts: FontTable,
    pub(crate) images: ImageTable,
    /// Constant-alpha graphics states, keyed by the alpha byte.
    alphas: BTreeMap<u8, String>,
    /// `(resource name, shading dictionary body)` for every gradient placed.
    shadings: Vec<(String, String)>,
    /// Paint items the backend could not represent, by stable code.
    gaps: BTreeMap<&'static str, usize>,
    font_source: &'a dyn PdfFontSource,
    media: &'a dyn PdfMediaSource,
}

impl<'a> Transcriber<'a> {
    pub(crate) fn new(font_source: &'a dyn PdfFontSource, media: &'a dyn PdfMediaSource) -> Self {
        Self {
            fonts: FontTable::new(),
            images: ImageTable::new(),
            alphas: BTreeMap::new(),
            shadings: Vec::new(),
            gaps: BTreeMap::new(),
            font_source,
            media,
        }
    }

    /// The shading resources placed so far, as `(name, dictionary body)`.
    pub(crate) fn shadings(&self) -> &[(String, String)] {
        &self.shadings
    }

    /// The constant-alpha graphics states placed so far, as `(name, alpha)`.
    pub(crate) fn alphas(&self) -> impl Iterator<Item = (&String, f32)> {
        self.alphas
            .iter()
            .map(|(alpha, name)| (name, f32::from(*alpha) / 255.0))
    }

    /// Paint families the transcription could not represent, with occurrence
    /// counts, so the caller reports them instead of the page quietly losing
    /// them.
    pub(crate) fn gaps(&self) -> &BTreeMap<&'static str, usize> {
        &self.gaps
    }

    fn gap(&mut self, code: &'static str) {
        *self.gaps.entry(code).or_insert(0) += 1;
    }

    /// Transcribes one page into a content stream.
    pub(crate) fn page(
        &mut self,
        writer: &mut Writer,
        page: PdfPage<'_>,
    ) -> Result<Vec<u8>, FontError> {
        let height = pt(page.height);
        let mut out = Content {
            bytes: Vec::with_capacity(4 * 1024),
            height,
            clips: 0,
        };
        for item in &page.list.items {
            self.item(writer, &mut out, item)?;
        }
        // A display list that pushed more clips than it popped still produces a
        // balanced stream rather than a truncated one.
        for _ in 0..out.clips {
            out.op("Q");
        }
        Ok(out.bytes)
    }

    fn item(
        &mut self,
        writer: &mut Writer,
        out: &mut Content,
        item: &PaintItem,
    ) -> Result<(), FontError> {
        match item {
            PaintItem::Glyphs { run } => self.glyphs(writer, out, run)?,
            PaintItem::Rect { rect, fill, stroke } => {
                out.path_rect(*rect);
                self.paint(out, fill.as_ref(), stroke.as_ref());
            }
            PaintItem::Ellipse { rect, fill, stroke } => {
                out.path_ellipse(*rect);
                self.paint(out, fill.as_ref(), stroke.as_ref());
            }
            PaintItem::RoundedRect {
                rect,
                radius,
                fill,
                stroke,
            } => {
                out.path_rounded_rect(*rect, *radius);
                self.paint(out, fill.as_ref(), stroke.as_ref());
            }
            PaintItem::Polygon {
                points,
                fill,
                stroke,
            } => {
                out.path_polygon(points);
                self.paint(out, fill.as_ref(), stroke.as_ref());
            }
            PaintItem::Line { from, to, stroke } => {
                out.set_stroke(stroke);
                out.line(*from, *to);
                out.op("S");
            }
            PaintItem::Image {
                media,
                rect,
                crop,
                transform,
            } => self.image(writer, out, media, *rect, crop.as_ref(), transform.as_ref()),
            PaintItem::Shape {
                geometry,
                fill,
                stroke,
                head_end,
                tail_end,
                transform,
            } => {
                self.shape(
                    out,
                    geometry,
                    fill.as_ref(),
                    stroke.as_ref(),
                    transform.as_ref(),
                    head_end.as_ref(),
                    tail_end.as_ref(),
                );
            }
            PaintItem::PushClip(rect) => {
                out.op("q");
                out.path_rect(*rect);
                out.op("W n");
                out.clips += 1;
            }
            PaintItem::PopClip => {
                if out.clips > 0 {
                    out.clips -= 1;
                    out.op("Q");
                }
            }
        }
        Ok(())
    }

    /// Fills and/or strokes the path already on the stream.
    fn paint(&mut self, out: &mut Content, fill: Option<&Color>, stroke: Option<&Stroke>) {
        match (fill, stroke) {
            (Some(fill), Some(stroke)) => {
                self.alpha(out, fill.a.min(stroke.color.a));
                out.set_fill_color(*fill);
                out.set_stroke(stroke);
                out.op("B");
            }
            (Some(fill), None) => {
                self.alpha(out, fill.a);
                out.set_fill_color(*fill);
                out.op("f");
            }
            (None, Some(stroke)) => {
                self.alpha(out, stroke.color.a);
                out.set_stroke(stroke);
                out.op("S");
            }
            (None, None) => out.op("n"),
        }
    }

    /// Selects the constant-alpha graphics state for `alpha`, minting one the
    /// first time a value is seen. Fully opaque paint needs no state change.
    fn alpha(&mut self, out: &mut Content, alpha: u8) {
        if alpha == 255 {
            return;
        }
        let next = self.alphas.len() + 1;
        let name = self
            .alphas
            .entry(alpha)
            .or_insert_with(|| format!("GS{next}"));
        out.bytes
            .extend_from_slice(format!("{} gs\n", pdf_name(name)).as_bytes());
    }

    /// Emits one glyph run as real text: an embedded font, the shaper's glyph
    /// ids, and the shaper's advances.
    fn glyphs(
        &mut self,
        writer: &mut Writer,
        out: &mut Content,
        run: &GlyphRun,
    ) -> Result<(), FontError> {
        if run.glyphs.is_empty() {
            return Ok(());
        }
        let resource = self.fonts.use_face(
            writer,
            self.font_source,
            run.font,
            run.glyphs
                .iter()
                .filter_map(|glyph| u16::try_from(glyph.id).ok()),
        )?;
        let units = f32::from(self.fonts.units_per_em(run.font).unwrap_or(1000));
        let size = pt(run.size);
        let scale = f32::from(run.character_scale_percent) / 100.0;
        let bytes = self
            .font_source
            .font_data(run.font)
            .ok_or(FontError::Unavailable(run.font.0))?;
        let face = FontRef::from_index(bytes, self.font_source.face_index(run.font)).ok();

        self.alpha(out, run.color[3]);
        out.bytes.extend_from_slice(
            format!("BT\n{} {} Tf\n", pdf_name(&resource), num(size)).as_bytes(),
        );
        if (scale - 1.0).abs() > f32::EPSILON {
            out.bytes
                .extend_from_slice(format!("{} Tz\n", num(scale * 100.0)).as_bytes());
        }
        out.bytes.extend_from_slice(
            format!(
                "{} {} {} rg\n1 0 0 1 {} {} Tm\n",
                channel(run.color[0]),
                channel(run.color[1]),
                channel(run.color[2]),
                num(pt(run.origin.x)),
                num(out.height - pt(run.origin.y)),
            )
            .as_bytes(),
        );

        // One `TJ` array: consecutive glyphs whose PDF advance already equals
        // the shaper's advance share a hex string, and any difference becomes
        // an explicit adjustment. The glyphs therefore land exactly where the
        // display list put them.
        let mut array = String::from("[<");
        let mut open = true;
        for glyph in &run.glyphs {
            if !open {
                array.push('<');
                open = true;
            }
            array.push_str(&format!("{:04X}", glyph.id.min(u32::from(u16::MAX))));
            let width = face
                .as_ref()
                .and_then(|face| {
                    u16::try_from(glyph.id)
                        .ok()
                        .map(|id| advance_units(face, id))
                })
                .unwrap_or(0.0)
                / units;
            let wanted = if scale.abs() > f32::EPSILON {
                pt(glyph.advance) / scale
            } else {
                0.0
            };
            let adjust = if size.abs() > f32::EPSILON {
                1000.0 * (width - wanted / size)
            } else {
                0.0
            };
            if adjust.abs() > 0.01 {
                array.push_str(&format!(">{}", num(adjust)));
                open = false;
            }
        }
        if open {
            array.push('>');
        }
        array.push_str("] TJ\nET\n");
        out.bytes.extend_from_slice(array.as_bytes());

        self.decorations(out, run, face.as_ref(), size);
        Ok(())
    }

    /// Draws the run's underline and strike-through.
    ///
    /// The display list carries these as flags on the run rather than as paint
    /// items, so both backends derive the geometry from the face's own metrics.
    /// The arithmetic here mirrors `casual-doc-render`'s line for line, with the
    /// device-pixel floors replaced by point floors.
    fn decorations(
        &mut self,
        out: &mut Content,
        run: &GlyphRun,
        face: Option<&FontRef<'_>>,
        size: f32,
    ) {
        let decoration = &run.decoration;
        if !(decoration.underline || decoration.strikethrough || decoration.double_strike) {
            return;
        }
        let advance: f32 = run.glyphs.iter().map(|glyph| pt(glyph.advance)).sum();
        if advance <= 0.0 {
            return;
        }
        let metrics =
            face.map(|face| Metrics::new(face, FontSize::new(size), LocationRef::default()));
        let x = pt(run.origin.x);
        let baseline = out.height - pt(run.origin.y);

        if decoration.underline {
            let (offset, thickness) = metrics
                .as_ref()
                .and_then(|metrics| metrics.underline)
                .map_or((-size * 0.12, size * 0.06), |d| (d.offset, d.thickness));
            let color = decoration.underline_color.unwrap_or(run.color);
            self.underline(
                out,
                x,
                baseline + offset,
                advance,
                thickness,
                color,
                decoration.underline_style,
                run,
            );
        }
        if decoration.strikethrough || decoration.double_strike {
            let (offset, thickness) = metrics
                .as_ref()
                .and_then(|metrics| metrics.strikeout)
                .map_or((size * 0.26, size * 0.06), |d| (d.offset, d.thickness));
            let offsets: &[f32] = if decoration.double_strike {
                &[offset + thickness, offset - thickness]
            } else {
                &[offset]
            };
            for line in offsets {
                self.alpha(out, run.color[3]);
                out.rule(x, baseline + line, advance, thickness, run.color);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn underline(
        &mut self,
        out: &mut Content,
        x: f32,
        y: f32,
        advance: f32,
        thickness: f32,
        color: [u8; 4],
        style: UnderlineStyle,
        run: &GlyphRun,
    ) {
        self.alpha(out, color[3]);
        let dashed = |on: f32, off: f32, out: &mut Content| {
            let step = if (on + off).is_finite() {
                (on + off).max(MIN_DECORATION_POINTS)
            } else {
                MIN_DECORATION_POINTS
            };
            let mut cursor = 0.0_f32;
            let mut drawn = 0;
            while cursor < advance && drawn < MAX_PATTERN_SEGMENTS {
                let segment = on.min(advance - cursor);
                if segment > 0.0 {
                    out.rule(x + cursor, y, segment, thickness, color);
                }
                cursor += step;
                drawn += 1;
            }
        };
        match style {
            UnderlineStyle::Single => out.rule(x, y, advance, thickness, color),
            UnderlineStyle::Words => {
                let mut cursor = 0.0_f32;
                let mut start: Option<f32> = None;
                for glyph in &run.glyphs {
                    if glyph.is_whitespace {
                        if let Some(from) = start.take()
                            && cursor > from
                        {
                            out.rule(x + from, y, cursor - from, thickness, color);
                        }
                    } else if start.is_none() {
                        start = Some(cursor);
                    }
                    cursor += pt(glyph.advance);
                }
                if let Some(from) = start
                    && cursor > from
                {
                    out.rule(x + from, y, cursor - from, thickness, color);
                }
            }
            UnderlineStyle::Wavy => {
                let amplitude = (thickness * 1.5).max(MIN_DECORATION_POINTS);
                let period = (thickness * 6.0).max(4.0 * MIN_DECORATION_POINTS);
                let step = (period / 8.0).clamp(0.5, 2.0);
                out.set_stroke_color(color);
                out.bytes.extend_from_slice(
                    format!("{} w\n", num(thickness.max(MIN_DECORATION_POINTS))).as_bytes(),
                );
                let wave = |px: f32| y - amplitude * (std::f32::consts::TAU * px / period).sin();
                out.bytes
                    .extend_from_slice(format!("{} {} m\n", num(x), num(wave(0.0))).as_bytes());
                let mut px = step;
                let mut drawn = 0;
                while px < advance && drawn < MAX_PATTERN_SEGMENTS {
                    out.bytes.extend_from_slice(
                        format!("{} {} l\n", num(x + px), num(wave(px))).as_bytes(),
                    );
                    px += step;
                    drawn += 1;
                }
                out.bytes.extend_from_slice(
                    format!("{} {} l\nS\n", num(x + advance), num(wave(advance))).as_bytes(),
                );
            }
            UnderlineStyle::Double => {
                let gap = (thickness * 2.0).max(1.5 * MIN_DECORATION_POINTS);
                out.rule(x, y + gap / 2.0, advance, thickness, color);
                out.rule(x, y - gap / 2.0, advance, thickness, color);
            }
            UnderlineStyle::Thick => out.rule(
                x,
                y,
                advance,
                (thickness * 2.0).max(1.5 * MIN_DECORATION_POINTS),
                color,
            ),
            UnderlineStyle::Dotted => {
                dashed(thickness.max(MIN_DECORATION_POINTS), thickness * 1.6, out)
            }
            UnderlineStyle::Dashed => dashed(thickness * 4.0, thickness * 3.0, out),
            UnderlineStyle::DotDash => {
                let (dash, dot, gap) = (
                    thickness * 4.0,
                    thickness.max(MIN_DECORATION_POINTS),
                    thickness * 2.5,
                );
                let raw = dash + gap + dot + gap;
                let period = if raw.is_finite() {
                    raw.max(MIN_DECORATION_POINTS)
                } else {
                    MIN_DECORATION_POINTS
                };
                let mut base = 0.0_f32;
                let mut drawn = 0;
                while base < advance && drawn < MAX_PATTERN_SEGMENTS {
                    let width = dash.min(advance - base);
                    if width > 0.0 {
                        out.rule(x + base, y, width, thickness, color);
                    }
                    let dot_start = base + dash + gap;
                    if dot_start < advance {
                        out.rule(
                            x + dot_start,
                            y,
                            dot.min(advance - dot_start),
                            thickness,
                            color,
                        );
                    }
                    base += period;
                    drawn += 1;
                }
            }
        }
    }

    /// Places an embedded picture, honouring its crop and its rotation/flip.
    fn image(
        &mut self,
        writer: &mut Writer,
        out: &mut Content,
        media: &str,
        rect: Rect,
        crop: Option<&CropRect>,
        transform: Option<&ShapeTransform>,
    ) {
        let Some(resource) = self.images.use_image(writer, self.media, media) else {
            // The raster backend paints a bordered box with a diagonal cross
            // when media bytes are present but undecodable, so a reader sees
            // "there was a picture here" rather than a blank gap. The PDF
            // paints the same mark, from the same geometry, and the omission
            // is reported as a finding either way.
            self.gap("pdf.image.unresolved");
            self.placeholder(out, rect, transform);
            return;
        };
        let (x, y, width, height) = out.rect_points(rect);
        out.op("q");
        if let Some(transform) = transform {
            out.concat_transform(transform);
        }
        // A crop selects a sub-rectangle of the SOURCE; the whole picture is
        // placed oversized so that its visible sub-rectangle exactly fills the
        // destination box, and the box clips the rest away. That keeps the
        // stored samples untouched -- no re-encode, no resample.
        let (place_x, place_y, place_w, place_h) = crop
            .filter(|crop| !crop.is_identity())
            .and_then(|crop| cropped_placement(x, y, width, height, crop))
            .unwrap_or((x, y, width, height));
        if (place_x, place_y, place_w, place_h) != (x, y, width, height) {
            out.bytes.extend_from_slice(
                format!(
                    "{} {} {} {} re W n\n",
                    num(x),
                    num(y),
                    num(width),
                    num(height)
                )
                .as_bytes(),
            );
        }
        out.bytes.extend_from_slice(
            format!(
                "{} 0 0 {} {} {} cm\n{} Do\nQ\n",
                num(place_w),
                num(place_h),
                num(place_x),
                num(place_y),
                pdf_name(&resource)
            )
            .as_bytes(),
        );
    }

    /// Paints the "unsupported picture" mark the raster backend paints: a
    /// stroked border box with a corner-to-corner cross, in a neutral grey.
    fn placeholder(&mut self, out: &mut Content, rect: Rect, transform: Option<&ShapeTransform>) {
        const PLACEHOLDER_GREY: [u8; 4] = [150, 150, 150, 255];
        let (x, y, width, height) = out.rect_points(rect);
        if width <= 0.0 || height <= 0.0 {
            return;
        }
        out.op("q");
        if let Some(transform) = transform {
            out.concat_transform(transform);
        }
        out.set_stroke_color(PLACEHOLDER_GREY);
        out.bytes.extend_from_slice(
            format!(
                "{} w\n{} {} {} {} re S\n{} {} m\n{} {} l\n{} {} m\n{} {} l\nS\nQ\n",
                num((width.min(height) * 0.02).max(MIN_DECORATION_POINTS)),
                num(x),
                num(y),
                num(width),
                num(height),
                num(x),
                num(y),
                num(x + width),
                num(y + height),
                num(x + width),
                num(y),
                num(x),
                num(y + height),
            )
            .as_bytes(),
        );
    }

    /// Paints a floating DrawingML shape.
    #[allow(clippy::too_many_arguments)]
    fn shape(
        &mut self,
        out: &mut Content,
        geometry: &ShapeGeometry,
        fill: Option<&Fill>,
        stroke: Option<&ShapeOutline>,
        transform: Option<&ShapeTransform>,
        head_end: Option<&LineEnd>,
        tail_end: Option<&LineEnd>,
    ) {
        let transformed = transform.is_some();
        if transformed {
            out.op("q");
            out.concat_transform(transform.expect("checked"));
        }
        match fill {
            Some(Fill::Solid(color)) => {
                out.geometry(geometry);
                self.alpha(out, color.a);
                out.set_fill_color(*color);
                out.op("f");
            }
            Some(Fill::Gradient(gradient)) => self.gradient(out, geometry, gradient),
            None => {}
        }
        if let Some(stroke) = stroke {
            out.geometry(geometry);
            self.alpha(out, stroke.color.a);
            out.set_stroke_color([
                stroke.color.r,
                stroke.color.g,
                stroke.color.b,
                stroke.color.a,
            ]);
            out.bytes.extend_from_slice(
                format!("{} w\n", num((stroke.width * PX96_TO_POINTS).max(0.1))).as_bytes(),
            );
            out.set_dash(stroke.dash, stroke.width * PX96_TO_POINTS);
            out.op("S");
            out.op("[] 0 d");
            // Arrowheads ride inside the shape's own transform, so a rotated
            // connector keeps its heads attached and oriented, exactly as the
            // raster backend places them.
            if let ShapeGeometry::Line { from, to } = geometry {
                let width = (stroke.width * PX96_TO_POINTS).max(MIN_DECORATION_POINTS);
                let color = [
                    stroke.color.r,
                    stroke.color.g,
                    stroke.color.b,
                    stroke.color.a,
                ];
                let a = (pt(from.x), out.height - pt(from.y));
                let b = (pt(to.x), out.height - pt(to.y));
                if let Some(head) = head_end {
                    self.alpha(out, stroke.color.a);
                    out.arrowhead(a, b, head, width, color);
                }
                if let Some(tail) = tail_end {
                    self.alpha(out, stroke.color.a);
                    out.arrowhead(b, a, tail, width, color);
                }
            }
        }
        if transformed {
            out.op("Q");
        }
    }

    /// Fills `geometry` with a PDF shading, which is what a gradient *is* in a
    /// vector PDF: no bitmap is produced and it stays sharp at any zoom.
    fn gradient(&mut self, out: &mut Content, geometry: &ShapeGeometry, gradient: &Gradient) {
        let Some(bounds) = geometry_bounds(geometry) else {
            self.gap("pdf.shape.gradient");
            return;
        };
        let (x, y, width, height) = out.rect_points(bounds);
        let Some(dictionary) = shading_dictionary(gradient, x, y, width, height) else {
            self.gap("pdf.shape.gradient");
            return;
        };
        let name = format!("Sh{}", self.shadings.len() + 1);
        self.shadings.push((name.clone(), dictionary));
        out.op("q");
        out.geometry(geometry);
        out.op("W n");
        out.bytes
            .extend_from_slice(format!("{} sh\nQ\n", pdf_name(&name)).as_bytes());
    }
}

/// One page's operator stream under construction.
struct Content {
    bytes: Vec<u8>,
    /// The page height in points, for flipping the display list's y axis.
    height: f32,
    /// How many clips are currently pushed.
    clips: usize,
}

impl Content {
    fn op(&mut self, op: &str) {
        self.bytes.extend_from_slice(op.as_bytes());
        self.bytes.push(b'\n');
    }

    /// A display-list rect as PDF `(x, y, width, height)` with `y` at the
    /// rectangle's bottom.
    fn rect_points(&self, rect: Rect) -> (f32, f32, f32, f32) {
        let width = pt(rect.size.width);
        let height = pt(rect.size.height);
        let x = pt(rect.origin.x);
        let y = self.height - pt(rect.origin.y) - height;
        (x, y, width, height)
    }

    fn path_rect(&mut self, rect: Rect) {
        let (x, y, width, height) = self.rect_points(rect);
        self.bytes.extend_from_slice(
            format!("{} {} {} {} re\n", num(x), num(y), num(width), num(height)).as_bytes(),
        );
    }

    fn path_ellipse(&mut self, rect: Rect) {
        let (x, y, width, height) = self.rect_points(rect);
        self.path_ellipse_points(x, y, width, height);
    }

    /// Four cubic Béziers fitted to a PDF-space box: the standard circular
    /// approximation, the same one the raster backend's oval path uses.
    fn path_ellipse_points(&mut self, x: f32, y: f32, width: f32, height: f32) {
        const KAPPA: f32 = 0.552_284_8;
        let (rx, ry) = (width / 2.0, height / 2.0);
        let (cx, cy) = (x + rx, y + ry);
        let (ox, oy) = (rx * KAPPA, ry * KAPPA);
        self.bytes.extend_from_slice(
            format!(
                "{} {} m\n{} {} {} {} {} {} c\n{} {} {} {} {} {} c\n{} {} {} {} {} {} c\n{} {} {} {} {} {} c\nh\n",
                num(cx - rx), num(cy),
                num(cx - rx), num(cy + oy), num(cx - ox), num(cy + ry), num(cx), num(cy + ry),
                num(cx + ox), num(cy + ry), num(cx + rx), num(cy + oy), num(cx + rx), num(cy),
                num(cx + rx), num(cy - oy), num(cx + ox), num(cy - ry), num(cx), num(cy - ry),
                num(cx - ox), num(cy - ry), num(cx - rx), num(cy - oy), num(cx - rx), num(cy),
            )
            .as_bytes(),
        );
    }

    fn path_rounded_rect(&mut self, rect: Rect, radius: Twip) {
        const KAPPA: f32 = 0.552_284_8;
        let (x, y, width, height) = self.rect_points(rect);
        let r = pt(radius).min(width / 2.0).min(height / 2.0).max(0.0);
        if r <= 0.0 {
            self.path_rect(rect);
            return;
        }
        let k = r * KAPPA;
        let (x1, y1) = (x + width, y + height);
        self.bytes.extend_from_slice(
            format!(
                "{} {} m\n{} {} l\n{} {} {} {} {} {} c\n{} {} l\n{} {} {} {} {} {} c\n{} {} l\n{} {} {} {} {} {} c\n{} {} l\n{} {} {} {} {} {} c\nh\n",
                num(x + r), num(y),
                num(x1 - r), num(y),
                num(x1 - r + k), num(y), num(x1), num(y + r - k), num(x1), num(y + r),
                num(x1), num(y1 - r),
                num(x1), num(y1 - r + k), num(x1 - r + k), num(y1), num(x1 - r), num(y1),
                num(x + r), num(y1),
                num(x + r - k), num(y1), num(x), num(y1 - r + k), num(x), num(y1 - r),
                num(x), num(y + r),
                num(x), num(y + r - k), num(x + r - k), num(y), num(x + r), num(y),
            )
            .as_bytes(),
        );
    }

    fn path_polygon(&mut self, points: &[Point]) {
        let Some(first) = points.first() else {
            self.op("n");
            return;
        };
        self.bytes.extend_from_slice(
            format!(
                "{} {} m\n",
                num(pt(first.x)),
                num(self.height - pt(first.y))
            )
            .as_bytes(),
        );
        for point in &points[1..] {
            self.bytes.extend_from_slice(
                format!(
                    "{} {} l\n",
                    num(pt(point.x)),
                    num(self.height - pt(point.y))
                )
                .as_bytes(),
            );
        }
        self.op("h");
    }

    fn line(&mut self, from: Point, to: Point) {
        self.bytes.extend_from_slice(
            format!(
                "{} {} m\n{} {} l\n",
                num(pt(from.x)),
                num(self.height - pt(from.y)),
                num(pt(to.x)),
                num(self.height - pt(to.y))
            )
            .as_bytes(),
        );
    }

    fn geometry(&mut self, geometry: &ShapeGeometry) {
        match geometry {
            ShapeGeometry::Rect { rect } => self.path_rect(*rect),
            ShapeGeometry::Ellipse { rect } => self.path_ellipse(*rect),
            ShapeGeometry::RoundedRect { rect, radius } => self.path_rounded_rect(*rect, *radius),
            ShapeGeometry::Polygon { points } => self.path_polygon(points),
            ShapeGeometry::Line { from, to } => self.line(*from, *to),
        }
    }

    /// A filled rule: the decoration primitive, centred on `y`.
    fn rule(&mut self, x: f32, y: f32, width: f32, thickness: f32, color: [u8; 4]) {
        let height = thickness.max(MIN_DECORATION_POINTS);
        self.set_fill_color(Color {
            r: color[0],
            g: color[1],
            b: color[2],
            a: color[3],
        });
        self.bytes.extend_from_slice(
            format!(
                "{} {} {} {} re f\n",
                num(x),
                num(y - height / 2.0),
                num(width),
                num(height)
            )
            .as_bytes(),
        );
    }

    /// Fills one line-end decoration at `tip`, oriented along `other -> tip`.
    ///
    /// The construction is `casual-doc-render`'s, in points rather than device
    /// pixels: the same unit vectors, the same width/length factors, and the
    /// same four shapes, so a connector's heads land where the editor draws
    /// them.
    fn arrowhead(
        &mut self,
        tip: (f32, f32),
        other: (f32, f32),
        end: &LineEnd,
        width: f32,
        color: [u8; 4],
    ) {
        if matches!(end.kind, LineEndKind::None) {
            return;
        }
        let (dx, dy) = (tip.0 - other.0, tip.1 - other.1);
        let length_of_segment = dx.hypot(dy);
        if length_of_segment <= f32::EPSILON {
            return;
        }
        let (ux, uy) = (dx / length_of_segment, dy / length_of_segment);
        let (px, py) = (-uy, ux);
        let half_w = width * size_factor(end.width) * 1.5;
        let length = width * size_factor(end.length) * 3.0;
        let base = (tip.0 - ux * length, tip.1 - uy * length);
        let left = (base.0 + px * half_w, base.1 + py * half_w);
        let right = (base.0 - px * half_w, base.1 - py * half_w);
        self.set_fill_color(Color {
            r: color[0],
            g: color[1],
            b: color[2],
            a: color[3],
        });
        let mut path = String::new();
        match end.kind {
            LineEndKind::Triangle | LineEndKind::Arrow => {
                path.push_str(&format!(
                    "{} {} m\n{} {} l\n{} {} l\nh\n",
                    num(tip.0),
                    num(tip.1),
                    num(left.0),
                    num(left.1),
                    num(right.0),
                    num(right.1)
                ));
            }
            LineEndKind::Stealth => {
                let notch = (tip.0 - ux * length * 0.6, tip.1 - uy * length * 0.6);
                path.push_str(&format!(
                    "{} {} m\n{} {} l\n{} {} l\n{} {} l\nh\n",
                    num(tip.0),
                    num(tip.1),
                    num(left.0),
                    num(left.1),
                    num(notch.0),
                    num(notch.1),
                    num(right.0),
                    num(right.1)
                ));
            }
            LineEndKind::Diamond => {
                let far = (tip.0 - ux * length * 2.0, tip.1 - uy * length * 2.0);
                path.push_str(&format!(
                    "{} {} m\n{} {} l\n{} {} l\n{} {} l\nh\n",
                    num(tip.0),
                    num(tip.1),
                    num(left.0),
                    num(left.1),
                    num(far.0),
                    num(far.1),
                    num(right.0),
                    num(right.1)
                ));
            }
            LineEndKind::Oval => {
                let radius = half_w.max(length / 2.0);
                let center = (tip.0 - ux * radius, tip.1 - uy * radius);
                self.path_ellipse_points(
                    center.0 - radius,
                    center.1 - radius,
                    radius * 2.0,
                    radius * 2.0,
                );
                self.op("f");
                return;
            }
            LineEndKind::None => return,
        }
        self.bytes.extend_from_slice(path.as_bytes());
        self.op("f");
    }

    fn set_fill_color(&mut self, color: Color) {
        self.bytes.extend_from_slice(
            format!(
                "{} {} {} rg\n",
                channel(color.r),
                channel(color.g),
                channel(color.b)
            )
            .as_bytes(),
        );
    }

    fn set_stroke_color(&mut self, color: [u8; 4]) {
        self.bytes.extend_from_slice(
            format!(
                "{} {} {} RG\n",
                channel(color[0]),
                channel(color[1]),
                channel(color[2])
            )
            .as_bytes(),
        );
    }

    fn set_stroke(&mut self, stroke: &Stroke) {
        self.set_stroke_color([
            stroke.color.r,
            stroke.color.g,
            stroke.color.b,
            stroke.color.a,
        ]);
        self.bytes.extend_from_slice(
            format!("{} w\n", num((stroke.width * PX96_TO_POINTS).max(0.1))).as_bytes(),
        );
    }

    /// Sets the stroke dash pattern for a preset DrawingML dash style.
    fn set_dash(&mut self, dash: DashStyle, width: f32) {
        let unit = width.max(0.1);
        let pattern: &[f32] = match dash {
            DashStyle::Solid => &[],
            DashStyle::Dot | DashStyle::SystemDot => &[1.0, 2.0],
            DashStyle::Dash | DashStyle::SystemDash => &[4.0, 3.0],
            DashStyle::LargeDash => &[8.0, 3.0],
            DashStyle::DashDot | DashStyle::SystemDashDot => &[4.0, 3.0, 1.0, 3.0],
            DashStyle::LargeDashDot => &[8.0, 3.0, 1.0, 3.0],
            DashStyle::LargeDashDotDot | DashStyle::SystemDashDotDot => {
                &[8.0, 3.0, 1.0, 3.0, 1.0, 3.0]
            }
        };
        if pattern.is_empty() {
            self.op("[] 0 d");
            return;
        }
        let values: Vec<String> = pattern.iter().map(|step| num(step * unit)).collect();
        self.bytes
            .extend_from_slice(format!("[{}] 0 d\n", values.join(" ")).as_bytes());
    }

    /// Concatenates a rotation/flip about the object's own centre.
    ///
    /// The display list's rotation is clockwise in a y-down space; PDF user
    /// space is y-up, so the sine terms change sign.
    fn concat_transform(&mut self, transform: &ShapeTransform) {
        let cx = pt(transform.center.x);
        let cy = self.height - pt(transform.center.y);
        let (sx, sy) = (
            if transform.flip_h { -1.0_f32 } else { 1.0 },
            if transform.flip_v { -1.0_f32 } else { 1.0 },
        );
        let angle = f32::from(i16::try_from(transform.rotation / 60_000).unwrap_or(0)).to_radians();
        let (sin, cos) = angle.sin_cos();
        // Translate to the centre, flip, rotate, translate back.
        let (a, b, c, d) = (cos * sx, -sin * sx, sin * sy, cos * sy);
        let e = cx - (a * cx + c * cy);
        let f = cy - (b * cx + d * cy);
        self.bytes.extend_from_slice(
            format!(
                "{} {} {} {} {} {} cm\n",
                num(a),
                num(b),
                num(c),
                num(d),
                num(e),
                num(f)
            )
            .as_bytes(),
        );
    }
}

/// The arrowhead size multiplier for a `ST_LineEndWidth`/`ST_LineEndLength`
/// token, matching the raster backend's factors.
fn size_factor(size: Option<LineEndSize>) -> f32 {
    match size {
        Some(LineEndSize::Small) => 0.75,
        None | Some(LineEndSize::Medium) => 1.0,
        Some(LineEndSize::Large) => 1.5,
    }
}

/// Twips to PDF points.
fn pt(value: Twip) -> f32 {
    value.raw() as f32 / TWIPS_PER_POINT
}

/// The advance of one glyph in font design units.
fn advance_units(face: &FontRef<'_>, glyph: u16) -> f32 {
    face.glyph_metrics(FontSize::unscaled(), LocationRef::default())
        .advance_width(skrifa::GlyphId::new(u32::from(glyph)))
        .unwrap_or(0.0)
}

/// Where the whole picture must be placed, and at what size, so that the
/// fraction of it the crop leaves visible exactly fills the destination box.
fn cropped_placement(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    crop: &CropRect,
) -> Option<(f32, f32, f32, f32)> {
    let full = CROP_FULL as f32;
    let left = crop.left.clamp(0, CROP_FULL) as f32 / full;
    let right = crop.right.clamp(0, CROP_FULL) as f32 / full;
    let top = crop.top.clamp(0, CROP_FULL) as f32 / full;
    let bottom = crop.bottom.clamp(0, CROP_FULL) as f32 / full;
    let visible_w = 1.0 - left - right;
    let visible_h = 1.0 - top - bottom;
    if visible_w <= f32::EPSILON || visible_h <= f32::EPSILON {
        return None;
    }
    let full_w = width / visible_w;
    let full_h = height / visible_h;
    // `top` hides the top of the source and PDF y grows upward, so the
    // oversized placement hangs below the destination box by the hidden
    // bottom fraction.
    Some((x - left * full_w, y - bottom * full_h, full_w, full_h))
}

/// The bounding rectangle of a shape geometry, for fitting a gradient to it.
fn geometry_bounds(geometry: &ShapeGeometry) -> Option<Rect> {
    match geometry {
        ShapeGeometry::Rect { rect }
        | ShapeGeometry::Ellipse { rect }
        | ShapeGeometry::RoundedRect { rect, .. } => Some(*rect),
        ShapeGeometry::Polygon { points } => {
            let first = points.first()?;
            let (mut min_x, mut min_y, mut max_x, mut max_y) =
                (first.x.raw(), first.y.raw(), first.x.raw(), first.y.raw());
            for point in points {
                min_x = min_x.min(point.x.raw());
                min_y = min_y.min(point.y.raw());
                max_x = max_x.max(point.x.raw());
                max_y = max_y.max(point.y.raw());
            }
            Some(Rect::new(
                Point::new(Twip(min_x), Twip(min_y)),
                casual_doc_layout::units::Size::new(Twip(max_x - min_x), Twip(max_y - min_y)),
            ))
        }
        // A zero-area line has no interior to fill.
        ShapeGeometry::Line { .. } => None,
    }
}

/// Builds an axial (`ShadingType 2`) or radial (`ShadingType 3`) shading
/// dictionary covering the shape's bounding box.
fn shading_dictionary(
    gradient: &Gradient,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> Option<String> {
    if gradient.stops.is_empty() {
        return None;
    }
    let function = stitched_function(gradient);
    let coords = match gradient.kind {
        GradientKind::Linear { angle_deg } => {
            // The display list's angle is clockwise from +x in a y-down space.
            let angle = angle_deg.to_radians();
            let (sin, cos) = angle.sin_cos();
            let (dx, dy) = (cos, -sin);
            let half = (width * dx.abs() + height * dy.abs()) / 2.0;
            let (cx, cy) = (x + width / 2.0, y + height / 2.0);
            format!(
                "/Coords[{} {} {} {}]",
                num(cx - dx * half),
                num(cy - dy * half),
                num(cx + dx * half),
                num(cy + dy * half)
            )
        }
        GradientKind::Radial => {
            let (cx, cy) = (x + width / 2.0, y + height / 2.0);
            let radius = (width * width + height * height).sqrt() / 2.0;
            format!(
                "/Coords[{} {} 0 {} {} {}]",
                num(cx),
                num(cy),
                num(cx),
                num(cy),
                num(radius)
            )
        }
    };
    let shading_type = match gradient.kind {
        GradientKind::Linear { .. } => 2,
        GradientKind::Radial => 3,
    };
    Some(format!(
        "<</ShadingType {shading_type}/ColorSpace/DeviceRGB{coords}/Function {function}/Extend[true true]>>"
    ))
}

/// A type-3 stitching function over the gradient's stops, or a single type-2
/// exponential when there are only two.
fn stitched_function(gradient: &Gradient) -> String {
    let mut stops = gradient.stops.clone();
    stops.sort_by(|a, b| a.position.total_cmp(&b.position));
    if stops.len() == 1 {
        let color = stops[0].color;
        return exponential(color, color);
    }
    let mut functions = Vec::new();
    let mut bounds = Vec::new();
    let mut encode = Vec::new();
    for pair in stops.windows(2) {
        functions.push(exponential(pair[0].color, pair[1].color));
        encode.push("0 1".to_owned());
    }
    for stop in &stops[1..stops.len() - 1] {
        bounds.push(num(stop.position.clamp(0.0, 1.0)));
    }
    if functions.len() == 1 {
        return functions.remove(0);
    }
    format!(
        "<</FunctionType 3/Domain[0 1]/Functions[{}]/Bounds[{}]/Encode[{}]>>",
        functions.join(""),
        bounds.join(" "),
        encode.join(" ")
    )
}

fn exponential(from: Color, to: Color) -> String {
    format!(
        "<</FunctionType 2/Domain[0 1]/C0[{} {} {}]/C1[{} {} {}]/N 1>>",
        channel(from.r),
        channel(from.g),
        channel(from.b),
        channel(to.r),
        channel(to.g),
        channel(to.b)
    )
}
