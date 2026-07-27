//! `casual-doc-wasm` — the `wasm-bindgen` façade the browser/webview viewer drives.
//!
//! This crate is a **bridge, not an engine** (doc 57 §1): it exposes the existing
//! Rust pipeline across the JS boundary. One handle, [`WasmDocument`], owns an
//! imported document, its [`PaginatedLayout`], and the shaper whose font registry
//! served that pagination — so rendering serves the exact faces layout shaped.
//!
//! P1G-001 surface (doc 57 §4.1–4.3): [`open`], [`WasmDocument::page_count`],
//! [`WasmDocument::page_size`], [`WasmDocument::render_page`]. The text layer,
//! hit-testing, and editing methods (§4.4–4.6) land in later milestones.
//!
//! Units are twips on the model side and device pixels only at the raster
//! boundary (doc 57 §3): `render_page(i, dpi)` rasterizes at `dpi`, where
//! `device_px = twip / 1440 * dpi`.

use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::document_layout::{document_page_config, paginate_document};
use casual_doc_layout::flow::node_plain_text;
use casual_doc_layout::hittest::{HitZone, LayoutSnapshot};
use casual_doc_layout::model::{ModelPos, ModelRange};
use casual_doc_layout::page::{Page, PaginatedLayout};
use casual_doc_layout::paginate::PageConfig;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::units::{Point, Rect, Size, Twip};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{BlockNode, Document};
use casual_doc_ooxml::{DocxPackage, PackageLimits};
use casual_doc_render::{MapMediaSource, RegistryFontSource, Surface, render};
use std::str::FromStr;
use wasm_bindgen::Clamped;
use wasm_bindgen::prelude::*;

/// Package admission limits for the viewer (64 MiB input, 256 MiB expanded) —
/// the same envelope the native `render_gallery` probe uses, so a document that
/// opens there opens here. All other bounds keep their [`PackageLimits::default`]
/// values.
fn viewer_limits() -> PackageLimits {
    PackageLimits {
        max_input_bytes: 64 * 1024 * 1024,
        max_total_expanded_bytes: 256 * 1024 * 1024,
        max_single_expanded_bytes: 64 * 1024 * 1024,
        ..PackageLimits::default()
    }
}

/// An open document: the imported model, its current pagination, and the shaper
/// (and its font registry) that produced it, plus the media bytes rendering needs.
///
/// Held together because rendering must serve the faces the shaper actually
/// resolved during pagination — [`ParleyShaper::registry`] is only complete after
/// [`paginate_document`] has shaped every paragraph.
#[wasm_bindgen]
pub struct WasmDocument {
    document: Document,
    layout: PaginatedLayout,
    shaper: ParleyShaper,
    media: MapMediaSource,
    /// The first-section page geometry — the fallback when a page's section id
    /// resolves to no boundary (a document with no `w:sectPr` falls back to
    /// US-Letter here, exactly as [`document_page_config`] defines).
    default_config: PageConfig,
}

impl core::fmt::Debug for WasmDocument {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // `ParleyShaper` and `MapMediaSource` are opaque; report the shape a
        // caller can act on without dumping the whole model.
        f.debug_struct("WasmDocument")
            .field("pages", &self.layout.page_count())
            .field("media_parts", &self.document.definitions().media.len())
            .finish_non_exhaustive()
    }
}

/// Imports a `.docx` and paginates it. Returns a handle over the open document.
///
/// Errors (bad ZIP, admission failure, import failure) throw a JS `Error`.
///
// TODO(P1G-003+): surface the structured `SdkError` `code`/`severity` and the
// import compatibility report on the thrown error, per doc 57 §5.5, so the host
// can show "opened with N unsupported constructs" instead of only a message.
#[wasm_bindgen]
pub fn open(bytes: &[u8]) -> Result<WasmDocument, JsValue> {
    open_document(bytes).map_err(to_js)
}

