//! The sparse-strip CPU painter: `vello_cpu`, fed by the same encoder.
//!
//! Two jobs. It checks the encoder on a machine with no GPU — `vello_hybrid`
//! and `vello_cpu` build the same sparse strips from the same commands, so an
//! encoder mistake shows here exactly as it would on the GPU — and it is the
//! measured CPU alternative to `tiny-skia` in `docs/architecture/179` §4. It is not the
//! reference: `casual-doc-render` is.

use casual_doc_layout::display::DisplayList;
use casual_doc_render::{GlyphSource, MediaSource};
use std::sync::Arc;
use vello_common::kurbo::{Affine, BezPath, Rect as KRect, Stroke};
use vello_common::paint::{ImageSource, PaintType};
use vello_common::peniko::{BlendMode, Color};
use vello_common::pixmap::Pixmap;
use vello_cpu::{RenderContext, Resources};

use crate::encode::{EncodeStats, SparseCanvas, encode_display_list};

/// Renders `list` with `vello_cpu` into premultiplied RGBA (`Surface::data`'s
/// layout), on a page cleared to `background`. `None` when a side exceeds the
/// renderer's 16-bit coordinates.
#[must_use]
pub fn render_sparse_cpu(
    list: &DisplayList,
    width: u32,
    height: u32,
    dpi: f32,
    background: [u8; 3],
    fonts: &dyn GlyphSource,
    media: &dyn MediaSource,
) -> Option<(Vec<u8>, EncodeStats)> {
    let (w, h) = (u16::try_from(width).ok()?, u16::try_from(height).ok()?);
    let mut context = RenderContext::new(w, h);
    let mut resources = Resources::new();
    let [r, g, b] = background;
    context.set_paint(Color::from_rgba8(r, g, b, 255));
    context.fill_rect(&KRect::new(0.0, 0.0, f64::from(width), f64::from(height)));
    let stats = {
        let mut canvas = CpuCanvas {
            context: &mut context,
            resources: &mut resources,
        };
        encode_display_list(list, &mut canvas, width, height, dpi, fonts, media)
    };
    context.flush();
    let mut target = Pixmap::new(w, h);
    context.render(&mut target, &mut resources);
    Some((target.data_as_u8_slice().to_vec(), stats))
}

struct CpuCanvas<'a> {
    context: &'a mut RenderContext,
    resources: &'a mut Resources,
}

impl SparseCanvas for CpuCanvas<'_> {
    fn set_transform(&mut self, transform: Affine) {
        self.context.set_transform(transform);
    }
    fn set_paint(&mut self, paint: PaintType) {
        self.context.set_paint(paint);
    }
    fn set_paint_transform(&mut self, transform: Affine) {
        self.context.set_paint_transform(transform);
    }
    fn reset_paint_transform(&mut self) {
        self.context.reset_paint_transform();
    }
    fn set_stroke(&mut self, stroke: Stroke) {
        self.context.set_stroke(stroke);
    }
    fn set_fill_rule(&mut self, rule: vello_common::peniko::Fill) {
        self.context.set_fill_rule(rule);
    }
    fn fill_path(&mut self, path: &BezPath) {
        self.context.fill_path(path);
    }
    fn stroke_path(&mut self, path: &BezPath) {
        self.context.stroke_path(path);
    }
    fn fill_rect(&mut self, rect: &KRect) {
        self.context.fill_rect(rect);
    }
    fn push_clip_layer(&mut self, path: &BezPath) {
        self.context.push_clip_layer(path);
    }
    fn push_blend_layer(&mut self, blend: BlendMode) {
        self.context.push_blend_layer(blend);
    }
    fn pop_layer(&mut self) {
        self.context.pop_layer();
    }
    fn register_image(&mut self, pixmap: Pixmap) -> ImageSource {
        ImageSource::opaque_id(self.resources.register_image(Arc::new(pixmap)))
    }
}
