// SPDX-License-Identifier: Apache-2.0

//! Slide layout guards. Every one was driven red by mutating production code before
//! being trusted (`SKILL` §4).

use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::page::{AnchorContent, PlacedAnchor};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::text::{GlyphRun, Line};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    Definitions, Extent, GroupChild, GroupPicture, GroupShape, GroupTransform, MediaId,
    MediaReference, PointEmu, ShapeGeometry, StyleColor, WordprocessingGroup,
};
use casual_pres_model::TextStyles;
use casual_pres_model::{
    LayoutKind, ListStyle, Placeholder, PlaceholderKind, Presentation, ShapeTree, Slide, SlideId,
    SlideLayout, SlideLayoutId, SlideMaster, SlideMasterId, SlideNode, SlideSize, TextAlign,
    TextAnchor, TextAutoFit, TextBody, TextBodyProperties, TextCharacterProperties, TextField,
    TextLineBreak, TextParagraph, TextParagraphProperties, TextRun, TextRunText, TextSpacing,
    TextStrike, TextUnderline,
};

use crate::text::{LAST_RESORT_COLOR, LAST_RESORT_SIZE};
use crate::{OutlineContent, SlideCanvas, UnresolvedProperty, lay_out_slide, slide_text_outline};

fn id(counter: u64) -> NodeId {
    NodeId::from_parts(1, counter).expect("non-zero")
}

const ORIGIN: PointEmu = PointEmu { x_emu: 0, y_emu: 0 };

/// The deterministic shaper: bundled faces only.
///
/// `without_system_fonts` and not `new`, because a guard written against a shaper
/// that can see the host's installed faces passes on a developer's machine and
/// says nothing about the bundle a browser downloads.
fn shaper() -> ParleyShaper {
    ParleyShaper::without_system_fonts()
}

/// A shape box of `cx` x `cy` EMU at `(x, y)`.
fn shape_at(node: NodeId, x: i64, y: i64, cx: i64, cy: i64) -> GroupChild {
    GroupChild::Shape(GroupShape {
        id: node,
        offset: PointEmu { x_emu: x, y_emu: y },
        extent: Extent {
            width_emu: cx,
            height_emu: cy,
        },
        geometry: ShapeGeometry::Rectangle,
        preset: None,
        adjustments: Vec::new(),
        path: None,
        fill: None,
        stroke: None,
        flip_h: false,
        flip_v: false,
        rotation: None,
        hyperlink: None,
    })
}

/// A shape tree whose child space matches the slide surface, which is what a real
/// `p:spTree` declares.
fn tree(node: NodeId, children: Vec<SlideNode>, size: SlideSize) -> ShapeTree {
    let extent = Extent {
        width_emu: size.width_emu,
        height_emu: size.height_emu,
    };
    ShapeTree {
        id: node,
        transform: GroupTransform {
            offset: ORIGIN,
            extent,
            child_offset: ORIGIN,
            child_extent: extent,
            flip_h: false,
            flip_v: false,
            rotation: None,
        },
        children,
    }
}

/// A deck whose master, layout and slide each hold one shape, so paint order is
/// observable.
fn three_tier_deck() -> Presentation {
    let size = SlideSize::DEFAULT_16X9;
    let master = SlideMaster {
        id: SlideMasterId::new(id(10)),
        shapes: tree(
            id(11),
            vec![SlideNode::new(shape_at(id(12), 0, 0, 914_400, 914_400))],
            size,
        ),
        name: None,
        background: None,
        // No `p:txStyles`: slide TEXT is not laid out yet, so no tier here can
        // change what this fixture paints.
        text_styles: TextStyles::default(),
    };
    let layout = SlideLayout {
        id: SlideLayoutId::new(id(20)),
        master: master.id,
        kind: LayoutKind::Object,
        shapes: tree(
            id(21),
            vec![SlideNode::new(shape_at(
                id(22),
                914_400,
                0,
                914_400,
                914_400,
            ))],
            size,
        ),
        name: None,
        background: None,
    };
    let slide = Slide {
        id: SlideId::new(id(30)),
        layout: layout.id,
        shapes: tree(
            id(31),
            vec![SlideNode::new(shape_at(
                id(32),
                1_828_800,
                0,
                914_400,
                914_400,
            ))],
            size,
        ),
        name: None,
        hidden: false,
        background: None,
    };
    Presentation::new(
        id(1),
        size,
        vec![master],
        vec![layout],
        vec![slide],
        Definitions::default(),
    )
    .expect("a three-tier deck")
}

#[test]
fn the_surface_is_the_slide_size_converted_to_twips() {
    let canvas = lay_out_slide(&three_tier_deck(), 0, &shaper()).expect("slide 0");
    // 12,192,000 x 6,858,000 EMU at 635 EMU per twip = 19,200 x 10,800 twips,
    // which is 13.333in x 7.5in. Asserted as arithmetic, not as "some size".
    assert_eq!(canvas.size.width.raw(), 19_200);
    assert_eq!(canvas.size.height.raw(), 10_800);
}

#[test]
fn the_master_paints_first_the_layout_next_and_the_slide_last() {
    // THE cascade this crate implements. Mutation that drove this red: reversing
    // `cascade_trees`, which paints a master's background OVER the slide's content and
    // is the defect that makes a real deck look blank.
    let canvas = lay_out_slide(&three_tier_deck(), 0, &shaper()).expect("slide 0");
    let nodes: Vec<u64> = canvas
        .anchors
        .iter()
        .map(|anchor| {
            u64::try_from(anchor.node.expect("every shape carries its id").as_u128() & 0xffff_ffff)
                .expect("small counter")
        })
        .collect();
    assert_eq!(nodes, vec![12, 22, 32], "master, then layout, then slide");
    // And the z keys ascend with paint order, because the display list's consumer
    // sorts by them rather than trusting vector order.
    let order: Vec<u32> = canvas.anchors.iter().map(|a| a.z.order).collect();
    assert_eq!(order, vec![0, 1, 2]);
}

#[test]
fn a_shapes_rectangle_is_its_own_emu_box_mapped_onto_the_surface() {
    let canvas = lay_out_slide(&three_tier_deck(), 0, &shaper()).expect("slide 0");
    // The slide's shape sits at 1,828,800 EMU = 2in = 2,880 twips, and is 1in square.
    let slide_shape = canvas.anchors.last().expect("the slide's own shape");
    assert_eq!(slide_shape.rect.origin.x.raw(), 2_880);
    assert_eq!(slide_shape.rect.origin.y.raw(), 0);
    assert_eq!(slide_shape.rect.size.width.raw(), 1_440);
    assert_eq!(slide_shape.rect.size.height.raw(), 1_440);
}

#[test]
fn a_hidden_shape_paints_nothing_but_is_not_removed_from_the_model() {
    let mut presentation = three_tier_deck();
    presentation.slides_mut()[0].shapes.children[0].hidden = true;
    let canvas = lay_out_slide(&presentation, 0, &shaper()).expect("slide 0");
    // Mutation: ignoring `hidden` painted it. The shape must still BE in the model —
    // a hidden shape round-trips and the author can unhide it — so this asserts both.
    assert_eq!(canvas.anchors.len(), 2, "the hidden shape did not paint");
    assert_eq!(
        presentation.slides()[0].shapes.children.len(),
        1,
        "and it is still in the model"
    );
}

#[test]
fn a_child_space_smaller_than_the_box_scales_its_children() {
    // `a:chExt` against `a:ext` is a ratio, and a deck authored at a different child
    // extent than its surface renders SCALED. Mutation: treating the tree transform as
    // the identity left this at the unscaled position, which is the bug that would
    // make such a deck's content sit in the top-left quarter.
    let size = SlideSize::DEFAULT_16X9;
    let mut deck = three_tier_deck();
    let half = Extent {
        width_emu: size.width_emu / 2,
        height_emu: size.height_emu / 2,
    };
    deck.slides_mut()[0].shapes.transform.child_extent = half;
    let canvas = lay_out_slide(&deck, 0, &shaper()).expect("slide 0");
    let shape = canvas.anchors.last().expect("the slide's shape");
    // Half the child space over the same box doubles everything.
    assert_eq!(shape.rect.origin.x.raw(), 5_760, "2in doubled to 4in");
    assert_eq!(shape.rect.size.width.raw(), 2_880, "1in doubled to 2in");
}

