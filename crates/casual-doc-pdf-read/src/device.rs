//! The evidence-collecting [`Device`]. Complexity: O(paint operations) per
//! page, plus image decode, all bounded by [`PdfReadLimits`].

use std::collections::{BTreeMap, BTreeSet};

use hayro_interpret::font::{Glyph, GlyphRun};
use hayro_interpret::hayro_cmap::BfString;
use hayro_interpret::{
    BlendMode, ClipPath, Device, DrawMode, DrawProps, FillRule, Image, ImageData, ImageDrawProps,
    LumaData, Paint, SoftMask,
};
use hayro_syntax::Filter;
use hayro_syntax::object::Name;
use kurbo::{Affine, BezPath, PathEl, Shape};

use crate::evidence::{
    Bounds, DocumentFonts, FontEvidence, FontProgramKind, GlyphEvidence, GlyphRunEvidence,
    ImageEvidence, ImageFormat, PageNotes, PaintEvidence, PathCommand, PathEvidence, Point,
    RenderMode, Rgba8, StrokeEvidence,
};
// Own line: a page-sized picture's ink bands.
use crate::evidence::{INK_MEASURED_AREA, InkBands};
use crate::limits::check;
use crate::{PdfReadError, PdfReadLimits};

/// The font index used for every Type3 glyph (hayro does not expose a Type3
/// font's identity on the glyph).
const TYPE3_KEY: u128 = u128::MAX;

/// One open marked-content sequence: the document's own, or the numbered
/// marker [`crate::content::instrument`] put around a show operator.
enum Mark {
    /// `(is /Artifact, MCID)`.
    Content(bool, Option<i32>),
    /// The show index of the operator painting inside it.
    Show(u32),
}

pub(crate) struct Collector<'c> {
    limits: &'c PdfReadLimits,
    fonts: &'c mut DocumentFonts,
    index: &'c mut BTreeMap<u128, usize>,
    programs_tried: &'c mut BTreeSet<usize>,
    clips: Vec<Option<Bounds>>,
    /// The opacity of every transparency group open around the paint, the
    /// product of the stack (`ca` of the graphics state an image is drawn
    /// under, a form's group alpha): what makes a picture a faint stamp.
    opacity: Vec<f32>,
    marked: Vec<Mark>,
    items: Vec<PaintEvidence>,
    notes: PageNotes,
    evidence: u64,
    error: Option<PdfReadError>,
}

fn point(affine: Affine, x: f64, y: f64) -> Point {
    let p = affine * kurbo::Point::new(x, y);
    Point { x: p.x, y: p.y }
}

fn color(paint: &Paint<'_>, notes: &mut PageNotes) -> Option<Rgba8> {
    match paint {
        Paint::Color(c) => Some(c.to_rgba().to_rgba8()),
        Paint::Pattern(_) => {
            notes.pattern_paints += 1;
            None
        }
    }
}

fn codec(filter: &Filter) -> &'static str {
    match filter {
        Filter::AsciiHexDecode => "ASCIIHex",
        Filter::Ascii85Decode => "ASCII85",
        Filter::LzwDecode => "LZW",
        Filter::FlateDecode => "Flate",
        Filter::RunLengthDecode => "RunLength",
        Filter::CcittFaxDecode => "CCITTFax",
        Filter::Jbig2Decode => "JBIG2",
        Filter::DctDecode => "DCT",
        Filter::JpxDecode => "JPX",
        Filter::Crypt => "Crypt",
    }
}

fn program_kind(bytes: &[u8]) -> FontProgramKind {
    match bytes.first_chunk::<4>() {
        Some(b"\x00\x01\x00\x00" | b"true") => FontProgramKind::TrueType,
        Some(b"OTTO") => FontProgramKind::OpenTypeCff,
        Some([1, 0, _, _]) => FontProgramKind::BareCff,
        _ => FontProgramKind::Other,
    }
}

