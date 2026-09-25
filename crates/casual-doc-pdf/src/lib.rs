//! Real-text PDF export for OpenDoc.
//!
//! This crate turns the shared display list into a **vector** PDF: text is
//! drawn as text with embedded subset fonts, rules and borders are paths, and
//! a picture is embedded once as an image XObject rather than flattened into a
//! page bitmap. The output is selectable, searchable, copyable and a fraction
//! of the size of the 150-DPI raster it replaces (`docs/104` HF-030,
//! `docs/105` OO-010, `docs/98`).
//!
//! # The parity rule
//!
//! The exporter performs **no layout**. It consumes the same
//! [`DisplayList`](casual_doc_layout::display::DisplayList) the raster backend
//! consumes, produced by the same pagination pass, and transcribes each
//! [`PaintItem`](casual_doc_layout::display::PaintItem) into content-stream
//! operators. It embeds the exact face the shaper resolved and emits the
//! shaper's glyph ids at the shaper's advances. Editor↔PDF drift is therefore
//! structural rather than a matter of care: the exporter is not permitted to be
//! "smarter" than the editor.
//!
//! # What it does not do
//!
//! Enumerated rather than implied, because absence from a support list is an
//! overstatement by omission:
//!
//! - **Tagged PDF** (`/StructTreeRoot`, marked content, reading order, alt
//!   text, `/Lang`) — a separate roadmap row; nothing here forecloses it.
//! - **Hyperlink, outline/bookmark and destination annotations** — these need a
//!   semantic side-channel from layout (`docs/98` §6) that does not exist yet.
//!   A hyperlink's text is exported; its clickable annotation is not.
//! - **PDF/A**, encryption, comment/markup export, page ranges.
//! - **Emphasis marks** and SVG pictures, neither of which the raster backend
//!   paints either.
//! - **Ligature and substituted-glyph text recovery.** The `ToUnicode` map is
//!   built by inverting the face's character map, so a glyph reached only
//!   through substitution copies as nothing; the count is reported as
//!   `pdf.font.unmapped_glyphs`.
//!
//! # Example
//!
//! ```no_run
//! use casual_doc_pdf::{BundledFontSource, NoMediaSource, PdfExportOptions, export_document};
//! # fn demo(document: &casual_doc_model::v1::Document) {
//! let export = export_document(document, &NoMediaSource, &PdfExportOptions::default())
//!     .expect("export");
//! assert_eq!(&export.bytes[..5], b"%PDF-");
//! # let _ = BundledFontSource;
//! # }
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod content;
mod font;
pub mod inspect;
mod picture;
mod subset;
mod writer;

use std::collections::BTreeMap;
use std::fmt;

use casual_doc_layout::compose::compose_page;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::font_registry::DynFace;
use casual_doc_layout::font_registry::FontRegistry;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_layout::text::FontId;
use casual_doc_model::v1::Document;

pub use content::PdfPage;
pub use font::EmbeddedFace;
pub use font::PdfFontSource;
pub use picture::MapMediaSource;
pub use picture::NoMediaSource;
pub use picture::PdfMediaSource;

use content::Transcriber;
// Own line (anti-conflict): the multiply `ExtGState`'s name, shared with the
// content stream that selects it so the two cannot disagree.
use content::MULTIPLY_GS_NAME;
use font::FontError;
use writer::Writer;
use writer::text_string;

/// The most pages one export will write.
///
/// A document that paginates past this is refused rather than silently
/// truncated: a PDF that quietly stops at page 5000 is worse than an error.
pub const MAX_EXPORT_PAGES: usize = 20_000;

/// Options for one export.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PdfExportOptions {
    /// Whether to write the document's own metadata into the `/Info`
    /// dictionary. The dates come from the document, never from the clock, so
    /// the export stays byte-reproducible either way.
    pub include_metadata: bool,
    /// The value written as `/Producer`. Defaults to the crate's own name.
    pub producer: Option<String>,
}

impl PdfExportOptions {
    /// Options that write document metadata into `/Info`.
    #[must_use]
    pub fn with_metadata() -> Self {
        Self {
            include_metadata: true,
            producer: None,
        }
    }
}

