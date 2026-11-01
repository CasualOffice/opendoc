// SPDX-License-Identifier: Apache-2.0

//! A page's display list as SVG (`docs/176` §12.13): the vector backend beside
//! the raster one ([`crate::render`]) and the PDF writer, painting the same
//! [`DisplayList`] the layout produced, so an SVG page is the PNG page and the
//! PDF page.
//!
//! # What it writes
//!
//! - **Coordinates** are the display list's own page twips, in a `viewBox` the
//!   size of the page; `width` and `height` are in points.
//! - **Glyphs** are painted as their outlines from the exact face the shaper
//!   used (skrifa, as the raster backend draws them), each distinct glyph once
//!   in `<defs>` and placed with `<use>`, so a page looks the same in every
//!   viewer whatever fonts it has.
//! - **Real text.** Over each run's outlines lies a `<text>` element carrying
//!   the run's characters, recovered from the paragraph through the glyphs'
//!   cluster offsets (the mechanism the PDF writer's `ToUnicode` uses), drawn
//!   transparent and stretched to the run's advance, so a viewer can select,
//!   search and read it aloud. Where the characters cannot be recovered (a run
//!   with no source paragraph, such as a list marker drawn from a symbol font),
//!   there is the outline alone.
//! - **Pictures** are embedded as `data:` URIs: PNG, JPEG, GIF and WebP as
//!   their own bytes, BMP and TIFF re-encoded as PNG (browsers do not show
//!   them), anything undecodable as the raster backend's crossed placeholder.
//! - **Shapes, clips and layers** map to SVG paths, `clipPath`s and groups
//!   (a multiplied layer as `mix-blend-mode: multiply`, a shadow blurred with
//!   `feGaussianBlur`).
//!
//! # What it does not (stated, not hidden)
//!
//! - Colour glyphs (COLR, bitmap emoji) are drawn from their outline glyph
//!   when the face has one, in the run's colour, and are otherwise omitted:
//!   the raster backend paints them in colour.
//! - Arrowheads on lines (`a:headEnd`/`a:tailEnd`) are not drawn.
//! - Glyph ink effects (shadow, emboss, outline) are not applied.
//!
//! # Determinism
//!
//! Numbers are written with one decimal in twips (a twentieth of a point
//! times ten) and ids are assigned in paint order, so the same list and faces
//! give the same bytes on every platform.
//!
//! # Complexity
//!
//! **O(items + glyphs + image bytes)** for one page: one pass to collect the
//! glyphs' cluster boundaries, one to paint; each distinct glyph is outlined
//! once.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::io::Cursor;

use casual_doc_layout::display::{
    Color, DisplayList, Fill, GradientKind, LayerBlend, PaintItem, PathCommand, ShapeGeometry,
    ShapeOutline, ShapeTransform, Stroke,
};
use casual_doc_layout::text::GlyphRun;
use casual_doc_layout::units::{Point, Rect, Twip};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    CROP_FULL, CropRect, DashStyle, FillRule as ModelFillRule, OPACITY_FULL,
};
use skrifa::instance::{LocationRef, Size};
use skrifa::metrics::Metrics;
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, GlyphId, MetadataProvider};

use crate::{GlyphSource, MediaSource};

/// Twips per device pixel at 96 dpi: a stroke width in the display list is in
/// pixels at the editor's 96 dpi.
const TWIPS_PER_PX: f32 = 15.0;

/// Supplies a paragraph's text, in the byte layout its glyphs' cluster offsets
/// index, for the real-text layer.
pub trait SvgTextSource {
    /// The text of paragraph `node`, or `None` when unknown. Called at most once
    /// per node per page.
    fn node_text(&self, node: NodeId) -> Option<String>;
}

/// A text source that knows nothing: outlines only.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoSvgText;

impl SvgTextSource for NoSvgText {
    fn node_text(&self, _node: NodeId) -> Option<String> {
        None
    }
}