/// Encodes 8-bit pixels as PNG. `channels` is 1, 2, 3 or 4.
fn png(width: u32, height: u32, channels: u8, data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(match channels {
        1 => png::ColorType::Grayscale,
        2 => png::ColorType::GrayscaleAlpha,
        3 => png::ColorType::Rgb,
        _ => png::ColorType::Rgba,
    });
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().ok()?;
    writer.write_image_data(data).ok()?;
    writer.finish().ok()?;
    Some(out)
}

/// Samples `alpha` (nearest neighbour) at the image's resolution.
fn alpha_at(alpha: &LumaData, width: u32, height: u32, x: u32, y: u32) -> u8 {
    let ax = (u64::from(x) * u64::from(alpha.width) / u64::from(width.max(1))) as usize;
    let ay = (u64::from(y) * u64::from(alpha.height) / u64::from(height.max(1))) as usize;
    alpha
        .data
        .get(ay * alpha.width as usize + ax)
        .copied()
        .unwrap_or(255)
}

fn raster_png(data: &ImageData, alpha: Option<&LumaData>) -> Option<(Vec<u8>, (u32, u32))> {
    let (width, height) = (data.width(), data.height());
    match (data, alpha) {
        (ImageData::Rgb(rgb), None) => Some((png(width, height, 3, &rgb.data)?, (width, height))),
        (ImageData::Luma(luma), None) => {
            Some((png(width, height, 1, &luma.data)?, (width, height)))
        }
        (ImageData::Rgb(rgb), Some(alpha)) => {
            let mut rgba = Vec::with_capacity(rgb.data.len() / 3 * 4);
            for y in 0..height {
                for x in 0..width {
                    let at = ((y * width + x) * 3) as usize;
                    rgba.extend_from_slice(rgb.data.get(at..at + 3)?);
                    rgba.push(alpha_at(alpha, width, height, x, y));
                }
            }
            Some((png(width, height, 4, &rgba)?, (width, height)))
        }
        (ImageData::Luma(luma), Some(alpha)) => {
            let mut la = Vec::with_capacity(luma.data.len() * 2);
            for y in 0..height {
                for x in 0..width {
                    la.push(*luma.data.get((y * width + x) as usize)?);
                    la.push(alpha_at(alpha, width, height, x, y));
                }
            }
            Some((png(width, height, 2, &la)?, (width, height)))
        }
    }
}

impl<'c> Collector<'c> {
    pub(crate) fn new(
        limits: &'c PdfReadLimits,
        fonts: &'c mut DocumentFonts,
        index: &'c mut BTreeMap<u128, usize>,
        programs_tried: &'c mut BTreeSet<usize>,
        page: Bounds,
    ) -> Self {
        Self {
            limits,
            fonts,
            index,
            programs_tried,
            clips: vec![Some(page)],
            opacity: vec![1.0],
            marked: Vec::new(),
            items: Vec::new(),
            notes: PageNotes::default(),
            evidence: 0,
            error: None,
        }
    }

    pub(crate) fn finish(self) -> Result<(Vec<PaintEvidence>, PageNotes), PdfReadError> {
        match self.error {
            Some(error) => Err(error),
            None => Ok((self.items, self.notes)),
        }
    }

    fn fail(&mut self, result: Result<(), PdfReadError>) -> bool {
        if let Err(error) = result {
            self.error.get_or_insert(error);
        }
        self.error.is_some()
    }

    fn count(&mut self, n: u64) -> bool {
        self.evidence += n;
        let result = check(
            "max_evidence_per_page",
            self.evidence,
            self.limits.max_evidence_per_page,
        );
        self.fail(result)
    }

    fn clip(&self) -> Option<Bounds> {
        self.clips.last().copied().flatten()
    }

    fn note_transparency(&mut self, opacity_below_one: bool, mask: bool, blend: BlendMode) {
        if opacity_below_one || mask || blend != BlendMode::Normal {
            self.notes.transparency += 1;
        }
    }

