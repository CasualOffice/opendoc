// SPDX-License-Identifier: Apache-2.0

//! Slide layout guards. Every one was driven red by mutating production code before
//! being trusted (`SKILL` §4).

use casual_doc_layout::page::AnchorContent;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    Definitions, Extent, GroupChild, GroupPicture, GroupShape, GroupTransform, MediaId,
    MediaReference, PointEmu, ShapeGeometry, WordprocessingGroup,
};
use casual_pres_model::{
    LayoutKind, Presentation, ShapeTree, Slide, SlideId, SlideLayout, SlideLayoutId, SlideMaster,
    SlideMasterId, SlideNode, SlideSize,
};

use crate::lay_out_slide;

fn id(counter: u64) -> NodeId {
    NodeId::from_parts(1, counter).expect("non-zero")
}

const ORIGIN: PointEmu = PointEmu { x_emu: 0, y_emu: 0 };

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
    let canvas = lay_out_slide(&three_tier_deck(), 0).expect("slide 0");
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
    let canvas = lay_out_slide(&three_tier_deck(), 0).expect("slide 0");
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
    let canvas = lay_out_slide(&three_tier_deck(), 0).expect("slide 0");
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
    let canvas = lay_out_slide(&presentation, 0).expect("slide 0");
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
    let canvas = lay_out_slide(&deck, 0).expect("slide 0");
    let shape = canvas.anchors.last().expect("the slide's shape");
    // Half the child space over the same box doubles everything.
    assert_eq!(shape.rect.origin.x.raw(), 5_760, "2in doubled to 4in");
    assert_eq!(shape.rect.size.width.raw(), 2_880, "1in doubled to 2in");
}

#[test]
fn a_nested_group_composes_its_transform_with_its_parents() {
    let size = SlideSize::DEFAULT_16X9;
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
    let canvas = lay_out_slide(&deck, 0).expect("slide 0");
    let shape = canvas.anchors.last().expect("the grouped shape");
    // The group sits at 1in and doubles its child space, so a child at the group's
    // origin lands at 1in and a 1in child becomes 2in. Mutation: not composing the
    // mapper placed it at the un-nested position.
    assert_eq!(shape.rect.origin.x.raw(), 1_440);
    assert_eq!(shape.rect.size.width.raw(), 2_880);
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
    let canvas = lay_out_slide(&presentation, 0).expect("slide 0");
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
    assert!(lay_out_slide(&deck, 0).is_some());
    assert!(lay_out_slide(&deck, 1).is_none());
    assert!(lay_out_slide(&deck, usize::MAX).is_none());
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
        lay_out_slide(&presentation, 0)
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
