// SPDX-License-Identifier: Apache-2.0

//! PDF import into the normalized model (`docs/182`).
//!
//! Depends only on `casual-doc-model` and `casual-doc-pdf-read`, following the
//! RTF importer's template (`docs/110`). The reader yields owned page evidence;
//! [`fixed`](crate::ImportJob) turns it into a `v1::Document` whose pages are
//! sections of page-anchored text boxes, pictures and shape groups (the exact
//! page layout), and [`PdfImport::flowing`] reconstructs from the same
//! evidence the flowing document every host opens by default (`docs/182` §4).
//! Every inference and loss is reported with a `pdf.import.*` finding
//! ([`report`]).
//!
//! The work is a resumable, budgeted job ([`ImportJob`]: `step` / `cancel` /
//! `finish`), the shape of `casual-doc-diff`'s `DiffJob`; nothing is published
//! until `finish` succeeds (PDFR-I3). [`import_pdf`] drives it to completion.
//!
//! Scanned pages and runs whose Unicode is unrecoverable go through [`ocr`]
//! first: classified from the native evidence, rasterised region by region,
//! read by the host's [`casual_doc_ocr::OcrProvider`] and merged native-first
//! (PDFR-I2). With no provider the regions are reported
//! (`pdf.import.ocr_unavailable`), never silently skipped.
//!
//! ```
//! use casual_doc_pdf_import::{import_pdf, PdfReadLimits};
//! let bytes = include_bytes!("../../../fixtures/pdf/synthetic-prose.pdf");
//! let imported = import_pdf(bytes.to_vec(), PdfReadLimits::default())?;
//! assert_eq!(imported.document.definitions().sections.len(), 2);
//! assert!(imported.report.has("pdf.import.font_substituted"));
//! # Ok::<(), casual_doc_pdf_import::PdfImportError>(())
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod fixed;
pub mod fonts;
pub mod group;
mod links;
pub mod ocr;
pub mod quotes;
pub mod reflow;
pub mod report;
#[cfg(test)]
mod synth;
pub mod wire;

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use casual_doc_model::v1::Document;
use casual_doc_pdf_read::PdfSource;
pub use casual_doc_pdf_read::{PdfReadError, PdfReadLimits, probe_pdf};

/// The OCR engine itself, so a host can implement its own
/// [`casual_doc_ocr::OcrProvider`] without naming the crate.
pub use casual_doc_ocr;
/// The OCR engine's host-facing types, so a host reaches the model set and
/// the provider seam through this crate.
pub use casual_doc_ocr::{
    Determinism as OcrDeterminism, OcrModels, OcrModelsError, OcrProvider, ProviderManifest,
    RecognitionModel,
};
pub use ocr::{OcrOptions, OcrSummary, OcrTextPlacement, PageClass, PageLayout, ScanBehindText};
pub use reflow::{PdfSourceState, ReflowOptions, Reflowed, ScanText};
pub use report::{PdfFinding, PdfImportReport, PdfModelOutcome};

/// Mixed into the id namespace of a reflowed document, so its ids are
/// deterministic (PDFR-I8) and distinct from the fixed-layout document's.
const REFLOW_NAMESPACE: u64 = 0x5245_464c_4f57_0001;

/// Why an import failed. Carries no document text.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PdfImportError {
    /// The reader refused the file (encryption, a limit, malformed bytes).
    Read(PdfReadError),
    /// The job was cancelled; nothing was produced.
    Cancelled,
    /// The built document failed model validation (an importer defect).
    Model(String),
}

impl PdfImportError {
    /// The `pdf.import.*` finding id for a refusal that has one.
    #[must_use]
    pub fn feature(&self) -> Option<&'static str> {
        match self {
            Self::Read(PdfReadError::Encrypted) => Some(report::ENCRYPTED_UNSUPPORTED),
            _ => None,
        }
    }
}

