//! Every paint family on one page, each where a mistake would show.
//!
//! `parity.rs` checks real documents, and real documents leave whole families
//! unexercised: no fixture puts ink outside an `exact`-height row's clip, and
//! the watermark multiplies over plain paper, where multiply and normal are the
//! same pixels. A guard that cannot see a family cannot fail for it, so this
//! sheet builds one display list that does: a fill larger than its clip, nested
//! clips, a multiply layer over colour, a rotated layer, gradient and dashed
//! shapes, a rotated shape, arrowheads, polygons and lines, and one real glyph
//! run repeated in every underline style, both strikes, every text effect and a
//! horizontal scale.
//!
//! Same tolerance as `parity.rs` (`support/mod.rs`), same reference
//! (`casual-doc-render`).

#![cfg(any(feature = "sparse-cpu", feature = "gpu"))]
// The per-page lines are the log a reviewer reads when the gate trips.
#![allow(clippy::print_stderr)]

use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::display::{
    Color, DisplayList, Fill, Gradient, GradientKind, GradientStop, LayerBlend, PaintItem,
    PathCommand, ShapeGeometry, ShapeOutline, ShapeTransform, Stroke,
};
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::text::GlyphRun;
use casual_doc_layout::units::{Point, Rect, Size, Twip};
use casual_doc_model::v1::{DashStyle, LineEnd, LineEndKind, LineEndSize, UnderlineStyle};
use casual_doc_ooxml::{DocxPackage, PackageLimits};
use casual_doc_render::{NoMediaSource, RegistryFontSource, Surface, render};

mod support;
use support::difference;

#[allow(dead_code)] // the generator file also carries what only the examples use
#[path = "../../casual-doc-render/examples/support/heavy_docx.rs"]
mod heavy;

const PAGE_W: i32 = 12_240;
const PAGE_H: i32 = 15_840;
/// One inch.
const IN: i32 = 1440;

fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color { r, g, b, a }
}

fn pt(x: i32, y: i32) -> Point {
    Point {
        x: Twip(x),
        y: Twip(y),
    }
}

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect {
        origin: pt(x, y),
        size: Size {
            width: Twip(w),
            height: Twip(h),
        },
    }
}

fn fill_rect(r: Rect, color: Color) -> PaintItem {
    PaintItem::Rect {
        rect: r,
        fill: Some(color),
        stroke: None,
    }
}

/// The first text run of the generated heavy page: real glyphs from a real
/// face, so the decorations sit on real metrics.
fn sample_run() -> (GlyphRun, RegistryFontSource) {
    let bytes = heavy::heavy_docx();
    let mut package = DocxPackage::open(&bytes, PackageLimits::default()).expect("opens");
    let document = import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .expect("imports")
    .document;
    let shaper = ParleyShaper::without_system_fonts();
    let pages = paginate_document(&document, &shaper);
    let fonts = RegistryFontSource::new(&shaper.registry());
    let list = compose_page(&pages.pages[0]);
    let run = list
        .items
        .iter()
        .find_map(|item| match item {
            PaintItem::Glyphs { run } if run.glyphs.len() > 20 => Some(run.clone()),
            _ => None,
        })
        .expect("the heavy page has text");
    (run, fonts)
}

