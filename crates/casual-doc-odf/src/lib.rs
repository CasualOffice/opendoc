//! Security-bounded OpenDocument package admission and semantic import.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod error;
mod manifest;
mod package;

#[cfg(test)]
mod tests;

pub use error::OdfError;
pub use manifest::ManifestEntry;
pub use package::{
    CONTENT_PART, MANIFEST_PART, MIMETYPE_PART, ODT_MIME, OdfPackageLimits, OdfVersion, OdtPackage,
};