#[wasm_bindgen]
impl WasmDocument {
    /// The number of laid-out pages.
    #[wasm_bindgen(getter, js_name = pageCount)]
    #[must_use]
    pub fn page_count(&self) -> u32 {
        // A paginated document never exceeds u32 pages within the admission
        // limits; the cast is saturating for defensiveness.
        u32::try_from(self.layout.page_count()).unwrap_or(u32::MAX)
    }

    /// The page box size of page `index` (0-based), in twips, resolved against
    /// that page's own section (`w:sectPr`) so multi-section documents report
    /// per-section geometry.
    ///
    /// Throws if `index` is out of range.
    #[wasm_bindgen(js_name = pageSize)]
    pub fn page_size(&self, index: u32) -> Result<PageSize, JsValue> {
        self.page_size_inner(index).map_err(to_js)
    }

    /// Rasterizes page `index` (0-based) at `dpi` device pixels per inch and
    /// returns raw premultiplied RGBA8888 the frontend blits via `putImageData`.
    ///
    /// Throws if `index` is out of range or the surface size is invalid.
    #[wasm_bindgen(js_name = renderPage)]
    pub fn render_page(&self, index: u32, dpi: f32) -> Result<PageBitmap, JsValue> {
        self.render_page_inner(index, dpi).map_err(to_js)
    }

    /// The code points the last pagination could **not** cover with any available
    /// face — i.e. what renders as `.notdef` tofu (▯). Returned as `u32` scalar
    /// values. A host queries this to decide which fallback fonts to fetch
    /// (typically CJK / complex scripts absent from the bundled Latin faces).
    #[wasm_bindgen(js_name = missingCoverage)]
    #[must_use]
    pub fn missing_coverage(&self) -> Vec<u32> {
        self.shaper
            .registry()
            .missing_coverage()
            .into_iter()
            .map(|c| c as u32)
            .collect()
    }

    /// Registers a host-provided font (e.g. a network-fetched Noto face) as a
    /// **coverage fallback** for the given ISO-15924 scripts (`"Hani"`, `"Hira"`,
    /// `"Kana"`, `"Hang"`, …), then re-paginates so runs the bundled faces miss now
    /// shape and render with it. This is the browser half of the font-provisioning
    /// strategy — the single host-populatable seam.
    ///
    /// `scripts` may be empty to register the face without wiring script fallback
    /// (see [`WasmDocument::register_font`]).
    #[wasm_bindgen(js_name = registerFallbackFont)]
    pub fn register_fallback_font(&mut self, bytes: &[u8], scripts: Vec<String>) {
        let refs: Vec<&str> = scripts.iter().map(String::as_str).collect();
        self.shaper.register_fallback_font(bytes.to_vec(), &refs);
        self.repaginate();
    }

    /// Registers a host-provided font by family (no script-fallback wiring), then
    /// re-paginates. Use when a document names a face the host can supply directly;
    /// for CJK / complex-script coverage prefer
    /// [`register_fallback_font`](Self::register_fallback_font).
    #[wasm_bindgen(js_name = registerFont)]
    pub fn register_font(&mut self, bytes: &[u8]) {
        self.shaper.register_font(bytes.to_vec());
        self.repaginate();
    }

    // ---- Selection & copy (P1G-003) ----------------------------------------
    //
    // The interaction pipeline of doc 58: a page-local point resolves to a model
    // anchor (`hitTest`); anchors form a `Selection` the frontend draws from
    // engine geometry (`caretRect`/`selectionRects`) so the highlight matches the
    // raster exactly; `copyText` is the first read-only action over a selection.
    // All positions are `NodeId` (32-hex string) + node-relative UTF-8 byte
    // offset — the layout anchor space (doc 58 §3).

