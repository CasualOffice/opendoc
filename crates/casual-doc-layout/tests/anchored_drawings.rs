//! Anchored (floating) drawing positioning (P1F-28, first cut), end to end.
//!
//! These drive the real pipeline — flow the body, paginate, run the
//! anchored-placement pass, then compose — and assert the Word-grade behaviors:
//!
//! - an anchored drawing with `positionH`/`positionV` `posOffset` composes to a
//!   `PaintItem::Image` at the computed absolute rect (NOT the inline cursor);
//! - `behindDoc` controls z-order: the image paints before/after the text;
//! - an anchored drawing does not consume an inline line box (it is removed from
//!   the flow), while an inline drawing still flows inline.

use casual_doc_layout::anchor::place_floats;
use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::display::PaintItem;
use casual_doc_layout::flow::build_galley;
use casual_doc_layout::paginate::{PageConfig, paginate};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::units::{Point, Size, Twip};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    AnchorHorizontal, AnchorVertical, AnchoredDrawing, BlockNode, DefinitionMap, Definitions,
    Document, DrawingAnchor, Extent, HorizontalAnchor, HorizontalPosition, InlineNode, MediaId,
    MediaReference, Paragraph, ParagraphProperties, Run, RunProperties, SectionId, VerticalAnchor,
    VerticalPosition, WrapMode,
};

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).unwrap()
}

/// A US-Letter page with 1-inch margins.
fn config() -> PageConfig {
    PageConfig {
        section: SectionId::new(node(9)),
        page_size: Size::new(Twip(12_240), Twip(15_840)),
        margin_top: Twip(1_440),
        margin_bottom: Twip(1_440),
        margin_start: Twip(1_440),
        margin_end: Twip(1_440),
        header_distance: Twip(720),
        footer_distance: Twip(720),
        header_height: Twip::ZERO,
        footer_height: Twip::ZERO,
    }
}

fn media_defs() -> (MediaId, Definitions) {
    let media_id = MediaId::new(node(70));
    let mut media = DefinitionMap::default();
    media.insert(
        media_id,
        MediaReference {
            relationship_id: "rId7".to_owned(),
            media_type: "image/png".to_owned(),
            part_name: "word/media/image1.png".to_owned(),
        },
    );
    (
        media_id,
        Definitions {
            media,
            ..Definitions::default()
        },
    )
}

fn run(id: u64, text: &str) -> InlineNode {
    InlineNode::Run(Run {
        id: node(id),
        properties: RunProperties::default(),
        text: text.to_owned(),
    })
}

fn anchored(id: u64, media: MediaId, h_offset: i64, v_offset: i64, behind_doc: bool) -> InlineNode {
    InlineNode::AnchoredDrawing(AnchoredDrawing {
        id: node(id),
        media,
        extent: Extent {
            width_emu: 914_400, // 1 inch = 1440 twips
            height_emu: 914_400,
        },
        anchor: DrawingAnchor {
            horizontal: AnchorHorizontal {
                relative_from: HorizontalAnchor::Page,
                position: HorizontalPosition::Offset(h_offset),
            },
            vertical: AnchorVertical {
                relative_from: VerticalAnchor::Page,
                position: VerticalPosition::Offset(v_offset),
            },
            wrap: WrapMode::None,
            behind_doc,
        },
        descr: Some("A floating logo".to_owned()),
        relative_height: None,
    })
}

#[test]
fn an_anchored_drawing_composes_at_its_resolved_page_rect() {
    let (media_id, definitions) = media_defs();
    // A paragraph carrying real text plus an anchored drawing at page offset
    // (914400, 1828800) EMU = (1440, 2880) twips from the page corner.
    let para = BlockNode::Paragraph(Paragraph {
        id: node(10),
        properties: ParagraphProperties::default(),
        inlines: vec![
            run(11, "Body text"),
            anchored(12, media_id, 914_400, 1_828_800, false),
        ],
    });
    let doc = Document::new(node(1), vec![para], definitions).unwrap();

    let shaper = ParleyShaper::new();
    let cfg = config();
    let galley = build_galley(&doc, &shaper, cfg.content_area().size.width);

    // The anchored drawing is NOT flowed inline: the paragraph's only line box
    // carries the text run's glyphs, no inline image.
    let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
        panic!("expected a paragraph fragment");
    };
    assert!(
        lines.lines.iter().all(|line| line.images.is_empty()),
        "an anchored drawing must not consume an inline line box"
    );

    let mut layout = paginate(&galley, &cfg);
    place_floats(&mut layout, &doc, &shaper, &cfg);

    // The page now carries the resolved anchored image.
    let page = &layout.pages[0];
    assert_eq!(page.anchored.len(), 1);
    assert_eq!(
        page.anchored[0].rect.origin,
        Point::new(Twip(1_440), Twip(2_880))
    );
    assert_eq!(
        page.anchored[0].rect.size,
        Size::new(Twip(1_440), Twip(1_440))
    );
    assert_eq!(page.anchored[0].descr.as_deref(), Some("A floating logo"));

    // It composes to a PaintItem::Image at that absolute rect — the page offset,
    // not the paragraph's flow cursor (which starts at the content-area origin).
    let list = compose_page(page);
    let rect = list
        .items
        .iter()
        .find_map(|item| match item {
            PaintItem::Image { media, rect } if media == "word/media/image1.png" => Some(*rect),
            _ => None,
        })
        .expect("an anchored image paint item");
    assert_eq!(rect.origin, Point::new(Twip(1_440), Twip(2_880)));
}

