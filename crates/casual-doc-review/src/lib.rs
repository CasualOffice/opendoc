// SPDX-License-Identifier: Apache-2.0

//! The one lowering of review edits: tracked suggestions, comments and their
//! decisions, shared by the editor (`casual-doc-wasm`) and the Native SDK and
//! MCP server (`docs/178` §8 prerequisite A1, ADR-072).

#![forbid(unsafe_code)]

pub mod body;
pub mod lower;
pub mod table_review;

pub use lower::{InsertStyle, Lowered, ReviewIds};
