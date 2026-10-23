//! Where the CPU raster spends its time — the instrument behind `docs/179`.
//!
//! Renders every page of two documents at three device scales and prints, per
//! document and scale, the median milliseconds for:
//!
//! * `alloc` — allocate + fill the page-sized pixmap (what `Surface::new` costs);
//! * `render` — the whole display list through [`render`];
//! * `glyphs` / `vector` / `images` — the same list filtered to one family of
//!   paint items (clip and layer brackets kept), so the split is measured rather
//!   than guessed;
//! * `outline` — only the `skrifa` outline extraction for every glyph (no fill),
//!   which separates "find the glyph's shape" from "rasterize it";
//! * `copy` — the `data().to_vec()` the wasm `render_page` makes to hand the
//!   pixels to JavaScript;
//! * `compose` — the display list build (`compose_page`) the host pays once per
//!   page per change.
//!
//! The two documents are the repository's `sample.docx` and a deterministic
//! HEAVY page generated here: dense 9pt justified text with mixed bold, italic,
//! underline and highlight, a shaded 6 × 12 table and a line of emoji (colour
//! glyphs). The scales are 96 dpi (100 % zoom at DPR 1), 192 dpi (200 % at DPR 1,
//! or 100 % at DPR 2) and 384 dpi (200 % at DPR 2 — what a Retina laptop asks for
//! at 200 %, given the webapp's backing-store DPR cap of 2).
//!
//! Fonts come from the bundled set only (`ParleyShaper::without_system_fonts`),
//! the same faces a browser has, so native and wasm measure the same work.
//!
//! `cargo run --release -p casual-doc-render --example render_profile [heavy.docx-out]`
//! — the optional path writes the generated heavy document, so the wasm harness
//! (`docs/179` §2) can time the identical file through the shipped `render_page`.
#![allow(clippy::print_stdout, clippy::print_stderr)] // a measuring tool, not library code

use std::time::Instant;

use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::display::{DisplayList, PaintItem};
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_ooxml::{DocxPackage, PackageLimits};
use casual_doc_render::{GlyphSource, MapMediaSource, RegistryFontSource, Surface, render};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, GlyphId, MetadataProvider};

#[path = "support/heavy_docx.rs"]
mod heavy;
use heavy::heavy_docx;

const SAMPLE: &[u8] = include_bytes!("../../../sample.docx");
const REPEATS: usize = 7;
const SCALES: [f32; 3] = [96.0, 192.0, 384.0];

fn main() {
    let heavy = heavy_docx();
    if let Some(out) = std::env::args().nth(1) {
        std::fs::write(&out, &heavy).expect("write the heavy fixture");
        eprintln!("wrote the heavy fixture to {out}");
    }
    println!(
        "doc,page,items,glyphs,dpi,px,alloc_ms,render_ms,glyphs_ms,vector_ms,images_ms,outline_ms,copy_ms,compose_ms"
    );
    profile("sample.docx", SAMPLE);
    profile("heavy", &heavy);
}