/// One decimal, no `-0.0`, no trailing `.0`.
fn num(value: f32) -> String {
    let rounded = (value * 10.0).round() / 10.0;
    let rounded = if rounded == 0.0 { 0.0 } else { rounded };
    let text = format!("{rounded:.1}");
    text.strip_suffix(".0").map_or(text.clone(), str::to_owned)
}

fn tw(value: Twip) -> String {
    num(value.raw() as f32)
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // XML 1.0 forbids most C0 controls; a tab or newline in a run reads
            // as a space.
            c if (c as u32) < 0x20 => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

fn color(c: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

fn rgba(c: [u8; 4]) -> Color {
    Color {
        r: c[0],
        g: c[1],
        b: c[2],
        a: c[3],
    }
}

/// `fill="…"` (with opacity when not opaque), or `fill="none"`.
fn fill_attr(fill: Option<Color>) -> String {
    match fill {
        Some(c) if c.a == 255 => format!(" fill=\"{}\"", color(c)),
        Some(c) => format!(
            " fill=\"{}\" fill-opacity=\"{:.3}\"",
            color(c),
            f32::from(c.a) / 255.0
        ),
        None => " fill=\"none\"".to_owned(),
    }
}

fn stroke_attr(color_: Color, width_px: f32, dash: Option<&[f32]>) -> String {
    let mut out = format!(
        " stroke=\"{}\" stroke-width=\"{}\"",
        color(color_),
        num(width_px * TWIPS_PER_PX)
    );
    if color_.a != 255 {
        let _ = write!(
            out,
            " stroke-opacity=\"{:.3}\"",
            f32::from(color_.a) / 255.0
        );
    }
    if let Some(dash) = dash {
        let pattern: Vec<String> = dash
            .iter()
            .map(|d| num(d * width_px.max(1.0) * TWIPS_PER_PX))
            .collect();
        let _ = write!(out, " stroke-dasharray=\"{}\"", pattern.join(" "));
    }
    out
}

fn plain_stroke(stroke: Option<&Stroke>) -> String {
    stroke.map_or_else(String::new, |s| stroke_attr(s.color, s.width, None))
}

/// The dash pattern (in multiples of the stroke width) the raster backend uses.
fn dash_multiples(dash: DashStyle) -> Option<&'static [f32]> {
    Some(match dash {
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
    })
}

/// `transform="…"` for a rotation/flip about the object's centre, the raster
/// backend's `T(c) · R(rot) · Flip · T(-c)`.
fn object_transform(transform: Option<&ShapeTransform>) -> String {
    let Some(t) = transform else {
        return String::new();
    };
    if t.rotation == 0 && !t.flip_h && !t.flip_v {
        return String::new();
    }
    let (cx, cy) = (tw(t.center.x), tw(t.center.y));
    let angle = t.rotation as f32 / 60_000.0;
    let sx = if t.flip_h { "-1" } else { "1" };
    let sy = if t.flip_v { "-1" } else { "1" };
    format!(
        " transform=\"translate({cx} {cy}) rotate({}) scale({sx} {sy}) translate({} {})\"",
        num(angle),
        num(-(t.center.x.raw() as f32)),
        num(-(t.center.y.raw() as f32))
    )
}

fn rect_attrs(rect: Rect) -> String {
    format!(
        "x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"",
        tw(rect.origin.x),
        tw(rect.origin.y),
        tw(rect.size.width),
        tw(rect.size.height)
    )
}

fn point(p: Point) -> String {
    format!("{} {}", tw(p.x), tw(p.y))
}

/// A geometry as one SVG element, with `attrs` appended.
fn geometry_element(geometry: &ShapeGeometry, attrs: &str) -> String {
    match geometry {
        ShapeGeometry::Rect { rect } => format!("<rect {}{attrs}/>", rect_attrs(*rect)),
        ShapeGeometry::Ellipse { rect } => ellipse(*rect, attrs),
        ShapeGeometry::RoundedRect { rect, radius } => format!(
            "<rect {} rx=\"{}\"{attrs}/>",
            rect_attrs(*rect),
            tw(*radius)
        ),
        ShapeGeometry::Path {
            commands,
            closed,
            fill_rule,
        } => {
            let mut d = path_data(commands);
            if *closed {
                d.push_str(" Z");
            }
            let rule = if matches!(fill_rule, ModelFillRule::EvenOdd) {
                " fill-rule=\"evenodd\""
            } else {
                ""
            };
            format!("<path d=\"{d}\"{rule}{attrs}/>")
        }
        ShapeGeometry::Line { from, to } => format!(
            "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"{attrs}/>",
            tw(from.x),
            tw(from.y),
            tw(to.x),
            tw(to.y)
        ),
    }
}

fn ellipse(rect: Rect, attrs: &str) -> String {
    let rx = rect.size.width.raw() as f32 / 2.0;
    let ry = rect.size.height.raw() as f32 / 2.0;
    format!(
        "<ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\"{attrs}/>",
        num(rect.origin.x.raw() as f32 + rx),
        num(rect.origin.y.raw() as f32 + ry),
        num(rx),
        num(ry)
    )
}

fn path_data(commands: &[PathCommand]) -> String {
    let mut d = String::new();
    for command in commands {
        if !d.is_empty() {
            d.push(' ');
        }
        match command {
            PathCommand::MoveTo { point: p } => {
                let _ = write!(d, "M{}", point(*p));
            }
            PathCommand::LineTo { point: p } => {
                let _ = write!(d, "L{}", point(*p));
            }
            PathCommand::CubicTo {
                control1,
                control2,
                point: p,
            } => {
                let _ = write!(
                    d,
                    "C{} {} {}",
                    point(*control1),
                    point(*control2),
                    point(*p)
                );
            }
            PathCommand::QuadTo { control, point: p } => {
                let _ = write!(d, "Q{} {}", point(*control), point(*p));
            }
            PathCommand::Close => d.push('Z'),
        }
    }
    d
}

/// An SVG path pen over skrifa outlines, in twips relative to the glyph origin
/// with y flipped (font y grows up, page y grows down).
struct SvgPen {
    d: String,
}

impl SvgPen {
    fn at(&mut self, op: char, points: &[(f32, f32)]) {
        if !self.d.is_empty() {
            self.d.push(' ');
        }
        self.d.push(op);
        for (i, (x, y)) in points.iter().enumerate() {
            if i > 0 {
                self.d.push(' ');
            }
            let _ = write!(self.d, "{} {}", num(*x), num(-*y));
        }
    }
}

impl OutlinePen for SvgPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.at('M', &[(x, y)]);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.at('L', &[(x, y)]);
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.at('Q', &[(cx0, cy0), (x, y)]);
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.at('C', &[(cx0, cy0), (cx1, cy1), (x, y)]);
    }
    fn close(&mut self) {
        self.d.push_str(" Z");
    }
}

