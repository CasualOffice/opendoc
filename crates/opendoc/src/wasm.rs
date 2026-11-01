// SPDX-License-Identifier: Apache-2.0

//! The wasm-bindgen facade that `@casualoffice/opendoc` (`packages/opendoc`)
//! wraps. No DOM and no `web-sys`: Node, Deno, Bun, workers and pages alike.
//!
//! Every structured value crosses as JSON (the read view's own serialisation,
//! so the package and the Rust API cannot drift), and every byte payload as a
//! `Uint8Array`. An error is a `JsError` whose message starts with the stable
//! code (`"open: ..."`), which the package turns into an `OpenDocError`.
//!
//! Nothing here is called by a native test: constructing a `JsValue` off
//! wasm32 aborts. The package's `node --test` suite exercises it instead.

use wasm_bindgen::prelude::*;

use crate::{
    Document, DocumentSpec, EditCommand, Error, Format, OpenOptions, RenderOptions, SaveMode,
    TemplateOptions, Undo,
};

fn js(error: Error) -> JsError {
    JsError::new(&error.to_string())
}

fn json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_owned())
}

fn parse_format(name: &str) -> Result<Format, JsError> {
    name.parse().map_err(js)
}

/// Collects media for a spec, then builds the document.
#[wasm_bindgen]
#[derive(Debug, Default)]
pub struct WasmBuilder {
    media: Vec<Vec<u8>>,
}

#[wasm_bindgen]
impl WasmBuilder {
    /// An empty builder.
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a picture's bytes; returns the index an image's `media` names.
    #[wasm_bindgen(js_name = addMedia)]
    pub fn add_media(&mut self, bytes: &[u8]) -> u32 {
        self.media.push(bytes.to_vec());
        u32::try_from(self.media.len() - 1).unwrap_or(u32::MAX)
    }

    /// The DOCX package for a spec, without opening it.
    ///
    /// # Errors
    ///
    /// `build: ...` naming the first invalid value.
    #[wasm_bindgen(js_name = buildDocx)]
    pub fn build_docx(&self, spec_json: &str) -> Result<Vec<u8>, JsError> {
        let spec: DocumentSpec = serde_json::from_str(spec_json)
            .map_err(|e| JsError::new(&format!("build: not a document spec: {e}")))?;
        crate::builder::build_docx(&spec, &self.media).map_err(js)
    }

    /// Builds and opens the document.
    ///
    /// # Errors
    ///
    /// As `buildDocx`.
    pub fn build(&self, spec_json: &str) -> Result<WasmDocument, JsError> {
        Document::from_spec_json(spec_json, &self.media)
            .map(WasmDocument::new)
            .map_err(js)
    }
}

/// An open document.
#[wasm_bindgen]
#[derive(Debug)]
pub struct WasmDocument {
    inner: Document,
    last_report: String,
    last_width: u32,
    last_height: u32,
    fonts: Vec<Vec<u8>>,
}

impl WasmDocument {
    fn new(inner: Document) -> Self {
        Self {
            inner,
            last_report: "null".to_owned(),
            last_width: 0,
            last_height: 0,
            fonts: Vec::new(),
        }
    }
}

#[wasm_bindgen]
impl WasmDocument {
    /// Opens bytes. `format` and `file_name` are optional hints (`""` for none).
    ///
    /// # Errors
    ///
    /// `open: ...` / `unsupported-format: ...`.
    pub fn open(
        bytes: &[u8],
        format: &str,
        file_name: &str,
        author: &str,
    ) -> Result<WasmDocument, JsError> {
        let options = OpenOptions {
            format: if format.is_empty() {
                None
            } else {
                Some(parse_format(format)?)
            },
            file_name: (!file_name.is_empty()).then(|| file_name.to_owned()),
            author: (!author.is_empty()).then(|| author.to_owned()),
            render: RenderOptions::default(),
        };
        Document::open_with(bytes, &options)
            .map(Self::new)
            .map_err(js)
    }

    /// A blank document.
    ///
    /// # Errors
    ///
    /// Never in practice.
    pub fn create() -> Result<WasmDocument, JsError> {
        Document::create().map(Self::new).map_err(js)
    }

    /// The source format's short name, or `""`.
    #[wasm_bindgen(js_name = sourceFormat)]
    #[must_use]
    pub fn source_format(&self) -> String {
        self.inner
            .source_format()
            .map_or(String::new(), |f| f.name().to_owned())
    }

