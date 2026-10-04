// SPDX-License-Identifier: Apache-2.0

//! Presentation model guards.
//!
//! Every guard here was driven red by mutating production code before being
//! trusted; the mutation is named beside the assertion it protects, because a guard
//! that cannot fail is worse than no guard (`SKILL` §4).

use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    Definitions, Extent, GroupChild, GroupPicture, GroupShape, GroupTransform, MAX_GROUP_DEPTH,
    MediaId, MediaReference, PointEmu, ShapeGeometry, WordprocessingGroup,
};

use crate::{
    LayoutKind, MAX_SLIDE_EMU, MIN_SLIDE_EMU, Placeholder, PlaceholderKind, PlaceholderOrientation,
    PlaceholderSize, Presentation, PresentationError, SCHEMA_VERSION, ShapeTree, Slide, SlideAxis,
    SlideId, SlideLayout, SlideLayoutId, SlideMaster, SlideMasterId, SlideNode, SlideSize,
    SlideSizeKind,
};

/// A node id from a small counter, so a fixture reads as `id(7)`.
fn id(counter: u64) -> NodeId {
    NodeId::from_parts(1, counter).expect("non-zero")
}

const BOX: Extent = Extent {
    width_emu: 914_400,
    height_emu: 914_400,
};
const ORIGIN: PointEmu = PointEmu { x_emu: 0, y_emu: 0 };