    /// Resolves a page-local point (twips) on 1-based `page` to the nearest caret
    /// anchor. Returns `undefined` if the page has no lines. `zone` is `"content"`
    /// (inside a line) or `"outside"` (snapped in from a margin).
    #[wasm_bindgen(js_name = hitTest)]
    #[must_use]
    pub fn hit_test(&self, page: u32, x_twip: i32, y_twip: i32) -> Option<HitPayload> {
        let snapshot = LayoutSnapshot::new(&self.layout);
        let hit = snapshot.hit_test(page, Point::new(Twip(x_twip), Twip(y_twip)))?;
        Some(HitPayload {
            node: hit.pos.node.to_string(),
            offset: hit.pos.offset,
            zone: match hit.zone {
                HitZone::Content => "content",
                HitZone::Outside => "outside",
            },
        })
    }

    /// The caret box for a model anchor, as `[page, xTwip, yTwip, wTwip, hTwip]`
    /// (page-local; `w` is 0). Empty if the anchor resolves to no line (e.g. a
    /// stale position) or `node` is malformed.
    #[wasm_bindgen(js_name = caretRect)]
    #[must_use]
    pub fn caret_rect(&self, node: &str, offset: u32) -> Vec<i32> {
        let Some(pos) = parse_pos(node, offset) else {
            return Vec::new();
        };
        LayoutSnapshot::new(&self.layout)
            .caret_rect(pos)
            .map(|(page, rect)| flat_rect(page, rect).to_vec())
            .unwrap_or_default()
    }

    /// Highlight rectangles for a selection, flattened as `[page, x, y, w, h, …]`
    /// (page-local twips), one 5-tuple per covered line-fragment. The range may
    /// span nodes and pages; an empty/inverted range yields no rectangles.
    #[wasm_bindgen(js_name = selectionRects)]
    #[must_use]
    pub fn selection_rects(
        &self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
    ) -> Vec<i32> {
        let Some(range) = parse_range(start_node, start_offset, end_node, end_offset) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (page, rect) in LayoutSnapshot::new(&self.layout).selection_rects(range) {
            out.extend_from_slice(&flat_rect(page, rect));
        }
        out
    }

    /// The plain text a selection covers — the first (read-only) action of the
    /// interaction pipeline (doc 58 §4). Walks the model between the two anchors
    /// in document order, slicing each node's shaped text at the byte offsets and
    /// joining paragraphs with `\n`. Empty if either anchor is unknown.
    ///
    // Byte-exact for `Run`/`Tab`/wrapper content (the common case). Documents whose
    // paragraphs contain fields/symbols/inline objects that contribute shaped
    // glyphs may drift; hardening to a shaping-time extractor is a follow-up.
    #[wasm_bindgen(js_name = copyText)]
    #[must_use]
    pub fn copy_text(
        &self,
        start_node: &str,
        start_offset: u32,
        end_node: &str,
        end_offset: u32,
    ) -> String {
        let Some(range) = parse_range(start_node, start_offset, end_node, end_offset) else {
            return String::new();
        };
        self.copy_text_inner(range)
    }
}

/// Internal engine calls, returning plain `Result<_, String>`. The `#[wasm_bindgen]`
/// wrappers above convert the error to a thrown JS `Error` at the boundary — so
/// these run under `cargo test` on native targets, where constructing a `JsValue`
/// would panic ("cannot call wasm-bindgen imported functions on non-wasm targets").
impl WasmDocument {
    /// See [`WasmDocument::page_size`].
    fn page_size_inner(&self, index: u32) -> Result<PageSize, String> {
        let page = self.page(index)?;
        let size = self.page_box(page);
        Ok(PageSize {
            width_twip: size.width.raw(),
            height_twip: size.height.raw(),
        })
    }

