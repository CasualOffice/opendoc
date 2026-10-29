// SPDX-License-Identifier: Apache-2.0

//! **OpenDoc Viewer SDK**: the stable read-only API (`docs/176`, ADR-071).
//!
//! One pipeline, request in and result out: **open → paginate once → read,
//! render or convert**. There is no caret, selection, hit-testing, incremental
//! re-layout, transaction log, undo, diff or collaboration here, by
//! construction: this crate does not depend on `casual-doc-edit`,
//! `casual-doc-transaction`, `casual-doc-selection`, `casual-doc-diff` or
//! `casual-doc-wasm` (the `pdf` feature is the one exception, and `docs/176`
//! §4.2 says why and how it goes).
//!
//! The layout is computed once, at [`ViewerDocument::open`], with the same
//! `paginate_document` / `compose_page` / `render` calls the editor and the
//! `opendoc-render` corpus tool make, so a page this crate rasterises is the page
//! the editor shows (`docs/98`: no second, "smarter" layout). The only calls
//! that lay the document out again are the font registrations, because a face
//! moves line and page breaks.
//!
//! # The surface
//!
//! | Call | What it returns |
//! | --- | --- |
//! | [`ViewerDocument::open`], [`open_with`](ViewerDocument::open_with) | A paginated document, or a sentence saying why not |
//! | [`page_count`](ViewerDocument::page_count), [`page_size_twips`](ViewerDocument::page_size_twips), [`page_sizes_twips`](ViewerDocument::page_sizes_twips) | Page geometry |
//! | [`render_page`](ViewerDocument::render_page), [`render_page_png`](ViewerDocument::render_page_png) | RGBA or PNG at a DPI |
//! | [`page_text`](ViewerDocument::page_text), [`plain_text`](ViewerDocument::plain_text) | Body text per page and whole |
//! | [`outline`](ViewerDocument::outline) | Headings with level and page |
//! | [`font_needs`](ViewerDocument::font_needs), [`missing_code_points`](ViewerDocument::missing_code_points) | What a host could supply |
//! | [`register_fonts`](ViewerDocument::register_fonts), [`register_fallback_font`](ViewerDocument::register_fallback_font) | The host font hook |
//! | `to_pdf` (feature `pdf`) | Real-text vector PDF of the same layout |
//!
//! Page-to-SVG is deliberately absent: the engine has no SVG backend, so it is
//! not "cheap" (`docs/176` §11).
//!
//! ```no_run
//! # fn main() -> Result<(), opendoc_viewer::ViewerError> {
//! let bytes = std::fs::read("report.docx").expect("read");
//! let doc = opendoc_viewer::ViewerDocument::open(&bytes)?;
//! let png = doc.render_page_png(0, 96.0)?;
//! # let _ = png;
//! # Ok(())
//! # }
//! ```

use std::cell::OnceCell;
use std::collections::{BTreeMap, HashMap};
use std::fmt;

use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::cascade::StyleCascade;
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::flow::node_plain_text;
use casual_doc_layout::font_substitution::{DeclaredFamilies, substitute};
use casual_doc_layout::page::PaginatedLayout;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::text::LineBreak;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{BlockNode, Document};
use casual_doc_render::{MediaSource, RegistryFontSource, Surface, render};

#[cfg(feature = "wasm")]
pub mod web;

/// Why a viewer call failed. Messages are plain sentences a host can show.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ViewerError {
    /// The bytes could not be opened as a document.
    Open(String),
    /// `index` is not a page of this document.
    NoSuchPage {
        /// The 0-based index asked for.
        index: usize,
        /// The document's page count.
        pages: usize,
    },
    /// The raster surface could not be allocated (zero or oversized).
    Surface(String),
    /// Encoding the result (PNG, PDF) failed.
    Encode(String),
}