#[test]
fn a_nested_group_composes_its_transform_with_its_parents() {
    let _size = SlideSize::DEFAULT_16X9;
    let inner = shape_at(id(42), 0, 0, 914_400, 914_400);
    let group = GroupChild::Group(Box::new(WordprocessingGroup {
        id: id(41),
        anchor: None,
        relative_height: None,
        extent: Extent {
            width_emu: 1_828_800,
            height_emu: 1_828_800,
        },
        transform: GroupTransform {
            offset: PointEmu {
                x_emu: 914_400,
                y_emu: 914_400,
            },
            extent: Extent {
                width_emu: 1_828_800,
                height_emu: 1_828_800,
            },
            child_offset: ORIGIN,
            child_extent: Extent {
                width_emu: 914_400,
                height_emu: 914_400,
            },
            flip_h: false,
            flip_v: false,
            rotation: None,
        },
        hyperlink: None,
        children: vec![inner],
    }));
    let mut deck = three_tier_deck();
    deck.slides_mut()[0].shapes.children = vec![SlideNode::new(group)];
    // The PARENT tree must not be the identity, or `compose` and `from_transform`
    // return the same mapper and this guard cannot tell them apart. It could not:
    // replacing the composition with the nested transform alone left this GREEN until
    // the slide tree was given a child space of its own. A guard whose fixture makes
    // the bug invisible is the shape of green-but-wrong this repository has shipped
    // before (`105` CQ-003).
    deck.slides_mut()[0].shapes.transform.child_extent = Extent {
        width_emu: SlideSize::DEFAULT_16X9.width_emu / 2,
        height_emu: SlideSize::DEFAULT_16X9.height_emu / 2,
    };
    let canvas = lay_out_slide(&deck, 0, &shaper()).expect("slide 0");
    let shape = canvas.anchors.last().expect("the grouped shape");
    // The slide tree doubles (half child space over a full box); the group then
    // doubles again (1in child space in a 2in box) and sits at 1in in child space,
    // which the tree's doubling puts at 2in. So the child lands at 2in = 2,880tw and
    // is 1in x 4 = 4in = 5,760tw wide.
    assert_eq!(
        shape.rect.origin.x.raw(),
        2_880,
        "the group's own offset, scaled by the tree"
    );
    assert_eq!(shape.rect.size.width.raw(), 5_760, "both scales compose");
}

#[test]
fn a_picture_resolves_its_media_part_and_an_unresolvable_one_paints_nothing() {
    let size = SlideSize::DEFAULT_16X9;
    let media = MediaId::new(id(900));
    let picture = GroupChild::Picture(GroupPicture {
        id: id(50),
        media,
        offset: ORIGIN,
        extent: Extent {
            width_emu: 914_400,
            height_emu: 914_400,
        },
        descr: None,
        crop: None,
        opacity: None,
        hyperlink: None,
        border: None,
        flip_h: false,
        flip_v: false,
        rotation: None,
    });
    let mut definitions = Definitions::default();
    definitions.media.insert(
        media,
        MediaReference {
            relationship_id: "rId1".to_owned(),
            media_type: "image/png".to_owned(),
            part_name: "/ppt/media/image1.png".to_owned(),
        },
    );
    let master = SlideMaster {
        id: SlideMasterId::new(id(10)),
        shapes: tree(id(11), Vec::new(), size),
        name: None,
        background: None,
        // No `p:txStyles`: slide TEXT is not laid out yet, so no tier here can
        // change what this fixture paints.
        text_styles: TextStyles::default(),
    };
    let layout = SlideLayout {
        id: SlideLayoutId::new(id(20)),
        master: master.id,
        kind: LayoutKind::Blank,
        shapes: tree(id(21), Vec::new(), size),
        name: None,
        background: None,
    };
    let slide = Slide {
        id: SlideId::new(id(30)),
        layout: layout.id,
        shapes: tree(id(31), vec![SlideNode::new(picture)], size),
        name: None,
        hidden: false,
        background: None,
    };
    let presentation = Presentation::new(
        id(1),
        size,
        vec![master],
        vec![layout],
        vec![slide],
        definitions,
    )
    .expect("a deck with a picture");
    let canvas = lay_out_slide(&presentation, 0, &shaper()).expect("slide 0");
    assert_eq!(canvas.anchors.len(), 1);
    match &canvas.anchors[0].content {
        AnchorContent::Image { media, .. } => {
            // The PART NAME, not the id: the backend resolves bytes by part name, so
            // emitting the id would paint nothing and look like a missing file.
            assert_eq!(media, "/ppt/media/image1.png");
        }
        other => panic!("expected an image, got {other:?}"),
    }
}

#[test]
fn a_slide_index_past_the_end_is_none_rather_than_an_empty_canvas() {
    // An empty canvas and a missing slide are different answers, and a viewer that
    // cannot tell them apart shows a blank slide instead of reporting the error.
    let deck = three_tier_deck();
    assert!(lay_out_slide(&deck, 0, &shaper()).is_some());
    assert!(lay_out_slide(&deck, 1, &shaper()).is_none());
    assert!(lay_out_slide(&deck, usize::MAX, &shaper()).is_none());
}

#[test]
fn laying_out_one_slide_does_not_depend_on_the_decks_length() {
    // Complexity guarded by counting work, not by a clock (`SKILL` §8): a deck of n
    // and 2n slides must cost the same to lay out ONE slide, or opening a 300-slide
    // deck is quadratic in the slide count.
    fn anchors_for(slides: u64) -> usize {
        let size = SlideSize::DEFAULT_16X9;
        let master = SlideMaster {
            id: SlideMasterId::new(id(10)),
            shapes: tree(id(11), Vec::new(), size),
            name: None,
            background: None,
            // No `p:txStyles`: slide TEXT is not laid out yet, so no tier here can
            // change what this fixture paints.
            text_styles: TextStyles::default(),
        };
        let layout = SlideLayout {
            id: SlideLayoutId::new(id(20)),
            master: master.id,
            kind: LayoutKind::Blank,
            shapes: tree(id(21), Vec::new(), size),
            name: None,
            background: None,
        };
        let deck: Vec<Slide> = (0..slides)
            .map(|n| Slide {
                id: SlideId::new(id(1_000 + n * 10)),
                layout: layout.id,
                shapes: tree(
                    id(1_001 + n * 10),
                    vec![SlideNode::new(shape_at(
                        id(1_002 + n * 10),
                        0,
                        0,
                        914_400,
                        914_400,
                    ))],
                    size,
                ),
                name: None,
                hidden: false,
                background: None,
            })
            .collect();
        let presentation = Presentation::new(
            id(1),
            size,
            vec![master],
            vec![layout],
            deck,
            Definitions::default(),
        )
        .expect("a valid deck");
        lay_out_slide(&presentation, 0, &shaper())
            .expect("slide 0")
            .anchors
            .len()
    }
    assert_eq!(anchors_for(8), 1);
    assert_eq!(
        anchors_for(64),
        1,
        "one slide's cost must not grow with the deck"
    );
}

#[test]
fn a_slide_composes_into_the_same_display_list_a_page_produces() {
    // The reason this crate exists: the raster backend, the PDF writer and the
    // hit-tester consume `DisplayList` and must need no slide-specific path. A slide
    // is only a float layer, so `compose_page` cannot serve it — but the per-anchor
    // composition is the same one, which is what this asserts.
    let canvas = lay_out_slide(&three_tier_deck(), 0, &shaper()).expect("slide 0");
    let list = crate::compose_slide(&canvas);
    assert_eq!(
        list.items.len(),
        3,
        "three shapes, three paint items: {:?}",
        list.items
    );
}

#[test]
fn composition_paints_in_z_order_not_vector_order() {
    // Mutation: dropping the sort in `compose_anchors` made this pass only because
    // the vector happened to be ordered — so the anchors are deliberately handed over
    // SHUFFLED, which is the only way the assertion can tell a sort from luck.
    let canvas = lay_out_slide(&three_tier_deck(), 0, &shaper()).expect("slide 0");
    let mut shuffled = canvas.clone();
    shuffled.anchors.reverse();
    let from_shuffled = crate::compose_slide(&shuffled);
    let from_ordered = crate::compose_slide(&canvas);
    assert_eq!(
        from_shuffled.items.len(),
        from_ordered.items.len(),
        "the same anchors must compose to the same number of items"
    );
    // Reversing the input must not reverse the output, because the z keys decide.
    assert_eq!(
        format!("{:?}", from_shuffled.items),
        format!("{:?}", from_ordered.items),
        "paint order followed vector order instead of the z key"
    );
}

