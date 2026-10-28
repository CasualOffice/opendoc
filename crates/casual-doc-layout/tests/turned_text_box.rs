//! Turned text-box text (`docs/182` G-3): `wps:bodyPr@vert`, `@rot` and a group
//! child's `a:xfrm@rot`, through layout, composition and hit-testing.
//!
//! The guarantees, each asserted on the real pipeline (paginate → place floats
//! → compose → hit-test):
//!
//! - vertical lines wrap at the frame's HEIGHT, not its width;
//! - the text is composed upright in its content box and turned onto the frame
//!   by one layer about the frame's centre;
//! - a click on the turned frame resolves to the character drawn there, at
//!   each quarter turn;
//! - a group child's rotation turns its frame and its text together.

use casual_doc_layout::compose::compose_page;
use casual_doc_layout::display::PaintItem;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::hittest::LayoutSnapshot;
use casual_doc_layout::page::{AnchorContent, PaginatedLayout, PlacedAnchor, TextOrientation};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::units::{Point, Twip};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    AnchorHorizontal, AnchorVertical, BlockNode, Definitions, Document, DrawingAnchor, Extent,
    GroupChild, GroupTextBox, GroupTransform, HorizontalAnchor, HorizontalPosition, InlineNode,
    Paragraph, ParagraphProperties, PointEmu, Run, RunProperties, ShapeGeometry, TextBox,
    TextBoxBodyProperties, TextBoxInsets, TextBoxVerticalText, VerticalAnchor, VerticalPosition,
    WordprocessingGroup, WrapMode,
};

const EMU_PER_TWIP: i64 = 635;
/// The frame: 1 in wide, 3 in tall, its top-left at (2000, 2000) twips.
const FRAME_X: i32 = 2_000;
const FRAME_Y: i32 = 2_000;
const FRAME_W: i32 = 1_440;
const FRAME_H: i32 = 4_320;
/// About 1.6 in at 11 pt: one line in the 3 in direction, two or more in the
/// 1 in one.
const TEXT: &str = "alpha beta gamma delta";

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).unwrap()
}

fn anchor() -> DrawingAnchor {
    DrawingAnchor {
        horizontal: AnchorHorizontal {
            relative_from: HorizontalAnchor::Page,
            position: HorizontalPosition::Offset(i64::from(FRAME_X) * EMU_PER_TWIP),
        },
        vertical: AnchorVertical {
            relative_from: VerticalAnchor::Page,
            position: VerticalPosition::Offset(i64::from(FRAME_Y) * EMU_PER_TWIP),
        },
        wrap: WrapMode::None,
        wrap_text: None,
        wrap_distances: Default::default(),
        wrap_polygon: None,
        behind_doc: false,
    }
}

fn extent() -> Extent {
    Extent {
        width_emu: i64::from(FRAME_W) * EMU_PER_TWIP,
        height_emu: i64::from(FRAME_H) * EMU_PER_TWIP,
    }
}

fn body(properties: TextBoxBodyProperties) -> TextBoxBodyProperties {
    TextBoxBodyProperties {
        insets: TextBoxInsets {
            left_emu: 0,
            top_emu: 0,
            right_emu: 0,
            bottom_emu: 0,
        },
        ..properties
    }
}

fn content() -> Vec<BlockNode> {
    vec![BlockNode::Paragraph(Paragraph {
        id: node(21),
        properties: ParagraphProperties::default().into(),
        inlines: vec![InlineNode::Run(Run {
            id: node(22),
            properties: RunProperties::default().into(),
            text: TEXT.to_owned(),
        })],
    })]
}

fn document_with(inline: InlineNode) -> Document {
    Document::new(
        node(1),
        vec![BlockNode::Paragraph(Paragraph {
            id: node(10),
            properties: ParagraphProperties::default().into(),
            inlines: vec![inline],
        })],
        Definitions::default(),
    )
    .expect("valid")
}

/// A lone floating text box with `properties` (insets zeroed).
fn lone(properties: TextBoxBodyProperties) -> Document {
    document_with(InlineNode::TextBox(Box::new(TextBox {
        hyperlink: None,
        id: node(20),
        anchor: Some(anchor()),
        relative_height: Some(1),
        extent: Some(extent()),
        fill: None,
        border: None,
        body_properties: body(properties),
        blocks: content(),
    })))
}