impl fmt::Display for PdfImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => {
                if let Some(feature) = self.feature() {
                    write!(f, "{error} ({feature})")
                } else {
                    write!(f, "{error}")
                }
            }
            Self::Cancelled => f.write_str("the import was cancelled"),
            Self::Model(reason) => write!(f, "the imported document is invalid: {reason}"),
        }
    }
}

impl std::error::Error for PdfImportError {}

impl From<PdfReadError> for PdfImportError {
    fn from(error: PdfReadError) -> Self {
        Self::Read(error)
    }
}

/// Where each imported page and text frame came from in the original file:
/// what a byte-preserving save (`docs/182` §7, P4) needs to carry an unchanged
/// page over and to remove exactly an edited frame's operators.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceMap {
    /// One per page, in page order.
    pub pages: Vec<PageSource>,
    /// One per text frame, in build order.
    pub frames: Vec<FrameSource>,
}

/// One page's origin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageSource {
    /// The page object's number and generation.
    pub object: Option<(u32, u16)>,
    /// The page stream's show-operator count, when every run is numbered.
    pub shows: Option<u32>,
}

/// One text frame's origin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameSource {
    /// The page index.
    pub page: usize,
    /// The `TextBox` node the frame became.
    pub frame: casual_doc_model::NodeId,
    /// The show operators that painted its glyphs, ascending.
    pub shows: Vec<u32>,
    /// Whether every glyph has a known show (none came from a form XObject or
    /// an un-numbered page): only then can the frame's text be removed.
    pub complete: bool,
}

/// A finished import.
#[derive(Debug)]
pub struct PdfImport {
    /// The validated document.
    pub document: Document,
    /// Media bytes keyed by the part names the document references.
    pub resources: BTreeMap<String, Vec<u8>>,
    /// Every inference and loss.
    pub report: PdfImportReport,
    /// The original bytes, shared with the reader (no copy).
    pub original: Arc<Vec<u8>>,
    /// Pages imported.
    pub pages: usize,
    /// Line-against-block comparisons the grouping made: a work counter for
    /// complexity tests, never a time.
    pub comparisons: u64,
    /// The PDF source state (`docs/182` §3, the I4 sidecar): what
    /// [`PdfImport::reflow`] reads, so conversion needs no second parse.
    pub source: PdfSourceState,
    /// The id namespace of this import (the bytes' hash).
    pub namespace: u64,
    /// Page objects and frame operators (`docs/182` P4).
    pub sources: SourceMap,
    /// What OCR classified, read and skipped, and which model read it.
    pub ocr: OcrSummary,
    /// Layout regions per page (P6), when the host supplied a layout
    /// provider; empty otherwise. Evidence for reflow, not structure.
    pub layout: Vec<PageLayout>,
}

impl PdfImport {
    /// "Convert to flowing document" (`docs/182` §4.2, §18): a **new**
    /// document of paragraphs, headings, lists, tables, header/footer parts,
    /// footnotes and column sections, built from this import's source state.
    /// The fixed-layout document is untouched. Pure and deterministic: the
    /// same import reflows to the same document. This is the stable entry
    /// point the editor, MCP and SDK lanes call. O(document log document).
    ///
    /// # Errors
    ///
    /// [`PdfImportError::Model`] if the result fails validation (a defect).
    pub fn reflow(&self) -> Result<Reflowed, PdfImportError> {
        self.reflow_with(ReflowOptions::default())
    }

    /// [`PdfImport::reflow`] with explicit [`ReflowOptions`] (what becomes
    /// of text read from pictures: line boxes over them, the default, or
    /// paragraphs of the flow). O(document log document).
    ///
    /// # Errors
    ///
    /// As [`PdfImport::reflow`].
    pub fn reflow_with(&self, options: ReflowOptions) -> Result<Reflowed, PdfImportError> {
        reflow::build_with(
            &self.source,
            &self.resources,
            self.namespace ^ REFLOW_NAMESPACE,
            options,
        )
    }

