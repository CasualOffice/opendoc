//! The watermark, in pixels (`109` OO-006).
//!
//! `casual-doc-layout/tests/watermark.rs` proves the display list: the stamp is
//! resolved, bracketed by its transform, and emitted behind the body. None of that
//! proves a pixel changed. The transform bracket is a NEW renderer primitive —
//! `PushTransform` draws its contents into an offscreen pixmap and composites them
//! back through the matrix — and a bracket the executor silently ignored would
//! leave every one of those layout tests green while the page painted the stamp
//! flat, in the right place, looking deliberate.
//!
//! So this test rasterises real pages and reads the ink back.
//!
//! The load-bearing assertion is the DIAGONAL one: it compares the ink's own
//! orientation against a level stamp of the same words. Counting ink alone would
//! pass against a renderer that drew the glyphs and dropped the rotation, which is
//! exactly the failure mode a new primitive has.

use casual_doc_layout::compose::compose_page;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Definitions, Document, InlineNode, PageMargins, PageSize, Paragraph,
    ParagraphProperties, Rgba, Run, RunProperties, SectionBoundary, SectionColumns, SectionId,
    Watermark, WatermarkContent, WatermarkLayout, WatermarkText,
};
use casual_doc_render::{BundledFontSource, MapMediaSource, Surface, render};

const DPI: f32 = 96.0;

fn node(id: u64) -> NodeId {
    NodeId::from_parts(id, 1).unwrap()
}

