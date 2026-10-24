//! The fallback: one paint item rasterized by the reference renderer.
//!
//! The GPU encoder draws the families that dominate a page's cost as vectors.
//! For the rest — a colour glyph run, a shape with arrowheads, a picture — it
//! asks `casual-doc-render` to paint that ONE item into a small transparent tile
//! and composites the tile. That keeps a single implementation of each rare
//! family (the reference's), and it makes those items pixel-identical to the
//! CPU page instead of merely close (named pattern: a software fallback, as GPU
//! 2D renderers keep for paths their shaders do not cover).
//!
//! The item is MOVED, not the renderer: the display list is in whole twips, so
//! the tile origin is chosen on a twip that is also a whole device pixel
//! whenever the scale allows it (every multiple of 96 dpi, and 144 dpi). The
//! tile then lands texel for texel. At a scale where no such twip exists (110 %
//! zoom is 105.6 dpi) the tile is placed at its fractional position and
//! resampled.
//!
//! **Complexity.** O(tile pixels) per fallback item; the tile is bounded by the
//! item's own device box clipped to the page.

use casual_doc_layout::display::{DisplayList, PaintItem, PathCommand, ShapeGeometry};
use casual_doc_layout::units::{Point, Rect, Twip};
use casual_doc_render::{GlyphSource, MediaSource, Surface, render};
use vello_common::kurbo::{Point as KPoint, Rect as KRect};
use vello_common::peniko::color::PremulRgba8;
use vello_common::pixmap::Pixmap;

use crate::encode::object_transform;

/// Twips per inch: the display list's unit.
const TWIPS_PER_INCH: f64 = 1440.0;

/// A rasterized item and where its top-left pixel goes, in page device pixels.
#[derive(Debug)]
pub(crate) struct Tile {
    pub(crate) pixmap: Pixmap,
    pub(crate) x: f64,
    pub(crate) y: f64,
    /// The tile sits on whole pixels, so it is copied rather than resampled.
    pub(crate) nearest: bool,
}

/// Paints `item` alone through the reference renderer, into a tile covering its
/// device box clipped to the `width × height` page. `None` when the item has no
/// visible box on the page.
pub(crate) fn rasterize(
    item: &PaintItem,
    width: u32,
    height: u32,
    dpi: f32,
    fonts: &dyn GlyphSource,
    media: &dyn MediaSource,
) -> Option<Tile> {
    let page = KRect::new(0.0, 0.0, f64::from(width), f64::from(height));
    let bounds = device_bounds(item, dpi)?.intersect(page);
    if bounds.width() <= 0.0 || bounds.height() <= 0.0 {
        return None;
    }
    let dpi64 = f64::from(dpi);
    let step = whole_pixel_twips(dpi);
    let origin_twip = |px: f64| -> i32 {
        let twip = (px * TWIPS_PER_INCH / dpi64).floor() as i64;
        let snapped = twip.div_euclid(i64::from(step)) * i64::from(step);
        snapped.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
    };
    let (ox, oy) = (origin_twip(bounds.x0), origin_twip(bounds.y0));
    let x = f64::from(ox) * dpi64 / TWIPS_PER_INCH;
    let y = f64::from(oy) * dpi64 / TWIPS_PER_INCH;
    let w = (bounds.x1 - x).ceil().max(1.0);
    let h = (bounds.y1 - y).ceil().max(1.0);
    if w > f64::from(u16::MAX) || h > f64::from(u16::MAX) {
        return None;
    }
    let (w, h) = (w as u32, h as u32);

    let mut moved = item.clone();
    translate_item(&mut moved, Twip(-ox), Twip(-oy));
    let mut list = DisplayList::new();
    list.push(moved);
    let mut surface = Surface::transparent(w, h).ok()?;
    render(&list, &mut surface, dpi, fonts, media);

    let pixels: Vec<PremulRgba8> = surface
        .data()
        .chunks_exact(4)
        .map(|p| PremulRgba8 {
            r: p[0],
            g: p[1],
            b: p[2],
            a: p[3],
        })
        .collect();
    let pixmap = Pixmap::from_parts(pixels, w as u16, h as u16);
    let whole = |v: f64| (v - v.round()).abs() < 1e-6;
    Some(Tile {
        pixmap,
        x,
        y,
        nearest: whole(x) && whole(y),
    })
}