    /// The PDF opened as a flowing document: what every host opens by
    /// default (`docs/182` §4.2), Word's PDF Reflow and Google Docs' "Open
    /// with" behaviour. The document is [`PdfImport::reflow`]'s; the report
    /// is the whole import's: what reading the pages found (fonts
    /// substituted, text with no Unicode, OCR, active content, symbol and
    /// ligature mapping), then what the reconstruction inferred, plus
    /// [`report::LAYOUT_INFERRED`] (the layout is evidence, not the
    /// document) and [`report::EXACT_SAVE_FIXED_LAYOUT_ONLY`] (saved as PDF
    /// it is a new file). Fixed-layout-only rows (frames, reading order of
    /// frames) are left out. O(document log document).
    ///
    /// # Errors
    ///
    /// As [`PdfImport::reflow`].
    pub fn flowing(&self) -> Result<Reflowed, PdfImportError> {
        self.flowing_with(ReflowOptions::default())
    }

    /// [`PdfImport::flowing`] with explicit [`ReflowOptions`].
    /// O(document log document).
    ///
    /// # Errors
    ///
    /// As [`PdfImport::reflow`].
    pub fn flowing_with(&self, options: ReflowOptions) -> Result<Reflowed, PdfImportError> {
        let mut reflowed = self.reflow_with(options)?;
        reflowed.report = flowing_report(&self.report, &reflowed.report, self.pages);
        Ok(reflowed)
    }
}

/// The rows of a fixed-layout report that describe its frames, not the PDF:
/// a flowing document has no frames, and its reconstruction reports its own
/// paragraphs and reading order.
const FIXED_ONLY: &[&str] = &[
    report::PARAGRAPH_BOUNDARY_INFERRED,
    report::READING_ORDER_INFERRED,
    report::TAG_STRUCTURE_IGNORED,
    report::VECTOR_SPLIT,
];

/// The report of a flowing open: the reading half of `fixed`, the
/// reconstruction `reflowed`, and the two rows that say what a flowing
/// document is. A feature both report keeps the reconstruction's row.
/// O(findings log findings).
fn flowing_report(
    fixed: &PdfImportReport,
    reflowed: &PdfImportReport,
    pages: usize,
) -> PdfImportReport {
    let mut rows: BTreeMap<&'static str, PdfFinding> = BTreeMap::new();
    for finding in &fixed.findings {
        if !FIXED_ONLY.contains(&finding.feature) {
            rows.insert(finding.feature, finding.clone());
        }
    }
    for finding in &reflowed.findings {
        rows.insert(finding.feature, finding.clone());
    }
    let pages = u32::try_from(pages.max(1)).unwrap_or(u32::MAX);
    rows.insert(
        report::LAYOUT_INFERRED,
        PdfFinding {
            feature: report::LAYOUT_INFERRED,
            occurrences: pages,
            outcome: PdfModelOutcome::Degraded,
            first_page: Some(0),
        },
    );
    rows.insert(
        report::EXACT_SAVE_FIXED_LAYOUT_ONLY,
        PdfFinding {
            feature: report::EXACT_SAVE_FIXED_LAYOUT_ONLY,
            occurrences: 1,
            outcome: PdfModelOutcome::Mapped,
            first_page: None,
        },
    );
    PdfImportReport {
        findings: rows.into_values().collect(),
    }
}

/// Where a job is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Progress {
    /// More pages to do.
    Pending {
        /// Pages done.
        page: usize,
        /// Pages in total.
        of: usize,
    },
    /// Every page is done; call [`ImportJob::finish`].
    Done,
    /// The job was cancelled.
    Cancelled,
    /// A page failed; [`ImportJob::finish`] returns the error.
    Failed,
}

/// A resumable, budgeted, cancellable import (`docs/182` §6).
///
/// [`ImportJob::begin`] admits the file (O(document): the reader's bomb
/// pre-pass and active-content inventory). Each [`ImportJob::step`] then does
/// at most `budget` pages, each bounded by the per-page limits, so a step is
/// O(budget). [`ImportJob::finish`] validates and returns the document
/// atomically; a cancelled or failed job returns no document (PDFR-I3).
pub struct ImportJob {
    source: PdfSource,
    builder: Option<fixed::Builder>,
    ocr: ocr::OcrStage,
    next_page: usize,
    state: State,
    sidecar: PdfSourceState,
    namespace: u64,
    quotes: quotes::QuoteNames,
}

