// SPDX-License-Identifier: Apache-2.0

//! The one capability registry (`docs/181`, ADR-075).
//!
//! A feature reaches the editor and every agent front door through this crate:
//!
//! - [`registry`]: every agent-facing command, declared once — its id, its
//!   family, the operations it may emit, whether it can be a tracked
//!   suggestion, and a worked example. `casual-doc-ai` parses edit plans
//!   against it, and `opendoc-mcp` publishes it.
//! - [`coverage`]: every [`casual_doc_edit::Operation`] variant classified as
//!   reached by a command or not exposed, with a reason. The classification is
//!   an exhaustive `match`, so a new operation does not compile until someone
//!   decides whether agents get it.
//! - [`edit`]: the vocabulary as types; [`schema`]: the JSON Schema derived
//!   from them, published by every front door.
//! - the lowerings the editor and agents share, one module per family:
//!   [`table`], [`format`] (character formatting), [`paragraph`], [`lists`],
//!   [`rewrite`] (the `rewrite_block` dialect), [`links`] (hyperlinks and
//!   bookmarks), [`fields`], [`objects`] (pictures and shapes), [`notes`],
//!   [`breaks`] and [`sections`] (page setup and running content).
//!
//! Every lowering here is pure: it reads the document and returns operations.
//! Nothing in this crate applies an operation or builds a transaction; the
//! callers' one choke point does (ADR-030 I1, `docs/147`).

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod breaks;
pub mod charts;
pub mod coverage;
pub mod edit;
pub mod fields;
pub mod format;
pub mod links;
pub mod lists;
pub mod notes;
pub mod objects;
pub mod paragraph;
pub mod registry;
pub mod rewrite;
pub mod schema;
pub mod sections;
pub mod table;

pub use registry::{COMMANDS, Command, Family, Trackable, find, ids};