    /// See [`WasmDocument::render_page`].
    fn render_page_inner(&self, index: u32, dpi: f32) -> Result<PageBitmap, String> {
        let page = self.page(index)?;
        let size = self.page_box(page);
        let width_px = size.width.to_device_px(dpi).ceil() as u32;
        let height_px = size.height.to_device_px(dpi).ceil() as u32;

        // The page background (`w:background`) fills the page behind everything,
        // matching the native renderer.
        let mut surface = match self.document.background() {
            Some(c) => Surface::with_background(width_px, height_px, [c.r, c.g, c.b]),
            None => Surface::new(width_px, height_px),
        }
        .map_err(|e| format!("allocate surface: {e:?}"))?;

        // Serve bundled (+ any host-registered) faces from the shaper's registry,
        // taken after pagination shaped every paragraph — so the renderer outlines
        // the same face layout measured.
        let registry = self.shaper.registry();
        let fonts = RegistryFontSource::new(&registry);
        render(&compose_page(page), &mut surface, dpi, &fonts, &self.media);

        Ok(PageBitmap {
            width_px,
            height_px,
            rgba: surface.data().to_vec(),
        })
    }

    /// Re-runs pagination against the current document and shaper. Called after a
    /// font registration so the new face participates in shaping + coverage. The
    /// page geometry (`default_config`) is font-independent and unchanged.
    fn repaginate(&mut self) {
        self.layout = paginate_document(&self.document, &self.shaper);
    }

    /// The page at `index`, or an out-of-range message.
    fn page(&self, index: u32) -> Result<&Page, String> {
        self.layout.pages.get(index as usize).ok_or_else(|| {
            format!(
                "page index {index} out of range (0..{})",
                self.layout.page_count()
            )
        })
    }

    /// The page box (width/height) for a page, from its own section geometry,
    /// falling back to the first-section config when the section id is unknown.
    fn page_box(&self, page: &Page) -> Size {
        self.document
            .definitions()
            .sections
            .iter()
            .find(|s| s.id == page.section)
            .map_or(self.default_config.page_size, |s| {
                Size::new(
                    Twip(s.page_size.width_twips),
                    Twip(s.page_size.height_twips),
                )
            })
    }

    /// Plain text for a model range: slice the start/end nodes' shaped text at the
    /// byte offsets and join the paragraphs the range spans with `\n`.
    fn copy_text_inner(&self, range: ModelRange) -> String {
        // Text-bearing nodes in document order — the order hit-testing traverses.
        let mut nodes: Vec<(NodeId, String)> = Vec::new();
        collect_block_text(self.document.body(), &mut nodes);

        let start = nodes.iter().position(|(id, _)| *id == range.start.node);
        let end = nodes.iter().position(|(id, _)| *id == range.end.node);
        let (Some(mut si), Some(mut ei)) = (start, end) else {
            return String::new();
        };
        let (mut so, mut eo) = (range.start.offset as usize, range.end.offset as usize);
        // Order the endpoints (a backward drag selects the same text).
        if si > ei || (si == ei && so > eo) {
            std::mem::swap(&mut si, &mut ei);
            std::mem::swap(&mut so, &mut eo);
        }

        if si == ei {
            return slice_bytes(&nodes[si].1, so, eo);
        }
        let mut parts = Vec::with_capacity(ei - si + 1);
        parts.push(slice_bytes(&nodes[si].1, so, nodes[si].1.len()));
        for (_, text) in &nodes[si + 1..ei] {
            parts.push(text.clone());
        }
        parts.push(slice_bytes(&nodes[ei].1, 0, eo));
        parts.join("\n")
    }
}

/// Collects every text-bearing node (paragraphs, including those inside tables and
/// block content controls) in document order, paired with its shaped plain text —
/// the byte space hit-testing addresses (doc 58 §3).
fn collect_block_text(blocks: &[BlockNode], out: &mut Vec<(NodeId, String)>) {
    for block in blocks {
        match block {
            BlockNode::Paragraph(p) => out.push((p.id, node_plain_text(&p.inlines))),
            BlockNode::Table(t) => {
                for row in &t.rows {
                    for cell in &row.cells {
                        collect_block_text(&cell.blocks, out);
                    }
                }
            }
            BlockNode::Sdt(sdt) => collect_block_text(&sdt.blocks, out),
            BlockNode::AltChunk(_) => {}
        }
    }
}