#[derive(Clone, Debug, PartialEq)]
enum State {
    Running,
    Cancelled,
    Failed(PdfImportError),
}

impl fmt::Debug for ImportJob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImportJob")
            .field("next_page", &self.next_page)
            .field("pages", &self.source.page_count())
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

/// The FNV-1a hash of the bytes: the id namespace, so the same file imports to
/// the same ids on every run and platform (PDFR-I8). O(bytes).
fn namespace_of(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

impl ImportJob {
    /// Admits `bytes` and prepares the job. O(document).
    ///
    /// # Errors
    ///
    /// [`PdfImportError::Read`] when the reader refuses the file.
    pub fn begin(bytes: Vec<u8>, limits: PdfReadLimits) -> Result<Self, PdfImportError> {
        Self::begin_with_ocr(bytes, limits, OcrOptions::default())
    }

    /// [`ImportJob::begin`] with the host's OCR (and optional layout)
    /// configuration. O(document).
    ///
    /// # Errors
    ///
    /// [`PdfImportError::Read`] when the reader refuses the file.
    pub fn begin_with_ocr(
        bytes: Vec<u8>,
        limits: PdfReadLimits,
        ocr: OcrOptions,
    ) -> Result<Self, PdfImportError> {
        let namespace = namespace_of(&bytes);
        let source = PdfSource::open(bytes, limits)?;
        let builder = fixed::Builder::new(namespace, source.active_content());
        let sidecar = PdfSourceState {
            structure: source.structure_tree(),
            active: source.active_content().clone(),
            ..PdfSourceState::default()
        };
        Ok(Self {
            source,
            builder: Some(builder),
            ocr: ocr::OcrStage::new(ocr),
            next_page: 0,
            state: State::Running,
            sidecar,
            namespace,
            quotes: quotes::QuoteNames::default(),
        })
    }

    /// Pages in the document.
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.source.page_count()
    }

    /// Predefined CMaps the file names that no source has yet
    /// (`casual_doc_pdf_read::provision`, `docs/182` §17a). O(fonts).
    #[must_use]
    pub fn missing_cmaps(&self) -> Vec<String> {
        self.source.missing_cmaps()
    }

    /// Pages read so far that have something to read optically: scans and
    /// pages that mix native text with a picture of text
    /// ([`PageClass::Raster`], [`PageClass::Hybrid`]). A host reads this
    /// between steps to start loading its recognition models while the rest
    /// of the import runs (`docs/182` §17a.5). O(pages read).
    #[must_use]
    pub fn scanned_pages(&self) -> usize {
        self.ocr
            .summary
            .classes
            .iter()
            .filter(|class| matches!(class, Some(PageClass::Raster | PageClass::Hybrid)))
            .count()
    }

    /// Does at most `budget` pages (at least one). O(budget pages).
    pub fn step(&mut self, budget: usize) -> Progress {
        match self.state {
            State::Cancelled => return Progress::Cancelled,
            State::Failed(_) => return Progress::Failed,
            State::Running => {}
        }
        let total = self.source.page_count();
        let end = self.next_page.saturating_add(budget.max(1)).min(total);
        while self.next_page < end {
            let index = self.next_page;
            let builder = self.builder.as_mut().expect("running job has a builder");
            let result = self
                .source
                .page_evidence(index)
                .and_then(|page| Ok((page, self.source.annotations(index)?)))
                .map_err(PdfImportError::from)
                .and_then(|(mut page, annotations)| {
                    // Curly quotes a wrong `ToUnicode` flattened, from the
                    // glyph names; before OCR, which reads only what has no
                    // Unicode at all.
                    let quoted = self.quotes.correct(&mut page, self.source.fonts());
                    builder.findings_mut().record(
                        report::QUOTE_FROM_GLYPH_NAME,
                        PdfModelOutcome::Mapped,
                        quoted,
                        Some(index),
                    );
                    self.ocr
                        .page(&mut page, self.source.bytes(), builder.findings_mut())?;
                    builder.page(&page, self.source.fonts(), &annotations)?;
                    self.sidecar
                        .record(&page, annotations, group::LayoutParams::PDFMINER);
                    Ok(())
                });
            match result {
                Ok(()) => {}
                Err(PdfImportError::Cancelled) => {
                    self.cancel();
                    return Progress::Cancelled;
                }
                Err(error) => {
                    self.builder = None;
                    self.state = State::Failed(error);
                    return Progress::Failed;
                }
            }
            self.next_page += 1;
        }
        if self.next_page >= total {
            Progress::Done
        } else {
            Progress::Pending {
                page: self.next_page,
                of: total,
            }
        }
    }

    /// Stops the job. Takes effect immediately; the partial build is dropped.
    pub fn cancel(&mut self) {
        self.builder = None;
        self.state = State::Cancelled;
    }

    /// Returns the document, or why there is none. Runs any remaining pages
    /// first, so a caller that only wants the result may call this directly.
    ///
    /// # Errors
    ///
    /// [`PdfImportError::Cancelled`], the page error that failed the job, or
    /// [`PdfImportError::Model`].
    pub fn finish(mut self) -> Result<PdfImport, PdfImportError> {
        loop {
            match self.step(usize::MAX) {
                Progress::Done => break,
                Progress::Pending { .. } => {}
                Progress::Cancelled => return Err(PdfImportError::Cancelled),
                Progress::Failed => {
                    return Err(match self.state {
                        State::Failed(error) => error,
                        _ => unreachable!("Failed progress implies a failed state"),
                    });
                }
            }
        }
        let mut builder = self.builder.take().expect("a done job has a builder");
        let comparisons = builder.comparisons;
        let sources = std::mem::take(&mut builder.sources);
        let mut sidecar = std::mem::take(&mut self.sidecar);
        sidecar.fonts = self.source.fonts().clone();
        sidecar.font_family = builder.font_family.clone();
        sidecar.font_programs = builder.programs.clone();
        for (page, bounds, media, ink, opacity) in std::mem::take(&mut builder.placed_images) {
            if let Some(record) = sidecar.pages.get_mut(page) {
                record.images.push(reflow::SourceImage {
                    bounds,
                    media,
                    ink,
                    opacity,
                });
            }
        }
        let (document, resources, report) = builder.finish()?;
        Ok(PdfImport {
            document,
            resources,
            report,
            original: self.source.bytes().clone(),
            pages: self.source.page_count(),
            comparisons,
            source: sidecar,
            namespace: self.namespace,
            sources,
            ocr: std::mem::take(&mut self.ocr.summary),
            layout: std::mem::take(&mut self.ocr.layout),
        })
    }
}

/// Imports a PDF in one call (the synchronous path: [`ImportJob`] run to
/// completion under `limits`). O(document).
///
/// # Errors
///
/// As [`ImportJob::begin`] and [`ImportJob::finish`].
pub fn import_pdf(bytes: Vec<u8>, limits: PdfReadLimits) -> Result<PdfImport, PdfImportError> {
    ImportJob::begin(bytes, limits)?.finish()
}

/// [`import_pdf`] with the host's OCR configuration. O(document) plus the
/// provider's work, bounded by `ocr`'s budgets.
///
/// # Errors
///
/// As [`ImportJob::begin_with_ocr`] and [`ImportJob::finish`].
pub fn import_pdf_with_ocr(
    bytes: Vec<u8>,
    limits: PdfReadLimits,
    ocr: OcrOptions,
) -> Result<PdfImport, PdfImportError> {
    ImportJob::begin_with_ocr(bytes, limits, ocr)?.finish()
}

#[cfg(test)]
mod tests;