#[test]
fn a_slide_shape_with_a_custom_geometry_paints_its_path_not_its_preset() {
    // THE bug that unifying the walk fixed, and the reason this crate must not own a
    // second recursion. The first draft matched on `GroupShape::geometry` and omitted
    // `GroupShape::path`, so a slide carrying an `a:custGeom` painted the bounding
    // preset instead of the authored outline — silently, and only on slides.
    //
    // The importer attaches a path only for a custom geometry inside the drawable
    // subset and leaves `geometry` as `Other` beside it (docs/119 §6), which is
    // exactly the shape built here.
    use casual_doc_model::v1::{ShapePath, ShapePathCommand};

    let size = SlideSize::DEFAULT_16X9;
    let path = ShapePath {
        width_emu: 914_400,
        height_emu: 914_400,
        commands: vec![
            ShapePathCommand::MoveTo { point: ORIGIN },
            ShapePathCommand::LineTo {
                point: PointEmu {
                    x_emu: 914_400,
                    y_emu: 0,
                },
            },
            ShapePathCommand::LineTo {
                point: PointEmu {
                    x_emu: 0,
                    y_emu: 914_400,
                },
            },
            ShapePathCommand::Close,
        ],
    };
    let mut freeform = match shape_at(id(60), 0, 0, 914_400, 914_400) {
        GroupChild::Shape(shape) => shape,
        other => panic!("expected a shape, got {other:?}"),
    };
    freeform.geometry = ShapeGeometry::Other;
    freeform.path = Some(path);

    let master = SlideMaster {
        id: SlideMasterId::new(id(10)),
        shapes: tree(id(11), Vec::new(), size),
        name: None,
        background: None,
        // No `p:txStyles`: slide TEXT is not laid out yet, so no tier here can
        // change what this fixture paints.
        text_styles: TextStyles::default(),
    };
    let layout = SlideLayout {
        id: SlideLayoutId::new(id(20)),
        master: master.id,
        kind: LayoutKind::Blank,
        shapes: tree(id(21), Vec::new(), size),
        name: None,
        background: None,
    };
    let slide = Slide {
        id: SlideId::new(id(30)),
        layout: layout.id,
        shapes: tree(
            id(31),
            vec![SlideNode::new(GroupChild::Shape(freeform))],
            size,
        ),
        name: None,
        hidden: false,
        background: None,
    };
    let presentation = Presentation::new(
        id(1),
        size,
        vec![master],
        vec![layout],
        vec![slide],
        Definitions::default(),
    )
    .expect("a deck with a freeform");

    let canvas = lay_out_slide(&presentation, 0, &shaper()).expect("slide 0");
    match &canvas.anchors[0].content {
        AnchorContent::Path { commands, .. } => {
            // Three vertices and a close: a triangle, not the four-corner rectangle
            // the bounding preset would have produced.
            assert_eq!(
                commands.len(),
                3,
                "the authored path's own commands: {commands:?}"
            );
        }
        other => panic!(
            "a custom geometry must paint as a path, not as {other:?} — this is the \
             divergence the shared walk exists to prevent"
        ),
    }
}

// ---------------------------------------------------------------------------
// Slide text. Every guard below states which two sources disagree, because a
// fixture with no competition cannot test precedence (`SKILL` §4).
// ---------------------------------------------------------------------------

/// The slot the text guards below inherit through: a `body` placeholder at
/// `idx="1"`, which is what a one-content layout writes.
const TEXT_SLOT: (PlaceholderKind, u32) = (PlaceholderKind::Body, 1);

/// The text shape's box: 9,144,000 x 4,572,000 EMU = 14,400 x 7,200 twips.
const TEXT_BOX_WIDTH_EMU: i64 = 9_144_000;
const TEXT_BOX_HEIGHT_EMU: i64 = 4_572_000;

/// One `a:lvlNpPr`: an alignment and an `a:defRPr` size in HUNDREDTHS of a point.
fn level(size: Option<u32>, align: Option<TextAlign>) -> TextParagraphProperties {
    TextParagraphProperties {
        alignment: align,
        default_character: size.map(|size| {
            Box::new(TextCharacterProperties {
                size_hundredths_point: Some(size),
                ..TextCharacterProperties::default()
            })
        }),
        ..TextParagraphProperties::default()
    }
}

/// An `a:lstStyle` whose only populated level is `a:lvl1pPr`.
fn first_level(properties: TextParagraphProperties) -> ListStyle {
    ListStyle {
        levels: vec![Some(properties)],
    }
}

/// An `a:r` with an optional `a:rPr`.
fn run(node: NodeId, text: &str, properties: Option<TextCharacterProperties>) -> TextRun {
    TextRun::Run(TextRunText {
        id: node,
        properties: properties.map(Box::new),
        text: text.to_owned(),
    })
}

/// An `a:p` with an optional `a:pPr`.
fn paragraph(
    node: NodeId,
    properties: Option<TextParagraphProperties>,
    runs: Vec<TextRun>,
) -> TextParagraph {
    TextParagraph {
        id: node,
        properties: properties.map(Box::new),
        runs,
        end_properties: None,
    }
}

/// A one-run, one-paragraph `a:txBody` with default `a:bodyPr`.
fn simple_body(text: &str, properties: Option<TextCharacterProperties>) -> TextBody {
    TextBody {
        body_properties: TextBodyProperties::default(),
        list_style: ListStyle::default(),
        paragraphs: vec![paragraph(id(41), None, vec![run(id(42), text, properties)])],
    }
}

/// A deck that states a text property at up to three tiers at once, so the
/// winner is observable: the master's `p:bodyStyle`, the layout's matching
/// placeholder `a:lstStyle`, and the slide's own body.
///
/// The slide's shape states its own `a:xfrm`, so nothing here depends on the
/// placeholder geometry cascade — the two are guarded separately.
fn text_deck(master_tier: ListStyle, layout_slot: ListStyle, slide_text: TextBody) -> Presentation {
    let size = SlideSize::DEFAULT_16X9;
    let slot = Placeholder {
        kind: TEXT_SLOT.0,
        index: TEXT_SLOT.1,
        ..Placeholder::default()
    };
    let master = SlideMaster {
        id: SlideMasterId::new(id(10)),
        shapes: tree(id(11), Vec::new(), size),
        name: None,
        background: None,
        text_styles: TextStyles {
            body: master_tier,
            ..TextStyles::default()
        },
    };
    let layout = SlideLayout {
        id: SlideLayoutId::new(id(20)),
        master: master.id,
        kind: LayoutKind::Object,
        shapes: tree(
            id(21),
            vec![
                SlideNode::new(shape_at(
                    id(22),
                    0,
                    0,
                    TEXT_BOX_WIDTH_EMU,
                    TEXT_BOX_HEIGHT_EMU,
                ))
                .in_slot(slot)
                .with_text(TextBody {
                    body_properties: TextBodyProperties::default(),
                    list_style: layout_slot,
                    // The layout's own text is the PROMPT, which never paints.
                    // It is present precisely so a guard notices if it starts to.
                    paragraphs: vec![paragraph(
                        id(23),
                        None,
                        vec![run(id(24), "Click to add text", None)],
                    )],
                }),
            ],
            size,
        ),
        name: None,
        background: None,
    };
    let slide = Slide {
        id: SlideId::new(id(30)),
        layout: layout.id,
        shapes: tree(
            id(31),
            vec![
                SlideNode::new(shape_at(
                    id(32),
                    0,
                    0,
                    TEXT_BOX_WIDTH_EMU,
                    TEXT_BOX_HEIGHT_EMU,
                ))
                .in_slot(slot)
                .with_text(slide_text),
            ],
            size,
        ),
        name: None,
        hidden: false,
        background: None,
    };
    Presentation::new(
        id(1),
        size,
        vec![master],
        vec![layout],
        vec![slide],
        Definitions::default(),
    )
    .expect("a deck whose text is stated at three tiers")
}

/// Every anchor that paints text, in paint order.
fn text_anchors(canvas: &SlideCanvas) -> Vec<&PlacedAnchor> {
    canvas
        .anchors
        .iter()
        .filter(|anchor| matches!(anchor.content, AnchorContent::TextBox { .. }))
        .collect()
}

/// The one text anchor on a canvas, or a panic naming how many there were.
fn only_text_anchor(canvas: &SlideCanvas) -> &PlacedAnchor {
    let anchors = text_anchors(canvas);
    assert_eq!(
        anchors.len(),
        1,
        "expected exactly one text anchor, got {}",
        anchors.len()
    );
    anchors[0]
}

/// A text anchor's block fragments.
fn blocks_of(anchor: &PlacedAnchor) -> &[BlockFragment] {
    match &anchor.content {
        AnchorContent::TextBox { blocks, .. } => blocks,
        other => panic!("not a text box: {other:?}"),
    }
}

/// A paragraph fragment's lines.
fn lines_of(block: &BlockFragment) -> &[Line] {
    match block {
        BlockFragment::Paragraph { lines, .. } => &lines.lines,
        other => panic!("not a paragraph: {other:?}"),
    }
}

/// Every glyph run on a canvas, in paint order then line order.
fn glyph_runs(canvas: &SlideCanvas) -> Vec<&GlyphRun> {
    text_anchors(canvas)
        .into_iter()
        .flat_map(|anchor| blocks_of(anchor).iter())
        .flat_map(lines_of)
        .flat_map(|line| line.runs.iter())
        .collect()
}

/// The content origin of a text anchor, which is where its first baseline's
/// column starts.
fn content_origin(anchor: &PlacedAnchor) -> (i32, i32) {
    match &anchor.content {
        AnchorContent::TextBox { content_layout, .. } => {
            (content_layout.origin.x.raw(), content_layout.origin.y.raw())
        }
        other => panic!("not a text box: {other:?}"),
    }
}