    /// The font index for a hayro font key, creating an identity from the
    /// glyph's own metadata when the resources did not declare it.
    fn font_for(&mut self, key: u128, glyph: &Glyph<'_>) -> Result<usize, PdfReadError> {
        let index = match self.index.get(&key) {
            Some(index) => *index,
            None => {
                check(
                    "max_fonts",
                    self.fonts.fonts.len() as u64 + 1,
                    self.limits.max_fonts,
                )?;
                let data = match glyph {
                    Glyph::Outline(outline) => outline.font_data(),
                    Glyph::Type3(_) => None,
                };
                let evidence = FontEvidence {
                    name: data
                        .as_ref()
                        .and_then(|d| d.postscript_name.clone())
                        .unwrap_or_else(|| {
                            if key == TYPE3_KEY {
                                "Type3".to_owned()
                            } else {
                                String::new()
                            }
                        }),
                    family: None,
                    bold: data
                        .as_ref()
                        .and_then(|d| d.weight)
                        .is_some_and(|w| w >= 600),
                    italic: data.as_ref().is_some_and(|d| d.is_italic),
                    serif: data.as_ref().is_some_and(|d| d.is_serif),
                    monospace: data.as_ref().is_some_and(|d| d.is_monospace),
                    subtype: if key == TYPE3_KEY {
                        "Type3".to_owned()
                    } else {
                        String::new()
                    },
                    program: None,
                };
                let index = self.fonts.fonts.len();
                self.index.insert(key, index);
                self.fonts.fonts.push(evidence);
                index
            }
        };
        if self.programs_tried.insert(index)
            && let Glyph::Outline(outline) = glyph
            && let Some(data) = outline.font_data()
        {
            let bytes: &[u8] = (*data.data).as_ref();
            check(
                "max_font_program_bytes",
                bytes.len() as u64,
                self.limits.max_font_program_bytes,
            )?;
            self.fonts.fonts[index].program = Some((program_kind(bytes), bytes.to_vec()));
        }
        Ok(index)
    }

    fn image_evidence(
        &mut self,
        transform: Affine,
        width: u32,
        height: u32,
        encoded: Option<(Vec<u8>, ImageFormat, (u32, u32))>,
        codecs: Vec<&'static str>,
        ink: Option<InkBands>,
    ) {
        let Some((bytes, format, pixels)) = encoded else {
            self.notes.undecodable_images += 1;
            return;
        };
        let (w, h) = (f64::from(width), f64::from(height));
        let origin = point(transform, 0.0, 0.0);
        let right = point(transform, w, 0.0);
        let down = point(transform, 0.0, h);
        let col0 = (right.x - origin.x, right.y - origin.y);
        let col1 = (down.x - origin.x, down.y - origin.y);
        let size = (col0.0.hypot(col0.1), col1.0.hypot(col1.1));
        let center = point(transform, w / 2.0, h / 2.0);
        let rotation_degrees = col0.1.atan2(col0.0).to_degrees();
        let flip_v = col0.0 * col1.1 - col0.1 * col1.0 < 0.0;
        let bounds = Bounds {
            x0: center.x - size.0 / 2.0,
            y0: center.y - size.1 / 2.0,
            x1: center.x + size.0 / 2.0,
            y1: center.y + size.1 / 2.0,
        };
        let clip = self.clip().and_then(|clip| {
            if clip.contains(&bounds, 0.5) {
                None
            } else {
                Some(clip.intersect(&bounds).unwrap_or(Bounds {
                    x0: bounds.x0,
                    y0: bounds.y0,
                    x1: bounds.x0,
                    y1: bounds.y0,
                }))
            }
        });
        self.items.push(PaintEvidence::Image(ImageEvidence {
            bytes,
            format,
            pixels,
            codecs,
            center,
            size,
            rotation_degrees,
            flip_v,
            clip,
            bounds,
            ink,
            opacity: self.opacity.last().copied().unwrap_or(1.0),
        }));
    }
}

/// Whether a picture drawn with `transform` over `width` x `height` pixels
/// covers at least [`INK_MEASURED_AREA`]. O(1).
fn page_sized(transform: Affine, width: u32, height: u32) -> bool {
    let (w, h) = (f64::from(width), f64::from(height));
    let origin = point(transform, 0.0, 0.0);
    let right = point(transform, w, 0.0);
    let down = point(transform, 0.0, h);
    let across = (right.x - origin.x).hypot(right.y - origin.y);
    let tall = (down.x - origin.x).hypot(down.y - origin.y);
    across * tall >= INK_MEASURED_AREA
}

