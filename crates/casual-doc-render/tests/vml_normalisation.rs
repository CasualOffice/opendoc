//! Legacy VML, end to end: import → layout → the page's paint list
//! (`docs/109` HF-390..HF-393, `docs/08` ADR-072).
//!
//! `fixtures/generated/vml-shapes.docx` carries one of each VML construct the
//! importer normalises into the DrawingML model. Each test asserts what a
//! reader of the page sees — a painted shape of the right kind, in the right
//! colour, with its text on the page — because the defect this closes was a
//! shape that imported, was kept, and drew nothing.
//!
//! Every shape in the fixture has its own fill or outline colour, so a paint
//! item is identified by colour, not by its position in the list.

use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::display::{Color, Fill, PaintItem, PathCommand, ShapeGeometry};
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::units::Rect;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{BlockNode, Document, GroupChild, InlineNode};
use casual_doc_ooxml::{DocxPackage, PackageLimits};

const VML_SHAPES_DOCX: &[u8] = include_bytes!("../../../fixtures/generated/vml-shapes.docx");

/// The fixture, imported and laid out; its first page's paint list.
fn first_page() -> (Document, Vec<PaintItem>) {
    let mut package = DocxPackage::open(VML_SHAPES_DOCX, PackageLimits::default()).unwrap();
    let document = import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .expect("the fixture imports")
    .document;
    let shaper = ParleyShaper::new();
    let pages = paginate_document(&document, &shaper);
    let items = compose_page(&pages.pages[0]).items;
    (document, items)
}

const fn rgb(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    (r, g, b)
}

fn channels(color: &Color) -> (u8, u8, u8) {
    (color.r, color.g, color.b)
}

/// The `PaintItem::Shape` whose solid fill (or first gradient stop) is `fill`.
fn shape_filled(items: &[PaintItem], fill: (u8, u8, u8)) -> &PaintItem {
    items
        .iter()
        .find(|item| match item {
            PaintItem::Shape {
                fill: Some(Fill::Solid(color)),
                ..
            } => channels(color) == fill,
            PaintItem::Shape {
                fill: Some(Fill::Gradient(gradient)),
                ..
            } => gradient
                .stops
                .first()
                .is_some_and(|stop| channels(&stop.color) == fill),
            _ => false,
        })
        .unwrap_or_else(|| panic!("no shape filled {fill:?} is painted on the page"))
}

/// The `PaintItem::Shape` outlined in `stroke`.
fn shape_stroked(items: &[PaintItem], stroke: (u8, u8, u8)) -> &PaintItem {
    items
        .iter()
        .find(|item| {
            matches!(item, PaintItem::Shape { stroke: Some(outline), .. }
                if channels(&outline.color) == stroke)
        })
        .unwrap_or_else(|| panic!("no shape outlined {stroke:?} is painted on the page"))
}

/// The id of the PARAGRAPH holding a run whose text is `text`, anywhere in the
/// body, the headers or the text boxes (top-level and inside groups) — the
/// anchor a painted glyph run carries (`GlyphRun::node`).
fn paragraph_of(document: &Document, text: &str) -> NodeId {
    fn in_blocks(blocks: &[BlockNode], text: &str) -> Option<NodeId> {
        blocks.iter().find_map(|block| match block {
            BlockNode::Paragraph(paragraph) => {
                if paragraph
                    .inlines
                    .iter()
                    .any(|inline| matches!(inline, InlineNode::Run(run) if run.text == text))
                {
                    Some(paragraph.id)
                } else {
                    in_inlines(&paragraph.inlines, text)
                }
            }
            _ => None,
        })
    }
    fn in_children(children: &[GroupChild], text: &str) -> Option<NodeId> {
        children.iter().find_map(|child| match child {
            GroupChild::TextBox(text_box) => in_blocks(&text_box.blocks, text),
            GroupChild::Group(group) => in_children(&group.children, text),
            _ => None,
        })
    }
    fn in_inlines(inlines: &[InlineNode], text: &str) -> Option<NodeId> {
        inlines.iter().find_map(|inline| match inline {
            InlineNode::TextBox(text_box) => in_blocks(&text_box.blocks, text),
            InlineNode::Group(group) => in_children(&group.children, text),
            _ => None,
        })
    }
    in_blocks(document.body(), text)
        .or_else(|| {
            document
                .definitions()
                .headers
                .iter()
                .find_map(|(_, header)| in_blocks(&header.blocks, text))
        })
        .unwrap_or_else(|| panic!("no run reads {text:?} in the model"))
}

/// Where the glyphs of paragraph `id` are painted, or `None` when they are not.
fn painted_at(items: &[PaintItem], id: NodeId) -> Option<casual_doc_layout::units::Point> {
    items.iter().find_map(|item| match item {
        PaintItem::Glyphs { run } if run.node == Some(id) => Some(run.origin),
        _ => None,
    })
}

fn contains(rect: &Rect, point: casual_doc_layout::units::Point) -> bool {
    point.x >= rect.origin.x
        && point.x <= rect.origin.x + rect.size.width
        && point.y >= rect.origin.y
        && point.y <= rect.origin.y + rect.size.height
}