impl fmt::Display for ViewerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open(reason) => write!(f, "open document: {reason}"),
            Self::NoSuchPage { index, pages } => {
                write!(f, "page {index} does not exist; the document has {pages}")
            }
            Self::Surface(reason) => write!(f, "allocate page surface: {reason}"),
            Self::Encode(reason) => write!(f, "encode output: {reason}"),
        }
    }
}

impl std::error::Error for ViewerError {}

/// A rasterised page: premultiplied RGBA8888, row-major, `width_px * 4` per row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageBitmap {
    /// Width in device pixels.
    pub width_px: u32,
    /// Height in device pixels.
    pub height_px: u32,
    /// `width_px * height_px * 4` bytes.
    pub rgba: Vec<u8>,
}

/// One heading of the document outline.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct OutlineEntry {
    /// 1-based heading level (1 = top): an effective `w:outlineLvl`, or a
    /// `Title` / `Heading N` style, the same rule the editor's Outline uses.
    pub level: u8,
    /// The heading's text, trimmed. Headings with no text are left out.
    pub text: String,
    /// The 0-based page the heading starts on, or `None` when it is not laid
    /// out (a heading inside a part the body flow does not place).
    pub page: Option<usize>,
}

/// A font the document asked for and the layout did not have.
///
/// The host answers it by registering a face: either the requested family
/// itself (a licensed Calibri, say) or the named metric-compatible
/// [`substitute`](Self::substitute), which the npm package ships as files the
/// host serves itself (`docs/176` §10, Q2: no CDN fetch by default).
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct FontNeed {
    /// The family name the document requested (`w:rFonts`).
    pub requested: String,
    /// The family the engine substitutes for it (`Carlito` for Calibri,
    /// `Liberation Serif` for Times New Roman, …).
    pub substitute: Option<&'static str>,
    /// Whether the substitute is available to the layout right now (embedded
    /// in this build, or registered by the host). When it is, the runs already
    /// shape with metric-compatible advances, and supplying the requested face
    /// itself is an upgrade, not a repair.
    pub substitute_available: bool,
}

/// Package part name → encoded image bytes, served to both backends.
#[derive(Debug, Default)]
struct Media(BTreeMap<String, Vec<u8>>);

impl MediaSource for Media {
    fn media_bytes(&self, media: &str) -> Option<&[u8]> {
        self.0.get(media).map(Vec::as_slice)
    }
}

#[cfg(feature = "pdf")]
impl casual_doc_pdf::PdfMediaSource for Media {
    fn media_bytes(&self, media: &str) -> Option<&[u8]> {
        self.0.get(media).map(Vec::as_slice)
    }
}

/// How a document is opened.
///
/// The default is the **deterministic** configuration: only the faces this build
/// embeds and the ones a host registers, never the machine's installed fonts.
/// That is what makes a page render to the same pixels on a laptop, a CI runner
/// and a server (`docs/176` §2.1), and it is the browser's configuration too.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct ViewerOptions {
    /// Let the shaper fall back to the operating system's installed faces
    /// (desktop fidelity for CJK and symbols, at the cost of pixels that depend
    /// on the machine). Off by default. Has no effect in a browser build.
    pub system_fonts: bool,
    /// Faces registered before the first layout, so a host that already knows
    /// its fonts pays for one pagination instead of two.
    pub fonts: Vec<Vec<u8>>,
    /// OCR for scanned PDF pages (`docs/182` §20a); `None` (the default)
    /// runs none and a scan opens as its picture, reported
    /// `pdf.import.ocr_unavailable`.
    #[cfg(feature = "pdf-import")]
    pub pdf_ocr: Option<PdfOcr>,
}

/// The host's PDF OCR configuration, shared: two options are equal when
/// they hold the same configuration (the same `Arc`), since a provider has
/// no value equality.
#[cfg(feature = "pdf-import")]
#[derive(Clone, Debug)]
pub struct PdfOcr(pub std::sync::Arc<casual_doc_io::PdfOcrOptions>);

#[cfg(feature = "pdf-import")]
impl PartialEq for PdfOcr {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}