/// The ink bands of decoded samples (`InkBands`). O(pixels).
fn ink_bands(data: &ImageData) -> Option<InkBands> {
    let (width, height) = (data.width() as usize, data.height() as usize);
    if width == 0 || height < 8 {
        return None;
    }
    // Per row: whether any pixel is ink. Sampled every other column.
    let dark = |y: usize| -> bool {
        (0..width).step_by(2).any(|x| {
            let luma = match data {
                ImageData::Rgb(rgb) => {
                    let at = (y * width + x) * 3;
                    match rgb.data.get(at..at + 3) {
                        Some(&[r, g, b]) => {
                            (299 * u32::from(r) + 587 * u32::from(g) + 114 * u32::from(b)) / 1000
                        }
                        _ => 255,
                    }
                }
                ImageData::Luma(luma) => u32::from(*luma.data.get(y * width + x).unwrap_or(&255)),
            };
            luma < 180
        })
    };
    let rows: Vec<bool> = (0..height).map(dark).collect();
    // A band: ink within the outer quarter, then a blank gap of at least 2%
    // of the height before any more ink.
    let gap = (height / 50).max(2);
    let band = |order: &mut dyn Iterator<Item = usize>| -> Option<usize> {
        let mut seen = false;
        let mut blank = 0;
        let mut last_ink = 0;
        for (n, y) in order.enumerate() {
            if rows[y] {
                if blank >= gap && seen {
                    break;
                }
                seen = true;
                blank = 0;
                last_ink = n;
            } else if seen {
                blank += 1;
                if blank >= gap {
                    return Some(last_ink + 1);
                }
            }
            if !seen && n > height / 4 {
                return None;
            }
        }
        None
    };
    let top = band(&mut (0..height))
        .filter(|&n| n <= height / 4)
        .unwrap_or(0);
    let bottom = band(&mut (0..height).rev())
        .filter(|&n| n <= height / 4)
        .map_or(height, |n| height - n);
    Some(InkBands {
        top: top as f32 / height as f32,
        bottom: bottom as f32 / height as f32,
    })
}

fn path_commands(path: &BezPath, transform: Affine) -> (Vec<PathCommand>, Bounds) {
    let mut commands = Vec::new();
    let mut bounds = Bounds {
        x0: f64::INFINITY,
        y0: f64::INFINITY,
        x1: f64::NEG_INFINITY,
        y1: f64::NEG_INFINITY,
    };
    let mut grow = |p: Point| {
        bounds.x0 = bounds.x0.min(p.x);
        bounds.y0 = bounds.y0.min(p.y);
        bounds.x1 = bounds.x1.max(p.x);
        bounds.y1 = bounds.y1.max(p.y);
        p
    };
    for element in path.elements() {
        let map = |p: kurbo::Point| point(transform, p.x, p.y);
        commands.push(match *element {
            PathEl::MoveTo(p) => PathCommand::MoveTo(grow(map(p))),
            PathEl::LineTo(p) => PathCommand::LineTo(grow(map(p))),
            PathEl::QuadTo(c, p) => PathCommand::QuadTo(grow(map(c)), grow(map(p))),
            PathEl::CurveTo(c1, c2, p) => {
                PathCommand::CubicTo(grow(map(c1)), grow(map(c2)), grow(map(p)))
            }
            PathEl::ClosePath => PathCommand::Close,
        });
    }
    if commands.is_empty() {
        bounds = Bounds::default();
    }
    (commands, bounds)
}

fn bf_text(text: BfString) -> String {
    match text {
        BfString::Char(c) => c.to_string(),
        BfString::String(s) => s,
    }
}

