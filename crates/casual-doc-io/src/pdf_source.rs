//! A PDF-origin session without the PDF reader (`docs/182` §17a).
//!
//! The editor's main WebAssembly module does not link hayro, the reader or the
//! importer: those live in a second module, loaded the first time a PDF is
//! opened, which hands back a `casual-doc-pdf-wire` [`ImportPackage`]. This
//! module turns that package (or, in a native host, the importer's own output)
//! into the same [`ImportArtifact`] the in-process adapter always produced:
//!
//! - the document and its media;
//! - the `pdf.import.*` report, mapped once ([`pdf_session_artifact`]);
//! - the [`SourceEnvelope`] holding the original bytes (the exact save) and
//!   the byte-preserving save's baseline: the `casual-doc-pdf-wire`
//!   `Baseline` bytes the importer computed, kept opaque and handed back to
//!   the planner at save, so this module never digests a document;
//! - what OCR did ([`pdf_ocr_record`]).
//!
//! and saves it ([`export_pdf_session`]): the original back when unchanged or
//! undone, an incremental update when that is safe (the plan and the hayro
//! half through a [`PdfSavePlanner`], the overlay and the update's writer
//! here), the semantic writer, reported, otherwise.
//!
//! The package is checked before anything is built from it: its own codec is
//! bounded, the original handed in beside it must have the length and FNV-1a
//! hash the import recorded, and the document goes through the snapshot
//! importer's limits and validation like any shared snapshot.

use std::sync::Arc;

use casual_doc_model::SnapshotLimits;
use casual_doc_model::v1::Document;
use casual_doc_pdf_wire::{Finding, ImportPackage, OcrSummary, Outcome, fnv1a64};

pub use crate::pdf_preserve::PdfSavePlanner;
use crate::pdf_preserve::{Fallback, ORIGINAL_WRITTEN_BACK, Planned, plan_and_write};
use crate::{
    AdapterError, CompatibilityEntry, CompatibilityReport, DocumentResources, ExportArtifact,
    ExportMode, ExportRequest, FeatureLocation, FormatId, FormatProfile, ImportArtifact,
    ModelOutcome, RecoveryReport, RetentionOutcome, SourceEnvelope, formats,
};

/// How a PDF opens (`docs/182` §4): the host's choice, made at open.
///
/// **Flowing** is the default, the way Word's PDF Reflow and Google Docs
/// open a PDF: the page layout is evidence, and the document is an ordinary
/// flowing one (paragraphs with styles, lists, tables, header and footer,
/// footnotes, columns, inline pictures), with no text boxes and no
/// positioned frames. It saves as DOCX by default, and saved as PDF it is a
/// new file. **Fixed** ("Keep exact page layout") keeps every page as it is
/// drawn, one positioned frame per text block; only fixed layout saves back
/// into the original PDF (the byte-preserving save, P4).
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum PdfLayout {
    /// Reconstructed as a flowing document (the default).
    #[default]
    Flowing,
    /// Every page as drawn, in positioned frames.
    Fixed,
}

impl PdfLayout {
    /// `flowing` or `fixed`: the token hosts pass (`?pdfLayout=`, MCP's
    /// `layout`). O(1).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Flowing => "flowing",
            Self::Fixed => "fixed",
        }
    }

    /// The layout a host's token names, in any case; `None` for anything
    /// else. O(token).
    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        if token.eq_ignore_ascii_case("flowing") {
            Some(Self::Flowing)
        } else if token.eq_ignore_ascii_case("fixed") {
            Some(Self::Fixed)
        } else {
            None
        }
    }
}