/// How much a finding matters.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum PdfSeverity {
    /// The output is complete; the note records how something was achieved.
    Note,
    /// Something in the document is not represented in the PDF.
    Degraded,
}

/// One thing the caller must be told about the export.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PdfFinding {
    /// A stable machine-readable code, for example `pdf.image.unresolved`.
    pub code: String,
    /// How many times it occurred.
    pub occurrences: usize,
    /// How much it matters.
    pub severity: PdfSeverity,
    /// A human-readable explanation.
    pub detail: String,
}

/// A finished export.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PdfExport {
    /// The PDF file bytes.
    pub bytes: Vec<u8>,
    /// The number of pages written.
    pub pages: usize,
    /// One entry per embedded face.
    pub faces: Vec<EmbeddedFace>,
    /// Everything the caller must be told, including every degradation.
    pub findings: Vec<PdfFinding>,
}

/// Why an export could not be produced.
///
/// Every variant is a loud refusal. The exporter never writes a file that
/// silently misrepresents the document (`AGENTS.md`: no silent data loss).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PdfError {
    /// The document paginated past [`MAX_EXPORT_PAGES`].
    TooManyPages {
        /// The page count the layout pass produced.
        pages: usize,
    },
    /// A face the layout pass used could not be embedded.
    Font(String),
}

impl fmt::Display for PdfError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyPages { pages } => write!(
                formatter,
                "the document lays out to {pages} pages, past the {MAX_EXPORT_PAGES}-page export limit"
            ),
            Self::Font(detail) => formatter.write_str(detail),
        }
    }
}

impl std::error::Error for PdfError {}

impl From<FontError> for PdfError {
    fn from(error: FontError) -> Self {
        Self::Font(error.to_string())
    }
}

/// A [`PdfFontSource`] over the layout crate's bundled faces.
#[derive(Clone, Copy, Debug, Default)]
pub struct BundledFontSource;

impl PdfFontSource for BundledFontSource {
    fn font_data(&self, font: FontId) -> Option<&[u8]> {
        Some(casual_doc_layout::fonts::face_bytes(font))
    }
}

/// A [`PdfFontSource`] over the bundled faces **and** a shaper's dynamic
/// registry, so a system-resolved or host-registered fallback face is embedded
/// as the face it is rather than substituted.
///
/// Snapshot the registry *after* pagination, when every fallback the document
/// needs has been interned — exactly as `casual-doc-render`'s equivalent source
/// requires.
#[derive(Clone, Debug, Default)]
pub struct RegistryFontSource {
    dynamic: BTreeMap<u32, DynFace>,
}

impl RegistryFontSource {
    /// Snapshots `registry`'s dynamic faces.
    #[must_use]
    pub fn new(registry: &FontRegistry) -> Self {
        Self {
            dynamic: registry
                .snapshot()
                .into_iter()
                .map(|(id, face)| (id.0, face))
                .collect(),
        }
    }
}

impl PdfFontSource for RegistryFontSource {
    fn font_data(&self, font: FontId) -> Option<&[u8]> {
        if let Some(face) = self.dynamic.get(&font.0) {
            Some(face.bytes.as_slice())
        } else {
            Some(casual_doc_layout::fonts::face_bytes(font))
        }
    }

    fn face_index(&self, font: FontId) -> u32 {
        self.dynamic.get(&font.0).map_or(0, |face| face.index)
    }
}

/// Lays a document out and exports every page as real-text PDF.
///
/// The pagination pass here is the **same** `paginate_document` /
/// `compose_page` pair the on-screen viewer runs, and the faces come from that
/// pass's own registry, so the PDF carries the faces the layout actually
/// measured with.
///
/// # Errors
///
/// Returns [`PdfError::TooManyPages`] for a document past the page ceiling and
/// [`PdfError::Font`] when a face the layout used cannot be embedded.
pub fn export_document(
    document: &Document,
    media: &dyn PdfMediaSource,
    options: &PdfExportOptions,
) -> Result<PdfExport, PdfError> {
    let shaper = ParleyShaper::new();
    let laid_out = paginate_document(document, &shaper);
    if laid_out.pages.len() > MAX_EXPORT_PAGES {
        return Err(PdfError::TooManyPages {
            pages: laid_out.pages.len(),
        });
    }
    let lists: Vec<_> = laid_out
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
    // Snapshot the registry AFTER pagination, when every fallback face the
    // document needed has been interned.
    let fonts = RegistryFontSource::new(&shaper.registry());
    let metadata = options
        .include_metadata
        .then(|| document.properties().map(info_entries))
        .flatten()
        .unwrap_or_default();
    write_pdf_with_metadata(&pages, &fonts, media, options, &metadata)
}

