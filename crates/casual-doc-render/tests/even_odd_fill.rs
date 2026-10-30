// Copyright The opendoc Authors
// SPDX-License-Identifier: Apache-2.0

//! A geometry's fill rule reaches the pixels (`docs/182` gap G-2).
//!
//! A PDF page fills a ring by drawing two same-wound squares with `f*`: under
//! even-odd the inner square is a hole, under non-zero it is not. The model now
//! carries the rule on `CustomGeometry`, and these guards rasterise a whole page
//! — model → layout → display list → tiny-skia — and read the pixel at the centre
//! of the ring. Counting commands would prove the rule was copied; only a pixel
//! proves it was painted.

use casual_doc_layout::compose::compose_page;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    AnchorHorizontal, AnchorVertical, BlockNode, CustomGeometry, Definitions, Document,
    DrawingAnchor, Extent, Fill, FillRule, GeometryPoint, GroupChild, GroupShape, GroupTransform,
    HorizontalAnchor, HorizontalPosition, InlineNode, Paragraph, ParagraphProperties, PointEmu,
    Rgba, ShapeGeometry, ShapePath, ShapePathCommand, VerticalAnchor, VerticalPosition,
    WordprocessingGroup, WrapMode,
};
use casual_doc_render::{BundledFontSource, MapMediaSource, Surface, render};

const DPI: f32 = 96.0;
/// One inch, in EMU: the shape's box, placed one inch from the page corner.
const INCH: i64 = 914_400;

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).unwrap()
}

/// A square in the path's 100×100 space, wound clockwise on the page.
fn square(x: i64, y: i64, size: i64) -> Vec<ShapePathCommand> {
    let mut commands = vec![ShapePathCommand::MoveTo {
        point: GeometryPoint::literal(x, y),
    }];
    for (cx, cy) in [(x + size, y), (x + size, y + size), (x, y + size)] {
        commands.push(ShapePathCommand::LineTo {
            point: GeometryPoint::literal(cx, cy),
        });
    }
    commands.push(ShapePathCommand::Close);
    commands
}

/// A one-inch shape at (1in, 1in) whose single path is an outer square and a
/// same-wound inner one, filled solid red under `rule`.
fn document(rule: FillRule) -> Document {
    let mut commands = square(0, 0, 100);
    commands.extend(square(25, 25, 50));
    let path = ShapePath {
        width_emu: 100,
        height_emu: 100,
        stroke: false,
        ..ShapePath::new(commands)
    };
    let extent = Extent {
        width_emu: INCH,
        height_emu: INCH,
    };
    let shape = GroupChild::Shape(GroupShape {
        hyperlink: None,
        id: node(91),
        offset: PointEmu { x_emu: 0, y_emu: 0 },
        extent,
        geometry: ShapeGeometry::Other,
        preset: None,
        adjustments: Vec::new(),
        path: Some(CustomGeometry::single_path(path).with_fill_rule(rule)),
        fill: Some(Fill::Solid(Rgba {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        })),
        stroke: None,
        flip_h: false,
        flip_v: false,
        rotation: None,
        shadow: None,
    });
    let group = InlineNode::Group(Box::new(WordprocessingGroup {
        hyperlink: None,
        id: node(90),
        anchor: Some(DrawingAnchor {
            horizontal: AnchorHorizontal {
                relative_from: HorizontalAnchor::Page,
                position: HorizontalPosition::Offset(INCH),
            },
            vertical: AnchorVertical {
                relative_from: VerticalAnchor::Page,
                position: VerticalPosition::Offset(INCH),
            },
            wrap: WrapMode::None,
            wrap_text: None,
            wrap_distances: Default::default(),
            wrap_polygon: None,
            behind_doc: false,
            allow_overlap: true,
        }),
        relative_height: Some(1),
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
        children: vec![shape],
    }));
    Document::new(
        node(1),
        vec![BlockNode::Paragraph(Paragraph {
            id: node(10),
            properties: ParagraphProperties::default().into(),
            inlines: vec![group],
        })],
        Definitions::default(),
    )
    .expect("the probe document is valid")
}

/// The page's RGBA at device pixel `(x, y)` after painting `document(rule)`.
fn painted(rule: FillRule, points: &[(u32, u32)]) -> Vec<[u8; 4]> {
    let doc = document(rule);
    let pages = paginate_document(&doc, &ParleyShaper::new());
    let page = pages.pages.first().expect("the document paginates");
    let width = page.page_size.width.to_device_px(DPI).ceil() as u32;
    let height = page.page_size.height.to_device_px(DPI).ceil() as u32;
    let mut surface = Surface::new(width, height).expect("a page-sized surface");
    render(
        &compose_page(page),
        &mut surface,
        DPI,
        &BundledFontSource,
        &MapMediaSource::default(),
    );
    points
        .iter()
        .map(|&(x, y)| {
            let i = ((y * width + x) * 4) as usize;
            let d = &surface.data()[i..i + 4];
            [d[0], d[1], d[2], d[3]]
        })
        .collect()
}

fn is_red(pixel: [u8; 4]) -> bool {
    pixel[3] > 200 && pixel[0] > 200 && pixel[1] < 60 && pixel[2] < 60
}

/// The box spans device px 96..192 on both axes at 96 dpi: the centre (144,144)
/// is inside the inner square, (100,100) is in the ring.
const CENTRE: (u32, u32) = (144, 144);
const RING: (u32, u32) = (100, 100);

#[test]
fn an_even_odd_path_paints_its_inner_square_as_a_hole() {
    let pixels = painted(FillRule::EvenOdd, &[CENTRE, RING]);
    assert!(is_red(pixels[1]), "the ring is filled: {:?}", pixels[1]);
    assert!(
        !is_red(pixels[0]),
        "under even-odd the same-wound inner square is a hole, got {:?}",
        pixels[0]
    );
}

#[test]
fn a_non_zero_path_fills_its_same_wound_inner_square() {
    let pixels = painted(FillRule::NonZero, &[CENTRE, RING]);
    assert!(is_red(pixels[1]), "the ring is filled: {:?}", pixels[1]);
    assert!(
        is_red(pixels[0]),
        "under non-zero a same-wound inner square adds winding and is filled, got {:?}",
        pixels[0]
    );
}
