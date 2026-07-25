//! Renders a sample paragraph to a PNG (manual visual check; not a test).
use casual_doc_layout::compose::compose_paragraph;
use casual_doc_layout::fonts::ROBOTO_REGULAR;
use casual_doc_layout::model::{ModelPos, ModelRange};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::text::{Decoration, FontId, LineConstraints, LineShaper, StyledRun};
use casual_doc_layout::units::{Point, Twip};
use casual_doc_model::NodeId;
use casual_doc_render::{SingleFontSource, Surface, render};

fn main() {
    let shaper = ParleyShaper::new();
    let node = NodeId::from_parts(1, 1).unwrap();
    let runs = [
        StyledRun {
            text: "OpenDoc ",
            font: FontId(0),
            size: Twip::from_points(28),
            color: [20, 20, 20, 255],
            decoration: Decoration::default(),
        },
        StyledRun {
            text: "layout engine",
            font: FontId(0),
            size: Twip::from_points(28),
            color: [200, 60, 20, 255],
            decoration: Decoration {
                underline: true,
                strikethrough: false,
            },
        },
    ];
    let layout = shaper.shape_paragraph(
        &runs,
        LineConstraints {
            max_width: Twip::from_points(400),
            rtl: false,
        },
        ModelRange::new(ModelPos::new(node, 0), ModelPos::new(node, 0)),
    );
    let list = compose_paragraph(
        &layout,
        Point::new(Twip::from_points(16), Twip::from_points(40)),
    );
    let mut surface = Surface::new(640, 120).unwrap();
    render(
        &list,
        &mut surface,
        96.0,
        &SingleFontSource::new(ROBOTO_REGULAR),
    );
    std::fs::write(
        std::env::args().nth(1).unwrap(),
        surface.encode_png().unwrap(),
    )
    .unwrap();
}