/// The text behind each run, from cluster offsets into its paragraph.
struct RunText {
    /// Every cluster offset the page's glyphs carry, per paragraph: a run's
    /// text ends at the next boundary after its last cluster.
    boundaries: HashMap<NodeId, BTreeSet<u32>>,
    texts: HashMap<NodeId, String>,
}

impl RunText {
    fn build(list: &DisplayList, source: &dyn SvgTextSource) -> Self {
        let mut boundaries: HashMap<NodeId, BTreeSet<u32>> = HashMap::new();
        for item in &list.items {
            if let PaintItem::Glyphs { run } = item
                && let Some(node) = run.node
                && !run.is_marker
            {
                let set = boundaries.entry(node).or_default();
                set.extend(run.glyphs.iter().map(|g| g.cluster));
            }
        }
        let texts = boundaries
            .keys()
            .filter_map(|node| source.node_text(*node).map(|text| (*node, text)))
            .collect();
        Self { boundaries, texts }
    }

    fn of(&self, run: &GlyphRun) -> Option<String> {
        if run.is_marker || run.is_leader {
            return None;
        }
        let node = run.node?;
        let text = self.texts.get(&node)?;
        let first = run.glyphs.iter().map(|g| g.cluster).min()? as usize;
        let last = run.glyphs.iter().map(|g| g.cluster).max()?;
        let end = self
            .boundaries
            .get(&node)
            .and_then(|set| set.range(last + 1..).next().copied())
            .map_or(text.len(), |b| b as usize)
            .min(text.len());
        let slice = text.get(first..end)?;
        (!slice.trim().is_empty()).then(|| slice.to_owned())
    }
}