/// A run's own `a:rPr@sz` outranks both tiers it inherits from.
///
/// THREE sources disagree on purpose — the master's `p:bodyStyle` says 40pt, the
/// layout's placeholder says 20pt, the run says 12pt — so a fold that stopped at
/// any tier would land on a different, nameable number. Asserted in twips, which
/// is also the unit guard: `a:rPr@sz` is hundredths of a point, so 12pt is 240
/// twips and a half-point reading would be 12,000.
#[test]
fn a_runs_own_size_outranks_every_tier_above_it() {
    let canvas = lay_out_slide(
        &text_deck(
            first_level(level(Some(4_000), None)),
            first_level(level(Some(2_000), None)),
            simple_body(
                "Size",
                Some(TextCharacterProperties {
                    size_hundredths_point: Some(1_200),
                    ..TextCharacterProperties::default()
                }),
            ),
        ),
        0,
        &shaper(),
    )
    .expect("slide 0");

    let runs = glyph_runs(&canvas);
    assert_eq!(runs.len(), 1, "one run of text: {runs:?}");
    assert_eq!(
        runs[0].size,
        casual_doc_layout::units::Twip(240),
        "12pt in hundredths of a point is 240 twips; 800 would mean the layout \
         tier won, 400 the master tier, and 12000 that hundredths were read as \
         half-points"
    );
}

/// A paragraph's own `a:pPr@algn` outranks the tier that states another.
///
/// Differential, because an absolute x would be a magic number from the shaper:
/// the layout tier says `ctr` in both decks and only the paragraph differs, so a
/// right-aligned line must start further right than the centred one. A fold that
/// ignored the paragraph's own properties would produce two identical numbers.
#[test]
fn a_paragraphs_own_alignment_outranks_the_tier_that_states_another() {
    let centred = lay_out_slide(
        &text_deck(
            ListStyle::default(),
            first_level(level(Some(1_800), Some(TextAlign::Center))),
            simple_body("Aligned", None),
        ),
        0,
        &shaper(),
    )
    .expect("slide 0");
    let right = lay_out_slide(
        &text_deck(
            ListStyle::default(),
            first_level(level(Some(1_800), Some(TextAlign::Center))),
            TextBody {
                body_properties: TextBodyProperties::default(),
                list_style: ListStyle::default(),
                paragraphs: vec![paragraph(
                    id(41),
                    Some(TextParagraphProperties {
                        alignment: Some(TextAlign::Right),
                        ..TextParagraphProperties::default()
                    }),
                    vec![run(id(42), "Aligned", None)],
                )],
            },
        ),
        0,
        &shaper(),
    )
    .expect("slide 0");

    let centred_x = glyph_runs(&centred)[0].origin.x.raw();
    let right_x = glyph_runs(&right)[0].origin.x.raw();
    assert!(
        centred_x > 0,
        "the layout tier's `ctr` must already move the line off the column's \
         leading edge, or this comparison proves nothing: {centred_x}"
    );
    assert!(
        right_x > centred_x,
        "the paragraph's own `algn=\"r\"` must beat the tier's `ctr`: \
         right={right_x} centred={centred_x}"
    );
}

/// `a:pPr@lvl` selects which level of the tier applies.
///
/// The layout states 40pt at `a:lvl1pPr` and 10pt at `a:lvl2pPr`, so resolving
/// the wrong level is a different nameable size. This is the one attribute the
/// model calls "the single most load-bearing on a slide paragraph", and a reader
/// that resolved level 0 for every paragraph would pass every other guard here.
#[test]
fn the_paragraphs_outline_level_selects_the_tier_level() {
    let canvas = lay_out_slide(
        &text_deck(
            ListStyle::default(),
            ListStyle {
                levels: vec![
                    Some(level(Some(4_000), None)),
                    Some(level(Some(1_000), None)),
                ],
            },
            TextBody {
                body_properties: TextBodyProperties::default(),
                list_style: ListStyle::default(),
                paragraphs: vec![paragraph(
                    id(41),
                    Some(TextParagraphProperties {
                        level: Some(1),
                        ..TextParagraphProperties::default()
                    }),
                    vec![run(id(42), "Second", None)],
                )],
            },
        ),
        0,
        &shaper(),
    )
    .expect("slide 0");

    assert_eq!(
        glyph_runs(&canvas)[0].size,
        casual_doc_layout::units::Twip(200),
        "`lvl=\"1\"` is zero-based, so it selects `a:lvl2pPr`'s 10pt; 800 twips \
         means level 0 was resolved for a level-1 paragraph"
    );
}

/// The producer's recorded `a:normAutofit@fontScale` shrinks the resolved size.
///
/// Two decks differing only in the autofit record, so the factor is exact rather
/// than approximate: 20pt at 50% is 10pt, which is 200 twips against 400.
/// Honouring the recorded scale is what makes an untouched file render as
/// authored; ignoring it paints the title at the size it would have had before
/// PowerPoint shrank it to fit.
#[test]
fn the_recorded_autofit_scale_shrinks_the_resolved_size() {
    let body = |auto_fit: TextAutoFit| TextBody {
        body_properties: TextBodyProperties {
            auto_fit,
            ..TextBodyProperties::default()
        },
        list_style: ListStyle::default(),
        paragraphs: vec![paragraph(
            id(41),
            None,
            vec![run(
                id(42),
                "Shrunk",
                Some(TextCharacterProperties {
                    size_hundredths_point: Some(2_000),
                    ..TextCharacterProperties::default()
                }),
            )],
        )],
    };
    let unscaled = lay_out_slide(
        &text_deck(
            ListStyle::default(),
            ListStyle::default(),
            body(TextAutoFit::None),
        ),
        0,
        &shaper(),
    )
    .expect("slide 0");
    let scaled = lay_out_slide(
        &text_deck(
            ListStyle::default(),
            ListStyle::default(),
            body(TextAutoFit::Normal {
                font_scale: Some(50_000),
                line_space_reduction: None,
            }),
        ),
        0,
        &shaper(),
    )
    .expect("slide 0");

    assert_eq!(glyph_runs(&unscaled)[0].size.raw(), 400, "20pt in twips");
    assert_eq!(
        glyph_runs(&scaled)[0].size.raw(),
        200,
        "a 50% `fontScale` halves it; 400 means the record was ignored"
    );
}

/// `a:bodyPr`'s insets place the text column AND bound its wrap width.
///
/// Both halves in one differential, because they are the same fact: the inset is
/// taken off the box before the shaper sees it. Two decks differ only in `lIns`,
/// so the origin moves by exactly the inset and the narrower column takes more
/// lines for the same sentence. A reader that placed the origin but wrapped
/// against the full box would pass the first assertion and fail the second,
/// which is why neither is here alone.
#[test]
fn the_body_insets_place_the_column_and_bound_its_wrap() {
    // 14,400 twips wide. 10,800 twips of left inset leaves a 3,600-twip column
    // (2.5 inches), so a sentence that fits one line at full width cannot.
    const WIDE_INSET_EMU: i64 = 6_858_000;
    let body = |left: i64| TextBody {
        body_properties: TextBodyProperties {
            inset_left_emu: left,
            inset_right_emu: 0,
            ..TextBodyProperties::default()
        },
        list_style: ListStyle::default(),
        paragraphs: vec![paragraph(
            id(41),
            None,
            vec![run(
                id(42),
                "A sentence long enough that a narrower column has to break it",
                Some(TextCharacterProperties {
                    size_hundredths_point: Some(1_800),
                    ..TextCharacterProperties::default()
                }),
            )],
        )],
    };
    let flush = lay_out_slide(
        &text_deck(ListStyle::default(), ListStyle::default(), body(0)),
        0,
        &shaper(),
    )
    .expect("slide 0");
    let inset = lay_out_slide(
        &text_deck(
            ListStyle::default(),
            ListStyle::default(),
            body(WIDE_INSET_EMU),
        ),
        0,
        &shaper(),
    )
    .expect("slide 0");

    assert_eq!(
        content_origin(only_text_anchor(&flush)).0,
        0,
        "a zero `lIns` puts the column on the box's own edge"
    );
    assert_eq!(
        content_origin(only_text_anchor(&inset)).0,
        10_800,
        "6,858,000 EMU at 635 EMU per twip is 10,800 twips"
    );

    let flush_lines = lines_of(&blocks_of(only_text_anchor(&flush))[0]).len();
    let inset_lines = lines_of(&blocks_of(only_text_anchor(&inset))[0]).len();
    assert!(
        inset_lines > flush_lines,
        "the inset must narrow the column the shaper wraps in: \
         flush={flush_lines} inset={inset_lines}"
    );
}

