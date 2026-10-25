//! `renderPageSparse`: the opt-in second raster (`docs/architecture/179`, ADR-073 Proposed).
//!
//! The SAME page `renderPage` draws — same layout, same compose options, same
//! fonts, same media, same background — painted by `casual-doc-render-gpu`'s
//! sparse-strip encoder on `vello_cpu` instead of `tiny-skia`. Same return
//! contract (premultiplied RGBA8888 in a [`PageBitmap`]), so a host can swap
//! the call behind a flag and compare.
//!
//! Compiled only with the `sparse-raster` feature, which `webapp/build.sh` does
//! not enable: the shipped module, its size and `renderPage` are unchanged.
//! `tiny-skia` stays the default and the reference; this backend is held to it
//! by `casual-doc-render-gpu/tests/parity.rs`.

use casual_doc_layout::compose::{ComposeOptions, compose_page_with};
use casual_doc_render::RegistryFontSource;
use casual_doc_render_gpu::cpu::render_sparse_cpu;
use wasm_bindgen::prelude::*;

use crate::{BorrowedMedia, PageBitmap, WasmDocument, to_js};

#[wasm_bindgen]
impl WasmDocument {
    /// [`WasmDocument::render_page`] through the sparse-strip raster. Measured
    /// 3–5x faster on text-dense pages (`docs/architecture/179` §4); within the parity
    /// tolerance of `renderPage`, not pixel-identical to it.
    ///
    /// Throws where `renderPage` throws, and when a side exceeds 65,535 device
    /// pixels (the renderer's 16-bit coordinates) — call `renderPage` then.
    #[wasm_bindgen(js_name = renderPageSparse)]
    pub fn render_page_sparse(&mut self, index: u32, dpi: f32) -> Result<PageBitmap, JsValue> {
        self.render_page_sparse_inner(index, dpi).map_err(to_js)
    }
}

impl WasmDocument {
    fn render_page_sparse_inner(&mut self, index: u32, dpi: f32) -> Result<PageBitmap, String> {
        self.ensure_page_resident(index);
        let page = self.render_page_of(index)?;
        let size = self.page_box(page);
        let width_px = size.width.to_device_px(dpi).ceil() as u32;
        let height_px = size.height.to_device_px(dpi).ceil() as u32;
        if width_px == 0 || height_px == 0 {
            return Err(format!("allocate surface: {width_px}x{height_px}"));
        }
        let background = self
            .document
            .background()
            .map_or([255, 255, 255], |c| [c.r, c.g, c.b]);
        let registry = self.shaper.registry();
        let fonts = RegistryFontSource::new(&registry);
        let list = compose_page_with(page, &ComposeOptions { marks: self.marks });
        let (rgba, _stats) = render_sparse_cpu(
            &list,
            width_px,
            height_px,
            dpi,
            background,
            &fonts,
            &BorrowedMedia(self.resources.as_map()),
        )
        .ok_or_else(|| format!("{width_px}x{height_px} exceeds the sparse raster's 65,535 px"))?;
        Ok(PageBitmap {
            width_px,
            height_px,
            rgba,
        })
    }
}

#[cfg(test)]
mod tests {

    /// The opt-in paints the page `renderPage` paints: same size, and ink in
    /// the same places within the parity tolerance's spirit (here: most pixels
    /// agree closely). The full tolerance lives in `casual-doc-render-gpu`.
    #[test]
    fn the_sparse_raster_paints_the_page_render_page_paints() {
        let bytes = include_bytes!("../../../sample.docx");
        let mut doc = crate::open(bytes).expect("sample opens");
        let reference = doc.render_page_inner(1, 96.0).expect("cpu page");
        let sparse = doc.render_page_sparse_inner(1, 96.0).expect("sparse page");
        assert_eq!(
            (reference.width_px, reference.height_px),
            (sparse.width_px, sparse.height_px)
        );
        let total: u64 = reference
            .rgba
            .iter()
            .zip(&sparse.rgba)
            .map(|(a, b)| u64::from(a.abs_diff(*b)))
            .sum();
        let mean = total as f64 / reference.rgba.len() as f64;
        let ink = sparse.rgba.chunks_exact(4).filter(|p| p[0] < 128).count();
        assert!(
            ink > 1_000,
            "the sparse page has text on it ({ink} dark pixels)"
        );
        assert!(mean < 3.0, "mean channel difference {mean:.3}");
    }
}
