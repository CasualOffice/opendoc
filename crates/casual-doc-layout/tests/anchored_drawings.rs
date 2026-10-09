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
use casual_doc_layout::display::{
    Fill as DisplayFill, PaintItem, ShapeGeometry as DisplayShapeGeometry,
};
use casual_doc_layout::flow::{build_galley, build_galley_cached, flow_header_footer};
use casual_doc_layout::incremental::{DirtySet, GalleyCache};
use casual_doc_layout::page::{AnchorContent, Page};
use casual_doc_layout::paginate::{PageConfig, paginate, resolve_anchored_fields, resolve_fields};
use casual_doc_layout::running::{
    HeaderFooter as RunningBand, RunningContent, place_running_content,
};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::text::FieldKind;
use casual_doc_layout::units::{Point, Size, Twip};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    AnchorHorizontal, AnchorVertical, AnchoredDrawing, BlockNode, CellVerticalAlignment,
    DefinitionMap, Definitions, Document, DrawingAnchor, Extent, Field, GridColumn,
    HeaderFooter as ModelHeaderFooter, HeaderFooterId, HeightRule, HorizontalAlign,
    HorizontalAnchor, HorizontalPosition, InlineNode, MediaId, MediaReference, PageMargins,
    PageSize, Paragraph, ParagraphProperties, RowHeight, Run, RunProperties, SectionBoundary,
    SectionColumns, SectionId, Table, TableCell, TableCellProperties, TableProperties, TableRow,
    TableRowProperties, VerticalAnchor, VerticalMerge, VerticalPosition, WrapDistances, WrapMode,
};
// Separate `use` line (anti-conflict): the authored wrap side, read by
// `an_authored_wrap_text_decides_the_side_the_text_keeps`.
use casual_doc_model::v1::WrapSide;

/// The vertices a resolved path arrives at, in order.
///
/// The typed presets emit only moves and lines, so for them this is exactly the
/// vertex list the tests asserted before the path primitive replaced it — which is
/// what keeps those assertions comparable across the change.
fn endpoints(commands: &[casual_doc_layout::display::PathCommand]) -> Vec<Point> {
    commands
        .iter()
        .filter_map(|command| command.endpoint())
        .collect()
}

/// Whether a resolved path ends by closing its subpath.
fn closes(commands: &[casual_doc_layout::display::PathCommand]) -> bool {
    commands.last() == Some(&casual_doc_layout::display::PathCommand::Close)
}

/// The vertices of a geometry's FIRST path and whether it closes — what every
/// one-path preset and freeform resolves to.
fn first_path(paths: &[casual_doc_layout::page::AnchorPath]) -> (Vec<Point>, bool) {
    let commands = &paths.first().expect("at least one path").commands;
    (endpoints(commands), closes(commands))
}

/// The corner radius of a resolved `roundRect`: the standard's path starts at
/// `(l, y1)` on the left edge and its first arc ends at `(x1, t)` on the top
/// edge, and `x1 - l` is the radius.
///
/// The rounded rectangle used to be a closed-form primitive carrying its radius;
/// it now resolves through ECMA-376's own definition (true circular arcs rather
/// than the old quadratic corners), so the radius is read off the outline.
fn corner_radius(content: &AnchorContent) -> Twip {
    use casual_doc_layout::display::PathCommand;
    let AnchorContent::Path { paths, .. } = content else {
        panic!("expected a roundRect path, got {content:?}");
    };
    let commands = &paths[0].commands;
    let Some(PathCommand::MoveTo { point: start }) = commands.first() else {
        panic!("a path begins with a move: {commands:?}");
    };
    let Some(PathCommand::CubicTo { point: corner, .. }) = commands.get(1) else {
        panic!("a roundRect's first segment is its top-left arc: {commands:?}");
    };
    Twip(corner.x.raw() - start.x.raw())
}

/// Asserts two closed outlines are the SAME polygon: identical vertices in
/// identical cyclic order, whatever vertex each starts from.
///
/// The standard's preset paths trace each outline from the vertex ECMA-376's
/// `a:pathLst` starts at (a `triangle` starts bottom-left); the hand-written
/// vertex lists these tests were first written against started elsewhere. The
/// shape is what is asserted, to the twip and in order — the start vertex is not
/// a property of the shape.
#[track_caller]
fn assert_same_outline(actual: &[Point], expected: &[Point]) {
    assert_eq!(actual.len(), expected.len(), "{actual:?} vs {expected:?}");
    let matches = (0..expected.len()).any(|shift| {
        (0..expected.len()).all(|index| actual[(index + shift) % actual.len()] == expected[index])
    });
    assert!(
        matches,
        "not the same outline in the same order:\n actual   {actual:?}\n expected {expected:?}"
    );
}

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
        properties: RunProperties::default().into(),
        text: text.to_owned(),
    })
}

fn anchored(id: u64, media: MediaId, h_offset: i64, v_offset: i64, behind_doc: bool) -> InlineNode {
    InlineNode::AnchoredDrawing(Box::new(AnchoredDrawing {
        hyperlink: None,
        opacity: None,
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
            wrap_text: None,
            wrap_distances: Default::default(),
            wrap_polygon: None,
            behind_doc,
        },
        descr: Some("A floating logo".to_owned()),
        relative_height: None,
        crop: None,
        border: None,
        flip_h: false,
        flip_v: false,
        rotation: None,
    }))
}

#[test]
fn an_anchored_drawing_composes_at_its_resolved_page_rect() {
    let (media_id, definitions) = media_defs();
    // A paragraph carrying real text plus an anchored drawing at page offset
    // (914400, 1828800) EMU = (1440, 2880) twips from the page corner.
    let para = BlockNode::Paragraph(Paragraph {
        id: node(10),
        properties: ParagraphProperties::default().into(),
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
            PaintItem::Image { media, rect, .. } if media == "word/media/image1.png" => Some(*rect),
            _ => None,
        })
        .expect("an anchored image paint item");
    assert_eq!(rect.origin, Point::new(Twip(1_440), Twip(2_880)));
}

/// EMU → twip rounding is reachable from a real anchored drawing, not only from the
/// converter's own unit tests (`156` §6 row 0.6).
///
/// Every other fixture in this file uses "nice inch" EMU values — 914400, 1828800 —
/// which are exact multiples of 635 and so quantise identically under any rounding
/// rule. That is why converging the rule moved no committed golden, and it is exactly
/// why this guard is needed: with no non-integral value anywhere in the suite, a
/// regression from rounding back to truncation would be invisible to all of it.
#[test]
fn a_non_integral_emu_anchor_rounds_rather_than_truncates() {
    let (media_id, definitions) = media_defs();
    // 1_000_000 EMU = 1574.80 twips; 400_000 EMU = 629.92 twips. Neither is a
    // multiple of 635, so truncation and rounding disagree on all four numbers.
    let drawing = InlineNode::AnchoredDrawing(Box::new(AnchoredDrawing {
        hyperlink: None,
        opacity: None,
        id: node(12),
        media: media_id,
        extent: Extent {
            width_emu: 1_000_000,
            height_emu: 1_000_000,
        },
        anchor: DrawingAnchor {
            horizontal: AnchorHorizontal {
                relative_from: HorizontalAnchor::Page,
                position: HorizontalPosition::Offset(1_000_000),
            },
            vertical: AnchorVertical {
                relative_from: VerticalAnchor::Page,
                position: VerticalPosition::Offset(400_000),
            },
            wrap: WrapMode::None,
            wrap_text: None,
            wrap_distances: Default::default(),
            wrap_polygon: None,
            behind_doc: false,
        },
        descr: None,
        relative_height: None,
        crop: None,
        border: None,
        flip_h: false,
        flip_v: false,
        rotation: None,
    }));
    let para = BlockNode::Paragraph(Paragraph {
        id: node(10),
        properties: ParagraphProperties::default().into(),
        inlines: vec![run(11, "Body text"), drawing],
    });
    let doc = Document::new(node(1), vec![para], definitions).unwrap();

    let shaper = ParleyShaper::new();
    let cfg = config();
    let galley = build_galley(&doc, &shaper, cfg.content_area().size.width);
    let mut layout = paginate(&galley, &cfg);
    place_floats(&mut layout, &doc, &shaper, &cfg);

    let placed = &layout.pages[0].anchored[0];
    // Truncation would give origin (1574, 629) and a 1574-twip square.
    assert_eq!(placed.rect.origin, Point::new(Twip(1_575), Twip(630)));
    assert_eq!(placed.rect.size, Size::new(Twip(1_575), Twip(1_575)));
}

/// A group with `a:xfrm@rot` turns its children, which it did not before: the
/// rotation was modelled and round-tripped but never applied, so a rotated Word
/// group painted unrotated (`156` §6 row 0.5).
///
/// The child's rect stays axis-aligned and MOVES to where the rotation sends its
/// centre, while the orientation rides the `ShapeTransform` the painter already
/// honours about a free centre. That is what lets a rigid group transform ride the
/// existing per-object path instead of needing a group container in the placed
/// output.
#[test]
fn a_rotated_group_turns_its_children_about_the_group_centre() {
    let (_media_id, definitions) = media_defs();
    // Group box: 2in x 1in at page (1in, 1in) -> origin (1440,1440) size (2880,1440),
    // so its centre is (2880, 2160). The child is a 1in square at the group's own
    // origin, centre (2160, 2160) — 720 twips LEFT of the group centre.
    let group_extent = Extent {
        width_emu: 1_828_800,
        height_emu: 914_400,
    };
    let child_extent = Extent {
        width_emu: 914_400,
        height_emu: 914_400,
    };
    let child = GroupChild::Shape(GroupShape {
        hyperlink: None,
        id: node(31),
        offset: PointEmu { x_emu: 0, y_emu: 0 },
        extent: child_extent,
        geometry: ShapeGeometry::Rectangle,
        preset: None,
        adjustments: Vec::new(),
        path: None,
        fill: None,
        stroke: None,
        flip_h: false,
        flip_v: false,
        rotation: None,
    });
    let group_with = |rotation: Option<i32>| {
        InlineNode::Group(Box::new(WordprocessingGroup {
            hyperlink: None,
            id: node(30),
            anchor: Some(page_anchor(914_400, 914_400)),
            relative_height: None,
            extent: group_extent,
            transform: GroupTransform {
                offset: PointEmu { x_emu: 0, y_emu: 0 },
                extent: group_extent,
                child_offset: PointEmu { x_emu: 0, y_emu: 0 },
                child_extent: group_extent,
                flip_h: false,
                flip_v: false,
                rotation,
            },
            children: vec![child.clone()],
        }))
    };

    let place = |rotation: Option<i32>| {
        let para = BlockNode::Paragraph(Paragraph {
            id: node(10),
            properties: ParagraphProperties::default().into(),
            inlines: vec![run(11, "Body text"), group_with(rotation)],
        });
        let doc = Document::new(node(1), vec![para], definitions.clone()).unwrap();
        let shaper = ParleyShaper::new();
        let cfg = config();
        let galley = build_galley(&doc, &shaper, cfg.content_area().size.width);
        let mut layout = paginate(&galley, &cfg);
        place_floats(&mut layout, &doc, &shaper, &cfg);
        let placed = &layout.pages[0].anchored[0];
        (placed.rect, placed.transform)
    };

    // Unrotated: the child sits at the group origin, untransformed. This is the
    // identity fast path, and it is why adding the machinery moved no golden.
    let (rect, transform) = place(None);
    assert_eq!(rect.origin, Point::new(Twip(1_440), Twip(1_440)));
    assert_eq!(rect.size, Size::new(Twip(1_440), Twip(1_440)));
    assert!(
        transform.is_none(),
        "an unrotated group must add no transform"
    );

    // 90° clockwise about (2880, 2160): the child centre (2160, 2160) is 720 twips
    // to the LEFT, so it lands 720 ABOVE at (2880, 1440) — left -> up is clockwise
    // when y grows downward. Origin therefore (2880-720, 1440-720).
    let (rect, transform) = place(Some(90 * 60_000));
    assert_eq!(
        rect.origin,
        Point::new(Twip(2_160), Twip(720)),
        "the child's rect must move to where the group rotation sends its centre"
    );
    assert_eq!(
        rect.size,
        Size::new(Twip(1_440), Twip(1_440)),
        "size is rigid"
    );
    let transform = transform.expect("a rotated group must give its child a transform");
    assert_eq!(transform.rotation, 90 * 60_000);
    assert!(!transform.flip_h && !transform.flip_v);
    assert_eq!(
        transform.center,
        Point::new(Twip(2_880), Twip(1_440)),
        "the painter must turn the child about its NEW centre"
    );
}

#[test]
fn behind_doc_controls_the_paint_order_relative_to_text() {
    let (media_id, definitions) = media_defs();
    // Two anchored drawings: one behind the text, one in front.
    let para = BlockNode::Paragraph(Paragraph {
        id: node(10),
        properties: ParagraphProperties::default().into(),
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
    Fill, GroupChild, GroupPicture, GroupShape, GroupTextBox, GroupTransform, PointEmu, Rgba,
    ShapeAdjustment, ShapeGeometry, ShapeStroke, TextBox, TextBoxAutoFit, TextBoxBodyProperties,
    TextBoxHorizontalOverflow, TextBoxInsets, TextBoxVerticalAnchor, TextBoxVerticalOverflow,
    WordprocessingGroup,
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
        wrap_text: None,
        wrap_distances: Default::default(),
        wrap_polygon: None,
        behind_doc: false,
    }
}

fn paragraph_anchor() -> DrawingAnchor {
    DrawingAnchor {
        horizontal: AnchorHorizontal {
            relative_from: HorizontalAnchor::Page,
            position: HorizontalPosition::Offset(0),
        },
        vertical: AnchorVertical {
            relative_from: VerticalAnchor::Paragraph,
            position: VerticalPosition::Offset(0),
        },
        wrap: WrapMode::None,
        wrap_text: None,
        wrap_distances: Default::default(),
        wrap_polygon: None,
        behind_doc: false,
    }
}

fn top_bottom_anchor(bottom_twips: i64) -> DrawingAnchor {
    DrawingAnchor {
        horizontal: AnchorHorizontal {
            relative_from: HorizontalAnchor::Column,
            position: HorizontalPosition::Offset(0),
        },
        vertical: AnchorVertical {
            relative_from: VerticalAnchor::Paragraph,
            position: VerticalPosition::Offset(0),
        },
        wrap: WrapMode::TopAndBottom,
        wrap_text: None,
        wrap_distances: WrapDistances {
            bottom_emu: bottom_twips * 635,
            ..WrapDistances::default()
        },
        wrap_polygon: None,
        behind_doc: false,
    }
}

fn top_bottom_drawing(id: u64, media: MediaId, height_twips: i64, bottom_twips: i64) -> InlineNode {
    InlineNode::AnchoredDrawing(Box::new(AnchoredDrawing {
        hyperlink: None,
        opacity: None,
        id: node(id),
        media,
        extent: Extent {
            width_emu: 63_500,
            height_emu: height_twips * 635,
        },
        anchor: top_bottom_anchor(bottom_twips),
        descr: None,
        relative_height: None,
        crop: None,
        border: None,
        flip_h: false,
        flip_v: false,
        rotation: None,
    }))
}

fn anchored_at_paragraph(id: u64, media: MediaId) -> InlineNode {
    InlineNode::AnchoredDrawing(Box::new(AnchoredDrawing {
        hyperlink: None,
        opacity: None,
        id: node(id),
        media,
        extent: Extent {
            width_emu: 63_500,
            height_emu: 63_500,
        },
        anchor: paragraph_anchor(),
        descr: None,
        relative_height: None,
        crop: None,
        border: None,
        flip_h: false,
        flip_v: false,
        rotation: None,
    }))
}

fn anchored_at_column_right(id: u64, media: MediaId) -> InlineNode {
    InlineNode::AnchoredDrawing(Box::new(AnchoredDrawing {
        hyperlink: None,
        opacity: None,
        id: node(id),
        media,
        extent: Extent {
            width_emu: 635_000,
            height_emu: 63_500,
        },
        anchor: DrawingAnchor {
            horizontal: AnchorHorizontal {
                relative_from: HorizontalAnchor::Column,
                position: HorizontalPosition::Align(HorizontalAlign::Right),
            },
            vertical: AnchorVertical {
                relative_from: VerticalAnchor::Paragraph,
                position: VerticalPosition::Offset(0),
            },
            wrap: WrapMode::None,
            wrap_text: None,
            wrap_distances: WrapDistances::default(),
            wrap_polygon: None,
            behind_doc: false,
        },
        descr: None,
        relative_height: None,
        crop: None,
        border: None,
        flip_h: false,
        flip_v: false,
        rotation: None,
    }))
}

fn anchored_at_page_right(id: u64, media: MediaId) -> InlineNode {
    InlineNode::AnchoredDrawing(Box::new(AnchoredDrawing {
        hyperlink: None,
        opacity: None,
        id: node(id),
        media,
        extent: Extent {
            width_emu: 635_000,
            height_emu: 63_500,
        },
        anchor: DrawingAnchor {
            horizontal: AnchorHorizontal {
                relative_from: HorizontalAnchor::Page,
                position: HorizontalPosition::Align(HorizontalAlign::Right),
            },
            vertical: AnchorVertical {
                relative_from: VerticalAnchor::Paragraph,
                position: VerticalPosition::Offset(0),
            },
            wrap: WrapMode::None,
            wrap_text: None,
            wrap_distances: WrapDistances::default(),
            wrap_polygon: None,
            behind_doc: false,
        },
        descr: None,
        relative_height: None,
        crop: None,
        border: None,
        flip_h: false,
        flip_v: false,
        rotation: None,
    }))
}

#[test]
fn a_paragraph_relative_anchor_gets_implicit_keep_with_next() {
    // Issue #359: a paragraph carrying a floating drawing anchored to the
    // paragraph is given implicit keep-with-next, so a page break cannot strand
    // the anchored graphic at a page bottom while its continuation flows onto the
    // next page (the author's title-line-1 + logo, title-line-2 block). A
    // page/margin-anchored float, which floats at a fixed page position
    // regardless of where its paragraph lands, is left unaffected.
    let (media_id, definitions) = media_defs();
    let blocks = vec![
        // Title line 1, carrying a paragraph-anchored logo.
        BlockNode::Paragraph(Paragraph {
            id: node(10),
            properties: ParagraphProperties::default().into(),
            inlines: vec![anchored_at_paragraph(11, media_id), run(12, "Title line 1")],
        }),
        // Title line 2 — the continuation that must stay with line 1.
        BlockNode::Paragraph(Paragraph {
            id: node(13),
            properties: ParagraphProperties::default().into(),
            inlines: vec![run(14, "Title line 2")],
        }),
        // A body paragraph with a vertically PAGE-anchored float (floats at a
        // fixed page position regardless of where the paragraph lands).
        BlockNode::Paragraph(Paragraph {
            id: node(15),
            properties: ParagraphProperties::default().into(),
            inlines: vec![anchored(16, media_id, 0, 5_000, false), run(17, "Body")],
        }),
        // A body paragraph with a paragraph-anchored but WRAPPING (`wrapSquare`)
        // float — a side image text flows around, not an overlay decoration.
        BlockNode::Paragraph(Paragraph {
            id: node(18),
            properties: ParagraphProperties::default().into(),
            inlines: vec![wrapping_at_paragraph(19, media_id), run(20, "Wrapped")],
        }),
    ];
    let doc = Document::new(node(1), blocks, definitions).unwrap();
    let shaper = ParleyShaper::new();
    let galley = build_galley(&doc, &shaper, config().content_area().size.width);

    let keep_next = |index: usize| {
        let BlockFragment::Paragraph { break_control, .. } = &galley[index] else {
            panic!("expected a paragraph fragment");
        };
        break_control.keep_next
    };
    assert!(
        keep_next(0),
        "a paragraph-anchored wrapNone overlay keeps with the next paragraph"
    );
    assert!(!keep_next(1), "a plain paragraph is unaffected");
    assert!(
        !keep_next(2),
        "a page-anchored float floats independently and is not kept"
    );
    assert!(
        !keep_next(3),
        "a wrapping (wrapSquare) side float is a flow element and is not kept"
    );
}

/// A paragraph-anchored but wrapping (`wrapSquare`) float — a side image text
/// flows around, as opposed to a `wrapNone` overlay decoration.
fn wrapping_at_paragraph(id: u64, media: MediaId) -> InlineNode {
    let InlineNode::AnchoredDrawing(mut drawing) = anchored_at_paragraph(id, media) else {
        unreachable!("anchored_at_paragraph builds an anchored drawing");
    };
    drawing.anchor.wrap = WrapMode::Square;
    InlineNode::AnchoredDrawing(drawing)
}

fn one_cell_table(
    table_id: u64,
    row_id: u64,
    cell_id: u64,
    paragraph_id: u64,
    inlines: Vec<InlineNode>,
) -> BlockNode {
    BlockNode::Table(Box::new(Table {
        id: node(table_id),
        grid: vec![GridColumn {
            width_twips: Some(4_000),
        }],
        grid_change: None,
        properties: TableProperties::default(),
        rows: vec![TableRow {
            id: node(row_id),
            properties: TableRowProperties::default(),
            cells: vec![TableCell {
                id: node(cell_id),
                properties: TableCellProperties::default(),
                blocks: vec![BlockNode::Paragraph(Paragraph {
                    id: node(paragraph_id),
                    properties: ParagraphProperties::default().into(),
                    inlines,
                })],
            }],
        }],
    }))
}

fn assert_top_bottom_barrier(fragment: &BlockFragment, expected: Twip) {
    let BlockFragment::Paragraph { lines, .. } = fragment else {
        panic!("expected a paragraph fragment");
    };
    assert!(lines.lines.len() >= 2, "barrier plus visible text line");
    let barrier = &lines.lines[0];
    assert_eq!(barrier.height, expected);
    assert!(barrier.runs.is_empty());
    assert!(barrier.images.is_empty());
    assert!(barrier.text_boxes.is_empty());
    assert!(barrier.fields.is_empty());
    assert!(
        lines.lines[1]
            .runs
            .iter()
            .all(|run| run.origin.y.raw() >= expected.raw()),
        "visible text begins below the non-painting float exclusion"
    );
}

#[test]
fn top_and_bottom_reflow_coalesces_pictures_text_boxes_and_groups() {
    let (media_id, definitions) = media_defs();
    let text_box = InlineNode::TextBox(Box::new(TextBox {
        hyperlink: None,
        id: node(20),
        anchor: Some(top_bottom_anchor(0)),
        relative_height: None,
        extent: Some(Extent {
            width_emu: 127_000,
            height_emu: 200 * 635,
        }),
        fill: None,
        border: None,
        body_properties: Default::default(),
        blocks: vec![BlockNode::Paragraph(Paragraph {
            id: node(21),
            properties: ParagraphProperties::default().into(),
            inlines: vec![run(22, "inside")],
        })],
    }));
    let group_extent = Extent {
        width_emu: 127_000,
        height_emu: 300 * 635,
    };
    let group = InlineNode::Group(Box::new(WordprocessingGroup {
        hyperlink: None,
        id: node(30),
        anchor: Some(top_bottom_anchor(50)),
        relative_height: None,
        extent: group_extent,
        transform: GroupTransform {
            offset: PointEmu { x_emu: 0, y_emu: 0 },
            extent: group_extent,
            child_offset: PointEmu { x_emu: 0, y_emu: 0 },
            child_extent: group_extent,
            flip_h: false,
            flip_v: false,
            rotation: None,
        },
        children: vec![GroupChild::Shape(GroupShape {
            hyperlink: None,
            id: node(31),
            offset: PointEmu { x_emu: 0, y_emu: 0 },
            extent: group_extent,
            geometry: ShapeGeometry::Rectangle,
            preset: None,
            adjustments: Vec::new(),
            path: None,
            fill: None,
            stroke: None,
            flip_h: false,
            flip_v: false,
            rotation: None,
        })],
    }));
    let paragraph = BlockNode::Paragraph(Paragraph {
        id: node(10),
        properties: ParagraphProperties::default().into(),
        inlines: vec![
            run(11, "Body text"),
            top_bottom_drawing(12, media_id, 100, 20),
            text_box,
            group,
        ],
    });
    let document = Document::new(node(1), vec![paragraph], definitions).unwrap();

    let shaper = ParleyShaper::new();
    let galley = build_galley(&document, &shaper, config().content_area().size.width);

    assert_top_bottom_barrier(&galley[0], Twip(350));
    let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
        unreachable!();
    };
    assert_eq!(
        lines.lines.len(),
        2,
        "overlapping exclusions take their maximum instead of summing"
    );
}