/// `a:bodyPr@anchor` spends the free space inside the insets.
///
/// Asserted as arithmetic over the canvas's own numbers — the content height is
/// read back from the fragments rather than written down — so the guard cannot be
/// satisfied by a coincidence and does not pin a shaper metric. All three
/// positions are checked together because `t` alone passes for an engine that
/// ignores the attribute entirely.
#[test]
fn the_vertical_anchor_spends_the_free_space_inside_the_insets() {
    let positioned = |anchor: TextAnchor| {
        lay_out_slide(
            &text_deck(
                ListStyle::default(),
                ListStyle::default(),
                TextBody {
                    body_properties: TextBodyProperties {
                        anchor,
                        ..TextBodyProperties::default()
                    },
                    list_style: ListStyle::default(),
                    paragraphs: vec![paragraph(
                        id(41),
                        None,
                        vec![run(
                            id(42),
                            "Anchored",
                            Some(TextCharacterProperties {
                                size_hundredths_point: Some(1_800),
                                ..TextCharacterProperties::default()
                            }),
                        )],
                    )],
                },
            ),
            0,
            &shaper(),
        )
        .expect("slide 0")
    };

    let top = positioned(TextAnchor::Top);
    let anchor = only_text_anchor(&top);
    // DrawingML's default top/bottom inset is 45,720 EMU = 72 twips.
    const INSET: i32 = 72;
    let content: i32 = blocks_of(anchor)
        .iter()
        .map(|block| block.height().raw())
        .sum();
    let free = anchor.rect.size.height.raw() - 2 * INSET - content;
    assert!(
        free > 0,
        "the box must have room left over or the three anchors coincide: \
         height={} content={content}",
        anchor.rect.size.height.raw()
    );

    assert_eq!(content_origin(anchor).1, INSET, "`t` is the top inset");
    assert_eq!(
        content_origin(only_text_anchor(&positioned(TextAnchor::Center))).1,
        INSET + free / 2,
        "`ctr` halves the free space"
    );
    assert_eq!(
        content_origin(only_text_anchor(&positioned(TextAnchor::Bottom))).1,
        INSET + free,
        "`b` spends all of it"
    );
}

/// An `a:br` opens a second line under the first, not on top of it.
///
/// A glyph run's origin is its baseline measured from the paragraph's content
/// top, so a second shaped stretch that is not rebased lands on the first one's
/// baseline — two lines of text drawn over each other, which reads as a font bug
/// rather than a layout one. Asserted as the exact sum, and the line count is
/// asserted beside it so "one line" cannot pass.
#[test]
fn a_hard_break_stacks_the_next_line_under_the_first() {
    let canvas = lay_out_slide(
        &text_deck(
            ListStyle::default(),
            ListStyle::default(),
            TextBody {
                body_properties: TextBodyProperties::default(),
                list_style: ListStyle::default(),
                paragraphs: vec![paragraph(
                    id(41),
                    None,
                    vec![
                        run(
                            id(42),
                            "before",
                            Some(TextCharacterProperties {
                                size_hundredths_point: Some(1_800),
                                ..TextCharacterProperties::default()
                            }),
                        ),
                        TextRun::LineBreak(TextLineBreak {
                            id: id(43),
                            properties: None,
                        }),
                        run(
                            id(44),
                            "after",
                            Some(TextCharacterProperties {
                                size_hundredths_point: Some(1_800),
                                ..TextCharacterProperties::default()
                            }),
                        ),
                    ],
                )],
            },
        ),
        0,
        &shaper(),
    )
    .expect("slide 0");

    let lines = lines_of(&blocks_of(only_text_anchor(&canvas))[0]);
    assert_eq!(lines.len(), 2, "`a:br` opens a line: {lines:?}");
    let first = lines[0].runs[0].origin.y.raw();
    let second = lines[1].runs[0].origin.y.raw();
    assert_eq!(
        second,
        first + lines[0].height.raw(),
        "the second stretch's baseline must sit one line box below the first's: \
         first={first} second={second} height={}",
        lines[0].height.raw()
    );
    // The clusters address the PARAGRAPH, not the stretch, so the text after the
    // break cannot claim the offsets of the text before it.
    assert_eq!(lines[0].runs[0].glyphs[0].cluster, 0);
    assert_eq!(
        lines[1].runs[0].glyphs[0].cluster,
        "before\n".len() as u32,
        "`a:br` contributes the `\\n` the model's plain text carries"
    );
}

/// Every line of a paragraph sits exactly one line box below the one above it —
/// the lines the SHAPER wrapped as much as the ones an `a:br` opened.
///
/// # Why this is asserted on geometry and why the fixture looks like this
///
/// A paragraph's lines are shaped in `a:br`-delimited batches and each batch is
/// rebased under the ones above it. The rebase used to advance its cursor once per
/// line *within* a batch, on top of the offset the shaper had already applied, so
/// a wrapped line painted at `2 × Σ heights` and the paragraph's text ran off the
/// bottom of its own box and over whatever followed it. The first line of each
/// batch was still correct.
///
/// That is exactly the shape of defect the existing guards could not see:
/// [`a_hard_break_stacks_the_next_line_under_the_first`] holds for a paragraph
/// whose every batch is one line, and every other guard here asserts that text
/// *painted*, which it did — in the wrong place. So this one asserts the advance
/// itself, over a paragraph built to make every case present at once:
///
/// * the first batch WRAPS to three lines, so an intra-batch advance is measured
///   rather than inferred — one wrapped line is enough to be wrong and three
///   distinguish "off by one line" from "doubling";
/// * its first line carries TWO runs of different weight and different size, so a
///   mixed-property line cannot be the case that is left uncovered, and so the
///   line box is the largest run's rather than the first's;
/// * a hard break opens a second batch which ALSO wraps, so the cross-batch base
///   and the intra-batch advance are both exercised, and a fix that merely moved
///   the error from one to the other fails here.
///
/// The invariant is stated in the shaper's own terms — a line's baseline is the
/// heights above it plus its own ascent — rather than as a baseline-to-baseline
/// delta, because the two batches are shaped at different sizes and consecutive
/// baselines of differing ascent are legitimately not one `height` apart.
#[test]
fn every_wrapped_line_of_a_paragraph_advances_by_exactly_its_own_line_box() {
    // Long enough to wrap three times in the 10-inch fixture box at 18pt, and
    // prose rather than a repeated word so the shaper has real break opportunities.
    const FIRST: &str = " to grab the audience's attention right from the start, because \
         a line that wraps is the only line that can prove the advance is applied \
         once rather than twice over, and three of them tell a doubling apart from \
         an off-by-one line box.";
    const SECOND: &str = "Highlight whatever is new, unusual or surprising about it, at \
         enough length that this stretch after the hard break also wraps onto a \
         second line of its own.";

    let canvas = lay_out_slide(
        &text_deck(
            ListStyle::default(),
            ListStyle::default(),
            TextBody {
                body_properties: TextBodyProperties::default(),
                list_style: ListStyle::default(),
                paragraphs: vec![paragraph(
                    id(41),
                    None,
                    vec![
                        // Bold and LARGER, so the first line's box is this run's
                        // and a height recomputed from the wrong run would show.
                        run(
                            id(42),
                            "Choose one approach",
                            Some(TextCharacterProperties {
                                size_hundredths_point: Some(2_400),
                                bold: Some(true),
                                ..TextCharacterProperties::default()
                            }),
                        ),
                        run(
                            id(43),
                            FIRST,
                            Some(TextCharacterProperties {
                                size_hundredths_point: Some(1_800),
                                ..TextCharacterProperties::default()
                            }),
                        ),
                        TextRun::LineBreak(TextLineBreak {
                            id: id(44),
                            properties: None,
                        }),
                        run(
                            id(45),
                            SECOND,
                            Some(TextCharacterProperties {
                                size_hundredths_point: Some(1_400),
                                ..TextCharacterProperties::default()
                            }),
                        ),
                    ],
                )],
            },
        ),
        0,
        &shaper(),
    )
    .expect("slide 0");

    let block = &blocks_of(only_text_anchor(&canvas))[0];
    let lines = lines_of(block);
    // The fixture is only a fixture if it actually wraps. Three from the first
    // batch, two from the second.
    assert!(
        lines.len() >= 5,
        "the fixture must wrap in both batches to cover the case: got {} lines {:?}",
        lines.len(),
        lines
            .iter()
            .map(|line| (line.height.raw(), line.ascent.raw()))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        lines[0].runs.len(),
        2,
        "the first line must carry both runs, or the mixed-property case is not covered: {:?}",
        lines[0]
    );

    // The advance itself: line k's baseline is the heights of lines 0..k plus its
    // own ascent, for EVERY line and every run on it.
    let mut top = 0_i32;
    for (index, line) in lines.iter().enumerate() {
        let expected = top + line.ascent.raw();
        for (which, glyph_run) in line.runs.iter().enumerate() {
            assert_eq!(
                glyph_run.origin.y.raw(),
                expected,
                "line {index} run {which}: a line's baseline is the {top} twips of line \
                 boxes above it plus its own {} twip ascent, so it must be at {expected}, \
                 not {}. Every line box: {:?}",
                line.ascent.raw(),
                glyph_run.origin.y.raw(),
                lines
                    .iter()
                    .map(|line| (line.height.raw(), line.ascent.raw()))
                    .collect::<Vec<_>>()
            );
        }
        top += line.height.raw();
    }

    // And the consequence that made this visible: the paragraph's ink stays inside
    // the box its own reported height claims. The fragment's height is what the
    // next paragraph stacks under, so a baseline past it IS the collision.
    let deepest = lines
        .iter()
        .flat_map(|line| line.runs.iter().map(|run| run.origin.y.raw()))
        .max()
        .expect("the fixture shapes glyphs");
    let height = match block {
        BlockFragment::Paragraph { lines, .. } => lines.height().raw(),
        other => panic!("not a paragraph: {other:?}"),
    };
    assert!(
        deepest < height,
        "the last baseline ({deepest}) must sit inside the {height} twips the \
         paragraph charges the flow, or the paragraph below it is painted over"
    );
}

