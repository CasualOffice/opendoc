// SPDX-License-Identifier: Apache-2.0

//! Security-bounded DOCX package admission and on-demand part reads.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod contenttypes;
mod discovery;
mod error;
mod package;
mod path;
/// Derivation of the committed preset-geometry table, shared by the generator and
/// the guard that checks its output.
pub mod preset_table;
mod relationships;

#[cfg(test)]
mod tests;

pub use casual_doc_package::{CancellationToken, PackageEntry, PackageLimits, PartCompression};
pub use error::PackageError;
pub use package::{DocxPackage, PartManifestEntry, SourcePackageSnapshot};
pub use relationships::{DocumentRelationship, TargetMode};
