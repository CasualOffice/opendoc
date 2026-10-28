// SPDX-License-Identifier: Apache-2.0

//! PDF import into the normalized model, fixed layout first (`docs/182`).
//!
//! Depends only on `casual-doc-model` and `casual-doc-pdf-read`, following the
//! RTF importer's template (`docs/110`). The reader yields owned page evidence;
//! [`fixed`](crate::ImportJob) turns it into a `v1::Document` whose pages are
//! sections of page-anchored text boxes, pictures and shape groups. Every
//! inference and loss is reported with a `pdf.import.*` finding ([`report`]).
//!
//! The work is a resumable, budgeted job ([`ImportJob`]: `step` / `cancel` /
//! `finish`), the shape of `casual-doc-diff`'s `DiffJob`; nothing is published
//! until `finish` succeeds (PDFR-I3). [`import_pdf`] drives it to completion.
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
pub mod group;
pub mod report;

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use casual_doc_model::v1::Document;
use casual_doc_pdf_read::PdfSource;
pub use casual_doc_pdf_read::{PdfReadError, PdfReadLimits, probe_pdf};

pub use report::{PdfFinding, PdfImportReport, PdfModelOutcome};

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
    next_page: usize,
    state: State,
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
        let namespace = namespace_of(&bytes);
        let source = PdfSource::open(bytes, limits)?;
        let builder = fixed::Builder::new(namespace, source.active_content());
        Ok(Self {
            source,
            builder: Some(builder),
            next_page: 0,
            state: State::Running,
        })
    }

    /// Pages in the document.
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.source.page_count()
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
            let result = self
                .source
                .page_evidence(index)
                .map_err(PdfImportError::from)
                .and_then(|page| {
                    let builder = self.builder.as_mut().expect("running job has a builder");
                    builder.page(&page, self.source.fonts())
                });
            if let Err(error) = result {
                self.builder = None;
                self.state = State::Failed(error);
                return Progress::Failed;
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
        let builder = self.builder.take().expect("a done job has a builder");
        let comparisons = builder.comparisons;
        let (document, resources, report) = builder.finish()?;
        Ok(PdfImport {
            document,
            resources,
            report,
            original: self.source.bytes().clone(),
            pages: self.source.page_count(),
            comparisons,
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

#[cfg(test)]
mod tests;
