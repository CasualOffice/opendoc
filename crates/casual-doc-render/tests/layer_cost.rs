//! A layer costs its content, not the page (HF-423).
//!
//! `draw_layer` used to give every `PushLayer` a page-sized offscreen pixmap:
//! an allocation, a clear and a bilinear composite of every page pixel, per
//! layer. A recognised scan page opens two layers per OCR line box, so a page
//! of ~70 lines painted in 12.9 s at 192 dpi in Chrome and the tab froze on
//! every scroll. A layer is now sized to its content's bounds.
//!
//! Asserted as the guarantee, not a time: the offscreen pixels a page of 100
//! small layers allocates stay under two pages' worth (it was 100 pages), and
//! an upright layer paints exactly what its content paints without one.
//!
//! Seen red: with `draw_layer` back on a page-sized pixmap, the first test
//! counts 100 pages of offscreen pixels.

use casual_doc_layout::display::{Color, DisplayList, LayerBlend, PaintItem, ShapeTransform};
use casual_doc_layout::units::{Point, Rect, Size, Twip};
use casual_doc_render::{
    BundledFontSource, MapMediaSource, Surface, layer_pixels_allocated, render,
};

const DPI: f32 = 96.0;
/// US Letter at 96 dpi.
const PAGE: (u32, u32) = (816, 1056);
const TWIPS_PER_PX: i32 = 15;

fn px_rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect::new(
        Point::new(Twip(x * TWIPS_PER_PX), Twip(y * TWIPS_PER_PX)),
        Size::new(Twip(w * TWIPS_PER_PX), Twip(h * TWIPS_PER_PX)),
    )
}

fn filled(rect: Rect) -> PaintItem {
    PaintItem::Rect {
        rect,
        fill: Some(Color {
            r: 20,
            g: 60,
            b: 160,
            a: 255,
        }),
        stroke: None,
    }
}

/// `count` small boxes down the page, each in its own layer: every other one
/// turned 3 degrees about its centre, the way an OCR line box is.
fn page_of_layers(count: i32) -> DisplayList {
    let mut list = DisplayList::new();
    for i in 0..count {
        let (x, y, w, h) = (40 + (i % 2) * 360, 20 + (i / 2) * 20, 300, 12);
        let transform = (i % 2 == 1).then(|| ShapeTransform {
            rotation: 3 * 60_000,
            flip_h: false,
            flip_v: false,
            center: Point::new(
                Twip((x + w / 2) * TWIPS_PER_PX),
                Twip((y + h / 2) * TWIPS_PER_PX),
            ),
        });
        list.push(PaintItem::PushLayer {
            transform,
            blend: LayerBlend::Normal,
        });
        list.push(filled(px_rect(x, y, w, h)));
        list.push(PaintItem::PopLayer);
    }
    list
}

fn paint(list: &DisplayList) -> Surface {
    let mut surface = Surface::new(PAGE.0, PAGE.1).expect("surface");
    render(
        list,
        &mut surface,
        DPI,
        &BundledFontSource,
        &MapMediaSource::new(),
    );
    surface
}

#[test]
fn layers_cost_their_content_not_the_page() {
    let list = page_of_layers(100);
    let before = layer_pixels_allocated();
    paint(&list);
    let allocated = layer_pixels_allocated() - before;
    let page = u64::from(PAGE.0) * u64::from(PAGE.1);
    assert!(
        allocated < 2 * page,
        "100 small layers allocated {allocated} offscreen pixels, {:.1} pages' worth: \
         a layer is paying for the page, not for its content",
        allocated as f64 / page as f64,
    );
}

#[test]
fn an_upright_layer_paints_what_its_content_paints() {
    let mut layered = DisplayList::new();
    let mut plain = DisplayList::new();
    for (i, rect) in [
        px_rect(33, 41, 120, 17),
        px_rect(400, 700, 7, 90),
        px_rect(0, 0, 816, 3),
    ]
    .into_iter()
    .enumerate()
    {
        layered.push(PaintItem::PushLayer {
            transform: None,
            blend: if i == 1 {
                LayerBlend::Multiply
            } else {
                LayerBlend::Normal
            },
        });
        layered.push(filled(rect));
        layered.push(PaintItem::PopLayer);
        plain.push(filled(rect));
    }
    assert_eq!(
        paint(&layered).data(),
        paint(&plain).data(),
        "a layer sized to its content moved its pixels",
    );
}
