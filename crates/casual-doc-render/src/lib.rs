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

use casual_doc_layout::display::{DisplayList, PaintItem};
use casual_doc_layout::text::{FontId, GlyphRun};
use casual_doc_layout::units::Rect;
use skrifa::instance::{LocationRef, Size};
use skrifa::metrics::Metrics;
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, GlyphId, MetadataProvider};
use tiny_skia::{
    Color, FillRule, Mask, Paint, PathBuilder, Pixmap, Rect as SkRect, Stroke, Transform,
};

/// Supplies the raw font bytes (and face index) for a [`FontId`] so the renderer
/// can extract glyph outlines from the exact face the shaper used.
pub trait GlyphSource {
    /// The font file bytes for `font`, or `None` if unknown (glyphs are skipped).
    fn font_data(&self, font: FontId) -> Option<&[u8]>;
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
        let mut pixmap = Pixmap::new(width, height).ok_or(RenderError::InvalidSize)?;
        pixmap.fill(Color::WHITE);
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
/// `fonts`; a glyph whose font is unknown is skipped.
pub fn render(list: &DisplayList, surface: &mut Surface, dpi: f32, fonts: &dyn GlyphSource) {
    // A stack of clip masks: each entry is the *effective* clip (the intersection
    // of every enclosing `PushClip` rectangle). While non-empty, its top is
    // passed to every paint call so content outside an `exact`-height row's clip
    // rect is not drawn.
    let mut clip_stack: Vec<Mask> = Vec::new();
    for item in &list.items {
        match item {
            PaintItem::Rect { rect, fill, stroke } => {
                let clip = clip_stack.last();
                if let Some(color) = fill {
                    let mut paint = Paint::default();
                    paint.set_color_rgba8(color.r, color.g, color.b, color.a);
                    paint.anti_alias = true;
                    if let Some(path) = rect_path(*rect, dpi) {
                        surface.pixmap.fill_path(
                            &path,
                            &paint,
                            FillRule::Winding,
                            Transform::identity(),
                            clip,
                        );
                    }
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
                    if let Some(path) = rect_path(*rect, dpi) {
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
            PaintItem::Image { .. } => {
                // Images land with a later slice.
            }
        }
    }
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
    let Ok(font) = FontRef::new(bytes) else {
        return;
    };
    let outlines = font.outline_glyphs();
    let size_px = run.size.to_device_px(dpi);
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
    if run.decoration.underline || run.decoration.strikethrough {
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
                draw_decoration(
                    surface,
                    clip,
                    start_x,
                    baseline_y - offset,
                    advance,
                    thickness,
                    run.color,
                );
            }
            if run.decoration.strikethrough {
                let (offset, thickness) = metrics
                    .strikeout
                    .map(|d| (d.offset, d.thickness))
                    .unwrap_or((size_px * 0.26, size_px * 0.06));
                draw_decoration(
                    surface,
                    clip,
                    start_x,
                    baseline_y - offset,
                    advance,
                    thickness,
                    run.color,
                );
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

/// A `skrifa` outline pen that appends glyph contours to a `tiny-skia` path,
/// translating font space (origin at the glyph, y-up) into device space
/// (`origin_x`, `baseline_y`, y-down).
struct GlyphPen<'a> {
    builder: &'a mut PathBuilder,
    origin_x: f32,
    baseline_y: f32,
}

impl GlyphPen<'_> {
    fn map(&self, x: f32, y: f32) -> (f32, f32) {
        (self.origin_x + x, self.baseline_y - y)
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
                text: "Hello, opendoc!",
                font: FontId(0),
                size: Twip::from_points(24),
                bold: false,
                italic: false,
                letter_spacing: Twip::ZERO,
                color: [0, 0, 0, 255],
                decoration: Decoration::default(),
                highlight: None,
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
        render(&list, &mut surface, 96.0, &fonts);

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
            color: [0, 0, 0, 255],
            origin: Point::new(Twip::ZERO, Twip::from_points(12)),
            bidi_level: 0,
            decoration: Decoration::default(),
            highlight: None,
            glyphs: vec![casual_doc_layout::text::Glyph {
                id: 5,
                advance: Twip::from_points(6),
                cluster: 0,
            }],
        };
        let mut list = DisplayList::new();
        list.push(PaintItem::Glyphs { run });
        let fonts = SingleFontSource::new(ROBOTO_REGULAR); // only serves FontId(0)
        render(&list, &mut surface, 96.0, &fonts); // FontId(9) unknown -> skipped
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
        render(&list, &mut surface, dpi, &fonts);

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
        render(&list, &mut surface, dpi, &fonts);

        let i = (60 * 100 + 60) * 4;
        let px = &surface.data()[i..i + 4];
        assert!(
            px[0] < 40 && px[1] < 40 && px[2] < 40,
            "with the clip popped, the whole rect is painted (got {:?})",
            &px[..4]
        );
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
                text: "Hello",
                font: FontId(0),
                size: Twip::from_points(24),
                bold: false,
                italic: false,
                letter_spacing: Twip::ZERO,
                color: [0, 0, 0, 255],
                decoration,
                highlight: None,
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
    fn a_plain_run_draws_no_decoration_line() {
        // A plain run has no ink below the baseline (no underline, no descenders).
        let (plain, baseline) = render_decorated(Decoration::default());
        assert_eq!(
            band_dark(&plain, baseline + 2, baseline + 12),
            0,
            "a plain run has no underline"
        );
    }
}