fn sheet(run: &GlyphRun) -> DisplayList {
    let mut list = DisplayList::new();
    let red = rgba(220, 30, 30, 255);
    let cyan = rgba(0, 200, 220, 255);
    let yellow = rgba(250, 220, 0, 255);

    // A fill twice the size of its clip: only the clipped square may show.
    list.push(PaintItem::PushClip(rect(IN / 2, IN / 2, IN, IN)));
    list.push(fill_rect(rect(IN / 4, IN / 4, 2 * IN, 2 * IN), red));
    // Nested: the intersection of both clips.
    list.push(PaintItem::PushClip(rect(IN, IN, IN, IN)));
    list.push(fill_rect(rect(0, 0, 3 * IN, 3 * IN), cyan));
    list.push(PaintItem::PopClip);
    list.push(PaintItem::PopClip);

    // Multiply over colour: cyan × yellow is green, not yellow.
    list.push(fill_rect(rect(3 * IN, IN / 2, IN, IN), cyan));
    list.push(PaintItem::PushLayer {
        transform: None,
        blend: LayerBlend::Multiply,
    });
    list.push(fill_rect(rect(3 * IN + IN / 2, IN / 2, IN, IN), yellow));
    list.push(PaintItem::PopLayer);

    // A rotated layer holding a fill and text.
    list.push(PaintItem::PushLayer {
        transform: Some(ShapeTransform {
            rotation: 30 * 60_000,
            flip_h: false,
            flip_v: false,
            center: pt(6 * IN, IN),
        }),
        blend: LayerBlend::Normal,
    });
    list.push(fill_rect(rect(5 * IN + IN / 2, IN / 2, IN, IN), red));
    let mut rotated = run.clone();
    rotated.origin = pt(5 * IN, IN);
    list.push(PaintItem::Glyphs { run: rotated });
    list.push(PaintItem::PopLayer);

    // Shapes: linear and radial gradients, a dashed curved outline, a rotated
    // flipped rectangle, a line with every arrowhead.
    let stops = vec![
        GradientStop {
            position: 0.0,
            color: rgba(20, 60, 200, 255),
        },
        GradientStop {
            position: 1.0,
            color: rgba(240, 240, 120, 255),
        },
    ];
    list.push(PaintItem::Shape {
        geometry: ShapeGeometry::Ellipse {
            rect: rect(IN / 2, 3 * IN, 2 * IN, IN),
        },
        fill: Some(Fill::Gradient(Gradient {
            stops: stops.clone(),
            kind: GradientKind::Linear { angle_deg: 30.0 },
        })),
        stroke: None,
        head_end: None,
        tail_end: None,
        transform: None,
    });
    list.push(PaintItem::Shape {
        geometry: ShapeGeometry::RoundedRect {
            rect: rect(3 * IN, 3 * IN, 2 * IN, IN),
            radius: Twip(IN / 4),
        },
        fill: Some(Fill::Gradient(Gradient {
            stops,
            kind: GradientKind::Radial,
        })),
        stroke: Some(ShapeOutline {
            color: rgba(0, 0, 0, 255),
            width: 2.0,
            dash: DashStyle::Solid,
        }),
        head_end: None,
        tail_end: None,
        transform: None,
    });
    list.push(PaintItem::Shape {
        geometry: ShapeGeometry::Path {
            commands: vec![
                PathCommand::MoveTo {
                    point: pt(IN / 2, 5 * IN),
                },
                PathCommand::CubicTo {
                    control1: pt(IN, 4 * IN),
                    control2: pt(2 * IN, 6 * IN),
                    point: pt(3 * IN, 5 * IN),
                },
                PathCommand::QuadTo {
                    control: pt(2 * IN, 6 * IN + IN / 2),
                    point: pt(IN / 2, 5 * IN),
                },
            ],
            closed: true,
            fill_rule: casual_doc_model::v1::FillRule::NonZero,
        },
        fill: Some(Fill::Solid(rgba(120, 200, 120, 255))),
        stroke: Some(ShapeOutline {
            color: rgba(20, 20, 120, 255),
            width: 3.0,
            dash: DashStyle::DashDot,
        }),
        head_end: None,
        tail_end: None,
        transform: None,
    });
    list.push(PaintItem::Shape {
        geometry: ShapeGeometry::Rect {
            rect: rect(4 * IN, 4 * IN + IN / 2, 2 * IN, IN / 2),
        },
        fill: Some(Fill::Solid(rgba(200, 100, 40, 255))),
        stroke: None,
        head_end: None,
        tail_end: None,
        transform: Some(ShapeTransform {
            rotation: -20 * 60_000,
            flip_h: true,
            flip_v: false,
            center: pt(5 * IN, 4 * IN + 3 * IN / 4),
        }),
    });
    for (i, kind) in [
        LineEndKind::Triangle,
        LineEndKind::Stealth,
        LineEndKind::Diamond,
        LineEndKind::Oval,
        LineEndKind::Arrow,
    ]
    .into_iter()
    .enumerate()
    {
        let y = 6 * IN + i as i32 * IN / 3;
        list.push(PaintItem::Shape {
            geometry: ShapeGeometry::Line {
                from: pt(IN / 2, y),
                to: pt(3 * IN, y + IN / 6),
            },
            fill: None,
            stroke: Some(ShapeOutline {
                color: rgba(0, 0, 0, 255),
                width: 2.0,
                dash: DashStyle::Solid,
            }),
            head_end: Some(LineEnd {
                kind,
                width: Some(LineEndSize::Large),
                length: None,
            }),
            tail_end: Some(LineEnd {
                kind,
                width: None,
                length: Some(LineEndSize::Small),
            }),
            transform: None,
        });
    }

    // Flat furniture: a polygon with a stroke, a stroked rect, a line, an
    // ellipse, a rounded rect, and a hairline rule thinner than a pixel.
    list.push(PaintItem::Polygon {
        points: vec![
            pt(4 * IN, 6 * IN),
            pt(5 * IN, 6 * IN + IN / 2),
            pt(4 * IN + IN / 2, 7 * IN),
        ],
        fill: Some(rgba(90, 90, 200, 255)),
        stroke: Some(Stroke {
            color: rgba(0, 0, 0, 255),
            width: 1.5,
        }),
    });
    list.push(PaintItem::Rect {
        rect: rect(5 * IN + IN / 2, 6 * IN, IN, IN / 2),
        fill: None,
        stroke: Some(Stroke {
            color: rgba(200, 0, 120, 255),
            width: 2.0,
        }),
    });
    list.push(PaintItem::Line {
        from: pt(5 * IN, 7 * IN),
        to: pt(7 * IN, 7 * IN + IN / 2),
        stroke: Stroke {
            color: rgba(0, 120, 0, 255),
            width: 1.0,
        },
    });
    list.push(PaintItem::Ellipse {
        rect: rect(6 * IN + IN / 2, 3 * IN, IN, IN),
        fill: Some(rgba(250, 150, 0, 255)),
        stroke: Some(Stroke {
            color: rgba(0, 0, 0, 255),
            width: 1.0,
        }),
    });
    list.push(PaintItem::RoundedRect {
        rect: rect(6 * IN + IN / 2, 4 * IN + IN / 4, IN, IN / 2),
        radius: Twip(IN / 8),
        fill: Some(rgba(0, 150, 150, 255)),
        stroke: None,
    });
    list.push(fill_rect(
        rect(IN / 2, 8 * IN, 7 * IN, 5),
        rgba(0, 0, 0, 255),
    ));

    // One run per decoration and effect, one line each.
    let styles = [
        UnderlineStyle::Single,
        UnderlineStyle::Double,
        UnderlineStyle::Thick,
        UnderlineStyle::Dotted,
        UnderlineStyle::Dashed,
        UnderlineStyle::DotDash,
        UnderlineStyle::Wavy,
        UnderlineStyle::Words,
    ];
    let mut y = 8 * IN + IN / 3;
    let mut line = |mutate: &dyn Fn(&mut GlyphRun)| {
        let mut r = run.clone();
        r.origin = pt(IN / 2, y);
        r.decoration.underline = false;
        r.decoration.strikethrough = false;
        r.decoration.double_strike = false;
        mutate(&mut r);
        list.push(PaintItem::Glyphs { run: r });
        y += IN / 4;
    };
    for style in styles {
        line(&|r: &mut GlyphRun| {
            r.decoration.underline = true;
            r.decoration.underline_style = style;
            r.decoration.underline_color = Some([200, 0, 0, 255]);
        });
    }
    line(&|r: &mut GlyphRun| r.decoration.strikethrough = true);
    line(&|r: &mut GlyphRun| r.decoration.double_strike = true);
    line(&|r: &mut GlyphRun| r.decoration.effects.shadow = true);
    line(&|r: &mut GlyphRun| r.decoration.effects.emboss = true);
    line(&|r: &mut GlyphRun| r.decoration.effects.imprint = true);
    line(&|r: &mut GlyphRun| r.decoration.effects.outline = true);
    line(&|r: &mut GlyphRun| {
        r.character_scale_percent = 150;
        r.color = [30, 90, 200, 255];
    });
    list
}