#[cfg(feature = "pdf-import")]
impl Eq for PdfOcr {}

impl ViewerOptions {
    /// The options with OS font fallback switched on or off.
    #[must_use]
    pub fn with_system_fonts(mut self, on: bool) -> Self {
        self.system_fonts = on;
        self
    }

    /// The options with one more face registered before the first layout.
    #[must_use]
    pub fn with_font(mut self, bytes: Vec<u8>) -> Self {
        self.fonts.push(bytes);
        self
    }

    /// The options with the host's OCR for scanned PDF pages: build `ocr`
    /// with `PdfOcrOptions::from_models` over the host's models, or with
    /// the host's own provider.
    #[cfg(feature = "pdf-import")]
    #[must_use]
    pub fn with_pdf_ocr(mut self, ocr: casual_doc_io::PdfOcrOptions) -> Self {
        self.pdf_ocr = Some(PdfOcr(std::sync::Arc::new(ocr)));
        self
    }
}

/// An opened, paginated, immutable document.
///
/// Every query is a read of the layout computed at open (or at the last font
/// registration); nothing else re-lays the document out.
pub struct ViewerDocument {
    document: Document,
    media: Media,
    shaper: ParleyShaper,
    layout: PaginatedLayout,
    font_needs: Vec<FontNeed>,
    missing: Vec<char>,
    /// Built on the first per-page text request and kept until the next
    /// layout: one O(document) walk instead of one per page.
    text_index: OnceCell<TextIndex>,
}

impl fmt::Debug for ViewerDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ViewerDocument")
            .field("pages", &self.layout.pages.len())
            .field("media", &self.media.0.len())
            .field("font_needs", &self.font_needs)
            .finish_non_exhaustive()
    }
}

/// The input ceilings, the same envelope the editor admits (`casual-doc-wasm`'s
/// `viewer_limits`): a large DOCX is large because of its pictures.
fn package_limits() -> casual_doc_ooxml::PackageLimits {
    casual_doc_ooxml::PackageLimits {
        max_input_bytes: 200 * 1024 * 1024,
        max_total_expanded_bytes: 512 * 1024 * 1024,
        max_single_expanded_bytes: 200 * 1024 * 1024,
        ..casual_doc_ooxml::PackageLimits::default()
    }
}

impl ViewerDocument {
    /// Opens `bytes` and paginates once.
    ///
    /// Without the `formats` feature only DOCX is accepted, through
    /// `casual-doc-import` directly; with it, every format the editor opens is
    /// detected through the `casual-doc-io` registry.
    ///
    /// **O(document)**: one import walk and one pagination.
    ///
    /// # Errors
    ///
    /// [`ViewerError::Open`] when the bytes are not an admissible document.
    pub fn open(bytes: &[u8]) -> Result<Self, ViewerError> {
        Self::open_with(bytes, &ViewerOptions::default())
    }

    /// [`open`](Self::open) with explicit [`ViewerOptions`].
    ///
    /// # Errors
    ///
    /// As [`open`](Self::open).
    pub fn open_with(bytes: &[u8], options: &ViewerOptions) -> Result<Self, ViewerError> {
        let (document, media) = import(bytes, options)?;
        Ok(Self::from_document(document, media.0, options))
    }

    /// Paginates a document the caller already holds (one edited after it was
    /// opened, say), with its media parts keyed by part name, the shape of
    /// `casual_doc_io::DocumentResources`. This is how a headless editing session
    /// (`casual-doc-ai`, `docs/178`) renders the state it has rather than the
    /// bytes it started from.
    ///
    /// **O(document)**: one pagination.
    #[must_use]
    pub fn from_document(
        document: Document,
        media: BTreeMap<String, Vec<u8>>,
        options: &ViewerOptions,
    ) -> Self {
        let media = Media(media);
        let shaper = if options.system_fonts {
            ParleyShaper::new()
        } else {
            ParleyShaper::without_system_fonts()
        };
        for face in &options.fonts {
            shaper.register_font(face.clone());
        }
        // The document's own faces (DOCX `.odttf`, and the font programs a PDF
        // import carries, `docs/182` G-1) outrank substitutes for their family.
        // A face that fails to register keeps its substitute.
        drop(casual_doc_layout::font_registry::register_embedded_fonts(
            &shaper,
            &document,
            |part| media.0.get(part).map(Vec::as_slice),
        ));
        let mut doc = Self {
            document,
            media,
            shaper,
            layout: PaginatedLayout { pages: Vec::new() },
            font_needs: Vec::new(),
            missing: Vec::new(),
            text_index: OnceCell::new(),
        };
        doc.relayout();
        doc
    }

