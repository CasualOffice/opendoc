// Copyright The opendoc Authors
// SPDX-License-Identifier: Apache-2.0

//! Pie and doughnut reach actual pixels — tier 1B (`docs/155` §7.3, `docs/105`
//! FID-R-08).
//!
//! # Why these guards count pixels rather than path commands
//!
//! `casual-doc-layout/src/arc.rs` already proves the *geometry*: that the cubic
//! approximation stays on the circle, that three equal sweeps meet without a gap,
//! that a hole is left empty. Asserting the same thing again over a command list
//! would be asserting the mechanism twice and the guarantee never (`SKILL` §10).
//!
//! What is NOT proven by any of that is the thing the lane keeps getting wrong:
//! that the figure **arrives on the page**. A chart was typed, round-tripped, loss
//! reported and retention-guarded while a user still saw the literal text
//! `[chart]` (`SKILL` §9 rule 4). So each guard here rasterises a page and
//! measures painted area by colour:
//!
//! - three equal values must paint three sectors of **equal area** that **fill the
//!   disc**, which is simultaneously "they close the circle" and "they do not
//!   overlap" — an overlap would make one colour's area short, a gap would leave
//!   background inside the disc;
//! - a doughnut's hole must still be background after painting;
//! - a zero-valued category must contribute **no** coloured area.
//!
//! None of those can be satisfied by a path that is built but not composed, not
//! painted, or painted outside the chart box.

use casual_doc_layout::compose::compose_page;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    Axis, BlockNode, Chart, ChartCoverage, ChartGroup, ChartGroupKind, ChartId, ChartValue,
    DataRange, Definitions, DisplayBlanks, Document, EmbeddedKind, EmbeddedObject, EmbeddedPart,
    Extent, InlineNode, PlotArea, Series,
};
use casual_doc_render::{BundledFontSource, MapMediaSource, Surface, render};

const DPI: f32 = 96.0;

/// The theme accents `ChartStyle::default` assigns to pie points, in order. A pie
/// colours by point, so these are the three colours three categories take.
const ACCENTS: [[u8; 3]; 3] = [[0x44, 0x72, 0xC4], [0xED, 0x7D, 0x31], [0xA5, 0xA5, 0xA5]];

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).unwrap()
}

/// A chart projection over `values`, in the requested family.
fn chart(kind: ChartGroupKind, values: &[&str]) -> Chart {
    Chart {
        object: node(901),
        coverage: ChartCoverage::Partial,
        title: None,
        auto_title_deleted: true,
        plot_area: PlotArea {
            groups: vec![ChartGroup {
                kind,
                series: vec![Series {
                    index: 0,
                    order: 0,
                    values: DataRange {
                        formula: None,
                        point_count: u32::try_from(values.len()).unwrap(),
                        points: values
                            .iter()
                            .enumerate()
                            .map(|(index, value)| {
                                (
                                    u32::try_from(index).unwrap(),
                                    ChartValue::Number((*value).to_owned()),
                                )
                            })
                            .collect(),
                        number_format: None,
                    },
                    ..Series::default()
                }],
                // A pie has no axes, which is why nothing here declares one and
                // why the axis furniture is absent from the painted page.
                axis_ids: Vec::new(),
                vary_colors: false,
            }],
            axes: Vec::<Axis>::new(),
        },
        legend: None,
        // Every remaining field is named rather than defaulted: a field added to
        // `Chart` must make this fixture state its intent, not inherit one
        // (`SKILL` §5a - defaulting is how a new field becomes a silent drop).
        plot_visible_only: true,
        display_blanks_as: DisplayBlanks::Gap,
        vary_colors: false,
        external_data: None,
    }
}

