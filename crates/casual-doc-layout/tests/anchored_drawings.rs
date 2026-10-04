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

/// The vertices a resolved path arrives at, in order.
///
/// The typed presets emit only moves and lines, so for them this is exactly the
/// vertex list the tests asserted before the path primitive replaced it — which is
/// what keeps those assertions comparable across the change.
fn endpoints(commands: &[casual_doc_layout::display::PathCommand]) -> Vec<Point> {
    commands.iter().map(|command| command.endpoint()).collect()
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
    assert!(matches!(
        anchored[1].content,
        AnchorContent::RoundedRectangle {
            radius: Twip(360),
            ..
        }
    ));

    let list = compose_page(&layout.pages[0]);
    assert!(list.items.iter().any(|item| matches!(
        item,
        PaintItem::Shape {
            geometry: DisplayShapeGeometry::Ellipse { .. },
            ..
        }
    )));
    assert!(list.items.iter().any(|item| matches!(
        item,
        PaintItem::Shape {
            geometry: DisplayShapeGeometry::RoundedRect {
                radius: Twip(360),
                ..
            },
            ..
        }
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
            AnchorContent::Path { commands, .. } => Some(endpoints(commands)),
            _ => None,
        })
        .collect();
    assert_eq!(polygons.len(), 3);
    assert_eq!(
        polygons[0],
        vec![
            Point::new(Twip(2_160), Twip(1_440)),
            Point::new(Twip(2_880), Twip(2_880)),
            Point::new(Twip(1_440), Twip(2_880)),
        ]
    );
    assert_eq!(
        polygons[1],
        vec![
            Point::new(Twip(2_880), Twip(1_440)),
            Point::new(Twip(4_320), Twip(2_880)),
            Point::new(Twip(2_880), Twip(2_880)),
        ]
    );
    assert_eq!(
        polygons[2],
        vec![
            Point::new(Twip(5_040), Twip(1_440)),
            Point::new(Twip(5_760), Twip(2_160)),
            Point::new(Twip(5_040), Twip(2_880)),
            Point::new(Twip(4_320), Twip(2_160)),
        ]
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
    use casual_doc_model::v1::{
        DashStyle, Definitions, FillStyle, FormatScheme, LineStyle, Rgba, ShapeStyleRef, StyleColor,
    };

    let green = Rgba {
        r: 0,
        g: 255,
        b: 0,
        a: 255,
    };
    let red = Rgba {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };
    let shape_id = node(91);

    let place = |explicit_fill: Option<casual_doc_model::v1::Fill>| {
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
                fill_styles: vec![Some(FillStyle {
                    color: StyleColor::Placeholder,
                })],
                line_styles: vec![Some(LineStyle {
                    width_emu: 6_350,
                    color: StyleColor::Placeholder,
                    dash: Some(DashStyle::Dash),
                })],
            }),
            ..Definitions::default()
        };
        definitions.shape_styles.insert(
            shape_id,
            ShapeStyleRef {
                fill_idx: Some(1),
                fill_color: Some(green),
                line_idx: Some(1),
                line_color: Some(red),
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
    };

    let AnchorContent::Rectangle { fill, stroke } = place(None) else {
        panic!("expected a rectangle");
    };
    assert_eq!(
        fill,
        Some(casual_doc_model::v1::Fill::Solid(green)),
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
    let AnchorContent::Rectangle { fill, .. } = place(Some(explicit.clone())) else {
        panic!("expected a rectangle");
    };
    assert_eq!(
        fill,
        Some(explicit),
        "an explicit spPr fill must not be overridden by the style reference"
    );
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
        match content {
            AnchorContent::RoundedRectangle { radius, .. } => radius,
            other => panic!("expected a rounded rectangle, got {other:?}"),
        }
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
    // One referencing the box resolves too: `ss` is 1440, so ss/4 = 360 and the
    // radius is 1440 * 360 / 100000 = 5.
    assert_eq!(
        radius_with(adj("*/ ss 1 4")),
        Twip(5),
        "a box-relative guide"
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
    use casual_doc_model::v1::{ShapePath, ShapePathCommand};

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
        path: Some(ShapePath {
            width_emu: 100,
            height_emu: 100,
            commands: vec![
                ShapePathCommand::MoveTo {
                    point: PointEmu { x_emu: 0, y_emu: 0 },
                },
                ShapePathCommand::CubicBezTo {
                    control1: PointEmu {
                        x_emu: 30,
                        y_emu: 80,
                    },
                    control2: PointEmu {
                        x_emu: 70,
                        y_emu: 80,
                    },
                    point: PointEmu {
                        x_emu: 100,
                        y_emu: 0,
                    },
                },
            ],
        }),
        fill: None,
        stroke: None,
        flip_h: false,
        flip_v: false,
        rotation: None,
    });

    let content = only_anchor_content(&single_child_group_document(child));
    let AnchorContent::Path {
        commands, closed, ..
    } = content
    else {
        panic!("expected a path, got {content:?}");
    };
    assert!(!closed, "no a:close was authored");
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
    use casual_doc_model::v1::{ShapePath, ShapePathCommand};

    let child_extent = Extent {
        width_emu: 914_400,
        height_emu: 914_400,
    };
    let group_extent = Extent {
        width_emu: 2 * 914_400,
        height_emu: 914_400,
    };
    let move_to = |x_emu, y_emu| ShapePathCommand::MoveTo {
        point: PointEmu { x_emu, y_emu },
    };
    let line_to = |x_emu, y_emu| ShapePathCommand::LineTo {
        point: PointEmu { x_emu, y_emu },
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
            path: Some(path),
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
                    commands: vec![move_to(0, 0), line_to(1000, 0)],
                },
            ),
            // A closed triangle with both axes scaled.
            shape(
                92,
                914_400,
                ShapePath {
                    width_emu: 100,
                    height_emu: 100,
                    commands: vec![
                        move_to(50, 0),
                        line_to(100, 100),
                        line_to(0, 100),
                        ShapePathCommand::Close,
                    ],
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
            AnchorContent::Path {
                commands, closed, ..
            } => Some((endpoints(commands), *closed)),
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

    // …and the `closed` flag survives into the display list, which is the half
    // the backends actually read.
    let list = compose_page(&layout.pages[0]);
    let closed: Vec<bool> = list
        .items
        .iter()
        .filter_map(|item| match item {
            PaintItem::Shape {
                geometry: DisplayShapeGeometry::Path { closed, .. },
                ..
            } => Some(*closed),
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
        RoundedRectangle,
        Ellipse,
        Line,
    }

    fn expected(geometry: ShapeGeometry) -> Expected {
        match geometry {
            ShapeGeometry::Rectangle | ShapeGeometry::Other => Expected::Rectangle,
            ShapeGeometry::RoundRectangle => Expected::RoundedRectangle,
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
            (
                Expected::Polygon(count),
                AnchorContent::Path {
                    commands, closed, ..
                },
            ) => {
                let points = endpoints(commands);
                assert_eq!(points.len(), count, "{geometry:?} vertex count");
                assert!(*closed, "{geometry:?} is a closed outline");
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
            (Expected::Rectangle, AnchorContent::Rectangle { .. })
            | (Expected::RoundedRectangle, AnchorContent::RoundedRectangle { .. })
            | (Expected::Ellipse, AnchorContent::Ellipse { .. })
            | (Expected::Line, AnchorContent::Line { .. }) => {}
            (want, other) => {
                panic!("{geometry:?} wanted {want:?}, resolved to {other:?}");
            }
        }
    }
}

/// Three of the new presets, to the twip, so the outlines are arithmetic rather
/// than "some polygon arrived". A `homePlate` whose point was on the wrong side,
/// or a `plus` with the arms inverted, would satisfy a vertex count.
#[test]
fn new_presets_resolve_to_their_documented_outlines() {
    let outline = |geometry, adjustments| {
        let document = single_child_group_document(preset_shape_child(geometry, adjustments));
        match only_anchor_content(&document) {
            AnchorContent::Path { commands, .. } => endpoints(&commands),
            other => panic!("expected a path, got {other:?}"),
        }
    };
    let at = |x, y| Point::new(Twip(x), Twip(y));

    // `homePlate` at its preset default `adj` = 50000: the point is half the
    // shorter side (720tw) deep, on the RIGHT.
    assert_eq!(
        outline(ShapeGeometry::HomePlate, Vec::new()),
        vec![
            at(1_440, 1_440),
            at(2_160, 1_440),
            at(2_880, 2_160),
            at(2_160, 2_880),
            at(1_440, 2_880),
        ]
    );

    // `plus` at its preset default `adj` = 25000: 360tw arms.
    assert_eq!(
        outline(ShapeGeometry::Plus, Vec::new()),
        vec![
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
        ]
    );

    // The regular `pentagon`, apex up, filling the box.
    assert_eq!(
        outline(ShapeGeometry::Pentagon, Vec::new()),
        vec![
            at(2_160, 1_440),
            at(2_880, 1_990),
            at(2_605, 2_880),
            at(1_715, 2_880),
            at(1_440, 1_990),
        ]
    );

    // An authored `a:avLst` really moves the outline: the default `hexagon`
    // insets its corners by 360tw (25000 of the 1440tw shorter side), an
    // authored 40000 by 576tw.
    let default_hexagon = outline(ShapeGeometry::Hexagon, Vec::new());
    assert_eq!(default_hexagon[1], at(1_800, 1_440));
    let authored_hexagon = outline(
        ShapeGeometry::Hexagon,
        vec![ShapeAdjustment {
            name: "adj".to_owned(),
            formula: "val 40000".to_owned(),
        }],
    );
    assert_eq!(authored_hexagon[1], at(2_016, 1_440));
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