/// The original bytes an import retained, and what a byte-preserving save
/// compares the document against (the `I4` sidecar, `docs/182` §7 P4). No PDF
/// object reaches the model (MF-I1).
#[derive(Debug)]
pub(crate) struct PdfSourceState {
    /// Fixed layout's original is what the exact and byte-preserving saves
    /// write; a flowing document's is kept only so the host can open the PDF
    /// again (in its exact layout, or with OCR). Never written back.
    pub(crate) original: Option<Arc<Vec<u8>>>,
    /// How this session's document was made from the PDF.
    pub(crate) layout: PdfLayout,
    /// The encoded `casual-doc-pdf-wire` `Baseline`: opaque here.
    pub(crate) baseline: Option<Vec<u8>>,
    /// What OCR did, reduced to what the editor's summary shows (no
    /// recognised text). Present for every PDF-origin session.
    pub(crate) ocr: OcrSummary,
    /// The importer's full OCR record (provider manifest with every model's
    /// SHA-256, PDFR-I8), when the import ran in process; a session opened
    /// from the PDF module's package carries only [`PdfSourceState::ocr`].
    #[cfg(feature = "pdf-import")]
    pub(crate) ocr_detail: Option<casual_doc_pdf_import::OcrSummary>,
    /// What reflow reads, and the import's id namespace, when the import ran
    /// in process (`crate::pdf_reflow`); `None` in a reflowed document's own
    /// envelope and in a session opened from the PDF module's package (that
    /// module reflows itself). O(glyphs + path commands) for the session.
    #[cfg(feature = "pdf-import")]
    pub(crate) reflow: Option<(casual_doc_pdf_import::PdfSourceState, u64)>,
}

/// Everything a PDF-origin session's artifact is built from.
pub(crate) struct SessionParts<'a> {
    pub(crate) document: Document,
    pub(crate) resources: DocumentResources,
    pub(crate) findings: &'a [Finding],
    pub(crate) baseline: Option<Vec<u8>>,
    pub(crate) original: Option<Arc<Vec<u8>>>,
    pub(crate) ocr: OcrSummary,
    #[cfg(feature = "pdf-import")]
    pub(crate) ocr_detail: Option<casual_doc_pdf_import::OcrSummary>,
    #[cfg(feature = "pdf-import")]
    pub(crate) reflow: Option<(casual_doc_pdf_import::PdfSourceState, u64)>,
    pub(crate) layout: PdfLayout,
}

fn pdf_format() -> FormatId {
    FormatId::new(formats::PDF).expect("built-in PDF format id is valid")
}

/// The import artifact of a PDF-origin session: the document, its media, the
/// report and the retained source. `original` is `None` when the host did not
/// ask to retain it; `baseline` is the encoded `casual-doc-pdf-wire`
/// `Baseline` the importer computed (`casual-doc-pdf-save::baseline`), `None`
/// when there is none, and then no edited save preserves. O(findings).
#[must_use]
pub fn pdf_session_artifact(
    document: Document,
    resources: DocumentResources,
    findings: &[Finding],
    baseline: Option<Vec<u8>>,
    original: Option<Arc<Vec<u8>>>,
) -> ImportArtifact {
    session_artifact(SessionParts {
        document,
        resources,
        findings,
        baseline,
        original,
        ocr: OcrSummary::default(),
        #[cfg(feature = "pdf-import")]
        ocr_detail: None,
        #[cfg(feature = "pdf-import")]
        reflow: None,
        layout: PdfLayout::Fixed,
    })
}

/// [`pdf_session_artifact`] with what OCR did. O(findings).
pub(crate) fn session_artifact(parts: SessionParts<'_>) -> ImportArtifact {
    let SessionParts {
        document,
        resources,
        findings,
        baseline,
        original,
        ocr,
        #[cfg(feature = "pdf-import")]
        ocr_detail,
        #[cfg(feature = "pdf-import")]
        reflow,
        layout,
    } = parts;
    let mut report = CompatibilityReport {
        entries: findings
            .iter()
            .map(|finding| CompatibilityEntry {
                feature: finding.feature.clone(),
                occurrences: finding.occurrences,
                location: FeatureLocation {
                    part_name: finding.first_page.map(|page| format!("page {}", page + 1)),
                    namespace: None,
                    local_name: Some(finding.feature.clone()),
                    attribute_name: None,
                },
                model_outcome: match finding.outcome {
                    Outcome::Mapped => ModelOutcome::Mapped,
                    Outcome::Degraded => ModelOutcome::Degraded,
                    Outcome::Omitted => ModelOutcome::Omitted,
                },
                retention_outcome: match finding.outcome {
                    Outcome::Mapped => RetentionOutcome::NotApplicable,
                    // The original bytes are retained when asked for, but no
                    // adapter re-emits PDF objects from them except the exact
                    // unchanged write, so a lost construct is not retained.
                    Outcome::Degraded | Outcome::Omitted => RetentionOutcome::NotRetained,
                },
            })
            .collect(),
    };
    report.sort();
    // A baseline without the original it describes preserves nothing, and a
    // flowing document has nothing of the original's pages to preserve.
    let baseline = baseline.filter(|_| original.is_some() && layout == PdfLayout::Fixed);
    let format = pdf_format();
    // A flowing document is a DOCX-shaped document: that is what it saves as
    // unless the host asks otherwise. The envelope stays the PDF's, so the
    // host can still open the original again.
    let profile = match layout {
        PdfLayout::Fixed => format.clone(),
        PdfLayout::Flowing => {
            FormatId::new(formats::DOCX).expect("built-in DOCX format id is valid")
        }
    };
    ImportArtifact {
        document,
        resources,
        source: SourceEnvelope::new(
            format.clone(),
            env!("CARGO_PKG_VERSION").to_owned(),
            PdfSourceState {
                baseline,
                original,
                layout,
                ocr,
                #[cfg(feature = "pdf-import")]
                ocr_detail,
                #[cfg(feature = "pdf-import")]
                reflow,
            },
        ),
        report,
        recovery: RecoveryReport::default(),
        format: FormatProfile {
            format: profile,
            version: None,
        },
    }
}

