// SPDX-License-Identifier: Apache-2.0

//! Optical evidence for OpenDoc: text recognition and page-layout regions
//! from a raster, behind provider traits (`docs/182` P5/P6, `docs/131` §11).
//!
//! This crate knows nothing about PDF and builds no document (PDFR-I1, I5).
//! It turns a [`Raster`] into owned evidence — [`OcrLine`]s of [`OcrWord`]s
//! with pixel boxes and confidences, or [`LayoutRegion`]s — and records what
//! produced it ([`ProviderManifest`], PDFR-I8). `casual-doc-pdf-import`
//! decides where to rasterise and merges the result under native-first
//! precedence (PDFR-I2).
//!
//! **One Rust path, headless and in WebAssembly.** The default recognition
//! provider (feature `paddle`, owner decision 2026-10-29) runs PaddleOCR
//! PP-OCRv5 — Apache-2.0 weights, multilingual including CJK — on Robert
//! Knight's `rten` inference engine (MIT OR Apache-2.0, pure Rust), so the
//! Native SDK, the MCP server, CI and the browser run the same code — the
//! reason `docs/182` §3.1 chose hayro over PDF.js. The fallback provider
//! (feature `ocrs`) runs the `ocrs` pipeline on the same engine. The layout provider (feature
//! `layout-heron`) runs Docling's `docling-layout-heron` RT-DETRv2 export
//! (Apache-2.0) on the same engine.
//!
//! **Weights are the host's.** No model is committed, bundled or fetched by
//! this crate. A host passes model bytes it obtained under its own policy
//! (a file, a cache, a host `fetch` with an integrity hash); with none, a
//! provider cannot be constructed and the importer reports
//! `pdf.import.ocr_unavailable`. Nothing here opens a socket (PDFR-I6).
//! [`OcrModels`] is the typed set a host builds (from bytes, or natively from
//! a directory) and turns into providers.
//!
//! **Bounded and interruptible (PDFR-I7).** Every provider call takes
//! [`OcrLimits`] (pixels, lines, characters) and an [`Interrupt`] it polls
//! between model runs, so a host's Cancel or time budget stops it within one
//! model run. Time is the host's clock: `std::time::Instant` does not exist in
//! `wasm32-unknown-unknown`, so [`Deadline`] is native-only and a browser host
//! implements [`Interrupt`] over `performance.now()`.
//!
//! ```
//! use casual_doc_ocr::{CancelToken, Interrupt, Stop};
//! let cancel = CancelToken::new();
//! assert_eq!(cancel.poll(), None);
//! cancel.cancel();
//! assert_eq!(cancel.poll(), Some(Stop::Cancelled));
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod evidence;
mod interrupt;
pub mod layout;
mod models;
mod provider;
mod raster;

#[cfg(any(feature = "ocrs", feature = "layout-heron", feature = "paddle"))]
mod digest;
#[cfg(feature = "layout-heron")]
mod heron;
#[cfg(feature = "ocrs")]
mod ocrs_provider;
#[cfg(feature = "paddle")]
mod paddle;

pub use evidence::{Determinism, OcrBox, OcrLine, OcrPage, OcrWord, ProviderManifest};
#[cfg(not(target_family = "wasm"))]
pub use interrupt::Deadline;
pub use interrupt::{CancelToken, Interrupt, Never, Stop};
pub use layout::{
    HeuristicLayout, LayoutInput, LayoutProvider, LayoutRegion, RegionKind, TextLineHint,
};
pub use models::{OcrModels, OcrModelsError, RecognitionModel};
pub use provider::{OcrError, OcrLimits, OcrProvider};
pub use raster::Raster;

#[cfg(feature = "layout-heron")]
pub use heron::HeronLayout;
#[cfg(feature = "ocrs")]
pub use ocrs_provider::OcrsProvider;
#[cfg(feature = "paddle")]
pub use paddle::{PaddleOcr, RecognitionPack};
