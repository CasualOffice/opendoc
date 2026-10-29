// SPDX-License-Identifier: Apache-2.0

//! Security-bounded PDF reading for import (`docs/182` §3, §5.1, §8).
//!
//! This crate wraps hayro (`hayro-syntax` + `hayro-interpret`, Apache-2.0 OR
//! MIT) behind a small API and turns each page into owned [`PageEvidence`]: glyph
//! runs with Unicode, font, size, colour, render mode and orientation; paths
//! with fill rule and stroke; images decoded and re-encoded for the model's
//! media table; marked content; page boxes. It is **evidence only** (PDFR-I1):
//! nothing here builds a document. `casual-doc-pdf-import` does that.
//!
//! It is separate from `casual-doc-pdf` (the writer) because the reader is the
//! attack surface: every input is hostile, and every bound in `docs/21`'s PDF
//! table is enforced here, before the work it bounds.
//!
//! ```
//! use casual_doc_pdf_read::{PdfReadLimits, PdfSource};
//! let bytes = include_bytes!("../../../fixtures/pdf/synthetic-prose.pdf");
//! let mut source = PdfSource::open(bytes.to_vec(), PdfReadLimits::default())?;
//! assert_eq!(source.page_count(), 2);
//! let page = source.page_evidence(0)?;
//! assert!(!page.items.is_empty());
//! # Ok::<(), casual_doc_pdf_read::PdfReadError>(())
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod admit;
mod annots;
pub mod content;
mod device;
mod evidence;
mod limits;
pub mod provision;
pub mod revision;
mod structure;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use hayro_interpret::{Context, InterpreterCache, TransformExt};
use hayro_syntax::{LoadPdfError, Pdf};
use kurbo::Rect;

pub use annots::{AnnotationEvidence, AnnotationKind, LinkTarget, pdf_date, text_string};
pub use evidence::{
    ActiveContent, Bounds, DocumentFonts, FontEvidence, FontProgramKind, GlyphEvidence,
    GlyphRunEvidence, ImageEvidence, ImageFormat, PageEvidence, PageNotes, PaintEvidence,
    PathCommand, PathEvidence, Point, RenderMode, Rgba8, StrokeEvidence,
};
pub use limits::PdfReadLimits;
pub use structure::{StructLeaf, StructNode, StructureTree};

/// The password the reader opens every file with. An unencrypted file ignores
/// it; an encrypted one fails decryption, even when its user password is
/// empty, which is the first-release refusal (`docs/182` Q7).
const REFUSAL_PASSWORD: &str = "\u{1}opendoc-refuses-encrypted-pdf";

/// Why a PDF was refused. Never carries document text.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PdfReadError {
    /// The file is encrypted (`pdf.import.encrypted_unsupported`).
    Encrypted,
    /// The bytes are not a readable PDF.
    Invalid,
    /// The file parses but has no page.
    NoPages,
    /// A `docs/21` bound was exceeded.
    Limit {
        /// The limit's field name in [`PdfReadLimits`].
        limit: &'static str,
        /// What the input asked for (or reached before the reader stopped).
        observed: u64,
        /// What the limit allows.
        allowed: u64,
    },
    /// A page index past the end.
    NoSuchPage(usize),
    /// A page needs predefined CMaps (`UniJIS-UCS2-H`, …) the host did not
    /// provide (`provision`, `docs/182` §17a). Its text would be unreadable,
    /// so the page is refused rather than imported without it.
    CMapUnavailable(Vec<String>),
}

impl fmt::Display for PdfReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encrypted => {
                f.write_str("the PDF is encrypted, which this release does not open")
            }
            Self::Invalid => f.write_str("the bytes are not a readable PDF"),
            Self::NoPages => f.write_str("the PDF has no pages"),
            Self::Limit {
                limit,
                observed,
                allowed,
            } => write!(f, "limit {limit} exceeded: {observed} > {allowed}"),
            Self::NoSuchPage(index) => write!(f, "no page {index}"),
            Self::CMapUnavailable(names) => write!(
                f,
                "the PDF needs character maps this editor was not given ({}) (pdf.import.cmap_unavailable)",
                names.join(", ")
            ),
        }
    }
}

