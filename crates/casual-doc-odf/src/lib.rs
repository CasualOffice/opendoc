//! Security-bounded OpenDocument package admission and semantic import.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod content;
mod error;
mod export;
mod manifest;
mod master_page;
mod metadata;
mod package;
mod page_style;

#[cfg(test)]
mod content_tests;
#[cfg(test)]
mod tests;

pub use content::{
    CompatibilityEntry, CompatibilityReport, ModelOutcome, OdfImportLimits, OdtImport,
    RetentionOutcome, import_content_xml, import_content_xml_with_cancellation,
};
pub use error::OdfError;
pub use export::{OdfExportLimits, OdtExport, write_odt, write_odt_with_retained_parts};
pub use manifest::ManifestEntry;
pub use package::{
    CONTENT_PART, MANIFEST_PART, META_PART, MIMETYPE_PART, ODT_MIME, OdfPackageLimits,
    OdfRetainedParts, OdfVersion, OdtPackage, RetainedPart, STYLES_PART,
};
