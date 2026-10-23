//! The wasm-bindgen façade of the Viewer SDK (`docs/176` §3.1).
//!
//! A thin, owned-value boundary over [`ViewerDocument`]: bytes in, bytes or plain
//! numbers out. No callbacks, no events, no editing verbs — a viewer host blits
//! `renderPage`'s RGBA or shows `renderPagePng`'s PNG and is done.

use wasm_bindgen::prelude::*;

use crate::ViewerDocument;

/// A document opened read-only in the browser.
#[wasm_bindgen]
#[derive(Debug)]
pub struct WasmViewer {
    inner: ViewerDocument,
}

/// A rasterised page crossing the boundary.
#[wasm_bindgen]
#[derive(Debug)]
pub struct WasmPageBitmap {
    width_px: u32,
    height_px: u32,
    rgba: Vec<u8>,
}

#[wasm_bindgen]
impl WasmPageBitmap {
    /// Width in device pixels.
    #[wasm_bindgen(getter, js_name = widthPx)]
    #[must_use]
    pub fn width_px(&self) -> u32 {
        self.width_px
    }

    /// Height in device pixels.
    #[wasm_bindgen(getter, js_name = heightPx)]
    #[must_use]
    pub fn height_px(&self) -> u32 {
        self.height_px
    }

    /// Premultiplied RGBA8888 for `putImageData`.
    #[must_use]
    pub fn rgba(&self) -> Vec<u8> {
        self.rgba.clone()
    }
}

fn to_js(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

#[wasm_bindgen]
impl WasmViewer {
    /// Opens and paginates `bytes`. Throws on a document it cannot open.
    pub fn open(bytes: &[u8]) -> Result<WasmViewer, JsValue> {
        ViewerDocument::open(bytes)
            .map(|inner| Self { inner })
            .map_err(to_js)
    }

    /// The number of laid-out pages.
    #[wasm_bindgen(getter, js_name = pageCount)]
    #[must_use]
    pub fn page_count(&self) -> u32 {
        u32::try_from(self.inner.page_count()).unwrap_or(u32::MAX)
    }

    /// `[width, height]` of page `index` in twips; empty past the end.
    #[wasm_bindgen(js_name = pageSizeTwips)]
    #[must_use]
    pub fn page_size_twips(&self, index: u32) -> Vec<i32> {
        self.inner
            .page_size_twips(index as usize)
            .map_or_else(Vec::new, |(w, h)| vec![w, h])
    }

    /// Registers a host-fetched face and re-paginates once.
    #[wasm_bindgen(js_name = registerFont)]
    pub fn register_font(&mut self, bytes: &[u8]) {
        self.inner.register_font(bytes.to_vec());
    }

    /// Rasterises page `index` at `dpi`.
    #[wasm_bindgen(js_name = renderPage)]
    pub fn render_page(&self, index: u32, dpi: f32) -> Result<WasmPageBitmap, JsValue> {
        let bitmap = self.inner.render_page(index as usize, dpi).map_err(to_js)?;
        Ok(WasmPageBitmap {
            width_px: bitmap.width_px,
            height_px: bitmap.height_px,
            rgba: bitmap.rgba,
        })
    }

    /// Page `index` at `dpi` as PNG bytes (thumbnails).
    #[wasm_bindgen(js_name = renderPagePng)]
    pub fn render_page_png(&self, index: u32, dpi: f32) -> Result<Vec<u8>, JsValue> {
        self.inner
            .render_page_png(index as usize, dpi)
            .map_err(to_js)
    }

    /// The body text, one line per paragraph.
    #[wasm_bindgen(js_name = plainText)]
    #[must_use]
    pub fn plain_text(&self) -> String {
        self.inner.plain_text()
    }

    /// The whole document as real-text PDF bytes.
    #[cfg(feature = "pdf")]
    #[wasm_bindgen(js_name = toPdf)]
    pub fn to_pdf(&self) -> Result<Vec<u8>, JsValue> {
        self.inner.to_pdf().map_err(to_js)
    }
}
