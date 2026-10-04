// SPDX-License-Identifier: Apache-2.0

//! Presentation model guards.
//!
//! Every guard here was driven red by mutating production code before being
//! trusted; the mutation is named beside the assertion it protects, because a guard
//! that cannot fail is worse than no guard (`SKILL` §4).

use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    ColorScheme, ColorTransform, Definitions, Extent, FontCollection, FontScheme, GroupChild,
    GroupPicture, GroupShape, GroupTransform, MAX_GROUP_DEPTH, MediaId, MediaReference, PointEmu,
    RgbColor, Rgba, SchemeColor, ShapeGeometry, SystemColor, ThemeFontEntry, WordprocessingGroup,
};

use crate::{
    LayoutKind, MAX_SLIDE_EMU, MIN_SLIDE_EMU, Placeholder, PlaceholderKind, PlaceholderOrientation,
    PlaceholderSize, Presentation, PresentationError, SCHEMA_VERSION, ShapeTree, Slide, SlideAxis,
    SlideId, SlideLayout, SlideLayoutId, SlideMaster, SlideMasterId, SlideNode, SlideSize,
    SlideSizeKind, TextStyles,
};
// Own line (anti-conflict): the theme indirection, new in this change.
use crate::{
    ColorMap, ColorMapping, ColorRole, SchemeColorToken, THEME_COLOR_SLOTS, ThemeColorSlot,
    ThemeFontReference, ThemePalette,
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
        // No `p:txStyles`: this fixture predates the text cascade, and empty
        // tiers are exactly what a master with no `p:txStyles` carries.
        text_styles: TextStyles::default(),
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
        // No `p:txStyles`: this fixture predates the text cascade, and empty
        // tiers are exactly what a master with no `p:txStyles` carries.
        text_styles: TextStyles::default(),
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
        // No `p:txStyles`: this fixture predates the text cascade, and empty
        // tiers are exactly what a master with no `p:txStyles` carries.
        text_styles: TextStyles::default(),
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
            // No `p:txStyles`: this fixture predates the text cascade, and empty
            // tiers are exactly what a master with no `p:txStyles` carries.
            text_styles: TextStyles::default(),
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
            // No `p:txStyles`: this fixture predates the text cascade, and empty
            // tiers are exactly what a master with no `p:txStyles` carries.
            text_styles: TextStyles::default(),
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

/// A `p:defaultTextStyle` deeper than nine levels is refused, like every other
/// `a:lstStyle` in the model.
///
/// The bottom tier arrives through `with_default_text_style` AFTER construction, so
/// it misses `Presentation::new`'s validation unless the builder re-validates. It
/// does, and this is what says so — otherwise the nine-level ceiling would hold for
/// a shape's list style and not for the one every shape inherits through.
#[test]
fn a_default_text_style_deeper_than_nine_levels_is_refused() {
    use crate::{TEXT_LEVELS, TextParagraphProperties};

    let presentation = deck();
    let mut levels: Vec<Option<TextParagraphProperties>> = (0..=TEXT_LEVELS)
        .map(|_| Some(TextParagraphProperties::default()))
        .collect();
    assert_eq!(levels.len(), TEXT_LEVELS + 1, "one level too many");

    let refused = presentation
        .clone()
        .with_default_text_style(crate::ListStyle {
            levels: levels.clone(),
        });
    assert!(
        matches!(refused, Err(PresentationError::TooManyTextLevels(count)) if count == TEXT_LEVELS + 1),
        "a tenth level must be refused, not stored: {refused:?}"
    );

    // The control: exactly nine is accepted, so the guard is bounding the ceiling
    // rather than refusing every default text style.
    levels.pop();
    assert!(
        presentation
            .with_default_text_style(crate::ListStyle { levels })
            .is_ok(),
        "nine levels is the ceiling, not an error"
    );
}

// ---------------------------------------------------------------------------
// The theme indirection: the colour map, the palette, and the font references.
// ---------------------------------------------------------------------------

/// A scheme with twelve distinguishable slots, so a wrong binding is visible.
///
/// `dk1` and `lt1` are `a:sysClr` with a `lastClr`, which is the form Office
/// writes, and every value differs from every other.
fn scheme() -> ColorScheme {
    let srgb = |r, g, b| SchemeColor::Srgb(RgbColor { r, g, b });
    ColorScheme {
        name: "Fixture".to_owned(),
        dark1: SchemeColor::System(SystemColor {
            value: "windowText".to_owned(),
            last_color: Some(RgbColor { r: 0, g: 0, b: 0 }),
        }),
        light1: SchemeColor::System(SystemColor {
            value: "window".to_owned(),
            last_color: Some(RgbColor {
                r: 255,
                g: 255,
                b: 255,
            }),
        }),
        dark2: srgb(0x44, 0x54, 0x6A),
        light2: srgb(0xE7, 0xE6, 0xE6),
        accent1: srgb(0x44, 0x72, 0xC4),
        accent2: srgb(0xED, 0x7D, 0x31),
        accent3: srgb(0xA5, 0xA5, 0xA5),
        accent4: srgb(0xFF, 0xC0, 0x00),
        accent5: srgb(0x5B, 0x9B, 0xD5),
        accent6: srgb(0x70, 0xAD, 0x47),
        hyperlink: srgb(0x05, 0x63, 0xC1),
        followed_hyperlink: srgb(0x95, 0x4F, 0x72),
    }
}

/// `tx1` is resolved THROUGH the colour map and `dk1` is not.
///
/// This is the distinction the whole type exists for. `ST_SchemeColorVal` admits
/// both spellings and they mean different things: `tx1` is the presentation's
/// "text" ROLE, which the master's `p:clrMap` binds, while `dk1` names the
/// `a:clrScheme` entry itself. Folding the two into one alias table — which is what
/// a reader naturally does — makes `tx1` unmappable, and under the dark map here
/// that paints black text on a black background.
///
/// Mutation: make `SchemeColorToken::from_token` answer
/// `Self::Slot(ThemeColorSlot::Dark1)` for `"tx1"`.
#[test]
fn a_colour_role_goes_through_the_map_and_a_slot_name_does_not() {
    // The dark design: PowerPoint writes exactly this for a dark master.
    let dark = ColorMap {
        background1: ThemeColorSlot::Dark1,
        text1: ThemeColorSlot::Light1,
        background2: ThemeColorSlot::Dark2,
        text2: ThemeColorSlot::Light2,
        ..ColorMap::IDENTITY
    };
    let palette = ThemePalette::new(&scheme(), dark);
    let white = Rgba {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    let black = Rgba {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };

    assert_eq!(
        palette.resolve("tx1", ColorTransform::default()),
        Some(white),
        "tx1 is a role, and this map binds it to a:lt1"
    );
    assert_eq!(
        palette.resolve("dk1", ColorTransform::default()),
        Some(black),
        "dk1 names the scheme entry directly and must NOT be remapped"
    );
    assert_eq!(
        palette.resolve("bg1", ColorTransform::default()),
        Some(black),
        "and bg1 is the other half of the same swap"
    );

    // The same markup under the identity map gives the opposite answers, which is
    // what makes the two readings distinguishable at all.
    let light = ThemePalette::new(&scheme(), ColorMap::IDENTITY);
    assert_eq!(light.resolve("tx1", ColorTransform::default()), Some(black));
    assert_eq!(
        light.resolve("dk1", ColorTransform::default()),
        Some(black),
        "a slot name is map-independent, so this one does not move"
    );

    // `phClr` is a formal parameter, not a colour, and a token outside the
    // enumeration is unanswerable rather than defaulted.
    assert_eq!(palette.resolve("phClr", ColorTransform::default()), None);
    assert_eq!(palette.resolve("accent9", ColorTransform::default()), None);
}

/// A colour transform is applied in the per-100000 units the file states, and a
/// different transform gives a different colour.
///
/// Mutation: divide `ColorTransform`'s values by 100 instead of 100000 in
/// `apply`.
#[test]
fn a_palette_folds_a_transform_in_per_100000_units() {
    let palette = ThemePalette::new(&scheme(), ColorMap::IDENTITY);
    let tint = ColorTransform {
        tint: Some(40_000),
        ..ColorTransform::default()
    };
    assert_eq!(
        palette.resolve("accent1", tint),
        Some(Rgba {
            r: 180,
            g: 199,
            b: 231,
            a: 255
        }),
        "4472C4 tinted 40% toward white"
    );
    let shade = ColorTransform {
        shade: Some(40_000),
        ..ColorTransform::default()
    };
    assert_ne!(
        palette.resolve("accent1", shade),
        palette.resolve("accent1", tint),
        "a shade of the same magnitude must not equal a tint"
    );
    assert_eq!(
        palette.resolve("accent1", ColorTransform::default()),
        Some(Rgba {
            r: 0x44,
            g: 0x72,
            b: 0xC4,
            a: 255
        }),
        "and no transform leaves the slot alone"
    );
}

/// The colour-map chain is slide, then layout, then master, then the identity —
/// and an absent entry at one tier falls through rather than defaulting.
///
/// Mutation: drop the `slides` lookup from `ColorMapping::in_force`, which makes a
/// slide's own `a:overrideClrMapping` resolve to its layout's instead.
#[test]
fn the_colour_map_chain_is_slide_then_layout_then_master() {
    let master = SlideMasterId::new(id(10));
    let layout = SlideLayoutId::new(id(20));
    let slide = SlideId::new(id(30));
    let bare = SlideId::new(id(31));

    let map_with = |accent2| ColorMap {
        accent2,
        ..ColorMap::IDENTITY
    };
    let mut mapping = ColorMapping::default();
    mapping
        .masters
        .insert(master, map_with(ThemeColorSlot::Accent2));
    mapping
        .layouts
        .insert(layout, map_with(ThemeColorSlot::Accent4));
    mapping
        .slides
        .insert(slide, map_with(ThemeColorSlot::Accent5));

    assert_eq!(
        mapping.in_force(slide, Some(layout), Some(master)).accent2,
        ThemeColorSlot::Accent5,
        "the slide's own override wins"
    );
    assert_eq!(
        mapping.in_force(bare, Some(layout), Some(master)).accent2,
        ThemeColorSlot::Accent4,
        "a slide with no override takes its layout's"
    );
    assert_eq!(
        mapping.in_force(bare, None, Some(master)).accent2,
        ThemeColorSlot::Accent2,
        "and with no layout override, the master's"
    );
    assert_eq!(
        mapping.in_force(bare, None, None),
        ColorMap::IDENTITY,
        "a deck that states nothing gets the identity, not an empty map"
    );
    assert!(!mapping.is_empty());
    assert!(ColorMapping::default().is_empty());
}

/// A colour map keyed by a part the deck does not hold is REFUSED.
///
/// The failure mode is a wrong answer rather than a missing one:
/// [`ColorMapping::in_force`] falls through to the tier above, so a dangling key
/// silently resolves a role to another tier's slot and repaints the slide.
///
/// Mutation: make `theme::validate_mapping` return `Ok(())` unconditionally.
#[test]
fn a_colour_map_keyed_by_an_unknown_part_is_refused() {
    let mut mapping = ColorMapping::default();
    let stranger = SlideId::new(id(999));
    mapping.slides.insert(stranger, ColorMap::IDENTITY);
    assert_eq!(
        deck().with_color_mapping(mapping).map(|_| ()),
        Err(PresentationError::DanglingColorMapRef(stranger.node_id())),
    );

    // And the real slide's id is accepted, so the guard is not refusing everything.
    let slide = deck().slides()[0].id;
    let mut good = ColorMapping::default();
    good.slides.insert(slide, ColorMap::IDENTITY);
    let deck = deck()
        .with_color_mapping(good)
        .expect("a map keyed by a slide this deck holds");
    assert_eq!(deck.color_mapping().slides.len(), 1);
    assert_eq!(
        deck.color_map_of(&deck.slides()[0]),
        ColorMap::IDENTITY,
        "and the chain lookup finds it"
    );
}

/// A `+mj-lt`-style reference names a collection AND a script axis, and resolves
/// to the family in that exact cell.
///
/// Mutation: swap `major`/`minor` in `ThemeFontReference::resolve`, or map every
/// suffix to the latin entry.
#[test]
fn a_theme_font_reference_names_one_cell_of_the_font_scheme() {
    let entry = |typeface: &str| ThemeFontEntry {
        typeface: typeface.to_owned(),
        panose: None,
        pitch_family: None,
        charset: None,
    };
    let font_scheme = FontScheme {
        major: FontCollection {
            latin: entry("Calibri Light"),
            ea: entry("Yu Gothic Light"),
            // The empty "fall back to the latin entry" marker.
            cs: entry(""),
            script_overrides: Vec::new(),
        },
        minor: FontCollection {
            latin: entry("Calibri"),
            ea: entry(""),
            cs: entry(""),
            script_overrides: Vec::new(),
        },
    };

    for (reference, expected) in [
        ("+mj-lt", Some("Calibri Light")),
        ("+mn-lt", Some("Calibri")),
        ("+mj-ea", Some("Yu Gothic Light")),
        // Empty is the fall-back marker, not a family: answering `Some("")` would
        // put a font nobody asked for in front of the matcher.
        ("+mj-cs", None),
        ("+mn-ea", None),
    ] {
        let parsed = ThemeFontReference::parse(reference)
            .unwrap_or_else(|| panic!("{reference} is a theme reference"));
        assert_eq!(
            parsed.resolve(&font_scheme),
            expected,
            "{reference} resolved to the wrong cell"
        );
    }

    // A concrete family is not a reference at all.
    assert_eq!(ThemeFontReference::parse("Calibri"), None);
    assert_eq!(ThemeFontReference::parse("+mj"), None);
    assert_eq!(ThemeFontReference::parse("+xx-lt"), None);
    assert_eq!(ThemeFontReference::parse("+mj-zz"), None);
}

/// Every role and every slot round-trips through its token spelling.
///
/// A silent typo in one of the twenty-four strings would make one binding
/// unreadable, and a reader that fell back to the identity for it is exactly the
/// half-applied map the design refuses.
///
/// Mutation: misspell any one token, e.g. `ThemeColorSlot::FollowedHyperlink`'s as
/// `folHLink`.
#[test]
fn every_slot_and_role_token_round_trips() {
    for slot in ThemeColorSlot::ALL {
        assert_eq!(
            ThemeColorSlot::from_token(slot.token()),
            Some(slot),
            "{} does not round-trip",
            slot.token()
        );
    }
    for (index, slot) in ThemeColorSlot::ALL.iter().enumerate() {
        assert_eq!(slot.index(), index, "slot order must be a:clrScheme's");
    }
    for role in ColorRole::ALL {
        // Each role's attribute name is also its `a:schemeClr@val` spelling, and
        // must parse back as a ROLE rather than as a slot.
        assert_eq!(
            SchemeColorToken::from_token(role.attribute()),
            Some(SchemeColorToken::Role(role)),
            "{} must parse as a role",
            role.attribute()
        );
        // Under the identity map every role reaches its own default slot.
        assert_eq!(
            ColorMap::IDENTITY.slot(role),
            role.default_slot(),
            "the identity map must agree with each role's default"
        );
    }
    for slot in [
        ThemeColorSlot::Dark1,
        ThemeColorSlot::Light1,
        ThemeColorSlot::Dark2,
        ThemeColorSlot::Light2,
    ] {
        assert_eq!(
            SchemeColorToken::from_token(slot.token()),
            Some(SchemeColorToken::Slot(slot)),
            "{} must parse as a slot, bypassing the map",
            slot.token()
        );
    }
    assert_eq!(
        SchemeColorToken::from_token("phClr"),
        Some(SchemeColorToken::Placeholder)
    );
    assert_eq!(SchemeColorToken::from_token("nonsense"), None);
    assert_eq!(THEME_COLOR_SLOTS, ThemeColorSlot::ALL.len());
}

/// The deck-level resolvers answer nothing when the package carried no theme,
/// rather than substituting Office defaults.
///
/// Mutation: make `Presentation::theme_palette_of` fall back to
/// `ColorScheme::default()`.
#[test]
fn a_deck_with_no_theme_resolves_nothing_rather_than_a_default_palette() {
    let deck = deck();
    assert!(
        deck.theme_palette_of(&deck.slides()[0]).is_none(),
        "no a:clrScheme means no palette, and a fabricated one looks deliberate"
    );
    assert_eq!(
        deck.resolve_typeface(&crate::Typeface {
            name: "+mn-lt".to_owned(),
            panose: None,
        }),
        None
    );
    // A concrete family still passes through, because it needs no theme.
    assert_eq!(
        deck.resolve_typeface(&crate::Typeface {
            name: "Inter".to_owned(),
            panose: None,
        }),
        Some("Inter")
    );
}

/// A theme that IS present resolves through the deck, under the slide's own map.
///
/// Mutation: make `Presentation::color_map_of` return `ColorMap::IDENTITY`.
#[test]
fn a_deck_resolves_its_palette_under_the_slide_effective_map() {
    let mut deck = deck();
    deck.definitions_mut().color_scheme = Some(scheme());
    let slide_id = deck.slides()[0].id;
    let mut mapping = ColorMapping::default();
    mapping.slides.insert(
        slide_id,
        ColorMap {
            text1: ThemeColorSlot::Accent6,
            ..ColorMap::IDENTITY
        },
    );
    let deck = deck
        .with_color_mapping(mapping)
        .expect("the map names this deck's slide");
    let palette = deck
        .theme_palette_of(&deck.slides()[0])
        .expect("the deck carries a colour scheme");
    assert_eq!(
        palette.resolve("tx1", ColorTransform::default()),
        Some(Rgba {
            r: 0x70,
            g: 0xAD,
            b: 0x47,
            a: 255
        }),
        "the slide's own map binds tx1 to a:accent6"
    );
    assert_eq!(
        palette.slot(ThemeColorSlot::Dark1),
        Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 255
        },
        "and a slot lookup is map-independent, so a:dk1 is still black"
    );

    // The side table is keyed by a part ID, which is a `NodeId` — and a JSON map
    // key has to be a string. `NodeId` serializes as one, so this works; asserted
    // rather than assumed, because a map whose keys could not serialize would make
    // every snapshot of a themed deck fail and nothing else in the suite carries a
    // non-empty mapping.
    let json = serde_json::to_string(&deck).expect("a deck with a colour map serializes");
    let parsed: Presentation = serde_json::from_str(&json).expect("and round-trips");
    assert_eq!(parsed, deck);
    assert_eq!(parsed.color_mapping().slides.len(), 1);
}

// ---------------------------------------------------------------------------
// Tables
// ---------------------------------------------------------------------------

/// A grid of `widths`, in EMU.
fn grid(widths: &[i64]) -> Vec<crate::TableGridColumn> {
    widths
        .iter()
        .map(|width_emu| crate::TableGridColumn {
            width_emu: *width_emu,
        })
        .collect()
}

/// A row whose cells carry the merge roles in `roles`, with ids from `first`.
///
/// Written out rather than defaulted: the whole point of these fixtures is the
/// roles, so a helper that filled them in would be testing the helper.
fn row(
    node: NodeId,
    first: u64,
    roles: &[(crate::CellMerge, crate::CellMerge)],
) -> crate::TableRow {
    crate::TableRow {
        id: node,
        height_emu: 370_840,
        cells: roles
            .iter()
            .enumerate()
            .map(|(index, (horizontal, vertical))| crate::TableCell {
                id: id(first + index as u64),
                horizontal: *horizontal,
                vertical: *vertical,
                properties: crate::TableCellProperties::default(),
                text: None,
            })
            .collect(),
    }
}

/// A two-by-two table with no merges at all, as the baseline every merge fixture
/// perturbs by exactly one cell.
fn plain_table() -> crate::SlideTable {
    use crate::CellMerge::None as Un;
    crate::SlideTable {
        id: id(100),
        properties: crate::TableProperties::default(),
        grid: grid(&[1_828_800, 2_743_200]),
        rows: vec![
            row(id(101), 110, &[(Un, Un), (Un, Un)]),
            row(id(102), 120, &[(Un, Un), (Un, Un)]),
        ],
    }
}

#[test]
fn an_unmerged_table_validates_and_derives_its_width_from_its_grid() {
    let table = plain_table();
    table.validate().expect("a plain two-by-two table is valid");
    assert_eq!(
        table.width_emu(),
        4_572_000,
        "an a:tbl states no width; the grid's sum IS the width"
    );
}

/// `@gridSpan`/`@rowSpan` and `@hMerge`/`@vMerge` are one enum per axis, so a cell
/// cannot hold both — and a well-formed merge tiles its line exactly.
#[test]
fn a_well_formed_merge_on_either_axis_validates() {
    use crate::CellMerge::{Continuation, None as Un, Origin};
    let mut table = plain_table();
    // Row 0 is one merged cell across both columns; row 1 is untouched.
    table.rows[0] = row(id(101), 110, &[(Origin(2), Un), (Continuation, Un)]);
    table
        .validate()
        .expect("an origin plus its one continuation tiles a two-column row");

    let mut table = plain_table();
    // Column 0 is one merged cell down both rows.
    table.rows[0] = row(id(101), 110, &[(Un, Origin(2)), (Un, Un)]);
    table.rows[1] = row(id(102), 120, &[(Un, Continuation), (Un, Un)]);
    table
        .validate()
        .expect("a rowSpan plus its one vMerge tiles a two-row column");
}

/// A continuation with nothing covering it is refused, on the axis it is on.
///
/// Mutation that drove this red: deleting the `if owed == 0` arm from
/// `validate_merge_run`, which makes an orphan `@hMerge` validate — and an orphan
/// continuation renders as a cell that paints nothing with nothing beside it.
#[test]
fn a_continuation_with_no_origin_is_refused_on_its_own_axis() {
    use crate::CellMerge::{Continuation, None as Un};
    let mut table = plain_table();
    table.rows[0] = row(id(101), 110, &[(Continuation, Un), (Un, Un)]);
    assert_eq!(
        table.validate().expect_err("an orphan hMerge"),
        PresentationError::UnanchoredCellMerge {
            cell: id(110),
            axis: crate::TableAxis::Column,
        },
        "the COLUMN axis, because @hMerge is horizontal; naming the row axis here \
         would send a reader to the wrong half of the file"
    );

    let mut table = plain_table();
    table.rows[1] = row(id(102), 120, &[(Un, Continuation), (Un, Un)]);
    assert_eq!(
        table.validate().expect_err("an orphan vMerge"),
        PresentationError::UnanchoredCellMerge {
            cell: id(120),
            axis: crate::TableAxis::Row,
        },
        "and the ROW axis for @vMerge"
    );
}

/// An origin claiming more than its line holds is refused.
///
/// Mutation that drove this red: deleting the trailing `if owed != 0` check in
/// `validate_merge_run`. A `@gridSpan="2"` in the last column then validates, and
/// the merged cell's box runs past the table's own right edge.
#[test]
fn an_origin_claiming_past_the_edge_of_its_line_is_refused() {
    use crate::CellMerge::{None as Un, Origin};
    let mut table = plain_table();
    table.rows[0] = row(id(101), 110, &[(Un, Un), (Origin(2), Un)]);
    assert_eq!(
        table.validate().expect_err("a span past the last column"),
        PresentationError::OverlappingCellMerge {
            cell: id(111),
            axis: crate::TableAxis::Column,
        }
    );
}

/// A vertical merge is checked COLUMN-major, which a row-major walk cannot do.
///
/// This is the guard that proves the second pass exists. A `@rowSpan="2"` on row 0
/// is answered by a `@vMerge` on row 1 in the SAME column index, so a validator
/// that only ever walked rows left-to-right would accept a `rowSpan` with nothing
/// under it — and that renders as a cell whose box covers the row below while that
/// row's own cell is still painted inside it.
///
/// Mutation that drove this red: deleting the `for column in 0..self.grid.len()`
/// loop from `SlideTable::validate`, which leaves only the row-major pass.
#[test]
fn a_row_span_with_no_continuation_under_it_is_refused() {
    use crate::CellMerge::{None as Un, Origin};
    let mut table = plain_table();
    // Row 0 column 0 claims two rows; row 1 column 0 is a PLAIN cell, which is the
    // overlap. Every row is individually well formed horizontally, so a row-major
    // validator sees nothing wrong.
    table.rows[0] = row(id(101), 110, &[(Un, Origin(2)), (Un, Un)]);
    // Every row here is individually well formed on the HORIZONTAL axis — each
    // holds two plain cells — so the row-major pass finds nothing. Only the
    // column-major pass can see that column 0 has an origin claiming two rows
    // with a plain cell in the second.
    assert_eq!(
        table
            .validate()
            .expect_err("a rowSpan with a plain cell under it"),
        PresentationError::OverlappingCellMerge {
            cell: id(120),
            axis: crate::TableAxis::Row,
        },
        "the cell the overlap was detected AT, which is the one in the row below"
    );
}

/// A span of one is not a merge.
///
/// Mutation that drove this red: relaxing the `span < 2` check to `span < 1`.
/// `@gridSpan="1"` is the schema default, so admitting it would give one fact two
/// models — and `CellMerge::Origin(1)` would then compete with `CellMerge::None`
/// everywhere either is matched.
#[test]
fn a_merge_origin_spanning_one_cell_is_refused() {
    use crate::CellMerge::{None as Un, Origin};
    let mut table = plain_table();
    table.rows[0] = row(id(101), 110, &[(Origin(1), Un), (Un, Un)]);
    assert_eq!(
        table.validate().expect_err("a one-cell span"),
        PresentationError::CellSpanOutOfDomain {
            cell: id(110),
            span: 1,
        }
    );
}

/// A row must hold one cell per grid column, continuations included.
///
/// Mutation that drove this red: changing the check to `>` so a SHORT row passes.
/// A short row shifts every cell after the gap into the wrong grid column, which
/// is a table that renders with its content in the wrong places rather than one
/// that renders broken.
#[test]
fn a_row_whose_cell_count_does_not_match_the_grid_is_refused() {
    use crate::CellMerge::None as Un;
    let mut table = plain_table();
    table.rows[1] = row(id(102), 120, &[(Un, Un)]);
    assert_eq!(
        table.validate().expect_err("a row one cell short"),
        PresentationError::TableRowWidthMismatch {
            table: id(100),
            row: id(102),
            cells: 1,
            columns: 2,
        }
    );
}

#[test]
fn a_table_with_no_grid_columns_is_refused() {
    let mut table = plain_table();
    table.grid.clear();
    assert_eq!(
        table.validate().expect_err("no grid"),
        PresentationError::EmptyTableGrid(id(100)),
        "an a:tbl with no a:gridCol has no geometry at all"
    );
}

#[test]
fn a_negative_table_measure_is_refused() {
    let mut table = plain_table();
    table.grid[1].width_emu = -1;
    assert_eq!(
        table.validate().expect_err("a negative a:gridCol@w"),
        PresentationError::TableMeasureOutOfDomain(-1)
    );
    let mut table = plain_table();
    table.rows[0].height_emu = -2;
    assert_eq!(
        table.validate().expect_err("a negative a:tr@h"),
        PresentationError::TableMeasureOutOfDomain(-2)
    );
}

/// A cell id colliding with a shape id is caught by the deck's ONE uniqueness
/// walk.
///
/// Mutation that drove this red: deleting the `child.table` arm from
/// `ShapeTree::visit_node_ids`. The collision then goes undetected, and two
/// different things answer to one id — which is what makes a hit test resolve to
/// the wrong object.
#[test]
fn a_table_cell_id_colliding_with_a_shape_id_is_refused() {
    let mut presentation = deck();
    let mut table = plain_table();
    // `id(32)` is the slide's own shape in `deck()`, so this cell answers to a
    // name something else already has.
    table.rows[0].cells[0].id = id(32);
    presentation.slides_mut()[0]
        .shapes
        .children
        .push(SlideNode::new(shape(id(40))).with_table(table));
    assert_eq!(
        presentation.validate().expect_err("a colliding cell id"),
        PresentationError::DuplicateNodeId(id(32))
    );
}

/// Two `a:tblStyle` entries with one GUID are refused.
///
/// Mutation that drove this red: deleting the `TableStyles::validate` call from
/// `Presentation::validate`. A duplicate is a wrong ANSWER rather than a missing
/// one — `TableStyles::style` takes the first match, so the second entry silently
/// never applies and a table wears the wrong design with nothing reporting it.
#[test]
fn two_table_styles_with_the_same_guid_are_refused() {
    const GUID: &str = "{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}";
    let styles = crate::TableStyles {
        default_style_id: Some(GUID.to_owned()),
        styles: vec![
            crate::TableStyle {
                id: GUID.to_owned(),
                name: Some("First".to_owned()),
            },
            crate::TableStyle {
                id: GUID.to_owned(),
                name: Some("Second".to_owned()),
            },
        ],
    };
    assert_eq!(
        deck()
            .with_table_styles(styles)
            .expect_err("two entries with one id"),
        PresentationError::DuplicateTableStyleId(GUID.to_owned())
    );
}

/// A style GUID is matched EXACTLY — braces and case included.
///
/// `{5C22544A-…}` is one token. A reader that trimmed the braces, or compared
/// case-insensitively, would join a table to a style the file does not name — and
/// a `tableStyles.xml` holding two entries whose GUIDs differ only in case is
/// legal.
#[test]
fn a_table_style_guid_is_matched_exactly() {
    const BRACED: &str = "{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}";
    let styles = crate::TableStyles {
        default_style_id: None,
        styles: vec![crate::TableStyle {
            id: BRACED.to_owned(),
            name: Some("Medium Style 2 - Accent 1".to_owned()),
        }],
    };
    assert_eq!(
        styles.style(BRACED).and_then(|style| style.name.as_deref()),
        Some("Medium Style 2 - Accent 1")
    );
    assert!(
        styles
            .style("5C22544A-7EE6-4342-B048-85BDC9FD1C3A")
            .is_none(),
        "the unbraced spelling is a DIFFERENT token"
    );
    assert!(
        styles
            .style("{5c22544a-7ee6-4342-b048-85bdc9fd1c3a}")
            .is_none(),
        "and so is the lowercase one"
    );
}

/// `CellMerge::units` answers what each role occupies, which is what the layout
/// arithmetic sums.
#[test]
fn a_cell_merge_role_knows_how_many_grid_units_it_owns() {
    use crate::CellMerge;
    assert_eq!(
        (
            CellMerge::None.units(),
            CellMerge::Origin(3).units(),
            CellMerge::Continuation.units(),
        ),
        (1, 3, 0),
        "a continuation owns NOTHING, which is the fact that keeps a covered \
         cell's width out of the sum"
    );
}

/// A deck's theme is validated, through the SHARED rule rather than a copy.
///
/// Before `Definitions::validate_theme` was public, nothing in this crate
/// validated the theme at all — so a deck carrying a font scheme past the model's
/// own bound validated clean while the identical DOCX was refused. One rule, two
/// document classes, and this is the guard that says the delegation happened
/// rather than that the bound exists somewhere.
#[test]
fn deck_validation_delegates_to_the_shared_theme_rule() {
    use casual_doc_model::v1::{Definitions, FontCollection, FontScheme, ThemeFontEntry};

    let over_long = "x".repeat(600);
    let scheme = FontScheme {
        major: FontCollection {
            latin: ThemeFontEntry {
                typeface: over_long.clone(),
                ..ThemeFontEntry::default()
            },
            ..FontCollection::default()
        },
        ..FontScheme::default()
    };
    let definitions = Definitions {
        font_scheme: Some(scheme),
        ..Definitions::default()
    };

    // Through `Presentation::new`, which validates: a deck that fails validation
    // must not exist as a value at all, which is the same contract
    // `the_smallest_deck_validates_and_stamps_the_schema_version` rests on.
    let master = SlideMaster {
        id: SlideMasterId::new(id(10)),
        shapes: tree(id(11), vec![SlideNode::new(shape(id(12)))]),
        name: None,
        background: None,
        text_styles: TextStyles::default(),
    };
    let layout = SlideLayout {
        id: SlideLayoutId::new(id(20)),
        master: master.id,
        kind: LayoutKind::Object,
        shapes: tree(id(21), vec![SlideNode::new(shape(id(22)))]),
        name: None,
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
    let error = Presentation::new(
        id(1),
        SlideSize::DEFAULT_16X9,
        vec![master],
        vec![layout],
        vec![slide],
        definitions,
    )
    .expect_err("an over-long theme typeface must be refused");
    assert!(
        format!("{error:?}").contains("fontScheme"),
        "and refused BY THE THEME RULE, naming the field it broke: {error:?}"
    );
}