/// HF-390: a `v:rect` is a painted rectangle in its fill colour, at its
/// authored size (144pt x 36pt = 2880 x 720 twips), with its outline.
#[test]
fn a_vml_rectangle_is_painted_at_its_size_with_fill_and_outline() {
    let (_, items) = first_page();
    let PaintItem::Shape {
        geometry: ShapeGeometry::Rect { rect },
        stroke,
        ..
    } = shape_filled(&items, rgb(0x44, 0x72, 0xc4))
    else {
        panic!("a v:rect is painted as a rectangle");
    };
    assert_eq!((rect.size.width.0, rect.size.height.0), (2880, 720));
    let outline = stroke.as_ref().expect("the rectangle is outlined");
    assert_eq!(channels(&outline.color), rgb(0x1f, 0x38, 0x64));
}

/// HF-390: a `v:roundrect` is a rounded rectangle; its two-colour `v:fill`
/// is a gradient, not a flattened colour.
#[test]
fn a_vml_rounded_rectangle_is_painted_rounded_with_its_gradient() {
    let (_, items) = first_page();
    let PaintItem::Shape { geometry, fill, .. } = shape_filled(&items, rgb(0xff, 0xc0, 0x00))
    else {
        unreachable!()
    };
    assert!(
        !matches!(geometry, ShapeGeometry::Rect { .. }),
        "a v:roundrect is not a square-cornered rectangle: {geometry:?}"
    );
    assert!(matches!(fill, Some(Fill::Gradient(_))));
}

/// HF-390: a `v:oval` is an ellipse, and its `v:stroke@dashstyle` is a dash.
#[test]
fn a_vml_oval_is_painted_as_an_ellipse_with_its_dash() {
    let (_, items) = first_page();
    let PaintItem::Shape {
        geometry, stroke, ..
    } = shape_filled(&items, rgb(0xc0, 0x00, 0x00))
    else {
        unreachable!()
    };
    assert!(
        matches!(geometry, ShapeGeometry::Ellipse { .. }),
        "{geometry:?}"
    );
    let outline = stroke.as_ref().expect("outlined");
    assert_ne!(
        outline.dash,
        casual_doc_model::v1::DashStyle::Solid,
        "the dashed stroke is drawn dashed"
    );
}

/// HF-391: a `v:line` from bottom-left to top-right is drawn in that direction
/// (DrawingML draws top-left to bottom-right unless flipped), with the
/// arrowhead `endarrow` names at its END.
#[test]
fn a_vml_line_runs_from_its_from_point_to_its_to_point_with_its_arrowhead() {
    let (_, items) = first_page();
    let PaintItem::Shape {
        geometry,
        tail_end,
        transform,
        ..
    } = shape_stroked(&items, rgb(0x00, 0xb0, 0x50))
    else {
        unreachable!()
    };
    assert!(
        matches!(geometry, ShapeGeometry::Line { .. }),
        "{geometry:?}"
    );
    assert!(
        tail_end.is_some(),
        "the end arrow is drawn at the line's end"
    );
    let flip = transform.expect("an up-and-right line is flipped vertically");
    assert!(flip.flip_v && !flip.flip_h, "{flip:?}");
}

/// HF-392: a `v:shape` naming a `v:shapetype` is drawn from the shapetype's
/// path, evaluated through its formulas and adjust value — a triangle, three
/// corners, not the bounding box.
#[test]
fn a_vml_shape_takes_its_path_from_its_shapetype_formulas() {
    let (_, items) = first_page();
    let PaintItem::Shape { geometry, .. } = shape_filled(&items, rgb(0x70, 0x30, 0xa0)) else {
        unreachable!()
    };
    let ShapeGeometry::Path { commands, .. } = geometry else {
        panic!("a path, not a box: {geometry:?}");
    };
    let corners = commands
        .iter()
        .filter(|command| {
            matches!(
                command,
                PathCommand::MoveTo { .. } | PathCommand::LineTo { .. }
            )
        })
        .count();
    assert_eq!(corners, 3, "{commands:?}");
}

/// HF-392: a `v:shape` with its own `path` is drawn from it (a seven-corner
/// arrow).
#[test]
fn a_vml_shape_is_drawn_from_its_own_path() {
    let (_, items) = first_page();
    let PaintItem::Shape { geometry, .. } = shape_filled(&items, rgb(0xed, 0x7d, 0x31)) else {
        unreachable!()
    };
    let ShapeGeometry::Path { commands, .. } = geometry else {
        panic!("a path, not a box: {geometry:?}");
    };
    let corners = commands
        .iter()
        .filter(|command| {
            matches!(
                command,
                PathCommand::MoveTo { .. } | PathCommand::LineTo { .. }
            )
        })
        .count();
    assert_eq!(corners, 7, "{commands:?}");
}