/// Writes one page.
struct Writer<'a> {
    fonts: &'a dyn GlyphSource,
    media: &'a dyn MediaSource,
    defs: String,
    body: String,
    /// `(font, glyph, size)` → its `<defs>` id, or `None` when the glyph has no
    /// outline.
    glyphs: BTreeMap<(u32, u32, i32), Option<String>>,
    /// Media key → its `<defs>` image id and pixel size.
    images: BTreeMap<String, Option<(String, u32, u32)>>,
    next_id: usize,
    /// Open groups, so an unbalanced list still closes every one.
    open: usize,
}

impl Writer<'_> {
    fn id(&mut self, prefix: &str) -> String {
        self.next_id += 1;
        format!("{prefix}{}", self.next_id)
    }

    fn glyph_def(
        &mut self,
        font: &FontRef<'_>,
        font_id: u32,
        glyph: u32,
        size: Twip,
    ) -> Option<String> {
        let key = (font_id, glyph, size.raw());
        if let Some(found) = self.glyphs.get(&key) {
            return found.clone();
        }
        let outlines = font.outline_glyphs();
        let mut pen = SvgPen { d: String::new() };
        let drawn = outlines.get(GlyphId::new(glyph)).is_some_and(|outline| {
            let settings =
                DrawSettings::unhinted(Size::new(size.raw() as f32), LocationRef::default());
            outline.draw(settings, &mut pen).is_ok()
        });
        let id = (drawn && !pen.d.is_empty()).then(|| {
            let id = format!("g{}-{}-{}", font_id, glyph, size.raw());
            let _ = writeln!(self.defs, "<path id=\"{id}\" d=\"{}\"/>", pen.d);
            id
        });
        self.glyphs.insert(key, id.clone());
        id
    }

    fn glyphs(&mut self, run: &GlyphRun, text: Option<String>) {
        let Some(bytes) = self.fonts.font_data(run.font) else {
            return;
        };
        let Ok(font) = FontRef::from_index(bytes, self.fonts.face_index(run.font)) else {
            return;
        };
        let scale_x = f32::from(run.character_scale_percent) / 100.0;
        let fill = rgba(run.color);
        let start = run.origin.x.raw() as f32;
        let baseline = run.origin.y.raw() as f32;
        let mut pen = start;
        let mut uses = String::new();
        for glyph in &run.glyphs {
            if let Some(id) = self.glyph_def(&font, run.font.0, glyph.id, run.size) {
                let scale = if (scale_x - 1.0).abs() > f32::EPSILON {
                    format!(" scale({scale_x:.3} 1)")
                } else {
                    String::new()
                };
                let _ = write!(
                    uses,
                    "<use xlink:href=\"#{id}\" transform=\"translate({} {}){scale}\"/>",
                    num(pen),
                    num(baseline)
                );
            }
            pen += glyph.advance.raw() as f32;
        }
        let advance = pen - start;
        if !uses.is_empty() {
            let _ = writeln!(self.body, "<g{}>{uses}</g>", fill_attr(Some(fill)));
        }
        // Decorations from the face's own metrics, as the raster backend draws them.
        let size = run.size.raw() as f32;
        if advance > 0.0
            && (run.decoration.underline
                || run.decoration.strikethrough
                || run.decoration.double_strike)
        {
            let metrics = Metrics::new(&font, Size::new(size), LocationRef::default());
            if run.decoration.underline {
                let (offset, thickness) = metrics
                    .underline
                    .map(|d| (d.offset, d.thickness))
                    .unwrap_or((-size * 0.12, size * 0.06));
                let line_color = rgba(run.decoration.underline_color.unwrap_or(run.color));
                self.decoration(start, baseline - offset, advance, thickness, line_color);
                if matches!(
                    run.decoration.underline_style,
                    casual_doc_model::v1::UnderlineStyle::Double
                ) {
                    self.decoration(
                        start,
                        baseline - offset + thickness * 2.0,
                        advance,
                        thickness,
                        line_color,
                    );
                }
            }
            if run.decoration.strikethrough || run.decoration.double_strike {
                let (offset, thickness) = metrics
                    .strikeout
                    .map(|d| (d.offset, d.thickness))
                    .unwrap_or((size * 0.26, size * 0.06));
                let offsets: &[f32] = if run.decoration.double_strike {
                    &[offset + thickness, offset - thickness]
                } else {
                    &[offset]
                };
                for line in offsets {
                    self.decoration(start, baseline - line, advance, thickness, fill);
                }
            }
        }
        if let Some(text) = text
            && advance > 0.0
        {
            let _ = writeln!(
                self.body,
                "<text x=\"{}\" y=\"{}\" font-size=\"{}\" textLength=\"{}\" lengthAdjust=\"spacingAndGlyphs\" fill-opacity=\"0\" xml:space=\"preserve\">{}</text>",
                num(start),
                num(baseline),
                num(size),
                num(advance),
                escape(&text)
            );
        }
    }

    fn decoration(&mut self, x: f32, top: f32, width: f32, thickness: f32, c: Color) {
        let _ = writeln!(
            self.body,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{}/>",
            num(x),
            num(top),
            num(width),
            num(thickness.max(1.0)),
            fill_attr(Some(c))
        );
    }

    fn fill_for(&mut self, fill: Option<&Fill>) -> String {
        match fill {
            None => " fill=\"none\"".to_owned(),
            Some(Fill::Solid(c)) => fill_attr(Some(*c)),
            Some(Fill::Gradient(gradient)) => {
                let id = self.id("grad");
                let stops: String = gradient
                    .stops
                    .iter()
                    .map(|s| {
                        format!(
                            "<stop offset=\"{}\" stop-color=\"{}\"{}/>",
                            num(s.position),
                            color(s.color),
                            if s.color.a == 255 {
                                String::new()
                            } else {
                                format!(" stop-opacity=\"{:.3}\"", f32::from(s.color.a) / 255.0)
                            }
                        )
                    })
                    .collect();
                match gradient.kind {
                    GradientKind::Linear { angle_deg } => {
                        let (dy, dx) = angle_deg.to_radians().sin_cos();
                        let _ = writeln!(
                            self.defs,
                            "<linearGradient id=\"{id}\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\">{stops}</linearGradient>",
                            num(0.5 - dx / 2.0),
                            num(0.5 - dy / 2.0),
                            num(0.5 + dx / 2.0),
                            num(0.5 + dy / 2.0)
                        );
                    }
                    GradientKind::Radial => {
                        let _ = writeln!(
                            self.defs,
                            "<radialGradient id=\"{id}\">{stops}</radialGradient>"
                        );
                    }
                }
                format!(" fill=\"url(#{id})\"")
            }
        }
    }

    fn outline(stroke: Option<&ShapeOutline>) -> String {
        stroke.map_or_else(String::new, |s| {
            stroke_attr(s.color, s.width, dash_multiples(s.dash))
        })
    }

    /// The image `media` as a `<defs>` entry: its id and pixel size, or `None`
    /// when the bytes are missing (nothing is painted) or undecodable
    /// (`Some` with no id: a placeholder).
    fn image_def(&mut self, media: &str) -> Option<Option<(String, u32, u32)>> {
        if let Some(found) = self.images.get(media) {
            return Some(found.clone());
        }
        let bytes = self.media.media_bytes(media)?;
        let reader = image::ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .ok()?;
        let format = reader.format();
        let dimensions = image::ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .ok()
            .and_then(|r| r.into_dimensions().ok());
        let embedded = match (format, dimensions) {
            (Some(f), Some((w, h))) => {
                let direct = match f {
                    image::ImageFormat::Png => Some("image/png"),
                    image::ImageFormat::Jpeg => Some("image/jpeg"),
                    image::ImageFormat::Gif => Some("image/gif"),
                    image::ImageFormat::WebP => Some("image/webp"),
                    _ => None,
                };
                let payload = match direct {
                    Some(mime) => Some((mime, bytes.to_vec())),
                    None => decode_bounded(bytes).and_then(|decoded| {
                        let mut out = Vec::new();
                        decoded
                            .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
                            .ok()
                            .map(|()| ("image/png", out))
                    }),
                };
                payload.map(|(mime, data)| {
                    let id = self.id("img");
                    let _ = writeln!(
                        self.defs,
                        "<image id=\"{id}\" width=\"{w}\" height=\"{h}\" preserveAspectRatio=\"none\" xlink:href=\"data:{mime};base64,{}\"/>",
                        base64(&data)
                    );
                    (id, w, h)
                })
            }
            _ => None,
        };
        self.images.insert(media.to_owned(), embedded.clone());
        Some(embedded)
    }

    fn image(
        &mut self,
        media: &str,
        rect: Rect,
        crop: Option<&CropRect>,
        transform: Option<&ShapeTransform>,
        opacity: Option<u32>,
    ) {
        let Some(def) = self.image_def(media) else {
            return;
        };
        let opacity = opacity
            .filter(|o| *o < OPACITY_FULL)
            .map_or_else(String::new, |o| {
                format!(" opacity=\"{:.3}\"", o as f32 / OPACITY_FULL as f32)
            });
        let Some((id, w, h)) = def else {
            // Bytes present, format not decodable: the raster backend's crossed box.
            let r = rect_attrs(rect);
            let (x0, y0) = (rect.origin.x.raw(), rect.origin.y.raw());
            let (x1, y1) = (x0 + rect.size.width.raw(), y0 + rect.size.height.raw());
            let _ = writeln!(
                self.body,
                "<g{}><rect {r} fill=\"#f0f0f0\" stroke=\"#909090\" stroke-width=\"15\"/><path d=\"M{x0} {y0} L{x1} {y1} M{x1} {y0} L{x0} {y1}\" stroke=\"#909090\" stroke-width=\"15\"/></g>",
                object_transform(transform)
            );
            return;
        };
        let full = CROP_FULL as f32;
        let (l, t, r, b) = crop.map_or((0.0, 0.0, 0.0, 0.0), |c| {
            (
                c.left as f32 / full,
                c.top as f32 / full,
                c.right as f32 / full,
                c.bottom as f32 / full,
            )
        });
        let (w, h) = (w as f32, h as f32);
        let view = format!(
            "{} {} {} {}",
            num(l * w),
            num(t * h),
            num((1.0 - l - r).max(0.001) * w),
            num((1.0 - t - b).max(0.001) * h)
        );
        let _ = writeln!(
            self.body,
            "<svg {} viewBox=\"{view}\" preserveAspectRatio=\"none\"{opacity}{}><use xlink:href=\"#{id}\"/></svg>",
            rect_attrs(rect),
            object_transform(transform)
        );
    }

    fn open_group(&mut self, attrs: &str) {
        let _ = writeln!(self.body, "<g{attrs}>");
        self.open += 1;
    }

    fn close_group(&mut self) {
        if self.open > 0 {
            self.body.push_str("</g>\n");
            self.open -= 1;
        }
    }
}