/// The smallest whole number of twips that is also a whole number of device
/// pixels at `dpi`, or 1 when the scale has none worth using (a fractional dpi).
fn whole_pixel_twips(dpi: f32) -> i32 {
    if dpi.fract() != 0.0 || dpi <= 0.0 {
        return 1;
    }
    let dpi = dpi as i64;
    let gcd = gcd(1440, dpi);
    let step = 1440 / gcd;
    // Twips per pixel at `dpi` is 1440 / dpi; a step is a pixel multiple when
    // step * dpi / 1440 is whole, which 1440 / gcd(1440, dpi) is by construction.
    i32::try_from(step).unwrap_or(1)
}

fn gcd(mut a: i64, mut b: i64) -> i64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.abs().max(1)
}

/// A conservative device box for an item the encoder sends to the fallback.
fn device_bounds(item: &PaintItem, dpi: f32) -> Option<KRect> {
    let px = |t: Twip| f64::from(t.to_device_px(dpi));
    let rect_box = |r: &Rect| {
        KRect::new(
            px(r.origin.x),
            px(r.origin.y),
            px(r.origin.x) + px(r.size.width),
            px(r.origin.y) + px(r.size.height),
        )
    };
    let points_box = |points: &mut dyn Iterator<Item = Point>| -> Option<KRect> {
        let mut out: Option<KRect> = None;
        for p in points {
            let q = KPoint::new(px(p.x), px(p.y));
            out = Some(match out {
                Some(r) => r.union_pt(q),
                None => KRect::from_points(q, q),
            });
        }
        out
    };
    match item {
        PaintItem::Glyphs { run } => {
            let size = f64::from(run.size.to_device_px(dpi));
            let advance: f64 = run
                .glyphs
                .iter()
                .map(|g| f64::from(g.advance.to_device_px(dpi)))
                .sum();
            let x = px(run.origin.x);
            let y = px(run.origin.y);
            // A colour glyph's art can overhang its advance; a size of margin on
            // every side covers it and the effect offsets.
            Some(KRect::new(
                x - size - 2.0,
                y - 2.0 * size - 2.0,
                x + advance + size + 2.0,
                y + size + 2.0,
            ))
        }
        PaintItem::Shape {
            geometry,
            stroke,
            transform,
            ..
        } => {
            let local = match geometry {
                ShapeGeometry::Rect { rect }
                | ShapeGeometry::Ellipse { rect }
                | ShapeGeometry::RoundedRect { rect, .. } => rect_box(rect),
                ShapeGeometry::Path { commands, .. } => {
                    points_box(&mut commands.iter().flat_map(|command| command.points()))?
                }
                ShapeGeometry::Line { from, to } => points_box(&mut [*from, *to].into_iter())?,
            };
            // The longest arrowhead (a diamond) reaches 9 line widths past its
            // end point; the box grows by more than that.
            let pad = f64::from(stroke.as_ref().map_or(1.0, |s| s.width.max(1.0))) * 10.0 + 2.0;
            let grown = local.inflate(pad, pad);
            Some(transformed_box(
                grown,
                object_transform(transform.as_ref(), dpi),
            ))
        }
        PaintItem::Image {
            rect, transform, ..
        } => {
            let grown = rect_box(rect).inflate(2.0, 2.0);
            Some(transformed_box(
                grown,
                object_transform(transform.as_ref(), dpi),
            ))
        }
        PaintItem::Rect { rect, .. }
        | PaintItem::Ellipse { rect, .. }
        | PaintItem::RoundedRect { rect, .. } => Some(rect_box(rect).inflate(4.0, 4.0)),
        PaintItem::Polygon { points, .. } => {
            Some(points_box(&mut points.iter().copied())?.inflate(4.0, 4.0))
        }
        PaintItem::Line { from, to, stroke } => {
            let pad = f64::from(stroke.width) + 2.0;
            Some(points_box(&mut [*from, *to].into_iter())?.inflate(pad, pad))
        }
        PaintItem::PushClip(_)
        | PaintItem::PopClip
        | PaintItem::PushLayer { .. }
        | PaintItem::PopLayer => None,
    }
}