/// A model anchor `NodeId` + byte offset, or `None` if `node` is not a valid id.
fn parse_pos(node: &str, offset: u32) -> Option<ModelPos> {
    Some(ModelPos::new(NodeId::from_str(node).ok()?, offset))
}

/// A [`ModelRange`] from two string-encoded anchors, or `None` if either id is
/// malformed.
fn parse_range(
    start_node: &str,
    start_offset: u32,
    end_node: &str,
    end_offset: u32,
) -> Option<ModelRange> {
    Some(ModelRange::new(
        parse_pos(start_node, start_offset)?,
        parse_pos(end_node, end_offset)?,
    ))
}

/// A page-local rectangle flattened to `[page, x, y, w, h]` twips.
fn flat_rect(page: u32, rect: Rect) -> [i32; 5] {
    [
        page as i32,
        rect.origin.x.raw(),
        rect.origin.y.raw(),
        rect.size.width.raw(),
        rect.size.height.raw(),
    ]
}

/// `text[from..to]` clamped to the string's byte length and snapped to char
/// boundaries — never panics on a stale or off-boundary offset.
fn slice_bytes(text: &str, from: usize, to: usize) -> String {
    let clamp = |i: usize| {
        let mut i = i.min(text.len());
        while i < text.len() && !text.is_char_boundary(i) {
            i += 1;
        }
        i
    };
    let (a, b) = (clamp(from), clamp(to));
    if a >= b {
        String::new()
    } else {
        text[a..b].to_string()
    }
}

/// The model anchor a point resolved to (doc 58 §2 `TextCaret`/`Outside`): a
/// `NodeId` (32-hex string), a node-relative UTF-8 byte offset, and whether the
/// point was inside content or snapped in from outside.
#[wasm_bindgen]
#[derive(Clone, Debug)]
pub struct HitPayload {
    node: String,
    offset: u32,
    zone: &'static str,
}

#[wasm_bindgen]
impl HitPayload {
    /// The anchor node id (32-hex string; compare/pass back, never arithmetic).
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn node(&self) -> String {
        self.node.clone()
    }

    /// The node-relative UTF-8 byte offset.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn offset(&self) -> u32 {
        self.offset
    }

    /// `"content"` (inside a line) or `"outside"` (snapped from a margin).
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn zone(&self) -> String {
        self.zone.to_string()
    }
}

/// A page box size in twips (doc 57 §4.2).
#[wasm_bindgen(js_name = PageSize)]
#[derive(Clone, Copy, Debug)]
pub struct PageSize {
    width_twip: i32,
    height_twip: i32,
}

#[wasm_bindgen(js_class = PageSize)]
impl PageSize {
    /// Page box width in twips (1/1440 in).
    #[wasm_bindgen(getter, js_name = widthTwip)]
    #[must_use]
    pub fn width_twip(&self) -> i32 {
        self.width_twip
    }

    /// Page box height in twips (1/1440 in).
    #[wasm_bindgen(getter, js_name = heightTwip)]
    #[must_use]
    pub fn height_twip(&self) -> i32 {
        self.height_twip
    }
}

/// A rasterized page: RGBA8888 premultiplied pixels plus their device dimensions
/// (doc 57 §5.1). Blit with `new ImageData(rgba, widthPx, heightPx)`.
#[wasm_bindgen]
#[derive(Debug)]
pub struct PageBitmap {
    width_px: u32,
    height_px: u32,
    rgba: Vec<u8>,
}

#[wasm_bindgen]
impl PageBitmap {
    /// Bitmap width in device pixels.
    #[wasm_bindgen(getter, js_name = widthPx)]
    #[must_use]
    pub fn width_px(&self) -> u32 {
        self.width_px
    }

    /// Bitmap height in device pixels.
    #[wasm_bindgen(getter, js_name = heightPx)]
    #[must_use]
    pub fn height_px(&self) -> u32 {
        self.height_px
    }