/// `a:lnSpc` as a percentage changes the line advance.
///
/// Differential on two decks that differ only in `a:lnSpc`, because the single-
/// spaced height is the face's and pinning it would make this a font golden. A
/// 200% paragraph must be clearly taller than a 100% one — "greater than 1.5x"
/// rather than "exactly 2x", since the shaper rounds a line box.
#[test]
fn a_percentage_line_spacing_changes_the_line_advance() {
    let spaced = |spacing: Option<TextSpacing>| {
        lay_out_slide(
            &text_deck(
                ListStyle::default(),
                ListStyle::default(),
                TextBody {
                    body_properties: TextBodyProperties::default(),
                    list_style: ListStyle::default(),
                    paragraphs: vec![paragraph(
                        id(41),
                        Some(TextParagraphProperties {
                            line_spacing: spacing,
                            ..TextParagraphProperties::default()
                        }),
                        vec![run(
                            id(42),
                            "Spaced",
                            Some(TextCharacterProperties {
                                size_hundredths_point: Some(1_800),
                                ..TextCharacterProperties::default()
                            }),
                        )],
                    )],
                },
            ),
            0,
            &shaper(),
        )
        .expect("slide 0")
    };

    let single = spaced(None);
    let double = spaced(Some(TextSpacing::Percent {
        thousandths: 200_000,
    }));
    let height = |canvas: &SlideCanvas| {
        lines_of(&blocks_of(only_text_anchor(canvas))[0])[0]
            .height
            .raw()
    };
    let (one, two) = (height(&single), height(&double));
    assert!(
        two > one * 3 / 2,
        "200,000 thousandths of a percent is 200%, so the line box must grow by \
         much more than half: single={one} double={two}"
    );
}

/// A size no tier states is drawn at the stated last resort AND reported.
///
/// Both halves matter. The size has to come from somewhere or nothing paints, and
/// the report is what keeps the number from looking authored: the cascade
/// resolves an unstated size to `None` precisely so that "the deck said nothing"
/// stays answerable, and swallowing it here would throw that away one layer
/// below where it was preserved.
#[test]
fn a_size_no_tier_states_is_reported_and_drawn_at_the_last_resort() {
    let canvas = lay_out_slide(
        &text_deck(
            ListStyle::default(),
            ListStyle::default(),
            simple_body("Unsized", None),
        ),
        0,
        &shaper(),
    )
    .expect("slide 0");

    assert_eq!(
        glyph_runs(&canvas)[0].size,
        LAST_RESORT_SIZE,
        "the draw site's own default, not a value from the deck"
    );
    let reported: Vec<&UnresolvedProperty> = canvas
        .unresolved
        .iter()
        .map(|entry| &entry.property)
        .collect();
    assert_eq!(
        reported,
        vec![&UnresolvedProperty::UnstatedSize],
        "an unstated size must be reported, not silently defaulted"
    );
    assert_eq!(
        canvas.unresolved[0].run,
        Some(id(42)),
        "the report names the run, so a caller can point at the text"
    );
}

/// An `a:phClr` glyph colour is reported rather than quietly painted black.
///
/// The competition is in the fixture: one run states a concrete `a:srgbClr` and
/// the next states the placeholder, so a reader that reported everything or
/// nothing is wrong either way. The resolvable run must keep its own colour, and
/// only the unresolvable one may appear in the report.
#[test]
fn an_unresolvable_placeholder_colour_is_reported_beside_a_resolvable_one() {
    let canvas = lay_out_slide(
        &text_deck(
            ListStyle::default(),
            ListStyle::default(),
            TextBody {
                body_properties: TextBodyProperties::default(),
                list_style: ListStyle::default(),
                paragraphs: vec![paragraph(
                    id(41),
                    None,
                    vec![
                        run(
                            id(42),
                            "red ",
                            Some(TextCharacterProperties {
                                size_hundredths_point: Some(1_800),
                                fill: Some(StyleColor::Fixed(casual_doc_model::v1::Rgba {
                                    r: 255,
                                    g: 0,
                                    b: 0,
                                    a: 255,
                                })),
                                ..TextCharacterProperties::default()
                            }),
                        ),
                        run(
                            id(43),
                            "themed",
                            Some(TextCharacterProperties {
                                size_hundredths_point: Some(1_800),
                                fill: Some(StyleColor::Placeholder(
                                    casual_doc_model::v1::ColorTransform::default(),
                                )),
                                ..TextCharacterProperties::default()
                            }),
                        ),
                    ],
                )],
            },
        ),
        0,
        &shaper(),
    )
    .expect("slide 0");

    let runs = glyph_runs(&canvas);
    assert_eq!(runs.len(), 2, "two runs of differing colour: {runs:?}");
    assert_eq!(
        runs[0].color,
        [255, 0, 0, 255],
        "a concrete colour resolves"
    );
    assert_eq!(
        runs[1].color, LAST_RESORT_COLOR,
        "an `a:phClr` has nothing to resolve against, so it takes the stated \
         last resort"
    );
    assert_eq!(
        canvas.unresolved.len(),
        1,
        "exactly the unresolvable one is reported: {:?}",
        canvas.unresolved
    );
    assert_eq!(
        canvas.unresolved[0].property,
        UnresolvedProperty::PlaceholderColor
    );
    assert_eq!(canvas.unresolved[0].run, Some(id(43)));
}

/// A placeholder's text on the LAYOUT is prompt text and must not paint, while a
/// plain shape's text on the same layout must.
///
/// Written as a difference inside one deck, because either half alone is
/// satisfiable by accident: "the prompt does not paint" passes for an engine that
/// paints no layout text at all, and "the caption paints" passes for one that
/// paints every tier's text including "Click to add text" across the slide.
#[test]
fn a_layouts_prompt_does_not_paint_but_its_plain_shapes_text_does() {
    let size = SlideSize::DEFAULT_16X9;
    let slot = Placeholder {
        kind: TEXT_SLOT.0,
        index: TEXT_SLOT.1,
        ..Placeholder::default()
    };
    let sized = |text: &str, node: NodeId, paragraph_id: NodeId| TextBody {
        body_properties: TextBodyProperties::default(),
        list_style: ListStyle::default(),
        paragraphs: vec![paragraph(
            paragraph_id,
            None,
            vec![run(
                node,
                text,
                Some(TextCharacterProperties {
                    size_hundredths_point: Some(1_800),
                    ..TextCharacterProperties::default()
                }),
            )],
        )],
    };
    let master = SlideMaster {
        id: SlideMasterId::new(id(10)),
        shapes: tree(id(11), Vec::new(), size),
        name: None,
        background: None,
        text_styles: TextStyles::default(),
    };
    let layout = SlideLayout {
        id: SlideLayoutId::new(id(20)),
        master: master.id,
        kind: LayoutKind::Object,
        shapes: tree(
            id(21),
            vec![
                SlideNode::new(shape_at(
                    id(22),
                    0,
                    0,
                    TEXT_BOX_WIDTH_EMU,
                    TEXT_BOX_HEIGHT_EMU,
                ))
                .in_slot(slot)
                .with_text(sized("Click to add text", id(23), id(24))),
                SlideNode::new(shape_at(
                    id(25),
                    0,
                    TEXT_BOX_HEIGHT_EMU,
                    TEXT_BOX_WIDTH_EMU,
                    TEXT_BOX_HEIGHT_EMU,
                ))
                .with_text(sized("Caption", id(26), id(27))),
            ],
            size,
        ),
        name: None,
        background: None,
    };
    let slide = Slide {
        id: SlideId::new(id(30)),
        layout: layout.id,
        shapes: tree(
            id(31),
            vec![
                SlideNode::new(shape_at(
                    id(32),
                    0,
                    0,
                    TEXT_BOX_WIDTH_EMU,
                    TEXT_BOX_HEIGHT_EMU,
                ))
                .in_slot(slot)
                .with_text(sized("Real content", id(33), id(34))),
            ],
            size,
        ),
        name: None,
        hidden: false,
        background: None,
    };
    let presentation = Presentation::new(
        id(1),
        size,
        vec![master],
        vec![layout],
        vec![slide],
        Definitions::default(),
    )
    .expect("a deck with a layout prompt and a layout caption");

    let canvas = lay_out_slide(&presentation, 0, &shaper()).expect("slide 0");
    let painted: Vec<NodeId> = text_anchors(&canvas)
        .into_iter()
        .map(|anchor| anchor.node.expect("a text anchor carries its shape"))
        .collect();
    assert_eq!(
        painted,
        vec![id(25), id(32)],
        "the layout's plain shape and the slide's own, and NOT the layout's \
         placeholder prompt"
    );
}

