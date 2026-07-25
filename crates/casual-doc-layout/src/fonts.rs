//! Bundled fonts shared by the shaper and the renderer.
//!
//! The layout engine registers these into an empty `fontique` collection (no
//! system fonts) for deterministic, WASM-safe layout. The renderer must extract
//! glyph outlines from the *same* bytes so shaping and rasterization agree on the
//! exact face; exposing the blobs here is the bridge until the fuller font
//! resolver (`P1C-002`, `40-FONT-MANAGEMENT-DESIGN.md`) formalizes `FontId`
//! resolution. See `fonts/README.md` for provenance and licensing.

/// Roboto Regular (Apache-2.0) — the current default face, `FontId(0)`.
pub const ROBOTO_REGULAR: &[u8] = include_bytes!("../fonts/Roboto-Regular.ttf");