fn profile(name: &str, bytes: &[u8]) {
    let mut package = DocxPackage::open(bytes, PackageLimits::default()).expect("opens");
    let document = import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .expect("imports")
    .document;
    let mut media = MapMediaSource::new();
    for (_id, reference) in document.definitions().media.iter() {
        if let Ok(part) = package.read_part(&reference.part_name) {
            media.insert(reference.part_name.clone(), part);
        }
    }
    let shaper = ParleyShaper::without_system_fonts();
    let pages = paginate_document(&document, &shaper);
    let registry = shaper.registry();
    let fonts = RegistryFontSource::new(&registry);

    for (index, page) in pages.pages.iter().enumerate() {
        let compose_ms = median(|| {
            std::hint::black_box(compose_page(page));
        });
        let list = compose_page(page);
        let glyph_count = count_glyphs(&list);
        let only_glyphs = filtered(&list, |item| matches!(item, PaintItem::Glyphs { .. }));
        let only_vector = filtered(&list, |item| {
            matches!(
                item,
                PaintItem::Rect { .. }
                    | PaintItem::Ellipse { .. }
                    | PaintItem::RoundedRect { .. }
                    | PaintItem::Polygon { .. }
                    | PaintItem::Shape { .. }
                    | PaintItem::Line { .. }
            )
        });
        let only_images = filtered(&list, |item| matches!(item, PaintItem::Image { .. }));
        for dpi in SCALES {
            let w = page.page_size.width.to_device_px(dpi).ceil() as u32;
            let h = page.page_size.height.to_device_px(dpi).ceil() as u32;
            let alloc = median(|| {
                std::hint::black_box(Surface::new(w, h).expect("surface"));
            });
            let time_list = |list: &DisplayList| {
                median(|| {
                    let mut surface = Surface::new(w, h).expect("surface");
                    render(list, &mut surface, dpi, &fonts, &media);
                    std::hint::black_box(surface.data()[0]);
                }) - alloc
            };
            let full = time_list(&list);
            let glyphs = time_list(&only_glyphs);
            let vector = time_list(&only_vector);
            let images = time_list(&only_images);
            let outline = median(|| extract_outlines(&list, dpi, &fonts));
            let surface = Surface::new(w, h).expect("surface");
            let copy = median(|| {
                std::hint::black_box(surface.data().to_vec());
            });
            println!(
                "{name},{},{},{glyph_count},{dpi},{w}x{h},{alloc:.2},{full:.2},{glyphs:.2},{vector:.2},{images:.2},{outline:.2},{copy:.2},{compose_ms:.2}",
                index + 1,
                list.items.len(),
            );
        }
    }
}

/// Median wall-clock milliseconds of `REPEATS` runs, after one warm-up.
fn median(mut f: impl FnMut()) -> f64 {
    f();
    let mut samples: Vec<f64> = (0..REPEATS)
        .map(|_| {
            let start = Instant::now();
            f();
            start.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    samples.sort_by(f64::total_cmp);
    samples[REPEATS / 2]
}

/// `list` with only the items `keep` accepts, plus every clip / layer bracket so
/// the clip stack still balances.
fn filtered(list: &DisplayList, keep: impl Fn(&PaintItem) -> bool) -> DisplayList {
    let mut out = DisplayList::new();
    for item in &list.items {
        let bracket = matches!(
            item,
            PaintItem::PushClip(_)
                | PaintItem::PopClip
                | PaintItem::PushLayer { .. }
                | PaintItem::PopLayer
        );
        if bracket || keep(item) {
            out.push(item.clone());
        }
    }
    out
}

fn count_glyphs(list: &DisplayList) -> usize {
    list.items
        .iter()
        .map(|item| match item {
            PaintItem::Glyphs { run } => run.glyphs.len(),
            _ => 0,
        })
        .sum()
}

/// Extracts every glyph outline the list names, exactly as the renderer does,
/// into a throwaway pen — the "shape lookup" half of text cost.
fn extract_outlines(list: &DisplayList, dpi: f32, fonts: &dyn GlyphSource) {
    let mut sink = CountingPen(0);
    for item in &list.items {
        let PaintItem::Glyphs { run } = item else {
            continue;
        };
        let Some(bytes) = fonts.font_data(run.font) else {
            continue;
        };
        let Ok(font) = FontRef::from_index(bytes, fonts.face_index(run.font)) else {
            continue;
        };
        let outlines = font.outline_glyphs();
        let size = run.size.to_device_px(dpi);
        for glyph in &run.glyphs {
            if let Some(outline) = outlines.get(GlyphId::new(glyph.id)) {
                let settings = DrawSettings::unhinted(Size::new(size), LocationRef::default());
                let _ = outline.draw(settings, &mut sink);
            }
        }
    }
    std::hint::black_box(sink.0);
}

struct CountingPen(usize);

impl OutlinePen for CountingPen {
    fn move_to(&mut self, _: f32, _: f32) {
        self.0 += 1;
    }
    fn line_to(&mut self, _: f32, _: f32) {
        self.0 += 1;
    }
    fn quad_to(&mut self, _: f32, _: f32, _: f32, _: f32) {
        self.0 += 1;
    }
    fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {
        self.0 += 1;
    }
    fn close(&mut self) {
        self.0 += 1;
    }
}