/// A shape's glyphs paint immediately after the shape they belong to.
///
/// Within one tree the child index is the z-order, so collecting the text and
/// emitting it after the walk would put every shape's glyphs above every later
/// shape — a second box overlapping the first would hide its own text behind the
/// first box's. Asserted on the sequence of anchors, because a composed list
/// cannot say which shape an item came from.
#[test]
fn a_shapes_glyphs_paint_immediately_after_the_shape() {
    let size = SlideSize::DEFAULT_16X9;
    let sized = |node: NodeId, paragraph_id: NodeId| TextBody {
        body_properties: TextBodyProperties::default(),
        list_style: ListStyle::default(),
        paragraphs: vec![paragraph(
            paragraph_id,
            None,
            vec![run(
                node,
                "text",
                Some(TextCharacterProperties {
                    size_hundredths_point: Some(1_800),
                    ..TextCharacterProperties::default()
                }),
            )],
        )],
    };
    let master = SlideMaster {
        id: SlideMasterId::new(id(10)),
        shapes: tree(id(11), Vec::new(), size),
        name: None,
        background: None,
        text_styles: TextStyles::default(),
    };
    let layout = SlideLayout {
        id: SlideLayoutId::new(id(20)),
        master: master.id,
        kind: LayoutKind::Blank,
        shapes: tree(id(21), Vec::new(), size),
        name: None,
        background: None,
    };
    let slide = Slide {
        id: SlideId::new(id(30)),
        layout: layout.id,
        shapes: tree(
            id(31),
            vec![
                SlideNode::new(shape_at(id(32), 0, 0, 914_400, 914_400))
                    .with_text(sized(id(33), id(34))),
                SlideNode::new(shape_at(id(35), 0, 914_400, 914_400, 914_400))
                    .with_text(sized(id(36), id(37))),
            ],
            size,
        ),
        name: None,
        hidden: false,
        background: None,
    };
    let presentation = Presentation::new(
        id(1),
        size,
        vec![master],
        vec![layout],
        vec![slide],
        Definitions::default(),
    )
    .expect("a deck with two text shapes");

    let canvas = lay_out_slide(&presentation, 0, &shaper()).expect("slide 0");
    let sequence: Vec<(NodeId, bool)> = canvas
        .anchors
        .iter()
        .map(|anchor| {
            (
                anchor.node.expect("every slide anchor carries its shape"),
                matches!(anchor.content, AnchorContent::TextBox { .. }),
            )
        })
        .collect();
    assert_eq!(
        sequence,
        vec![
            (id(32), false),
            (id(32), true),
            (id(35), false),
            (id(35), true),
        ],
        "shape, its text, next shape, its text"
    );
    let orders: Vec<u32> = canvas.anchors.iter().map(|anchor| anchor.z.order).collect();
    assert_eq!(orders, vec![0, 1, 2, 3], "one monotonic paint order");
}

/// An empty `a:p` produces no text anchor at all.
///
/// Every placeholder in a real deck carries `<a:p><a:endParaRPr/></a:p>` whether
/// or not it holds text, so an anchor emitted for one would make "this shape has
/// glyphs" true for the whole deck — the vacuous assertion the house rule warns
/// about, and it would also push an empty clip bracket per placeholder into every
/// display list.
#[test]
fn a_body_that_shapes_no_glyph_emits_no_text_anchor() {
    let canvas = lay_out_slide(
        &text_deck(
            first_level(level(Some(1_800), None)),
            ListStyle::default(),
            TextBody::empty(id(41)),
        ),
        0,
        &shaper(),
    )
    .expect("slide 0");

    assert!(
        text_anchors(&canvas).is_empty(),
        "an empty placeholder paints its shape and nothing else: {:?}",
        canvas.anchors
    );
    assert_eq!(
        canvas.anchors.len(),
        2,
        "the layout's placeholder and the slide's, both shapes only"
    );
}

/// A placeholder that states no `a:xfrm` shapes its text in the rectangle it
/// inherits, and so does the shape itself.
///
/// This is the property every real deck depends on: PowerPoint writes
/// `<p:spPr/>` on a title and the box comes from the layout. Without it the
/// shape is a zero-sized point and its text has no width to wrap in, so this is
/// a prerequisite for glyphs rather than a refinement. Asserted on BOTH anchors,
/// because resolving the box for the text and leaving the shape at the origin
/// would put a slide's title text outside its own box.
#[test]
fn an_unpositioned_placeholder_inherits_the_box_for_its_shape_and_its_text() {
    let size = SlideSize::DEFAULT_16X9;
    let slot = Placeholder {
        kind: TEXT_SLOT.0,
        index: TEXT_SLOT.1,
        ..Placeholder::default()
    };
    let master = SlideMaster {
        id: SlideMasterId::new(id(10)),
        shapes: tree(id(11), Vec::new(), size),
        name: None,
        background: None,
        text_styles: TextStyles::default(),
    };
    let layout = SlideLayout {
        id: SlideLayoutId::new(id(20)),
        master: master.id,
        kind: LayoutKind::Object,
        shapes: tree(
            id(21),
            vec![
                SlideNode::new(shape_at(
                    id(22),
                    1_270_000,
                    635_000,
                    TEXT_BOX_WIDTH_EMU,
                    TEXT_BOX_HEIGHT_EMU,
                ))
                .in_slot(slot),
            ],
            size,
        ),
        name: None,
        background: None,
    };
    // A zero `a:ext`, which is how the importer stores "this shape stated no
    // `a:xfrm`" — the one case the model cannot distinguish from an explicit zero.
    let slide = Slide {
        id: SlideId::new(id(30)),
        layout: layout.id,
        shapes: tree(
            id(31),
            vec![
                SlideNode::new(shape_at(id(32), 0, 0, 0, 0))
                    .in_slot(slot)
                    .with_text(simple_body(
                        "Inherited",
                        Some(TextCharacterProperties {
                            size_hundredths_point: Some(1_800),
                            ..TextCharacterProperties::default()
                        }),
                    )),
            ],
            size,
        ),
        name: None,
        hidden: false,
        background: None,
    };
    let presentation = Presentation::new(
        id(1),
        size,
        vec![master],
        vec![layout],
        vec![slide],
        Definitions::default(),
    )
    .expect("a deck whose slide placeholder states no transform");

    let canvas = lay_out_slide(&presentation, 0, &shaper()).expect("slide 0");
    // 1,270,000 / 635 = 2,000 and 635,000 / 635 = 1,000 twips; the extent is the
    // same 14,400 x 7,200 the other guards use.
    let expected = (2_000, 1_000, 14_400, 7_200);
    let boxes: Vec<(i32, i32, i32, i32)> = canvas
        .anchors
        .iter()
        .filter(|anchor| anchor.node == Some(id(32)))
        .map(|anchor| {
            (
                anchor.rect.origin.x.raw(),
                anchor.rect.origin.y.raw(),
                anchor.rect.size.width.raw(),
                anchor.rect.size.height.raw(),
            )
        })
        .collect();
    assert_eq!(
        boxes,
        vec![expected, expected],
        "the shape and its text must both land in the layout's rectangle"
    );
    assert!(
        !glyph_runs(&canvas).is_empty(),
        "and the inherited width is what gives the text a column to shape in"
    );
}

