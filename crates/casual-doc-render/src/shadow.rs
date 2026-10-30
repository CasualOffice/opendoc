//! The CPU raster of a [`PaintItem::Shadow`](casual_doc_layout::display::PaintItem::Shadow):
//! a shape's silhouette painted into an off-screen pixmap, blurred, and
//! composited back moved by the shadow's offset (`docs/153`
//! `object.shape-shadow`).
//!
//! The browser canvas path is this same renderer (the wasm build rasterizes
//! pages through `casual_doc_render::render`), so this one implementation is
//! what a reader sees in the editor and in a PNG export.
//!
//! ## The blur
//!
//! A Gaussian of standard deviation `sigma` approximated by THREE successive
//! box blurs, the standard construction (W3C Filter Effects §`feGaussianBlur`
//! recommends it, and every browser canvas `shadowBlur` uses it): three passes of
//! a box of width `d = floor(sigma * 3 * sqrt(2 * pi) / 4 + 0.5)` are within a
//! few percent of the true Gaussian. Each pass is a running sum, so a pass is
//! O(pixels) whatever the radius.
//!
//! DrawingML's `@blurRad` is a blur RADIUS; it is read as `sigma = blurRad / 2`,
//! which is the relationship the canvas `shadowBlur` attribute defines
//! (`shadowBlur = 2 * sigma`) and which matches the softness Word paints for
//! its presets closely enough that the 4 pt preset reads as a 4 pt blur.
//!
//! ## Bounds
//!
//! The off-screen pixmap is the silhouette's device bounds grown by three
//! sigmas (where a Gaussian has fallen to nothing) and clipped to the page
//! grown by the same margin, so a shadow costs O(its own area), never more
//! than a page's. A blur radius far beyond any real document is clamped to
//! [`MAX_SIGMA_PX`].

use casual_doc_layout::display::{Color as DisplayColor, ShapeGeometry, ShapeTransform};
use casual_doc_layout::units::{Point, Twip};
use tiny_skia::{FillRule, Mask, Paint, Path, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform};

use crate::Surface;

/// The largest blur standard deviation painted, in device pixels. A Gaussian
/// this wide is already a uniform haze; a larger one only costs memory.
const MAX_SIGMA_PX: f32 = 128.0;

/// Everything one shadow paints from, so the raster takes one argument.
pub(crate) struct ShadowPaint<'a> {
    pub(crate) geometry: &'a ShapeGeometry,
    pub(crate) filled: bool,
    pub(crate) stroke_width: Option<f32>,
    pub(crate) color: DisplayColor,
    pub(crate) offset: Point,
    pub(crate) blur: Twip,
    pub(crate) transform: Option<&'a ShapeTransform>,
}

