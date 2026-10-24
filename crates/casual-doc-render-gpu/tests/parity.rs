//! The second backend paints what the reference paints — within a stated
//! tolerance, page by page.
//!
//! `casual-doc-render` (tiny-skia) is the golden reference; nothing here moves
//! it. Each page of each fixture is rendered by the reference and by this
//! crate, and compared pixel by pixel within the tolerance `support/mod.rs`
//! states (mean difference, share of disagreeing pixels, and the worst
//! 32-pixel square, so one missing word cannot hide on a full page).
//!
//! Two painters consume the same encoder:
//!
//! * `vello_cpu` (`sparse-cpu`) runs everywhere, CI's Linux runners included,
//!   so the encoder is always checked;
//! * `vello_hybrid` (`gpu`) runs where a GPU adapter exists (a developer's
//!   machine, CI's macOS runner). Where none exists the test says so and
//!   passes: the CPU check above already covers the encoder, and a missing GPU
//!   is the documented "use the CPU raster" case, not a defect.

#![cfg(any(feature = "sparse-cpu", feature = "gpu"))]
// The per-page lines are the log a reviewer reads when the gate trips.
#![allow(clippy::print_stderr)]

use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::display::DisplayList;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_ooxml::{DocxPackage, PackageLimits};
use casual_doc_render::{MapMediaSource, RegistryFontSource, Surface, render};

mod support;
use support::difference;

#[allow(dead_code)] // the generator file also carries what only the examples use
#[path = "../../casual-doc-render/examples/support/heavy_docx.rs"]
mod heavy;

/// The scales checked: 100 % at DPR 1, and 200 % (or 100 % at DPR 2).
const SCALES: [f32; 2] = [96.0, 192.0];

/// One page ready to paint on either backend.
struct Page {
    name: String,
    list: DisplayList,
    width_twip: i32,
    height_twip: i32,
}

struct Doc {
    pages: Vec<Page>,
    fonts: RegistryFontSource,
    media: MapMediaSource,
}

fn fixtures() -> Vec<(&'static str, Vec<u8>)> {
    let mut out: Vec<(&'static str, Vec<u8>)> = vec![
        (
            "sample.docx",
            include_bytes!("../../../sample.docx").to_vec(),
        ),
        ("heavy", heavy::heavy_docx()),
    ];
    macro_rules! generated {
        ($($name:literal),* $(,)?) => {
            $(out.push((
                $name,
                include_bytes!(concat!("../../../fixtures/generated/", $name)).to_vec(),
            ));)*
        };
    }
    // Shapes and custom geometry (fills, gradients, dashes, arrowheads), a chart,
    // a rotated multiply-blended watermark (a layer), colour emoji in every
    // container (the glyph fallback), pictures, merged tables and mixed scripts.
    generated!(
        "shapes.docx",
        "preset-shapes.docx",
        "custom-geometry.docx",
        "vml-shapes.docx",
        "chart.docx",
        "watermark.docx",
        "emoji-containers.docx",
        "wrapped-pictures.docx",
        "merged-tables.docx",
        "mixed-unicode.docx",
        "grouped-text-boxes.docx",
    );
    out
}

fn load(name: &str, bytes: &[u8]) -> Doc {
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
    // Bundled faces only: what every machine has, so the reference is the same
    // page on a developer's Mac and on CI.
    let shaper = ParleyShaper::without_system_fonts();
    let paginated = paginate_document(&document, &shaper);
    let fonts = RegistryFontSource::new(&shaper.registry());
    let pages = paginated
        .pages
        .iter()
        .enumerate()
        .map(|(index, page)| Page {
            name: format!("{name} p{}", index + 1),
            list: compose_page(page),
            width_twip: page.page_size.width.raw(),
            height_twip: page.page_size.height.raw(),
        })
        .collect();
    Doc {
        pages,
        fonts,
        media,
    }
}

fn device(twip: i32, dpi: f32) -> u32 {
    (twip as f32 * dpi / 1440.0).ceil() as u32
}

/// Renders every page at every scale on the reference and through `paint`,
/// and returns every page outside the tolerance, worst first, with a line per
/// page for the log.
fn check(label: &str, mut paint: impl FnMut(&Page, &Doc, u32, u32, f32) -> Vec<u8>) {
    let mut failures = Vec::new();
    let mut checked = 0usize;
    for (name, bytes) in fixtures() {
        let doc = load(name, &bytes);
        for page in &doc.pages {
            for dpi in SCALES {
                let (w, h) = (device(page.width_twip, dpi), device(page.height_twip, dpi));
                let mut surface = Surface::new(w, h).expect("surface");
                render(&page.list, &mut surface, dpi, &doc.fonts, &doc.media);
                let candidate = paint(page, &doc, w, h, dpi);
                let diff = difference(surface.data(), &candidate, w as usize);
                checked += 1;
                eprintln!("{label} {} @{dpi}: {diff}", page.name);
                if !diff.within() {
                    support::dump(
                        &format!("{label}-{}-{dpi}", page.name),
                        w,
                        h,
                        surface.data(),
                        &candidate,
                    );
                    failures.push(format!("{} @{dpi} dpi: {diff}", page.name));
                }
            }
        }
    }
    assert!(checked > 40, "the fixture set rendered ({checked} pages)");
    assert!(
        failures.is_empty(),
        "{label} differs from the CPU reference beyond the stated tolerance on {} page(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[cfg(feature = "sparse-cpu")]
#[test]
fn the_encoder_on_the_sparse_cpu_painter_matches_the_reference() {
    use casual_doc_render_gpu::cpu::render_sparse_cpu;
    check("vello_cpu", |page, doc, w, h, dpi| {
        render_sparse_cpu(
            &page.list,
            w,
            h,
            dpi,
            [255, 255, 255],
            &doc.fonts,
            &doc.media,
        )
        .expect("within 16-bit coordinates")
        .0
    });
}

#[cfg(feature = "gpu")]
#[test]
fn the_gpu_painter_matches_the_reference_where_a_gpu_exists() {
    use casual_doc_render_gpu::gpu::GpuRenderer;
    let mut gpu = match GpuRenderer::new_blocking() {
        Ok(gpu) => gpu,
        Err(err) => {
            eprintln!("SKIPPED: {err}. The sparse-cpu test checks the encoder on this machine.");
            return;
        }
    };
    eprintln!("adapter: {}", gpu.adapter());
    check("vello_hybrid", |page, doc, w, h, dpi| {
        gpu.render_rgba(
            &page.list,
            w,
            h,
            dpi,
            [255, 255, 255],
            &doc.fonts,
            &doc.media,
        )
        .expect("renders")
        .rgba
    });
}
