//! Emoji put ink on the page, in every block container — the guard that matters.
//!
//! The model held the emoji the whole time this was broken (the status bar
//! counted them), so a test over the document would have passed throughout. The
//! failure was that nothing drew them. This test therefore renders the page and
//! looks at the pixels.
//!
//! The uniform-flow rule says every block container flows through the same
//! pipeline as the body, so one coverage fix should serve all of them — but
//! "should" is the word that lets a container quietly miss out, and the owner
//! saw this in a **table cell**. `fixtures/generated/emoji-containers.docx` puts
//! the same five emoji in four containers and this asserts each one paints:
//!
//! | container | where it lives |
//! | --- | --- |
//! | body paragraph | `page.placed`, a `Paragraph` |
//! | table cell | `page.placed`, a `TableRow`'s cell blocks |
//! | running header | `page.header` |
//! | footnote body | `page.footnotes` |
//!
//! Deliberately NOT a golden image: a hash would move on any font or antialias
//! change and tells you nothing about which container went dark. Ink inside the
//! emoji run's own box is the guarantee; the exact pixels are the circumstance.

use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::display::PaintItem;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::fonts::NOTO_EMOJI;
use casual_doc_layout::page::{Page, PlacedFragment};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::text::GlyphRun;
use casual_doc_layout::units::Twip;
use casual_doc_render::{BundledFontSource, NoMediaSource, Surface, render};

const EMOJI_CONTAINERS_DOCX: &[u8] =
    include_bytes!("../../../fixtures/generated/emoji-containers.docx");
const DPI: f32 = 96.0;

fn first_page() -> Page {
    let mut package = DocxPackage::open(EMOJI_CONTAINERS_DOCX, PackageLimits::default())
        .expect("the emoji fixture opens");
    let imported = import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .expect("the emoji fixture imports");
    // The DETERMINISTIC shaper: bundled faces only, exactly what a browser has
    // before it fetches anything. A native `ParleyShaper::new()` would resolve
    // these emoji from the developer's OS colour emoji face, so this test would
    // pass on macOS with the bundle empty — which is the state that shipped.
    let mut pages =
        paginate_document(&imported.document, &ParleyShaper::without_system_fonts()).pages;
    assert!(!pages.is_empty(), "the fixture paginates to a page");
    pages.remove(0)
}

use casual_doc_ooxml::{DocxPackage, PackageLimits};

/// Every glyph run inside `fragment` (recursing into table cells and text boxes)
/// that was shaped from the bundled emoji family.
fn emoji_runs_in(fragment: &BlockFragment, out: &mut Vec<GlyphRun>) {
    match fragment {
        BlockFragment::Paragraph { lines, .. } => {
            for line in &lines.lines {
                for run in &line.runs {
                    if NOTO_EMOJI.contains(run.font) {
                        out.push(run.clone());
                    }
                }
                for text_box in &line.text_boxes {
                    for block in &text_box.blocks {
                        emoji_runs_in(block, out);
                    }
                }
            }
        }
        BlockFragment::TableRow { cells, .. } => {
            for cell in cells {
                for block in &cell.blocks {
                    emoji_runs_in(block, out);
                }
            }
        }
    }
}

fn emoji_runs(placed: &[PlacedFragment]) -> Vec<GlyphRun> {
    let mut out = Vec::new();
    for fragment in placed {
        emoji_runs_in(&fragment.fragment, &mut out);
    }
    out
}

/// The emoji runs reached each of the four containers, shaped from the emoji
/// face rather than from a Latin face's glyph ids.
#[test]
fn every_container_shapes_its_emoji_with_the_emoji_face() {
    let page = first_page();
    let body: Vec<_> = page
        .placed
        .iter()
        .filter(|placed| matches!(placed.fragment, BlockFragment::Paragraph { .. }))
        .cloned()
        .collect();
    let table: Vec<_> = page
        .placed
        .iter()
        .filter(|placed| matches!(placed.fragment, BlockFragment::TableRow { .. }))
        .cloned()
        .collect();

    for (container, placed) in [
        ("body paragraph", body),
        ("table cell", table),
        ("running header", page.header.clone()),
        ("footnote body", page.footnotes.clone()),
    ] {
        let runs = emoji_runs(&placed);
        assert!(
            !runs.is_empty(),
            "{container}: no run shaped with the bundled emoji face — this container \
             did not get the fix the body got",
        );
        for run in &runs {
            assert!(
                run.glyphs.iter().all(|glyph| glyph.id != 0),
                "{container}: a .notdef glyph survived — tofu",
            );
        }
    }
}

/// Non-white pixels inside `rect` (device px, clamped to the surface).
fn ink_in(surface: &Surface, width: u32, height: u32, rect: (f32, f32, f32, f32)) -> usize {
    let (left, top, right, bottom) = rect;
    let x0 = left.floor().max(0.0) as u32;
    let y0 = top.floor().max(0.0) as u32;
    let x1 = (right.ceil() as u32).min(width);
    let y1 = (bottom.ceil() as u32).min(height);
    let data = surface.data();
    let mut ink = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let index = ((y * width + x) * 4) as usize;
            let pixel = &data[index..index + 4];
            if pixel[0] != 255 || pixel[1] != 255 || pixel[2] != 255 {
                ink += 1;
            }
        }
    }
    ink
}

/// The composed page actually PAINTS each emoji run — the assertion a
/// model-level test can never make, and the one that was missing while every
/// emoji in the product rendered as a box.
#[test]
fn every_emoji_run_on_the_page_puts_ink_where_it_stands() {
    let page = first_page();
    let list = compose_page(&page);
    let width = (page.page_size.width.to_device_px(DPI)).ceil() as u32;
    let height = (page.page_size.height.to_device_px(DPI)).ceil() as u32;
    let mut surface = Surface::new(width, height).expect("page surface");
    render(&list, &mut surface, DPI, &BundledFontSource, &NoMediaSource);

    let emoji: Vec<&GlyphRun> = list
        .items
        .iter()
        .filter_map(|item| match item {
            PaintItem::Glyphs { run } if NOTO_EMOJI.contains(run.font) => Some(run),
            _ => None,
        })
        .collect();
    assert!(
        emoji.len() >= 4,
        "expected an emoji run in each of the four containers, found {}",
        emoji.len(),
    );

    for run in emoji {
        let advance: Twip = run
            .glyphs
            .iter()
            .fold(Twip::ZERO, |sum, glyph| sum + glyph.advance);
        let left = run.origin.x.to_device_px(DPI);
        let baseline = run.origin.y.to_device_px(DPI);
        let box_px = (
            left,
            baseline - run.ascent.to_device_px(DPI),
            left + advance.to_device_px(DPI),
            baseline + run.descent.to_device_px(DPI),
        );
        let ink = ink_in(&surface, width, height, box_px);
        assert!(
            ink > 0,
            "an emoji run at {box_px:?} painted NOTHING — the run is in the display \
             list and the pixels under it are blank",
        );
    }
}