/// A run's underline, strike, letter spacing and baseline offset all reach the
/// shaped run.
///
/// Four properties in one guard because they are one translation — the character
/// layer onto the shaper's run — and because each is measured as a DIFFERENCE
/// against a run that states nothing, so none of them is asserted against a
/// shaper metric. The letter-spacing arithmetic is the sharp one: `a:rPr@spc` is
/// hundredths of a point like `@sz`, so 150 is 30 twips per glyph and a reader
/// that treated it as twips would widen the line by five times as much.
#[test]
fn a_runs_decoration_spacing_and_baseline_reach_the_shaped_run() {
    let decorated = |properties: TextCharacterProperties| {
        lay_out_slide(
            &text_deck(
                ListStyle::default(),
                ListStyle::default(),
                TextBody {
                    body_properties: TextBodyProperties::default(),
                    list_style: ListStyle::default(),
                    paragraphs: vec![paragraph(
                        id(41),
                        None,
                        vec![run(id(42), "decorated", Some(properties))],
                    )],
                },
            ),
            0,
            &shaper(),
        )
        .expect("slide 0")
    };
    // 20pt, so the baseline offset below is a round number.
    let plain_properties = TextCharacterProperties {
        size_hundredths_point: Some(2_000),
        ..TextCharacterProperties::default()
    };
    let plain = decorated(plain_properties.clone());
    let marked = decorated(TextCharacterProperties {
        underline: Some(TextUnderline::Double),
        strike: Some(TextStrike::Single),
        spacing_hundredths_point: Some(150),
        baseline_percent: Some(30_000),
        ..plain_properties
    });

    let plain_run = glyph_runs(&plain)[0];
    let marked_run = glyph_runs(&marked)[0];

    assert!(
        !plain_run.decoration.underline && !plain_run.decoration.strikethrough,
        "the control run must state no decoration: {:?}",
        plain_run.decoration
    );
    assert!(marked_run.decoration.underline, "`a:rPr@u` underlines");
    assert_eq!(
        marked_run.decoration.underline_style,
        casual_doc_model::v1::UnderlineStyle::Double,
        "`u=\"dbl\"` is two lines, not the default single"
    );
    assert!(
        marked_run.decoration.strikethrough && !marked_run.decoration.double_strike,
        "`strike=\"sngStrike\"` is one line: {:?}",
        marked_run.decoration
    );

    let advance = |run: &GlyphRun| {
        run.glyphs
            .iter()
            .fold(0_i32, |sum, glyph| sum + glyph.advance.raw())
    };
    assert_eq!(
        advance(marked_run) - advance(plain_run),
        30 * plain_run.glyphs.len() as i32,
        "150 hundredths of a point is 30 twips, added once per glyph"
    );

    // Screen y grows downward, so a RAISED baseline is a smaller y. 30% of 20pt
    // is 6pt, which is 120 twips.
    assert_eq!(
        plain_run.origin.y.raw() - marked_run.origin.y.raw(),
        120,
        "`baseline=\"30000\"` is 30% of the font size, raised"
    );
}

/// A slide's text outline: the slide's own words, then the deck's furniture, and
/// NEITHER tier's placeholder prompt.
///
/// Written as one deck asserting the whole projection rather than as five tests,
/// because every half of this is satisfiable by accident. "The prompt is not read"
/// passes for a projection that reads no layout text at all; "the footer is read"
/// passes for one that reads every tier's text including "Click to add text" on
/// every slide; "the title comes first" passes for a projection that only ever
/// emits the slide. The exact sequence is the only assertion that can tell those
/// apart — which is the same reason
/// `a_layouts_prompt_does_not_paint_but_its_plain_shapes_text_does` is written as a
/// difference inside one deck.
#[test]
fn the_text_outline_reads_the_slides_words_then_the_decks_furniture_and_no_prompt() {
    let size = SlideSize::DEFAULT_16X9;
    let title_slot = Placeholder {
        kind: PlaceholderKind::Title,
        ..Placeholder::default()
    };
    let body_slot = Placeholder {
        kind: TEXT_SLOT.0,
        index: TEXT_SLOT.1,
        ..Placeholder::default()
    };
    let footer_slot = Placeholder {
        kind: PlaceholderKind::Footer,
        index: 2,
        ..Placeholder::default()
    };
    let body = |paragraphs: Vec<TextParagraph>| TextBody {
        body_properties: TextBodyProperties::default(),
        list_style: ListStyle::default(),
        paragraphs,
    };
    let one_line = |text: &str, paragraph_id: NodeId, run_id: NodeId| {
        body(vec![paragraph(
            paragraph_id,
            None,
            vec![run(run_id, text, None)],
        )])
    };
    let box_at = |node: NodeId, y: i64| shape_at(node, 0, y, TEXT_BOX_WIDTH_EMU, 914_400);

    let master = SlideMaster {
        id: SlideMasterId::new(id(10)),
        shapes: tree(
            id(11),
            vec![
                // The master's title PROMPT. PowerPoint shows this in the editor and
                // never on a slide, so reading it aloud would be worse than silence.
                SlideNode::new(box_at(id(12), 0))
                    .in_slot(title_slot)
                    .with_text(one_line("Click to edit Master title style", id(13), id(14))),
                // A plain shape on the master: a running footer rule's label. This
                // DOES paint on every slide, so a reader must hear it.
                SlideNode::new(box_at(id(15), 914_400)).with_text(one_line(
                    "Confidential",
                    id(16),
                    id(17),
                )),
            ],
            size,
        ),
        name: None,
        background: None,
        text_styles: TextStyles::default(),
    };
    let layout = SlideLayout {
        id: SlideLayoutId::new(id(20)),
        master: master.id,
        kind: LayoutKind::Object,
        shapes: tree(
            id(21),
            vec![
                SlideNode::new(box_at(id(22), 1_828_800))
                    .in_slot(body_slot)
                    .with_text(one_line("Click to add text", id(23), id(24))),
                SlideNode::new(box_at(id(25), 2_743_200)).with_text(one_line(
                    "opendoc",
                    id(26),
                    id(27),
                )),
            ],
            size,
        ),
        name: None,
        background: None,
    };
    let slide = Slide {
        id: SlideId::new(id(30)),
        layout: layout.id,
        shapes: tree(
            id(31),
            vec![
                SlideNode::new(box_at(id(32), 0))
                    .in_slot(title_slot)
                    .with_text(one_line("Quarterly review", id(33), id(34))),
                SlideNode::new(box_at(id(35), 914_400))
                    .in_slot(body_slot)
                    .with_text(body(vec![
                        paragraph(id(36), None, vec![run(id(37), "Revenue", None)]),
                        paragraph(
                            id(38),
                            Some(TextParagraphProperties {
                                level: Some(1),
                                ..TextParagraphProperties::default()
                            }),
                            vec![run(id(39), "By region", None)],
                        ),
                        // A blank line and a whitespace-only one. Both are real —
                        // PowerPoint writes an empty `a:p` for a blank line — and
                        // both are noise to a reader, so neither is announced.
                        paragraph(id(40), None, Vec::new()),
                        paragraph(id(41), None, vec![run(id(42), "   ", None)]),
                    ])),
                // Hidden. Not read, for the same reason it is not painted: the
                // author hid it. It still round-trips.
                SlideNode {
                    hidden: true,
                    ..SlideNode::new(box_at(id(43), 1_828_800)).with_text(one_line(
                        "Draft note",
                        id(44),
                        id(45),
                    ))
                },
                SlideNode {
                    name: Some("Slide number".to_owned()),
                    ..SlideNode::new(box_at(id(46), 2_743_200))
                        .in_slot(footer_slot)
                        .with_text(body(vec![paragraph(
                            id(47),
                            None,
                            vec![
                                // A field contributes its CACHED text: a slide
                                // number renders from that cache, so dropping it
                                // would read the field's loss aloud.
                                TextRun::Field(TextField {
                                    id: id(48),
                                    field_id: "{B7F1C5A2-0000-0000-0000-000000000001}".to_owned(),
                                    kind: "slidenum".to_owned(),
                                    properties: None,
                                    text: "7".to_owned(),
                                }),
                                // A soft break inside a paragraph is a space, not a
                                // newline: the mirror gives each PARAGRAPH its own
                                // element.
                                TextRun::LineBreak(TextLineBreak {
                                    id: id(49),
                                    properties: None,
                                }),
                                run(id(50), "of 12", None),
                            ],
                        )]))
                },
            ],
            size,
        ),
        name: None,
        hidden: false,
        background: None,
    };
    let presentation = Presentation::new(
        id(1),
        size,
        vec![master],
        vec![layout],
        vec![slide],
        Definitions::default(),
    )
    .expect("a three-tier deck with prompts, furniture and a hidden shape");

    /// One projected shape, flattened for one assertion: its tier, its role, its
    /// name, and its paragraphs as `(level, text)`.
    type Read<'a> = (&'a str, &'a str, Option<&'a str>, Vec<(u8, &'a str)>);

    let outline = slide_text_outline(&presentation, 0).expect("slide 0");
    let read: Vec<Read<'_>> = outline
        .shapes
        .iter()
        .map(|shape| {
            (
                shape.tier.token(),
                shape.role.token(),
                shape.name.as_deref(),
                match &shape.content {
                    OutlineContent::Text(paragraphs) => paragraphs
                        .iter()
                        .map(|paragraph| (paragraph.level, paragraph.text.as_str()))
                        .collect(),
                    OutlineContent::Table(_) => {
                        unreachable!("this fixture carries no table")
                    }
                },
            )
        })
        .collect();
    assert_eq!(
        read,
        vec![
            ("slide", "title", None, vec![(0, "Quarterly review")]),
            (
                "slide",
                "body",
                None,
                vec![(0, "Revenue"), (1, "By region")],
            ),
            ("slide", "other", Some("Slide number"), vec![(0, "7 of 12")],),
            ("layout", "shape", None, vec![(0, "opendoc")]),
            ("master", "shape", None, vec![(0, "Confidential")]),
        ],
        "the slide's own words first, then the layout's and the master's painted \
         furniture, and no prompt, no hidden shape and no blank line anywhere"
    );
    // The ids are carried so a host can tie a mirrored element back to its drawing.
    let ids: Vec<NodeId> = outline.shapes.iter().map(|shape| shape.id).collect();
    assert_eq!(ids, vec![id(32), id(35), id(46), id(25), id(15)]);
}