/// HF-393: a `v:group` is ONE object whose children keep their places in its
/// coordinate space: the oval sits inside the frame, and the group's text box
/// text is painted inside the frame too, through the shared text pipeline.
#[test]
fn a_vml_group_places_its_children_and_text_inside_its_frame() {
    let (document, items) = first_page();
    let PaintItem::Shape {
        geometry: ShapeGeometry::Rect { rect: frame },
        fill: None,
        ..
    } = shape_stroked(&items, rgb(0x2f, 0x55, 0x97))
    else {
        panic!("the group's frame is an unfilled rectangle");
    };
    // 288pt x 72pt.
    assert_eq!((frame.size.width.0, frame.size.height.0), (5760, 1440));
    let PaintItem::Shape {
        geometry: ShapeGeometry::Ellipse { rect: oval },
        ..
    } = shape_filled(&items, rgb(0x70, 0xad, 0x47))
    else {
        panic!("the group's oval is an ellipse");
    };
    assert!(
        contains(frame, oval.origin),
        "oval {oval:?} outside frame {frame:?}"
    );
    // The oval starts 100 coordinate units in from the group's origin on both
    // axes: 100/4000 of 5760 twips across, 100/1000 of 1440 twips down.
    assert_eq!(
        (
            oval.origin.x.0 - frame.origin.x.0,
            oval.origin.y.0 - frame.origin.y.0
        ),
        (144, 144),
        "the oval keeps its place in the group's coordinate space"
    );
    // The oval is 800 of the group's 4000 coordinate units wide: a fifth of
    // the frame.
    assert_eq!(oval.size.width.0, frame.size.width.0 / 5);
    let text = paragraph_of(&document, "Text inside a VML group");
    let origin = painted_at(&items, text).expect("the group's text is painted");
    assert!(
        contains(frame, origin),
        "text at {origin:?} outside frame {frame:?}"
    );
    // And at ITS place in the group, not wherever an inline box would land:
    // the text box starts 1100 of 4000 units in, 1584 twips across the frame.
    assert!(
        (origin.x.0 - frame.origin.x.0 - 1584).abs() <= 20,
        "text at {origin:?}, frame at {:?}",
        frame.origin
    );
    // One object: the group is a single model node holding the frame, the
    // oval and the text box.
    let groups: Vec<_> = document
        .body()
        .iter()
        .filter_map(|block| match block {
            BlockNode::Paragraph(paragraph) => Some(paragraph.inlines.iter()),
            _ => None,
        })
        .flatten()
        .filter_map(|inline| match inline {
            InlineNode::Group(group)
                if group
                    .children
                    .iter()
                    .any(|child| matches!(child, GroupChild::TextBox(_))) =>
            {
                Some(group.children.len())
            }
            _ => None,
        })
        .collect();
    assert_eq!(groups, vec![3], "one group of three children");
}

/// HF-393: a stand-alone `v:textbox` flows its content and paints it, and the
/// box keeps its fill.
#[test]
fn a_vml_text_box_paints_its_text_and_its_fill() {
    let (document, items) = first_page();
    let text = paragraph_of(
        &document,
        "A VML text box flows through the shared pipeline",
    );
    assert!(
        painted_at(&items, text).is_some(),
        "the text box's text is painted"
    );
    assert!(items.iter().any(|item| matches!(item,
        PaintItem::Rect { fill: Some(color), .. } if channels(color) == rgb(0xde, 0xeb, 0xf7))
        || matches!(item, PaintItem::Shape { fill: Some(Fill::Solid(color)), .. }
            if channels(color) == rgb(0xde, 0xeb, 0xf7))));
}

/// HF-391: a picture fill and a `v:imagedata` picture both paint the image.
#[test]
fn a_picture_fill_and_a_vml_picture_both_paint_the_image() {
    let (_, items) = first_page();
    let images = items
        .iter()
        .filter(|item| matches!(item, PaintItem::Image { .. }))
        .count();
    assert_eq!(images, 2, "the picture fill and the picture");
}

/// HF-393: where `mc:AlternateContent` carries DrawingML and a VML fallback,
/// the DrawingML is drawn and the fallback is not — never both.
#[test]
fn alternate_content_draws_the_drawingml_choice_and_never_the_vml_fallback() {
    let (document, items) = first_page();
    let choice = paragraph_of(&document, "DrawingML choice");
    assert!(painted_at(&items, choice).is_some());
    let fallback_in_model = std::panic::catch_unwind(|| paragraph_of(&document, "VML fallback"));
    assert!(
        fallback_in_model.is_err(),
        "the VML fallback is not imported at all"
    );
}

/// HF-393: a header's VML text box is drawn on the page, at its absolute
/// page position (20pt from the top edge).
#[test]
fn a_header_vml_text_box_is_painted_at_its_page_position() {
    let (document, items) = first_page();
    let text = paragraph_of(&document, "Header text in a VML text box");
    let origin = painted_at(&items, text).expect("the header text box's text is painted");
    // The box's top is 400 twips down the page; the baseline sits inside it.
    assert!((400..800).contains(&origin.y.0), "{origin:?}");
}
