//! Imports a real corpus .docx and renders its first page to a PNG — the full
//! pipeline: import -> `paginate_document` -> compose -> render. Manual check.
//!
//! The one-call [`paginate_document`] driver derives the real page geometry from
//! the document's section (page size + margins), flows the section's
//! headers/footers, paginates, and runs the running-content / field / anchored
//! -drawing passes — so this renders a real page at the document's true size, with
//! its headers/footers and anchored images, rather than a hand-built US-Letter box.
//!
//! Fonts are served through a [`RegistryFontSource`] built from the shaper's
//! dynamic font registry, so glyphs the shaper resolved to a fallback face render
//! from that face's bytes. On native the render crate enables the OS system-font
//! source by default, so a run of CJK / symbol / complex-script text that the
//! bundled Latin faces do not cover shapes and rasterizes with an installed OS font
//! instead of `.notdef` tofu — no feature flag needed:
//! `cargo run --example render_docx_page <out.png>`.
#![allow(clippy::print_stderr)] // a manual example, not library code
use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_ooxml::{DocxPackage, PackageLimits};
use casual_doc_render::{MapMediaSource, RegistryFontSource, Surface, render};

fn main() {
    let out = std::env::args()
        .nth(1)
        .expect("usage: render_docx_page <out.png>");
    let bytes = include_bytes!("../../../fixtures/corpus/real-producer-rich.docx");
    let mut package = DocxPackage::open(bytes, PackageLimits::default()).unwrap();
    let document = import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .unwrap()
    .document;

    // Serve the package's embedded pictures (`word/media/*`) to the renderer,
    // keyed by part name — the display list's media key. Mirrors how fonts are
    // served through a `GlyphSource`.
    let mut media = MapMediaSource::new();
    for (_id, reference) in document.definitions().media.iter() {
        if let Ok(part_bytes) = package.read_part(&reference.part_name) {
            media.insert(reference.part_name.clone(), part_bytes);
        }
    }

    let shaper = ParleyShaper::new();
    // One call: derive the document's true geometry, flow its headers/footers,
    // paginate, and run every post-pass (running content, page fields, anchors).
    let pages = paginate_document(&document, &shaper);
    // The page dimensions for the render surface come from the same derived
    // geometry the driver used (full page size, band-independent).
    let page = pages.pages.first().expect("at least one page");

    let dpi = 96.0;
    let w = page.page_size.width.to_device_px(dpi).ceil() as u32;
    let h = page.page_size.height.to_device_px(dpi).ceil() as u32;
    let mut surface = Surface::new(w, h).unwrap();
    // Serve bundled faces *and* any fallback face the shaper resolved (an OS font
    // with `--features system-fonts`, or a host-registered blob) from the shaper's
    // dynamic registry — taken after pagination has shaped every paragraph.
    let registry = shaper.registry();
    let fonts = RegistryFontSource::new(&registry);
    render(&compose_page(page), &mut surface, dpi, &fonts, &media);
    std::fs::write(&out, surface.encode_png().unwrap()).unwrap();
    let uncovered = registry.missing_coverage();
    eprintln!(
        "rendered {} page(s) at {}×{} twips -> {out}",
        pages.page_count(),
        page.page_size.width.raw(),
        page.page_size.height.raw(),
    );
    if !uncovered.is_empty() {
        let sample: String = uncovered.iter().take(16).collect();
        eprintln!(
            "coverage gap: {} code point(s) with no covering face (e.g. {sample:?}) — \
             build with --features system-fonts, or register a covering font",
            uncovered.len(),
        );
    }
}
