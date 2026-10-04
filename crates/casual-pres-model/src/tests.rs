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