/// A one-child group whose text box is turned by `rotation` (60000ths).
fn grouped(rotation: i32) -> Document {
    let transform = GroupTransform {
        offset: PointEmu { x_emu: 0, y_emu: 0 },
        extent: extent(),
        child_offset: PointEmu { x_emu: 0, y_emu: 0 },
        child_extent: extent(),
        flip_h: false,
        flip_v: false,
        rotation: None,
    };
    document_with(InlineNode::Group(Box::new(WordprocessingGroup {
        hyperlink: None,
        id: node(30),
        anchor: Some(anchor()),
        relative_height: Some(1),
        extent: extent(),
        transform,
        children: vec![GroupChild::TextBox(GroupTextBox {
            id: node(20),
            offset: PointEmu { x_emu: 0, y_emu: 0 },
            extent: extent(),
            geometry: ShapeGeometry::Rectangle,
            preset: None,
            adjustments: Vec::new(),
            blocks: content(),
            fill: None,
            border: None,
            body_properties: body(TextBoxBodyProperties::default()),
            hyperlink: None,
            flip_h: false,
            flip_v: false,
            rotation: Some(rotation),
        })],
    })))
}

fn layout(document: &Document) -> PaginatedLayout {
    paginate_document(document, &ParleyShaper::new())
}

fn the_box(layout: &PaginatedLayout) -> &PlacedAnchor {
    layout.pages[0]
        .anchored
        .iter()
        .find(|anchor| matches!(anchor.content, AnchorContent::TextBox { .. }))
        .expect("the text box is placed")
}

fn orientation_of(anchor: &PlacedAnchor) -> TextOrientation {
    match &anchor.content {
        AnchorContent::TextBox { orientation, .. } => *orientation,
        _ => unreachable!(),
    }
}

fn line_count(anchor: &PlacedAnchor) -> usize {
    match &anchor.content {
        AnchorContent::TextBox { blocks, .. } => blocks
            .iter()
            .map(|block| match block {
                casual_doc_layout::block::BlockFragment::Paragraph { lines, .. } => {
                    lines.lines.len()
                }
                _ => 0,
            })
            .sum(),
        _ => unreachable!(),
    }
}

fn vertical(direction: TextBoxVerticalText) -> TextBoxBodyProperties {
    TextBoxBodyProperties {
        vertical_text: direction,
        ..TextBoxBodyProperties::default()
    }
}

#[test]
fn vertical_lines_wrap_at_the_frames_height_and_keep_its_rect() {
    let upright = layout(&lone(TextBoxBodyProperties::default()));
    assert!(
        line_count(the_box(&upright)) >= 2,
        "the probe text wraps in the 1 in direction, or this test proves nothing"
    );
    for direction in [
        TextBoxVerticalText::Vertical,
        TextBoxVerticalText::Vertical270,
    ] {
        let turned = layout(&lone(vertical(direction)));
        let placed = the_box(&turned);
        assert_eq!(
            line_count(placed),
            1,
            "{direction:?}: the line runs the frame's 3 in height"
        );
        assert_eq!(
            (placed.rect.origin.x, placed.rect.origin.y),
            (Twip(FRAME_X), Twip(FRAME_Y))
        );
        assert_eq!(
            (placed.rect.size.width, placed.rect.size.height),
            (Twip(FRAME_W), Twip(FRAME_H)),
            "{direction:?}: the frame keeps its authored size"
        );
        let turns = if direction == TextBoxVerticalText::Vertical {
            1
        } else {
            3
        };
        assert_eq!(
            orientation_of(placed),
            TextOrientation {
                rotation: turns * 5_400_000,
                exchanged: true
            }
        );
        assert!(placed.transform.is_none(), "a lone frame is not turned");
    }
}

/// The text is composed upright inside the exchanged content box and turned
/// by exactly one layer about the frame's centre.
#[test]
fn turned_text_is_composed_inside_one_layer_about_the_frame_centre() {
    let turned = layout(&lone(vertical(TextBoxVerticalText::Vertical)));
    let list = compose_page(&turned.pages[0]);
    let open = list
        .items
        .iter()
        .position(|item| {
            matches!(item, PaintItem::PushLayer { transform: Some(t), .. }
            if t.rotation == 5_400_000
                && t.center == Point::new(Twip(FRAME_X + FRAME_W / 2), Twip(FRAME_Y + FRAME_H / 2)))
        })
        .expect("a quarter-turn layer about the frame centre");
    let close = open
        + list.items[open..]
            .iter()
            .position(|item| matches!(item, PaintItem::PopLayer))
            .expect("the layer closes");
    let glyphs: Vec<_> = list.items[open..close]
        .iter()
        .filter_map(|item| match item {
            PaintItem::Glyphs { run } => Some(run.origin),
            _ => None,
        })
        .collect();
    assert!(!glyphs.is_empty(), "the text is inside the layer");
    // The exchanged box: 3 in wide, 1 in tall, centred on the frame.
    let left = FRAME_X + FRAME_W / 2 - FRAME_H / 2;
    let top = FRAME_Y + FRAME_H / 2 - FRAME_W / 2;
    for origin in glyphs {
        assert!(
            origin.x.raw() >= left
                && origin.x.raw() < left + FRAME_H
                && origin.y.raw() >= top
                && origin.y.raw() < top + FRAME_W,
            "a run at {origin:?} lies in the upright content box"
        );
    }
    // Outside the layer, no glyph: nothing of the box paints unturned.
    assert!(
        list.items[..open]
            .iter()
            .chain(&list.items[close..])
            .all(|item| !matches!(item, PaintItem::Glyphs { .. })),
        "every glyph of the box is turned"
    );
}

