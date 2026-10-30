// SPDX-License-Identifier: Apache-2.0

//! # OpenDoc Native SDK
//!
//! Create, read, modify, template, convert and render word-processing
//! documents, with the engine's own page layout and a report of anything a
//! round trip would lose (`docs/176` §3.3 and §12).
//!
//! ```no_run
//! use opendoc::{Document, Format, Position, RunFormat, SaveMode};
//!
//! # fn main() -> opendoc::Result<()> {
//! let bytes = std::fs::read("in.docx").expect("a file");
//! let mut doc = Document::open(&bytes)?;
//! println!("{} findings at open", doc.import_report().findings.len());
//!
//! for paragraph in doc.paragraphs() {
//!     println!("{:?}: {}", paragraph.style, paragraph.text);
//! }
//!
//! doc.edit(|e| {
//!     let p = e.append_heading("Quarterly report", 1)?;
//!     e.set_run_format(&p, 0, 9, &RunFormat::italic())?;
//!     e.insert_table(&Position::End, 2, 3, &[])?;
//!     e.replace_all("ACME", "Contoso")?;
//!     e.accept_all_revisions()?;
//!     Ok(())
//! })?;
//!
//! let (docx, report) = doc.save(Format::Docx, SaveMode::PreserveWhenSafe)?;
//! assert!(report.is_lossless() || !report.losses.is_empty());
//! let pdf = doc.to_pdf()?;
//! let png = doc.render_page_png(0, 96.0)?;
//! # let _ = (docx, pdf, png);
//! # Ok(())
//! # }
//! ```
//!
//! **Design rules** (each is a test in this crate):
//!
//! - **No writable DOM.** Reads are a projection ([`view`]) with stable node
//!   ids; writes are [`Editor`] calls lowered to the engine's closed operation
//!   set and applied through its single choke point (ADR-030 I1), all or
//!   nothing per batch.
//! - **Loss is reported, never silent.** [`Document::import_report`] says what
//!   the open did not carry; every [`Document::save`] returns a [`SaveReport`]
//!   whose `losses` name what the file will not hold.
//! - **Deterministic pixels by default.** Layout uses the bundled faces (plus
//!   any the host registers), so a page renders the same on every machine.
//! - **One engine.** The page a PNG shows is the page the PDF prints and the
//!   page the editor and the Viewer SDK show.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod builder;
mod document;
pub mod edit;
mod error;
mod format;
pub mod report;
pub mod template;
pub mod view;
#[cfg(feature = "wasm")]
mod wasm;

pub use builder::DocumentSpec;
pub use document::{Document, OpenOptions, PageImage, PageSize, RenderOptions};
pub use edit::{EditCommand, Editor, Position, PropertiesPatch, RunFormat, Undo};
pub use error::{Error, ErrorCode, Result};
pub use format::{Format, SaveMode};
pub use report::{Finding, ImportReport, SaveReport};
pub use template::TemplateOptions;

/// The SDK's version (the engine's workspace version).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Converts `bytes` to `to` in one call: open (detecting the source format),
/// then save semantically. Returns the bytes and the save report.
///
/// # Errors
///
/// As [`Document::open`] and [`Document::save`].
pub fn convert(bytes: &[u8], to: Format) -> Result<(Vec<u8>, SaveReport)> {
    let document = Document::open(bytes)?;
    let mode = if document.source_format() == Some(to) {
        SaveMode::PreserveWhenSafe
    } else {
        SaveMode::Semantic
    };
    document.save(to, mode)
}

/// Re-exported for bindings that encode media as base64 on a JSON boundary.
#[doc(hidden)]
pub mod base64 {
    /// Standard base64 encoding.
    #[must_use]
    pub fn encode(bytes: &[u8]) -> String {
        crate::builder::base64_encode(bytes)
    }

    /// Standard base64 decoding (`None` for invalid input).
    #[must_use]
    pub fn decode(text: &str) -> Option<Vec<u8>> {
        crate::builder::base64_decode(text)
    }
}
