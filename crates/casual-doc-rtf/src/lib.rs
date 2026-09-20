//! Security-bounded Rich Text Format (RTF) admission and semantic import.
//!
//! RTF is not a package: it is a single brace-delimited stream of backslash
//! control words, so none of the ZIP-shaped defences the DOCX and ODT adapters
//! rely on apply here. This crate supplies the equivalents — group-depth,
//! stream-length, declared-binary-length and expansion bounds — and maps the
//! admitted subset into `casual_doc_model::v1`.
//!
//! The supported, degraded, and dropped construct families are enumerated in
//! `docs/110-RTF-IMPORT-PROFILE.md`. Anything this crate does not map produces
//! a compatibility finding; nothing is dropped silently.
//!
//! ```
//! use casual_doc_rtf::{RtfLimits, import_rtf};
//!
//! let imported = import_rtf(br"{\rtf1\ansi Hello\par}", RtfLimits::default())?;
//! assert_eq!(imported.document.body().len(), 1);
//! # Ok::<(), casual_doc_rtf::RtfError>(())
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod builder;
mod code_page;
mod error;
mod import;
mod lexer;
mod limits;
mod report;
mod tables;

#[cfg(test)]
mod tests;

pub use code_page::{CodePage, code_page};
pub use error::RtfError;
pub use import::{RtfImport, import_rtf, probe_rtf};
pub use lexer::is_rtf;
pub use limits::RtfLimits;
pub use report::{
    RtfCompatibilityEntry, RtfCompatibilityReport, RtfModelOutcome, RtfRetentionOutcome,
};

/// The MIME type registered for Rich Text Format.
pub const RTF_MIME: &str = "application/rtf";