/// Writes pages that have already been laid out.
///
/// This is the seam a host uses when it already holds the display lists — the
/// viewer, for instance, which must not lay the document out twice.
///
/// # Errors
///
/// Returns [`PdfError::TooManyPages`] past the page ceiling and
/// [`PdfError::Font`] when a face cannot be embedded.
pub fn write_pdf(
    pages: &[PdfPage<'_>],
    fonts: &dyn PdfFontSource,
    media: &dyn PdfMediaSource,
    options: &PdfExportOptions,
) -> Result<PdfExport, PdfError> {
    write_pdf_with_metadata(pages, fonts, media, options, &[])
}

fn write_pdf_with_metadata(
    pages: &[PdfPage<'_>],
    fonts: &dyn PdfFontSource,
    media: &dyn PdfMediaSource,
    options: &PdfExportOptions,
    metadata: &[(&'static str, String)],
) -> Result<PdfExport, PdfError> {
    if pages.len() > MAX_EXPORT_PAGES {
        return Err(PdfError::TooManyPages { pages: pages.len() });
    }
    let mut writer = Writer::new();
    let catalog = writer.reserve();
    let page_tree = writer.reserve();
    let resources = writer.reserve();

    let mut transcriber = Transcriber::new(fonts, media);
    let mut page_objects = Vec::with_capacity(pages.len());
    for page in pages {
        let stream = transcriber.page(&mut writer, *page)?;
        let content = writer.reserve();
        writer.stream(content, "", &stream, true);
        let object = writer.reserve();
        writer.object(
            object,
            &format!(
                "<</Type/Page/Parent {}/MediaBox[0 0 {} {}]/Resources {}/Contents {}>>",
                page_tree.reference(),
                writer::num(points(page.width)),
                writer::num(points(page.height)),
                resources.reference(),
                content.reference(),
            ),
        );
        page_objects.push(object);
    }

    let mut shading_entries = String::new();
    for (name, dictionary) in transcriber.shadings().to_vec() {
        let object = writer.reserve();
        writer.object(object, &dictionary);
        shading_entries.push_str(&writer::name(&name));
        shading_entries.push(' ');
        shading_entries.push_str(&object.reference());
    }
    let mut alpha_entries = String::new();
    for (name, alpha) in transcriber.alphas() {
        alpha_entries.push_str(&writer::name(name));
        alpha_entries.push_str(&format!(
            "<</Type/ExtGState/ca {}/CA {}>>",
            writer::num(alpha),
            writer::num(alpha)
        ));
    }
    // The multiply blend a watermark layer composites through. Declared only when
    // the content stream actually selected it, so an ordinary page's resource
    // dictionary is byte-identical to before.
    if transcriber.uses_multiply_blend() {
        alpha_entries.push_str(&writer::name(MULTIPLY_GS_NAME));
        alpha_entries.push_str("<</Type/ExtGState/BM/Multiply>>");
    }

    let mut findings = Vec::new();
    for (code, occurrences) in transcriber.gaps() {
        findings.push(PdfFinding {
            code: (*code).to_owned(),
            occurrences: *occurrences,
            severity: PdfSeverity::Degraded,
            detail: gap_detail(code),
        });
    }
    for media_key in transcriber.images.unresolved() {
        findings.push(PdfFinding {
            code: "pdf.image.missing_bytes".to_owned(),
            occurrences: 1,
            severity: PdfSeverity::Degraded,
            detail: format!(
                "the host served no usable bytes for `{media_key}`, so that picture is absent from the PDF"
            ),
        });
    }

    let font_entries = transcriber.fonts.resource_dictionary();
    let image_entries = transcriber.images.resource_dictionary();
    let has_fonts = !transcriber.fonts.is_empty();
    let has_images = !transcriber.images.is_empty();
    let faces = transcriber.fonts.write(&mut writer, fonts)?;
    transcriber.images.write(&mut writer);

    for face in &faces {
        if !face.subset {
            findings.push(PdfFinding {
                code: "pdf.font.not_subsetted".to_owned(),
                occurrences: 1,
                severity: PdfSeverity::Note,
                detail: format!(
                    "`{}` was embedded whole ({} bytes): its outlines are CFF, or the face forbids subsetting",
                    face.base_font, face.embedded_bytes
                ),
            });
        }
        if face.unmapped_glyphs > 0 {
            findings.push(PdfFinding {
                code: "pdf.font.unmapped_glyphs".to_owned(),
                occurrences: face.unmapped_glyphs,
                severity: PdfSeverity::Degraded,
                detail: format!(
                    "{} glyphs of `{}` have no character mapping, so copying them out of the PDF yields nothing",
                    face.unmapped_glyphs, face.base_font
                ),
            });
        }
    }

    let mut resource_body = String::from("<<");
    if has_fonts {
        resource_body.push_str(&format!("/Font<<{font_entries}>>"));
    }
    if has_images {
        resource_body.push_str(&format!("/XObject<<{image_entries}>>"));
    }
    if !shading_entries.is_empty() {
        resource_body.push_str(&format!("/Shading<<{shading_entries}>>"));
    }
    if !alpha_entries.is_empty() {
        resource_body.push_str(&format!("/ExtGState<<{alpha_entries}>>"));
    }
    resource_body.push_str("/ProcSet[/PDF/Text/ImageB/ImageC/ImageI]>>");
    writer.object(resources, &resource_body);

    let kids: Vec<String> = page_objects
        .iter()
        .map(|object| object.reference())
        .collect();
    writer.object(
        page_tree,
        &format!(
            "<</Type/Pages/Count {}/Kids[{}]>>",
            page_objects.len(),
            kids.join(" ")
        ),
    );

    let info = if metadata.is_empty() && options.producer.is_none() {
        None
    } else {
        let object = writer.reserve();
        let producer = options
            .producer
            .clone()
            .unwrap_or_else(|| "OpenDoc".to_owned());
        let mut body = format!("<</Producer {}", text_string(&producer));
        for (key, value) in metadata {
            body.push_str(&format!("/{key} {}", text_string(value)));
        }
        body.push_str(">>");
        writer.object(object, &body);
        Some(object)
    };

    writer.object(
        catalog,
        &format!("<</Type/Catalog/Pages {}>>", page_tree.reference()),
    );

    Ok(PdfExport {
        bytes: writer.finish(catalog, info),
        pages: pages.len(),
        faces,
        findings,
    })
}

/// The `/Info` entries a document's own metadata supplies. No clock is read:
/// a date is written only when the document carries one.
fn info_entries(
    properties: &casual_doc_model::v1::DocumentProperties,
) -> Vec<(&'static str, String)> {
    let core = &properties.core;
    [
        ("Title", core.title.as_ref()),
        ("Author", core.creator.as_ref()),
        ("Subject", core.subject.as_ref()),
        ("Keywords", core.keywords.as_ref()),
        ("CreationDate", core.created.as_ref()),
        ("ModDate", core.modified.as_ref()),
    ]
    .into_iter()
    .filter_map(|(key, value)| value.map(|value| (key, value.clone())))
    .collect()
}

fn gap_detail(code: &str) -> String {
    match code {
        "pdf.image.unresolved" => {
            "a picture could not be embedded and is absent from the page".to_owned()
        }
        "pdf.shape.gradient" => {
            "a gradient fill had no representable geometry and was not painted".to_owned()
        }
        other => format!("`{other}` could not be represented in the PDF"),
    }
}

/// A twip length in PDF points.
fn points(value: casual_doc_layout::units::Twip) -> f32 {
    value.raw() as f32 / 20.0
}
