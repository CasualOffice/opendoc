// SPDX-License-Identifier: Apache-2.0

//! The read-only engine cut behind the **Viewer SDK** and the headless
//! render/convert path (`docs/176`, ADR-071 proposed).
//!
//! One pipeline, request in and result out: **open → paginate once → render or
//! convert**. There is no caret, selection, hit-testing, incremental re-layout,
//! transaction log, undo, diff or collaboration here, by construction: this crate
//! does not depend on `casual-doc-edit`, `casual-doc-transaction`,
//! `casual-doc-selection`, `casual-doc-diff` or `casual-doc-wasm` (the `pdf`
//! feature is the one exception, and `docs/176` §4.3 says why and how it goes).
//!
//! The layout is computed exactly once, at [`ViewerDocument::open`], with the
//! same `paginate_document` / `compose_page` / `render` calls the editor and the
//! `opendoc-render` corpus tool make, so a page this crate rasterises is the page
//! the editor shows (`docs/98`: no second, "smarter" layout).
//!
//! ```no_run
//! # fn main() -> Result<(), casual_doc_viewer::ViewerError> {
//! let bytes = std::fs::read("report.docx").expect("read");
//! let doc = casual_doc_viewer::ViewerDocument::open(&bytes)?;
//! let png = doc.render_page_png(0, 96.0)?;
//! # let _ = png;
//! # Ok(())
//! # }
//! ```

use std::collections::BTreeMap;
use std::fmt;

use casual_doc_layout::compose::compose_page;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::flow::node_plain_text;
use casual_doc_layout::page::PaginatedLayout;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_model::v1::{BlockNode, Document};
use casual_doc_render::{MediaSource, RegistryFontSource, Surface, render};

#[cfg(feature = "wasm")]
pub mod web;

/// Why a viewer call failed. Messages are plain sentences a host can show.
#[derive(Clone, Debug, Eq, PartialEq)]
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
/// The default is the **deterministic** configuration: only the bundled faces
/// and the ones a host registers, never the machine's installed fonts. That is
/// what makes a page render to the same pixels on a laptop, a CI runner and a
/// server (`docs/176` §2.1), and it is the browser's configuration too.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct ViewerOptions {
    /// Let the shaper fall back to the operating system's installed faces
    /// (desktop fidelity for CJK and symbols, at the cost of pixels that depend
    /// on the machine). Off by default. Has no effect in a browser build.
    pub system_fonts: bool,
}

impl ViewerOptions {
    /// The options with OS font fallback switched on or off.
    #[must_use]
    pub fn with_system_fonts(mut self, on: bool) -> Self {
        self.system_fonts = on;
        self
    }
}

/// An opened, paginated, immutable document.
///
/// Every query is a read of the layout computed at open (or at the last
/// [`register_font`](Self::register_font)); nothing re-lays the document out.
pub struct ViewerDocument {
    document: Document,
    media: Media,
    shaper: ParleyShaper,
    layout: PaginatedLayout,
}

impl fmt::Debug for ViewerDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ViewerDocument")
            .field("pages", &self.layout.pages.len())
            .field("media", &self.media.0.len())
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
        let (document, media) = import(bytes)?;
        let shaper = if options.system_fonts {
            ParleyShaper::new()
        } else {
            ParleyShaper::without_system_fonts()
        };
        let layout = paginate_document(&document, &shaper);
        Ok(Self {
            document,
            media,
            shaper,
            layout,
        })
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
    #[must_use]
    pub fn page_size_twips(&self, index: usize) -> Option<(i32, i32)> {
        self.layout
            .pages
            .get(index)
            .map(|page| (page.page_size.width.0, page.page_size.height.0))
    }

    /// Registers a host-supplied face (a browser's fetched Roboto, a server's
    /// licensed Calibri) and re-paginates once, because a face moves line and
    /// page breaks. Call before rendering; it is the only call that re-lays out.
    ///
    /// **O(document)**.
    pub fn register_font(&mut self, bytes: Vec<u8>) {
        self.shaper.register_font(bytes);
        self.layout = paginate_document(&self.document, &self.shaper);
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
        let page = self
            .layout
            .pages
            .get(index)
            .ok_or(ViewerError::NoSuchPage {
                index,
                pages: self.layout.pages.len(),
            })?;
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

    /// The body's text, one line per paragraph, tables and content controls in
    /// reading order. For indexing and search; not a layout-faithful export.
    ///
    /// **O(document)**.
    #[must_use]
    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        append_blocks(self.document.body(), &mut out);
        out
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
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                out.push_str(&node_plain_text(&paragraph.inlines));
                out.push('\n');
            }
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        append_blocks(&cell.blocks, out);
                    }
                }
            }
            BlockNode::Sdt(sdt) => append_blocks(&sdt.blocks, out),
            BlockNode::AltChunk(_) => {}
        }
    }
}

