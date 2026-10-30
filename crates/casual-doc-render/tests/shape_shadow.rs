// Copyright The opendoc Authors
// SPDX-License-Identifier: Apache-2.0

//! A shape's outer shadow reaches the pixels (`docs/153` `object.shape-shadow`).
//!
//! Model → layout → display list → tiny-skia, read back as pixels: the page
//! beside a shape, where only its shadow can land, is darkened when the shape
//! has a shadow and white when it has none, and a blurred shadow bleeds past
//! the hard silhouette's edge. Counting display items would prove the shadow
//! was emitted; only a pixel proves it was painted.

use casual_doc_layout::compose::compose_page;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    AnchorHorizontal, AnchorVertical, BlockNode, Definitions, Document, DrawingAnchor, Extent,
    Fill, GroupChild, GroupShape, GroupTransform, HorizontalAnchor, HorizontalPosition, InlineNode,
    Paragraph, ParagraphProperties, PointEmu, Rgba, ShapeGeometry, ShapeShadow, VerticalAnchor,
    VerticalPosition, WordprocessingGroup, WrapMode,
};
use casual_doc_render::{BundledFontSource, MapMediaSource, Surface, render};

const DPI: f32 = 96.0;
/// One inch, in EMU: the shape's box, placed one inch from the page corner, so
/// it covers device pixels 96..192 on both axes.
const INCH: i64 = 914_400;

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).unwrap()
}

/// Half-opaque black, a quarter inch down and right (45°), blurred by
/// `blur_emu`.
fn shadow(blur_emu: i64) -> ShapeShadow {
    ShapeShadow {
        color: Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 128,
        },
        blur_rad_emu: blur_emu,
        dist_emu: INCH / 4,
        dir: 2_700_000,
        alignment: None,
        rot_with_shape: Some(false),
        sx: None,
        sy: None,
        kx: None,
        ky: None,
    }
}

fn document(shadow: Option<ShapeShadow>) -> Document {
    let extent = Extent {
        width_emu: INCH,
        height_emu: INCH,
    };
    let shape = GroupChild::Shape(GroupShape {
        hyperlink: None,
        id: node(91),
        offset: PointEmu { x_emu: 0, y_emu: 0 },
        extent,
        geometry: ShapeGeometry::Rectangle,
        preset: None,
        adjustments: Vec::new(),
        path: None,
        fill: Some(Fill::Solid(Rgba {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        })),
        stroke: None,
        shadow,
        flip_h: false,
        flip_v: false,
        rotation: None,
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

/// The page's RGBA at each device pixel after painting `document(shadow)`.
fn painted(shadow: Option<ShapeShadow>, points: &[(u32, u32)]) -> Vec<[u8; 4]> {
    let doc = document(shadow);
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

/// The offset is 0.25 in at 45°, about 17 px on each axis, so the shadow
/// covers 113..209: (200, 200) is past the shape's corner and inside the
/// shadow; (211, 150) is just past the hard shadow's right edge.
const IN_SHADOW: (u32, u32) = (200, 200);
const PAST_EDGE: (u32, u32) = (212, 150);
const INSIDE_SHAPE: (u32, u32) = (150, 150);

fn is_white(pixel: [u8; 4]) -> bool {
    pixel[0] > 250 && pixel[1] > 250 && pixel[2] > 250
}

fn grey_level(pixel: [u8; 4]) -> u8 {
    pixel[0]
}

#[test]
fn a_shadow_darkens_the_page_beside_its_shape_and_the_shape_stays_on_top() {
    let pixels = painted(Some(shadow(0)), &[IN_SHADOW, INSIDE_SHAPE, PAST_EDGE]);
    let level = grey_level(pixels[0]);
    assert!(
        (110..=145).contains(&level) && pixels[0][0] == pixels[0][2],
        "half-opaque black over white is mid grey, got {:?}",
        pixels[0]
    );
    assert!(
        pixels[1][0] > 200 && pixels[1][1] < 60,
        "the shape paints over its own shadow: {:?}",
        pixels[1]
    );
    assert!(
        is_white(pixels[2]),
        "a hard shadow stops at its edge: {:?}",
        pixels[2]
    );
}

#[test]
fn no_shadow_leaves_the_page_beside_the_shape_white() {
    let pixels = painted(None, &[IN_SHADOW]);
    assert!(is_white(pixels[0]), "{:?}", pixels[0]);
}

#[test]
fn a_blurred_shadow_bleeds_past_the_hard_edge_and_softens_inside_it() {
    // Word's 4 pt preset blur.
    let pixels = painted(Some(shadow(50_800)), &[IN_SHADOW, PAST_EDGE]);
    assert!(
        !is_white(pixels[1]),
        "the blur reaches past the silhouette: {:?}",
        pixels[1]
    );
    assert!(
        grey_level(pixels[1]) > grey_level(pixels[0]),
        "and is lighter there than deep inside the shadow: {:?} vs {:?}",
        pixels[1],
        pixels[0]
    );
}