/// A one-paragraph document whose only content is the chart.
fn document(kind: ChartGroupKind, values: &[&str]) -> Document {
    let object = node(901);
    let mut definitions = Definitions::default();
    let mut projection = chart(kind, values);
    projection.object = object;
    definitions
        .charts
        .insert(ChartId::new(node(902)), projection);
    Document::new(
        node(1_000),
        vec![BlockNode::Paragraph(casual_doc_model::v1::Paragraph {
            id: node(1),
            properties: casual_doc_model::v1::ParagraphProperties::default().into(),
            inlines: vec![InlineNode::EmbeddedObject(Box::new(EmbeddedObject {
                id: object,
                kind: EmbeddedKind::Chart,
                part: EmbeddedPart {
                    relationship_id: "rId4".to_owned(),
                    relationship_type:
                        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart"
                            .to_owned(),
                    part_name: "word/charts/chart1.xml".to_owned(),
                },
                extra_parts: Vec::new(),
                preview: None,
                // 4in square, so the pie is a generous circle on US Letter.
                extent: Extent {
                    width_emu: 3_657_600,
                    height_emu: 3_657_600,
                },
                prog_id: None,
            }))],
        })],
        definitions,
    )
    .expect("a chart-bearing document is valid")
}

/// Rasterises the first page: `(width, height, rgba)`.
fn paint(kind: ChartGroupKind, values: &[&str]) -> (u32, u32, Vec<u8>) {
    let doc = document(kind, values);
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
    (width, height, surface.data().to_vec())
}

/// How many pixels are within `tolerance` of `rgb` on every channel.
///
/// A tolerance rather than an exact match because the sectors are anti-aliased;
/// edge pixels are blends and are deliberately not counted as either colour.
fn area_of(pixels: &[u8], rgb: [u8; 3], tolerance: i32) -> usize {
    pixels
        .chunks_exact(4)
        .filter(|pixel| {
            pixel[3] > 0
                && (0..3).all(|channel| {
                    (i32::from(pixel[channel]) - i32::from(rgb[channel])).abs() <= tolerance
                })
        })
        .count()
}

#[test]
fn three_equal_slices_paint_three_equal_sectors_that_fill_the_disc() {
    // The guarantee, measured as area: equal values => equal areas, and the three
    // together ARE the disc. An overlap shortens one colour; a gap leaves white
    // inside the circle. Neither can pass this.
    let (_, _, pixels) = paint(
        ChartGroupKind::Pie {
            first_slice_angle: 0,
        },
        &["1", "1", "1"],
    );
    let areas: Vec<usize> = ACCENTS
        .iter()
        .map(|accent| area_of(&pixels, *accent, 6))
        .collect();
    for (index, area) in areas.iter().enumerate() {
        assert!(
            *area > 2_000,
            "accent {index} must paint a real sector, got {area} px of {areas:?}"
        );
    }
    let total: usize = areas.iter().sum();
    let largest = *areas.iter().max().expect("three areas");
    let smallest = *areas.iter().min().expect("three areas");
    // Thirds of one circle, so within a few percent of each other once
    // anti-aliased edges and the hairline separators are discounted.
    assert!(
        largest - smallest < total / 20,
        "three equal values must paint three equal sectors, got {areas:?}"
    );
    // And the three together fill a disc: a disc of radius r has area pi*r^2,
    // and the chart box is 4in square at 96 dpi, so r is at most 192 px and the
    // plot inset makes it a little less. Anything far below that means a sector
    // is missing or the sweeps did not close.
    assert!(
        total > 60_000,
        "three sectors must cover the disc, got {total} px across {areas:?}"
    );
}