/// Opens the PDF module's package as an import artifact. `original` is the
/// file the host read; it must be the one imported (same length and hash),
/// and it is retained for the exact and byte-preserving saves.
/// O(document + original).
///
/// # Errors
///
/// A malformed package, an original that is not the imported file, or a
/// document the snapshot importer refuses.
pub fn import_pdf_package(
    package: &[u8],
    original: Vec<u8>,
    limits: SnapshotLimits,
) -> Result<ImportArtifact, AdapterError> {
    let package = ImportPackage::decode(package).map_err(|e| AdapterError::new(e.to_string()))?;
    if package.original_len != original.len() as u64 || package.original_hash != fnv1a64(&original)
    {
        return Err(AdapterError::new(
            "the PDF module's import is of a different file than the one being opened",
        ));
    }
    let document = Document::from_json(&package.document_json, limits)
        .map_err(|e| AdapterError::new(format!("the imported PDF's document: {e}")))?;
    let mut resources = DocumentResources::default();
    for (name, bytes) in package.resources {
        resources.insert(name, bytes);
    }
    Ok(session_artifact(SessionParts {
        document,
        resources,
        findings: &package.findings,
        baseline: (!package.baseline.is_empty()).then_some(package.baseline),
        original: Some(Arc::new(original)),
        ocr: package.ocr,
        #[cfg(feature = "pdf-import")]
        ocr_detail: None,
        #[cfg(feature = "pdf-import")]
        reflow: None,
        layout: PdfLayout::Fixed,
    }))
}

/// Opens the PDF module's flowing package (the default open, and "Convert to
/// flowing document", `docs/182` §4.2, §18) as a flowing document: its
/// document, media and report. `original`, when the host passes the file the
/// package was made from, must be that file (same length and hash); it is
/// kept only so the host can open the PDF again (in its exact page layout,
/// or with OCR), never written back. There is no baseline: saved as PDF the
/// document is the semantic writer, reported. It does not convert again.
/// O(document + original).
///
/// # Errors
///
/// A malformed package, an original that is not the converted file, or a
/// document the snapshot importer refuses.
pub fn import_pdf_reflow_package(
    package: &[u8],
    original: Option<Vec<u8>>,
    limits: SnapshotLimits,
) -> Result<ImportArtifact, AdapterError> {
    let package = ImportPackage::decode(package).map_err(|e| AdapterError::new(e.to_string()))?;
    if let Some(original) = &original
        && (package.original_len != original.len() as u64
            || package.original_hash != fnv1a64(original))
    {
        return Err(AdapterError::new(
            "the PDF module's conversion is of a different file than the one being opened",
        ));
    }
    let document = Document::from_json(&package.document_json, limits)
        .map_err(|e| AdapterError::new(format!("the converted PDF's document: {e}")))?;
    let mut resources = DocumentResources::default();
    for (name, bytes) in package.resources {
        resources.insert(name, bytes);
    }
    Ok(session_artifact(SessionParts {
        document,
        resources,
        findings: &package.findings,
        baseline: None,
        original: original.map(Arc::new),
        ocr: package.ocr,
        #[cfg(feature = "pdf-import")]
        ocr_detail: None,
        #[cfg(feature = "pdf-import")]
        reflow: None,
        layout: PdfLayout::Flowing,
    }))
}

/// How the PDF-origin session `source` was opened; `None` when `source` is
/// not a PDF's. O(1).
#[must_use]
pub fn pdf_layout(source: &SourceEnvelope) -> Option<PdfLayout> {
    source.state::<PdfSourceState>().map(|state| state.layout)
}