#[test]
fn wrap_clearance_changes_invalidate_the_paragraph_cache() {
    let document = |height_twips| {
        let (media_id, definitions) = media_defs();
        let paragraph = BlockNode::Paragraph(Paragraph {
            id: node(10),
            properties: ParagraphProperties::default().into(),
            inlines: vec![
                run(11, "cached"),
                top_bottom_drawing(12, media_id, height_twips, 0),
            ],
        });
        Document::new(node(1), vec![paragraph], definitions).unwrap()
    };
    let first = document(100);
    let second = document(250);
    let shaper = ParleyShaper::new();
    let width = config().content_area().size.width;
    let mut cache = GalleyCache::default();
    let first_galley =
        build_galley_cached(&first, &shaper, width, &mut cache, &DirtySet::everything());
    assert_top_bottom_barrier(&first_galley[0], Twip(100));

    let second_galley = build_galley_cached(&second, &shaper, width, &mut cache, &DirtySet::new());
    assert_top_bottom_barrier(&second_galley[0], Twip(250));
    assert_eq!(
        second_galley,
        build_galley(&second, &shaper, width),
        "the cached path matches a fresh layout after exclusion geometry changes"
    );
}

#[test]
fn unsupported_wrap_frames_remain_flow_neutral() {
    let (media_id, definitions) = media_defs();
    let mut square = top_bottom_drawing(12, media_id, 1_440, 100);
    let InlineNode::AnchoredDrawing(square_drawing) = &mut square else {
        unreachable!();
    };
    square_drawing.anchor.wrap = WrapMode::Square;

    let mut page_relative = top_bottom_drawing(13, media_id, 1_440, 100);
    let InlineNode::AnchoredDrawing(page_relative_drawing) = &mut page_relative else {
        unreachable!();
    };
    page_relative_drawing.anchor.vertical.relative_from = VerticalAnchor::Page;
    let paragraph = BlockNode::Paragraph(Paragraph {
        id: node(10),
        properties: ParagraphProperties::default().into(),
        inlines: vec![run(11, "unchanged"), square, page_relative],
    });
    let document = Document::new(node(1), vec![paragraph], definitions).unwrap();

    let shaper = ParleyShaper::new();
    let galley = build_galley(&document, &shaper, config().content_area().size.width);
    let BlockFragment::Paragraph { lines, .. } = &galley[0] else {
        panic!("expected a paragraph");
    };
    assert_eq!(
        lines.lines.len(),
        1,
        "square and page-relative exclusions wait for the bounded page-level pass"
    );
}

#[test]
fn top_and_bottom_reflow_survives_an_inline_wrapper_inside_a_table_cell() {
    use casual_doc_model::v1::{Hyperlink, HyperlinkTarget, InternalTarget};

    let (media_id, definitions) = media_defs();
    let wrapped_float = InlineNode::Hyperlink(Box::new(Hyperlink {
        id: node(45),
        target: HyperlinkTarget::Internal(InternalTarget {
            anchor: "bookmark".to_owned(),
        }),
        tooltip: None,
        inlines: vec![top_bottom_drawing(46, media_id, 1_440, 100)],
    }));
    let table = one_cell_table(40, 41, 42, 43, vec![run(44, "cell text"), wrapped_float]);
    let document = Document::new(node(1), vec![table], definitions).unwrap();

    let shaper = ParleyShaper::new();
    let galley = build_galley(&document, &shaper, config().content_area().size.width);
    let BlockFragment::TableRow { cells, .. } = &galley[0] else {
        panic!("expected the table row");
    };

    assert_top_bottom_barrier(&cells[0].blocks[0], Twip(1_540));
}

#[test]
fn top_and_bottom_reflow_repeats_in_both_headers_and_footers() {
    let (media_id, definitions) = media_defs();
    let header_block = BlockNode::Paragraph(Paragraph {
        id: node(60),
        properties: ParagraphProperties::default().into(),
        inlines: vec![run(61, "header"), top_bottom_drawing(62, media_id, 200, 20)],
    });
    let footer_block = BlockNode::Paragraph(Paragraph {
        id: node(70),
        properties: ParagraphProperties::default().into(),
        inlines: vec![run(71, "footer"), top_bottom_drawing(72, media_id, 300, 30)],
    });
    let body = vec![
        BlockNode::Paragraph(Paragraph {
            id: node(80),
            properties: ParagraphProperties::default().into(),
            inlines: vec![run(81, "page one")],
        }),
        BlockNode::Paragraph(Paragraph {
            id: node(82),
            properties: ParagraphProperties {
                page_break_before: Some(true),
                ..ParagraphProperties::default()
            }
            .into(),
            inlines: vec![run(83, "page two")],
        }),
    ];
    let document = Document::new(node(1), body, definitions).unwrap();
    let shaper = ParleyShaper::new();
    let header = flow_header_footer(
        &document,
        &[header_block],
        &shaper,
        config().content_area().size.width,
    );
    let footer = flow_header_footer(
        &document,
        &[footer_block],
        &shaper,
        config().content_area().size.width,
    );
    assert_top_bottom_barrier(&header[0], Twip(220));
    assert_top_bottom_barrier(&footer[0], Twip(330));

    let running = RunningContent {
        header: RunningBand {
            default: header,
            ..RunningBand::default()
        },
        footer: RunningBand {
            default: footer,
            ..RunningBand::default()
        },
        ..RunningContent::default()
    };
    let mut cfg = config();
    let (header_height, footer_height) = running.band_heights();
    cfg.header_height = header_height;
    cfg.footer_height = footer_height;
    let galley = build_galley(&document, &shaper, cfg.content_area().size.width);
    let mut layout = paginate(&galley, &cfg);
    place_running_content(&mut layout, &running, &cfg);

    assert_eq!(layout.pages.len(), 2);
    for page in &layout.pages {
        assert_top_bottom_barrier(&page.header[0].fragment, Twip(220));
        assert_top_bottom_barrier(&page.footer[0].fragment, Twip(330));
    }
}

#[test]
fn a_float_in_a_body_table_cell_uses_the_nested_paragraph_on_its_actual_page() {
    let (media_id, definitions) = media_defs();
    let first = BlockNode::Paragraph(Paragraph {
        id: node(10),
        properties: ParagraphProperties::default().into(),
        inlines: vec![run(11, "page one")],
    });
    let page_two = BlockNode::Paragraph(Paragraph {
        id: node(20),
        properties: ParagraphProperties {
            page_break_before: Some(true),
            ..ParagraphProperties::default()
        }
        .into(),
        inlines: vec![run(21, "page two")],
    });
    let table = one_cell_table(
        30,
        31,
        32,
        40,
        vec![run(41, "cell"), anchored_at_paragraph(42, media_id)],
    );
    let doc = Document::new(node(1), vec![first, page_two, table], definitions).unwrap();

    let shaper = ParleyShaper::new();
    let cfg = config();
    let galley = build_galley(&doc, &shaper, cfg.content_area().size.width);
    let mut layout = paginate(&galley, &cfg);
    place_floats(&mut layout, &doc, &shaper, &cfg);

    assert_eq!(layout.pages.len(), 2);
    assert!(
        layout.pages[0].anchored.is_empty(),
        "the nested float must not fall back to page zero"
    );
    assert_eq!(layout.pages[1].anchored.len(), 1);

    let placed_row = layout.pages[1]
        .placed
        .iter()
        .find(|placed| matches!(placed.fragment, BlockFragment::TableRow { id, .. } if id == node(31)))
        .expect("the table row is placed on page two");
    let BlockFragment::TableRow { cells, .. } = &placed_row.fragment else {
        unreachable!()
    };
    let row_height = placed_row.fragment.height();
    let expected_y =
        placed_row.rect.origin.y + cells[0].content_y_offset(cells[0].box_height(row_height));
    assert_eq!(
        layout.pages[1].anchored[0].rect.origin.y, expected_y,
        "paragraph-relative placement includes the table cell content offset"
    );
}

#[test]
fn a_column_relative_float_in_a_nested_cell_uses_the_containing_flow_column() {
    let (media_id, definitions) = media_defs();
    let inner = one_cell_table(
        600,
        601,
        602,
        603,
        vec![
            run(604, "nested cell"),
            anchored_at_column_right(605, media_id),
        ],
    );
    let outer = BlockNode::Table(Box::new(Table {
        id: node(500),
        grid: vec![GridColumn {
            width_twips: Some(4_000),
        }],
        grid_change: None,
        properties: TableProperties::default(),
        rows: vec![TableRow {
            id: node(501),
            properties: TableRowProperties::default(),
            cells: vec![TableCell {
                id: node(502),
                properties: TableCellProperties::default(),
                blocks: vec![inner],
            }],
        }],
    }));
    let doc = Document::new(node(1), vec![outer], definitions).unwrap();

    let shaper = ParleyShaper::new();
    let cfg = config();
    let galley = build_galley(&doc, &shaper, cfg.content_area().size.width);
    let mut layout = paginate(&galley, &cfg);
    let placed_outer_row = layout.pages[0]
        .placed
        .iter_mut()
        .find(|placed| {
            matches!(
                placed.fragment,
                BlockFragment::TableRow { id, .. } if id == node(501)
            )
        })
        .expect("the outer row");
    // Simulate the outer row being placed in a 2,000-twip newspaper column at
    // x=5,000. The nested cell itself is not the `column` reference frame.
    placed_outer_row.rect.origin.x = Twip(5_000);
    placed_outer_row.rect.size.width = Twip(2_000);

    place_floats(&mut layout, &doc, &shaper, &cfg);

    assert_eq!(layout.pages[0].anchored.len(), 1);
    assert_eq!(
        layout.pages[0].anchored[0].rect.origin.x,
        Twip(6_000),
        "right alignment uses column right (7,000) minus the 1,000-twip float"
    );
}

#[test]
fn a_float_in_a_bottom_aligned_vertical_merge_uses_the_full_merged_box() {
    let (media_id, definitions) = media_defs();
    let exact = TableRowProperties {
        height: RowHeight {
            value_twips: Some(1_000),
            rule: Some(HeightRule::Exact),
        },
        ..TableRowProperties::default()
    };
    let table = BlockNode::Table(Box::new(Table {
        id: node(500),
        grid: vec![GridColumn {
            width_twips: Some(4_000),
        }],
        grid_change: None,
        properties: TableProperties::default(),
        rows: vec![
            TableRow {
                id: node(501),
                properties: exact.clone(),
                cells: vec![TableCell {
                    id: node(510),
                    properties: TableCellProperties {
                        vertical_merge: Some(VerticalMerge::Restart),
                        vertical_alignment: Some(CellVerticalAlignment::Bottom),
                        ..TableCellProperties::default()
                    },
                    blocks: vec![BlockNode::Paragraph(Paragraph {
                        id: node(511),
                        properties: ParagraphProperties::default().into(),
                        inlines: vec![run(512, "merged"), anchored_at_paragraph(513, media_id)],
                    })],
                }],
            },
            TableRow {
                id: node(502),
                properties: exact,
                cells: vec![TableCell {
                    id: node(520),
                    properties: TableCellProperties {
                        vertical_merge: Some(VerticalMerge::Continue),
                        ..TableCellProperties::default()
                    },
                    blocks: vec![BlockNode::Paragraph(Paragraph {
                        id: node(521),
                        properties: ParagraphProperties::default().into(),
                        inlines: Vec::new(),
                    })],
                }],
            },
        ],
    }));
    let doc = Document::new(node(1), vec![table], definitions).unwrap();

    let shaper = ParleyShaper::new();
    let cfg = config();
    let galley = build_galley(&doc, &shaper, cfg.content_area().size.width);
    let mut layout = paginate(&galley, &cfg);
    place_floats(&mut layout, &doc, &shaper, &cfg);

    assert_eq!(layout.pages.len(), 1);
    assert_eq!(layout.pages[0].anchored.len(), 1);
    let placed_row = &layout.pages[0].placed[0];
    let BlockFragment::TableRow { cells, .. } = &placed_row.fragment else {
        unreachable!()
    };
    let row_height = placed_row.fragment.height();
    let cell_height = cells[0].box_height(row_height);
    assert_eq!(cell_height, Twip(2_000));
    let expected_y = placed_row.rect.origin.y + cells[0].content_y_offset(cell_height);
    assert_eq!(layout.pages[0].anchored[0].rect.origin.y, expected_y);
    assert!(
        expected_y.raw() > placed_row.rect.bottom().raw(),
        "bottom alignment is resolved against both covered rows, not row one"
    );
}

#[test]
fn a_float_in_a_header_table_cell_is_discovered_and_repeated_per_page() {
    let (media_id, mut definitions) = media_defs();
    let header_table = one_cell_table(
        300,
        301,
        302,
        310,
        vec![
            run(311, "header cell"),
            anchored_at_paragraph(312, media_id),
        ],
    );
    definitions.headers.insert(
        HeaderFooterId::new(node(320)),
        ModelHeaderFooter {
            blocks: vec![header_table.clone()],
        },
    );
    let body = vec![
        BlockNode::Paragraph(Paragraph {
            id: node(10),
            properties: ParagraphProperties::default().into(),
            inlines: vec![run(11, "page one")],
        }),
        BlockNode::Paragraph(Paragraph {
            id: node(20),
            properties: ParagraphProperties {
                page_break_before: Some(true),
                ..ParagraphProperties::default()
            }
            .into(),
            inlines: vec![run(21, "page two")],
        }),
    ];
    let doc = Document::new(node(1), body, definitions).unwrap();

    let shaper = ParleyShaper::new();
    let mut cfg = config();
    let header = flow_header_footer(
        &doc,
        &[header_table],
        &shaper,
        cfg.content_area().size.width,
    );
    let running = RunningContent {
        header: RunningBand {
            default: header,
            ..RunningBand::default()
        },
        ..RunningContent::default()
    };
    cfg.header_height = running.band_heights().0;
    let galley = build_galley(&doc, &shaper, cfg.content_area().size.width);
    let mut layout = paginate(&galley, &cfg);
    place_running_content(&mut layout, &running, &cfg);
    place_floats(&mut layout, &doc, &shaper, &cfg);

    assert_eq!(layout.pages.len(), 2);
    for page in &layout.pages {
        assert_eq!(
            page.anchored.len(),
            1,
            "the selected header table float repeats on every page"
        );
        let placed_row = page.header.first().expect("the header table row");
        let BlockFragment::TableRow { cells, .. } = &placed_row.fragment else {
            panic!("expected a header table row");
        };
        let row_height = placed_row.fragment.height();
        let expected_y =
            placed_row.rect.origin.y + cells[0].content_y_offset(cells[0].box_height(row_height));
        assert_eq!(page.anchored[0].rect.origin.y, expected_y);
    }
}

