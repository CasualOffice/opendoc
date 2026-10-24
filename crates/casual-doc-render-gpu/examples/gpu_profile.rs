//! The GPU backend against the CPU reference, page by page — the instrument
//! behind `docs/179` §4.
//!
//! For every page of `sample.docx` and the generated heavy page, at 96, 192 and
//! 384 dpi, prints the median milliseconds of:
//!
//! * `cpu` — the reference: allocate the surface and `casual_doc_render::render`;
//! * `sparse_cpu` — the same encoder on `vello_cpu` (single-threaded);
//! * `gpu_submit` — encode onto `vello_hybrid` and submit: the CPU side of a GPU
//!   frame (path flattening and strip building happen here);
//! * `gpu_view` — `gpu_submit` plus waiting for the GPU to finish, into a
//!   texture: what a host that owns a WebGPU canvas pays;
//! * `gpu_rgba` — `gpu_view` plus reading the page back as RGBA: what an RGBA
//!   host (`putImageData`) would pay;
//!
//! and, against the CPU page, the largest channel difference, the mean channel
//! difference, and the share of pixels whose largest channel difference exceeds
//! the parity test's per-pixel threshold.
//!
//! `cargo run --release -p casual-doc-render-gpu --all-features --example gpu_profile`
#![allow(clippy::print_stdout, clippy::print_stderr)] // a measuring tool, not library code

use std::time::Instant;

use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_ooxml::{DocxPackage, PackageLimits};
use casual_doc_render::{MapMediaSource, RegistryFontSource, Surface, render};
use casual_doc_render_gpu::cpu::render_sparse_cpu;
use casual_doc_render_gpu::gpu::GpuRenderer;

#[path = "../../casual-doc-render/examples/support/heavy_docx.rs"]
mod heavy;

const SAMPLE: &[u8] = include_bytes!("../../../sample.docx");
const REPEATS: usize = 7;
const SCALES: [f32; 3] = [96.0, 192.0, 384.0];
/// The parity test's per-pixel threshold (`tests/parity.rs`).
const PIXEL_THRESHOLD: u8 = 64;

fn main() {
    let mut gpu = match GpuRenderer::new_blocking() {
        Ok(gpu) => gpu,
        Err(err) => {
            eprintln!("no GPU here ({err}); nothing to measure");
            return;
        }
    };
    eprintln!("adapter: {}", gpu.adapter());
    println!(
        "doc,page,dpi,px,cpu_ms,sparse_cpu_ms,gpu_submit_ms,gpu_view_ms,gpu_rgba_ms,fallback_items,max_diff,mean_diff,over_threshold_pct"
    );
    profile(&mut gpu, "sample.docx", SAMPLE);
    profile(&mut gpu, "heavy", &heavy::heavy_docx());
}

fn profile(gpu: &mut GpuRenderer, name: &str, bytes: &[u8]) {
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
    let white = [255, 255, 255];

    for (index, page) in pages.pages.iter().enumerate() {
        let list = compose_page(page);
        for dpi in SCALES {
            let w = page.page_size.width.to_device_px(dpi).ceil() as u32;
            let h = page.page_size.height.to_device_px(dpi).ceil() as u32;
            let mut reference = Vec::new();
            let cpu = median(|| {
                let mut surface = Surface::new(w, h).expect("surface");
                render(&list, &mut surface, dpi, &fonts, &media);
                reference = surface.data().to_vec();
            });
            let sparse = median(|| {
                std::hint::black_box(render_sparse_cpu(&list, w, h, dpi, white, &fonts, &media));
            });
            let view = gpu.offscreen_view(w, h);
            let with_view = median(|| {
                gpu.render_to_view(&list, w, h, dpi, white, &fonts, &media, &view)
                    .expect("renders");
                gpu.wait_idle().expect("idle");
            });
            let mut page_out = None;
            let rgba = median(|| {
                page_out = Some(
                    gpu.render_rgba(&list, w, h, dpi, white, &fonts, &media)
                        .expect("renders"),
                );
            });
            let page_out = page_out.expect("rendered");
            let (max, mean, over) = compare(&reference, &page_out.rgba);
            println!(
                "{name},{},{dpi},{w}x{h},{cpu:.2},{sparse:.2},{:.2},{with_view:.2},{rgba:.2},{},{max},{mean:.3},{over:.3}",
                index + 1,
                submit_only(gpu, &list, w, h, dpi, &fonts, &media, &view),
                page_out.stats.fallback_items,
            );
        }
    }
}

/// The CPU side alone: median time from the start of the encode to the return
/// of the submit, with the GPU drained outside the timed region.
#[allow(clippy::too_many_arguments)]
fn submit_only(
    gpu: &mut GpuRenderer,
    list: &casual_doc_layout::display::DisplayList,
    w: u32,
    h: u32,
    dpi: f32,
    fonts: &RegistryFontSource,
    media: &MapMediaSource,
    view: &wgpu::TextureView,
) -> f64 {
    let mut samples = Vec::with_capacity(REPEATS);
    for round in 0..=REPEATS {
        let start = Instant::now();
        gpu.render_to_view(list, w, h, dpi, [255, 255, 255], fonts, media, view)
            .expect("renders");
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        gpu.wait_idle().expect("idle");
        if round > 0 {
            samples.push(elapsed);
        }
    }
    samples.sort_by(f64::total_cmp);
    samples[REPEATS / 2]
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

/// Largest channel difference, mean channel difference, and the percentage of
/// pixels whose largest channel difference exceeds `PIXEL_THRESHOLD`.
fn compare(a: &[u8], b: &[u8]) -> (u8, f64, f64) {
    let mut max = 0u8;
    let mut sum = 0u64;
    let mut over = 0u64;
    for (pa, pb) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
        let mut worst = 0u8;
        for c in 0..4 {
            let d = pa[c].abs_diff(pb[c]);
            sum += u64::from(d);
            worst = worst.max(d);
        }
        max = max.max(worst);
        if worst > PIXEL_THRESHOLD {
            over += 1;
        }
    }
    let pixels = (a.len() / 4).max(1) as f64;
    (
        max,
        sum as f64 / (pixels * 4.0),
        over as f64 * 100.0 / pixels,
    )
}