fn transformed_box(rect: KRect, transform: vello_common::kurbo::Affine) -> KRect {
    transform.transform_rect_bbox(rect)
}

fn shift_point(point: &mut Point, dx: Twip, dy: Twip) {
    point.x = Twip(point.x.0.saturating_add(dx.0));
    point.y = Twip(point.y.0.saturating_add(dy.0));
}

fn shift_rect(rect: &mut Rect, dx: Twip, dy: Twip) {
    shift_point(&mut rect.origin, dx, dy);
}

/// Moves every coordinate `item` carries by `(dx, dy)` twips. Sizes, radii and
/// advances are lengths and stay put.
pub(crate) fn translate_item(item: &mut PaintItem, dx: Twip, dy: Twip) {
    match item {
        PaintItem::Glyphs { run } => shift_point(&mut run.origin, dx, dy),
        PaintItem::Rect { rect, .. }
        | PaintItem::Ellipse { rect, .. }
        | PaintItem::RoundedRect { rect, .. }
        | PaintItem::PushClip(rect) => shift_rect(rect, dx, dy),
        PaintItem::Polygon { points, .. } => {
            for point in points {
                shift_point(point, dx, dy);
            }
        }
        PaintItem::Image {
            rect, transform, ..
        } => {
            shift_rect(rect, dx, dy);
            if let Some(transform) = transform {
                shift_point(&mut transform.center, dx, dy);
            }
        }
        PaintItem::Line { from, to, .. } => {
            shift_point(from, dx, dy);
            shift_point(to, dx, dy);
        }
        PaintItem::Shape {
            geometry,
            transform,
            ..
        } => {
            match geometry {
                ShapeGeometry::Rect { rect }
                | ShapeGeometry::Ellipse { rect }
                | ShapeGeometry::RoundedRect { rect, .. } => shift_rect(rect, dx, dy),
                ShapeGeometry::Path { commands, .. } => {
                    for command in commands {
                        match command {
                            PathCommand::MoveTo { point } | PathCommand::LineTo { point } => {
                                shift_point(point, dx, dy);
                            }
                            PathCommand::CubicTo {
                                control1,
                                control2,
                                point,
                            } => {
                                shift_point(control1, dx, dy);
                                shift_point(control2, dx, dy);
                                shift_point(point, dx, dy);
                            }
                            PathCommand::QuadTo { control, point } => {
                                shift_point(control, dx, dy);
                                shift_point(point, dx, dy);
                            }
                            PathCommand::Close => {}
                        }
                    }
                }
                ShapeGeometry::Line { from, to } => {
                    shift_point(from, dx, dy);
                    shift_point(to, dx, dy);
                }
            }
            if let Some(transform) = transform {
                shift_point(&mut transform.center, dx, dy);
            }
        }
        PaintItem::PopClip | PaintItem::PushLayer { .. } | PaintItem::PopLayer => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_whole_pixel_step_is_whole_pixels_at_every_common_scale() {
        for dpi in [96.0_f32, 120.0, 144.0, 192.0, 288.0, 384.0] {
            let step = whole_pixel_twips(dpi);
            let px = f64::from(step) * f64::from(dpi) / TWIPS_PER_INCH;
            assert!(
                (px - px.round()).abs() < 1e-9,
                "{dpi} dpi: {step} twips is {px} px"
            );
        }
        assert_eq!(whole_pixel_twips(105.6), 1, "a fractional scale has none");
    }
}
