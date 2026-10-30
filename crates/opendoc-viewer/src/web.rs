//! The wasm-bindgen façade of the Viewer SDK (`docs/176` §3.1).
//!
//! A thin, owned-value boundary over [`ViewerDocument`]: bytes in, bytes, plain
//! numbers or JSON text out. No callbacks, no events, no editing verbs. The
//! JavaScript package (`packages/opendoc-viewer/src/index.js`) wraps this into
//! the documented API; hosts use that, not these names.
//!
//! Several faces cross the boundary as ONE byte buffer plus a list of lengths,
//! so registering a family's four faces is one call and one layout, without a
//! `js-sys` dependency for arrays of arrays.

use wasm_bindgen::prelude::*;

use crate::{ViewerDocument, ViewerOptions};

/// A document opened read-only in the browser or in Node.
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

/// Splits `blob` into the faces `lengths` describe. A length list that runs
/// past the buffer is refused rather than truncated.
fn split_faces(blob: &[u8], lengths: &[u32]) -> Result<Vec<Vec<u8>>, JsValue> {
    let mut faces = Vec::with_capacity(lengths.len());
    let mut at = 0usize;
    for &length in lengths {
        let end = at
            .checked_add(length as usize)
            .filter(|end| *end <= blob.len())
            .ok_or_else(|| JsValue::from_str("font lengths run past the font buffer"))?;
        faces.push(blob[at..end].to_vec());
        at = end;
    }
    Ok(faces)
}

#[wasm_bindgen]
impl WasmViewer {
    /// Opens and paginates `bytes`. Throws a sentence on a document it cannot
    /// open.
    pub fn open(bytes: &[u8]) -> Result<WasmViewer, JsValue> {
        ViewerDocument::open(bytes)
            .map(|inner| Self { inner })
            .map_err(to_js)
    }

    /// Opens `bytes` named `name`: with the `formats` feature the name selects
    /// Markdown or HTML, whose bytes carry no evidence of their own.
    #[wasm_bindgen(js_name = openNamed)]
    pub fn open_named(bytes: &[u8], name: &str) -> Result<WasmViewer, JsValue> {
        ViewerDocument::open_with(bytes, &ViewerOptions::default().with_file_name(name))
            .map(|inner| Self { inner })
            .map_err(to_js)
    }

    /// Opens `bytes` with faces registered BEFORE the first layout (one
    /// pagination instead of two).
    #[wasm_bindgen(js_name = openWithFonts)]
    pub fn open_with_fonts(
        bytes: &[u8],
        font_blob: &[u8],
        font_lengths: Vec<u32>,
    ) -> Result<WasmViewer, JsValue> {
        let mut options = ViewerOptions::default();
        for face in split_faces(font_blob, &font_lengths)? {
            options = options.with_font(face);
        }
        ViewerDocument::open_with(bytes, &options)
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

    /// Every page's `[width, height]` in twips, flattened.
    #[wasm_bindgen(js_name = pageSizesTwips)]
    #[must_use]
    pub fn page_sizes_twips(&self) -> Vec<i32> {
        self.inner
            .page_sizes_twips()
            .into_iter()
            .flat_map(|(w, h)| [w, h])
            .collect()
    }

    /// Registers the faces in `font_blob` (split by `font_lengths`) and
    /// re-paginates once. Returns the number of faces registered.
    #[wasm_bindgen(js_name = registerFonts)]
    pub fn register_fonts(
        &mut self,
        font_blob: &[u8],
        font_lengths: Vec<u32>,
    ) -> Result<u32, JsValue> {
        let faces = split_faces(font_blob, &font_lengths)?;
        Ok(u32::try_from(self.inner.register_fonts(faces)).unwrap_or(u32::MAX))
    }

    /// Registers a coverage-fallback face for comma-separated ISO 15924
    /// `scripts` and re-paginates once. Returns the number of faces registered.
    #[wasm_bindgen(js_name = registerFallbackFont)]
    pub fn register_fallback_font(&mut self, bytes: &[u8], scripts: &str) -> u32 {
        let scripts: Vec<&str> = scripts
            .split(',')
            .map(str::trim)
            .filter(|script| !script.is_empty())
            .collect();
        u32::try_from(self.inner.register_fallback_font(bytes.to_vec(), &scripts))
            .unwrap_or(u32::MAX)
    }

    /// The fonts the layout asked for and did not have, as JSON:
    /// `[{"requested", "substitute", "substituteAvailable"}]`.
    #[wasm_bindgen(js_name = fontNeedsJson)]
    #[must_use]
    pub fn font_needs_json(&self) -> String {
        serde_json::Value::Array(
            self.inner
                .font_needs()
                .iter()
                .map(|need| {
                    serde_json::json!({
                        "requested": need.requested,
                        "substitute": need.substitute,
                        "substituteAvailable": need.substitute_available,
                    })
                })
                .collect(),
        )
        .to_string()
    }

    /// Code points no registered face could draw, as scalar values.
    #[wasm_bindgen(js_name = missingCodePoints)]
    #[must_use]
    pub fn missing_code_points(&self) -> Vec<u32> {
        self.inner
            .missing_code_points()
            .iter()
            .map(|&ch| u32::from(ch))
            .collect()
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

    /// The body text laid out on page `index`.
    #[wasm_bindgen(js_name = pageText)]
    pub fn page_text(&self, index: u32) -> Result<String, JsValue> {
        self.inner.page_text(index as usize).map_err(to_js)
    }

    /// The body text, one line per paragraph.
    #[wasm_bindgen(js_name = plainText)]
    #[must_use]
    pub fn plain_text(&self) -> String {
        self.inner.plain_text()
    }

    /// The heading outline as JSON: `[{"level", "text", "page"}]`, `page`
    /// 0-based or `null`.
    #[wasm_bindgen(js_name = outlineJson)]
    #[must_use]
    pub fn outline_json(&self) -> String {
        serde_json::Value::Array(
            self.inner
                .outline()
                .into_iter()
                .map(|entry| {
                    serde_json::json!({
                        "level": entry.level,
                        "text": entry.text,
                        "page": entry.page,
                    })
                })
                .collect(),
        )
        .to_string()
    }

    /// Whether this build carries the `pdf` feature.
    #[wasm_bindgen(js_name = hasPdf)]
    #[must_use]
    pub fn has_pdf() -> bool {
        cfg!(feature = "pdf")
    }

    /// The whole document as real-text PDF bytes.
    #[cfg(feature = "pdf")]
    #[wasm_bindgen(js_name = toPdf)]
    pub fn to_pdf(&self) -> Result<Vec<u8>, JsValue> {
        self.inner.to_pdf().map_err(to_js)
    }
}