fn hit_offset(layout: &PaginatedLayout, x: i32, y: i32) -> u32 {
    let hit = LayoutSnapshot::new(layout)
        .hit_test_text_box(1, Point::new(Twip(x), Twip(y)))
        .expect("a hit in the box");
    assert_eq!(hit.pos.node, node(21), "the box's paragraph");
    hit.pos.offset
}

/// A click resolves to the character DRAWN there. With the line turned a
/// quarter clockwise it reads top to bottom along the frame's right edge; at
/// three quarters, bottom to top along its left edge; at a half turn, right to
/// left along its bottom.
#[test]
fn a_click_on_turned_text_resolves_to_the_character_drawn_there() {
    let near = |start: u32| start <= 2;
    let far = |at: u32| at >= 10;
    // One quarter: the line's start is at the top, its first line at the right.
    let quarter = layout(&lone(vertical(TextBoxVerticalText::Vertical)));
    let x = FRAME_X + FRAME_W - 100;
    assert!(
        near(hit_offset(&quarter, x, FRAME_Y + 60)),
        "top: line start"
    );
    assert!(
        far(hit_offset(&quarter, x, FRAME_Y + 2_000)),
        "lower: later"
    );
    // Three quarters: the line starts at the bottom, its first line at the left.
    let three = layout(&lone(vertical(TextBoxVerticalText::Vertical270)));
    let x = FRAME_X + 100;
    assert!(
        near(hit_offset(&three, x, FRAME_Y + FRAME_H - 60)),
        "bottom: line start"
    );
    assert!(
        far(hit_offset(&three, x, FRAME_Y + FRAME_H - 2_000)),
        "higher: later"
    );
    // A half turn (`bodyPr@rot`): upside down, starting at the bottom right.
    let half = layout(&lone(TextBoxBodyProperties {
        text_rotation: 10_800_000,
        ..TextBoxBodyProperties::default()
    }));
    assert_eq!(
        orientation_of(the_box(&half)),
        TextOrientation {
            rotation: 10_800_000,
            exchanged: false
        }
    );
    let y = FRAME_Y + FRAME_H - 100;
    assert!(
        near(hit_offset(&half, FRAME_X + FRAME_W - 40, y)),
        "right: line start"
    );
    assert!(far(hit_offset(&half, FRAME_X + 200, y)), "left: later");
}

/// A grouped text box's `a:xfrm@rot` turns its frame (the chrome transform)
/// and its text (the orientation) by the same angle, at any angle.
#[test]
fn a_grouped_text_box_turns_its_frame_and_text_together() {
    for rotation in [1_800_000, 5_400_000, -1_800_000] {
        let turned = layout(&grouped(rotation));
        let placed = the_box(&turned);
        let transform = placed.transform.expect("the frame is turned");
        assert_eq!(
            transform.rotation, rotation,
            "{rotation}: the frame's rotation"
        );
        assert_eq!(
            orientation_of(placed),
            TextOrientation::new(i64::from(rotation), false),
            "{rotation}: the text turns with its shape"
        );
    }
    // At an arbitrary angle a click still finds the character drawn under it:
    // the first line's start and end, turned 30 degrees about the centre on
    // the page (computed here, independently of the layout's own inverse).
    let turned = layout(&grouped(1_800_000));
    let on_page = |x: i32, y: i32| {
        let (cx, cy) = (
            f64::from(FRAME_X + FRAME_W / 2),
            f64::from(FRAME_Y + FRAME_H / 2),
        );
        let (sin, cos) = 30f64.to_radians().sin_cos();
        let (dx, dy) = (f64::from(x) - cx, f64::from(y) - cy);
        (
            (cx + dx * cos - dy * sin).round() as i32,
            (cy + dx * sin + dy * cos).round() as i32,
        )
    };
    let (x, y) = on_page(FRAME_X + 40, FRAME_Y + 120);
    assert!(hit_offset(&turned, x, y) <= 1, "30 degrees: line start");
    let (x, y) = on_page(FRAME_X + FRAME_W - 40, FRAME_Y + 120);
    assert!(hit_offset(&turned, x, y) >= 6, "30 degrees: line end");
}
