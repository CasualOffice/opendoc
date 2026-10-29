// SPDX-License-Identifier: Apache-2.0

//! The Rust core of `opendoc.ai` (`docs/178`, ADR-072): what an agent needs to
//! read a document by anchor and change it safely, over the same engine the
//! editor runs.
//!
//! - [`Session`]: one open document, its one commit choke point (the engine's
//!   capability and protection checks, then the transaction log), export with
//!   the loss gate, render, undo.
//! - [`read`]: outline, blocks (JSON or anchored Markdown), styles, tables,
//!   comments, tracked changes, search, retrieval chunks, validation. Paged,
//!   token-bounded, truncation always stated.
//! - [`anchor`]: `NodeId` plus a quote; refuses rather than guesses.
//! - [`plan`]: the closed edit-plan vocabulary, compiled to the closed op set;
//!   tracked suggestions by default through `casual_doc_review::lower`, the
//!   editor's own lowering; direct edits only under a profile that grants
//!   `edit`; preview equals apply.
//! - [`compare`]: budgeted structural comparison.
//! - [`editor`]: the live editor's entry point: a plan compiled to its
//!   operations and redline, committed by nobody (`docs/181` §4.4).
//!
//! This crate never calls a model, holds no credential, and opens no network
//! connection. The MCP server (`tools/opendoc-mcp`) is a thin adapter over it;
//! the Python and JavaScript packages will be too.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod anchor;
pub mod compare;
pub mod editor;
pub mod error;
// Own line (anti-conflict): heading levels and page labels for the reference
// commands (`docs/181` §12, lane references).
mod layout_facts;
pub mod limits;
pub mod plan;
pub mod profile;
mod project;
pub mod read;
pub mod session;
mod structure;
pub mod surface;
pub mod text;

pub use error::{AiError, AiResult};
pub use limits::Limits;
pub use profile::Profile;
/// The author every agent suggestion and comment carries unless an operator sets
/// another: the owner's answer to `docs/178` Q3 (2026-10-29). It is written into
/// `w:author`, so Word's review pane shows it too.
pub const AI_AUTHOR: &str = "Casual AI";

/// How a PDF session opens: flowing (the default) or its exact page layout
/// (`docs/182` §4).
#[cfg(feature = "pdf-import")]
pub use casual_doc_io::PdfLayout;
/// The host's OCR for PDF sessions (`docs/182` §20a), re-exported so a front
/// door (`opendoc-mcp`) configures it without naming `casual-doc-io`.
#[cfg(feature = "pdf-import")]
pub use casual_doc_io::{PdfOcrModels, PdfOcrModelsError, PdfOcrOptions, PdfOcrSummary, pdf_ocr};
pub use session::{Clock, ExportFormat, Session, SessionConfig};
pub use surface::Surface;
