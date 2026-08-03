//! Format-neutral, security-bounded ZIP package admission and part reads.
//!
//! This crate validates only the ZIP container. OPC, ODF, and other document
//! package profiles remain in their own crates and run after this admission
//! boundary.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod archive;
mod error;
mod limits;
mod package;
mod path;

#[cfg(test)]
mod tests;

pub use error::PackageError;
pub use limits::PackageLimits;
pub use package::{BoundedPackage, CancellationToken, PackageEntry, PartCompression};