fn check(
    label: &str,
    mut paint: impl FnMut(&DisplayList, u32, u32, f32, &RegistryFontSource) -> Vec<u8>,
) {
    let (run, fonts) = sample_run();
    let list = sheet(&run);
    let mut failures = Vec::new();
    for dpi in [96.0_f32, 192.0] {
        let w = (PAGE_W as f32 * dpi / 1440.0).ceil() as u32;
        let h = (PAGE_H as f32 * dpi / 1440.0).ceil() as u32;
        let mut surface = Surface::new(w, h).expect("surface");
        render(&list, &mut surface, dpi, &fonts, &NoMediaSource);
        let candidate = paint(&list, w, h, dpi, &fonts);
        let diff = difference(surface.data(), &candidate, w as usize);
        support::dump(
            &format!("{label}-sheet-{dpi}"),
            w,
            h,
            surface.data(),
            &candidate,
        );
        eprintln!("{label} sheet @{dpi}: {diff}");
        if !diff.within() {
            failures.push(format!("@{dpi} dpi: {diff}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{label} paints the feature sheet differently from the CPU reference:\n{}",
        failures.join("\n")
    );
}

#[cfg(feature = "sparse-cpu")]
#[test]
fn every_paint_family_on_the_sparse_cpu_painter_matches_the_reference() {
    check("vello_cpu", |list, w, h, dpi, fonts| {
        casual_doc_render_gpu::cpu::render_sparse_cpu(
            list,
            w,
            h,
            dpi,
            [255, 255, 255],
            fonts,
            &NoMediaSource,
        )
        .expect("within 16-bit coordinates")
        .0
    });
}

#[cfg(feature = "gpu")]
#[test]
fn every_paint_family_on_the_gpu_matches_the_reference_where_a_gpu_exists() {
    let mut gpu = match casual_doc_render_gpu::gpu::GpuRenderer::new_blocking() {
        Ok(gpu) => gpu,
        Err(err) => {
            eprintln!("SKIPPED: {err}. The sparse-cpu sheet checks the encoder on this machine.");
            return;
        }
    };
    check("vello_hybrid", |list, w, h, dpi, fonts| {
        gpu.render_rgba(list, w, h, dpi, [255, 255, 255], fonts, &NoMediaSource)
            .expect("renders")
            .rgba
    });
}