    /// Paginates from scratch and re-reads what the pass could not supply.
    /// **O(document)**.
    fn relayout(&mut self) {
        self.text_index = OnceCell::new();
        let registry = self.shaper.registry();
        registry.clear_missing_coverage();
        drop(self.shaper.take_unresolved_families());
        self.layout = paginate_document(&self.document, &self.shaper);
        let declared = DeclaredFamilies::from_font_table(&self.document.definitions().font_table);
        self.font_needs = self
            .shaper
            .take_unresolved_families()
            .into_iter()
            .map(|requested| {
                let substitute =
                    substitute(&requested, declared.kind_of(&requested)).map(|sub| sub.family.name);
                FontNeed {
                    substitute_available: substitute
                        .is_some_and(|family| self.shaper.has_family(family)),
                    requested,
                    substitute,
                }
            })
            .collect();
        self.missing = registry.missing_coverage();
    }

    /// The normalized document, for read access to its structure.
    #[must_use]
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// The number of laid-out pages.
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.layout.pages.len()
    }

    /// Page `index`'s size in twips (`(width, height)`), or `None` past the end.
    /// Sections may change paper size or orientation, so pages can differ.
    #[must_use]
    pub fn page_size_twips(&self, index: usize) -> Option<(i32, i32)> {
        self.layout
            .pages
            .get(index)
            .map(|page| (page.page_size.width.0, page.page_size.height.0))
    }

    /// Every page's size in twips, in page order. **O(pages)**.
    #[must_use]
    pub fn page_sizes_twips(&self) -> Vec<(i32, i32)> {
        self.layout
            .pages
            .iter()
            .map(|page| (page.page_size.width.0, page.page_size.height.0))
            .collect()
    }

    /// The fonts the last layout asked for and did not have, sorted by name.
    #[must_use]
    pub fn font_needs(&self) -> &[FontNeed] {
        &self.font_needs
    }

    /// Code points no registered face could draw in the last layout (they
    /// painted as a box), or that only the monochrome emoji base drew. A host
    /// answers them with [`register_fallback_font`](Self::register_fallback_font).
    #[must_use]
    pub fn missing_code_points(&self) -> &[char] {
        &self.missing
    }

    /// Registers one host-supplied face and re-paginates.
    /// [`register_fonts`](Self::register_fonts) does several for one layout.
    ///
    /// **O(document)**.
    pub fn register_font(&mut self, bytes: Vec<u8>) {
        self.register_fonts([bytes]);
    }

    /// Registers host-supplied faces (a browser's fetched Carlito, a server's
    /// licensed Calibri) under the family names in their `name` tables, and
    /// re-paginates **once**, because a face moves line and page breaks.
    /// Returns the number of faces registered (a `.ttc` adds several; bytes
    /// that are not a font add none).
    ///
    /// **O(document)**.
    pub fn register_fonts(&mut self, faces: impl IntoIterator<Item = Vec<u8>>) -> usize {
        let registered = faces
            .into_iter()
            .map(|bytes| self.shaper.register_font(bytes).len())
            .sum();
        if registered > 0 {
            self.relayout();
        }
        registered
    }

    /// Registers a face as the coverage fallback for `scripts` (ISO 15924
    /// codes such as `Hani`, `Hira`, `Kana`, `Hang`, `Arab`, or `Zyyy` for
    /// emoji and symbols) and re-paginates once. Returns the number of faces
    /// registered.
    ///
    /// **O(document)**.
    pub fn register_fallback_font(&mut self, bytes: Vec<u8>, scripts: &[&str]) -> usize {
        let registered = self.shaper.register_fallback_font(bytes, scripts).len();
        if registered > 0 {
            self.relayout();
        }
        registered
    }

    /// Rasterises page `index` (0-based) at `dpi`.
    ///
    /// **O(page)**: one compose and one raster of that page alone.
    ///
    /// # Errors
    ///
    /// [`ViewerError::NoSuchPage`] past the end; [`ViewerError::Surface`] when
    /// the size at `dpi` cannot be allocated.
    pub fn render_page(&self, index: usize, dpi: f32) -> Result<PageBitmap, ViewerError> {
        let (surface, width_px, height_px) = self.render_surface(index, dpi)?;
        Ok(PageBitmap {
            width_px,
            height_px,
            rgba: surface.data().to_vec(),
        })
    }

    /// [`render_page`](Self::render_page), encoded as PNG — the thumbnail call.
    ///
    /// # Errors
    ///
    /// As [`render_page`](Self::render_page), plus [`ViewerError::Encode`].
    pub fn render_page_png(&self, index: usize, dpi: f32) -> Result<Vec<u8>, ViewerError> {
        self.render_surface(index, dpi)?
            .0
            .encode_png()
            .map_err(|error| ViewerError::Encode(format!("{error:?}")))
    }

    /// The rendered surface and its pixel size. **O(page)**.
    fn render_surface(&self, index: usize, dpi: f32) -> Result<(Surface, u32, u32), ViewerError> {
        let page = self.page(index)?;
        if !(dpi.is_finite() && dpi > 0.0) {
            return Err(ViewerError::Surface(format!(
                "{dpi} dpi is not a positive resolution"
            )));
        }
        // Rounded up so a fractional page edge is painted, never cropped; at
        // least one pixel so a degenerate page size is refused by the surface,
        // not by an overflow here.
        let width = page.page_size.width.to_device_px(dpi).ceil().max(1.0) as u32;
        let height = page.page_size.height.to_device_px(dpi).ceil().max(1.0) as u32;
        let mut surface = match self.document.background() {
            Some(c) => Surface::with_background(width, height, [c.r, c.g, c.b]),
            None => Surface::new(width, height),
        }
        .map_err(|error| ViewerError::Surface(format!("{width}x{height}: {error:?}")))?;
        // The registry snapshot AFTER pagination, so the raster outlines the
        // faces the layout measured with.
        let registry = self.shaper.registry();
        let fonts = RegistryFontSource::new(&registry);
        render(&compose_page(page), &mut surface, dpi, &fonts, &self.media);
        Ok((surface, width, height))
    }

    /// Page `index`, or [`ViewerError::NoSuchPage`].
    fn page(&self, index: usize) -> Result<&casual_doc_layout::page::Page, ViewerError> {
        self.layout.pages.get(index).ok_or(ViewerError::NoSuchPage {
            index,
            pages: self.layout.pages.len(),
        })
    }

    /// The body's text, one line per paragraph, tables and content controls in
    /// reading order. For indexing and search; not a layout-faithful export.
    /// Headers, footers and notes are not included.
    ///
    /// **O(document)**.
    #[must_use]
    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        append_blocks(self.document.body(), &mut out);
        out
    }

    /// The body text laid out on page `index`, in flow order: the lines the
    /// page carries, with a newline where a paragraph ends. A paragraph split
    /// across two pages contributes its first lines here and the rest to the
    /// next page, with no newline at the split, so the pages' texts
    /// concatenated read as [`plain_text`](Self::plain_text) (except for a
    /// repeated table header row, which is read on every page that prints it).
    /// Headers, footers and notes are not included.
    ///
    /// **O(page)**, after one O(document) index on the first call.
    ///
    /// # Errors
    ///
    /// [`ViewerError::NoSuchPage`] past the end.
    pub fn page_text(&self, index: usize) -> Result<String, ViewerError> {
        let page = self.page(index)?;
        let index = self
            .text_index
            .get_or_init(|| TextIndex::build(&self.document, &self.layout));
        let mut out = String::new();
        for placed in &page.placed {
            append_fragment_text(&placed.fragment, index, &mut out);
        }
        Ok(out)
    }

    /// The page label printed on the page each top-level node is first laid
    /// out on, keyed by node: what a table of contents or a page
    /// cross-reference prints for it. The editor's table of contents reads the
    /// same function (`casual_doc_layout::paginate::first_page_labels`) over
    /// its own layout, so a headless session and the editor agree.
    ///
    /// **O(placed fragments + pages).**
    #[must_use]
    pub fn page_labels(&self) -> HashMap<NodeId, String> {
        casual_doc_layout::paginate::first_page_labels(
            &self.layout,
            &self.document.definitions().sections,
        )
    }

    /// The headings of the body, in document order, with the page each starts
    /// on.
    ///
    /// **O(document + layout)**: one walk of the body with the style cascade
    /// built once, and one walk of the placed fragments for the pages.
    #[must_use]
    pub fn outline(&self) -> Vec<OutlineEntry> {
        let mut first_page: HashMap<NodeId, usize> = HashMap::new();
        for (index, page) in self.layout.pages.iter().enumerate() {
            for placed in &page.placed {
                note_first_page(&placed.fragment, index, &mut first_page);
            }
        }
        let definitions = self.document.definitions();
        let cascade = StyleCascade::new(definitions);
        let mut entries = Vec::new();
        visit_body_paragraphs(self.document.body(), &mut |paragraph| {
            let Some(level) = casual_doc_layout::fold::heading_level(
                paragraph.properties.get(),
                definitions,
                &cascade,
            ) else {
                return;
            };
            let text = node_plain_text(&paragraph.inlines);
            let text = text.trim();
            if !text.is_empty() {
                entries.push(OutlineEntry {
                    level,
                    text: text.to_owned(),
                    page: first_page.get(&paragraph.id).copied(),
                });
            }
        });
        entries
    }

    /// The whole document as real-text vector PDF, from the layout already
    /// computed (no second pagination).
    ///
    /// **O(document)**.
    ///
    /// # Errors
    ///
    /// [`ViewerError::Encode`] past the PDF page ceiling or when a face cannot
    /// be embedded.
    #[cfg(feature = "pdf")]
    pub fn to_pdf(&self) -> Result<Vec<u8>, ViewerError> {
        use casual_doc_pdf::{DocumentText, PdfExportOptions, PdfPage, write_pdf_with_text};
        let lists: Vec<_> = self
            .layout
            .pages
            .iter()
            .map(|page| (page.page_size, compose_page(page)))
            .collect();
        let pages: Vec<PdfPage<'_>> = lists
            .iter()
            .map(|(size, list)| PdfPage {
                width: size.width,
                height: size.height,
                list,
            })
            .collect();
        let fonts = casual_doc_pdf::RegistryFontSource::new(&self.shaper.registry());
        let text = DocumentText::new(&self.document);
        write_pdf_with_text(
            &pages,
            &fonts,
            &self.media,
            &PdfExportOptions::default(),
            &text,
        )
        .map(|export| export.bytes)
        .map_err(|error| ViewerError::Encode(format!("pdf: {error}")))
    }
}

