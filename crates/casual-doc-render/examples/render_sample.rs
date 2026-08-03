//! Renders regular/bold/italic/bold-italic text to a PNG (manual visual check).
#![allow(clippy::print_stderr)]
use casual_doc_layout::compose::compose_paragraph;
use casual_doc_layout::fonts::face_id;
use casual_doc_layout::model::{ModelPos, ModelRange};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::text::{Decoration, LineConstraints, LineShaper, StyledRun, TextAlignment};
use casual_doc_layout::units::{Point, Twip};
use casual_doc_model::NodeId;
use casual_doc_render::{BundledFontSource, NoMediaSource, Surface, render};

fn styled(text: &str, bold: bool, italic: bool, color: [u8; 4]) -> StyledRun<'_> {
    StyledRun {
        text: text.into(),
        requested_family: None,
        font: face_id(bold, italic),
        size: Twip::from_points(26),
        character_scale_percent: 100,
        bold,
        italic,
        letter_spacing: Twip::ZERO,
        color,
        decoration: Decoration::default(),
        highlight: None,
        shading: None,
        baseline_shift: Twip::ZERO,
    }
}

fn main() {
    let out = std::env::args().nth(1).unwrap();
    let shaper = ParleyShaper::new();
    let node = NodeId::from_parts(1, 1).unwrap();
    let runs = [
        styled("Regular ", false, false, [20, 20, 20, 255]),
        styled("Bold ", true, false, [180, 40, 20, 255]),
        styled("Italic ", false, true, [20, 100, 40, 255]),
        styled("BoldItalic", true, true, [40, 40, 160, 255]),
    ];
    let layout = shaper.shape_paragraph(
        &runs,
        LineConstraints {
            max_width: Twip::from_points(460),
            alignment: TextAlignment::Start,
            ..LineConstraints::default()
        },
        ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 0)),
    );
    let list = compose_paragraph(
        &layout,
        Point::new(Twip::from_points(16), Twip::from_points(40)),
    );
    let mut surface = Surface::new(680, 100).unwrap();
    render(
        &list,
        &mut surface,
        96.0,
        &BundledFontSource,
        &NoMediaSource,
    );
    std::fs::write(&out, surface.encode_png().unwrap()).unwrap();
    eprintln!("done -> {out}");
}