/// Paints one shadow onto `surface` under `clip`.
///
/// O(w * h) in the shadow's own device box (three blur passes, each a running
/// sum per row and per column).
pub(crate) fn render_shadow(
    surface: &mut Surface,
    shadow: &ShadowPaint<'_>,
    dpi: f32,
    clip: Option<&Mask>,
) {
    if !shadow.filled && shadow.stroke_width.is_none() {
        return;
    }
    let Some(path) = geometry_path(shadow.geometry, dpi) else {
        return;
    };
    let transform = crate::object_transform(shadow.transform, dpi);
    let Some(path) = path.transform(transform) else {
        return;
    };
    let sigma = (shadow.blur.to_device_px(dpi) / 2.0).clamp(0.0, MAX_SIGMA_PX);
    let stroke = shadow.stroke_width.map_or(0.0, |width| width.max(1.0));
    let margin = (3.0 * sigma).ceil() + stroke + 2.0;
    let bounds = path.bounds();
    let page_w = surface.pixmap.width() as f32;
    let page_h = surface.pixmap.height() as f32;
    let dx = shadow.offset.x.to_device_px(dpi);
    let dy = shadow.offset.y.to_device_px(dpi);
    // The box the shadow lands in, in page pixels, before the offset is taken
    // back out: clipped to the page (grown by the blur margin, so the blur near
    // an edge still sees the silhouette beyond it).
    let left = (bounds.left() - margin).max(-dx - margin).floor();
    let top = (bounds.top() - margin).max(-dy - margin).floor();
    let right = (bounds.right() + margin).min(page_w - dx + margin).ceil();
    let bottom = (bounds.bottom() + margin).min(page_h - dy + margin).ceil();
    if right <= left || bottom <= top {
        return;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // positive, page-bounded
    let (width, height) = ((right - left) as u32, (bottom - top) as u32);
    let Some(mut layer) = Pixmap::new(width, height) else {
        return;
    };
    let mut paint = Paint::default();
    paint.set_color_rgba8(shadow.color.r, shadow.color.g, shadow.color.b, 255);
    paint.anti_alias = true;
    let local = Transform::from_translate(-left, -top);
    if shadow.filled {
        layer.fill_path(&path, &paint, FillRule::Winding, local, None);
    }
    if let Some(width) = shadow.stroke_width {
        let outline = Stroke {
            width: width.max(1.0),
            ..Stroke::default()
        };
        layer.stroke_path(&path, &paint, &outline, local, None);
    }
    if sigma >= 0.5 {
        blur(&mut layer, sigma);
    }
    let paint = PixmapPaint {
        opacity: f32::from(shadow.color.a) / 255.0,
        ..PixmapPaint::default()
    };
    #[allow(clippy::cast_possible_truncation)] // page-bounded device pixels
    surface.pixmap.draw_pixmap(
        (left + dx).round() as i32,
        (top + dy).round() as i32,
        layer.as_ref(),
        &paint,
        Transform::identity(),
        clip,
    );
}

/// The geometry's path in device pixels (untransformed). O(c) in its commands.
fn geometry_path(geometry: &ShapeGeometry, dpi: f32) -> Option<Path> {
    match geometry {
        ShapeGeometry::Rect { rect } => crate::rect_path(*rect, dpi),
        ShapeGeometry::Ellipse { rect } => crate::ellipse_path(*rect, dpi),
        ShapeGeometry::RoundedRect { rect, radius } => {
            crate::rounded_rect_path(*rect, *radius, dpi)
        }
        // A shadow is a silhouette, so an open path casts the figure it would
        // fill, exactly as a filled open `a:custGeom` paints.
        ShapeGeometry::Path {
            commands, closed, ..
        } => crate::command_path(commands, *closed, dpi),
        ShapeGeometry::Line { from, to } => {
            let mut builder = PathBuilder::new();
            builder.move_to(from.x.to_device_px(dpi), from.y.to_device_px(dpi));
            builder.line_to(to.x.to_device_px(dpi), to.y.to_device_px(dpi));
            builder.finish()
        }
    }
}

/// The box width for one of three passes approximating a Gaussian of `sigma`.
fn box_width(sigma: f32) -> usize {
    let d = (sigma * 3.0 * (2.0 * core::f32::consts::PI).sqrt() / 4.0 + 0.5).floor();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // sigma is clamped
    let d = d.max(1.0) as usize;
    d
}

/// Blurs the premultiplied pixmap in place: three box passes horizontally,
/// then three vertically. Blurring premultiplied channels is what keeps a
/// translucent edge from darkening. O(w * h).
fn blur(pixmap: &mut Pixmap, sigma: f32) {
    let width = pixmap.width() as usize;
    let height = pixmap.height() as usize;
    let d = box_width(sigma);
    // An odd box is centred; an even one is offset by half a pixel, alternated
    // so the three passes do not drift (the W3C construction).
    let passes: [(usize, usize); 3] = if d % 2 == 1 {
        [(d / 2, d / 2); 3]
    } else {
        [(d / 2, d / 2 - 1), (d / 2 - 1, d / 2), (d / 2, d / 2)]
    };
    let data = pixmap.data_mut();
    let mut line = vec![0_u8; width.max(height) * 4];
    for (before, after) in passes {
        for row in 0..height {
            let start = row * width * 4;
            let slice = &mut data[start..start + width * 4];
            box_pass(slice, 4, width, before, after, &mut line);
        }
    }
    let mut column = vec![0_u8; height * 4];
    for x in 0..width {
        for (y, texel) in column.chunks_exact_mut(4).enumerate() {
            let at = (y * width + x) * 4;
            texel.copy_from_slice(&data[at..at + 4]);
        }
        for (before, after) in passes {
            box_pass(&mut column, 4, height, before, after, &mut line);
        }
        for (y, texel) in column.chunks_exact(4).enumerate() {
            let at = (y * width + x) * 4;
            data[at..at + 4].copy_from_slice(texel);
        }
    }
}

/// One box pass over `count` RGBA texels of `values` (stride 4): each output
/// is the mean of the inputs from `before` texels behind it to `after` ahead,
/// with transparent texels beyond either end. A running sum per channel, so
/// O(count) for any box. `scratch` holds the input copy.
fn box_pass(
    values: &mut [u8],
    stride: usize,
    count: usize,
    before: usize,
    after: usize,
    scratch: &mut [u8],
) {
    let span = before + after + 1;
    let input = &mut scratch[..count * stride];
    input.copy_from_slice(&values[..count * stride]);
    for channel in 0..stride {
        let at = |index: usize| u32::from(input[index * stride + channel]);
        let mut sum: u32 = (0..after.min(count)).map(at).sum();
        for (index, out) in values.chunks_exact_mut(stride).take(count).enumerate() {
            if index + after < count {
                sum += at(index + after);
            }
            if index > before {
                sum -= at(index - before - 1);
            }
            #[allow(clippy::cast_possible_truncation)] // a mean of u8 values
            {
                out[channel] = ((sum + span as u32 / 2) / span as u32) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_box_pass_spreads_a_point_evenly_and_keeps_its_mass() {
        // One opaque texel in a row of nine, a box of three: three texels of a
        // third each, and nothing elsewhere.
        let mut row = vec![0_u8; 9 * 4];
        row[4 * 4..4 * 4 + 4].copy_from_slice(&[255, 255, 255, 255]);
        let mut scratch = vec![0_u8; row.len()];
        box_pass(&mut row, 4, 9, 1, 1, &mut scratch);
        let alpha: Vec<u8> = row.chunks_exact(4).map(|texel| texel[3]).collect();
        assert_eq!(alpha, vec![0, 0, 0, 85, 85, 85, 0, 0, 0]);
    }

    #[test]
    fn the_box_width_follows_the_three_pass_gaussian_construction() {
        assert_eq!(box_width(1.0), 2);
        assert_eq!(box_width(4.0), 8);
        assert_eq!(box_width(0.1), 1);
    }
}