/// What OCR did during the import a PDF-origin session came from, reduced to
/// what the editor shows: page classes, regions read and skipped, words kept,
/// and the recogniser's name. `None` when `source` is not PDF-origin. O(1).
#[must_use]
pub fn pdf_ocr_record(source: &SourceEnvelope) -> Option<&OcrSummary> {
    source.state::<PdfSourceState>().map(|state| &state.ocr)
}

/// The original bytes `source` retained, when it is a PDF session made with
/// retention: what the host imports again (in either layout, or with OCR).
/// Only a fixed-layout session's original is ever saved back
/// ([`export_pdf_session`]). O(1).
#[must_use]
pub fn pdf_retained_original(source: &SourceEnvelope) -> Option<&[u8]> {
    source
        .state::<PdfSourceState>()?
        .original
        .as_deref()
        .map(Vec::as_slice)
}

/// Saves a PDF-origin session as PDF (`docs/182` §7, P4):
///
/// - `Semantic`: `semantic(request)`, the real-text writer;
/// - `ExactIfUnchanged` on an unchanged session: the original, byte for byte;
/// - otherwise, a session whose content equals the import's: the original,
///   reported `pdf.export.original_written_back`;
/// - otherwise `planner`'s answer (`casual-doc-pdf-save`): the original back,
///   reported `pdf.export.original_written_back`, for an edit undone; an
///   incremental update when it is safe; with no planner, or when it is not
///   safe, `semantic(request)` plus `pdf.export.preserve_fallback.<reason>`.
///
/// O(document) at save.
///
/// # Errors
///
/// The semantic writer's.
pub fn export_pdf_session(
    request: ExportRequest<'_>,
    planner: Option<&dyn PdfSavePlanner>,
    semantic: &dyn Fn(ExportRequest<'_>) -> Result<ExportArtifact, AdapterError>,
) -> Result<ExportArtifact, AdapterError> {
    let state = request.source.and_then(|s| s.state::<PdfSourceState>());
    let original = state.and_then(|state| state.original.as_ref());
    let artifact = |bytes: Vec<u8>, report| ExportArtifact {
        bytes,
        report,
        format: FormatProfile {
            format: pdf_format(),
            version: None,
        },
        mime_type: "application/pdf".to_owned(),
        suggested_extension: "pdf".to_owned(),
    };
    if request.mode == ExportMode::Semantic {
        return semantic(request);
    }
    // A flowing document is a new document: its PDF is the semantic writer's,
    // and the report says why the original was not kept (`docs/182` §7).
    if state.is_some_and(|state| state.layout == PdfLayout::Flowing) {
        let mut exported = semantic(request)?;
        exported
            .report
            .entries
            .push(Fallback::new("flowing_layout").entry());
        exported.report.sort();
        return Ok(exported);
    }
    if request.mode == ExportMode::ExactIfUnchanged
        && request.source_unchanged
        && let Some(bytes) = original
    {
        return Ok(artifact(bytes.to_vec(), CompatibilityReport::default()));
    }
    // Edited (or asked to preserve): keep the original when that is safe.
    let fallback = match (original, state.and_then(|s| s.baseline.as_deref()), planner) {
        (Some(original), Some(baseline), Some(planner)) => {
            match plan_and_write(
                planner,
                request.document,
                request.resources,
                original,
                baseline,
            ) {
                Planned::WriteBack => {
                    let mut report = CompatibilityReport::default();
                    report.entries.push(CompatibilityEntry {
                        feature: ORIGINAL_WRITTEN_BACK.to_owned(),
                        occurrences: 1,
                        location: FeatureLocation::default(),
                        model_outcome: ModelOutcome::Mapped,
                        retention_outcome: RetentionOutcome::Preserved,
                    });
                    return Ok(artifact(original.to_vec(), report));
                }
                Planned::Written(bytes, report) => return Ok(artifact(bytes, report)),
                Planned::Fallback(fallback) => fallback,
            }
        }
        // The PDF module is not loaded (or the host has no planner): said,
        // not silently semantic.
        (Some(_), Some(_), None) => Fallback::new("pdf_module_unavailable"),
        _ => Fallback::no_retained_source(),
    };
    let mut exported = semantic(request)?;
    exported.report.entries.push(fallback.entry());
    exported.report.sort();
    Ok(exported)
}