    /// The premultiplied RGBA8888 pixels, row-major, as a `Uint8ClampedArray` —
    /// the exact backing an `ImageData` takes. Copies out of the surface.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn rgba(&self) -> Clamped<Vec<u8>> {
        Clamped(self.rgba.clone())
    }
}

/// Imports + paginates a `.docx` into a [`WasmDocument`], returning a plain
/// message on failure. Split from the `#[wasm_bindgen]` [`open`] so it runs
/// under native `cargo test`.
fn open_document(bytes: &[u8]) -> Result<WasmDocument, String> {
    let mut package =
        DocxPackage::open(bytes, viewer_limits()).map_err(|e| format!("open package: {e:?}"))?;
    let imported = import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .map_err(|e| format!("import document: {e:?}"))?;
    let document = imported.document;

    // Snapshot the inline-image bytes rendering will need, so the handle owns
    // everything and the package can be dropped.
    let mut media = MapMediaSource::new();
    for (_id, reference) in document.definitions().media.iter() {
        if let Ok(part_bytes) = package.read_part(&reference.part_name) {
            media.insert(reference.part_name.clone(), part_bytes);
        }
    }

    let shaper = ParleyShaper::new();
    // One call: per-section geometry, flowed headers/footers, anchored drawings,
    // and page-number fields — the same entry point the native renderer uses.
    let layout = paginate_document(&document, &shaper);
    let default_config = document_page_config(&document);

    Ok(WasmDocument {
        document,
        layout,
        shaper,
        media,
        default_config,
    })
}