impl<'a> Device<'a> for Collector<'_> {
    fn draw_path(&mut self, path: &BezPath, props: DrawProps<'a>, mode: &DrawMode) {
        if self.error.is_some() || self.count(1) {
            return;
        }
        self.note_transparency(
            matches!(&props.paint, Paint::Color(c) if c.to_rgba().to_rgba8()[3] < 255),
            props.soft_mask.is_some(),
            props.blend_mode,
        );
        let (commands, bounds) = path_commands(path, props.transform);
        if commands.is_empty() {
            return;
        }
        let scale = props.transform.determinant().abs().sqrt();
        let stroke_of =
            |stroke: &hayro_interpret::StrokeProps, notes: &mut PageNotes| StrokeEvidence {
                color: color(&props.paint, notes),
                width: f64::from(stroke.line_width) * scale,
                dashed: !stroke.dash_array.is_empty(),
            };
        let clipped = self.clip().is_some_and(|clip| !clip.contains(&bounds, 0.5));
        let (fill, even_odd, stroke) = match mode {
            DrawMode::Fill(rule) => (
                Some(color(&props.paint, &mut self.notes)),
                *rule == FillRule::EvenOdd,
                None,
            ),
            DrawMode::Stroke(stroke) => {
                let stroke = stroke_of(stroke, &mut self.notes);
                // `B`/`b` reach the device as a fill then a stroke of the same
                // path; they are one object in the source and in the model.
                if let Some(PaintEvidence::Path(previous)) = self.items.last_mut()
                    && previous.stroke.is_none()
                    && previous.fill.is_some()
                    && previous.commands == commands
                {
                    previous.stroke = Some(stroke);
                    return;
                }
                (None, false, Some(stroke))
            }
            DrawMode::FillAndStroke(rule, stroke) => (
                Some(color(&props.paint, &mut self.notes)),
                *rule == FillRule::EvenOdd,
                Some(stroke_of(stroke, &mut self.notes)),
            ),
            DrawMode::Invisible => return,
        };
        self.items.push(PaintEvidence::Path(PathEvidence {
            commands,
            fill,
            even_odd,
            stroke,
            bounds,
            clipped,
        }));
    }

    fn push_clip_path(&mut self, clip: &ClipPath) {
        let bbox = clip.path.bounding_box();
        let bounds = Bounds {
            x0: bbox.x0,
            y0: bbox.y0,
            x1: bbox.x1,
            y1: bbox.y1,
        };
        let next = self.clip().and_then(|current| current.intersect(&bounds));
        // An empty intersection clips everything; keep it as a zero box so a
        // later paint is seen as clipped rather than unclipped.
        self.clips.push(Some(next.unwrap_or(Bounds {
            x0: bounds.x0,
            y0: bounds.y0,
            x1: bounds.x0,
            y1: bounds.y0,
        })));
    }

    fn push_transparency_group(
        &mut self,
        opacity: f32,
        mask: Option<SoftMask<'a>>,
        blend_mode: BlendMode,
    ) {
        self.note_transparency(opacity < 0.999, mask.is_some(), blend_mode);
        let outer = self.opacity.last().copied().unwrap_or(1.0);
        self.opacity.push(outer * opacity.clamp(0.0, 1.0));
    }

    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, mode: &DrawMode) {
        if self.error.is_some() || self.count(run.glyphs().len() as u64) {
            return;
        }
        let Some(first) = run.glyphs().first() else {
            return;
        };
        let key = match &**first {
            Glyph::Outline(outline) => outline.font_cache_key(),
            Glyph::Type3(_) => TYPE3_KEY,
        };
        let font = match self.font_for(key, first) {
            Ok(font) => font,
            Err(error) => {
                self.error.get_or_insert(error);
                return;
            }
        };
        let first_transform = props.transform * first.transform();
        let [a, b, c, d, _, _] = first_transform.as_coeffs();
        let size = c.hypot(d) * 1000.0;
        let rotation_degrees = b.atan2(a).to_degrees();
        let oblique = oblique([a, b, c, d]);
        let along = a.hypot(b);
        let codes = crate::unicode::FontCodes::of(&self.fonts.fonts[font].name);
        let mut glyphs = Vec::with_capacity(run.glyphs().len());
        for glyph in run.glyphs() {
            let transform = props.transform * glyph.transform();
            let origin = point(transform, 0.0, 0.0);
            let advance = match &**glyph {
                Glyph::Outline(outline) => outline
                    .advance_width()
                    .map_or(0.0, |w| f64::from(w) * along),
                Glyph::Type3(_) => {
                    self.notes.type3_glyphs += 1;
                    0.0
                }
            };
            // The text made fit for editing (`unicode`): ligatures
            // decomposed, symbol-font codes mapped, what is not text dropped.
            let normalized =
                crate::unicode::normalize(glyph.as_unicode().map(bf_text).as_deref(), codes);
            self.notes.ligatures_decomposed += u32::from(normalized.ligature);
            self.notes.symbols_mapped += u32::from(normalized.symbol);
            glyphs.push(GlyphEvidence {
                text: normalized.text,
                origin,
                advance,
                glyph: match &**glyph {
                    Glyph::Outline(outline) => Some(outline.glyph_id().to_u32()),
                    Glyph::Type3(_) => None,
                },
            });
        }
        // Type3 glyphs carry no advance; take it from the next glyph's origin.
        for at in 1..glyphs.len() {
            if glyphs[at - 1].advance == 0.0 {
                let (p, q) = (glyphs[at - 1].origin, glyphs[at].origin);
                glyphs[at - 1].advance = (q.x - p.x).hypot(q.y - p.y);
            }
        }
        // Marks and accents drawn as their own glyphs join their letter
        // (upright runs only: the overlap test is along the baseline).
        if rotation_degrees.abs() < 0.5 {
            self.notes.marks_composed += crate::unicode::compose_run(&mut glyphs);
        }
        let render = match mode {
            DrawMode::Fill(_) => RenderMode::Fill,
            DrawMode::Stroke(_) => RenderMode::Stroke,
            DrawMode::FillAndStroke(..) => RenderMode::FillStroke,
            DrawMode::Invisible => RenderMode::Invisible,
        };
        // Render modes 2 and 6 reach the device as a fill then a stroke of the
        // same glyphs: one run in the source, one run in the model.
        if render == RenderMode::Stroke
            && let Some(PaintEvidence::Glyphs(previous)) = self.items.last_mut()
            && previous.mode == RenderMode::Fill
            && previous.glyphs == glyphs
        {
            previous.mode = RenderMode::FillStroke;
            return;
        }
        let fill = color(&props.paint, &mut self.notes);
        self.note_transparency(
            fill.is_some_and(|f| f[3] < 255),
            props.soft_mask.is_some(),
            props.blend_mode,
        );
        if let Some(clip) = self.clip() {
            let cut = glyphs.iter().any(|g| {
                g.origin.x < clip.x0 - 0.5
                    || g.origin.x > clip.x1 + 0.5
                    || g.origin.y < clip.y0 - 0.5
                    || g.origin.y > clip.y1 + 0.5
            });
            if cut {
                self.notes.clipped_text += 1;
            }
        }
        let (artifact, mcid) = self
            .marked
            .iter()
            .fold((false, None), |(artifact, mcid), mark| match mark {
                Mark::Content(a, m) => (artifact || *a, m.or(mcid)),
                Mark::Show(_) => (artifact, mcid),
            });
        let show = self.marked.iter().rev().find_map(|mark| match mark {
            Mark::Show(show) => Some(*show),
            Mark::Content(..) => None,
        });
        self.items.push(PaintEvidence::Glyphs(GlyphRunEvidence {
            font,
            size,
            rotation_degrees,
            oblique,
            fill,
            mode: render,
            glyphs,
            mcid,
            artifact,
            show,
        }));
    }

    fn draw_image(&mut self, image: Image<'a, '_>, props: ImageDrawProps<'a>) {
        if self.error.is_some() || self.count(1) {
            return;
        }
        let (width, height) = (image.width(), image.height());
        let pixels = u64::from(width) * u64::from(height);
        let result = check("max_image_pixels", pixels, self.limits.max_image_pixels);
        if self.fail(result) {
            return;
        }
        match image {
            Image::Raster(raster) => {
                let stream = raster.stream();
                let filters = stream.filters();
                let codecs: Vec<&'static str> = if filters.is_empty() {
                    vec!["raw"]
                } else {
                    filters.iter().map(codec).collect()
                };
                // A plain DeviceRGB/DeviceGray JPEG with no mask or decode
                // array is carried through unchanged: re-encoding a photo as
                // PNG would multiply its size for no fidelity gain.
                let dict = stream.dict();
                let plain_colour = dict
                    .get::<Name<'_>>(b"ColorSpace")
                    .is_some_and(|name| matches!(name.as_str(), "DeviceRGB" | "DeviceGray"));
                let passthrough = filters.as_slice() == [Filter::DctDecode]
                    && plain_colour
                    && !dict.contains_key(b"SMask")
                    && !dict.contains_key(b"Mask")
                    && !dict.contains_key(b"Decode");
                let mut encoded = None;
                let mut ink = None;
                // A page-sized picture's ink bands (a letterhead's logo band)
                // are measured from its samples, once.
                let measure = page_sized(props.transform, width, height);
                if passthrough {
                    encoded = Some((
                        stream.raw_data().into_owned(),
                        ImageFormat::Jpeg,
                        (width, height),
                    ));
                    if measure {
                        raster.with_rgba(|data, _| ink = ink_bands(&data), None);
                    }
                } else {
                    raster.with_rgba(
                        |data, alpha| {
                            if measure {
                                ink = ink_bands(&data);
                            }
                            encoded = raster_png(&data, alpha.as_ref())
                                .map(|(bytes, pixels)| (bytes, ImageFormat::Png, pixels));
                        },
                        None,
                    );
                }
                self.image_evidence(props.transform, width, height, encoded, codecs, ink);
            }
            Image::Stencil(stencil) => {
                let mut encoded = None;
                let mut notes = PageNotes::default();
                stencil.with_stencil(
                    |luma, paint| {
                        let [r, g, b, a] = color(paint, &mut notes).unwrap_or([0, 0, 0, 255]);
                        let mut rgba = Vec::with_capacity(luma.data.len() * 4);
                        for coverage in &luma.data {
                            rgba.extend_from_slice(&[
                                r,
                                g,
                                b,
                                (u16::from(*coverage) * u16::from(a) / 255) as u8,
                            ]);
                        }
                        encoded = png(luma.width, luma.height, 4, &rgba)
                            .map(|bytes| (bytes, ImageFormat::Png, (luma.width, luma.height)));
                    },
                    None,
                );
                self.notes.pattern_paints += notes.pattern_paints;
                self.image_evidence(
                    props.transform,
                    width,
                    height,
                    encoded,
                    vec!["stencil"],
                    None,
                );
            }
        }
    }

    fn pop_clip(&mut self) {
        if self.clips.len() > 1 {
            self.clips.pop();
        }
    }

    fn pop_transparency_group(&mut self) {
        if self.opacity.len() > 1 {
            self.opacity.pop();
        }
    }

    fn begin_marked_content(&mut self, tag: &[u8], mcid: Option<i32>) {
        self.marked.push(match crate::content::show_of_tag(tag) {
            Some(show) if mcid.is_none() => Mark::Show(show),
            _ => Mark::Content(tag == b"Artifact", mcid),
        });
    }

    fn end_marked_content(&mut self) {
        self.marked.pop();
    }
}

/// Whether a glyph transform's linear part `[a, b, c, d]` (the baseline's
/// direction is `(a, b)`, the glyph's vertical `(c, d)`) slants the glyphs
/// as a synthesised italic does: the vertical leans 6 to 30 degrees off the
/// baseline's perpendicular, whichever way the page's axes run. A rotated
/// run that is not sheared is upright (its two axes stay perpendicular).
/// O(1).
pub(crate) fn oblique([a, b, c, d]: [f64; 4]) -> bool {
    if a.hypot(b) < 1e-9 || c.hypot(d) < 1e-9 {
        return false;
    }
    let mut between = (d.atan2(c) - b.atan2(a)).to_degrees().abs() % 360.0;
    if between > 180.0 {
        between = 360.0 - between;
    }
    let slant = (90.0 - between).abs();
    (6.0..=30.0).contains(&slant)
}
