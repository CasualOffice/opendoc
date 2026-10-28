// Copyright The opendoc Authors
// SPDX-License-Identifier: Apache-2.0

//! Turned text-box text reaches the pixels (`docs/182` G-3).
//!
//! A whole page is rasterised — model → layout → display list → tiny-skia — and
//! the ink of a vertical text box is measured. Upright, one line of the probe
//! text runs across the page (its ink is wider than tall). Turned a quarter, it
//! runs down the frame: taller than wide, inside the frame, and on the side the
//! line's top faces — the frame's right edge for `vert`, its left for `vert270`.
//! Only the pixels can say the turn was painted rather than merely recorded.

use casual_doc_layout::compose::compose_page;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    AnchorHorizontal, AnchorVertical, BlockNode, Definitions, Document, DrawingAnchor, Extent,
    HorizontalAnchor, HorizontalPosition, InlineNode, Paragraph, ParagraphProperties, Run,
    RunProperties, TextBox, TextBoxBodyProperties, TextBoxInsets, TextBoxVerticalText,
    VerticalAnchor, VerticalPosition, WrapMode,
};
use casual_doc_render::{BundledFontSource, MapMediaSource, Surface, render};

const DPI: f32 = 96.0;
/// The frame, in device pixels at 96 dpi (15 twips each): 1 in wide, 4 in
/// tall, its top-left at (96, 96).
const FRAME: (u32, u32, u32, u32) = (96, 96, 96, 384);
const EMU_PER_PX: i64 = 9_525;

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).unwrap()
}

fn document(direction: TextBoxVerticalText) -> Document {
    let (x, y, w, h) = FRAME;
    let text_box = InlineNode::TextBox(Box::new(TextBox {
        hyperlink: None,
        id: node(20),
        anchor: Some(DrawingAnchor {
            horizontal: AnchorHorizontal {
                relative_from: HorizontalAnchor::Page,
                position: HorizontalPosition::Offset(i64::from(x) * EMU_PER_PX),
            },
            vertical: AnchorVertical {
                relative_from: VerticalAnchor::Page,
                position: VerticalPosition::Offset(i64::from(y) * EMU_PER_PX),
            },
            wrap: WrapMode::None,
            wrap_text: None,
            wrap_distances: Default::default(),
            wrap_polygon: None,
            behind_doc: false,
        }),
        relative_height: Some(1),
        extent: Some(Extent {
            width_emu: i64::from(w) * EMU_PER_PX,
            height_emu: i64::from(h) * EMU_PER_PX,
        }),
        fill: None,
        border: None,
        body_properties: TextBoxBodyProperties {
            insets: TextBoxInsets {
                left_emu: 0,
                top_emu: 0,
                right_emu: 0,
                bottom_emu: 0,
            },
            vertical_text: direction,
            ..TextBoxBodyProperties::default()
        },
        blocks: vec![BlockNode::Paragraph(Paragraph {
            id: node(21),
            properties: ParagraphProperties::default().into(),
            inlines: vec![InlineNode::Run(Run {
                id: node(22),
                properties: RunProperties::default().into(),
                text: "HHHHHHHHHH".to_owned(),
            })],
        })],
    }));
    Document::new(
        node(1),
        vec![BlockNode::Paragraph(Paragraph {
            id: node(10),
            properties: ParagraphProperties::default().into(),
            inlines: vec![text_box],
        })],
        Definitions::default(),
    )
    .expect("valid")
}

/// The ink bounding box `(x0, y0, x1, y1)` of the page, in device pixels.
fn ink(direction: TextBoxVerticalText) -> (u32, u32, u32, u32) {
    let doc = document(direction);
    let pages = paginate_document(&doc, &ParleyShaper::new());
    let page = pages.pages.first().expect("a page");
    let width = page.page_size.width.to_device_px(DPI).ceil() as u32;
    let height = page.page_size.height.to_device_px(DPI).ceil() as u32;
    let mut surface = Surface::new(width, height).expect("surface");
    render(
        &compose_page(page),
        &mut surface,
        DPI,
        &BundledFontSource,
        &MapMediaSource::default(),
    );
    let data = surface.data();
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for y in 0..height {
        for x in 0..width {
            // Ink: a dark pixel on the white page.
            if data[((y * width + x) * 4) as usize] < 128 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    assert!(x0 <= x1, "{direction:?}: the box paints ink");
    (x0, y0, x1, y1)
}

#[test]
fn upright_probe_text_is_wider_than_tall() {
    // The control: without a turn, the same line is horizontal, so the
    // assertions below can tell the two apart.
    let (x0, y0, x1, y1) = ink(TextBoxVerticalText::Horizontal);
    assert!(x1 - x0 > 3 * (y1 - y0), "{:?}", (x0, y0, x1, y1));
}

#[test]
fn a_vertical_text_box_paints_its_line_down_the_frame() {
    let (fx, fy, fw, fh) = FRAME;
    for (direction, right_side) in [
        (TextBoxVerticalText::Vertical, true),
        (TextBoxVerticalText::Vertical270, false),
    ] {
        let (x0, y0, x1, y1) = ink(direction);
        assert!(
            y1 - y0 > 3 * (x1 - x0),
            "{direction:?}: the line runs down the page, ink {:?}",
            (x0, y0, x1, y1)
        );
        assert!(
            x0 >= fx && x1 < fx + fw && y0 >= fy && y1 < fy + fh,
            "{direction:?}: the ink {:?} stays inside the frame",
            (x0, y0, x1, y1)
        );
        let middle = fx + fw / 2;
        if right_side {
            assert!(
                x0 >= middle,
                "vert: the line hugs the right edge, ink x {x0}"
            );
        } else {
            assert!(
                x1 < middle,
                "vert270: the line hugs the left edge, ink x {x1}"
            );
        }
    }
}