/// Appends each paragraph's text and a newline, descending into tables and
/// block content controls. **O(blocks)**.
fn append_blocks(blocks: &[BlockNode], out: &mut String) {
    visit_body_paragraphs(blocks, &mut |paragraph| {
        out.push_str(&node_plain_text(&paragraph.inlines));
        out.push('\n');
        // A text box's content is the paragraph's, read after it: a floating
        // frame (a PDF-origin page is nothing else, `docs/182`) or an inline
        // one. Without this a fixed-layout document read as empty.
        for inline in &paragraph.inlines {
            if let casual_doc_model::v1::InlineNode::TextBox(text_box) = inline {
                append_blocks(&text_box.blocks, out);
            }
        }
    });
}

/// Calls `visit` for every paragraph in `blocks`, in reading order, descending
/// into table cells and block content controls. **O(blocks)**.
fn visit_body_paragraphs<'a>(
    blocks: &'a [BlockNode],
    visit: &mut impl FnMut(&'a casual_doc_model::v1::Paragraph),
) {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => visit(paragraph),
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        visit_body_paragraphs(&cell.blocks, visit);
                    }
                }
            }
            BlockNode::Sdt(sdt) => visit_body_paragraphs(&sdt.blocks, visit),
            BlockNode::AltChunk(_) => {}
        }
    }
}

