//! Security-bounded DOCX package admission and on-demand part reads.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod archive;
mod contenttypes;
mod discovery;
mod error;
mod limits;
mod package;
mod path;
mod relationships;

#[cfg(test)]
mod tests;

pub use error::PackageError;
pub use limits::PackageLimits;
pub use package::{
    CancellationToken, DocxPackage, PackageEntry, PartCompression, PartManifestEntry,
    SourcePackageSnapshot,
};
pub use relationships::{DocumentRelationship, TargetMode};
