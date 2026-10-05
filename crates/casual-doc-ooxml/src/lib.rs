// SPDX-License-Identifier: Apache-2.0

//! Security-bounded DOCX package admission and on-demand part reads.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod contenttypes;
mod discovery;
mod error;
mod package;
mod path;
mod recover;
mod relationships;

#[cfg(test)]
mod tests;

pub use casual_doc_package::{CancellationToken, PackageEntry, PackageLimits, PartCompression};
pub use error::PackageError;
// Own line, kept out of any sorted block (the repo's parallel-PR rule).
pub use recover::{PackageRepair, repair_archive};
pub use package::{DocxPackage, PartManifestEntry, SourcePackageSnapshot};
pub use relationships::{DocumentRelationship, TargetMode};