#[test]
fn floating_text_box_body_properties_apply_in_both_headers_and_footers() {
    let (_media_id, mut definitions) = media_defs();
    let header_box = InlineNode::TextBox(Box::new(TextBox {
        hyperlink: None,
        id: node(410),
        anchor: Some(paragraph_anchor()),
        relative_height: Some(10),
        extent: Some(Extent {
            width_emu: 1_200 * 635,
            height_emu: 800 * 635,
        }),
        fill: None,
        border: None,
        body_properties: TextBoxBodyProperties {
            insets: TextBoxInsets {
                left_emu: 40 * 635,
                top_emu: 50 * 635,
                right_emu: 60 * 635,
                bottom_emu: 70 * 635,
            },
            vertical_anchor: TextBoxVerticalAnchor::Center,
            horizontal_overflow: TextBoxHorizontalOverflow::Clip,
            vertical_overflow: TextBoxVerticalOverflow::Clip,
            auto_fit: TextBoxAutoFit::None,
            // Not what this fixture asserts: the flow direction and the text's own
            // rotation have their own guards, so they are deliberately unstated here
            // rather than defaulted to quiet the compiler.
            vertical: casual_doc_model::v1::TextVertical::Horizontal,
            text_rotation: None,
        },
        blocks: vec![BlockNode::Paragraph(Paragraph {
            id: node(411),
            properties: ParagraphProperties::default().into(),
            inlines: vec![run(412, "header box")],
        })],
    }));
    let footer_box = InlineNode::TextBox(Box::new(TextBox {
        hyperlink: None,
        id: node(420),
        anchor: Some(paragraph_anchor()),
        relative_height: Some(20),
        extent: Some(Extent {
            width_emu: 1_200 * 635,
            height_emu: 635,
        }),
        fill: None,
        border: None,
        body_properties: TextBoxBodyProperties {
            insets: TextBoxInsets {
                left_emu: 80 * 635,
                top_emu: 90 * 635,
                right_emu: 100 * 635,
                bottom_emu: 110 * 635,
            },
            auto_fit: TextBoxAutoFit::Shape,
            ..TextBoxBodyProperties::default()
        },
        blocks: vec![BlockNode::Paragraph(Paragraph {
            id: node(421),
            properties: ParagraphProperties::default().into(),
            inlines: vec![run(422, "footer box")],
        })],
    }));
    let header_block = BlockNode::Paragraph(Paragraph {
        id: node(400),
        properties: ParagraphProperties::default().into(),
        inlines: vec![header_box],
    });
    let footer_block = BlockNode::Paragraph(Paragraph {
        id: node(401),
        properties: ParagraphProperties::default().into(),
        inlines: vec![footer_box],
    });
    definitions.headers.insert(
        HeaderFooterId::new(node(430)),
        ModelHeaderFooter {
            blocks: vec![header_block.clone()],
        },
    );
    definitions.footers.insert(
        HeaderFooterId::new(node(431)),
        ModelHeaderFooter {
            blocks: vec![footer_block.clone()],
        },
    );
    let body = vec![
        BlockNode::Paragraph(Paragraph {
            id: node(440),
            properties: ParagraphProperties::default().into(),
            inlines: vec![run(441, "page one")],
        }),
        BlockNode::Paragraph(Paragraph {
            id: node(442),
            properties: ParagraphProperties {
                page_break_before: Some(true),
                ..ParagraphProperties::default()
            }
            .into(),
            inlines: vec![run(443, "page two")],
        }),
    ];
    let document = Document::new(node(1), body, definitions).unwrap();
    let shaper = ParleyShaper::new();
    let mut cfg = config();
    let running = RunningContent {
        header: RunningBand {
            default: flow_header_footer(
                &document,
                &[header_block],
                &shaper,
                cfg.content_area().size.width,
            ),
            ..RunningBand::default()
        },
        footer: RunningBand {
            default: flow_header_footer(
                &document,
                &[footer_block],
                &shaper,
                cfg.content_area().size.width,
            ),
            ..RunningBand::default()
        },
        ..RunningContent::default()
    };
    let (header_height, footer_height) = running.band_heights();
    cfg.header_height = header_height;
    cfg.footer_height = footer_height;
    let galley = build_galley(&document, &shaper, cfg.content_area().size.width);
    let mut layout = paginate(&galley, &cfg);
    place_running_content(&mut layout, &running, &cfg);
    place_floats(&mut layout, &document, &shaper, &cfg);

    assert_eq!(layout.pages.len(), 2);
    for page in &layout.pages {
        let text_boxes: Vec<_> = page
            .anchored
            .iter()
            .filter_map(|anchor| match &anchor.content {
                casual_doc_layout::page::AnchorContent::TextBox { content_layout, .. } => {
                    Some((anchor, content_layout))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            text_boxes.len(),
            2,
            "both the header and footer text boxes repeat on every page"
        );
        let header = text_boxes
            .iter()
            .find(|(_, content)| content.origin.x == Twip(40))
            .expect("header box");
        assert_eq!(header.0.rect.size.height, Twip(800));
        assert!(header.1.origin.y.raw() > 50);
        assert!(header.1.clip_horizontal && header.1.clip_vertical);
        let footer = text_boxes
            .iter()
            .find(|(_, content)| content.origin.x == Twip(80))
            .expect("footer box");
        assert!(
            footer.0.rect.size.height.raw() > 1,
            "shape autofit grows the footer box around its content"
        );

        let list = compose_page(page);
        assert!(
            list.items
                .iter()
                .any(|item| matches!(item, PaintItem::PushClip(_))),
            "the header text-box clip reaches the shared page display list"
        );
    }
}

#[test]
fn grouped_text_box_uses_body_properties_and_shape_autofit() {
    let (_media_id, definitions) = media_defs();
    let group = InlineNode::Group(Box::new(WordprocessingGroup {
        hyperlink: None,
        id: node(500),
        anchor: Some(page_anchor(0, 0)),
        relative_height: Some(1),
        extent: Extent {
            width_emu: 2_000 * 635,
            height_emu: 2_000 * 635,
        },
        transform: GroupTransform {
            offset: PointEmu { x_emu: 0, y_emu: 0 },
            extent: Extent {
                width_emu: 2_000 * 635,
                height_emu: 2_000 * 635,
            },
            child_offset: PointEmu { x_emu: 0, y_emu: 0 },
            child_extent: Extent {
                width_emu: 2_000 * 635,
                height_emu: 2_000 * 635,
            },
            flip_h: false,
            flip_v: false,
            rotation: None,
        },
        children: vec![GroupChild::TextBox(GroupTextBox {
            hyperlink: None,
            id: node(501),
            offset: PointEmu { x_emu: 0, y_emu: 0 },
            extent: Extent {
                width_emu: 1_000 * 635,
                height_emu: 635,
            },
            // A PLAIN text box: this fixture measures where the body content
            // lands inside the box, not what is drawn behind it.
            geometry: ShapeGeometry::Rectangle,
            preset: None,
            adjustments: Vec::new(),
            blocks: vec![BlockNode::Paragraph(Paragraph {
                id: node(502),
                properties: ParagraphProperties::default().into(),
                inlines: vec![run(503, "grouped box")],
            })],
            fill: None,
            border: None,
            body_properties: TextBoxBodyProperties {
                insets: TextBoxInsets {
                    left_emu: 30 * 635,
                    top_emu: 40 * 635,
                    right_emu: 50 * 635,
                    bottom_emu: 60 * 635,
                },
                auto_fit: TextBoxAutoFit::Shape,
                ..TextBoxBodyProperties::default()
            },
            flip_h: false,
            flip_v: false,
            rotation: None,
        })],
    }));
    let body = vec![BlockNode::Paragraph(Paragraph {
        id: node(510),
        properties: ParagraphProperties::default().into(),
        inlines: vec![group],
    })];
    let document = Document::new(node(1), body, definitions).unwrap();
    let shaper = ParleyShaper::new();
    let cfg = config();
    let galley = build_galley(&document, &shaper, cfg.content_area().size.width);
    let mut layout = paginate(&galley, &cfg);
    place_floats(&mut layout, &document, &shaper, &cfg);

    let grouped = layout.pages[0]
        .anchored
        .first()
        .expect("the grouped text box is placed");
    assert!(grouped.rect.size.height.raw() > 1);
    let casual_doc_layout::page::AnchorContent::TextBox { content_layout, .. } = &grouped.content
    else {
        panic!("expected grouped text-box content");
    };
    assert_eq!(content_layout.origin.x, Twip(30));
    assert_eq!(content_layout.origin.y, Twip(40));
}

#[test]
fn a_header_float_uses_the_section_recorded_on_its_page() {
    let (media_id, mut definitions) = media_defs();
    let section = SectionBoundary {
        id: SectionId::new(node(900)),
        page_size: PageSize {
            width_twips: 20_000,
            height_twips: 10_000,
        },
        page_margins: PageMargins {
            top_twips: 2_000,
            bottom_twips: 500,
            start_twips: 3_000,
            end_twips: 1_000,
            header_twips: None,
            footer_twips: None,
            gutter_twips: None,
        },
        columns: SectionColumns {
            count: 1,
            space_twips: None,
            separator: None,
            equal_width: None,
            columns: Vec::new(),
        },
        headers: Vec::new(),
        footers: Vec::new(),
        section_type: None,
        title_page: None,
        vertical_alignment: None,
        page_numbering: Default::default(),
        doc_grid: Default::default(),
        orientation: None,
        paper_source: Default::default(),
        page_borders: Default::default(),
        line_numbering: Default::default(),
        watermark: None,
        footnote_props: Default::default(),
        endnote_props: Default::default(),
        text_direction: None,
        bidi: false,
        section_change: None,
    };
    let section_id = section.id;
    definitions.sections = vec![section];
    let header_block = BlockNode::Paragraph(Paragraph {
        id: node(910),
        properties: ParagraphProperties::default().into(),
        inlines: vec![
            run(911, "section header"),
            anchored_at_page_right(912, media_id),
        ],
    });
    definitions.headers.insert(
        HeaderFooterId::new(node(920)),
        ModelHeaderFooter {
            blocks: vec![header_block.clone()],
        },
    );
    let body = vec![BlockNode::Paragraph(Paragraph {
        id: node(10),
        properties: ParagraphProperties::default().into(),
        inlines: vec![run(11, "body")],
    })];
    let doc = Document::new(node(1), body, definitions).unwrap();

    let shaper = ParleyShaper::new();
    let mut cfg = config();
    let header = flow_header_footer(
        &doc,
        &[header_block],
        &shaper,
        cfg.content_area().size.width,
    );
    let running = RunningContent {
        header: RunningBand {
            default: header,
            ..RunningBand::default()
        },
        ..RunningContent::default()
    };
    cfg.header_height = running.band_heights().0;
    let galley = build_galley(&doc, &shaper, cfg.content_area().size.width);
    let mut layout = paginate(&galley, &cfg);
    place_running_content(&mut layout, &running, &cfg);
    // The running-content selector records this page as belonging to the later
    // section even though the manual paginator config is the first-section one.
    layout.pages[0].section = section_id;
    place_floats(&mut layout, &doc, &shaper, &cfg);

    assert_eq!(layout.pages[0].anchored.len(), 1);
    assert_eq!(
        layout.pages[0].anchored[0].rect.origin.x,
        Twip(19_000),
        "page-right alignment uses the recorded section width (20,000)"
    );
}

#[test]
fn a_floating_text_box_places_at_its_anchor_not_inline() {
    let (_media, definitions) = media_defs();
    // A paragraph carrying body text plus a FLOATING text box (anchor set) at page
    // offset (1440, 2880) twips, 2x1 inch, white fill, and a 30-twip outline.
    let float = InlineNode::TextBox(Box::new(TextBox {
        hyperlink: None,
        id: node(20),
        anchor: Some(page_anchor(914_400, 1_828_800)),
        relative_height: Some(100),
        extent: Some(Extent {
            width_emu: 1_828_800,
            height_emu: 914_400,
        }),
        fill: Some(Fill::Solid(Rgba {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        })),
        border: Some(ShapeStroke {
            color: Rgba {
                r: 10,
                g: 20,
                b: 30,
                a: 255,
            },
            width_emu: 19_050,
            dash: None,
            head_end: None,
            tail_end: None,
        }),
        body_properties: Default::default(),
        blocks: vec![BlockNode::Paragraph(Paragraph {
            id: node(21),
            properties: ParagraphProperties::default().into(),
            inlines: vec![run(22, "Powered by")],
        })],
    }));
    let para = BlockNode::Paragraph(Paragraph {
        id: node(10),
        properties: ParagraphProperties::default().into(),
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
        PaintItem::Shape {
            geometry: DisplayShapeGeometry::Rect { rect },
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
        flip_h: false,
        flip_v: false,
        rotation: None,
    };
    let rect = |id: u64, off: i64| {
        GroupChild::Shape(GroupShape {
            hyperlink: None,
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
            preset: None,
            adjustments: Vec::new(),
            path: None,
            fill: Some(Fill::Solid(Rgba {
                r: 200,
                g: 200,
                b: 200,
                a: 255,
            })),
            stroke: None,
            flip_h: false,
            flip_v: false,
            rotation: None,
        })
    };
    let group = InlineNode::Group(Box::new(WordprocessingGroup {
        hyperlink: None,
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
                hyperlink: None,
                opacity: None,
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
                crop: None,
                border: None,
                flip_h: false,
                flip_v: false,
                rotation: None,
            }),
            rect(33, 200_000),
        ],
    }));
    let para = BlockNode::Paragraph(Paragraph {
        id: node(10),
        properties: ParagraphProperties::default().into(),
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
            PaintItem::Shape {
                geometry: DisplayShapeGeometry::Rect { .. },
                fill: Some(DisplayFill::Solid(c)),
                ..
            } if *c
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

#[test]
fn ellipse_and_rounded_rectangle_reach_distinct_display_primitives() {
    let group_extent = Extent {
        width_emu: 1_828_800,
        height_emu: 914_400,
    };
    let child_extent = Extent {
        width_emu: 914_400,
        height_emu: 914_400,
    };
    let shape = |id, x_emu, geometry, adjustments| {
        GroupChild::Shape(GroupShape {
            hyperlink: None,
            id: node(id),
            offset: PointEmu { x_emu, y_emu: 0 },
            extent: child_extent,
            geometry,
            preset: None,
            adjustments,
            path: None,
            fill: Some(Fill::Solid(Rgba {
                r: 20,
                g: 80,
                b: 160,
                a: 255,
            })),
            stroke: None,
            flip_h: false,
            flip_v: false,
            rotation: None,
        })
    };
    let group = InlineNode::Group(Box::new(WordprocessingGroup {
        hyperlink: None,
        id: node(50),
        anchor: Some(page_anchor(914_400, 914_400)),
        relative_height: Some(9),
        extent: group_extent,
        transform: GroupTransform {
            offset: PointEmu { x_emu: 0, y_emu: 0 },
            extent: group_extent,
            child_offset: PointEmu { x_emu: 0, y_emu: 0 },
            child_extent: group_extent,
            flip_h: false,
            flip_v: false,
            rotation: None,
        },
        children: vec![
            shape(51, 0, ShapeGeometry::Ellipse, Vec::new()),
            shape(
                52,
                914_400,
                ShapeGeometry::RoundRectangle,
                vec![ShapeAdjustment {
                    name: "adj".to_owned(),
                    formula: "val 25000".to_owned(),
                }],
            ),
        ],
    }));
    let paragraph = BlockNode::Paragraph(Paragraph {
        id: node(10),
        properties: ParagraphProperties::default().into(),
        inlines: vec![run(11, "Body"), group],
    });
    let document = Document::new(node(1), vec![paragraph], Definitions::default()).unwrap();

    let shaper = ParleyShaper::new();
    let cfg = config();
    let galley = build_galley(&document, &shaper, cfg.content_area().size.width);
    let mut layout = paginate(&galley, &cfg);
    place_floats(&mut layout, &document, &shaper, &cfg);

    let anchored = &layout.pages[0].anchored;
    assert_eq!(anchored.len(), 2);
    assert!(matches!(anchored[0].content, AnchorContent::Ellipse { .. }));
    assert_eq!(
        corner_radius(&anchored[1].content),
        Twip(360),
        "adj 25000 of a 1440-twip side"
    );

    let list = compose_page(&layout.pages[0]);
    assert!(list.items.iter().any(|item| matches!(
        item,
        PaintItem::Shape {
            geometry: DisplayShapeGeometry::Ellipse { .. },
            ..
        }
    )));
    // The rounded rectangle reaches the display list as the curved path the
    // standard defines — four arcs, each a cubic — not as a box.
    assert!(list.items.iter().any(|item| matches!(
        item,
        PaintItem::Shape {
            geometry: DisplayShapeGeometry::Path { commands, .. },
            ..
        } if commands
            .iter()
            .filter(|command| matches!(command, casual_doc_layout::display::PathCommand::CubicTo { .. }))
            .count()
            == 4
    )));
}

#[test]
fn angular_presets_reach_exact_polygon_display_primitives() {
    let group_extent = Extent {
        width_emu: 3 * 914_400,
        height_emu: 914_400,
    };
    let child_extent = Extent {
        width_emu: 914_400,
        height_emu: 914_400,
    };
    let shape = |id, x_emu, geometry| {
        GroupChild::Shape(GroupShape {
            hyperlink: None,
            id: node(id),
            offset: PointEmu { x_emu, y_emu: 0 },
            extent: child_extent,
            geometry,
            preset: None,
            adjustments: Vec::new(),
            path: None,
            fill: Some(Fill::Solid(Rgba {
                r: 60,
                g: 120,
                b: 180,
                a: 255,
            })),
            stroke: None,
            flip_h: false,
            flip_v: false,
            rotation: None,
        })
    };
    let group = InlineNode::Group(Box::new(WordprocessingGroup {
        hyperlink: None,
        id: node(70),
        anchor: Some(page_anchor(914_400, 914_400)),
        relative_height: Some(10),
        extent: group_extent,
        transform: GroupTransform {
            offset: PointEmu { x_emu: 0, y_emu: 0 },
            extent: group_extent,
            child_offset: PointEmu { x_emu: 0, y_emu: 0 },
            child_extent: group_extent,
            flip_h: false,
            flip_v: false,
            rotation: None,
        },
        children: vec![
            shape(71, 0, ShapeGeometry::Triangle),
            shape(72, 914_400, ShapeGeometry::RightTriangle),
            shape(73, 2 * 914_400, ShapeGeometry::Diamond),
        ],
    }));
    let paragraph = BlockNode::Paragraph(Paragraph {
        id: node(10),
        properties: ParagraphProperties::default().into(),
        inlines: vec![run(11, "Body"), group],
    });
    let document = Document::new(node(1), vec![paragraph], Definitions::default()).unwrap();

    let shaper = ParleyShaper::new();
    let cfg = config();
    let galley = build_galley(&document, &shaper, cfg.content_area().size.width);
    let mut layout = paginate(&galley, &cfg);
    place_floats(&mut layout, &document, &shaper, &cfg);

    let polygons: Vec<Vec<Point>> = layout.pages[0]
        .anchored
        .iter()
        .filter_map(|anchor| match &anchor.content {
            AnchorContent::Path { paths, .. } => Some(first_path(paths).0),
            _ => None,
        })
        .collect();
    assert_eq!(polygons.len(), 3);
    assert_same_outline(
        &polygons[0],
        &[
            Point::new(Twip(2_160), Twip(1_440)),
            Point::new(Twip(2_880), Twip(2_880)),
            Point::new(Twip(1_440), Twip(2_880)),
        ],
    );
    assert_same_outline(
        &polygons[1],
        &[
            Point::new(Twip(2_880), Twip(1_440)),
            Point::new(Twip(4_320), Twip(2_880)),
            Point::new(Twip(2_880), Twip(2_880)),
        ],
    );
    assert_same_outline(
        &polygons[2],
        &[
            Point::new(Twip(5_040), Twip(1_440)),
            Point::new(Twip(5_760), Twip(2_160)),
            Point::new(Twip(5_040), Twip(2_880)),
            Point::new(Twip(4_320), Twip(2_160)),
        ],
    );

    let list = compose_page(&layout.pages[0]);
    assert_eq!(
        list.items
            .iter()
            .filter(|item| matches!(
                item,
                PaintItem::Shape {
                    geometry: DisplayShapeGeometry::Path { .. },
                    ..
                }
            ))
            .count(),
        3
    );
}

// --- Custom shape geometry (`a:custGeom`) — docs/119, `109` FID-G-01 --------

/// A custom path resolves to a polyline inside the shape's box, and the two
/// An untyped preset token resolves its real outline from the committed ECMA-376
/// table instead of painting as its bounding rectangle (`109` FID-L-04).
///
/// This is the row that made every Word arrow, callout, banner and flowchart symbol
/// draw as a box: `ShapeGeometry` types 22 of the 187 presets and everything else fell
/// through to a rectangle. The assertion is on the OUTLINE rather than on a type,
/// because "it is a path now" would pass for a path that happened to be four corners —
/// the point is that the shape has the vertex count its recipe produces and visits
/// points the bounding box does not.
///
/// With `a:arcTo` evaluated, an arc-bearing token resolves here too, so the only
/// reason left to fall back is a token the table does not carry.
#[test]
fn an_untyped_preset_resolves_its_outline_from_the_table() {
    use casual_doc_layout::page::AnchorContent;

    let content = |token: &str| {
        only_anchor_content(&single_child_group_document(GroupChild::Shape(
            GroupShape {
                hyperlink: None,
                id: node(91),
                offset: PointEmu { x_emu: 0, y_emu: 0 },
                extent: Extent {
                    width_emu: 914_400,
                    height_emu: 914_400,
                },
                geometry: ShapeGeometry::Other,
                preset: Some(token.to_owned()),
                adjustments: Vec::new(),
                path: None,
                fill: None,
                stroke: None,
                flip_h: false,
                flip_v: false,
                rotation: None,
            },
        )))
    };

    // `plus` is a twelve-vertex cross, arc-free, and nothing like its bounding box.
    let AnchorContent::Path { paths, .. } = content("plus") else {
        panic!("a cross must not paint as a rectangle");
    };
    let (points, closed) = first_path(&paths);
    assert_eq!(points.len(), 12, "a plus has twelve vertices");
    assert!(closed, "and it is a closed outline");
    // The box is 1440 twips square at (1440, 1440). A cross visits its edge MIDPOINTS,
    // which a rectangle never does — that is the assertion a type check cannot make.
    let xs: Vec<i32> = points.iter().map(|point| point.x.raw()).collect();
    let ys: Vec<i32> = points.iter().map(|point| point.y.raw()).collect();
    assert!(
        xs.iter().any(|x| *x > 1_440 && *x < 2_880),
        "a vertex strictly inside the horizontal span: {xs:?}"
    );
    assert!(
        ys.iter().any(|y| *y > 1_440 && *y < 2_880),
        "and inside the vertical span: {ys:?}"
    );

    // A preset needing arcs resolves as well, now that `a:arcTo` evaluates — this
    // assertion used to be that it fell back to its bounding rectangle. `ellipse`
    // reaches the table only as an untyped token like this; a shape whose model
    // geometry is `Ellipse` is still answered by the typed primitive.
    let AnchorContent::Path { paths, .. } = content("ellipse") else {
        panic!("an arc-bearing preset must not paint as a rectangle either");
    };
    let (points, closed) = first_path(&paths);
    assert!(closed, "an ellipse is a closed outline");
    // The box is the same 1440 twips square at (1440, 1440), so this is the circle of
    // radius 720 about (2160, 2160), drawn as a move to its leftmost point and four
    // quarter-turn cubics. Its on-curve points are the box's edge MIDPOINTS — the
    // same assertion the cross above makes, and one a rectangle cannot pass.
    let walk: Vec<(i32, i32)> = points
        .iter()
        .map(|point| (point.x.raw(), point.y.raw()))
        .collect();
    assert_eq!(
        walk,
        vec![
            (1_440, 2_160),
            (2_160, 1_440),
            (2_880, 2_160),
            (2_160, 2_880),
            (1_440, 2_160),
        ],
        "left, then clockwise through top, right and bottom, back to the left"
    );
    // An unknown token is the only thing that still falls back.
    assert!(matches!(
        content("notAShapeAtAll"),
        AnchorContent::Rectangle { .. }
    ));
}

/// A shape with no `spPr` fill resolves its appearance from the theme style its
/// `wps:style` names (`156` §6 row 0.2).
///
/// Word's Shape Styles gallery writes exactly this shape: no explicit fill or
/// outline, just `a:fillRef`/`a:lnRef` into the theme's format scheme with the colour
/// each substitutes for the entry's `a:phClr`. Those references were suppressed at
/// import and the scheme was retained only as opaque XML, so such a shape arrived with
/// no fill and no outline at all — it drew as an empty rectangle.
///
/// Resolution is at LAYOUT, so the model still says what the file said. The control
/// case matters as much as the resolved one: an explicit fill must still win, or a
/// shape that says `a:noFill` would be overridden by its own style reference.
#[test]
fn a_shape_with_no_explicit_fill_resolves_its_theme_style() {
    use casual_doc_layout::page::AnchorContent;
    use casual_doc_model::v1::DashStyle;

    let AnchorContent::Rectangle { fill, stroke } = place_themed_shape(1, 1, None) else {
        panic!("expected a rectangle");
    };
    assert_eq!(
        fill,
        // `plain` asserts the second half too: a themed fill files NO row under the
        // shape's id, so it must arrive with no `a:gradFill` geometry attached.
        Some(casual_doc_layout::page::AnchorFill::plain(
            casual_doc_model::v1::Fill::Solid(THEMED_GREEN)
        )),
        "the fillRef's colour is substituted for the entry's phClr"
    );
    let stroke = stroke.expect("the lnRef resolves an outline");
    assert_eq!(stroke.color, [255, 0, 0, 255], "the lnRef's colour");
    assert_eq!(stroke.width, Twip(10), "6350 EMU is 10 twips");
    assert_eq!(stroke.dash, DashStyle::Dash, "the entry's preset dash");

    // The control: an explicit fill is the shape's own statement and must win.
    let explicit = casual_doc_model::v1::Fill::Solid(Rgba {
        r: 1,
        g: 2,
        b: 3,
        a: 255,
    });
    let AnchorContent::Rectangle { fill, .. } = place_themed_shape(1, 1, Some(explicit.clone()))
    else {
        panic!("expected a rectangle");
    };
    assert_eq!(
        fill,
        Some(casual_doc_layout::page::AnchorFill::plain(explicit)),
        "an explicit spPr fill must not be overridden by the style reference"
    );
}

/// A path gradient's AUTHORED family and focus reach the display list.
///
/// `docs/156` §6 row 0.3's gradient half, at the altitude that was missing. The path
/// geometry has been imported, modeled and re-emitted since row 0.3's first commit,
/// and `Definitions::shape_fill_detail` held it under the shape's own id — but layout
/// never read that row, so `path="rect"`, `path="shape"` and `path="circle"` all
/// reached the rasteriser as the same `GradientKind::Radial` and all three painted as
/// concentric circles centred on the shape.
///
/// This drives the REAL pipeline — a `Definitions` row, the shared `GroupChild` walk
/// in `place_floats`, then `compose_page` — because a guard that builds a display-list
/// value directly proves the rasteriser works and proves nothing about whether layout
/// ever hands it that value. Both failure modes have happened here: a `compose` that
/// drops a field, and a group walk that never consults the side table.
///
/// The four families are asserted TOGETHER on purpose. A fixture carrying only
/// `path="rect"` cannot fail for the right reason: it would pass for any gradient at
/// all, including the old collapse. These four must produce four different display
/// values, which is exactly what the collapse could not do.
#[test]
fn a_path_gradients_authored_family_and_focus_reach_the_display_list() {
    use casual_doc_layout::display::{GradientKind as DisplayGradientKind, PaintItem};
    use casual_doc_model::v1::{GradientDetail, GradientPath, RelativeRect};

    // `l="25000" t="10000" r="25000" b="10000"` — a focus RECTANGLE, not the common
    // centre point, so the four edges can be told apart from each other and from the
    // `GradientFocus::CENTER` fallback below.
    let authored = RelativeRect {
        left: 25_000,
        top: 10_000,
        right: 25_000,
        bottom: 10_000,
    };
    let detail = |path: GradientPath| {
        Some(GradientDetail {
            path: Some(path),
            fill_to_rect: Some(authored),
            ..GradientDetail::default()
        })
    };

    let DisplayFill::Gradient(rect_gradient) = place_gradient_shape(
        casual_doc_model::v1::GradientKind::Radial,
        detail(GradientPath::Rect),
    ) else {
        panic!("expected a gradient fill");
    };
    match rect_gradient.kind {
        DisplayGradientKind::Path { path, focus } => {
            assert_eq!(
                path,
                GradientPath::Rect,
                "the authored `a:path path=\"rect\"` family reaches the display list"
            );
            // `ST_Percentage` (1/1000 of a percent) resolved to fractions of the box,
            // which is the unit the backends paint in.
            assert!(
                (focus.left - 0.25).abs() < 1e-4
                    && (focus.top - 0.10).abs() < 1e-4
                    && (focus.right - 0.25).abs() < 1e-4
                    && (focus.bottom - 0.10).abs() < 1e-4,
                "a:fillToRect resolves per edge (got {focus:?})"
            );
        }
        other => panic!("a rectangular path gradient must not collapse: {other:?}"),
    }

    // The control that makes the assertion above mean something: a DIFFERENT authored
    // family must reach a DIFFERENT display value. Before this row, both were
    // `Radial`.
    let DisplayFill::Gradient(circle_gradient) = place_gradient_shape(
        casual_doc_model::v1::GradientKind::Radial,
        detail(GradientPath::Circle),
    ) else {
        panic!("expected a gradient fill");
    };
    assert!(
        matches!(
            circle_gradient.kind,
            DisplayGradientKind::Path {
                path: GradientPath::Circle,
                ..
            }
        ),
        "`path=\"circle\"` is a different display value from `path=\"rect\"` \
         (got {:?})",
        circle_gradient.kind
    );
    let DisplayFill::Gradient(shape_gradient) = place_gradient_shape(
        casual_doc_model::v1::GradientKind::Radial,
        detail(GradientPath::Shape),
    ) else {
        panic!("expected a gradient fill");
    };
    assert!(
        matches!(
            shape_gradient.kind,
            DisplayGradientKind::Path {
                path: GradientPath::Shape,
                ..
            }
        ),
        "`path=\"shape\"` is a third display value (got {:?})",
        shape_gradient.kind
    );

    // An `a:path` with no `@path` token states no family, so there is nothing to
    // resolve and the concentric collapse is KEPT rather than guessed at. This is
    // also the shape an ODF-imported radial gradient arrives in.
    let DisplayFill::Gradient(untyped) = place_gradient_shape(
        casual_doc_model::v1::GradientKind::Radial,
        Some(GradientDetail {
            rotate_with_shape: Some(true),
            ..GradientDetail::default()
        }),
    ) else {
        panic!("expected a gradient fill");
    };
    assert!(
        matches!(untyped.kind, DisplayGradientKind::Radial),
        "a path gradient with no authored family stays concentric (got {:?})",
        untyped.kind
    );

    // No row at all: the same answer, reached by a different route.
    let DisplayFill::Gradient(no_row) =
        place_gradient_shape(casual_doc_model::v1::GradientKind::Radial, None)
    else {
        panic!("expected a gradient fill");
    };
    assert!(
        matches!(no_row.kind, DisplayGradientKind::Radial),
        "a gradient with no side-table row stays concentric (got {:?})",
        no_row.kind
    );

    // An absent `a:fillToRect` is the degenerate identity rect, which would make the
    // gradient zero-extent and paint flat; it resolves to the centre point instead,
    // and that decision is recorded on `GradientFocus::CENTER`.
    let DisplayFill::Gradient(no_focus) = place_gradient_shape(
        casual_doc_model::v1::GradientKind::Radial,
        Some(GradientDetail {
            path: Some(GradientPath::Rect),
            ..GradientDetail::default()
        }),
    ) else {
        panic!("expected a gradient fill");
    };
    match no_focus.kind {
        DisplayGradientKind::Path { focus, .. } => assert!(
            (focus.left - 0.5).abs() < 1e-6
                && (focus.top - 0.5).abs() < 1e-6
                && (focus.right - 0.5).abs() < 1e-6
                && (focus.bottom - 0.5).abs() < 1e-6,
            "an absent a:fillToRect centres the focus (got {focus:?})"
        ),
        other => panic!("expected a path gradient: {other:?}"),
    }

    // And a LINEAR gradient through the same side-table row is untouched: the row's
    // `path`/`fillToRect` belong to `a:path` and must not be read for an `a:lin`.
    let DisplayFill::Gradient(linear) = place_gradient_shape(
        casual_doc_model::v1::GradientKind::Linear { angle: 5_400_000 },
        detail(GradientPath::Rect),
    ) else {
        panic!("expected a gradient fill");
    };
    assert!(
        matches!(
            linear.kind,
            DisplayGradientKind::Linear { angle_deg } if (angle_deg - 90.0).abs() < 0.01
        ),
        "an `a:lin` sweep is still linear (got {:?})",
        linear.kind
    );

    // The whole thing is worthless if the item never reached the page, so assert the
    // route once: the shape is a real `PaintItem::Shape` on the composed list.
    let shape_id = node(93);
    let mut definitions = Definitions::default();
    definitions.shape_fill_detail.insert(
        shape_id,
        casual_doc_model::v1::ShapeFillDetail {
            gradient: detail(GradientPath::Rect),
            ..Default::default()
        },
    );
    let list = compose_gradient_page(casual_doc_model::v1::GradientKind::Radial, definitions);
    assert!(
        list.items.iter().any(|item| matches!(
            item,
            PaintItem::Shape {
                fill: Some(DisplayFill::Gradient(_)),
                ..
            }
        )),
        "the gradient arrives as a composed paint item, not only as anchor content"
    );
}

/// Places one grouped `wps:wsp` whose `a:gradFill` carries `kind`, with `detail`
/// filed in `Definitions::shape_fill_detail` under the shape's own id, then composes
/// the page and returns the display fill the rasteriser would receive.
///
/// The shape goes through the shared `GroupChild` walk because that walk is where the
/// side-table lookup lives, and it is the walk a slide's layout shares.
fn place_gradient_shape(
    kind: casual_doc_model::v1::GradientKind,
    detail: Option<casual_doc_model::v1::GradientDetail>,
) -> DisplayFill {
    use casual_doc_layout::display::PaintItem;

    let mut definitions = Definitions::default();
    if let Some(detail) = detail {
        definitions.shape_fill_detail.insert(
            node(93),
            casual_doc_model::v1::ShapeFillDetail {
                gradient: Some(detail),
                ..Default::default()
            },
        );
    }
    let list = compose_gradient_page(kind, definitions);
    list.items
        .iter()
        .find_map(|item| match item {
            PaintItem::Shape {
                fill: Some(fill), ..
            } => Some(fill.clone()),
            _ => None,
        })
        .expect("the shape composes to a filled paint item")
}

/// The composed display list for a page holding one grouped, gradient-filled shape.
fn compose_gradient_page(
    kind: casual_doc_model::v1::GradientKind,
    definitions: Definitions,
) -> casual_doc_layout::display::DisplayList {
    use casual_doc_model::v1::GradientStop as ModelGradientStop;

    let extent = Extent {
        width_emu: 914_400,
        height_emu: 914_400,
    };
    let child = GroupChild::Shape(GroupShape {
        hyperlink: None,
        id: node(93),
        offset: PointEmu { x_emu: 0, y_emu: 0 },
        extent,
        geometry: ShapeGeometry::Rectangle,
        preset: None,
        adjustments: Vec::new(),
        path: None,
        fill: Some(Fill::Gradient {
            stops: vec![
                ModelGradientStop {
                    position: 0,
                    color: Rgba {
                        r: 255,
                        g: 0,
                        b: 0,
                        a: 255,
                    },
                },
                ModelGradientStop {
                    position: 100_000,
                    color: Rgba {
                        r: 0,
                        g: 0,
                        b: 255,
                        a: 255,
                    },
                },
            ],
            kind,
        }),
        stroke: None,
        flip_h: false,
        flip_v: false,
        rotation: None,
    });
    let group = InlineNode::Group(Box::new(WordprocessingGroup {
        hyperlink: None,
        id: node(92),
        anchor: Some(page_anchor(914_400, 914_400)),
        relative_height: Some(7),
        extent,
        transform: GroupTransform {
            offset: PointEmu { x_emu: 0, y_emu: 0 },
            extent,
            child_offset: PointEmu { x_emu: 0, y_emu: 0 },
            child_extent: extent,
            flip_h: false,
            flip_v: false,
            rotation: None,
        },
        children: vec![child],
    }));
    let paragraph = BlockNode::Paragraph(Paragraph {
        id: node(10),
        properties: ParagraphProperties::default().into(),
        inlines: vec![run(11, "Body"), group],
    });
    let document = Document::new(node(1), vec![paragraph], definitions).unwrap();
    let shaper = ParleyShaper::new();
    let cfg = config();
    let galley = build_galley(&document, &shaper, cfg.content_area().size.width);
    let mut layout = paginate(&galley, &cfg);
    place_floats(&mut layout, &document, &shaper, &cfg);
    compose_page(&layout.pages[0])
}

/// A GRADIENT matrix entry resolves to a real gradient, per stop, and the entry
/// kinds nothing can paint leave the shape unfilled rather than approximately
/// filled.
///
/// This is the common case, not an edge: the default Office theme's `fillStyleLst`
/// is solid, gradient, gradient, and `a:fillRef idx="2"`/`idx="3"` is what the Shape
/// Styles gallery writes. Before this, every one of those shapes resolved to no fill
/// at all.
///
/// Three separate claims, and each needs its own stop or index to be checkable:
///
/// * **per-stop placeholder.** Stop 1 is `a:phClr` with a `a:tint`; stop 2 is a
///   colour the theme fixes. A build that put the placeholder on the ENTRY could
///   not represent this, and one that dropped the transform would resolve stop 1 to
///   the reference colour unchanged — which is how the default Office theme, whose
///   three stops are all `phClr` and differ ONLY by transform, would flatten to one
///   colour while looking deliberate.
/// * **a pattern stays unfilled.** Not "becomes its foreground colour".
/// * **a non-solid outline stays unstroked.** `ShapeStroke` holds one colour, so a
///   gradient outline has nowhere to go but a wrong single colour.
#[test]
fn a_themed_gradient_resolves_per_stop_and_unpaintable_entries_stay_unfilled() {
    use casual_doc_layout::page::AnchorContent;
    use casual_doc_model::v1::{Fill, GradientKind, GradientStop};

    let AnchorContent::Rectangle { fill, stroke } = place_themed_shape(2, 2, None) else {
        panic!("expected a rectangle");
    };
    assert_eq!(
        fill,
        Some(casual_doc_layout::page::AnchorFill::plain(Fill::Gradient {
            stops: vec![
                // `tint 40000` over pure green: each channel c -> c*0.4 + 255*0.6.
                GradientStop {
                    position: 0,
                    color: Rgba {
                        r: 153,
                        g: 255,
                        b: 153,
                        a: 255,
                    },
                },
                GradientStop {
                    position: 100_000,
                    color: THEMED_BLUE,
                },
            ],
            kind: GradientKind::Linear { angle: 5_400_000 },
        })),
        "the gradient entry resolves stop by stop, transform included"
    );
    assert!(
        stroke.is_none(),
        "a gradient outline entry has no single colour, so it must not resolve"
    );

    // A pattern entry: modeled, and painted by nothing.
    let AnchorContent::Rectangle { fill, .. } = place_themed_shape(3, 1, None) else {
        panic!("expected a rectangle");
    };
    assert_eq!(
        fill, None,
        "a pattern entry must leave the shape unfilled, not take its foreground colour"
    );

    // An entry the parse could not model at all.
    let AnchorContent::Rectangle { fill, .. } = place_themed_shape(4, 1, None) else {
        panic!("expected a rectangle");
    };
    assert_eq!(fill, None, "an unmodeled entry resolves to nothing");

    // Index 5 is past the list; it must not wrap or clamp onto entry 4.
    let AnchorContent::Rectangle { fill, .. } = place_themed_shape(5, 1, None) else {
        panic!("expected a rectangle");
    };
    assert_eq!(fill, None, "an index past the list resolves to nothing");
}

/// The colour a `a:fillRef`/`a:lnRef` names for the entry's `a:phClr`.
const THEMED_GREEN: Rgba = Rgba {
    r: 0,
    g: 255,
    b: 0,
    a: 255,
};

/// A colour the fixture's theme fixes itself, on the gradient's second stop.
const THEMED_BLUE: Rgba = Rgba {
    r: 0,
    g: 0,
    b: 255,
    a: 255,
};

/// Places one themed shape and returns its anchor content.
///
/// The format scheme is deliberately NOT uniform, so an index that resolves to the
/// wrong entry is visible rather than indistinguishable: entry 1 is a `phClr` solid,
/// entry 2 a two-stop gradient mixing a transformed placeholder with a fixed colour,
/// entry 3 a pattern, entry 4 an entry the parse could not model. The line list is
/// the same idea in miniature: entry 1 resolves, entry 2 (a non-solid outline) does
/// not.
fn place_themed_shape(
    fill_idx: u32,
    line_idx: u32,
    explicit_fill: Option<casual_doc_model::v1::Fill>,
) -> casual_doc_layout::page::AnchorContent {
    use casual_doc_model::v1::{
        ColorTransform, DashStyle, Definitions, FillStyle, FormatScheme, GradientKind,
        GradientStyle, GradientStyleStop, LineStyle, PatternStyle, ShapeStyleRef, StyleColor,
    };

    let shape_id = node(91);
    let red = Rgba {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };
    let child = GroupChild::Shape(GroupShape {
        hyperlink: None,
        id: shape_id,
        offset: PointEmu { x_emu: 0, y_emu: 0 },
        extent: Extent {
            width_emu: 914_400,
            height_emu: 914_400,
        },
        geometry: ShapeGeometry::Rectangle,
        preset: None,
        adjustments: Vec::new(),
        path: None,
        fill: explicit_fill,
        stroke: None,
        flip_h: false,
        flip_v: false,
        rotation: None,
    });
    let extent = Extent {
        width_emu: 914_400,
        height_emu: 914_400,
    };
    let group = InlineNode::Group(Box::new(WordprocessingGroup {
        hyperlink: None,
        id: node(90),
        anchor: Some(page_anchor(914_400, 914_400)),
        relative_height: Some(11),
        extent,
        transform: GroupTransform {
            offset: PointEmu { x_emu: 0, y_emu: 0 },
            extent,
            child_offset: PointEmu { x_emu: 0, y_emu: 0 },
            child_extent: extent,
            flip_h: false,
            flip_v: false,
            rotation: None,
        },
        children: vec![child],
    }));
    let mut definitions = Definitions {
        format_scheme: Some(FormatScheme {
            fill_styles: vec![
                Some(FillStyle::Solid {
                    color: StyleColor::Placeholder(ColorTransform::default()),
                }),
                Some(FillStyle::Gradient(GradientStyle {
                    stops: vec![
                        GradientStyleStop {
                            position: 0,
                            color: StyleColor::Placeholder(ColorTransform {
                                tint: Some(40_000),
                                ..ColorTransform::default()
                            }),
                        },
                        GradientStyleStop {
                            position: 100_000,
                            color: StyleColor::Fixed(THEMED_BLUE),
                        },
                    ],
                    kind: GradientKind::Linear { angle: 5_400_000 },
                })),
                Some(FillStyle::Pattern(PatternStyle {
                    preset: "pct25".to_owned(),
                    foreground: StyleColor::Placeholder(ColorTransform::default()),
                    background: StyleColor::Fixed(Rgba {
                        r: 255,
                        g: 255,
                        b: 255,
                        a: 255,
                    }),
                })),
                None,
            ],
            line_styles: vec![
                Some(LineStyle {
                    width_emu: 6_350,
                    color: StyleColor::Placeholder(ColorTransform::default()),
                    dash: Some(DashStyle::Dash),
                }),
                None,
            ],
            // No effect style resolves to anything paintable here, and the
            // appearance under test is the fill; see `themed_appearance`.
            effect_styles: Vec::new(),
        }),
        ..Definitions::default()
    };
    definitions.shape_styles.insert(
        shape_id,
        ShapeStyleRef {
            fill_idx: Some(fill_idx),
            fill_color: Some(THEMED_GREEN),
            line_idx: Some(line_idx),
            line_color: Some(red),
            effect_idx: None,
            // Named, not defaulted: this fixture's subject is the fill and the
            // outline, and `a:fontRef` is deliberately absent so the appearance
            // under test cannot be coming from anywhere else. The guard that
            // proves `font_ref` travels is in `casual-doc-import`.
            font_ref: None,
        },
    );
    let paragraph = BlockNode::Paragraph(Paragraph {
        id: node(10),
        properties: ParagraphProperties::default().into(),
        inlines: vec![run(11, "Body"), group],
    });
    let document = Document::new(node(1), vec![paragraph], definitions).unwrap();
    let shaper = ParleyShaper::new();
    let cfg = config();
    let galley = build_galley(&document, &shaper, cfg.content_area().size.width);
    let mut layout = paginate(&galley, &cfg);
    place_floats(&mut layout, &document, &shaper, &cfg);
    layout.pages[0].anchored[0].content.clone()
}

/// A grouped TEXT BOX resolves its theme style, which it did not.
///
/// `docs/156` section 6 row 0.2's last paintable gap. A text-bearing `wps:wsp`
/// imports as a `GroupTextBox` but keeps the shape's node id, and the importer files
/// its `wps:style` reference under that id BEFORE building the text box — so the
/// entry was always in `Definitions::shape_styles` and the layout simply never
/// looked. A themed text box rendered unfilled: PowerPoint and the Shape Styles
/// gallery lean on the matrix rather than writing an explicit `spPr` fill, so this is
/// the common case rather than an edge.
///
/// Resolved through the SAME function a text-free shape uses, so the two cannot
/// disagree about what `a:fillRef idx="1"` means — which is why this asserts the text
/// box gets the identical fill the shape guard above asserts.
#[test]
fn a_grouped_text_box_with_no_explicit_fill_resolves_its_theme_style() {
    use casual_doc_layout::page::AnchorContent;

    let AnchorContent::TextBox { fill, border, .. } = place_themed_text_box(None) else {
        panic!("expected a text box");
    };
    assert_eq!(
        fill,
        Some(casual_doc_layout::page::AnchorFill::plain(
            casual_doc_model::v1::Fill::Solid(THEMED_GREEN)
        )),
        "the fillRef's colour is substituted for the entry's phClr, exactly as it is \
         for a text-free shape"
    );
    let border = border.expect("the lnRef resolves an outline");
    assert_eq!(border.color, [255, 0, 0, 255], "the lnRef's colour");

    // The control: the box's own fill is its own statement and must win over the
    // matrix, the same precedence a text-free shape has.
    let explicit = casual_doc_model::v1::Fill::Solid(Rgba {
        r: 1,
        g: 2,
        b: 3,
        a: 255,
    });
    let AnchorContent::TextBox { fill, .. } = place_themed_text_box(Some(explicit.clone())) else {
        panic!("expected a text box");
    };
    assert_eq!(
        fill,
        Some(casual_doc_layout::page::AnchorFill::plain(explicit)),
        "an explicit fill outranks the style matrix"
    );
}

/// Places a grouped text box carrying a `wps:style` reference into the same format
/// scheme `place_themed_shape` uses, so the two can be compared directly.
fn place_themed_text_box(
    explicit_fill: Option<casual_doc_model::v1::Fill>,
) -> casual_doc_layout::page::AnchorContent {
    use casual_doc_model::v1::{
        ColorTransform, DashStyle, Definitions, FillStyle, FormatScheme, LineStyle, ShapeStyleRef,
        StyleColor,
    };

    let box_id = node(95);
    let red = Rgba {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };
    let child = GroupChild::TextBox(GroupTextBox {
        hyperlink: None,
        id: box_id,
        offset: PointEmu { x_emu: 0, y_emu: 0 },
        extent: Extent {
            width_emu: 914_400,
            height_emu: 914_400,
        },
        // A plain rectangle, so the fill reaches `AnchorContent::TextBox` directly
        // rather than through a preset backdrop — the backdrop path has its own
        // guards and would hide which value was used.
        geometry: ShapeGeometry::Rectangle,
        preset: None,
        adjustments: Vec::new(),
        blocks: vec![BlockNode::Paragraph(Paragraph {
            id: node(96),
            properties: ParagraphProperties::default().into(),
            inlines: vec![run(97, "themed")],
        })],
        fill: explicit_fill,
        border: None,
        body_properties: TextBoxBodyProperties::default(),
        flip_h: false,
        flip_v: false,
        rotation: None,
    });
    let extent = Extent {
        width_emu: 914_400,
        height_emu: 914_400,
    };
    let group = InlineNode::Group(Box::new(WordprocessingGroup {
        hyperlink: None,
        id: node(94),
        anchor: Some(page_anchor(914_400, 914_400)),
        relative_height: Some(11),
        extent,
        transform: GroupTransform {
            offset: PointEmu { x_emu: 0, y_emu: 0 },
            extent,
            child_offset: PointEmu { x_emu: 0, y_emu: 0 },
            child_extent: extent,
            flip_h: false,
            flip_v: false,
            rotation: None,
        },
        children: vec![child],
    }));
    let mut definitions = Definitions {
        format_scheme: Some(FormatScheme {
            fill_styles: vec![Some(FillStyle::Solid {
                color: StyleColor::Placeholder(ColorTransform::default()),
            })],
            line_styles: vec![Some(LineStyle {
                width_emu: 6_350,
                color: StyleColor::Placeholder(ColorTransform::default()),
                dash: Some(DashStyle::Dash),
            })],
            effect_styles: Vec::new(),
        }),
        ..Definitions::default()
    };
    definitions.shape_styles.insert(
        box_id,
        ShapeStyleRef {
            fill_idx: Some(1),
            fill_color: Some(THEMED_GREEN),
            line_idx: Some(1),
            line_color: Some(red),
            effect_idx: None,
            // Named, not defaulted: this fixture's subject is the fill and the
            // outline, and `a:fontRef` is deliberately absent so the appearance
            // under test cannot be coming from anywhere else. The guard that
            // proves `font_ref` travels is in `casual-doc-import`.
            font_ref: None,
        },
    );
    let paragraph = BlockNode::Paragraph(Paragraph {
        id: node(10),
        properties: ParagraphProperties::default().into(),
        inlines: vec![run(11, "Body"), group],
    });
    let document = Document::new(node(1), vec![paragraph], definitions).unwrap();
    let shaper = ParleyShaper::new();
    let cfg = config();
    let galley = build_galley(&document, &shaper, cfg.content_area().size.width);
    let mut layout = paginate(&galley, &cfg);
    place_floats(&mut layout, &document, &shaper, &cfg);
    layout.pages[0].anchored[0].content.clone()
}

/// A grouped TEXT BOX with `a:outerShdw` casts it too, through the same resolver.
///
/// This is the seam that matters for the second document class: `emit_text_box` is a
/// `GroupChildHost` method, so wiring the shadow there is what gives a slide's text
/// box one as well — and the shadow is cast by the layer, which means it is the
/// silhouette of the box and the text inside it rather than of a rectangle.
#[test]
fn a_grouped_text_box_with_an_outer_shadow_casts_it_too() {
    use casual_doc_model::v1::{Definitions, OuterShadow, ShapeFillDetail};

    let box_id = node(95);
    let child = GroupChild::TextBox(GroupTextBox {
        hyperlink: None,
        id: box_id,
        offset: PointEmu { x_emu: 0, y_emu: 0 },
        extent: Extent {
            width_emu: 914_400,
            height_emu: 914_400,
        },
        geometry: ShapeGeometry::Rectangle,
        preset: None,
        adjustments: Vec::new(),
        blocks: vec![BlockNode::Paragraph(Paragraph {
            id: node(96),
            properties: ParagraphProperties::default().into(),
            inlines: vec![run(97, "shadowed")],
        })],
        fill: None,
        border: None,
        body_properties: TextBoxBodyProperties::default(),
        flip_h: false,
        flip_v: false,
        rotation: None,
    });
    let extent = Extent {
        width_emu: 914_400,
        height_emu: 914_400,
    };
    let group = InlineNode::Group(Box::new(WordprocessingGroup {
        hyperlink: None,
        id: node(94),
        anchor: Some(page_anchor(914_400, 914_400)),
        relative_height: Some(11),
        extent,
        transform: GroupTransform {
            offset: PointEmu { x_emu: 0, y_emu: 0 },
            extent,
            child_offset: PointEmu { x_emu: 0, y_emu: 0 },
            child_extent: extent,
            flip_h: false,
            flip_v: false,
            rotation: None,
        },
        children: vec![child],
    }));
    let mut definitions = Definitions::default();
    definitions.shape_fill_detail.insert(
        box_id,
        ShapeFillDetail {
            picture: None,
            pattern: None,
            gradient: None,
            stroke: None,
            outer_shadow: Some(OuterShadow {
                blur_radius_emu: 50_800,
                // Straight down: 5400000 is 90 degrees, so the whole distance lands
                // on y and none on x. A convention that measured from +y instead
                // would put it all on x, and this is the row that tells them apart.
                distance_emu: 25_400,
                direction: 5_400_000,
                color: Rgba {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 161,
                },
            }),
        },
    );
    let paragraph = BlockNode::Paragraph(Paragraph {
        id: node(10),
        properties: ParagraphProperties::default().into(),
        inlines: vec![run(11, "Body"), group],
    });
    let document = Document::new(node(1), vec![paragraph], definitions).unwrap();
    let shaper = ParleyShaper::new();
    let cfg = config();
    let galley = build_galley(&document, &shaper, cfg.content_area().size.width);
    let mut layout = paginate(&galley, &cfg);
    place_floats(&mut layout, &document, &shaper, &cfg);

    let shadow = layout.pages[0].anchored[0]
        .shadow
        .expect("the text box carries its shadow onto the page");
    assert_eq!(shadow.blur, Twip(80));
    assert_eq!(shadow.offset_x, Twip(0), "90 degrees puts nothing on x");
    assert_eq!(
        shadow.offset_y,
        Twip(40),
        "and the whole 40 twips on y, downward"
    );
    assert_eq!(shadow.color, [0, 0, 0, 161]);
}

/// An adjustment guide that COMPUTES its value is honoured, not passed over for the
/// preset default (`109` FID-G-02 / FID-L-04 groundwork).
///
/// `a:avLst` guides are usually literals, so before the formula evaluator existed
/// layout matched on a `val ` prefix and fell back to the documented default for
/// anything else. That failure is invisible by construction: the shape still draws,
/// still looks like itself, and is simply the wrong proportions — with no report,
/// because nothing knew a formula had been skipped.
///
/// A `roundRect` is the clearest witness: its radius is `shorter * adj / 100000`, so
/// the default 16667 and a computed 25000 give visibly different radii on the same
/// 1440-twip box.
#[test]
fn an_adjustment_guide_that_computes_its_value_is_evaluated_not_defaulted() {
    let radius_with = |adjustments: Vec<ShapeAdjustment>| {
        let content = only_anchor_content(&single_child_group_document(preset_shape_child(
            ShapeGeometry::RoundRectangle,
            adjustments,
        )));
        corner_radius(&content)
    };
    let adj = |formula: &str| {
        vec![ShapeAdjustment {
            name: "adj".to_owned(),
            formula: formula.to_owned(),
        }]
    };

    // The preset default, 16667: 1440 * 16667 / 100000 = 240.
    assert_eq!(radius_with(Vec::new()), Twip(240), "preset default");
    // A literal still resolves exactly as it always did.
    assert_eq!(radius_with(adj("val 25000")), Twip(360), "literal");
    // And a formula computing the same 25000 must give the same radius. Before the
    // evaluator this silently returned the default's 240.
    assert_eq!(
        radius_with(adj("*/ 50000 1 2")),
        Twip(360),
        "a computed guide must not fall back to the default"
    );
    // One referencing the box resolves too, IN THE SHAPE'S OWN UNIT. Guides are
    // evaluated against the shape's extent in EMU — the unit the document wrote
    // them in — so `ss` is 914400 and `ss / 4` = 228600, which the definition's own
    // `pin 0 adj 50000` clamps to 50000: a fully rounded end, 720 twips. (Evaluated
    // in twips, as the first evaluator did, `ss` was 1440 and this read 5 twips — a
    // unit artifact, not a shape anyone authored.)
    assert_eq!(
        radius_with(adj("*/ ss 1 4")),
        Twip(720),
        "a box-relative guide, clamped by the definition"
    );
}

/// A curve's CONTROL points are resolved into page space, not just its endpoints
/// (`109` FID-G-02).
///
/// This is the specific way a curve goes wrong while still looking like a curve: if
/// only endpoints were mapped, the controls would stay in the path's own coordinate
/// space — tiny numbers near the page origin — and the curve would whip off toward
/// the top-left instead of bulging where it was authored. Every number below is
/// arithmetic on a 1" box at a known page position, not a snapshot.
#[test]
fn a_curves_control_points_are_resolved_into_page_space() {
    use casual_doc_layout::display::PathCommand;
    use casual_doc_model::v1::{CustomGeometry, GeometryPoint, ShapePath, ShapePathCommand};

    let child = GroupChild::Shape(GroupShape {
        hyperlink: None,
        id: node(91),
        offset: PointEmu { x_emu: 0, y_emu: 0 },
        extent: Extent {
            width_emu: 914_400,
            height_emu: 914_400,
        },
        geometry: ShapeGeometry::Other,
        preset: None,
        adjustments: Vec::new(),
        path: Some(CustomGeometry::single_path(ShapePath {
            width_emu: 100,
            height_emu: 100,
            ..ShapePath::new(vec![
                ShapePathCommand::MoveTo {
                    point: GeometryPoint::literal(0, 0),
                },
                ShapePathCommand::CubicBezTo {
                    control1: GeometryPoint::literal(30, 80),
                    control2: GeometryPoint::literal(70, 80),
                    point: GeometryPoint::literal(100, 0),
                },
            ])
        })),
        fill: None,
        stroke: None,
        flip_h: false,
        flip_v: false,
        rotation: None,
    });

    let content = only_anchor_content(&single_child_group_document(child));
    let AnchorContent::Path { paths, .. } = content else {
        panic!("expected a path, got {content:?}");
    };
    let commands = paths[0].commands.clone();
    assert!(!closes(&commands), "no a:close was authored");
    // The box is 1440 twips square at (1440, 1440), and `@w`/`@h` are 100, so a
    // coordinate maps to 1440 + round(1440 * value / 100).
    assert_eq!(
        commands,
        vec![
            PathCommand::MoveTo {
                point: Point::new(Twip(1_440), Twip(1_440)),
            },
            PathCommand::CubicTo {
                control1: Point::new(Twip(1_872), Twip(2_592)),
                control2: Point::new(Twip(2_448), Twip(2_592)),
                point: Point::new(Twip(2_880), Twip(1_440)),
            },
        ],
    );
}

/// `a:path` coordinate-space rules are applied per axis: a POSITIVE `@w`/`@h`
/// scales the coordinate to the box, and a ZERO one (an absent attribute) is an
/// absolute EMU offset that does not scale (docs/119 §3).
///
/// Both shapes sit in a 1" × 1" child box at a known page position, so the
/// expected twips are arithmetic, not a snapshot.
#[test]
fn a_custom_geometry_resolves_to_a_polyline_not_a_rectangle() {
    use casual_doc_model::v1::{CustomGeometry, GeometryPoint, ShapePath, ShapePathCommand};

    let child_extent = Extent {
        width_emu: 914_400,
        height_emu: 914_400,
    };
    let group_extent = Extent {
        width_emu: 2 * 914_400,
        height_emu: 914_400,
    };
    let move_to = |x_emu, y_emu| ShapePathCommand::MoveTo {
        point: GeometryPoint::literal(x_emu, y_emu),
    };
    let line_to = |x_emu, y_emu| ShapePathCommand::LineTo {
        point: GeometryPoint::literal(x_emu, y_emu),
    };
    let shape = |id, x_emu, path| {
        GroupChild::Shape(GroupShape {
            hyperlink: None,
            id: node(id),
            offset: PointEmu { x_emu, y_emu: 0 },
            extent: child_extent,
            geometry: ShapeGeometry::Other,
            preset: None,
            adjustments: Vec::new(),
            path: Some(CustomGeometry::single_path(path)),
            fill: None,
            stroke: None,
            flip_h: false,
            flip_v: false,
            rotation: None,
        })
    };

    let group = InlineNode::Group(Box::new(WordprocessingGroup {
        hyperlink: None,
        id: node(90),
        anchor: Some(page_anchor(914_400, 914_400)),
        relative_height: Some(11),
        extent: group_extent,
        transform: GroupTransform {
            offset: PointEmu { x_emu: 0, y_emu: 0 },
            extent: group_extent,
            child_offset: PointEmu { x_emu: 0, y_emu: 0 },
            child_extent: group_extent,
            flip_h: false,
            flip_v: false,
            rotation: None,
        },
        children: vec![
            // The loan-agreement rule: `@w` scales x across the box, `@h` is
            // absent so y stays an absolute EMU offset (0 = the box top).
            shape(
                91,
                0,
                ShapePath {
                    width_emu: 1000,
                    height_emu: 0,
                    ..ShapePath::new(vec![move_to(0, 0), line_to(1000, 0)])
                },
            ),
            // A closed triangle with both axes scaled.
            shape(
                92,
                914_400,
                ShapePath {
                    width_emu: 100,
                    height_emu: 100,
                    ..ShapePath::new(vec![
                        move_to(50, 0),
                        line_to(100, 100),
                        line_to(0, 100),
                        ShapePathCommand::Close,
                    ])
                },
            ),
        ],
    }));
    let paragraph = BlockNode::Paragraph(Paragraph {
        id: node(10),
        properties: ParagraphProperties::default().into(),
        inlines: vec![run(11, "Body"), group],
    });
    let document = Document::new(node(1), vec![paragraph], Definitions::default()).unwrap();

    let shaper = ParleyShaper::new();
    let cfg = config();
    let galley = build_galley(&document, &shaper, cfg.content_area().size.width);
    let mut layout = paginate(&galley, &cfg);
    place_floats(&mut layout, &document, &shaper, &cfg);

    let polygons: Vec<(Vec<Point>, bool)> = layout.pages[0]
        .anchored
        .iter()
        .filter_map(|anchor| match &anchor.content {
            AnchorContent::Path { paths, .. } => Some(first_path(paths)),
            _ => None,
        })
        .collect();
    assert_eq!(
        polygons.len(),
        2,
        "both freeforms are polylines; a rectangle fallback yields none"
    );

    // The group is anchored 1" from the page's top-left, so the first child box
    // is (1440, 1440) to (2880, 2880) twips. The rule spans its full width at
    // its top edge, and stays OPEN.
    assert_eq!(
        polygons[0],
        (
            vec![
                Point::new(Twip(1_440), Twip(1_440)),
                Point::new(Twip(2_880), Twip(1_440)),
            ],
            false
        )
    );
    // The triangle's second child box starts at x = 2880 twips.
    assert_eq!(
        polygons[1],
        (
            vec![
                Point::new(Twip(3_600), Twip(1_440)),
                Point::new(Twip(4_320), Twip(2_880)),
                Point::new(Twip(2_880), Twip(2_880)),
            ],
            true
        )
    );

    // …and the closing survives into the display list, which is the half the
    // backends actually read: as the authored `a:close` command, which a path with
    // several subpaths needs, rather than only as the whole-path flag.
    let list = compose_page(&layout.pages[0]);
    let closed: Vec<bool> = list
        .items
        .iter()
        .filter_map(|item| match item {
            PaintItem::Shape {
                geometry: DisplayShapeGeometry::Path { commands, closed },
                ..
            } => Some(*closed || closes(commands)),
            _ => None,
        })
        .collect();
    assert_eq!(closed, vec![false, true]);
}

// --- Footer page-number fields inside text boxes (SDS regression) ----------

/// A field inline node carrying a *stale* cached result — the baked value Word
/// wrote into the file that the field pass must overwrite with the live value.
fn field(id: u64, instruction: &str, cached: &str) -> InlineNode {
    InlineNode::Field(Box::new(Field {
        id: node(id),
        instruction: instruction.to_owned(),
        kind: casual_doc_model::v1::FieldKind::parse(instruction),
        inlines: vec![run(id + 1, cached)],
        form: None,
        update: Default::default(),
    }))
}

/// Collects the resolved `PAGE`/`NUMPAGES` marker values from a slice of flowed
/// block fragments, recursing into table cells and inline text boxes.
fn collect_field_values(
    blocks: &[BlockFragment],
    page: &mut Option<String>,
    numpages: &mut Option<String>,
) {
    for block in blocks {
        match block {
            BlockFragment::Paragraph { lines, .. } => {
                for line in &lines.lines {
                    for marker in &line.fields {
                        match marker.kind {
                            FieldKind::Page => *page = Some(marker.value.clone()),
                            FieldKind::NumPages => *numpages = Some(marker.value.clone()),
                            FieldKind::Passthrough => {}
                        }
                    }
                    for text_box in &line.text_boxes {
                        collect_field_values(&text_box.blocks, page, numpages);
                    }
                }
            }
            BlockFragment::TableRow { cells, .. } => {
                for cell in cells {
                    collect_field_values(&cell.blocks, page, numpages);
                }
            }
        }
    }
}

/// The `(PAGE, NUMPAGES)` values resolved inside `page`'s anchored (floating) text
/// boxes.
fn anchored_field_values(page: &Page) -> (Option<String>, Option<String>) {
    let mut page_value = None;
    let mut numpages = None;
    for anchor in &page.anchored {
        if let AnchorContent::TextBox { blocks, .. } = &anchor.content {
            collect_field_values(blocks, &mut page_value, &mut numpages);
        }
    }
    (page_value, numpages)
}

/// Builds a two-page document whose footer holds a *floating* text box carrying
/// `page_instr` (a `PAGE` field, possibly with a format switch) and a `NUMPAGES`
/// field — exactly the SDS corpus shape (a positioned `v:textbox` with complex
/// fields) — then runs the full post-pagination pipeline, including
/// [`resolve_anchored_fields`]. Returns the paginated layout.
fn footer_text_box_layout(page_instr: &str) -> casual_doc_layout::page::PaginatedLayout {
    let definitions = media_defs().1;
    let mut definitions = definitions;

    // The page number lives INSIDE a floating text box, with stale cached results
    // ("99") baked in — the bug is that these were shown verbatim on every page.
    let footer_box = InlineNode::TextBox(Box::new(TextBox {
        hyperlink: None,
        id: node(400),
        anchor: Some(page_anchor(2_743_200, 9_144_000)),
        relative_height: Some(1),
        extent: Some(Extent {
            width_emu: 914_400,
            height_emu: 228_600,
        }),
        fill: None,
        border: None,
        body_properties: TextBoxBodyProperties::default(),
        blocks: vec![BlockNode::Paragraph(Paragraph {
            id: node(410),
            properties: ParagraphProperties::default().into(),
            inlines: vec![
                field(420, page_instr, "99"),
                run(430, " / "),
                field(440, " NUMPAGES ", "99"),
            ],
        })],
    }));
    let footer_para = BlockNode::Paragraph(Paragraph {
        id: node(390),
        properties: ParagraphProperties::default().into(),
        inlines: vec![footer_box],
    });
    definitions.footers.insert(
        HeaderFooterId::new(node(380)),
        ModelHeaderFooter {
            blocks: vec![footer_para.clone()],
        },
    );

    // A two-page body (a forced page break) so the page number differs per page.
    let body = vec![
        BlockNode::Paragraph(Paragraph {
            id: node(10),
            properties: ParagraphProperties::default().into(),
            inlines: vec![run(11, "page one")],
        }),
        BlockNode::Paragraph(Paragraph {
            id: node(20),
            properties: ParagraphProperties {
                page_break_before: Some(true),
                ..ParagraphProperties::default()
            }
            .into(),
            inlines: vec![run(21, "page two")],
        }),
    ];
    let doc = Document::new(node(1), body, definitions).unwrap();

    let shaper = ParleyShaper::new();
    let mut cfg = config();
    let footer = flow_header_footer(&doc, &[footer_para], &shaper, cfg.content_area().size.width);
    let running = RunningContent {
        footer: RunningBand {
            default: footer,
            ..RunningBand::default()
        },
        ..RunningContent::default()
    };
    cfg.footer_height = running.band_heights().1;
    let galley = build_galley(&doc, &shaper, cfg.content_area().size.width);
    let mut layout = paginate(&galley, &cfg);
    place_running_content(&mut layout, &running, &cfg);
    resolve_fields(&mut layout, &shaper);
    place_floats(&mut layout, &doc, &shaper, &cfg);
    resolve_anchored_fields(&mut layout, &shaper);
    layout
}

#[test]
fn a_page_field_in_a_footer_text_box_resolves_per_page() {
    let layout = footer_text_box_layout(" PAGE ");
    assert_eq!(layout.pages.len(), 2);
    // The floating footer box repeats on every page, and its PAGE field shows the
    // current page (never the stale cached "99"); NUMPAGES shows the true total.
    assert_eq!(
        anchored_field_values(&layout.pages[0]),
        (Some("1".to_owned()), Some("2".to_owned())),
        "page 1 footer box shows 1 / 2"
    );
    assert_eq!(
        anchored_field_values(&layout.pages[1]),
        (Some("2".to_owned()), Some("2".to_owned())),
        "page 2 footer box shows 2 / 2"
    );
}

#[test]
fn a_mergeformat_switched_page_field_in_a_footer_text_box_resolves() {
    // Word commonly writes `PAGE \* MERGEFORMAT`; the switch must not defeat field
    // classification — the leading keyword still resolves to the live page number.
    let layout = footer_text_box_layout("PAGE  \\* MERGEFORMAT");
    assert_eq!(layout.pages.len(), 2);
    assert_eq!(
        anchored_field_values(&layout.pages[0]).0,
        Some("1".to_owned()),
        "a MERGEFORMAT-switched PAGE resolves on page 1"
    );
    assert_eq!(
        anchored_field_values(&layout.pages[1]).0,
        Some("2".to_owned()),
        "a MERGEFORMAT-switched PAGE resolves on page 2"
    );
}

#[test]
fn a_page_field_in_an_inline_text_box_resolves() {
    // An INLINE text box (no anchor) flows onto a line; its PAGE field must resolve
    // through the ordinary field pass, which now recurses into inline text boxes.
    let inline_box = InlineNode::TextBox(Box::new(TextBox {
        hyperlink: None,
        id: node(50),
        anchor: None,
        relative_height: None,
        extent: None,
        fill: None,
        border: None,
        body_properties: TextBoxBodyProperties::default(),
        blocks: vec![BlockNode::Paragraph(Paragraph {
            id: node(51),
            properties: ParagraphProperties::default().into(),
            inlines: vec![field(52, " PAGE ", "99")],
        })],
    }));
    let para = BlockNode::Paragraph(Paragraph {
        id: node(10),
        properties: ParagraphProperties::default().into(),
        inlines: vec![run(11, "Body "), inline_box],
    });
    let doc = Document::new(node(1), vec![para], media_defs().1).unwrap();

    let shaper = ParleyShaper::new();
    let cfg = config();
    let galley = build_galley(&doc, &shaper, cfg.content_area().size.width);
    let mut layout = paginate(&galley, &cfg);
    resolve_fields(&mut layout, &shaper);

    // Read the PAGE marker off the inline text box on the body fragment's line.
    let mut page_value = None;
    let mut numpages = None;
    for placed in &layout.pages[0].placed {
        collect_field_values(
            std::slice::from_ref(&placed.fragment),
            &mut page_value,
            &mut numpages,
        );
    }
    assert_eq!(
        page_value,
        Some("1".to_owned()),
        "an inline text box's PAGE field resolves to the current page, not the cached 99"
    );
}

// --- Preset shape coverage and text-in-shape geometry -----------------------

/// A group holding ONE 1"×1" child at the page's 1"/1" anchor, so every expected
/// twip below is arithmetic: the child box is `(1440, 1440)` to `(2880, 2880)`.
fn single_child_group_document(child: GroupChild) -> Document {
    let extent = Extent {
        width_emu: 914_400,
        height_emu: 914_400,
    };
    let group = InlineNode::Group(Box::new(WordprocessingGroup {
        hyperlink: None,
        id: node(90),
        anchor: Some(page_anchor(914_400, 914_400)),
        relative_height: Some(11),
        extent,
        transform: GroupTransform {
            offset: PointEmu { x_emu: 0, y_emu: 0 },
            extent,
            child_offset: PointEmu { x_emu: 0, y_emu: 0 },
            child_extent: extent,
            flip_h: false,
            flip_v: false,
            rotation: None,
        },
        children: vec![child],
    }));
    let paragraph = BlockNode::Paragraph(Paragraph {
        id: node(10),
        properties: ParagraphProperties::default().into(),
        inlines: vec![run(11, "Body"), group],
    });
    Document::new(node(1), vec![paragraph], Definitions::default()).unwrap()
}

/// Places `document`'s single float and returns its resolved content.
fn only_anchor_content(document: &Document) -> AnchorContent {
    let shaper = ParleyShaper::new();
    let cfg = config();
    let galley = build_galley(document, &shaper, cfg.content_area().size.width);
    let mut layout = paginate(&galley, &cfg);
    place_floats(&mut layout, document, &shaper, &cfg);
    assert_eq!(layout.pages[0].anchored.len(), 1);
    layout.pages[0].anchored[0].content.clone()
}

/// Every [`ShapeGeometry`] variant reaches a layout primitive that is actually
/// its shape — not a bounding rectangle wearing its name.
///
/// `expected` is exhaustive with no wildcard, so a variant added to the model
/// without a layout outline fails to compile here. That is the whole point of
/// the guard: `Other` already paints a bounding rectangle, so a named variant
/// that did the same would be a regression dressed as a feature.
#[test]
fn every_shape_geometry_reaches_its_own_layout_primitive() {
    /// How many vertices the preset's closed outline has, or the non-polygon
    /// primitive it resolves to instead.
    #[derive(Debug)]
    enum Expected {
        Polygon(usize),
        Rectangle,
        /// A closed path whose corners are curves — the `roundRect`, which now
        /// resolves through the standard's definition (four quarter-circle arcs)
        /// instead of a closed-form primitive with quadratic corners.
        RoundedPath,
        Ellipse,
        Line,
    }

    fn expected(geometry: ShapeGeometry) -> Expected {
        match geometry {
            ShapeGeometry::Rectangle | ShapeGeometry::Other => Expected::Rectangle,
            ShapeGeometry::RoundRectangle => Expected::RoundedPath,
            ShapeGeometry::Ellipse => Expected::Ellipse,
            ShapeGeometry::Line => Expected::Line,
            ShapeGeometry::Triangle | ShapeGeometry::RightTriangle => Expected::Polygon(3),
            ShapeGeometry::Diamond | ShapeGeometry::Parallelogram | ShapeGeometry::Trapezoid => {
                Expected::Polygon(4)
            }
            ShapeGeometry::Pentagon | ShapeGeometry::HomePlate => Expected::Polygon(5),
            ShapeGeometry::Hexagon | ShapeGeometry::Chevron => Expected::Polygon(6),
            ShapeGeometry::RightArrow
            | ShapeGeometry::LeftArrow
            | ShapeGeometry::UpArrow
            | ShapeGeometry::DownArrow => Expected::Polygon(7),
            ShapeGeometry::Octagon | ShapeGeometry::Star4 => Expected::Polygon(8),
            ShapeGeometry::Star5 | ShapeGeometry::LeftRightArrow => Expected::Polygon(10),
            ShapeGeometry::Plus => Expected::Polygon(12),
        }
    }

    for geometry in all_shape_geometries() {
        let document = single_child_group_document(preset_shape_child(geometry, Vec::new()));
        let content = only_anchor_content(&document);
        match (expected(geometry), &content) {
            (Expected::Polygon(count), AnchorContent::Path { paths, .. }) => {
                let (points, closed) = first_path(paths);
                assert_eq!(points.len(), count, "{geometry:?} vertex count");
                assert!(closed, "{geometry:?} is a closed outline");
                for point in points {
                    assert!(
                        point.x >= Twip(1_440)
                            && point.x <= Twip(2_880)
                            && point.y >= Twip(1_440)
                            && point.y <= Twip(2_880),
                        "{geometry:?} vertex {point:?} escapes its 1\" box"
                    );
                }
            }
            (Expected::RoundedPath, AnchorContent::Path { paths, .. }) => {
                let commands = &paths[0].commands;
                assert!(closes(commands), "{geometry:?} is a closed outline");
                assert_eq!(
                    commands
                        .iter()
                        .filter(|command| matches!(
                            command,
                            casual_doc_layout::display::PathCommand::CubicTo { .. }
                        ))
                        .count(),
                    4,
                    "{geometry:?} has four arc corners"
                );
            }
            (Expected::Rectangle, AnchorContent::Rectangle { .. })
            | (Expected::Ellipse, AnchorContent::Ellipse { .. })
            | (Expected::Line, AnchorContent::Line { .. }) => {}
            (want, other) => {
                panic!("{geometry:?} wanted {want:?}, resolved to {other:?}");
            }
        }
    }
}

/// A preset outside the typed set — kept only as its `ST_ShapeType` token — draws
/// from the standard's table instead of its bounding box (`109` FID-L-04).
///
/// `wedgeRectCallout` at its default adjust values puts its tail's tip at
/// `(hc - 0.20833w, vc + 0.625h)`: below the box, which is the whole point of a
/// callout and exactly what the bounding-rectangle fallback could never draw. The
/// tail leaves the bottom edge between `x1 = w·2/12` and `x2 = w·5/12`. Every
/// number is arithmetic on the 1" box at (1440, 1440), not a snapshot.
#[test]
fn a_retained_preset_token_draws_from_the_standards_table() {
    let outline = |token: &str, adjustments: Vec<ShapeAdjustment>| {
        let mut child = preset_shape_child(ShapeGeometry::Other, adjustments);
        if let GroupChild::Shape(shape) = &mut child {
            shape.preset = Some(token.to_owned());
        }
        only_anchor_content(&single_child_group_document(child))
    };
    let at = |x, y| Point::new(Twip(x), Twip(y));

    let AnchorContent::Path { paths, .. } = outline("wedgeRectCallout", Vec::new()) else {
        panic!("a callout must draw its outline, not a box");
    };
    let (points, closed) = first_path(&paths);
    assert!(closed, "the callout is a closed outline");
    let tail = [at(2_040, 2_880), at(1_860, 3_060), at(1_680, 2_880)];
    assert!(
        points.windows(3).any(|window| window == tail),
        "the tail leaves the bottom edge and reaches its tip below the box: {points:?}"
    );

    // The authored `a:avLst` overrides the definition's defaults by name: tip
    // centred, a full height below the centre.
    let AnchorContent::Path { paths, .. } = outline(
        "wedgeRectCallout",
        vec![
            ShapeAdjustment {
                name: "adj1".to_owned(),
                formula: "val 0".to_owned(),
            },
            ShapeAdjustment {
                name: "adj2".to_owned(),
                formula: "val 100000".to_owned(),
            },
        ],
    ) else {
        panic!("still a path");
    };
    let (points, _) = first_path(&paths);
    assert!(
        points
            .windows(3)
            .any(|window| window == [at(2_040, 2_880), at(2_160, 3_600), at(1_680, 2_880)]),
        "the authored adjust values move the tip: {points:?}"
    );

    // A token outside `ST_ShapeType` has no definition, and keeps painting the
    // bounding rectangle rather than nothing (docs/119 §6 "Rejected").
    assert!(matches!(
        outline("notAShapeType", Vec::new()),
        AnchorContent::Rectangle { .. }
    ));
}

/// The display-list items one preset shape composes to, in paint order.
fn composed_shape_items(child: GroupChild) -> Vec<PaintItem> {
    let document = single_child_group_document(child);
    let shaper = ParleyShaper::new();
    let cfg = config();
    let galley = build_galley(&document, &shaper, cfg.content_area().size.width);
    let mut layout = paginate(&galley, &cfg);
    place_floats(&mut layout, &document, &shaper, &cfg);
    compose_page(&layout.pages[0])
        .items
        .into_iter()
        .filter(|item| matches!(item, PaintItem::Shape { .. }))
        .collect()
}

/// A preset of several paths paints each one its OWN way (`a:path@fill`,
/// `@stroke`): the standard's `can` is a filled body, a LIGHTENED lid with no
/// outline, and an outline-only path over both. One item per path, in path
/// order — a single item filled the shape's colour would paint the lid the same
/// colour as the body and outline the seams that are not there.
#[test]
fn a_multi_path_preset_paints_each_path_with_its_own_fill_mode_and_stroke() {
    let mut child = preset_shape_child(ShapeGeometry::Other, Vec::new());
    if let GroupChild::Shape(shape) = &mut child {
        shape.preset = Some("can".to_owned());
        shape.stroke = Some(casual_doc_model::v1::ShapeStroke {
            color: Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            width_emu: 12_700,
            dash: None,
            head_end: None,
            tail_end: None,
        });
    }
    let painted: Vec<(Option<[u8; 3]>, bool)> = composed_shape_items(child)
        .iter()
        .map(|item| match item {
            PaintItem::Shape { fill, stroke, .. } => (
                fill.as_ref().map(|fill| match fill {
                    DisplayFill::Solid(color) => [color.r, color.g, color.b],
                    DisplayFill::Gradient(_) => panic!("a solid fill stays solid"),
                }),
                stroke.is_some(),
            ),
            _ => unreachable!("filtered to shapes"),
        })
        .collect();
    assert_eq!(
        painted,
        vec![
            // The body: the shape's own fill, no outline.
            (Some([60, 120, 180]), false),
            // The lid: `lighten` — `a:tint` at 60%, 255 - (255 - c) * 0.6.
            (Some([138, 174, 210]), false),
            // The outline over both, unfilled.
            (None, true),
        ]
    );
}

/// The arrowheads of an open preset ride the path that is OPEN and STROKED: the
/// standard's `arc` is a filled wedge with no outline plus the stroked arc
/// itself, and the arrowhead belongs on the arc, not on the wedge's closed
/// outline (which has no ends to put one on).
#[test]
fn an_arcs_arrowhead_rides_its_stroked_open_path() {
    use casual_doc_model::v1::{LineEnd, LineEndKind};
    let mut child = preset_shape_child(ShapeGeometry::Other, Vec::new());
    if let GroupChild::Shape(shape) = &mut child {
        shape.preset = Some("arc".to_owned());
        shape.stroke = Some(casual_doc_model::v1::ShapeStroke {
            color: Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            width_emu: 12_700,
            dash: None,
            head_end: None,
            tail_end: Some(LineEnd {
                kind: LineEndKind::Triangle,
                width: None,
                length: None,
            }),
        });
    }
    let tails: Vec<bool> = composed_shape_items(child)
        .iter()
        .map(|item| {
            matches!(
                item,
                PaintItem::Shape {
                    tail_end: Some(_),
                    ..
                }
            )
        })
        .collect();
    assert_eq!(
        tails,
        vec![false, true],
        "the wedge, then the arc with its head"
    );
}

/// Three of the new presets, to the twip, so the outlines are arithmetic rather
/// than "some polygon arrived". A `homePlate` whose point was on the wrong side,
/// or a `plus` with the arms inverted, would satisfy a vertex count.
#[test]
fn new_presets_resolve_to_their_documented_outlines() {
    let outline = |geometry, adjustments| {
        let document = single_child_group_document(preset_shape_child(geometry, adjustments));
        match only_anchor_content(&document) {
            AnchorContent::Path { paths, .. } => first_path(&paths).0,
            other => panic!("expected a path, got {other:?}"),
        }
    };
    let at = |x, y| Point::new(Twip(x), Twip(y));

    // `homePlate` at its preset default `adj` = 50000: the point is half the
    // shorter side (720tw) deep, on the RIGHT.
    assert_same_outline(
        &outline(ShapeGeometry::HomePlate, Vec::new()),
        &[
            at(1_440, 1_440),
            at(2_160, 1_440),
            at(2_880, 2_160),
            at(2_160, 2_880),
            at(1_440, 2_880),
        ],
    );

    // `plus` at its preset default `adj` = 25000: 360tw arms.
    assert_same_outline(
        &outline(ShapeGeometry::Plus, Vec::new()),
        &[
            at(1_440, 1_800),
            at(1_800, 1_800),
            at(1_800, 1_440),
            at(2_520, 1_440),
            at(2_520, 1_800),
            at(2_880, 1_800),
            at(2_880, 2_520),
            at(2_520, 2_520),
            at(2_520, 2_880),
            at(1_800, 2_880),
            at(1_800, 2_520),
            at(1_440, 2_520),
        ],
    );

    // The regular `pentagon`, apex up, filling the box.
    assert_same_outline(
        &outline(ShapeGeometry::Pentagon, Vec::new()),
        &[
            at(2_160, 1_440),
            at(2_880, 1_990),
            at(2_605, 2_880),
            at(1_715, 2_880),
            at(1_440, 1_990),
        ],
    );

    // An authored `a:avLst` really moves the outline: the default `hexagon`
    // insets its corners by 360tw (25000 of the 1440tw shorter side), an
    // authored 40000 by 576tw.
    let default_hexagon = outline(ShapeGeometry::Hexagon, Vec::new());
    assert!(
        default_hexagon.contains(&at(1_800, 1_440)),
        "{default_hexagon:?}"
    );
    let authored_hexagon = outline(
        ShapeGeometry::Hexagon,
        vec![ShapeAdjustment {
            name: "adj".to_owned(),
            formula: "val 40000".to_owned(),
        }],
    );
    assert!(
        authored_hexagon.contains(&at(2_016, 1_440))
            && !authored_hexagon.contains(&at(1_800, 1_440)),
        "{authored_hexagon:?}"
    );
}

/// A grouped text box whose `wps:wsp` is an ELLIPSE paints as an ellipse with
/// its text inside — "modeled is not shipped".
///
/// Before this, the text box always painted a rectangle: the geometry was not
/// in the model at all, and layout had only one shape for every text box.
#[test]
fn a_text_box_with_a_preset_geometry_paints_that_geometry_behind_its_text() {
    let document = single_child_group_document(GroupChild::TextBox(GroupTextBox {
        hyperlink: None,
        id: node(91),
        offset: PointEmu { x_emu: 0, y_emu: 0 },
        extent: Extent {
            width_emu: 914_400,
            height_emu: 914_400,
        },
        geometry: ShapeGeometry::Ellipse,
        preset: None,
        adjustments: Vec::new(),
        blocks: vec![BlockNode::Paragraph(Paragraph {
            id: node(92),
            properties: ParagraphProperties::default().into(),
            inlines: vec![run(93, "Inside")],
        })],
        fill: Some(Fill::Solid(Rgba {
            r: 10,
            g: 20,
            b: 30,
            a: 255,
        })),
        border: None,
        body_properties: TextBoxBodyProperties::default(),
        flip_h: false,
        flip_v: false,
        rotation: None,
    }));

    let content = only_anchor_content(&document);
    let AnchorContent::TextBox {
        blocks,
        fill,
        backdrop,
        ..
    } = &content
    else {
        panic!("expected a text box, got {content:?}");
    };
    assert!(!blocks.is_empty(), "the text still flows inside the shape");
    let backdrop = backdrop.as_deref().expect("the ellipse behind the text");
    assert!(
        matches!(backdrop, AnchorContent::Ellipse { fill: Some(_), .. }),
        "the backdrop is the ellipse, carrying the box's fill: {backdrop:?}"
    );
    assert!(
        fill.is_none(),
        "the rectangular box fill is unset, so nothing paints twice"
    );

    // And it reaches paint as an ellipse, with no rectangle standing in for it.
    let shaper = ParleyShaper::new();
    let cfg = config();
    let galley = build_galley(&document, &shaper, cfg.content_area().size.width);
    let mut layout = paginate(&galley, &cfg);
    place_floats(&mut layout, &document, &shaper, &cfg);
    let list = compose_page(&layout.pages[0]);
    assert!(
        list.items.iter().any(|item| matches!(
            item,
            PaintItem::Shape {
                geometry: DisplayShapeGeometry::Ellipse { .. },
                ..
            }
        )),
        "the display list must carry the ellipse"
    );
    assert!(
        !list.items.iter().any(|item| matches!(
            item,
            PaintItem::Shape {
                geometry: DisplayShapeGeometry::Rect { .. },
                ..
            }
        )),
        "nothing may paint a rectangle where the shape is an ellipse"
    );
}

/// Every [`ShapeGeometry`] variant, in one list the exhaustive matches above
/// keep honest.
fn all_shape_geometries() -> Vec<ShapeGeometry> {
    vec![
        ShapeGeometry::Rectangle,
        ShapeGeometry::RoundRectangle,
        ShapeGeometry::Ellipse,
        ShapeGeometry::Triangle,
        ShapeGeometry::RightTriangle,
        ShapeGeometry::Diamond,
        ShapeGeometry::Line,
        ShapeGeometry::Pentagon,
        ShapeGeometry::Hexagon,
        ShapeGeometry::Octagon,
        ShapeGeometry::Star5,
        ShapeGeometry::Star4,
        ShapeGeometry::RightArrow,
        ShapeGeometry::LeftArrow,
        ShapeGeometry::UpArrow,
        ShapeGeometry::DownArrow,
        ShapeGeometry::LeftRightArrow,
        ShapeGeometry::Parallelogram,
        ShapeGeometry::Trapezoid,
        ShapeGeometry::Chevron,
        ShapeGeometry::HomePlate,
        ShapeGeometry::Plus,
        ShapeGeometry::Other,
    ]
}

/// A 1"×1" filled shape child carrying `geometry` and its authored guides.
fn preset_shape_child(geometry: ShapeGeometry, adjustments: Vec<ShapeAdjustment>) -> GroupChild {
    GroupChild::Shape(GroupShape {
        hyperlink: None,
        id: node(91),
        offset: PointEmu { x_emu: 0, y_emu: 0 },
        extent: Extent {
            width_emu: 914_400,
            height_emu: 914_400,
        },
        geometry,
        preset: None,
        adjustments,
        path: None,
        fill: Some(Fill::Solid(Rgba {
            r: 60,
            g: 120,
            b: 180,
            a: 255,
        })),
        stroke: None,
        flip_h: false,
        flip_v: false,
        rotation: None,
    })
}

// --- Square-family wrap exclusion arithmetic (`crate::wrap_side`) ----------
//
// Every guard below places a square float somewhere OTHER than hard against a
// margin — a mid-column `posOffset` (what dragging a shape produces), a
// centred `align`, a `Page`-relative anchor — because flush-left was the one
// position where the old arithmetic happened to agree with Word, and every
// existing float guard used it.
//
// They assert the GUARANTEE, not a pixel count: text keeps its measure on the
// side the float is not on, and does not run through the float.

/// The authored width of every float in this section.
const WRAP_FLOAT_WIDTH: Twip = Twip(1_500);
/// Tall enough that the clearance reaches several lines of the anchor paragraph
/// and still carries into the next paragraph of the same cell.
const WRAP_FLOAT_HEIGHT: Twip = Twip(1_800);
/// A `wrapSquare` rectangle anchored to its paragraph's top, positioned
/// horizontally by `position` against `relative_from`.
fn wrap_float(
    id: u64,
    media: MediaId,
    relative_from: HorizontalAnchor,
    position: HorizontalPosition,
    vertical: AnchorVertical,
) -> InlineNode {
    // The float's authored side is absent, so `wrap_side()` answers Word's
    // default of `bothSides`. These guards are about WHERE the band lands, not
    // which side the producer asked for; the authored side has its own guard,
    // `an_authored_wrap_text_decides_the_side_the_text_keeps`.
    wrap_float_sided(id, media, relative_from, position, vertical, None)
}

/// The same float with an explicit `w:wrap@wrapText`.
fn wrap_float_sided(
    id: u64,
    media: MediaId,
    relative_from: HorizontalAnchor,
    position: HorizontalPosition,
    vertical: AnchorVertical,
    wrap_text: Option<WrapSide>,
) -> InlineNode {
    InlineNode::AnchoredDrawing(Box::new(AnchoredDrawing {
        hyperlink: None,
        opacity: None,
        id: node(id),
        media,
        extent: Extent {
            width_emu: i64::from(WRAP_FLOAT_WIDTH.raw()) * 635,
            height_emu: i64::from(WRAP_FLOAT_HEIGHT.raw()) * 635,
        },
        anchor: DrawingAnchor {
            horizontal: AnchorHorizontal {
                relative_from,
                position,
            },
            vertical,
            wrap: WrapMode::Square,
            wrap_text,
            wrap_distances: WrapDistances::default(),
            wrap_polygon: None,
            behind_doc: false,
        },
        descr: None,
        relative_height: None,
        crop: None,
        border: None,
        flip_h: false,
        flip_v: false,
        rotation: None,
    }))
}

/// A float anchored to the top of its own paragraph.
fn at_paragraph_top() -> AnchorVertical {
    AnchorVertical {
        relative_from: VerticalAnchor::Paragraph,
        position: VerticalPosition::Offset(0),
    }
}

/// Enough text that any available measure is filled and the wrap is visible.
fn wrap_filler(id: u64) -> InlineNode {
    run(
        id,
        &"text that must keep the measure in front of the shape ".repeat(14),
    )
}

/// How far a line's text commits to the measure, relative to the paragraph's
/// content box: the pen position of its LAST glyph, not the pen position after
/// it.
///
/// A soft-wrapped line keeps the space that broke it, and that space's advance
/// hangs past the break point — Word does the same — so counting it would
/// charge a line for whitespace that paints nothing.
fn line_right_extent(line: &casual_doc_layout::text::Line) -> Twip {
    line.runs
        .iter()
        .map(|run| {
            let keep = run.glyphs.len().saturating_sub(1);
            Twip(
                run.origin.x.raw()
                    + run
                        .glyphs
                        .iter()
                        .take(keep)
                        .map(|glyph| glyph.advance.raw())
                        .sum::<i32>(),
            )
        })
        .max()
        .unwrap_or(Twip::ZERO)
}

/// A one-cell, 6,000-twip table whose cell holds exactly two paragraphs, so a
/// float anchored in the first can be seen to narrow both. A table cell is the
/// container the page-level float pass deliberately does not reach, so what is
/// asserted through this helper is the paragraph-local and carry arithmetic in
/// `flow`.
fn wrap_cell_table(first: Vec<InlineNode>, second: Vec<InlineNode>) -> BlockNode {
    BlockNode::Table(Box::new(Table {
        id: node(850),
        grid: vec![GridColumn {
            width_twips: Some(6_000),
        }],
        grid_change: None,
        properties: TableProperties::default(),
        rows: vec![TableRow {
            id: node(851),
            properties: TableRowProperties::default(),
            cells: vec![TableCell {
                id: node(852),
                properties: TableCellProperties::default(),
                blocks: vec![
                    BlockNode::Paragraph(Paragraph {
                        id: node(853),
                        properties: ParagraphProperties::default().into(),
                        inlines: first,
                    }),
                    BlockNode::Paragraph(Paragraph {
                        id: node(854),
                        properties: ParagraphProperties::default().into(),
                        inlines: second,
                    }),
                ],
            }],
        }],
    }))
}

/// Flows `table` and returns the row's single cell fragment.
fn wrap_cell(table: BlockNode, definitions: Definitions) -> casual_doc_layout::block::CellFragment {
    let document = Document::new(node(840), vec![table], definitions).unwrap();
    let galley = build_galley(
        &document,
        &ParleyShaper::new(),
        config().content_area().size.width,
    );
    let BlockFragment::TableRow { cells, .. } = &galley[0] else {
        panic!("expected a table row fragment");
    };
    cells[0].clone()
}

/// The cell's content measure — the box the cell's paragraphs lay out in, and
/// therefore the coordinate space a float's `posOffset` and the text's
/// `origin.x` share.
fn wrap_cell_measure(cell: &casual_doc_layout::block::CellFragment) -> Twip {
    Twip(cell.width.raw() - cell.margins.start.raw() - cell.margins.end.raw())
}

/// The lines of `block` the float's clearance covers.
fn covered_lines(block: &BlockFragment) -> Vec<&casual_doc_layout::text::Line> {
    let BlockFragment::Paragraph { lines, .. } = block else {
        panic!("expected a paragraph fragment");
    };
    let mut covered = Vec::new();
    let mut y = Twip::ZERO;
    for line in &lines.lines {
        if y.raw() < WRAP_FLOAT_HEIGHT.raw() && !line.runs.is_empty() {
            covered.push(line);
        }
        y = Twip(y.raw() + line.height.raw());
    }
    covered
}

#[test]
fn a_mid_column_float_leaves_the_text_the_measure_in_front_of_it() {
    let (media, definitions) = media_defs();

    // Control: the same paragraph with no float fills the whole cell measure,
    // so the assertions below cannot pass vacuously. It also reports the cell's
    // real measure, which is what the float's `posOffset` is placed against —
    // guessing it is how a guard ends up asserting the wrong side.
    let control = wrap_cell(
        wrap_cell_table(vec![wrap_filler(862)], Vec::new()),
        definitions.clone(),
    );
    let measure = wrap_cell_measure(&control);
    // Three fifths in: a mid-column drag, flush against neither edge, and the
    // leading gap is the larger one — so text must keep ITS OWN leading edge
    // and stop at the float, instead of being indented past it.
    let offset = Twip(measure.raw() * 3 / 5);
    assert!(
        offset.raw() > WRAP_FLOAT_WIDTH.raw()
            && offset.raw() + WRAP_FLOAT_WIDTH.raw() < measure.raw(),
        "the float must sit strictly inside the cell measure"
    );
    assert!(
        covered_lines(&control.blocks[0])
            .iter()
            .any(|line| line_right_extent(line).raw() > offset.raw()),
        "without the float, text reaches past the float's leading edge"
    );

    let float = wrap_float(
        860,
        media,
        HorizontalAnchor::Column,
        HorizontalPosition::Offset(i64::from(offset.raw()) * 635),
        at_paragraph_top(),
    );

    // The anchor paragraph's own lines (the paragraph-local exclusion).
    let anchored = wrap_cell(
        wrap_cell_table(vec![float.clone(), wrap_filler(863)], Vec::new()),
        definitions.clone(),
    );
    let covered = covered_lines(&anchored.blocks[0]);
    assert!(covered.len() >= 3, "the clearance covers several lines");
    for line in covered {
        assert_eq!(
            line.runs[0].origin.x,
            Twip::ZERO,
            "text keeps its own leading edge; the float is not on that side"
        );
        assert!(
            line_right_extent(line).raw() <= offset.raw(),
            "text must stop at the float's leading edge ({}), reached {}",
            offset.raw(),
            line_right_extent(line).raw()
        );
    }

    // The following paragraph in the same cell (the carried exclusion).
    let carried = wrap_cell(
        wrap_cell_table(vec![float], vec![wrap_filler(864)]),
        definitions,
    );
    let BlockFragment::Paragraph { lines, .. } = &carried.blocks[1] else {
        panic!("expected the second cell paragraph");
    };
    let first = lines
        .lines
        .iter()
        .find(|line| !line.runs.is_empty())
        .expect("the following paragraph has text");
    assert_eq!(first.runs[0].origin.x, Twip::ZERO);
    assert!(
        line_right_extent(first).raw() <= offset.raw(),
        "the carried exclusion must also stop at the float, reached {}",
        line_right_extent(first).raw()
    );
}

#[test]
fn a_centred_float_still_displaces_the_text_around_it() {
    let (media, definitions) = media_defs();
    let float = wrap_float(
        870,
        media,
        HorizontalAnchor::Column,
        HorizontalPosition::Align(HorizontalAlign::Center),
        at_paragraph_top(),
    );
    let cell = wrap_cell(
        wrap_cell_table(vec![float, wrap_filler(871)], Vec::new()),
        definitions,
    );
    // A centred float's band starts half the slack in from the leading edge.
    let band_start = Twip((wrap_cell_measure(&cell).raw() - WRAP_FLOAT_WIDTH.raw()) / 2);
    let covered = covered_lines(&cell.blocks[0]);
    assert!(covered.len() >= 3, "the clearance covers several lines");
    for line in covered {
        assert!(
            line_right_extent(line).raw() <= band_start.raw(),
            "text ran through the centred float: reached {} with the float at {}",
            line_right_extent(line).raw(),
            band_start.raw()
        );
    }
}

#[test]
fn a_floating_text_box_excludes_text_in_its_own_table_cell() {
    let (_, definitions) = media_defs();
    let text_box = InlineNode::TextBox(Box::new(TextBox {
        hyperlink: None,
        id: node(880),
        anchor: Some(DrawingAnchor {
            horizontal: AnchorHorizontal {
                relative_from: HorizontalAnchor::Column,
                position: HorizontalPosition::Align(HorizontalAlign::Left),
            },
            vertical: at_paragraph_top(),
            wrap: WrapMode::Square,
            // The float's authored side is absent, so `wrap_side()` answers
            // Word's default of `bothSides`. These guards are about WHERE the
            // band lands, not which side the producer asked for.
            wrap_text: None,
            wrap_distances: WrapDistances::default(),
            wrap_polygon: None,
            behind_doc: false,
        }),
        relative_height: None,
        extent: Some(Extent {
            width_emu: i64::from(WRAP_FLOAT_WIDTH.raw()) * 635,
            height_emu: i64::from(WRAP_FLOAT_HEIGHT.raw()) * 635,
        }),
        fill: None,
        border: None,
        body_properties: Default::default(),
        blocks: vec![BlockNode::Paragraph(Paragraph {
            id: node(881),
            properties: ParagraphProperties::default().into(),
            inlines: vec![run(882, "pull quote")],
        })],
    }));

    let control = wrap_cell(
        wrap_cell_table(Vec::new(), vec![wrap_filler(883)]),
        definitions.clone(),
    );
    let BlockFragment::Paragraph { lines, .. } = &control.blocks[1] else {
        panic!("expected the second cell paragraph");
    };
    assert_eq!(
        lines.lines[0].runs[0].origin.x,
        Twip::ZERO,
        "without the text box the following paragraph starts at the cell edge"
    );

    let cell = wrap_cell(
        wrap_cell_table(vec![text_box], vec![wrap_filler(884)]),
        definitions,
    );
    let BlockFragment::Paragraph { lines, .. } = &cell.blocks[1] else {
        panic!("expected the second cell paragraph");
    };
    let first = lines
        .lines
        .iter()
        .find(|line| !line.runs.is_empty())
        .expect("the following paragraph has text");
    assert!(
        first.runs[0].origin.x.raw() >= WRAP_FLOAT_WIDTH.raw(),
        "a floating text box must exclude the following paragraph in its own \
         cell; the line starts at {} with the box {} wide",
        first.runs[0].origin.x.raw(),
        WRAP_FLOAT_WIDTH.raw()
    );
}

#[test]
fn a_page_relative_float_centred_in_the_measure_narrows_the_trailing_edge() {
    use casual_doc_layout::document_layout::paginate_document;

    // US-Letter with 1-inch margins (`paginate_document`'s fallback for a
    // document that declares no section): the body measure is 1440..10800.
    let body_start = Twip(1_440);
    let measure = Twip(9_360);
    // A `Page`-relative `posOffset` that centres the float's band in the body
    // measure — the one position the old midpoint comparison resolved the wrong
    // way, sending the text to the far side of the page instead of leaving it
    // at its own leading edge.
    let band_start = Twip(body_start.raw() + (measure.raw() - WRAP_FLOAT_WIDTH.raw()) / 2);
    let gap = Twip(band_start.raw() - body_start.raw());

    let (media, definitions) = media_defs();
    let float = wrap_float(
        890,
        media,
        HorizontalAnchor::Page,
        HorizontalPosition::Offset(i64::from(band_start.raw()) * 635),
        AnchorVertical {
            relative_from: VerticalAnchor::Page,
            position: VerticalPosition::Offset(i64::from(body_start.raw()) * 635),
        },
    );
    let body = |inlines: Vec<InlineNode>| {
        vec![
            BlockNode::Paragraph(Paragraph {
                id: node(891),
                properties: ParagraphProperties::default().into(),
                inlines,
            }),
            BlockNode::Paragraph(Paragraph {
                id: node(892),
                properties: ParagraphProperties::default().into(),
                inlines: vec![wrap_filler(893)],
            }),
        ]
    };
    let lines_of = |blocks: Vec<BlockNode>, definitions: Definitions| {
        let document = Document::new(node(894), blocks, definitions).unwrap();
        let layout = paginate_document(&document, &ParleyShaper::new());
        let placed = layout.pages[0]
            .placed
            .iter()
            .find(|placed| placed.fragment.node_id() == node(891))
            .expect("the anchoring paragraph is placed")
            .clone();
        let BlockFragment::Paragraph { lines, .. } = &placed.fragment else {
            panic!("expected a paragraph fragment");
        };
        lines.clone()
    };

    let control = lines_of(
        body(vec![wrap_filler(895)]),
        Definitions {
            media: definitions.media.clone(),
            ..Definitions::default()
        },
    );
    assert!(
        control
            .lines
            .iter()
            .filter(|line| !line.runs.is_empty())
            .any(|line| line_right_extent(line).raw() > gap.raw()),
        "without the float the paragraph reaches past the float's leading edge"
    );

    let wrapped = lines_of(body(vec![float, wrap_filler(896)]), definitions);
    let covered: Vec<_> = wrapped
        .lines
        .iter()
        .filter(|line| !line.runs.is_empty())
        .take(3)
        .collect();
    assert_eq!(covered.len(), 3, "the clearance covers several lines");
    for line in covered {
        assert_eq!(
            line.runs[0].origin.x,
            Twip::ZERO,
            "a float centred in the measure must not push the text to the far \
             side of the page"
        );
        assert!(
            line_right_extent(line).raw() <= gap.raw(),
            "text ran through the float: reached {} with the float at {}",
            line_right_extent(line).raw(),
            gap.raw()
        );
    }
}

/// **The authored `w:wrap@wrapText` decides which gap the text keeps, and it
/// beats the geometry.**
///
/// `casual-doc-layout::wrap_side::wrap_sides` used to **ignore its argument** and
/// answer `BothSides` for every float, so `DrawingAnchor::wrap_text` — imported,
/// modelled and exported by #738/#739 — was discarded at layout for every float
/// in every document. The exclusion arithmetic in `band_exclusion` was complete
/// and guarded the whole time, including `authored_sides_override_the_geometry`;
/// what no guard asked was whether the engine ever *handed* it the authored
/// value. That is the shape of a test that cannot fail, so this guard is written
/// at the only altitude that could have caught it: a laid-out paragraph.
///
/// The float sits three fifths across the measure, so the **leading** gap is the
/// wider one and `bothSides` keeps it (the documented approximation — our line
/// geometry is one measure with two insets, so a hole in the middle of a line is
/// not representable, and `bothSides` therefore keeps the wider gap, identical
/// to `largest`). An authored `right` asks for the gap geometry would NOT have
/// chosen, which is what makes the two cases distinguishable: under the stub
/// both read identically.
///
/// Asserted as a guarantee rather than a pixel count: with `bothSides` the text
/// keeps its own leading edge and stops before the float; with `right` the text
/// starts beyond the float's trailing edge. Neither bound is a measured constant.
#[test]
fn an_authored_wrap_text_decides_the_side_the_text_keeps() {
    let (media, definitions) = media_defs();

    let measure = wrap_cell_measure(&wrap_cell(
        wrap_cell_table(vec![wrap_filler(872)], Vec::new()),
        definitions.clone(),
    ));
    let offset = Twip(measure.raw() * 3 / 5);
    let band_end = Twip(offset.raw() + WRAP_FLOAT_WIDTH.raw());
    assert!(
        offset.raw() > measure.raw() - band_end.raw(),
        "the float must sit so the LEADING gap is the wider one, otherwise \
         `bothSides` and `right` would agree and this guard could not fail"
    );

    let laid_out = |wrap_text: Option<WrapSide>| {
        let float = wrap_float_sided(
            870,
            media,
            HorizontalAnchor::Column,
            HorizontalPosition::Offset(i64::from(offset.raw()) * 635),
            at_paragraph_top(),
            wrap_text,
        );
        let cell = wrap_cell(
            wrap_cell_table(vec![float, wrap_filler(871)], Vec::new()),
            definitions.clone(),
        );
        let lines = covered_lines(&cell.blocks[0]);
        assert!(
            !lines.is_empty(),
            "the float's clearance must cover at least one line of text, or \
             there is nothing for either case to be measured on"
        );
        lines
            .iter()
            .map(|line| {
                let start = line
                    .runs
                    .iter()
                    .map(|run| run.origin.x)
                    .min()
                    .expect("a covered line has a run");
                (start, line_right_extent(line))
            })
            .collect::<Vec<_>>()
    };

    // Default (absent `wrapText`): text keeps the wider LEADING gap.
    for (start, end) in laid_out(None) {
        assert_eq!(
            start,
            Twip::ZERO,
            "with the default `bothSides` the text keeps its own leading edge"
        );
        assert!(
            end.raw() <= offset.raw(),
            "with the default `bothSides` the text stops at the float's leading edge: \
             ends at {end:?}, float starts at {offset:?}"
        );
    }

    // Authored `right`: text flows only down the channel to the float's right,
    // which is the NARROWER gap here, so geometry alone would never pick it.
    for (start, _) in laid_out(Some(WrapSide::Right)) {
        assert!(
            start.raw() >= band_end.raw(),
            "an authored right-side wrapText puts the text beyond the float's \
             trailing edge: starts at {start:?}, float ends at {band_end:?}"
        );
    }
}

/// The authored outline geometry must TRAVEL, not merely be renderable.
///
/// `casual-doc-render`'s own guards build a `ShapeOutline` directly, so they prove the
/// rasterizer applies a cap, a join and an authored `a:custDash` — and prove nothing
/// about whether those values ever reach it. Two mutations showed exactly that hole:
/// making `compose` drop the three fields, and making the shared `GroupChild` walk
/// never look the side table up, both left the render suite GREEN while every
/// outline in every document silently lost its geometry.
///
/// So this drives the real path: a `Definitions::shape_fill_detail` row, through the
/// placement walk, through `compose_page`, to the display list the backend consumes.
#[test]
fn an_authored_cap_join_and_custom_dash_reach_the_display_list() {
    use casual_doc_layout::compose::compose_page;
    use casual_doc_layout::display::PaintItem;
    use casual_doc_model::v1::{
        DashStop, LineCap, LineJoin, Rgba, ShapeFillDetail, ShapeStroke, StrokeDetail,
    };

    let shape_id = node(91);
    let child = GroupChild::Shape(GroupShape {
        id: shape_id,
        offset: PointEmu { x_emu: 0, y_emu: 0 },
        extent: Extent {
            width_emu: 914_400,
            height_emu: 914_400,
        },
        geometry: ShapeGeometry::Rectangle,
        preset: None,
        adjustments: Vec::new(),
        path: None,
        fill: None,
        stroke: Some(ShapeStroke {
            color: Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            width_emu: 28_575,
            dash: None,
            head_end: None,
            tail_end: None,
        }),
        flip_h: false,
        flip_v: false,
        rotation: None,
        hyperlink: None,
    });

    // The same document the other float guards use, plus the side-table row that
    // carries what `ShapeStroke` has nowhere to put.
    let mut document = single_child_group_document(child);
    document.definitions_mut().shape_fill_detail.insert(
        shape_id,
        ShapeFillDetail {
            picture: None,
            pattern: None,
            gradient: None,
            stroke: Some(StrokeDetail {
                cap: Some(LineCap::Round),
                compound: None,
                align: None,
                join: Some(LineJoin::Bevel),
                custom_dash: vec![DashStop {
                    dash: 400_000,
                    space: 200_000,
                }],
            }),
            // Not what this fixture asserts; the shadow has its own guards.
            outer_shadow: None,
        },
    );

    let shaper = ParleyShaper::new();
    let cfg = config();
    let galley = build_galley(&document, &shaper, cfg.content_area().size.width);
    let mut layout = paginate(&galley, &cfg);
    place_floats(&mut layout, &document, &shaper, &cfg);
    let list = compose_page(&layout.pages[0]);

    let outline = list
        .items
        .iter()
        .find_map(|item| match item {
            PaintItem::Shape {
                stroke: Some(stroke),
                ..
            } => Some(stroke.clone()),
            _ => None,
        })
        .expect("the shape's outline reached the display list");

    assert_eq!(
        outline.cap,
        Some(LineCap::Round),
        "the authored cap must survive the walk and compose"
    );
    assert_eq!(outline.join, Some(LineJoin::Bevel), "and so must the join");
    assert_eq!(
        outline.custom_dash,
        vec![DashStop {
            dash: 400_000,
            space: 200_000
        }],
        "and so must the authored dash pattern"
    );
}

/// A vertical text box turns its TEXT and leaves its chrome upright.
///
/// `105` FID-L-08: `wps:bodyPr@vert` was not read at all, so a Word text box with
/// vertical text imported and rendered axis-aligned with nothing reported. The seam
/// to paint it has existed since the watermark introduced
/// `PaintItem::PushLayer`, whose own doc comment named this row as the caller it was
/// waiting for.
///
/// Three things are asserted together because each is a different way to get this
/// wrong: the text is wrapped in a layer (it rotates at all), the layer wraps ONLY
/// the clip and the blocks (the backdrop and border stay upright), and the box is
/// measured against its transposed axis (a quarter turn flows text along the short
/// side, so breaking lines at the upright width is the defect that looks like a
/// layout bug rather than a missing attribute).
#[test]
fn a_vertical_text_box_turns_its_text_and_not_its_chrome() {
    use casual_doc_layout::compose::compose_page;
    use casual_doc_layout::display::PaintItem;
    use casual_doc_model::v1::TextVertical;

    // Built per direction rather than mutated, because `body_properties` sits inside
    // a boxed inline inside a paragraph and reaching it to mutate would make the
    // fixture about navigation rather than about rotation.
    let child = |vertical: TextVertical| {
        GroupChild::TextBox(GroupTextBox {
            hyperlink: None,
            id: node(601),
            offset: PointEmu { x_emu: 0, y_emu: 0 },
            // Deliberately TALLER than it is wide, so a quarter turn gives the text a
            // longer line to run along and a transposed flow is observable.
            extent: Extent {
                width_emu: 400 * 635,
                height_emu: 1_200 * 635,
            },
            geometry: ShapeGeometry::Rectangle,
            preset: None,
            adjustments: Vec::new(),
            blocks: vec![BlockNode::Paragraph(Paragraph {
                id: node(602),
                properties: ParagraphProperties::default().into(),
                inlines: vec![run(603, "turned text")],
            })],
            // A fill and a border, so "the chrome stays upright" is something the
            // assertion can actually be about.
            fill: Some(Fill::Solid(Rgba {
                r: 200,
                g: 200,
                b: 200,
                a: 255,
            })),
            border: Some(ShapeStroke {
                color: Rgba {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                },
                width_emu: 12_700,
                dash: None,
                head_end: None,
                tail_end: None,
            }),
            body_properties: TextBoxBodyProperties {
                vertical,
                ..TextBoxBodyProperties::default()
            },
            flip_h: false,
            flip_v: false,
            rotation: None,
        })
    };
    let place = |vertical: TextVertical| {
        let document = single_child_group_document(child(vertical));
        let shaper = ParleyShaper::new();
        let cfg = config();
        let galley = build_galley(&document, &shaper, cfg.content_area().size.width);
        let mut layout = paginate(&galley, &cfg);
        place_floats(&mut layout, &document, &shaper, &cfg);
        compose_page(&layout.pages[0])
    };

    let upright = place(TextVertical::Horizontal);
    let turned = place(TextVertical::Vertical);

    // Upright text pushes no layer at all, so the seam costs an ordinary document
    // nothing.
    assert_eq!(
        upright
            .items
            .iter()
            .filter(|item| matches!(item, PaintItem::PushLayer { .. }))
            .count(),
        0,
        "an upright text box must not push a layer"
    );

    // Vertical text pushes exactly one, carrying a rotation.
    let layers: Vec<&PaintItem> = turned
        .items
        .iter()
        .filter(|item| matches!(item, PaintItem::PushLayer { .. }))
        .collect();
    assert_eq!(layers.len(), 1, "exactly one layer for the turned text");
    match layers[0] {
        PaintItem::PushLayer { transform, .. } => {
            let transform = transform.expect("the layer carries a rotation");
            assert_eq!(
                transform.rotation, 5_400_000,
                "`vert` is a quarter turn clockwise"
            );
        }
        other => panic!("expected a layer, got {other:?}"),
    }

    // The layer opens AFTER the chrome and closes before the end, so the backdrop
    // and border are outside it and stay upright. Asserted by position, because a
    // layer that wrapped everything would still contain a rotation and still pass a
    // "there is a layer" check.
    let push = turned
        .items
        .iter()
        .position(|item| matches!(item, PaintItem::PushLayer { .. }))
        .expect("a layer");
    let clip = turned
        .items
        .iter()
        .position(|item| matches!(item, PaintItem::PushClip(_)))
        .expect("the content clip");
    let pop = turned
        .items
        .iter()
        .position(|item| matches!(item, PaintItem::PopLayer))
        .expect("the layer closes");
    assert!(
        push < clip && clip < pop,
        "the layer must wrap the content clip and nothing before it: \
         push={push} clip={clip} pop={pop}"
    );
}

/// A quarter turn flows the text along the box's LONG axis.
///
/// The half of `wps:bodyPr@vert` that looks like a layout bug rather than a missing
/// attribute: flowing a turned box against its upright width breaks every line at
/// the wrong measure. `emit_text_box` measures against the transposed box, and this
/// is what makes that observable — a 400x1200tw box wraps the same sentence into
/// SEVEN runs upright and FIVE when turned, because the turned text has 1,200tw of
/// line to run along instead of 400.
///
/// Counted in glyph runs rather than asserted as a rect, because the box has a fixed
/// extent: the rect is 400x1200 either way, so a rect assertion cannot see this at
/// all. Disabling the transposition leaves the two counts equal.
#[test]
fn a_quarter_turn_flows_the_text_along_the_boxs_long_axis() {
    use casual_doc_layout::display::PaintItem;
    use casual_doc_model::v1::TextVertical;

    let runs = |vertical: TextVertical| {
        let document = single_child_group_document(turnable_text_box(vertical));
        let shaper = ParleyShaper::new();
        let cfg = config();
        let galley = build_galley(&document, &shaper, cfg.content_area().size.width);
        let mut layout = paginate(&galley, &cfg);
        place_floats(&mut layout, &document, &shaper, &cfg);
        casual_doc_layout::compose::compose_page(&layout.pages[0])
            .items
            .iter()
            .filter(|item| matches!(item, PaintItem::Glyphs { .. }))
            .count()
    };
    let upright = runs(TextVertical::Horizontal);
    let turned = runs(TextVertical::Vertical);
    assert!(
        turned < upright,
        "a turned box has a longer line to run along, so it must wrap LESS: \
         upright={upright} turned={turned}"
    );
    assert_eq!((upright, turned), (7, 5), "the measured counts");
}

/// A flow direction this build cannot express as a rotation is reported, never
/// approximated by one.
///
/// `eaVert` and the three WordArt directions re-order and re-orient individual
/// glyphs; a layer transform cannot express that. Approximating them with a quarter
/// turn would put something on screen that looks deliberate and is not what the file
/// says, so `TextVertical::layer_rotation` returns `None` for them and the box paints
/// upright with the loss named at import.
#[test]
fn a_flow_direction_that_is_not_a_rotation_pushes_no_layer() {
    use casual_doc_layout::display::PaintItem;
    use casual_doc_model::v1::TextVertical;

    // Derived from the enum rather than listed, so a new direction cannot be added
    // without deciding which side of this line it falls on.
    let not_rotations: Vec<TextVertical> = TextVertical::ALL
        .into_iter()
        .filter(|kind| kind.layer_rotation().is_none())
        .collect();
    assert_eq!(
        not_rotations.len(),
        4,
        "eaVert and the three WordArt directions: {not_rotations:?}"
    );

    for vertical in not_rotations {
        let document = single_child_group_document(turnable_text_box(vertical));
        let shaper = ParleyShaper::new();
        let cfg = config();
        let galley = build_galley(&document, &shaper, cfg.content_area().size.width);
        let mut layout = paginate(&galley, &cfg);
        place_floats(&mut layout, &document, &shaper, &cfg);
        let list = casual_doc_layout::compose::compose_page(&layout.pages[0]);
        assert_eq!(
            list.items
                .iter()
                .filter(|item| matches!(item, PaintItem::PushLayer { .. }))
                .count(),
            0,
            "{} must paint upright and be reported, not rotated",
            vertical.token()
        );
    }

    // And the two that ARE plain rotations do push one, so the assertion above is
    // about these four specifically and not about layers never being pushed.
    for vertical in [TextVertical::Vertical, TextVertical::Vertical270] {
        let document = single_child_group_document(turnable_text_box(vertical));
        let shaper = ParleyShaper::new();
        let cfg = config();
        let galley = build_galley(&document, &shaper, cfg.content_area().size.width);
        let mut layout = paginate(&galley, &cfg);
        place_floats(&mut layout, &document, &shaper, &cfg);
        let list = casual_doc_layout::compose::compose_page(&layout.pages[0]);
        assert_eq!(
            list.items
                .iter()
                .filter(|item| matches!(item, PaintItem::PushLayer { .. }))
                .count(),
            1,
            "{} is a plain rotation and must push a layer",
            vertical.token()
        );
    }
}

/// A grouped text box with a wrapping sentence, so a change of flow axis is visible.
fn turnable_text_box(vertical: casual_doc_model::v1::TextVertical) -> GroupChild {
    GroupChild::TextBox(GroupTextBox {
        hyperlink: None,
        id: node(701),
        offset: PointEmu { x_emu: 0, y_emu: 0 },
        // Deliberately far taller than wide: the whole point is that a quarter turn
        // swaps which dimension bounds the line.
        extent: Extent {
            width_emu: 400 * 635,
            height_emu: 1_200 * 635,
        },
        geometry: ShapeGeometry::Rectangle,
        preset: None,
        adjustments: Vec::new(),
        blocks: vec![BlockNode::Paragraph(Paragraph {
            id: node(702),
            properties: ParagraphProperties::default().into(),
            inlines: vec![run(703, "one two three four five six")],
        })],
        fill: None,
        border: None,
        body_properties: TextBoxBodyProperties {
            vertical,
            ..TextBoxBodyProperties::default()
        },
        flip_h: false,
        flip_v: false,
        rotation: None,
    })
}

/// A picture-filled shape paints its image CLIPPED to its own outline.
///
/// `docs/156` §6 row 0.3 and row 0.2 were both blocked on one missing primitive, not
/// on modelling: `PaintItem::PushClip` took a rectangle, so a picture-filled ellipse
/// or star could not be drawn at all and the fill was reported-and-dropped.
/// `PaintItem::PushClipPath` is that primitive.
///
/// Asserted as the expansion rather than as one item, because the whole point is that
/// it is three: clip, image, unclip — and the outline stroked OVER the picture, which
/// is the order Word draws it in. A stroke under a stretched fill is half-covered.
/// A Word shape carrying `a:outerShdw` casts a shadow: the anchor's paint items are
/// bracketed in a layer that states the blur, the cartesian offset and the colour.
///
/// This is the end of the chain the row needed — importer, model, layout, display
/// list, raster. The raster primitive has its own guards in `casual-doc-render`; what
/// this one asserts is that something actually ASKS for it, which is the half that
/// was missing when the blur landed: a shape could carry a shadow nothing emitted.
#[test]
fn a_shape_with_an_outer_shadow_casts_it_through_a_layer() {
    use casual_doc_layout::compose::compose_page;
    use casual_doc_layout::display::PaintItem;
    use casual_doc_model::v1::{OuterShadow, ShapeFillDetail};

    let shape_id = node(93);
    let child = GroupChild::Shape(GroupShape {
        hyperlink: None,
        id: shape_id,
        offset: PointEmu { x_emu: 0, y_emu: 0 },
        extent: Extent {
            width_emu: 914_400,
            height_emu: 914_400,
        },
        geometry: ShapeGeometry::Rectangle,
        preset: None,
        adjustments: Vec::new(),
        path: None,
        fill: Some(Fill::Solid(Rgba {
            r: 0x44,
            g: 0x72,
            b: 0xC4,
            a: 255,
        })),
        stroke: None,
        flip_h: false,
        flip_v: false,
        rotation: None,
    });

    let mut document = single_child_group_document(child);
    document.definitions_mut().shape_fill_detail.insert(
        shape_id,
        ShapeFillDetail {
            picture: None,
            pattern: None,
            gradient: None,
            stroke: None,
            outer_shadow: Some(OuterShadow {
                // 50800 EMU = 80 twips of blur.
                blur_radius_emu: 50_800,
                // 25400 EMU = 40 twips of distance, thrown at 45 degrees.
                distance_emu: 25_400,
                direction: 2_700_000,
                color: Rgba {
                    r: 0x80,
                    g: 0x80,
                    b: 0x80,
                    a: 128,
                },
            }),
        },
    );

    let shaper = ParleyShaper::new();
    let cfg = config();
    let galley = build_galley(&document, &shaper, cfg.content_area().size.width);
    let mut layout = paginate(&galley, &cfg);
    place_floats(&mut layout, &document, &shaper, &cfg);
    let list = compose_page(&layout.pages[0]);

    let layer_at = list
        .items
        .iter()
        .position(|item| {
            matches!(
                item,
                PaintItem::PushLayer {
                    shadow: Some(_),
                    ..
                }
            )
        })
        .expect("the shadowed shape opens a layer that states its shadow");
    let PaintItem::PushLayer {
        shadow: Some(shadow),
        ..
    } = &list.items[layer_at]
    else {
        unreachable!("matched above")
    };
    assert_eq!(shadow.blur, Twip(80), "50800 EMU of blurRad is 80 twips");
    // `@dist`/`@dir` are polar; the display list is cartesian. 40 twips at 45
    // degrees is 40 * cos(45) on each axis, and BOTH are positive because the
    // page's y axis points down — a negated sine would throw the shadow up-left,
    // which reads as a light source nobody chose.
    assert_eq!(shadow.offset_x, Twip(28));
    assert_eq!(shadow.offset_y, Twip(28));
    assert_eq!(
        (
            shadow.color.r,
            shadow.color.g,
            shadow.color.b,
            shadow.color.a
        ),
        (0x80, 0x80, 0x80, 128),
        "the shadow's own colour and its folded alpha reach the backend"
    );

    // The shape paints INSIDE the layer, and the layer closes. A shadow whose
    // bracket did not contain the shape would cast the shadow of nothing.
    let shape_at = list
        .items
        .iter()
        .skip(layer_at)
        .position(|item| matches!(item, PaintItem::Shape { .. }))
        .map(|offset| offset + layer_at)
        .expect("the shape paints");
    let pop_at = list
        .items
        .iter()
        .skip(shape_at)
        .position(|item| matches!(item, PaintItem::PopLayer))
        .map(|offset| offset + shape_at)
        .expect("the layer closes");
    assert!(
        layer_at < shape_at && shape_at < pop_at,
        "layer={layer_at} shape={shape_at} pop={pop_at}"
    );
}

/// A fully transparent shadow colour is NOT a layer.
///
/// Word writes `<a:alpha val="0"/>` on a shadow that has been switched off in the
/// UI rather than removing the effect, so this is the common case and not a corner:
/// an off-screen composite per shape for ink nobody can see is a cost with no
/// picture to show for it.
#[test]
fn an_invisible_shadow_opens_no_layer() {
    use casual_doc_layout::compose::compose_page;
    use casual_doc_layout::display::PaintItem;
    use casual_doc_model::v1::{OuterShadow, ShapeFillDetail};

    let shape_id = node(94);
    let child = GroupChild::Shape(GroupShape {
        hyperlink: None,
        id: shape_id,
        offset: PointEmu { x_emu: 0, y_emu: 0 },
        extent: Extent {
            width_emu: 914_400,
            height_emu: 914_400,
        },
        geometry: ShapeGeometry::Rectangle,
        preset: None,
        adjustments: Vec::new(),
        path: None,
        fill: Some(Fill::Solid(Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        })),
        stroke: None,
        flip_h: false,
        flip_v: false,
        rotation: None,
    });

    let mut document = single_child_group_document(child);
    document.definitions_mut().shape_fill_detail.insert(
        shape_id,
        ShapeFillDetail {
            picture: None,
            pattern: None,
            gradient: None,
            stroke: None,
            outer_shadow: Some(OuterShadow {
                blur_radius_emu: 50_800,
                distance_emu: 25_400,
                direction: 2_700_000,
                color: Rgba {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 0,
                },
            }),
        },
    );

    let shaper = ParleyShaper::new();
    let cfg = config();
    let galley = build_galley(&document, &shaper, cfg.content_area().size.width);
    let mut layout = paginate(&galley, &cfg);
    place_floats(&mut layout, &document, &shaper, &cfg);
    let list = compose_page(&layout.pages[0]);

    assert!(
        !list.items.iter().any(|item| matches!(
            item,
            PaintItem::PushLayer {
                shadow: Some(_),
                ..
            }
        )),
        "a shadow with zero alpha is not painted, so it opens no layer: {:?}",
        list.items
    );
    assert!(
        list.items
            .iter()
            .any(|item| matches!(item, PaintItem::Shape { .. })),
        "and the shape itself is unaffected"
    );
}

#[test]
fn a_picture_filled_shape_clips_its_image_to_its_outline() {
    use casual_doc_layout::compose::compose_page;
    use casual_doc_layout::display::PaintItem;
    use casual_doc_model::v1::{
        MediaId, MediaReference, PictureFill, PictureFillMode, ShapeFillDetail, ShapeStroke,
    };

    let shape_id = node(91);
    let media = MediaId::new(node(900));
    let child = GroupChild::Shape(GroupShape {
        hyperlink: None,
        id: shape_id,
        offset: PointEmu { x_emu: 0, y_emu: 0 },
        extent: Extent {
            width_emu: 914_400,
            height_emu: 914_400,
        },
        // A TRIANGLE, deliberately: a rectangle's clip would be indistinguishable
        // from no clip at all, so the guard would pass without the primitive.
        geometry: ShapeGeometry::Triangle,
        preset: None,
        adjustments: Vec::new(),
        path: None,
        fill: None,
        stroke: Some(ShapeStroke {
            color: Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            width_emu: 12_700,
            dash: None,
            head_end: None,
            tail_end: None,
        }),
        flip_h: false,
        flip_v: false,
        rotation: None,
    });

    let mut document = single_child_group_document(child);
    let definitions = document.definitions_mut();
    definitions.media.insert(
        media,
        MediaReference {
            relationship_id: "rId9".to_owned(),
            media_type: "image/png".to_owned(),
            part_name: "/word/media/fill.png".to_owned(),
        },
    );
    definitions.shape_fill_detail.insert(
        shape_id,
        ShapeFillDetail {
            picture: Some(PictureFill {
                media,
                mode: PictureFillMode::Stretch { fill_rect: None },
                crop: None,
                opacity: None,
                rotate_with_shape: None,
            }),
            pattern: None,
            gradient: None,
            stroke: None,
            // Not what this fixture asserts; the shadow has its own guards.
            outer_shadow: None,
        },
    );

    let shaper = ParleyShaper::new();
    let cfg = config();
    let galley = build_galley(&document, &shaper, cfg.content_area().size.width);
    let mut layout = paginate(&galley, &cfg);
    place_floats(&mut layout, &document, &shaper, &cfg);
    let list = compose_page(&layout.pages[0]);

    let clip_at = list
        .items
        .iter()
        .position(|item| matches!(item, PaintItem::PushClipPath { .. }))
        .expect("the picture is clipped to a PATH, not to a rectangle");
    let image_at = list
        .items
        .iter()
        .position(|item| matches!(item, PaintItem::Image { .. }))
        .expect("the fill paints as an image");
    let pop_at = list
        .items
        .iter()
        .skip(clip_at)
        .position(|item| matches!(item, PaintItem::PopClip))
        .map(|offset| offset + clip_at)
        .expect("the clip closes");
    assert!(
        clip_at < image_at && image_at < pop_at,
        "the image must paint INSIDE the clip: clip={clip_at} image={image_at} pop={pop_at}"
    );

    // The clip is the shape's own outline — a triangle's three vertices, not a
    // rectangle's four.
    match &list.items[clip_at] {
        PaintItem::PushClipPath { commands, closed } => {
            // The geometry engine closes a subpath with an explicit `Close`
            // command, so a closed outline is either flagged or ends in one.
            assert!(
                *closed
                    || matches!(
                        commands.last(),
                        Some(casual_doc_layout::display::PathCommand::Close)
                    ),
                "a filled shape's outline closes: {commands:?}"
            );
            let vertices = commands
                .iter()
                .filter(|command| {
                    !matches!(command, casual_doc_layout::display::PathCommand::Close)
                })
                .count();
            assert_eq!(
                vertices, 3,
                "the triangle's own outline clips the picture: {commands:?}"
            );
        }
        other => panic!("expected a path clip, got {other:?}"),
    }

    // And the outline is stroked AFTER the clip closes, so it paints over the
    // picture rather than under it.
    let stroke_at = list
        .items
        .iter()
        .position(|item| {
            matches!(
                item,
                PaintItem::Shape {
                    stroke: Some(_),
                    fill: None,
                    ..
                }
            )
        })
        .expect("the outline is stroked");
    assert!(
        stroke_at > pop_at,
        "the outline strokes over the picture, not under it: stroke={stroke_at} pop={pop_at}"
    );
}

/// A TILED picture fill is reported, not stretched.
///
/// `a:tile` repeats the picture from an offset at a scale; the display list has no
/// tiling primitive. Painting it stretched instead would put a single
/// shape-filling image where the file says a repeating pattern — the failure looks
/// deliberate, which is the whole reason the policy here is report-not-approximate
/// (the same call `a:pattFill` already takes).
///
/// Written because the stretch guard above could not see this: its fixture only
/// carries a stretch, so accepting a tile AS a stretch left it green.
#[test]
fn a_tiled_picture_fill_is_not_painted_as_a_stretch() {
    use casual_doc_layout::compose::compose_page;
    use casual_doc_layout::display::PaintItem;
    use casual_doc_model::v1::{
        MediaId, MediaReference, PictureFill, PictureFillMode, ShapeFillDetail,
    };

    let shape_id = node(91);
    let media = MediaId::new(node(900));
    let child = GroupChild::Shape(GroupShape {
        hyperlink: None,
        id: shape_id,
        offset: PointEmu { x_emu: 0, y_emu: 0 },
        extent: Extent {
            width_emu: 914_400,
            height_emu: 914_400,
        },
        geometry: ShapeGeometry::Triangle,
        preset: None,
        adjustments: Vec::new(),
        path: None,
        fill: None,
        stroke: None,
        flip_h: false,
        flip_v: false,
        rotation: None,
    });

    let mut document = single_child_group_document(child);
    let definitions = document.definitions_mut();
    definitions.media.insert(
        media,
        MediaReference {
            relationship_id: "rId9".to_owned(),
            media_type: "image/png".to_owned(),
            part_name: "/word/media/fill.png".to_owned(),
        },
    );
    definitions.shape_fill_detail.insert(
        shape_id,
        ShapeFillDetail {
            picture: Some(PictureFill {
                media,
                mode: PictureFillMode::Tile {
                    offset_x_emu: 0,
                    offset_y_emu: 0,
                    scale_x: None,
                    scale_y: None,
                    flip: casual_doc_model::v1::TileFlip::default(),
                    alignment: casual_doc_model::v1::RectAlignment::TopLeft,
                },
                crop: None,
                opacity: None,
                rotate_with_shape: None,
            }),
            pattern: None,
            gradient: None,
            stroke: None,
            // Not what this fixture asserts; the shadow has its own guards.
            outer_shadow: None,
        },
    );

    let shaper = ParleyShaper::new();
    let cfg = config();
    let galley = build_galley(&document, &shaper, cfg.content_area().size.width);
    let mut layout = paginate(&galley, &cfg);
    place_floats(&mut layout, &document, &shaper, &cfg);
    let list = compose_page(&layout.pages[0]);

    assert!(
        !list
            .items
            .iter()
            .any(|item| matches!(item, PaintItem::Image { .. })),
        "a tiled fill must not paint an image at all: {:?}",
        list.items
    );
    assert!(
        !list
            .items
            .iter()
            .any(|item| matches!(item, PaintItem::PushClipPath { .. })),
        "and must not clip, since there is nothing to clip"
    );
    // It falls through to the ordinary geometry, so the shape still draws as the
    // triangle it is — an unpainted fill must not erase the object.
    assert!(
        list.items.iter().any(|item| matches!(
            item,
            PaintItem::Shape {
                geometry: casual_doc_layout::display::ShapeGeometry::Path { .. },
                ..
            }
        )),
        "the shape itself still paints: {:?}",
        list.items
    );
}

/// The stepped contour FID-L-12's guards wrap to, in Word's 21,600 space over a
/// `WRAP_FLOAT_WIDTH` × `WRAP_FLOAT_HEIGHT` (1,500 × 1,800 twip) float: nothing
/// for the first 100 twips, the full width down to 600, then only the leading
/// third (500 twips) to the bottom. The empty top is what makes the FIRST line
/// of a paragraph depend on its own height: the band starts inside it.
fn stepped_contour() -> Vec<PointEmu> {
    [
        (0, 1_200),
        (21_600, 1_200),
        (21_600, 7_200),
        (7_200, 7_200),
        (7_200, 21_600),
        (0, 21_600),
    ]
    .into_iter()
    .map(|(x_emu, y_emu)| PointEmu { x_emu, y_emu })
    .collect()
}

/// [`wrap_float`] wrapping as `wrap`, carrying [`stepped_contour`].
fn contour_float(
    id: u64,
    media: MediaId,
    relative_from: HorizontalAnchor,
    position: HorizontalPosition,
    wrap: WrapMode,
) -> InlineNode {
    let InlineNode::AnchoredDrawing(mut drawing) =
        wrap_float(id, media, relative_from, position, at_paragraph_top())
    else {
        unreachable!("wrap_float builds an anchored drawing");
    };
    drawing.anchor.wrap = wrap;
    drawing.anchor.wrap_polygon = Some(stepped_contour());
    InlineNode::AnchoredDrawing(drawing)
}

/// The leading inset a line spanning `top..top + height` must have beside
/// [`stepped_contour`] whose float's top is at `float_top`, flush with the
/// measure's leading edge: the widest band the line overlaps.
fn contour_inset(float_top: i32, top: i32, height: i32) -> i32 {
    let overlaps = |from: i32, to: i32| top < float_top + to && top + height > float_top + from;
    if overlaps(100, 600) {
        1_500
    } else if overlaps(600, 1_800) {
        500
    } else {
        0
    }
}

/// Every text line of `blocks`: its top (from `origin`, stacking the blocks),
/// its height and its leading edge.
fn stacked_lines(blocks: &[BlockFragment], origin: i32) -> Vec<(i32, i32, i32)> {
    let mut out = Vec::new();
    let mut paragraph_top = origin;
    for block in blocks {
        let BlockFragment::Paragraph { lines, .. } = block else {
            panic!("expected a paragraph fragment");
        };
        let mut y = paragraph_top;
        for line in &lines.lines {
            if let Some(run) = line.runs.first() {
                out.push((y, line.height.raw(), run.origin.x.raw()));
            }
            y += line.height.raw();
        }
        paragraph_top += block.height().raw();
    }
    out
}

/// **Tight wrap follows the authored contour, band by band** (`docs/109`
/// FID-L-12). Through the paragraph-local exclusion (the anchoring paragraph)
/// and the carried one (the next paragraph of the same cell): a line beside the
/// contour's wide top is pushed past the whole float, a line beside its narrow
/// arm only past the arm, and the first line — which the contour's first band
/// starts INSIDE — is narrowed although its top is above that band.
///
/// The square-wrap control shows the same float, contour and all, excluding its
/// whole box, so the difference is the wrap mode and nothing else.
#[test]
fn a_tight_wrap_follows_its_contour_band_by_band_in_its_own_cell() {
    let (media, definitions) = media_defs();
    let laid_out = |wrap: WrapMode| {
        let float = contour_float(
            900,
            media,
            HorizontalAnchor::Column,
            HorizontalPosition::Offset(0),
            wrap,
        );
        let cell = wrap_cell(
            wrap_cell_table(
                vec![float, run(901, "A short anchor line.")],
                vec![wrap_filler(902)],
            ),
            definitions.clone(),
        );
        stacked_lines(&cell.blocks, 0)
    };

    let tight = laid_out(WrapMode::Tight);
    let mut beside_arm = 0;
    for &(top, height, x) in &tight {
        let expected = contour_inset(0, top, height);
        assert!(
            (x - expected).abs() <= 1,
            "the line at {top}..{} starts at {x}; the contour asks for {expected}",
            top + height
        );
        if expected == 500 {
            beside_arm += 1;
        }
    }
    assert!(
        beside_arm >= 2,
        "several lines must sit beside the contour's narrow arm: {tight:?}"
    );

    for (top, height, x) in laid_out(WrapMode::Square) {
        let expected = if top < WRAP_FLOAT_HEIGHT.raw() {
            1_500
        } else {
            0
        };
        assert!(
            (x - expected).abs() <= 1,
            "square wrap keeps the box: the line at {top}..{} starts at {x}, not {expected}",
            top + height
        );
    }
}

/// The page-level pass follows the contour too: a `Page`-relative float (which
/// the paragraph-local slice does not handle) narrows its own paragraph and the
/// following one band by band.
#[test]
fn a_page_relative_tight_wrap_follows_its_contour_in_the_following_paragraph() {
    use casual_doc_layout::document_layout::paginate_document;

    let (media, definitions) = media_defs();
    // At the left margin, so the float is flush with the body measure.
    let float = contour_float(
        910,
        media,
        HorizontalAnchor::Page,
        HorizontalPosition::Offset(1_440 * 635),
        WrapMode::Tight,
    );
    let document = Document::new(
        node(911),
        vec![
            BlockNode::Paragraph(Paragraph {
                id: node(912),
                properties: ParagraphProperties::default().into(),
                inlines: vec![float, run(913, "Anchor.")],
            }),
            BlockNode::Paragraph(Paragraph {
                id: node(914),
                properties: ParagraphProperties::default().into(),
                inlines: vec![wrap_filler(915)],
            }),
        ],
        definitions,
    )
    .unwrap();
    let layout = paginate_document(&document, &ParleyShaper::new());
    let page = &layout.pages[0];
    let float_top = page
        .anchored
        .first()
        .expect("the float is placed")
        .rect
        .origin
        .y
        .raw();
    let mut checked = 0;
    let mut beside_arm = 0;
    for placed in &page.placed {
        for (top, height, x) in stacked_lines(
            std::slice::from_ref(&placed.fragment),
            placed.rect.origin.y.raw(),
        ) {
            let expected = contour_inset(float_top, top, height);
            assert!(
                (x - expected).abs() <= 1,
                "the line at {top}..{} starts at {x}; the contour asks for {expected}",
                top + height
            );
            checked += 1;
            if expected == 500 {
                beside_arm += 1;
            }
        }
    }
    assert!(
        checked >= 6 && beside_arm >= 2,
        "{checked} lines, {beside_arm} beside the arm"
    );
}
