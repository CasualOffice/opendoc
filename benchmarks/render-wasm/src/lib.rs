//! Times the CPU raster and the sparse-strip encoder inside WebAssembly
//! (`docs/architecture/179` §2), and keeps the GPU path reachable for the size measurement
//! (§5). No wasm-bindgen: raw exports, timed from JS with `performance.now()`.
//! Run through `run.sh`.
#![allow(unsafe_code)]
use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::display::DisplayList;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_ooxml::{DocxPackage, PackageLimits};
use casual_doc_render::{MapMediaSource, RegistryFontSource, Surface, render};
use std::cell::RefCell;

struct State {
    lists: Vec<(DisplayList, f32, f32)>,
    fonts: RegistryFontSource,
    media: MapMediaSource,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    static INPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

#[unsafe(no_mangle)]
pub extern "C" fn input_ptr(len: usize) -> *mut u8 {
    INPUT.with(|i| {
        let mut i = i.borrow_mut();
        i.resize(len, 0);
        i.as_mut_ptr()
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn load() -> u32 {
    let bytes = INPUT.with(|i| i.borrow().clone());
    let mut package = DocxPackage::open(&bytes, PackageLimits::default()).unwrap();
    let document = import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .unwrap()
    .document;
    let mut media = MapMediaSource::new();
    for (_id, r) in document.definitions().media.iter() {
        if let Ok(p) = package.read_part(&r.part_name) {
            media.insert(r.part_name.clone(), p);
        }
    }
    let shaper = ParleyShaper::without_system_fonts();
    let pages = paginate_document(&document, &shaper);
    let fonts = RegistryFontSource::new(&shaper.registry());
    let lists = pages
        .pages
        .iter()
        .map(|p| {
            (
                compose_page(p),
                p.page_size.width.to_device_px(1.0),
                p.page_size.height.to_device_px(1.0),
            )
        })
        .collect::<Vec<_>>();
    let n = lists.len() as u32;
    STATE.with(|s| {
        *s.borrow_mut() = Some(State {
            lists,
            fonts,
            media,
        })
    });
    n
}

fn size(w: f32, h: f32, dpi: f32) -> (u32, u32) {
    ((w * dpi).ceil() as u32, (h * dpi).ceil() as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn render_cpu(page: u32, dpi: f32) -> u32 {
    STATE.with(|s| {
        let s = s.borrow();
        let s = s.as_ref().unwrap();
        let (l, w, h) = &s.lists[page as usize];
        let (w, h) = size(*w, *h, dpi);
        let mut surface = Surface::new(w, h).unwrap();
        render(l, &mut surface, dpi, &s.fonts, &s.media);
        let v = surface.data().to_vec();
        u32::from(v[v.len() / 2])
    })
}

#[cfg(feature = "sparse")]
#[unsafe(no_mangle)]
pub extern "C" fn render_sparse(page: u32, dpi: f32) -> u32 {
    STATE.with(|s| {
        let s = s.borrow();
        let s = s.as_ref().unwrap();
        let (l, w, h) = &s.lists[page as usize];
        let (w, h) = size(*w, *h, dpi);
        let (v, _) = casual_doc_render_gpu::cpu::render_sparse_cpu(
            l,
            w,
            h,
            dpi,
            [255, 255, 255],
            &s.fonts,
            &s.media,
        )
        .unwrap();
        u32::from(v[v.len() / 2])
    })
}

/// Keeps the whole GPU path reachable so the size measurement is honest: polls
/// the adapter request once and holds the page-render entry point.
#[cfg(feature = "gpu")]
#[unsafe(no_mangle)]
pub extern "C" fn gpu_probe() -> u32 {
    use casual_doc_render_gpu::gpu::GpuRenderer;
    use std::future::Future;
    let mut fut = Box::pin(GpuRenderer::new());
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    let ready = fut.as_mut().poll(&mut cx).is_ready();
    let render = GpuRenderer::render_to_view as *const () as usize;
    std::hint::black_box(render) as u32 + u32::from(ready)
}