/// What per-page text needs from the whole document, built once per layout.
#[derive(Debug, Default)]
struct TextIndex {
    /// Paragraph id → its text, in the byte space line ranges index.
    texts: HashMap<NodeId, String>,
    /// Paragraph id → the start offset of each of its laid-out lines, in
    /// layout order across every page. A page whose share of a paragraph ends
    /// mid-paragraph reads up to the next line's start, so the characters no
    /// line covers (the `w:br` of a page break) are read exactly once.
    line_starts: HashMap<NodeId, Vec<usize>>,
}

impl TextIndex {
    /// **O(document + layout)**.
    fn build(document: &Document, layout: &PaginatedLayout) -> Self {
        let mut index = Self::default();
        visit_body_paragraphs(document.body(), &mut |paragraph| {
            index
                .texts
                .insert(paragraph.id, node_plain_text(&paragraph.inlines));
        });
        for page in &layout.pages {
            for placed in &page.placed {
                index.note_line_starts(&placed.fragment);
            }
        }
        index
    }

    fn note_line_starts(&mut self, fragment: &BlockFragment) {
        match fragment {
            BlockFragment::Paragraph { id, lines, .. } => {
                let starts = self.line_starts.entry(*id).or_default();
                for line in &lines.lines {
                    if line.range.start.node == *id {
                        starts.push(line.range.start.offset as usize);
                    }
                }
            }
            BlockFragment::TableRow { cells, .. } => {
                for cell in cells {
                    for block in &cell.blocks {
                        self.note_line_starts(block);
                    }
                }
            }
        }
    }