#[test]
fn behind_doc_controls_the_paint_order_relative_to_text() {
    let (media_id, definitions) = media_defs();
    // Two anchored drawings: one behind the text, one in front.
    let para = BlockNode::Paragraph(Paragraph {
        id: node(10),
        properties: ParagraphProperties::default(),
        inlines: vec![
            run(11, "Body text"),
            anchored(12, media_id, 0, 0, true),      // behindDoc
            anchored(13, media_id, 100, 100, false), // in front
        ],
    });
    let doc = Document::new(node(1), vec![para], definitions).unwrap();

    let shaper = ParleyShaper::new();
    let cfg = config();
    let galley = build_galley(&doc, &shaper, cfg.content_area().size.width);
    let mut layout = paginate(&galley, &cfg);
    place_floats(&mut layout, &doc, &shaper, &cfg);

    let list = compose_page(&layout.pages[0]);
    let first_image = list
        .items
        .iter()
        .position(|item| matches!(item, PaintItem::Image { .. }))
        .expect("a behind-doc image paints first");
    let glyphs = list
        .items
        .iter()
        .position(|item| matches!(item, PaintItem::Glyphs { .. }))
        .expect("the body text glyphs");
    let last_image = list
        .items
        .iter()
        .rposition(|item| matches!(item, PaintItem::Image { .. }))
        .expect("an in-front image paints last");

    assert!(
        first_image < glyphs,
        "the behindDoc image paints before (behind) the text"
    );
    assert!(
        glyphs < last_image,
        "the in-front image paints after (above) the text"
    );
}

use casual_doc_model::v1::{
    GroupChild, GroupPicture, GroupShape, GroupTransform, PointEmu, Rgba, ShapeGeometry,
    ShapeStroke, TextBox, WordprocessingGroup,
};

fn page_anchor(h: i64, v: i64) -> DrawingAnchor {
    DrawingAnchor {
        horizontal: AnchorHorizontal {
            relative_from: HorizontalAnchor::Page,
            position: HorizontalPosition::Offset(h),
        },
        vertical: AnchorVertical {
            relative_from: VerticalAnchor::Page,
            position: VerticalPosition::Offset(v),
        },
        wrap: WrapMode::None,
        behind_doc: false,
    }
}

#[test]
fn a_floating_text_box_places_at_its_anchor_not_inline() {
    let (_media, definitions) = media_defs();
    // A paragraph carrying body text plus a FLOATING text box (anchor set) at page
    // offset (1440, 2880) twips, 2x1 inch, white fill, and a 30-twip outline.
    let float = InlineNode::TextBox(TextBox {
        id: node(20),
        anchor: Some(page_anchor(914_400, 1_828_800)),
        relative_height: Some(100),
        extent: Some(Extent {
            width_emu: 1_828_800,
            height_emu: 914_400,
        }),
        fill: Some(Rgba {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        }),
        border: Some(ShapeStroke {
            color: Rgba {
                r: 10,
                g: 20,
                b: 30,
                a: 255,
            },
            width_emu: 19_050,
        }),
        blocks: vec![BlockNode::Paragraph(Paragraph {
            id: node(21),
            properties: ParagraphProperties::default(),
            inlines: vec![run(22, "Powered by")],
        })],
    });
    let para = BlockNode::Paragraph(Paragraph {
        id: node(10),
        properties: ParagraphProperties::default(),
        inlines: vec![run(11, "Body text"), float],
    });
    let doc = Document::new(node(1), vec![para], definitions).unwrap();

    let shaper = ParleyShaper::new();
    let cfg = config();
    let galley = build_galley(&doc, &shaper, cfg.content_area().size.width);

    // The floating text box is NOT flowed inline: the body line carries no text box.
    let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
        panic!("expected a paragraph fragment");
    };
    assert!(
        lines.lines.iter().all(|line| line.text_boxes.is_empty()),
        "a floating text box must not flow inline"
    );

    let mut layout = paginate(&galley, &cfg);
    place_floats(&mut layout, &doc, &shaper, &cfg);

    // It is placed on the page at its resolved anchor rectangle.
    let page = &layout.pages[0];
    assert_eq!(page.anchored.len(), 1, "the floating text box is placed");
    assert_eq!(
        page.anchored[0].rect.origin,
        Point::new(Twip(1_440), Twip(2_880))
    );

    // Composition paints the box fill at its anchor origin and its glyphs, not at
    // the paragraph flow cursor.
    let list = compose_page(page);
    let fill = list.items.iter().find_map(|item| match item {
        PaintItem::Rect {
            rect,
            fill: Some(_),
            ..
        } => Some(*rect),
        _ => None,
    });
    assert_eq!(
        fill.expect("the box fill paints").origin,
        Point::new(Twip(1_440), Twip(2_880)),
        "the box fill is at the anchor, not inline"
    );
    assert!(
        list.items.iter().any(|item| matches!(
            item,
            PaintItem::Rect {
                stroke: Some(stroke),
                fill: None,
                ..
            } if stroke.color
                == (casual_doc_layout::display::Color {
                    r: 10,
                    g: 20,
                    b: 30,
                    a: 255,
                })
                && (stroke.width - 2.0).abs() < f32::EPSILON
        )),
        "the floating box keeps its authored outline color and width"
    );
}

