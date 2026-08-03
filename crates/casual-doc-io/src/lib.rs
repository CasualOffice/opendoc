//! Format-neutral document import/export contracts and deterministic dispatch.
//!
//! This crate is the adapter boundary described by doc 94. It does not parse or
//! write document formats itself: registered adapters map source bytes to the
//! normalized v1 model and back. The first built-in adapter delegates to the
//! existing DOCX package, importer, and semantic writer without changing them.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod artifact;
mod docx;
mod error;
mod format;
mod registry;
mod report;

pub use artifact::{
    DocumentResources, ExportArtifact, ExportMode, ExportRequest, FormatProfile, ImportArtifact,
    ImportRequest, SourceEnvelope,
};
pub use docx::{DocxAdapter, builtin_registry};
pub use error::{AdapterError, IoError};
pub use format::{FormatDescriptor, FormatId, FormatIdError, formats};
pub use registry::{
    DetectionRequest, FormatExporter, FormatImporter, FormatRegistry, FormatSelection,
    ProbeConfidence, ProbeRequest, ProbeResult,
};
pub use report::{
    CompatibilityEntry, CompatibilityReport, FeatureLocation, ModelOutcome, RetentionOutcome,
};
