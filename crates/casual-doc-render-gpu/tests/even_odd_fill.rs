//! The second backend honours a path's fill rule (`docs/182` G-2).
//!
//! The figure is two SAME-wound squares, one inside the other: under the
//! non-zero rule the inner square has winding number 2 and is filled; under
//! even-odd it is a hole. A backend that ignores the rule paints the even-odd
//! ring solid, which is what this crate did before the rule reached
//! `SparseCanvas` — so the guard samples the centre of the hole.
//!
//! Both painters are checked: `vello_cpu` everywhere (it runs the same encoder),
//! `vello_hybrid` where a GPU adapter exists. Without one the GPU test says so
//! and passes, as `tests/parity.rs` does.

#![cfg(any(feature = "sparse-cpu", feature = "gpu"))]
#![allow(clippy::print_stderr)]

use casual_doc_layout::display::{Color, DisplayList, Fill, PaintItem, PathCommand, ShapeGeometry};
use casual_doc_layout::units::{Point, Twip};
use casual_doc_model::v1::FillRule;
use casual_doc_render::{BundledFontSource, NoMediaSource};

/// 96 dpi: one pixel is 15 twips.
const DPI: f32 = 96.0;
const SIDE: u32 = 100;

fn p(x: i32, y: i32) -> Point {
    Point::new(Twip(x * 15), Twip(y * 15))
}

fn square(commands: &mut Vec<PathCommand>, x0: i32, y0: i32, x1: i32, y1: i32) {
    // Clockwise in y-down space, both squares the same way.
    commands.push(PathCommand::MoveTo { point: p(x0, y0) });
    commands.push(PathCommand::LineTo { point: p(x1, y0) });
    commands.push(PathCommand::LineTo { point: p(x1, y1) });
    commands.push(PathCommand::LineTo { point: p(x0, y1) });
    commands.push(PathCommand::Close);
}

fn ring(fill_rule: FillRule) -> DisplayList {
    let mut commands = Vec::new();
    square(&mut commands, 10, 10, 90, 90);
    square(&mut commands, 35, 35, 65, 65);
    let mut list = DisplayList::new();
    list.push(PaintItem::Shape {
        geometry: ShapeGeometry::Path {
            commands,
            closed: false,
            fill_rule,
        },
        fill: Some(Fill::Solid(Color::rgb(255, 0, 0))),
        stroke: None,
        head_end: None,
        tail_end: None,
        transform: None,
    });
    list
}

/// The (r, g, b) at device pixel `(x, y)` of premultiplied RGBA over white.
fn at(rgba: &[u8], x: u32, y: u32) -> (u8, u8, u8) {
    let i = ((y * SIDE + x) * 4) as usize;
    (rgba[i], rgba[i + 1], rgba[i + 2])
}

const RED: (u8, u8, u8) = (255, 0, 0);
const WHITE: (u8, u8, u8) = (255, 255, 255);

/// Asserts the guarantee on one painter: the even-odd centre is paper, the
/// non-zero centre is ink, and the ring itself is ink under both.
fn assert_rules(label: &str, paint: impl Fn(&DisplayList) -> Vec<u8>) {
    let even_odd = paint(&ring(FillRule::EvenOdd));
    assert_eq!(
        at(&even_odd, 50, 50),
        WHITE,
        "{label}: under even-odd the same-wound inner square is a hole"
    );
    assert_eq!(at(&even_odd, 20, 50), RED, "{label}: the ring is filled");
    let non_zero = paint(&ring(FillRule::NonZero));
    assert_eq!(
        at(&non_zero, 50, 50),
        RED,
        "{label}: under non-zero the same-wound inner square is filled"
    );
    // The rule does not leak: a non-zero fill painted after an even-odd one
    // on the same canvas is still non-zero.
    let mut both = ring(FillRule::EvenOdd);
    both.items.extend(ring(FillRule::NonZero).items);
    assert_eq!(
        at(&paint(&both), 50, 50),
        RED,
        "{label}: the even-odd rule is restored to non-zero after its fill"
    );
}

#[cfg(feature = "sparse-cpu")]
#[test]
fn the_sparse_cpu_painter_paints_an_even_odd_hole() {
    use casual_doc_render_gpu::cpu::render_sparse_cpu;
    let fonts = BundledFontSource;
    assert_rules("vello_cpu", |list| {
        render_sparse_cpu(
            list,
            SIDE,
            SIDE,
            DPI,
            [255, 255, 255],
            &fonts,
            &NoMediaSource,
        )
        .expect("small page")
        .0
    });
}

#[cfg(feature = "gpu")]
#[test]
fn the_gpu_painter_paints_an_even_odd_hole_where_a_gpu_exists() {
    use casual_doc_render_gpu::gpu::GpuRenderer;
    let mut gpu = match GpuRenderer::new_blocking() {
        Ok(gpu) => gpu,
        Err(err) => {
            eprintln!("SKIPPED: {err}. The sparse-cpu test checks the encoder on this machine.");
            return;
        }
    };
    let fonts = BundledFontSource;
    let gpu = std::cell::RefCell::new(&mut gpu);
    assert_rules("vello_hybrid", |list| {
        gpu.borrow_mut()
            .render_rgba(
                list,
                SIDE,
                SIDE,
                DPI,
                [255, 255, 255],
                &fonts,
                &NoMediaSource,
            )
            .expect("renders")
            .rgba
    });
}