#[test]
fn a_doughnut_leaves_a_hole_of_unpainted_background_at_its_centre() {
    // The pie and the doughnut are the same call with one argument changed, so
    // the discriminating comparison is between them rather than against a
    // constant: the pie's centre is coloured and the doughnut's is not.
    let hole_centre_is_coloured = |kind: ChartGroupKind| {
        let (width, height, pixels) = paint(kind, &["1", "1", "1"]);
        // The chart box is the only thing on the page, so the disc's centre is
        // the centre of the painted sectors. Locate it from the painted extent of
        // the first accent plus the third, rather than by assuming a layout.
        let mut min = (u32::MAX, u32::MAX);
        let mut max = (0u32, 0u32);
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            let index = u32::try_from(index).unwrap();
            let (x, y) = (index % width, index / width);
            if ACCENTS.iter().any(|accent| {
                (0..3).all(|channel| {
                    (i32::from(pixel[channel]) - i32::from(accent[channel])).abs() <= 6
                })
            }) {
                min = (min.0.min(x), min.1.min(y));
                max = (max.0.max(x), max.1.max(y));
            }
        }
        assert!(max.0 > min.0 && max.1 > min.1, "something must be painted");
        assert!(height > 0);
        let centre = ((min.0 + max.0) / 2, (min.1 + max.1) / 2);
        // Sample a small square at the centre and ask whether ANY of it is a
        // sector colour. One pixel could land on a hairline separator.
        let mut coloured = 0usize;
        for dy in -3i32..=3 {
            for dx in -3i32..=3 {
                let x = u32::try_from(i32::try_from(centre.0).unwrap() + dx).unwrap();
                let y = u32::try_from(i32::try_from(centre.1).unwrap() + dy).unwrap();
                let offset = ((y * width + x) * 4) as usize;
                let pixel = &pixels[offset..offset + 4];
                if ACCENTS.iter().any(|accent| {
                    (0..3).all(|channel| {
                        (i32::from(pixel[channel]) - i32::from(accent[channel])).abs() <= 6
                    })
                }) {
                    coloured += 1;
                }
            }
        }
        coloured
    };

    let pie = hole_centre_is_coloured(ChartGroupKind::Pie {
        first_slice_angle: 0,
    });
    let doughnut = hole_centre_is_coloured(ChartGroupKind::Doughnut {
        first_slice_angle: 0,
        hole_size: 50,
    });
    assert!(
        pie > 20,
        "a pie's centre is inside its wedges and must be coloured, got {pie}/49"
    );
    assert_eq!(
        doughnut, 0,
        "a doughnut's hole must be unpainted background, got {doughnut}/49 coloured"
    );
}

#[test]
fn a_zero_valued_category_paints_no_sector_at_all() {
    // A zero slice must be absent, not a hairline sliver on the boundary between
    // its neighbours. Measured by its accent having NO area while its two
    // neighbours do - so the guard cannot pass by nothing being painted.
    let (_, _, pixels) = paint(
        ChartGroupKind::Pie {
            first_slice_angle: 0,
        },
        &["1", "0", "1"],
    );
    let first = area_of(&pixels, ACCENTS[0], 6);
    let zero = area_of(&pixels, ACCENTS[1], 6);
    let third = area_of(&pixels, ACCENTS[2], 6);
    assert!(
        first > 2_000 && third > 2_000,
        "the two non-zero categories must still paint, got {first} and {third}"
    );
    assert_eq!(
        zero, 0,
        "a zero-valued category must paint nothing, got {zero} px of accent 2"
    );
}

#[test]
fn a_pie_is_painted_instead_of_the_chart_placeholder_text() {
    // The §9-rule-4 guard: the thing this lane has got wrong twice is shipping a
    // construct that is modelled and invisible. A painted pie and a `[chart]`
    // label are mutually exclusive outcomes, and the label is text, so its
    // absence is checked where text would be - the display list's glyph runs.
    use casual_doc_layout::display::PaintItem;
    let doc = document(
        ChartGroupKind::Pie {
            first_slice_angle: 0,
        },
        &["1", "2", "3"],
    );
    let pages = paginate_document(&doc, &ParleyShaper::new());
    let list = compose_page(pages.pages.first().expect("a page"));
    let glyphs = list
        .items
        .iter()
        .filter(|item| matches!(item, PaintItem::Glyphs { .. }))
        .count();
    assert_eq!(
        glyphs, 0,
        "a pie with no title, legend or data labels paints no text at all - a \
         glyph run here is the `[chart]` placeholder surviving alongside the chart"
    );
    let curved = list
        .items
        .iter()
        .filter(|item| match item {
            PaintItem::Shape {
                geometry: casual_doc_layout::display::ShapeGeometry::Path { commands, .. },
                ..
            } => commands.iter().any(|command| {
                matches!(
                    command,
                    casual_doc_layout::display::PathCommand::CubicTo { .. }
                )
            }),
            _ => false,
        })
        .count();
    assert_eq!(curved, 3, "three values paint three curved sectors");
}
