//! Imports a real corpus .docx and renders its first page to a PNG — the full
//! pipeline: import -> galley -> paginate -> compose -> render. Manual check.
#![allow(clippy::print_stderr)] // a manual example, not library code
use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_layout::anchor::{collect_anchored, place_anchored_drawings};
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::flow::build_galley;
use casual_doc_layout::paginate::{PageConfig, paginate};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::units::{Size, Twip};
use casual_doc_model::NodeId;
use casual_doc_model::v1::SectionId;
use casual_doc_ooxml::{DocxPackage, PackageLimits};
use casual_doc_render::{BundledFontSource, MapMediaSource, Surface, render};

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

    let config = PageConfig {
        section: SectionId::new(NodeId::from_parts(9, 1).unwrap()),
        page_size: Size::new(Twip(12_240), Twip(15_840)), // US Letter
        margin_top: Twip(1_440),
        margin_bottom: Twip(1_440),
        margin_start: Twip(1_440),
        margin_end: Twip(1_440),
        header_height: Twip(0),
        footer_height: Twip(0),
    };
    let shaper = ParleyShaper::new();
    let galley = build_galley(&document, &shaper, config.content_area().size.width);
    let mut pages = paginate(&galley, &config);
    // Post-pagination pass: resolve every anchored (floating) drawing onto its
    // page at its computed absolute position (P1F-28).
    place_anchored_drawings(&mut pages, &collect_anchored(&document), &config);
    let page = pages.pages.first().expect("at least one page");

    let dpi = 96.0;
    let w = config.page_size.width.to_device_px(dpi).ceil() as u32;
    let h = config.page_size.height.to_device_px(dpi).ceil() as u32;
    let mut surface = Surface::new(w, h).unwrap();
    render(
        &compose_page(page),
        &mut surface,
        dpi,
        &BundledFontSource,
        &media,
    );
    std::fs::write(&out, surface.encode_png().unwrap()).unwrap();
    eprintln!(
        "rendered {} paragraphs across {} page(s) -> {out}",
        galley.len(),
        pages.page_count()
    );
}