    /// Whether it changed since it was opened.
    #[wasm_bindgen(js_name = isModified)]
    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.inner.is_modified()
    }

    /// Sets the review author.
    #[wasm_bindgen(js_name = setAuthor)]
    pub fn set_author(&mut self, author: &str) {
        self.inner.set_author(author);
    }

    /// The import report, JSON.
    #[wasm_bindgen(js_name = importReport)]
    #[must_use]
    pub fn import_report(&self) -> String {
        json(self.inner.import_report())
    }

    /// The body's blocks, JSON.
    #[must_use]
    pub fn body(&self) -> String {
        json(&self.inner.body())
    }

    /// Plain text.
    #[must_use]
    pub fn text(&self) -> String {
        self.inner.text()
    }

    /// Sections, JSON.
    #[must_use]
    pub fn sections(&self) -> String {
        json(&self.inner.sections())
    }

    /// Styles, JSON.
    #[must_use]
    pub fn styles(&self) -> String {
        json(&self.inner.styles())
    }

    /// Comments, JSON.
    #[must_use]
    pub fn comments(&self) -> String {
        json(&self.inner.comments())
    }

    /// Tracked changes, JSON.
    #[must_use]
    pub fn revisions(&self) -> String {
        json(&self.inner.revisions())
    }

    /// Headers and footers, JSON.
    #[wasm_bindgen(js_name = headersFooters)]
    #[must_use]
    pub fn headers_footers(&self) -> String {
        json(&self.inner.headers_footers())
    }

    /// Notes, JSON.
    #[must_use]
    pub fn notes(&self) -> String {
        json(&self.inner.notes())
    }

    /// Media parts, JSON.
    #[must_use]
    pub fn media(&self) -> String {
        json(&self.inner.media())
    }

    /// A media part's bytes (empty when absent).
    #[wasm_bindgen(js_name = mediaBytes)]
    #[must_use]
    pub fn media_bytes(&self, part: &str) -> Vec<u8> {
        self.inner
            .media_bytes(part)
            .map(<[u8]>::to_vec)
            .unwrap_or_default()
    }

    /// Properties, JSON.
    #[must_use]
    pub fn properties(&self) -> String {
        json(&self.inner.properties())
    }

    /// Runs edit commands (a JSON array) as one batch; returns
    /// `{"results": [...], "undo": "..."}`.
    ///
    /// # Errors
    ///
    /// The first failing command's error; nothing changes.
    pub fn edit(&mut self, commands_json: &str) -> Result<String, JsError> {
        let commands: Vec<EditCommand> = serde_json::from_str(commands_json)
            .map_err(|e| JsError::new(&format!("invalid-argument: not a list of edits: {e}")))?;
        let (results, undo) = self.inner.edit_commands(&commands).map_err(js)?;
        Ok(serde_json::json!({ "results": results, "undo": undo.to_json() }).to_string())
    }

    /// Applies an undo; returns the redo.
    ///
    /// # Errors
    ///
    /// When it no longer fits.
    pub fn undo(&mut self, undo_json: &str) -> Result<String, JsError> {
        let undo = Undo::from_json(undo_json).map_err(js)?;
        self.inner
            .undo(&undo)
            .map(|redo| redo.to_json())
            .map_err(js)
    }

    /// Fills a template. `options_json`: `{"delimiters":{"start","end"},"nullValue","strict"}`.
    ///
    /// # Errors
    ///
    /// `template: ...` with every issue as JSON after the `|`.
    #[wasm_bindgen(js_name = renderTemplate)]
    pub fn render_template(&mut self, data_json: &str, options_json: &str) -> Result<(), JsError> {
        let data: serde_json::Value = serde_json::from_str(data_json)
            .map_err(|e| JsError::new(&format!("invalid-argument: data is not JSON: {e}")))?;
        let options = template_options(options_json)?;
        self.inner.render_template(&data, &options).map_err(js)
    }

    /// The tags a template uses, JSON `[{name, kind}]`.
    #[wasm_bindgen(js_name = templateTags)]
    pub fn template_tags(&self, options_json: &str) -> Result<String, JsError> {
        let options = template_options(options_json)?;
        let tags: Vec<_> = self
            .inner
            .template_tags(&options)
            .into_iter()
            .map(|(name, kind)| serde_json::json!({ "name": name, "kind": kind }))
            .collect();
        Ok(json(&tags))
    }

    /// Saves. `mode`: `"exact"`, `"preserve"` or `"semantic"` (`""` for the
    /// default). The report is `lastSaveReport()`.
    ///
    /// # Errors
    ///
    /// `unsupported-format: ...` / `save: ...`.
    pub fn save(&mut self, format: &str, mode: &str) -> Result<Vec<u8>, JsError> {
        let format = parse_format(format)?;
        let mode = if mode.is_empty() {
            SaveMode::default()
        } else {
            mode.parse().map_err(js)?
        };
        let (bytes, report) = self.inner.save(format, mode).map_err(js)?;
        self.last_report = json(&report);
        Ok(bytes)
    }

    /// The last save's report, JSON.
    #[wasm_bindgen(js_name = lastSaveReport)]
    #[must_use]
    pub fn last_save_report(&self) -> String {
        self.last_report.clone()
    }

    /// Laid-out pages.
    #[wasm_bindgen(js_name = pageCount)]
    #[must_use]
    pub fn page_count(&self) -> u32 {
        u32::try_from(self.inner.page_count()).unwrap_or(u32::MAX)
    }

    /// Page sizes, JSON `[{widthPt, heightPt}]`.
    #[wasm_bindgen(js_name = pageSizes)]
    #[must_use]
    pub fn page_sizes(&self) -> String {
        let sizes: Vec<_> = self
            .inner
            .page_sizes()
            .into_iter()
            .map(|s| serde_json::json!({ "widthPt": s.width_pt, "heightPt": s.height_pt }))
            .collect();
        json(&sizes)
    }

    /// One page's text.
    ///
    /// # Errors
    ///
    /// Past the last page.
    #[wasm_bindgen(js_name = pageText)]
    pub fn page_text(&self, index: u32) -> Result<String, JsError> {
        self.inner.page_text(index as usize).map_err(js)
    }

    /// One page as PNG.
    ///
    /// # Errors
    ///
    /// Past the last page, or a bad DPI.
    #[wasm_bindgen(js_name = renderPagePng)]
    pub fn render_page_png(&self, index: u32, dpi: f32) -> Result<Vec<u8>, JsError> {
        self.inner.render_page_png(index as usize, dpi).map_err(js)
    }

    /// One page as SVG, from the same layout as the PNG and the PDF.
    ///
    /// # Errors
    ///
    /// Past the last page.
    #[wasm_bindgen(js_name = renderPageSvg)]
    pub fn render_page_svg(&self, index: u32) -> Result<String, JsError> {
        self.inner.render_page_svg(index as usize).map_err(js)
    }

    /// One page as RGBA; its size is `lastRenderWidth`/`lastRenderHeight`.
    ///
    /// # Errors
    ///
    /// As `renderPagePng`.
    #[wasm_bindgen(js_name = renderPageRgba)]
    pub fn render_page_rgba(&mut self, index: u32, dpi: f32) -> Result<Vec<u8>, JsError> {
        let image = self.inner.render_page(index as usize, dpi).map_err(js)?;
        self.last_width = image.width;
        self.last_height = image.height;
        Ok(image.rgba)
    }

    /// The last RGBA render's width.
    #[wasm_bindgen(js_name = lastRenderWidth)]
    #[must_use]
    pub fn last_render_width(&self) -> u32 {
        self.last_width
    }

    /// The last RGBA render's height.
    #[wasm_bindgen(js_name = lastRenderHeight)]
    #[must_use]
    pub fn last_render_height(&self) -> u32 {
        self.last_height
    }

    /// A real-text PDF.
    ///
    /// # Errors
    ///
    /// `render: ...`.
    #[wasm_bindgen(js_name = toPdf)]
    pub fn to_pdf(&self) -> Result<Vec<u8>, JsError> {
        self.inner.to_pdf().map_err(js)
    }

    /// Headings with their pages, JSON `[{level, text, page}]`.
    #[must_use]
    pub fn outline(&self) -> String {
        let outline: Vec<_> = self
            .inner
            .outline()
            .into_iter()
            .map(|(level, text, page)| serde_json::json!({ "level": level, "text": text, "page": page }))
            .collect();
        json(&outline)
    }

    /// Families layout substituted, JSON.
    #[wasm_bindgen(js_name = fontNeeds)]
    #[must_use]
    pub fn font_needs(&self) -> String {
        json(&self.inner.font_needs())
    }

    /// Registers a font file for layout (re-lays the document out once).
    #[wasm_bindgen(js_name = registerFont)]
    pub fn register_font(&mut self, bytes: &[u8]) {
        self.fonts.push(bytes.to_vec());
        self.inner.set_render_options(RenderOptions {
            system_fonts: false,
            fonts: self.fonts.clone(),
        });
    }
}

