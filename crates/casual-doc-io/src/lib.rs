//! Format-neutral document import/export contracts and deterministic dispatch.
//!
//! This crate is the adapter boundary described by doc 94. It does not parse or
//! write document formats itself: registered adapters map source bytes to the
//! normalized v1 model and back. Built-in adapters cover DOCX, bounded ODT,
//! normalized JSON, and UTF-8 plain text with explicit capability descriptors.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod artifact;
mod docx;
mod error;
mod format;
mod normalized_json;
mod odt;
mod registry;
mod report;
mod text;

pub use artifact::{
    DocumentResources, ExportArtifact, ExportMode, ExportRequest, FormatProfile, ImportArtifact,
    ImportRequest, SourceEnvelope,
};
pub use docx::{DocxAdapter, builtin_registry, builtin_registry_with_package_limits};
pub use error::{AdapterError, IoError};
pub use format::{FormatDescriptor, FormatId, FormatIdError, formats};
pub use normalized_json::NormalizedJsonAdapter;
pub use odt::OdtAdapter;
pub use registry::{
    DetectionRequest, FormatExporter, FormatImporter, FormatRegistry, FormatSelection,
    ProbeConfidence, ProbeRequest, ProbeResult,
};
pub use report::{
    CompatibilityEntry, CompatibilityReport, FeatureLocation, ModelOutcome, RetentionOutcome,
};
pub use text::{PlainTextAdapter, PlainTextLimits};