    /// The first line start of `id` after `offset`, if a later line exists.
    fn next_line_start(&self, id: NodeId, offset: usize) -> Option<usize> {
        let starts = self.line_starts.get(&id)?;
        let at = starts.partition_point(|start| *start <= offset);
        starts.get(at).copied()
    }
}

/// Appends the text a placed fragment's lines cover. **O(lines in it)**.
fn append_fragment_text(fragment: &BlockFragment, index: &TextIndex, out: &mut String) {
    match fragment {
        BlockFragment::Paragraph { id, lines, .. } => {
            let Some(text) = index.texts.get(id) else {
                return;
            };
            // Lines on this page, in this paragraph's own byte space. A gap
            // between two lines (the `w:br` a hard break leaves out of both
            // ranges) is read too, so the text is the paragraph's and not only
            // its glyphs.
            let own = |line: &&casual_doc_layout::text::Line| {
                line.range.start.node == *id && line.range.end.node == *id
            };
            let mut cursor: Option<usize> = None;
            let mut last = None;
            for line in lines.lines.iter().filter(own) {
                let start = line.range.start.offset as usize;
                let end = line.range.end.offset as usize;
                let from = cursor.map_or(start, |at| at.min(start));
                if let Some(slice) = text.get(from..end.max(from)) {
                    out.push_str(slice);
                }
                cursor = Some(end.max(from));
                last = Some(line);
            }
            // The paragraph ends on this page when its last line here is its
            // last line at all; a page break that ends a paragraph leaves the
            // line marked `Page`, so the byte offset decides, not the marker.
            let done = last.is_none_or(|line| {
                line.line_break == LineBreak::ParagraphEnd
                    || line.range.end.offset as usize >= text.len()
            });
            if done {
                if let Some(rest) = cursor.and_then(|at| text.get(at..)) {
                    out.push_str(rest);
                }
                out.push('\n');
            } else if let Some(at) = cursor
                && let Some(gap) = index
                    .next_line_start(*id, at)
                    .and_then(|next| text.get(at..next))
            {
                out.push_str(gap);
            }
        }
        // A header row Word repeats on a continuation page is read again on
        // that page, because it is printed there.
        BlockFragment::TableRow { cells, .. } => {
            for cell in cells {
                for block in &cell.blocks {
                    append_fragment_text(block, index, out);
                }
            }
        }
    }
}

