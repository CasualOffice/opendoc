//! Renders one page of an arbitrary `.docx` to a PNG — a visual-fidelity probe
//! over the real sample corpus. `render_gallery <in.docx> <out.png> [page]`.
#![allow(clippy::print_stderr, clippy::print_stdout)] // a manual harness
use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::document_layout::{document_page_config, paginate_document};
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_ooxml::{DocxPackage, PackageLimits};
use casual_doc_render::{MapMediaSource, RegistryFontSource, Surface, render};

fn main() {
    let mut args = std::env::args().skip(1);
    let input = args
        .next()
        .expect("usage: render_gallery <in.docx> <out.png> [page]");
    let out = args
        .next()
        .expect("usage: render_gallery <in.docx> <out.png> [page]");
    let page_idx: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(0);

    let bytes = std::fs::read(&input).expect("read docx");
    let limits = PackageLimits {
        max_input_bytes: 64 * 1024 * 1024,
        max_total_expanded_bytes: 256 * 1024 * 1024,
        max_single_expanded_bytes: 64 * 1024 * 1024,
        ..PackageLimits::default()
    };
    let mut package = match DocxPackage::open(&bytes, limits) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("OPEN FAILED {input}: {e:?}");
            return;
        }
    };
    let imported = match import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    ) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("IMPORT FAILED {input}: {e:?}");
            return;
        }
    };
    let document = imported.document;

    let mut media = MapMediaSource::new();
    for (_id, reference) in document.definitions().media.iter() {
        if let Ok(part_bytes) = package.read_part(&reference.part_name) {
            media.insert(reference.part_name.clone(), part_bytes);
        }
    }

    let shaper = ParleyShaper::new();
    // One call: real per-section geometry, flowed headers/footers, anchored
    // drawings, and page-number fields.
    let pages = paginate_document(&document, &shaper);
    let config = document_page_config(&document);
    let page = match pages.pages.get(page_idx) {
        Some(p) => p,
        None => {
            eprintln!(
                "no page {page_idx} ({} pages) for {input}",
                pages.page_count()
            );
            return;
        }
    };

    let dpi = 96.0;
    let w = config.page_size.width.to_device_px(dpi).ceil() as u32;
    let h = config.page_size.height.to_device_px(dpi).ceil() as u32;
    let mut surface = Surface::new(w, h).unwrap();
    // Serve bundled + fallback (OS/system, with --features system-fonts) faces
    // from the shaper's registry, taken after pagination shaped every paragraph.
    let registry = shaper.registry();
    let fonts = RegistryFontSource::new(&registry);
    render(&compose_page(page), &mut surface, dpi, &fonts, &media);
    std::fs::write(&out, surface.encode_png().unwrap()).unwrap();
    let uncovered = registry.missing_coverage().len();
    println!(
        "OK {input}: {} pages -> {out} (page {page_idx}){}",
        pages.page_count(),
        if uncovered > 0 {
            format!(" [{uncovered} uncovered code points]")
        } else {
            String::new()
        },
    );
}