impl std::error::Error for PdfReadError {}

/// Whether `bytes` start like a PDF (`%PDF-` within the first 1,024 bytes,
/// which is where ISO 32000 lets the header sit). O(1).
#[must_use]
pub fn probe_pdf(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(1024)];
    head.windows(5).any(|window| window == b"%PDF-")
}

/// An admitted PDF, ready to yield page evidence.
///
/// Admission ([`PdfSource::open`]) is O(document): it inflates every Flate
/// stream once into a counting sink (constant memory) to refuse decompression
/// bombs before anything decodes them for real. Each [`PdfSource::page_evidence`]
/// call is O(page).
pub struct PdfSource {
    pdf: Pdf,
    bytes: Arc<Vec<u8>>,
    limits: PdfReadLimits,
    active: ActiveContent,
    fonts: DocumentFonts,
    /// hayro font cache key -> index into `fonts`.
    font_index: BTreeMap<u128, usize>,
    /// Fonts whose program extraction was already attempted.
    programs_tried: BTreeSet<usize>,
    /// Predefined CMaps the fonts name (`provision`).
    required_cmaps: BTreeSet<String>,
}

impl fmt::Debug for PdfSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PdfSource")
            .field("bytes", &self.bytes.len())
            .field("pages", &self.page_count())
            .field("fonts", &self.fonts.fonts.len())
            .finish_non_exhaustive()
    }
}

impl PdfSource {
    /// Admits `bytes` under `limits`.
    ///
    /// # Errors
    ///
    /// [`PdfReadError::Encrypted`], [`PdfReadError::Invalid`],
    /// [`PdfReadError::NoPages`], or a [`PdfReadError::Limit`].
    pub fn open(bytes: Vec<u8>, limits: PdfReadLimits) -> Result<Self, PdfReadError> {
        limits.validate()?;
        limits::check(
            "max_input_bytes",
            bytes.len() as u64,
            limits.max_input_bytes,
        )?;
        if !probe_pdf(&bytes) {
            return Err(PdfReadError::Invalid);
        }
        let bytes = Arc::new(bytes);
        let pdf =
            Pdf::new_with_password(bytes.clone(), REFUSAL_PASSWORD).map_err(
                |error| match error {
                    LoadPdfError::Decryption(_) => PdfReadError::Encrypted,
                    LoadPdfError::Invalid => PdfReadError::Invalid,
                },
            )?;
        limits::check("max_objects", pdf.len() as u64, limits.max_objects)?;
        let pages = pdf.pages().len() as u64;
        if pages == 0 {
            return Err(PdfReadError::NoPages);
        }
        limits::check("max_pages", pages, limits.max_pages)?;
        for page in pdf.pages().iter() {
            let (width, height) = page.render_dimensions();
            let side = f64::from(width.max(height)).ceil() as u64;
            limits::check("max_page_side_points", side, limits.max_page_side_points)?;
        }
        admit::bound_decoded_streams(&pdf, &limits)?;
        let active = admit::active_content(&pdf);
        let required_cmaps = admit::required_cmaps(&pdf);
        Ok(Self {
            pdf,
            bytes,
            limits,
            active,
            fonts: DocumentFonts::default(),
            font_index: BTreeMap::new(),
            programs_tried: BTreeSet::new(),
            required_cmaps,
        })
    }

    /// The predefined CMaps this file's Type0 fonts name, found at admission,
    /// so a host can fetch them before the first page
    /// ([`provision::provide_cmap`]). O(1).
    #[must_use]
    pub fn required_cmaps(&self) -> &BTreeSet<String> {
        &self.required_cmaps
    }

    /// [`Self::required_cmaps`] that no source has yet. O(required).
    #[must_use]
    pub fn missing_cmaps(&self) -> Vec<String> {
        self.required_cmaps
            .iter()
            .filter(|name| !provision::has_cmap(name))
            .cloned()
            .collect()
    }

