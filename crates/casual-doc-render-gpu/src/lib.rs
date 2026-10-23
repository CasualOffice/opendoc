// SPDX-License-Identifier: Apache-2.0

//! Opt-in GPU backend for the OpenDoc display list (`docs/179`, ADR-073).
//!
//! `casual-doc-render` (tiny-skia) is the default backend and the golden
//! reference. This crate paints the SAME [`casual_doc_layout::display::DisplayList`]
//! through `vello_hybrid` on `wgpu`, and is checked against the reference
//! within the tolerance `tests/parity.rs` states. Nothing uses it unless a host
//! asks: the editor's default path does not depend on it.
//!
//! * [`encode`] — the display list as `vello_common` drawing commands, onto any
//!   [`encode::SparseCanvas`]. Always built; no GPU dependency.
//! * `gpu` feature — [`gpu::GpuRenderer`], the `vello_hybrid` painter.
//! * `sparse-cpu` feature — [`cpu::render_sparse_cpu`], the same encoder on
//!   `vello_cpu`, so the encoder is tested on a machine with no GPU.
//!
//! The rare families (colour glyphs, arrowheads, pictures) are not encoded a
//! second time: the reference renderer paints each into a tile that is
//! composited as an image (module `tile`), so they match the CPU page exactly.

#![forbid(unsafe_code)]

pub mod encode;
mod tile;

#[cfg(feature = "gpu")]
pub mod gpu;

#[cfg(feature = "sparse-cpu")]
pub mod cpu;

pub use encode::{EncodeStats, SparseCanvas, encode_display_list};
