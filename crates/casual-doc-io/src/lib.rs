// SPDX-License-Identifier: Apache-2.0

//! Format-neutral document import/export contracts and deterministic dispatch.
//!
//! This crate is the adapter boundary described by doc 94. It does not parse or
//! write document formats itself: registered adapters map source bytes to the
//! normalized v1 model and back. The built-in registry ([`builtin_registry`])
//! covers DOCX, bounded ODT, normalized JSON and UTF-8 plain text both ways,
//! RTF import, and export-only DOTX, CommonMark, single-file HTML and PDF
//! ([`register_pdf_exporter`]), each with an explicit capability descriptor.
//! PDF import is a separate, explicit capability (feature `pdf-import`, then
//! `enable_pdf_import`): the built-in registry never imports PDF.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod artifact;
mod docx;
mod dotx;
mod error;
mod format;
mod html;
mod markdown;
mod normalized_json;
mod odt;
mod pdf;
#[cfg(feature = "pdf-import")]
mod pdf_import;
#[cfg(feature = "pdf-source")]
mod pdf_preserve;
#[cfg(feature = "pdf-source")]
mod pdf_source;
mod registry;
mod report;
mod rtf;
mod text;

pub use artifact::{
    DocumentResources, ExportArtifact, ExportMode, ExportRequest, FormatProfile, ImportArtifact,
    ImportRequest, SourceEnvelope,
};
pub use docx::{
    DocxAdapter, builtin_registry, builtin_registry_with_format_limits,
    builtin_registry_with_limits, builtin_registry_with_package_limits,
};
pub use dotx::{DOTX_MIME, DotxAdapter};
pub use error::{AdapterError, IoError};
pub use format::{FormatDescriptor, FormatId, FormatIdError, formats};
pub use html::{HTML_MIME, HtmlAdapter, HtmlLimits};
pub use markdown::{MARKDOWN_MIME, MarkdownAdapter, MarkdownLimits};
pub use normalized_json::NormalizedJsonAdapter;
pub use odt::OdtAdapter;
pub use pdf::{PdfAdapter, register_pdf_exporter};
pub use registry::{
    DetectionRequest, FormatExporter, FormatImporter, FormatRegistry, FormatSelection,
    ProbeConfidence, ProbeRequest, ProbeResult,
};
pub use report::{
    CompatibilityEntry, CompatibilityReport, FeatureLocation, ModelOutcome, RetentionOutcome,
};
// Own line, kept out of any sorted block (the repo's parallel-PR rule).
pub use report::{RecoveryReport, RepairSeverity, SourceRepair};
pub use rtf::RtfAdapter;
// Own lines, kept out of any sorted block (the repo's parallel-PR rule).
/// The OCR engine behind PDF import: provider traits, evidence, rasters and
/// the model set, for a host that implements its own provider.
#[cfg(feature = "pdf-import")]
pub use casual_doc_pdf_import::casual_doc_ocr as pdf_ocr;
/// The host's OCR for PDF import (`docs/182` §20a): the options, the model
/// set they are built from, the provider seam and what an import reports.
#[cfg(feature = "pdf-import")]
pub use casual_doc_pdf_import::{
    OcrModels as PdfOcrModels, OcrModelsError as PdfOcrModelsError, OcrOptions as PdfOcrOptions,
    OcrProvider as PdfOcrProvider, OcrSummary as PdfOcrSummary,
    OcrTextPlacement as PdfOcrTextPlacement, ProviderManifest as PdfOcrManifest,
    RecognitionModel as PdfRecognitionModel, ScanBehindText as PdfScanBehindText,
};
#[cfg(feature = "pdf-import")]
pub use casual_doc_pdf_import::{PdfReadLimits, Progress as PdfImportProgress};
#[cfg(feature = "pdf-source")]
pub use casual_doc_pdf_wire as pdf_wire;
#[cfg(feature = "pdf-import")]
pub use pdf_import::InProcessSavePlanner;
#[cfg(feature = "pdf-import")]
pub use pdf_import::{
    PdfImportAdapter, PdfStagedImport, builtin_registry_with_pdf_import, enable_pdf_import,
    enable_pdf_import_with_ocr, pdf_ocr_summary,
};
#[cfg(feature = "pdf-source")]
pub use pdf_source::import_pdf_reflow_package;
#[cfg(feature = "pdf-source")]
pub use pdf_source::pdf_ocr_record;
#[cfg(feature = "pdf-source")]
pub use pdf_source::{
    PdfSavePlanner, export_pdf_session, import_pdf_package, pdf_retained_original,
    pdf_session_artifact,
};
// Own line: "Convert to flowing document" from a PDF import (`docs/182` §18).
#[cfg(feature = "pdf-import")]
pub use pdf_import::pdf_reflow;
pub use text::{PlainTextAdapter, PlainTextLimits};