fn section(watermark: Option<Watermark>) -> SectionBoundary {
    SectionBoundary {
        id: SectionId::new(node(900)),
        page_size: PageSize {
            width_twips: 12_240,
            height_twips: 15_840,
        },
        page_margins: PageMargins {
            top_twips: 1_440,
            bottom_twips: 1_440,
            start_twips: 1_440,
            end_twips: 1_440,
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
        watermark,
        footnote_props: Default::default(),
        endnote_props: Default::default(),
        text_direction: None,
        bidi: false,
        section_change: None,
    }
}

/// A one-paragraph page. The body text is short and near the top, so the stamp
/// across the page's middle is separable from it.
fn document(watermark: Option<Watermark>) -> Document {
    let mut definitions = Definitions::default();
    definitions.sections.push(section(watermark));
    Document::new(
        node(1000),
        vec![BlockNode::Paragraph(Paragraph {
            id: node(1),
            properties: ParagraphProperties::default().into(),
            inlines: vec![InlineNode::Run(Run {
                id: node(2),
                properties: RunProperties::default().into(),
                text: "Body".to_owned(),
            })],
        })],
        definitions,
    )
    .expect("valid document")
}

/// Word's own watermark look: light grey and semitransparent.
///
/// The multiply test MUST use this rather than the black stamp below. Black over
/// text is black whichever way it is composited, so a black stamp cannot tell
/// multiplication from ordinary source-over — mutation testing found exactly that:
/// disabling the blend entirely left the test green.
fn grey_stamp(layout: WatermarkLayout) -> Watermark {
    Watermark {
        content: WatermarkContent::Text(WatermarkText {
            text: "DRAFT".to_owned(),
            font: None,
            size_half_points: None,
            color: Rgba {
                r: 0xc0,
                g: 0xc0,
                b: 0xc0,
                a: 255,
            },
            bold: false,
            italic: false,
        }),
        layout,
        semi_transparent: true,
    }
}

fn stamp(layout: WatermarkLayout) -> Watermark {
    Watermark {
        content: WatermarkContent::Text(WatermarkText {
            text: "DRAFT".to_owned(),
            font: None,
            size_half_points: None,
            color: Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            bold: false,
            italic: false,
        }),
        layout,
        semi_transparent: false,
    }
}

/// One rasterised page: `(width, height, rgba)`.
fn paint(watermark: Option<Watermark>) -> (u32, u32, Vec<u8>) {
    let doc = document(watermark);
    let pages = paginate_document(&doc, &ParleyShaper::new());
    let page = &pages.pages[0];
    let width = page.page_size.width.to_device_px(DPI).ceil() as u32;
    let height = page.page_size.height.to_device_px(DPI).ceil() as u32;
    let mut surface = Surface::new(width, height).unwrap();
    render(
        &compose_page(page),
        &mut surface,
        DPI,
        &BundledFontSource,
        &MapMediaSource::default(),
    );
    (width, height, surface.data().to_vec())
}

/// The dark pixels below the body text, as `(x, y)` pairs — the region the stamp
/// occupies and an unstamped page leaves blank.
fn stamp_pixels(width: u32, height: u32, rgba: &[u8]) -> Vec<(u32, u32)> {
    let skip = height / 4; // clear of the one short body line at the top
    let mut out = Vec::new();
    for y in skip..height {
        for x in 0..width {
            let p = ((y * width + x) * 4) as usize;
            let (r, g, b) = (rgba[p], rgba[p + 1], rgba[p + 2]);
            if (u32::from(r) + u32::from(g) + u32::from(b)) / 3 < 160 {
                out.push((x, y));
            }
        }
    }
    out
}

#[test]
fn an_unstamped_page_is_blank_below_its_text() {
    let (width, height, rgba) = paint(None);
    assert!(
        stamp_pixels(width, height, &rgba).is_empty(),
        "the baseline for every assertion below: nothing else paints in this region"
    );
}

#[test]
fn a_watermark_paints_ink_the_page_did_not_have() {
    let (width, height, rgba) = paint(Some(stamp(WatermarkLayout::Diagonal)));
    let ink = stamp_pixels(width, height, &rgba);
    assert!(
        ink.len() > 500,
        "a page-sized stamp should darken thousands of pixels, got {}",
        ink.len()
    );
}

/// The one that proves the transform bracket is honoured.
///
/// A level stamp's ink is a horizontal band: its rows are nearly identical in
/// extent, and its height is one line. A 315° stamp climbs across the page, so its
/// ink spans a far greater height and its x-centre MOVES from row to row. The
/// second property is what a dropped rotation cannot fake.
#[test]
fn the_diagonal_stamp_is_actually_rotated_and_not_merely_drawn() {
    let (dw, dh, diagonal) = paint(Some(stamp(WatermarkLayout::Diagonal)));
    let (hw, hh, horizontal) = paint(Some(stamp(WatermarkLayout::Horizontal)));

    let diagonal_ink = stamp_pixels(dw, dh, &diagonal);
    let horizontal_ink = stamp_pixels(hw, hh, &horizontal);
    assert!(!diagonal_ink.is_empty() && !horizontal_ink.is_empty());

    let span = |ink: &[(u32, u32)]| {
        let ys: Vec<u32> = ink.iter().map(|(_, y)| *y).collect();
        ys.iter().max().unwrap() - ys.iter().min().unwrap()
    };
    assert!(
        span(&diagonal_ink) > span(&horizontal_ink) * 3,
        "a climbing stamp covers far more vertical extent than a level one: \
         diagonal {} vs horizontal {}",
        span(&diagonal_ink),
        span(&horizontal_ink)
    );

    // The horizontal displacement between the ink's top band and its bottom band.
    let lean = |ink: &[(u32, u32)]| -> i64 {
        let (min_y, max_y) = (
            ink.iter().map(|(_, y)| *y).min().unwrap(),
            ink.iter().map(|(_, y)| *y).max().unwrap(),
        );
        let band = ((max_y - min_y) / 5).max(1);
        let mean_x = |lo: u32, hi: u32| -> i64 {
            let xs: Vec<i64> = ink
                .iter()
                .filter(|(_, y)| *y >= lo && *y <= hi)
                .map(|(x, _)| i64::from(*x))
                .collect();
            if xs.is_empty() {
                return 0;
            }
            xs.iter().sum::<i64>() / xs.len() as i64
        };
        mean_x(min_y, min_y + band) - mean_x(max_y - band, max_y)
    };

    // And it LEANS. The mean x of the ink's topmost rows against that of its
    // bottom rows: a climbing stamp's two ends sit at opposite sides of the page.
    //
    // Compared against a LEVEL stamp of the same word rather than against zero,
    // because a level stamp leans too — 89px here — and that is not a defect. The
    // top rows of "DRAFT" catch the narrow apex of the A and the bottom rows catch
    // its wide feet, so the mean x of a glyph's top is genuinely not the mean x of
    // its bottom. An absolute ceiling would have been a claim about letterforms
    // dressed as a claim about rotation, and it failed on exactly that.
    //
    // Measured at 96 dpi on US Letter. The numbers moved once the auto-fit was
    // corrected to bound the stamp's rotated BOX rather than its length, which is
    // why they are stated as a ratio here and re-measured rather than pinned.
    let diagonal_lean = lean(&diagonal_ink).abs();
    let horizontal_lean = lean(&horizontal_ink).abs();
    assert!(
        diagonal_lean > horizontal_lean * 3,
        "a 315° stamp must lean far more than the same word set level — that ratio \
         is what a DROPPED rotation cannot fake, since it would make the two equal. \
         Got diagonal {diagonal_lean}px vs level {horizontal_lean}px"
    );
    assert!(
        diagonal_lean > 200,
        "and it leans across a real fraction of the page, not by a glyph: {diagonal_lean}px"
    );
}

/// The two properties multiply buys, in pixels — the reason the stamp may be
/// painted on top of the page at all.
///
/// The live build reported the stamp "hiding behind" tables and pictures. It was
/// painted underneath, so anything opaque erased it; painting it on top with normal
/// blending would only have traded that for dimmed text. Multiplication escapes the
/// choice, and both halves are checkable:
///
///   * over an OPAQUE FILL covering the whole page, the stamp is still there;
///   * **no pixel anywhere on the page gets lighter.** Multiplication cannot
///     brighten, so the stamp cannot fade, wash out or erase one thing the page
///     already drew — which is precisely what painting on top would otherwise risk.
///
/// The second is the property that licenses the design, and it is asserted over
/// EVERY pixel rather than over the black ones. The first attempt claimed text
/// pixels were byte-identical, reasoning that `0 x anything = 0`. That is true of
/// pure black and false of an anti-aliased edge: a `15` edge pixel measured `0`
/// with the stamp — darker, not lighter, so text becomes a shade heavier and never
/// fainter. "Never lighter" is both the honest claim and the stronger one.
#[test]
fn a_multiplied_stamp_survives_an_opaque_fill_and_leaves_text_untouched() {
    use casual_doc_layout::display::{Color, PaintItem};

    /// Paints one page, optionally covering it first with an opaque fill — the
    /// table shading or full-page picture that used to erase the stamp.
    fn paint_over_fill(watermark: Option<Watermark>, fill: bool) -> (u32, u32, Vec<u8>) {
        // A page with real text on it, not the one-word page the other tests use:
        // the readability half of this test is vacuous without enough glyphs under
        // the stamp for the comparison to mean anything.
        let mut definitions = Definitions::default();
        definitions.sections.push(section(watermark));
        let body: Vec<BlockNode> = (0..12)
            .map(|n| {
                BlockNode::Paragraph(Paragraph {
                    id: node(100 + n * 2),
                    properties: ParagraphProperties::default().into(),
                    inlines: vec![InlineNode::Run(Run {
                        id: node(101 + n * 2),
                        properties: RunProperties::default().into(),
                        text: "The quick brown fox jumps over the lazy dog, twice over. ".repeat(3),
                    })],
                })
            })
            .collect();
        let doc = Document::new(node(1000), body, definitions).expect("valid document");
        let pages = paginate_document(&doc, &ParleyShaper::new());
        let page = &pages.pages[0];
        let width = page.page_size.width.to_device_px(DPI).ceil() as u32;
        let height = page.page_size.height.to_device_px(DPI).ceil() as u32;
        let mut list = casual_doc_layout::display::DisplayList::new();
        if fill {
            // Emitted FIRST so it is beneath everything the page paints, which is
            // exactly where a table's cell shading sits.
            list.push(PaintItem::Rect {
                rect: casual_doc_layout::units::Rect::new(
                    casual_doc_layout::units::Point::new(
                        casual_doc_layout::units::Twip(0),
                        casual_doc_layout::units::Twip(0),
                    ),
                    casual_doc_layout::units::Size::new(
                        page.page_size.width,
                        page.page_size.height,
                    ),
                ),
                fill: Some(Color::rgb(0xcc, 0xdd, 0xee)),
                stroke: None,
            });
        }
        list.items.extend(compose_page(page).items);
        let mut surface = Surface::new(width, height).unwrap();
        render(
            &list,
            &mut surface,
            DPI,
            &BundledFontSource,
            &MapMediaSource::default(),
        );
        (width, height, surface.data().to_vec())
    }

    // 1. Visible over an opaque fill that covers the entire page.
    let (w, h, filled_plain) = paint_over_fill(None, true);
    let (_, _, filled_stamped) = paint_over_fill(Some(grey_stamp(WatermarkLayout::Diagonal)), true);
    let changed = filled_plain
        .chunks(4)
        .zip(filled_stamped.chunks(4))
        .filter(|(a, b)| a[..3] != b[..3])
        .count();
    assert!(
        changed > 500,
        "a page covered by an opaque fill must still show the stamp; only {changed} \
         pixel(s) differ, which is what being painted UNDERNEATH looks like"
    );

    // 2. Nothing on the page gets lighter, anywhere. This is the whole readability
    //    guarantee: a stamp that cannot brighten cannot wash anything out.
    let (_, _, plain) = paint_over_fill(None, false);
    let (_, _, stamped) = paint_over_fill(Some(grey_stamp(WatermarkLayout::Diagonal)), false);
    let mut text_pixels = 0usize;
    for (before, after) in plain.chunks(4).zip(stamped.chunks(4)) {
        for channel in 0..3 {
            assert!(
                after[channel] <= before[channel],
                "the stamp brightened a pixel, which multiplication cannot do: \
                 {before:?} -> {after:?}"
            );
        }
        if (u32::from(before[0]) + u32::from(before[1]) + u32::from(before[2])) / 3 < 40 {
            text_pixels += 1;
        }
    }
    assert!(
        text_pixels > 500,
        "the guarantee above is vacuous without real text under the stamp; only \
         {text_pixels} near-black pixel(s) on the page"
    );
    let _ = (w, h);
}

#[test]
fn a_semitransparent_stamp_paints_lighter_than_an_opaque_one() {
    let mut faint = stamp(WatermarkLayout::Horizontal);
    faint.semi_transparent = true;
    let (fw, fh, faint_rgba) = paint(Some(faint));
    let (ow, oh, opaque_rgba) = paint(Some(stamp(WatermarkLayout::Horizontal)));

    // Mean darkness over the stamp band, so this compares ink WEIGHT rather than
    // pixel count: a faint stamp still covers the same glyph shapes.
    let mean = |w: u32, h: u32, rgba: &[u8]| -> f32 {
        let skip = h / 4;
        let mut sum = 0u64;
        let mut n = 0u64;
        for y in skip..h {
            for x in 0..w {
                let p = ((y * w + x) * 4) as usize;
                sum += u64::from(rgba[p]);
                n += 1;
            }
        }
        sum as f32 / n as f32
    };
    let faint_level = mean(fw, fh, &faint_rgba);
    let opaque_level = mean(ow, oh, &opaque_rgba);
    assert!(
        faint_level > opaque_level,
        "semitransparent must leave the page lighter: faint {faint_level} vs opaque {opaque_level}"
    );
}