    /// The number of pages. O(1).
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.pdf.pages().len()
    }

    /// The original bytes, shared rather than copied (retained for an exact
    /// write-back of an unchanged document, MF-I4).
    #[must_use]
    pub fn bytes(&self) -> &Arc<Vec<u8>> {
        &self.bytes
    }

    /// Active content found at admission: counted, never run.
    #[must_use]
    pub fn active_content(&self) -> &ActiveContent {
        &self.active
    }

    /// The fonts seen by the pages read so far, in first-use order.
    #[must_use]
    pub fn fonts(&self) -> &DocumentFonts {
        &self.fonts
    }

    /// The limits this source was admitted under.
    #[must_use]
    pub fn limits(&self) -> &PdfReadLimits {
        &self.limits
    }

    /// Interprets page `index` into owned evidence. O(page).
    ///
    /// # Errors
    ///
    /// [`PdfReadError::NoSuchPage`], or a [`PdfReadError::Limit`] for
    /// operators, evidence records, image pixels, fonts or font program bytes.
    pub fn page_evidence(&mut self, index: usize) -> Result<PageEvidence, PdfReadError> {
        let pages = self.pdf.pages();
        let page = pages.get(index).ok_or(PdfReadError::NoSuchPage(index))?;
        let operators = {
            let mut iter = page.operations();
            let mut count: u64 = 0;
            while iter.next().is_some() {
                count += 1;
                limits::check(
                    "max_operators_per_page",
                    count,
                    self.limits.max_operators_per_page,
                )?;
            }
            count
        };
        let (width, height) = page.render_dimensions();
        admit::learn_fonts(page, &mut self.fonts, &mut self.font_index, &self.limits)?;
        // The host's faces and CMaps, never hayro's bundled ones (`provision`).
        let misses = provision::Misses::default();
        let settings = hayro_interpret::InterpreterSettings {
            // Annotations are counted at admission and reported; their
            // appearance streams are not page content in a fixed-layout import.
            render_annotations: false,
            ..provision::settings_recording(misses.clone())
        };
        let cache = InterpreterCache::new();
        let mut context = Context::new(
            page.initial_transform(true).to_kurbo(),
            Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
            &cache,
            self.pdf.xref(),
            settings,
        );
        let mut collector = device::Collector::new(
            &self.limits,
            &mut self.fonts,
            &mut self.font_index,
            &mut self.programs_tried,
            Bounds {
                x0: 0.0,
                y0: 0.0,
                x1: f64::from(width),
                y1: f64::from(height),
            },
        );
        // Number the page stream's show operators by interpreting an
        // instrumented copy (`content::instrument`): identical paint, plus a
        // marked-content pair per show the device reads back. When the two
        // tokenizers disagree the original stream is interpreted as is and the
        // page is not preservable (`shows: None`). Annotations are not page
        // content here, so `interpret` is exactly `interpret_page`'s first half.
        let stream = page.page_stream().unwrap_or(&[]);
        let instrumented = content::tokenize(stream)
            .filter(|ops| content::agrees_with_hayro(stream, ops))
            .and_then(|ops| content::instrument(stream, &ops));
        let shows = instrumented.as_ref().map(|(_, shows)| *shows);
        let ops_source: &[u8] = instrumented.as_ref().map_or(stream, |(bytes, _)| bytes);
        hayro_interpret::interpret(
            hayro_syntax::content::TypedIter::new(ops_source),
            page.resources(),
            &mut context,
            &mut collector,
        );
        let (items, notes) = collector.finish()?;
        let missing: Vec<String> = misses
            .lock()
            .map(|names| names.iter().cloned().collect())
            .unwrap_or_default();
        if !missing.is_empty() {
            return Err(PdfReadError::CMapUnavailable(missing));
        }
        let rotation = match page.rotation() {
            hayro_syntax::page::Rotation::None => 0,
            hayro_syntax::page::Rotation::Horizontal => 90,
            hayro_syntax::page::Rotation::Flipped => 180,
            hayro_syntax::page::Rotation::FlippedHorizontal => 270,
        };
        Ok(PageEvidence {
            index,
            width: f64::from(width),
            height: f64::from(height),
            rotation,
            items,
            notes,
            operators,
            object: page.raw().obj_id().and_then(|id| {
                Some((
                    u32::try_from(id.obj_number).ok()?,
                    u16::try_from(id.gen_number).ok()?,
                ))
            }),
            shows,
        })
    }
}

#[cfg(test)]
mod tests;