fn template_options(json: &str) -> Result<TemplateOptions, JsError> {
    let mut options = TemplateOptions::default();
    if json.trim().is_empty() {
        return Ok(options);
    }
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| JsError::new(&format!("invalid-argument: options are not JSON: {e}")))?;
    if let Some(delimiters) = value.get("delimiters") {
        if let Some(start) = delimiters.get("start").and_then(|v| v.as_str()) {
            options.start = start.to_owned();
        }
        if let Some(end) = delimiters.get("end").and_then(|v| v.as_str()) {
            options.end = end.to_owned();
        }
    }
    if let Some(null) = value.get("nullValue").and_then(|v| v.as_str()) {
        options.null_value = null.to_owned();
    }
    if let Some(strict) = value.get("strict").and_then(serde_json::Value::as_bool) {
        options.strict = strict;
    }
    Ok(options)
}

/// Converts bytes to another format in one call (semantic save).
///
/// # Errors
///
/// As open and save.
#[wasm_bindgen]
pub fn convert(bytes: &[u8], to: &str) -> Result<Vec<u8>, JsError> {
    crate::convert(bytes, parse_format(to)?)
        .map(|(bytes, _)| bytes)
        .map_err(js)
}

/// The SDK version.
#[wasm_bindgen]
#[must_use]
pub fn version() -> String {
    crate::VERSION.to_owned()
}

/// Every format and its directions, JSON.
#[wasm_bindgen]
#[must_use]
pub fn formats() -> String {
    let list: Vec<_> = Format::ALL
        .into_iter()
        .map(|f| {
            serde_json::json!({
                "name": f.name(), "mime": f.mime_type(),
                "open": f.can_open(), "save": f.can_save(),
                "carriesDocument": f.carries_document()
            })
        })
        .collect();
    json(&list)
}