/// Records the first page each paragraph shows text on: a heading that opens
/// with a page break starts where its words are, not on the page the break
/// leaves. **O(fragments)**.
fn note_first_page(fragment: &BlockFragment, page: usize, first: &mut HashMap<NodeId, usize>) {
    match fragment {
        BlockFragment::Paragraph { id, lines, .. } => {
            let has_text = lines
                .lines
                .iter()
                .any(|line| line.range.end.offset > line.range.start.offset);
            if has_text {
                first.entry(*id).or_insert(page);
            }
        }
        BlockFragment::TableRow { cells, .. } => {
            for cell in cells {
                for block in &cell.blocks {
                    note_first_page(block, page, first);
                }
            }
        }
    }
}

/// DOCX only: the importer directly, with the package's pictures collected for
/// the renderer.
#[cfg(not(feature = "formats"))]
fn import(bytes: &[u8], _options: &ViewerOptions) -> Result<(Document, Media), ViewerError> {
    use casual_doc_import::{ImportConfig, ImportMode, import_package};
    use casual_doc_ooxml::DocxPackage;

    let mut package =
        DocxPackage::open(bytes, package_limits()).map_err(|e| ViewerError::Open(e.to_string()))?;
    let imported = import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .map_err(|e| ViewerError::Open(e.to_string()))?;
    let document = imported.document;
    let mut media = Media::default();
    for (_id, reference) in document.definitions().media.iter() {
        if let Ok(part) = package.read_part(&reference.part_name) {
            media.0.insert(reference.part_name.clone(), part);
        }
    }
    Ok((document, media))
}

/// Every format the editor opens, detected through the io registry.
#[cfg(feature = "formats")]
#[cfg_attr(not(feature = "pdf-import"), allow(unused_variables))]
fn import(bytes: &[u8], options: &ViewerOptions) -> Result<(Document, Media), ViewerError> {
    use casual_doc_io::{
        DetectionRequest, FormatSelection, PlainTextLimits, builtin_registry_with_limits,
    };
    #[allow(unused_mut)] // mutated only with `pdf-import`
    let mut registry = builtin_registry_with_limits(package_limits(), PlainTextLimits::default());
    // PDF opens only when this build asked for it (`docs/182` P1).
    #[cfg(feature = "pdf-import")]
    casual_doc_io::enable_pdf_import_with_ocr(
        &mut registry,
        casual_doc_io::PdfReadLimits::default(),
        options
            .pdf_ocr
            .as_ref()
            .map(|ocr| (*ocr.0).clone())
            .unwrap_or_default(),
    )
    .map_err(|e| ViewerError::Open(e.to_string()))?;
    let imported = registry
        .import(
            DetectionRequest {
                bytes,
                selection: FormatSelection::Auto,
                file_name_hint: None,
                mime_hint: None,
            },
            false,
        )
        .map_err(|e| ViewerError::Open(e.to_string()))?;
    Ok((
        imported.document,
        Media(imported.resources.as_map().clone()),
    ))
}