/// Decodes an image the browsers cannot show (BMP, TIFF) for re-encoding, under
/// the raster backend's dimension and allocation ceilings.
fn decode_bounded(bytes: &[u8]) -> Option<image::DynamicImage> {
    let format = image::guess_format(bytes).ok()?;
    let (width, height) = image::ImageReader::with_format(Cursor::new(bytes), format)
        .into_dimensions()
        .ok()?;
    if !crate::image_dimensions_within_limits(width, height) {
        return None;
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(crate::MAX_DECODED_IMAGE_DIMENSION);
    limits.max_image_height = Some(crate::MAX_DECODED_IMAGE_DIMENSION);
    limits.max_alloc = Some(crate::MAX_DECODED_IMAGE_BYTES);
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits);
    reader.decode().ok()
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[(n >> (18 - 6 * i)) as usize & 63]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// One page as an SVG document, `width` × `height` twips, painted on
/// `background` (the document's page colour; white when it has none), as the
/// raster backend's page is.
#[must_use]
pub fn render_svg(
    list: &DisplayList,
    width: Twip,
    height: Twip,
    background: [u8; 3],
    fonts: &dyn GlyphSource,
    media: &dyn MediaSource,
    text: &dyn SvgTextSource,
) -> String {
    let runs = RunText::build(list, text);
    let mut writer = Writer {
        fonts,
        media,
        defs: String::new(),
        body: String::new(),
        glyphs: BTreeMap::new(),
        images: BTreeMap::new(),
        next_id: 0,
        open: 0,
    };
    for item in &list.items {
        match item {
            PaintItem::Glyphs { run } => {
                let content = runs.of(run);
                writer.glyphs(run, content);
            }
            PaintItem::Rect { rect, fill, stroke } => {
                let _ = writeln!(
                    writer.body,
                    "<rect {}{}{}/>",
                    rect_attrs(*rect),
                    fill_attr(*fill),
                    plain_stroke(stroke.as_ref())
                );
            }
            PaintItem::Ellipse { rect, fill, stroke } => {
                let attrs = format!("{}{}", fill_attr(*fill), plain_stroke(stroke.as_ref()));
                let _ = writeln!(writer.body, "{}", ellipse(*rect, &attrs));
            }
            PaintItem::RoundedRect {
                rect,
                radius,
                fill,
                stroke,
            } => {
                let _ = writeln!(
                    writer.body,
                    "<rect {} rx=\"{}\"{}{}/>",
                    rect_attrs(*rect),
                    tw(*radius),
                    fill_attr(*fill),
                    plain_stroke(stroke.as_ref())
                );
            }
            PaintItem::Polygon {
                points,
                fill,
                stroke,
            } => {
                let list: Vec<String> = points.iter().map(|p| point(*p)).collect();
                let _ = writeln!(
                    writer.body,
                    "<polygon points=\"{}\"{}{}/>",
                    list.join(" "),
                    fill_attr(*fill),
                    plain_stroke(stroke.as_ref())
                );
            }
            PaintItem::Line { from, to, stroke } => {
                let _ = writeln!(
                    writer.body,
                    "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"{}/>",
                    tw(from.x),
                    tw(from.y),
                    tw(to.x),
                    tw(to.y),
                    stroke_attr(stroke.color, stroke.width, None)
                );
            }
            PaintItem::Shape {
                geometry,
                fill,
                stroke,
                transform,
                ..
            } => {
                let paint = writer.fill_for(fill.as_ref());
                let attrs = format!(
                    "{paint}{}{}",
                    Writer::outline(stroke.as_ref()),
                    object_transform(transform.as_ref())
                );
                let _ = writeln!(writer.body, "{}", geometry_element(geometry, &attrs));
            }
            PaintItem::Shadow {
                geometry,
                filled,
                stroke_width,
                color: shadow,
                offset,
                blur,
                transform,
            } => {
                let filter = if blur.raw() > 0 {
                    let id = writer.id("blur");
                    let _ = writeln!(
                        writer.defs,
                        "<filter id=\"{id}\" x=\"-50%\" y=\"-50%\" width=\"200%\" height=\"200%\"><feGaussianBlur stdDeviation=\"{}\"/></filter>",
                        num(blur.raw() as f32 / 2.0)
                    );
                    format!(" filter=\"url(#{id})\"")
                } else {
                    String::new()
                };
                let paint = if *filled {
                    fill_attr(Some(*shadow))
                } else {
                    " fill=\"none\"".to_owned()
                };
                let outline =
                    stroke_width.map_or_else(String::new, |w| stroke_attr(*shadow, w, None));
                let inner = geometry_element(
                    geometry,
                    &format!("{paint}{outline}{}", object_transform(transform.as_ref())),
                );
                let _ = writeln!(
                    writer.body,
                    "<g transform=\"translate({} {})\"{filter}>{inner}</g>",
                    tw(offset.x),
                    tw(offset.y)
                );
            }
            PaintItem::Image {
                media: key,
                rect,
                crop,
                transform,
                opacity,
            } => writer.image(key, *rect, crop.as_ref(), transform.as_ref(), *opacity),
            PaintItem::PushClip(rect) => {
                let id = writer.id("clip");
                let _ = writeln!(
                    writer.defs,
                    "<clipPath id=\"{id}\"><rect {}/></clipPath>",
                    rect_attrs(*rect)
                );
                writer.open_group(&format!(" clip-path=\"url(#{id})\""));
            }
            PaintItem::PushLayer { transform, blend } => {
                let mut attrs = object_transform(transform.as_ref());
                if matches!(blend, LayerBlend::Multiply) {
                    attrs.push_str(" style=\"mix-blend-mode:multiply\"");
                }
                writer.open_group(&attrs);
            }
            PaintItem::PopClip | PaintItem::PopLayer => writer.close_group(),
        }
    }
    while writer.open > 0 {
        writer.close_group();
    }
    let (w, h) = (width.raw(), height.raw());
    let mut out = String::with_capacity(writer.defs.len() + writer.body.len() + 512);
    let _ = writeln!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{}pt\" height=\"{}pt\" viewBox=\"0 0 {w} {h}\">",
        num(w as f32 / 20.0),
        num(h as f32 / 20.0)
    );
    if !writer.defs.is_empty() {
        let _ = write!(out, "<defs>\n{}</defs>\n", writer.defs);
    }
    let _ = writeln!(
        out,
        "<rect width=\"{w}\" height=\"{h}\"{}/>",
        fill_attr(Some(Color::rgb(
            background[0],
            background[1],
            background[2]
        )))
    );
    out.push_str(&writer.body);
    out.push_str("</svg>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_deterministic_and_short() {
        assert_eq!(num(12.0), "12");
        assert_eq!(num(-0.04), "0");
        assert_eq!(num(1.25), "1.3");
        assert_eq!(num(-3.5), "-3.5");
    }

    #[test]
    fn base64_matches_the_standard_alphabet() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn an_unbalanced_list_still_closes_every_group() {
        let mut list = DisplayList::new();
        list.push(PaintItem::PushClip(Rect::default()));
        list.push(PaintItem::PushLayer {
            transform: None,
            blend: LayerBlend::Multiply,
        });
        list.push(PaintItem::PopLayer);
        list.push(PaintItem::PopClip);
        list.push(PaintItem::PopClip);
        let svg = render_svg(
            &list,
            Twip(100),
            Twip(100),
            [255, 255, 255],
            &crate::BundledFontSource,
            &crate::NoMediaSource,
            &NoSvgText,
        );
        assert_eq!(svg.matches("<g").count(), svg.matches("</g>").count());
    }
}