/// Converts an internal error message to a thrown JS `Error`. Only ever runs at
/// the `#[wasm_bindgen]` boundary (never under native tests, where constructing a
/// `JsValue` would panic). Placeholder until the structured `SdkError` model
/// carries `code`/`severity` across the boundary (doc 57 §5.5).
fn to_js(message: String) -> JsValue {
    JsError::new(&message).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RICH_DOCX: &[u8] = include_bytes!("../../../fixtures/corpus/real-producer-rich.docx");

    /// The bridge paginates a corpus document to the **same** page count as the
    /// native pipeline — the façade adds no layout of its own (doc 57 §10,
    /// P1G-001 acceptance: "open → page_count matches native").
    #[test]
    fn page_count_matches_native() {
        let doc = open_document(RICH_DOCX).expect("open corpus docx");

        // Native reference: import + paginate directly, no bridge.
        let mut package = DocxPackage::open(RICH_DOCX, viewer_limits()).unwrap();
        let imported = import_package(
            &mut package,
            ImportConfig {
                mode: ImportMode::Semantic,
                ..ImportConfig::default()
            },
        )
        .unwrap();
        let shaper = ParleyShaper::new();
        let native = paginate_document(&imported.document, &shaper);

        assert!(
            native.page_count() > 0,
            "fixture should paginate to >=1 page"
        );
        assert_eq!(doc.page_count() as usize, native.page_count());
    }

    /// Every page reports a positive per-section page box, and rendering that page
    /// yields a device bitmap whose dimensions and RGBA length agree.
    #[test]
    fn render_page_dimensions_are_consistent() {
        let doc = open_document(RICH_DOCX).expect("open corpus docx");
        let dpi = 96.0;

        for i in 0..doc.page_count() {
            let size = doc.page_size(i).expect("page size");
            assert!(size.width_twip() > 0 && size.height_twip() > 0);

            let bitmap = doc.render_page(i, dpi).expect("render page");
            assert!(bitmap.width_px() > 0 && bitmap.height_px() > 0);
            // RGBA8888: four bytes per pixel, row-major.
            let expected = bitmap.width_px() as usize * bitmap.height_px() as usize * 4;
            assert_eq!(bitmap.rgba().0.len(), expected);
        }
    }

    /// A Latin corpus document has no missing coverage (the bundled faces cover
    /// it), and registering a fallback font runs the register → repaginate path
    /// without panicking and without disturbing a covered document's pagination.
    #[test]
    fn font_registration_repaginates_without_panic() {
        let mut doc = open_document(RICH_DOCX).expect("open corpus docx");
        assert!(
            doc.missing_coverage().is_empty(),
            "bundled faces cover this Latin document"
        );
        let before = doc.page_count();

        // A real face (bundled Roboto) wired as a Han fallback: exercises register
        // + repaginate. It adds no Han glyphs, so a Latin doc's pages are stable.
        doc.register_fallback_font(
            casual_doc_layout::fonts::ROBOTO_REGULAR,
            vec!["Hani".to_string()],
        );
        assert_eq!(doc.page_count(), before);
        assert!(doc.missing_coverage().is_empty());
    }

    /// An out-of-range page index is a clean error, not a panic. Exercises the
    /// inner methods: the `#[wasm_bindgen]` wrappers would construct a `JsValue`
    /// on the error path, which panics off-wasm.
    #[test]
    fn out_of_range_page_is_an_error() {
        let doc = open_document(RICH_DOCX).expect("open corpus docx");
        assert!(doc.page_size_inner(doc.page_count()).is_err());
        assert!(doc.render_page_inner(doc.page_count(), 96.0).is_err());
    }

    /// hit-test → caret/selection geometry → copy round-trips over a real
    /// paragraph: copying a node's full range reproduces its plain text, the
    /// geometry queries return well-formed rects, and a point inside the caret's
    /// line resolves back to the same node (the doc 58 pipeline, read-only end).
    #[test]
    fn selection_and_copy_roundtrip() {
        let doc = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(doc.document.body(), &mut nodes);
        let (node_id, text) = nodes
            .iter()
            .find(|(_, t)| !t.is_empty())
            .expect("a non-empty paragraph");
        let node = node_id.to_string();
        let end = text.len() as u32;

        // Copying the whole node reproduces its shaped plain text exactly.
        assert_eq!(doc.copy_text(&node, 0, &node, end), *text);
        // A backward range copies the same text (endpoints are ordered).
        assert_eq!(doc.copy_text(&node, end, &node, 0), *text);

        // Caret geometry: [page, x, y, w=0, h>0].
        let caret = doc.caret_rect(&node, 0);
        assert_eq!(caret.len(), 5, "one flat rect");
        assert_eq!(caret[3], 0, "caret is zero-width");
        assert!(caret[4] > 0, "caret has line height");

        // Selection geometry: at least one 5-tuple rect.
        let rects = doc.selection_rects(&node, 0, &node, end);
        assert!(!rects.is_empty() && rects.len().is_multiple_of(5));

        // A point inside the caret's line resolves back to the same node.
        let (page, x, y) = (caret[0] as u32, caret[1], caret[2]);
        let hit = doc
            .hit_test(page, x + 10, y + 10)
            .expect("a hit on content");
        assert_eq!(hit.node(), node);
        assert_eq!(hit.zone(), "content");

        // A malformed node id is an empty result, not a panic.
        assert!(doc.caret_rect("not-a-node", 0).is_empty());
        assert!(doc.copy_text("bad", 0, "bad", 1).is_empty());
    }

    /// Copy across two paragraphs joins them with a newline.
    #[test]
    fn copy_spans_paragraphs() {
        let doc = open_document(RICH_DOCX).expect("open corpus docx");
        let mut nodes = Vec::new();
        collect_block_text(doc.document.body(), &mut nodes);
        // Two *adjacent* text nodes, so exactly one newline joins them (no empty
        // paragraph in between contributing an extra break).
        let pair = nodes
            .windows(2)
            .find(|w| !w[0].1.is_empty() && !w[1].1.is_empty())
            .expect("two adjacent non-empty paragraphs");
        let (a_id, a_text) = &pair[0];
        let (b_id, b_text) = &pair[1];

        let copied = doc.copy_text(&a_id.to_string(), 0, &b_id.to_string(), b_text.len() as u32);
        assert_eq!(copied, format!("{a_text}\n{b_text}"));
    }
}