#[test]
fn a_group_paints_children_in_document_order_with_the_picture_at_its_own_extent() {
    let (media_id, definitions) = media_defs();
    // A group: a behind rectangle, then the picture (sized by its OWN 1-inch
    // extent, not the group's 2-inch extent), then a front rectangle. Identity
    // transform, group at page offset (1440, 1440) twips.
    let ident = GroupTransform {
        offset: PointEmu { x_emu: 0, y_emu: 0 },
        extent: Extent {
            width_emu: 1_828_800,
            height_emu: 1_828_800,
        },
        child_offset: PointEmu { x_emu: 0, y_emu: 0 },
        child_extent: Extent {
            width_emu: 1_828_800,
            height_emu: 1_828_800,
        },
    };
    let rect = |id: u64, off: i64| {
        GroupChild::Shape(GroupShape {
            id: node(id),
            offset: PointEmu {
                x_emu: off,
                y_emu: off,
            },
            extent: Extent {
                width_emu: 1_828_800,
                height_emu: 1_828_800,
            },
            geometry: ShapeGeometry::Rectangle,
            fill: Some(Rgba {
                r: 200,
                g: 200,
                b: 200,
                a: 255,
            }),
            stroke: None,
        })
    };
    let group = InlineNode::Group(WordprocessingGroup {
        id: node(30),
        anchor: Some(page_anchor(914_400, 914_400)),
        relative_height: Some(5),
        extent: Extent {
            width_emu: 1_828_800,
            height_emu: 1_828_800,
        },
        transform: ident,
        children: vec![
            rect(31, 0),
            GroupChild::Picture(GroupPicture {
                id: node(32),
                media: media_id,
                offset: PointEmu {
                    x_emu: 100_000,
                    y_emu: 100_000,
                },
                extent: Extent {
                    width_emu: 914_400, // 1 inch — NOT the 2-inch group extent
                    height_emu: 914_400,
                },
                descr: None,
            }),
            rect(33, 200_000),
        ],
    });
    let para = BlockNode::Paragraph(Paragraph {
        id: node(10),
        properties: ParagraphProperties::default(),
        inlines: vec![run(11, "Body"), group],
    });
    let doc = Document::new(node(1), vec![para], definitions).unwrap();

    let shaper = ParleyShaper::new();
    let cfg = config();
    let galley = build_galley(&doc, &shaper, cfg.content_area().size.width);
    let mut layout = paginate(&galley, &cfg);
    place_floats(&mut layout, &doc, &shaper, &cfg);

    let page = &layout.pages[0];
    assert_eq!(page.anchored.len(), 3, "three group children placed");
    // The picture is sized by its own extent (1 inch = 1440 twips), not the group.
    let image = page
        .anchored
        .iter()
        .find(|a| {
            matches!(
                a.content,
                casual_doc_layout::page::AnchorContent::Image { .. }
            )
        })
        .expect("the picture");
    assert_eq!(
        image.rect.size,
        Size::new(Twip(1_440), Twip(1_440)),
        "the grouped picture keeps its own extent, not the group's"
    );

    // Composition paints them in document order: rect, image, rect.
    let list = compose_page(page);
    let kinds: Vec<&str> = list
        .items
        .iter()
        .filter_map(|item| match item {
            PaintItem::Image { .. } => Some("image"),
            PaintItem::Rect { fill: Some(c), .. }
                if *c
                    == (casual_doc_layout::display::Color {
                        r: 200,
                        g: 200,
                        b: 200,
                        a: 255,
                    }) =>
            {
                Some("rect")
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        kinds,
        vec!["rect", "image", "rect"],
        "children paint in document order: a rectangle behind the picture, one in front"
    );
}
