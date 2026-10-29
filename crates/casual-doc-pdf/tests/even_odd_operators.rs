//! The PDF writer states a path's fill rule (`docs/182` G-2): an even-odd
//! geometry is filled with `f*` and clipped with `W*`, a non-zero one with `f`
//! and `W`.
//!
//! The written file is parsed back with [`casual_doc_pdf::inspect`] and its
//! content stream's OPERATORS are read, so the assertion is about what a PDF
//! reader will execute, not about the writer's internals. A solid fill reaches
//! `f`/`f*`; a gradient fill is a clip plus a shading, which reaches `W`/`W*`.

use casual_doc_layout::display::{
    Color, DisplayList, Fill, Gradient, GradientKind, GradientStop, PaintItem, PathCommand,
    ShapeGeometry,
};
use casual_doc_layout::units::{Point, Twip};
use casual_doc_model::v1::FillRule;
use casual_doc_pdf::inspect::{self, Object};
use casual_doc_pdf::{NoMediaSource, PdfExportOptions, PdfPage, write_pdf};

fn p(x: i32, y: i32) -> Point {
    Point::new(Twip(x), Twip(y))
}

/// Two same-wound nested squares: the figure where the two rules disagree.
fn ring() -> Vec<PathCommand> {
    let mut commands = Vec::new();
    for (x0, y0, x1, y1) in [(200, 200, 1800, 1800), (700, 700, 1300, 1300)] {
        commands.push(PathCommand::MoveTo { point: p(x0, y0) });
        commands.push(PathCommand::LineTo { point: p(x1, y0) });
        commands.push(PathCommand::LineTo { point: p(x1, y1) });
        commands.push(PathCommand::LineTo { point: p(x0, y1) });
        commands.push(PathCommand::Close);
    }
    commands
}

fn shape(fill_rule: FillRule, fill: Fill) -> PaintItem {
    PaintItem::Shape {
        geometry: ShapeGeometry::Path {
            commands: ring(),
            closed: false,
            fill_rule,
        },
        fill: Some(fill),
        stroke: None,
        head_end: None,
        tail_end: None,
        transform: None,
    }
}

fn gradient() -> Fill {
    Fill::Gradient(Gradient {
        stops: vec![
            GradientStop {
                position: 0.0,
                color: Color::rgb(255, 0, 0),
            },
            GradientStop {
                position: 1.0,
                color: Color::rgb(0, 0, 255),
            },
        ],
        kind: GradientKind::Linear { angle_deg: 0.0 },
    })
}

/// The painting and clipping operators of the single page `list` writes, in
/// order: the tokens a reader executes for fills and clips.
fn fill_and_clip_operators(list: &DisplayList) -> Vec<String> {
    let page = PdfPage {
        width: Twip(2_000),
        height: Twip(2_000),
        list,
    };
    let export = write_pdf(
        &[page],
        &casual_doc_pdf::BundledFontSource,
        &NoMediaSource,
        &PdfExportOptions::default(),
    )
    .expect("export");
    let pdf = inspect::parse(&export.bytes).expect("parse");
    let pages = pdf.pages();
    let contents = pdf.get(pages[0], "Contents").expect("a content stream");
    let streams: Vec<&Object> = match contents {
        Object::Array(items) => items.iter().map(|item| pdf.resolve(item)).collect(),
        other => vec![other],
    };
    let mut ops = Vec::new();
    for stream in streams {
        let data = pdf.stream_data(stream).expect("decodable");
        let text = String::from_utf8_lossy(&data).into_owned();
        ops.extend(
            text.split_whitespace()
                .filter(|token| matches!(*token, "f" | "f*" | "F" | "W" | "W*" | "B" | "B*"))
                .map(str::to_owned),
        );
    }
    ops
}

#[test]
fn an_even_odd_solid_fill_is_written_as_f_star() {
    let mut list = DisplayList::new();
    list.push(shape(FillRule::EvenOdd, Fill::Solid(Color::rgb(255, 0, 0))));
    assert_eq!(fill_and_clip_operators(&list), ["f*"]);
}

#[test]
fn a_non_zero_solid_fill_is_written_as_f() {
    let mut list = DisplayList::new();
    list.push(shape(FillRule::NonZero, Fill::Solid(Color::rgb(255, 0, 0))));
    assert_eq!(fill_and_clip_operators(&list), ["f"]);
}

#[test]
fn an_even_odd_gradient_fill_clips_with_w_star() {
    let mut list = DisplayList::new();
    list.push(shape(FillRule::EvenOdd, gradient()));
    assert_eq!(fill_and_clip_operators(&list), ["W*"]);
    let mut list = DisplayList::new();
    list.push(shape(FillRule::NonZero, gradient()));
    assert_eq!(fill_and_clip_operators(&list), ["W"]);
}