/// A minimal preset shape. Written out field by field rather than defaulted: a
/// fixture that reaches for `Default` satisfies the compiler while asserting
/// nothing about the fields it skipped (`SKILL` §5a).
fn shape(node: NodeId) -> GroupChild {
    GroupChild::Shape(GroupShape {
        id: node,
        offset: ORIGIN,
        extent: BOX,
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

/// A picture naming `media`, so a dangling reference is expressible.
fn picture(node: NodeId, media: MediaId) -> GroupChild {
    GroupChild::Picture(GroupPicture {
        id: node,
        media,
        offset: ORIGIN,
        extent: BOX,
        descr: None,
        crop: None,
        opacity: None,
        hyperlink: None,
        border: None,
        flip_h: false,
        flip_v: false,
        rotation: None,
    })
}

fn transform() -> GroupTransform {
    GroupTransform {
        offset: ORIGIN,
        extent: BOX,
        child_offset: ORIGIN,
        child_extent: BOX,
        flip_h: false,
        flip_v: false,
        rotation: None,
    }
}

/// A nested group holding `children`.
fn group(node: NodeId, children: Vec<GroupChild>) -> GroupChild {
    GroupChild::Group(Box::new(WordprocessingGroup {
        id: node,
        anchor: None,
        relative_height: None,
        extent: BOX,
        transform: transform(),
        hyperlink: None,
        children,
    }))
}

fn tree(node: NodeId, children: Vec<SlideNode>) -> ShapeTree {
    ShapeTree {
        id: node,
        transform: transform(),
        children,
    }
}

/// One master, one layout, one slide, each with one shape — the smallest deck that
/// exercises all three tiers.
fn deck() -> Presentation {
    let master = SlideMaster {
        id: SlideMasterId::new(id(10)),
        shapes: tree(id(11), vec![SlideNode::new(shape(id(12)))]),
        name: None,
        background: None,
    };
    let layout = SlideLayout {
        id: SlideLayoutId::new(id(20)),
        master: master.id,
        kind: LayoutKind::Object,
        shapes: tree(id(21), vec![SlideNode::new(shape(id(22)))]),
        name: Some("Title and Content".to_owned()),
        background: None,
    };
    let slide = Slide {
        id: SlideId::new(id(30)),
        layout: layout.id,
        shapes: tree(id(31), vec![SlideNode::new(shape(id(32)))]),
        name: None,
        hidden: false,
        background: None,
    };
    Presentation::new(
        id(1),
        SlideSize::DEFAULT_16X9,
        vec![master],
        vec![layout],
        vec![slide],
        Definitions::default(),
    )
    .expect("the smallest valid deck")
}

#[test]
fn the_smallest_deck_validates_and_stamps_the_schema_version() {
    let presentation = deck();
    assert_eq!(presentation.schema_version(), SCHEMA_VERSION);
    assert_eq!(presentation.slides().len(), 1);
    assert_eq!(presentation.slide_size(), SlideSize::DEFAULT_16X9);
}

// ---------------------------------------------------------------------------
// Slide size
// ---------------------------------------------------------------------------

#[test]
fn a_slide_narrower_than_the_schema_permits_is_refused() {
    // Mutation that drove this red: widening the lower bound to `0`.
    let error = SlideSize {
        width_emu: MIN_SLIDE_EMU - 1,
        height_emu: 6_858_000,
        kind: SlideSizeKind::Custom,
    }
    .validate()
    .expect_err("below ST_SlideSizeCoordinate");
    assert_eq!(
        error,
        PresentationError::SlideSizeOutOfDomain {
            axis: SlideAxis::Width,
            value: MIN_SLIDE_EMU - 1,
        }
    );
}

#[test]
fn a_slide_taller_than_the_schema_permits_is_refused_on_the_height_axis() {
    // The axis matters: an earlier draft charged both failures to `Width`, which
    // reported the wrong dimension and no count-based guard could notice.
    let error = SlideSize {
        width_emu: 12_192_000,
        height_emu: MAX_SLIDE_EMU + 1,
        kind: SlideSizeKind::Custom,
    }
    .validate()
    .expect_err("above ST_SlideSizeCoordinate");
    assert_eq!(
        error,
        PresentationError::SlideSizeOutOfDomain {
            axis: SlideAxis::Height,
            value: MAX_SLIDE_EMU + 1,
        }
    );
}

#[test]
fn both_defaults_are_inside_the_schema_domain() {
    SlideSize::DEFAULT_16X9.validate().expect("16:9 is legal");
    SlideSize::DEFAULT_4X3.validate().expect("4:3 is legal");
    // 13.333in x 7.5in and 10in x 7.5in, the two surfaces PowerPoint authors.
    assert_eq!(SlideSize::DEFAULT_16X9.width_emu, 12_192_000);
    assert_eq!(SlideSize::DEFAULT_4X3.width_emu, 9_144_000);
    assert_eq!(
        SlideSize::DEFAULT_16X9.height_emu,
        SlideSize::DEFAULT_4X3.height_emu,
        "both are 7.5in tall; only the width changes"
    );
}

// ---------------------------------------------------------------------------
// Token tables. Counts are DERIVED from `ALL`, never written twice.
// ---------------------------------------------------------------------------

#[test]
fn every_slide_size_token_round_trips_and_the_set_is_closed() {
    for kind in SlideSizeKind::ALL {
        assert_eq!(
            SlideSizeKind::from_token(kind.token()),
            kind,
            "{} did not round-trip",
            kind.token()
        );
    }
    let distinct: std::collections::BTreeSet<&str> =
        SlideSizeKind::ALL.iter().map(|kind| kind.token()).collect();
    assert_eq!(
        distinct.len(),
        SlideSizeKind::ALL.len(),
        "two kinds share a token, so one can never be read back"
    );
}

#[test]
fn an_unknown_slide_size_token_keeps_the_dimensions_rather_than_failing() {
    // `wideScreen` is what ONLYOFFICE emits; it is not one of
    // ST_SlideSizeType's values, and the dimensions carry the real geometry.
    assert_eq!(
        SlideSizeKind::from_token("wideScreen"),
        SlideSizeKind::Custom
    );
    assert_eq!(SlideSizeKind::from_token(""), SlideSizeKind::Custom);
}

#[test]
fn every_layout_kind_round_trips_and_the_set_is_closed() {
    for kind in LayoutKind::ALL {
        assert_eq!(
            LayoutKind::from_token(kind.token()),
            kind,
            "{} did not round-trip",
            kind.token()
        );
    }
    let distinct: std::collections::BTreeSet<&str> =
        LayoutKind::ALL.iter().map(|kind| kind.token()).collect();
    assert_eq!(distinct.len(), LayoutKind::ALL.len());
    assert_eq!(LayoutKind::from_token("notALayout"), LayoutKind::Custom);
}

#[test]
fn every_placeholder_kind_round_trips_and_the_set_is_closed() {
    for kind in PlaceholderKind::ALL {
        assert_eq!(
            PlaceholderKind::from_token(kind.token()),
            kind,
            "{} did not round-trip",
            kind.token()
        );
    }
    let distinct: std::collections::BTreeSet<&str> = PlaceholderKind::ALL
        .iter()
        .map(|kind| kind.token())
        .collect();
    assert_eq!(distinct.len(), PlaceholderKind::ALL.len());
    // An attribute-less `p:ph` is `obj`, which is the schema's own default.
    assert_eq!(PlaceholderKind::from_token(""), PlaceholderKind::Object);
    assert_eq!(PlaceholderKind::default(), PlaceholderKind::Object);
}

#[test]
fn exactly_the_two_title_tokens_are_titles() {
    let titles: Vec<&str> = PlaceholderKind::ALL
        .iter()
        .filter(|kind| kind.is_title())
        .map(|kind| kind.token())
        .collect();
    // Mutation that drove this red: making `is_title` match `Title` only, which
    // let a `ctrTitle` + `title` pair past the duplicate-title guard.
    assert_eq!(titles, vec!["title", "ctrTitle"]);
}

#[test]
fn the_placeholder_defaults_match_the_schema_defaults() {
    let placeholder = Placeholder::default();
    assert_eq!(placeholder.kind, PlaceholderKind::Object);
    assert_eq!(placeholder.index, 0);
    assert_eq!(placeholder.size, PlaceholderSize::Full);
    assert_eq!(placeholder.orientation, PlaceholderOrientation::Horizontal);
    assert!(!placeholder.has_custom_prompt);
    assert_eq!(
        PlaceholderSize::from_token("quarter"),
        PlaceholderSize::Quarter
    );
    assert_eq!(PlaceholderSize::from_token("other"), PlaceholderSize::Full);
    assert_eq!(
        PlaceholderOrientation::from_token("vert"),
        PlaceholderOrientation::Vertical
    );
    assert_eq!(
        PlaceholderOrientation::from_token("other"),
        PlaceholderOrientation::Horizontal
    );
}

// ---------------------------------------------------------------------------
// Envelope invariants
// ---------------------------------------------------------------------------

#[test]
fn a_presentation_with_no_slides_is_refused() {
    // PowerPoint cannot open a package with an empty `p:sldIdLst` — there is no
    // first slide — so this is refused at construction, not written and found
    // unopenable. Mutation: dropping the `slides.is_empty()` check.
    let error = Presentation::new(
        id(1),
        SlideSize::DEFAULT_16X9,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Definitions::default(),
    )
    .expect_err("an empty deck");
    assert_eq!(error, PresentationError::EmptyPresentation);
}

#[test]
fn a_slide_naming_a_layout_that_does_not_exist_is_refused() {
    let mut presentation = deck();
    let absent = SlideLayoutId::new(id(900));
    presentation.slides_mut()[0].layout = absent;
    let error = presentation.validate().expect_err("dangling layout");
    assert_eq!(
        error,
        PresentationError::DanglingLayoutRef(SlideId::new(id(30))),
        "the error names the SLIDE, which is the thing the author can fix"
    );
}

#[test]
fn a_layout_naming_a_master_that_does_not_exist_is_refused() {
    let master = SlideMaster {
        id: SlideMasterId::new(id(10)),
        shapes: tree(id(11), Vec::new()),
        name: None,
        background: None,
    };
    let layout = SlideLayout {
        id: SlideLayoutId::new(id(20)),
        master: SlideMasterId::new(id(901)),
        kind: LayoutKind::Blank,
        shapes: tree(id(21), Vec::new()),
        name: None,
        background: None,
    };
    let slide = Slide {
        id: SlideId::new(id(30)),
        layout: layout.id,
        shapes: tree(id(31), Vec::new()),
        name: None,
        hidden: false,
        background: None,
    };
    let error = Presentation::new(
        id(1),
        SlideSize::DEFAULT_16X9,
        vec![master],
        vec![layout],
        vec![slide],
        Definitions::default(),
    )
    .expect_err("dangling master");
    assert_eq!(
        error,
        PresentationError::DanglingMasterRef(SlideLayoutId::new(id(20)))
    );
}

#[test]
fn a_node_id_reused_across_two_tiers_is_refused() {
    // The payoff of sharing the document model's traversals: a slide shape and a
    // layout shape are walked by the same code, so a collision between them cannot
    // hide. Mutation: dropping `validate_unique_ids`.
    let mut presentation = deck();
    presentation.slides_mut()[0].shapes.children[0] = SlideNode::new(shape(id(22)));
    let error = presentation.validate().expect_err("a reused id");
    assert_eq!(error, PresentationError::DuplicateNodeId(id(22)));
}

#[test]
fn a_shape_id_colliding_with_a_media_id_is_refused() {
    // The sharper case, and the reason the definitions walk had to be the SAME
    // enumeration rather than a second copy: media ids and shape ids are both
    // `NodeId`, so a presentation that walked only its own parts would miss this.
    let mut presentation = deck();
    presentation.definitions_mut().media.insert(
        MediaId::new(id(32)),
        MediaReference {
            relationship_id: "rId1".to_owned(),
            media_type: "image/png".to_owned(),
            part_name: "/ppt/media/image1.png".to_owned(),
        },
    );
    let error = presentation.validate().expect_err("shape id is a media id");
    assert_eq!(error, PresentationError::DuplicateNodeId(id(32)));
}

// ---------------------------------------------------------------------------
// Placeholder slots
// ---------------------------------------------------------------------------

#[test]
fn two_shapes_in_one_slot_are_refused_but_two_indices_are_not() {
    let body = |index: u32| Placeholder {
        kind: PlaceholderKind::Body,
        index,
        size: PlaceholderSize::Full,
        orientation: PlaceholderOrientation::Horizontal,
        has_custom_prompt: false,
    };
    // Two `body` slots distinguished only by `idx` is what every two-content
    // layout carries, so this must be ACCEPTED.
    let two_columns = tree(
        id(40),
        vec![
            SlideNode::new(shape(id(41))).in_slot(body(1)),
            SlideNode::new(shape(id(42))).in_slot(body(2)),
        ],
    );
    two_columns
        .validate(&Definitions::default())
        .expect("two body slots at different indices are legal");

    // The same index twice is not: inheritance resolves by (type, idx) and there
    // is no rule that picks a winner. Mutation: keying `seen` on `kind` alone made
    // the legal case above fail, and keying it on `index` alone made this pass.
    let error = tree(
        id(40),
        vec![
            SlideNode::new(shape(id(41))).in_slot(body(1)),
            SlideNode::new(shape(id(42))).in_slot(body(1)),
        ],
    )
    .validate(&Definitions::default())
    .expect_err("one slot, two shapes");
    assert_eq!(
        error,
        PresentationError::DuplicatePlaceholder {
            tree: id(40),
            kind: PlaceholderKind::Body,
            index: 1,
        }
    );
}

#[test]
fn a_title_and_a_centered_title_together_are_refused() {
    let slot = |kind: PlaceholderKind| Placeholder {
        kind,
        index: 0,
        size: PlaceholderSize::Full,
        orientation: PlaceholderOrientation::Horizontal,
        has_custom_prompt: false,
    };
    // Distinct slots — different `kind`, same `idx` — so the duplicate-slot guard
    // does NOT catch this, which is exactly why the title rule is separate.
    let error = tree(
        id(50),
        vec![
            SlideNode::new(shape(id(51))).in_slot(slot(PlaceholderKind::Title)),
            SlideNode::new(shape(id(52))).in_slot(slot(PlaceholderKind::CtrTitle)),
        ],
    )
    .validate(&Definitions::default())
    .expect_err("two titles");
    assert_eq!(error, PresentationError::DuplicateTitlePlaceholder(id(50)));
}

#[test]
fn the_title_lookup_finds_either_title_token() {
    for kind in [PlaceholderKind::Title, PlaceholderKind::CtrTitle] {
        let shapes = tree(
            id(60),
            vec![
                SlideNode::new(shape(id(61))),
                SlideNode::new(shape(id(62))).in_slot(Placeholder {
                    kind,
                    index: 0,
                    size: PlaceholderSize::Full,
                    orientation: PlaceholderOrientation::Horizontal,
                    has_custom_prompt: false,
                }),
            ],
        );
        assert_eq!(
            shapes.title().map(SlideNode::id),
            Some(id(62)),
            "{} was not found as a title",
            kind.token()
        );
    }
}

#[test]
fn the_slot_index_lists_only_placeholder_shapes() {
    let shapes = tree(
        id(70),
        vec![
            SlideNode::new(shape(id(71))),
            SlideNode::new(shape(id(72))).in_slot(Placeholder {
                kind: PlaceholderKind::Title,
                index: 0,
                size: PlaceholderSize::Full,
                orientation: PlaceholderOrientation::Horizontal,
                has_custom_prompt: false,
            }),
        ],
    );
    let slots = shapes.slots();
    assert_eq!(slots.len(), 1, "the non-placeholder shape is not a slot");
    assert_eq!(slots[&(PlaceholderKind::Title, 0)], id(72));
}

// ---------------------------------------------------------------------------
// The inheritance cascade
// ---------------------------------------------------------------------------

#[test]
fn a_slot_resolves_through_slide_then_layout_then_master() {
    let slot = Placeholder {
        kind: PlaceholderKind::Title,
        index: 0,
        size: PlaceholderSize::Full,
        orientation: PlaceholderOrientation::Horizontal,
        has_custom_prompt: false,
    };
    let master = SlideMaster {
        id: SlideMasterId::new(id(10)),
        shapes: tree(id(11), vec![SlideNode::new(shape(id(12))).in_slot(slot)]),
        name: None,
        background: None,
    };
    let layout = SlideLayout {
        id: SlideLayoutId::new(id(20)),
        master: master.id,
        kind: LayoutKind::Title,
        shapes: tree(id(21), vec![SlideNode::new(shape(id(22))).in_slot(slot)]),
        name: None,
        background: None,
    };
    let bare = Slide {
        id: SlideId::new(id(30)),
        layout: layout.id,
        shapes: tree(id(31), Vec::new()),
        name: None,
        hidden: false,
        background: None,
    };
    let filled = Slide {
        id: SlideId::new(id(40)),
        layout: layout.id,
        shapes: tree(id(41), vec![SlideNode::new(shape(id(42))).in_slot(slot)]),
        name: None,
        hidden: false,
        background: None,
    };
    let presentation = Presentation::new(
        id(1),
        SlideSize::DEFAULT_16X9,
        vec![master],
        vec![layout],
        vec![bare.clone(), filled.clone()],
        Definitions::default(),
    )
    .expect("a three-tier deck");

    // The slide's own shape wins when it has one.
    assert_eq!(
        presentation
            .resolve_slot(&filled, PlaceholderKind::Title, 0)
            .map(SlideNode::id),
        Some(id(42))
    );
    // Otherwise the layout's. Mutation: returning `None` instead of descending to
    // the layout — which is the defect that makes a real deck render with every
    // title missing, because a title shape on a slide usually carries only text.
    assert_eq!(
        presentation
            .resolve_slot(&bare, PlaceholderKind::Title, 0)
            .map(SlideNode::id),
        Some(id(22))
    );
    // And a slot neither the slide nor the layout fills falls through to the
    // master. Built as a second deck whose LAYOUT carries no title shape, rather
    // than asserting the layout's id, so the master tier is actually reached
    // instead of being inferred from the first assertion passing.
    let bare_layout = SlideLayout {
        id: SlideLayoutId::new(id(20)),
        master: SlideMasterId::new(id(10)),
        kind: LayoutKind::Title,
        shapes: tree(id(21), Vec::new()),
        name: None,
        background: None,
    };
    let master_only = Presentation::new(
        id(1),
        SlideSize::DEFAULT_16X9,
        vec![SlideMaster {
            id: SlideMasterId::new(id(10)),
            shapes: tree(id(11), vec![SlideNode::new(shape(id(12))).in_slot(slot)]),
            name: None,
            background: None,
        }],
        vec![bare_layout],
        vec![bare.clone()],
        Definitions::default(),
    )
    .expect("a deck whose layout fills no slot");
    assert_eq!(
        master_only
            .resolve_slot(&bare, PlaceholderKind::Title, 0)
            .map(SlideNode::id),
        Some(id(12)),
        "the cascade stopped before the master"
    );
    // A slot nobody fills resolves to nothing rather than to the wrong shape.
    assert!(
        presentation
            .resolve_slot(&bare, PlaceholderKind::Chart, 7)
            .is_none()
    );
}

#[test]
fn layout_indices_resolve_in_one_pass_and_agree_with_the_scan() {
    let presentation = deck();
    let bulk = presentation.resolve_layout_indices();
    let naive: Vec<Option<usize>> = presentation
        .slides()
        .iter()
        .map(|slide| {
            presentation
                .layouts()
                .iter()
                .position(|layout| layout.id == slide.layout)
        })
        .collect();
    assert_eq!(bulk, naive, "the one-pass resolution disagrees with a scan");
    assert_eq!(bulk, vec![Some(0)]);
}

// ---------------------------------------------------------------------------
// Shape trees
// ---------------------------------------------------------------------------

#[test]
fn a_tree_with_children_and_a_zero_child_extent_is_refused() {
    // Children map from the child space into the box by ratio, so a zero child
    // extent collapses every child to a point. Mutation: dropping the check made a
    // whole slide of zero-size shapes validate and paint as nothing.
    let mut shapes = tree(id(80), vec![SlideNode::new(shape(id(81)))]);
    shapes.transform.child_extent.width_emu = 0;
    assert_eq!(
        shapes
            .validate(&Definitions::default())
            .expect_err("degenerate child space"),
        PresentationError::DegenerateChildSpace(id(80))
    );
}

#[test]
fn an_empty_tree_with_a_zero_child_extent_is_accepted() {
    // Nothing to collapse, and a blank master legitimately carries one — so the
    // refusal above must be conditional on having children, not unconditional.
    let mut shapes = tree(id(80), Vec::new());
    shapes.transform.child_extent.width_emu = 0;
    shapes.transform.child_extent.height_emu = 0;
    shapes
        .validate(&Definitions::default())
        .expect("an empty tree collapses nothing");
}

#[test]
fn a_picture_naming_media_that_does_not_exist_is_refused_at_any_depth() {
    let absent = MediaId::new(id(999));
    // Top level.
    assert_eq!(
        tree(id(90), vec![SlideNode::new(picture(id(91), absent))])
            .validate(&Definitions::default())
            .expect_err("dangling media"),
        PresentationError::DanglingMediaRef(id(999))
    );
    // And inside a nested group, which is what proves the recursion runs rather
    // than only the top level being checked. Mutation: not recursing into
    // `GroupChild::Group` left this green while the shape was unpaintable.
    let nested = tree(
        id(90),
        vec![SlideNode::new(group(
            id(92),
            vec![group(id(93), vec![picture(id(94), absent)])],
        ))],
    );
    assert_eq!(
        nested
            .validate(&Definitions::default())
            .expect_err("dangling media two groups down"),
        PresentationError::DanglingMediaRef(id(999))
    );
}

#[test]
fn a_resolvable_media_reference_is_accepted() {
    let mut definitions = Definitions::default();
    let media = MediaId::new(id(999));
    definitions.media.insert(
        media,
        MediaReference {
            relationship_id: "rId1".to_owned(),
            media_type: "image/png".to_owned(),
            part_name: "/ppt/media/image1.png".to_owned(),
        },
    );
    tree(id(90), vec![SlideNode::new(picture(id(91), media))])
        .validate(&definitions)
        .expect("the media entry resolves");
}

#[test]
fn a_group_nested_past_the_bound_is_refused_and_one_at_the_bound_is_not() {
    // Built by derivation from `MAX_GROUP_DEPTH` rather than a literal depth, so
    // the guard cannot drift from the constant it is about.
    let build = |depth: u32| {
        let mut child = shape(id(100));
        for level in 0..depth {
            child = group(id(200 + u64::from(level)), vec![child]);
        }
        tree(id(99), vec![SlideNode::new(child)])
    };
    build(MAX_GROUP_DEPTH)
        .validate(&Definitions::default())
        .expect("exactly at the bound is legal");
    let error = build(MAX_GROUP_DEPTH + 1)
        .validate(&Definitions::default())
        .expect_err("one past the bound");
    assert!(
        matches!(error, PresentationError::GroupNestingTooDeep(_)),
        "expected a nesting refusal, got {error}"
    );
}

#[test]
fn a_shape_tree_reports_its_own_id_and_every_descendant_id() {
    let shapes = tree(
        id(110),
        vec![SlideNode::new(group(
            id(111),
            vec![shape(id(112)), group(id(113), vec![shape(id(114))])],
        ))],
    );
    let mut ids = Vec::new();
    shapes
        .visit_node_ids(&mut |node| {
            ids.push(node);
            Ok(())
        })
        .expect("a total walk");
    // Mutation: omitting `visit(self.id)` dropped 110, and not recursing dropped
    // 113 and 114 — both of which would hide a duplicate id from validation.
    assert_eq!(
        ids,
        vec![id(110), id(111), id(112), id(113), id(114)],
        "the walk missed a node or visited one twice"
    );
}

// ---------------------------------------------------------------------------
// Complexity. Guarded by counting work, not by a clock (`SKILL` §8).
// ---------------------------------------------------------------------------

#[test]
fn the_id_walk_is_linear_in_the_deck_rather_than_quadratic() {
    /// Builds a deck of `slides` slides over one master and one layout, counting
    /// the visits the id walk makes.
    fn visits(slides: u64) -> usize {
        let master = SlideMaster {
            id: SlideMasterId::new(id(10)),
            shapes: tree(id(11), Vec::new()),
            name: None,
            background: None,
        };
        let layout = SlideLayout {
            id: SlideLayoutId::new(id(20)),
            master: master.id,
            kind: LayoutKind::Blank,
            shapes: tree(id(21), Vec::new()),
            name: None,
            background: None,
        };
        let deck: Vec<Slide> = (0..slides)
            .map(|index| Slide {
                id: SlideId::new(id(1_000 + index * 10)),
                layout: layout.id,
                shapes: tree(
                    id(1_001 + index * 10),
                    vec![SlideNode::new(shape(id(1_002 + index * 10)))],
                ),
                name: None,
                hidden: false,
                background: None,
            })
            .collect();
        let presentation = Presentation::new(
            id(1),
            SlideSize::DEFAULT_16X9,
            vec![master],
            vec![layout],
            deck,
            Definitions::default(),
        )
        .expect("a valid deck");
        let mut count = 0_usize;
        presentation
            .visit_node_ids(&mut |_| {
                count += 1;
                Ok(())
            })
            .expect("a total walk");
        count
    }

    let (n, two_n) = (visits(64), visits(128));
    // Three visits per slide plus a fixed five for the envelope, master and
    // layout, so doubling the deck doubles the work. A quadratic walk would give a
    // ratio near four; a timing threshold could not tell the two apart.
    assert_eq!(n, 64 * 3 + 5);
    assert_eq!(two_n, 128 * 3 + 5);
    assert!(
        two_n < n * 3,
        "the walk grew faster than linearly: {n} -> {two_n}"
    );
}

// ---------------------------------------------------------------------------
// Serialization
// ---------------------------------------------------------------------------

#[test]
fn a_deck_round_trips_through_json() {
    let presentation = deck();
    let json = serde_json::to_string(&presentation).expect("serializable");
    let parsed: Presentation = serde_json::from_str(&json).expect("deserializable");
    assert_eq!(parsed, presentation);
    parsed.validate().expect("a round trip stays valid");
}

#[test]
fn absent_optional_fields_are_omitted_rather_than_written_as_null() {
    // A deck whose every optional field is unset must not carry them, or every
    // snapshot grows and a later addition cannot be byte-compared.
    let json = serde_json::to_value(deck()).expect("serializable");
    let slide = &json["slides"][0];
    for omitted in ["name", "hidden", "background"] {
        assert!(
            slide.get(omitted).is_none(),
            "slide.{omitted} should be omitted when unset, got {slide}"
        );
    }
    let node = &json["slides"][0]["shapes"]["children"][0];
    for omitted in ["placeholder", "name", "hidden"] {
        assert!(
            node.get(omitted).is_none(),
            "shape.{omitted} should be omitted when unset, got {node}"
        );
    }
}

#[test]
fn an_unknown_field_is_refused_rather_than_ignored() {
    // `deny_unknown_fields` is what makes a snapshot from a newer build fail loudly
    // instead of silently losing whatever it carried.
    let json = serde_json::to_string(&deck()).expect("serializable");
    let tampered = json.replace("\"slides\":", "\"slidez\":");
    serde_json::from_str::<Presentation>(&tampered).expect_err("an unknown field must be refused");
}

#[test]
fn a_placeholder_serializes_only_what_departs_from_the_schema_default() {
    let json = serde_json::to_value(Placeholder::default()).expect("serializable");
    assert_eq!(
        json,
        serde_json::json!({ "kind": "object" }),
        "only the kind is written for an all-default `p:ph`"
    );
    let explicit = serde_json::to_value(Placeholder {
        kind: PlaceholderKind::Body,
        index: 2,
        size: PlaceholderSize::Half,
        orientation: PlaceholderOrientation::Vertical,
        has_custom_prompt: true,
    })
    .expect("serializable");
    assert_eq!(
        explicit,
        serde_json::json!({
            "kind": "body",
            "index": 2,
            "size": "half",
            "orientation": "vertical",
            "hasCustomPrompt": true
        })
    );
}

#[test]
fn a_snapshot_claiming_an_unsupported_schema_version_is_refused() {
    let json = serde_json::to_string(&deck()).expect("serializable");
    let tampered = json.replace("\"schemaVersion\":1", "\"schemaVersion\":2");
    let parsed: Presentation =
        serde_json::from_str(&tampered).expect("the field parses; the value is checked");
    assert_eq!(
        parsed.validate().expect_err("version 2"),
        PresentationError::UnsupportedSchemaVersion(2)
    );
}

// ---------------------------------------------------------------------------
// DrawingML text (`a:txBody`)
// ---------------------------------------------------------------------------

use crate::{
    AutoNumberScheme, MAX_FONT_SIZE_HUNDREDTHS, MAX_TEXT_LEVEL, MAX_TEXT_MARGIN_EMU,
    MIN_FONT_SIZE_HUNDREDTHS, TEXT_LEVELS, TextAlign, TextBody, TextBodyProperties, TextBullet,
    TextCharacterProperties, TextLineBreak, TextParagraph, TextParagraphProperties, TextRun,
    TextRunText, TextSpacing, TextUnderline, TextVertical, Typeface,
};

/// A run carrying `text`.
fn run(node: NodeId, text: &str) -> TextRun {
    TextRun::Run(TextRunText {
        id: node,
        properties: None,
        text: text.to_owned(),
    })
}

/// A paragraph of one run.
fn para(node: NodeId, run_id: NodeId, text: &str) -> TextParagraph {
    TextParagraph {
        id: node,
        properties: None,
        runs: vec![run(run_id, text)],
        end_properties: None,
    }
}

fn body(paragraphs: Vec<TextParagraph>) -> TextBody {
    TextBody {
        body_properties: TextBodyProperties::default(),
        list_style: crate::ListStyle::default(),
        paragraphs,
    }
}

#[test]
fn the_text_body_defaults_are_drawingmls_asymmetric_insets() {
    // 0.1in left/right and 0.05in top/bottom, NOT zero: an absent attribute means
    // the default, so zeroing them shifts every line of every slide.
    // Mutation that drove this red: defaulting all four insets to 0.
    let properties = TextBodyProperties::default();
    assert_eq!(properties.inset_left_emu, 91_440, "0.1in");
    assert_eq!(properties.inset_right_emu, 91_440);
    assert_eq!(properties.inset_top_emu, 45_720, "0.05in");
    assert_eq!(properties.inset_bottom_emu, 45_720);
    assert_eq!(
        properties.inset_left_emu,
        properties.inset_top_emu * 2,
        "the side inset is twice the vertical one"
    );
}

#[test]
fn every_autonumber_scheme_round_trips_and_the_set_is_closed() {
    for scheme in AutoNumberScheme::ALL {
        assert_eq!(
            AutoNumberScheme::from_token(scheme.token()),
            scheme,
            "{} did not round-trip",
            scheme.token()
        );
    }
    let distinct: std::collections::BTreeSet<&str> = AutoNumberScheme::ALL
        .iter()
        .map(|scheme| scheme.token())
        .collect();
    assert_eq!(
        distinct.len(),
        AutoNumberScheme::ALL.len(),
        "two schemes share a token, so one can never be read back"
    );
    assert_eq!(AutoNumberScheme::ALL.len(), 41);
    // An unknown scheme degrades rather than failing: a deck must still open.
    assert_eq!(
        AutoNumberScheme::from_token("klingonPeriod"),
        AutoNumberScheme::ArabicPeriod
    );
}

#[test]
fn every_text_token_table_round_trips_and_is_closed() {
    for align in TextAlign::ALL {
        assert_eq!(TextAlign::from_token(align.token()), align);
    }
    for vertical in TextVertical::ALL {
        assert_eq!(TextVertical::from_token(vertical.token()), vertical);
    }
    for underline in TextUnderline::ALL {
        assert_eq!(TextUnderline::from_token(underline.token()), underline);
    }
    for (count, tokens) in [
        (
            TextAlign::ALL.len(),
            TextAlign::ALL.map(TextAlign::token).to_vec(),
        ),
        (
            TextVertical::ALL.len(),
            TextVertical::ALL.map(TextVertical::token).to_vec(),
        ),
        (
            TextUnderline::ALL.len(),
            TextUnderline::ALL.map(TextUnderline::token).to_vec(),
        ),
    ] {
        let distinct: std::collections::BTreeSet<&str> = tokens.iter().copied().collect();
        assert_eq!(distinct.len(), count, "a token table has a collision");
    }
}

#[test]
fn only_the_horizontal_flow_direction_is_unrotated() {
    // A renderer that draws only horizontal text must branch on this, and report the
    // rest rather than drawing them axis-aligned and wrong. Mutation: making
    // `is_rotated` return false for everything collapsed six directions into one.
    let rotated: Vec<&str> = TextVertical::ALL
        .iter()
        .filter(|kind| kind.is_rotated())
        .map(|kind| kind.token())
        .collect();
    assert_eq!(
        rotated,
        vec![
            "vert",
            "vert270",
            "wordArtVert",
            "eaVert",
            "mongolianVert",
            "wordArtVertRtl"
        ]
    );
    assert!(!TextVertical::Horizontal.is_rotated());
}

#[test]
fn an_explicit_no_bullet_is_not_the_same_as_an_inherited_one() {
    // THE distinction that decides whether a deliberately unbulleted paragraph in a
    // bulleted body placeholder grows a bullet on reopen. `None` inherits;
    // `Some(TextBullet::None)` suppresses. Mutation: making `is_visible` true for
    // `TextBullet::None` erased the suppression.
    let inherits = TextParagraphProperties::default();
    assert!(inherits.bullet.is_none(), "unset means inherit");

    let suppressed = TextParagraphProperties {
        bullet: Some(TextBullet::None),
        ..TextParagraphProperties::default()
    };
    assert!(suppressed.bullet.is_some(), "an explicit buNone is stated");
    assert!(
        !suppressed.bullet.as_ref().expect("stated").is_visible(),
        "and it draws nothing"
    );

    let character = TextBullet::Character {
        character: "\u{2022}".to_owned(),
        font: Some(Typeface {
            name: "Arial".to_owned(),
            panose: None,
        }),
    };
    assert!(character.is_visible());
    assert!(
        TextBullet::AutoNumber {
            scheme: AutoNumberScheme::ArabicPeriod,
            start_at: Some(3),
        }
        .is_visible()
    );
}

#[test]
fn a_theme_font_reference_is_distinguished_from_a_family_name() {
    // `+mn-lt` must be resolved at layout against the theme, not treated as a family
    // called "+mn-lt" — which would fall back to a default font for most slide text.
    for reference in ["+mn-lt", "+mj-lt", "+mn-ea", "+mj-cs"] {
        assert!(
            Typeface {
                name: reference.to_owned(),
                panose: None
            }
            .is_theme_reference(),
            "{reference} is a theme reference"
        );
    }
    for family in ["Arial", "Calibri", "Noto Sans", ""] {
        assert!(
            !Typeface {
                name: family.to_owned(),
                panose: None
            }
            .is_theme_reference(),
            "{family} is a family"
        );
    }
}

#[test]
fn a_font_size_outside_the_schema_domain_is_refused_and_an_absent_one_is_not() {
    let sized = |size: u32| TextCharacterProperties {
        size_hundredths_point: Some(size),
        ..TextCharacterProperties::default()
    };
    assert!(sized(MIN_FONT_SIZE_HUNDREDTHS).size_in_domain(), "1pt");
    assert!(sized(MAX_FONT_SIZE_HUNDREDTHS).size_in_domain(), "4000pt");
    assert!(!sized(MIN_FONT_SIZE_HUNDREDTHS - 1).size_in_domain());
    assert!(!sized(MAX_FONT_SIZE_HUNDREDTHS + 1).size_in_domain());
    // Absent inherits, so it must NOT be refused.
    assert!(TextCharacterProperties::default().size_in_domain());

    // And the refusal surfaces through validation. This is the check that catches a
    // `w:sz` half-point value carried into `a:rPr@sz` without conversion: 24pt in
    // half-points is 48, which is below the 100 minimum.
    let paragraph = TextParagraph {
        id: id(300),
        properties: None,
        runs: vec![TextRun::Run(TextRunText {
            id: id(301),
            properties: Some(Box::new(sized(48))),
            text: "half-points by mistake".to_owned(),
        })],
        end_properties: None,
    };
    assert_eq!(
        body(vec![paragraph])
            .validate()
            .expect_err("48 hundredths is 0.48pt"),
        PresentationError::FontSizeOutOfDomain(48)
    );
}

#[test]
fn an_outline_level_above_eight_is_refused_and_eight_is_not() {
    let levelled = |level: u8| TextParagraphProperties {
        level: Some(level),
        ..TextParagraphProperties::default()
    };
    levelled(MAX_TEXT_LEVEL)
        .validate()
        .expect("level 8 is the ninth and last");
    levelled(0).validate().expect("level 0 is the first");
    // Mutation: dropping the bound let level 9 through, which indexes past the
    // nine-level list style and silently takes level 0's formatting.
    assert_eq!(
        levelled(MAX_TEXT_LEVEL + 1)
            .validate()
            .expect_err("level 9 does not exist"),
        PresentationError::TextLevelOutOfRange(9)
    );
    assert_eq!(TEXT_LEVELS, 9);
}

#[test]
fn a_negative_indent_is_accepted_because_every_bullet_uses_one() {
    // The hanging indent is negative by construction, so a non-negative bound on
    // `indent` would refuse ordinary bulleted text. Guarded because the margins DO
    // have a non-negative bound and sharing it would have been the natural mistake.
    TextParagraphProperties {
        margin_left_emu: Some(457_200),
        indent_emu: Some(-457_200),
        ..TextParagraphProperties::default()
    }
    .validate()
    .expect("a hanging indent is ordinary");

    assert_eq!(
        TextParagraphProperties {
            margin_left_emu: Some(-1),
            ..TextParagraphProperties::default()
        }
        .validate()
        .expect_err("a negative margin is not"),
        PresentationError::TextMarginOutOfDomain(-1)
    );
    assert_eq!(
        TextParagraphProperties {
            indent_emu: Some(-MAX_TEXT_MARGIN_EMU - 1),
            ..TextParagraphProperties::default()
        }
        .validate()
        .expect_err("but it is still bounded"),
        PresentationError::TextMarginOutOfDomain(-MAX_TEXT_MARGIN_EMU - 1)
    );
}

#[test]
fn a_text_body_with_no_paragraphs_is_refused_but_an_empty_paragraph_is_not() {
    assert_eq!(
        body(Vec::new()).validate().expect_err("no paragraphs"),
        PresentationError::EmptyTextBody
    );
    // An empty `a:p` is how PowerPoint writes a blank line, so refusing it would
    // refuse real files. Mutation: refusing an empty `runs` broke this.
    body(vec![TextParagraph::empty(id(310))])
        .validate()
        .expect("an empty paragraph is a blank line");
}

#[test]
fn an_empty_text_run_is_refused() {
    assert_eq!(
        body(vec![para(id(320), id(321), "")])
            .validate()
            .expect_err("an empty a:t"),
        PresentationError::EmptyTextRun(id(321))
    );
}

#[test]
fn a_list_style_deeper_than_nine_levels_is_refused() {
    let mut text = body(vec![TextParagraph::empty(id(330))]);
    text.list_style.levels = (0..=TEXT_LEVELS)
        .map(|_| Some(TextParagraphProperties::default()))
        .collect();
    assert_eq!(
        text.validate().expect_err("ten levels"),
        PresentationError::TooManyTextLevels(TEXT_LEVELS + 1)
    );
    text.list_style.levels.pop();
    text.validate().expect("nine is the limit, not an error");
}

#[test]
fn plain_text_joins_paragraphs_with_newlines_and_a_break_contributes_one() {
    let text = TextBody {
        body_properties: TextBodyProperties::default(),
        list_style: crate::ListStyle::default(),
        paragraphs: vec![
            TextParagraph {
                id: id(340),
                properties: None,
                runs: vec![
                    run(id(341), "first"),
                    TextRun::LineBreak(TextLineBreak {
                        id: id(342),
                        properties: None,
                    }),
                    run(id(343), "same paragraph"),
                ],
                end_properties: None,
            },
            para(id(344), id(345), "second"),
        ],
    };
    // Mutation: making a break contribute "" silently joined two visual lines, which
    // no count-based assertion would notice.
    assert_eq!(text.plain_text(), "first\nsame paragraph\nsecond");
    assert_eq!(text.paragraphs[0].level(), 0, "unstated level is zero");
}

// ---------------------------------------------------------------------------
// The guard that makes the interim carrier unreachable
// ---------------------------------------------------------------------------

#[test]
fn a_document_text_box_shape_is_refused_on_a_slide() {
    // `GroupTextBox` holds `Vec<BlockNode>` — WordprocessingML paragraphs — which
    // cannot express `a:pPr@lvl`, an inline bullet or an `a:lstStyle`. It was the
    // interim carrier for slide text before `TextBody` existed. Refusing it is what
    // makes that interim state UNREACHABLE rather than merely discouraged, so the
    // work cannot quietly be redone against the wrong type later.
    //
    // Mutation: removing the refusal let a slide validate while carrying text that
    // would have been silently downgraded at the first round trip.
    use casual_doc_model::v1::{GroupTextBox, ShapeGeometry, TextBoxBodyProperties};

    let text_box = GroupChild::TextBox(GroupTextBox {
        id: id(350),
        offset: ORIGIN,
        extent: BOX,
        geometry: ShapeGeometry::Rectangle,
        preset: None,
        adjustments: Vec::new(),
        // Empty deliberately, and it is not a shortcut: the refusal fires on the
        // CHILD KIND, so the blocks play no part in what this guard proves. Building
        // a real `w:p` here would assert something about the document model instead.
        blocks: Vec::new(),
        fill: None,
        border: None,
        body_properties: TextBoxBodyProperties::default(),
        hyperlink: None,
        flip_h: false,
        flip_v: false,
        rotation: None,
    });
    assert_eq!(
        tree(id(352), vec![SlideNode::new(text_box)])
            .validate(&Definitions::default())
            .expect_err("a document text box on a slide"),
        PresentationError::TextBoxShapeOnSlide(id(350))
    );
}

#[test]
fn a_text_node_id_colliding_with_a_shape_id_is_refused() {
    // Text ids live in the same `NodeId` space as shape ids, so the duplicate walk
    // has to descend into the text body. Mutation: not walking the text left a
    // paragraph id equal to a shape id undetected, which is exactly the class of
    // defect the shared traversal exists to prevent.
    let mut presentation = deck();
    presentation.slides_mut()[0].shapes.children[0].text =
        Some(body(vec![para(id(32), id(400), "collides with the shape")]));
    assert_eq!(
        presentation
            .validate()
            .expect_err("paragraph id is a shape id"),
        PresentationError::DuplicateNodeId(id(32))
    );
}

#[test]
fn a_slide_carrying_text_validates_and_round_trips() {
    let mut presentation = deck();
    presentation.slides_mut()[0].shapes.children[0].text = Some(TextBody {
        body_properties: TextBodyProperties {
            anchor: crate::TextAnchor::Center,
            vertical: TextVertical::Vertical270,
            auto_fit: crate::TextAutoFit::Normal {
                font_scale: Some(92_500),
                line_space_reduction: Some(10_000),
            },
            ..TextBodyProperties::default()
        },
        list_style: crate::ListStyle::default(),
        paragraphs: vec![TextParagraph {
            id: id(410),
            properties: Some(Box::new(TextParagraphProperties {
                level: Some(2),
                alignment: Some(TextAlign::Center),
                margin_left_emu: Some(914_400),
                indent_emu: Some(-342_900),
                line_spacing: Some(TextSpacing::Percent {
                    thousandths: 90_000,
                }),
                space_before: Some(TextSpacing::Points { hundredths: 1_000 }),
                bullet: Some(TextBullet::AutoNumber {
                    scheme: AutoNumberScheme::RomanUcPeriod,
                    start_at: Some(4),
                }),
                ..TextParagraphProperties::default()
            })),
            runs: vec![TextRun::Run(TextRunText {
                id: id(411),
                properties: Some(Box::new(TextCharacterProperties {
                    size_hundredths_point: Some(2_800),
                    bold: Some(true),
                    underline: Some(TextUnderline::Double),
                    latin: Some(Typeface {
                        name: "+mn-lt".to_owned(),
                        panose: None,
                    }),
                    ..TextCharacterProperties::default()
                })),
                text: "Agenda".to_owned(),
            })],
            end_properties: None,
        }],
    });
    presentation.validate().expect("a deck with slide text");

    let json = serde_json::to_string(&presentation).expect("serializable");
    let parsed: Presentation = serde_json::from_str(&json).expect("deserializable");
    assert_eq!(parsed, presentation, "slide text did not round-trip");
    assert_eq!(
        parsed.slides()[0].shapes.children[0]
            .text
            .as_ref()
            .expect("text")
            .plain_text(),
        "Agenda"
    );
}

#[test]
fn an_absent_text_body_is_omitted_from_the_snapshot() {
    // The overwhelming majority of shapes carry no text, so writing `"text": null`
    // on each would grow every snapshot and defeat byte comparison.
    let json = serde_json::to_value(deck()).expect("serializable");
    let node = &json["slides"][0]["shapes"]["children"][0];
    assert!(
        node.get("text").is_none(),
        "shape.text should be omitted when absent, got {node}"
    );
}

#[test]
fn spacing_as_a_percentage_and_as_points_are_distinct_representations() {
    // The schema admits one or the other, never both, so they must not collapse: 90%
    // line spacing and 90 hundredths of a point are wildly different and a type that
    // conflated them would round-trip one as the other.
    let percent = TextSpacing::Percent {
        thousandths: 90_000,
    };
    let points = TextSpacing::Points { hundredths: 90_000 };
    assert_ne!(percent, points);
    let encoded = serde_json::to_value(percent).expect("serializable");
    assert_eq!(encoded["unit"], "percent");
    assert_eq!(
        serde_json::to_value(points).expect("serializable")["unit"],
        "points"
    );
}
