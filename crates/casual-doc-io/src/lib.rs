// SPDX-License-Identifier: Apache-2.0

//! Format-neutral document import/export contracts and deterministic dispatch.
//!
//! This crate is the adapter boundary described by doc 94. It does not parse or
//! write document formats itself: registered adapters map source bytes to the
//! normalized v1 model and back. Built-in adapters cover DOCX, bounded ODT,
//! normalized JSON, UTF-8 plain text and export-only CommonMark and single-file
//! HTML, each with an
//! explicit capability descriptor, plus an opt-in export-only PDF adapter
//! ([`register_pdf_exporter`]).

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
    CompatibilityEntry, CompatibilityReport, Disposition, DispositionViolation, FeatureLocation,
    Finding, LedgerId, LedgerRecord, LossReporter, ModelOutcome, PartConstructDisposition,
    PartDisposition, PreservationKind, PreservationLedger, RetentionOutcome, SourceRetention,
};
// Own line, kept out of any sorted block (the repo's parallel-PR rule).
pub use report::{RecoveryReport, RepairSeverity, SourceRepair};
pub use rtf::RtfAdapter;
pub use text::{PlainTextAdapter, PlainTextLimits};