/// DOCX only: the importer directly, with the package's pictures collected for
/// the renderer.
#[cfg(not(feature = "formats"))]
fn import(bytes: &[u8]) -> Result<(Document, Media), ViewerError> {
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
fn import(bytes: &[u8]) -> Result<(Document, Media), ViewerError> {
    use casual_doc_io::{
        DetectionRequest, FormatSelection, PlainTextLimits, builtin_registry_with_limits,
    };
    let registry = builtin_registry_with_limits(package_limits(), PlainTextLimits::default());
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u8> {
        std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../sample.docx"))
            .expect("the committed root sample.docx")
    }

    #[test]
    fn a_docx_opens_paginates_and_renders_every_page() {
        let doc = ViewerDocument::open(&sample()).expect("open");
        assert!(doc.page_count() >= 1);
        for index in 0..doc.page_count() {
            let (w, h) = doc.page_size_twips(index).expect("size");
            let bitmap = doc.render_page(index, 48.0).expect("render");
            assert_eq!(bitmap.width_px, (w as f32 * 48.0 / 1440.0).ceil() as u32);
            assert_eq!(bitmap.height_px, (h as f32 * 48.0 / 1440.0).ceil() as u32);
            assert_eq!(
                bitmap.rgba.len(),
                (bitmap.width_px * bitmap.height_px * 4) as usize
            );
            // A rendered page is not blank: some pixel is not the white page.
            assert!(
                bitmap
                    .rgba
                    .chunks_exact(4)
                    .any(|px| px[..3] != [255, 255, 255]),
                "page {index} rendered blank"
            );
        }
    }

    #[test]
    fn a_page_past_the_end_is_refused_with_the_count() {
        let doc = ViewerDocument::open(&sample()).expect("open");
        let pages = doc.page_count();
        assert_eq!(
            doc.render_page(pages, 48.0),
            Err(ViewerError::NoSuchPage {
                index: pages,
                pages
            })
        );
    }

    #[test]
    fn a_thumbnail_is_a_png() {
        let doc = ViewerDocument::open(&sample()).expect("open");
        let png = doc.render_page_png(0, 24.0).expect("png");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn plain_text_carries_the_body() {
        let doc = ViewerDocument::open(&sample()).expect("open");
        let text = doc.plain_text();
        assert!(text.lines().any(|line| !line.trim().is_empty()));
    }

    #[cfg(not(feature = "formats"))]
    #[test]
    fn bytes_that_are_not_a_docx_are_refused() {
        assert!(matches!(
            ViewerDocument::open(b"not a document"),
            Err(ViewerError::Open(_))
        ));
    }

    /// With every importer registered, unstructured bytes are a plain-text
    /// document, exactly as the editor opens them.
    #[cfg(feature = "formats")]
    #[test]
    fn bytes_that_are_not_a_package_open_as_plain_text() {
        let doc = ViewerDocument::open(b"not a document").expect("plain text");
        assert_eq!(doc.plain_text(), "not a document\n");
    }

    #[test]
    fn the_default_options_leave_os_fonts_off_and_render_repeatably() {
        assert!(!ViewerOptions::default().system_fonts);
        let a = ViewerDocument::open(&sample()).expect("open");
        let b = ViewerDocument::open(&sample()).expect("open");
        assert_eq!(a.render_page(0, 48.0), b.render_page(0, 48.0));
    }

    #[cfg(feature = "pdf")]
    #[test]
    fn the_pdf_has_one_page_per_laid_out_page() {
        let doc = ViewerDocument::open(&sample()).expect("open");
        let pdf = doc.to_pdf().expect("pdf");
        assert!(pdf.starts_with(b"%PDF-"));
        // Each page object is written as `<</Type/Page/Parent …` (lib.rs of
        // casual-doc-pdf); the page tree is `/Type/Pages`, which this skips.
        let marker = b"/Type/Page/Parent";
        let count = pdf.windows(marker.len()).filter(|w| w == marker).count();
        assert_eq!(count, doc.page_count());
    }
}
