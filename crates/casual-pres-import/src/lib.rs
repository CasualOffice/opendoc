// SPDX-License-Identifier: Apache-2.0

//! PresentationML semantic import: a `.pptx` package into
//! [`casual_pres_model::Presentation`].
//!
//! # What this is
//!
//! A vertical slice, deliberately deep rather than broad. It opens a real deck —
//! container, OPC graph, slide order, the three inheritance tiers, the shape tree
//! with geometry and fills, the placeholder slots, and DrawingML text in full —
//! and it says precisely what it does not cover. [`import_pptx`]'s module
//! documentation enumerates both halves of that, families rather than successes
//! (`SKILL` §9.3).
//!
//! ```no_run
//! use casual_doc_package::PackageLimits;
//! use casual_pres_import::{ImportLimits, import_pptx};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let bytes = std::fs::read("deck.pptx")?;
//! let imported = import_pptx(&bytes, PackageLimits::default(), ImportLimits::default())?;
//!
//! // Slide order is `p:sldIdLst`'s order, which is the order the deck is shown
//! // in — not the order the slide parts happen to be named in.
//! for slide in imported.presentation.slides() {
//!     println!("{:?}", slide.name);
//! }
//!
//! // The fidelity report is not a side channel: it is what makes "direct OOXML"
//! // an advantage over a converter rather than a claim.
//! for entry in &imported.report.entries {
//!     println!("{} x{} {:?}", entry.feature, entry.occurrences, entry.disposition);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Two things it is not
//!
//! **It is not a round trip.** There is no PresentationML writer in this
//! repository, so nothing retains the source bytes and no finding in the report
//! can claim `preserved`. Every entry reads `not-retained`, `rejected` or
//! `degraded`, which is the honest reading of today's pipeline rather than a
//! pessimistic one — see `loss.rs`.
//!
//! **It does not render text.** `casual-pres-layout` consumes a validated
//! [`Presentation`](casual_pres_model::Presentation) and turns its shapes into a
//! display list, so a deck imported here can be laid out. The placeholder
//! *text* cascade — the chain that gives a run with no stated size its font size,
//! through the layout, the master's `p:txStyles` and `p:defaultTextStyle` — is
//! not built, and this importer reads neither of the last two tiers. So the text
//! model arrives complete and nothing yet draws it (`SKILL` §9.4: modelled is not
//! shipped).
//!
//! # Where the security boundary is
//!
//! The ZIP container is admitted by `casual_doc_package::BoundedPackage` — the
//! one bounded ZIP reader in the repository, carrying the zip-bomb ratio, the
//! entry ceiling, path normalization, and the overlap, symlink and encryption
//! refusals. This crate adds no second one. It does add a second **OPC** reader,
//! because `casual-doc-ooxml`'s is `pub(crate)` and hard-wired to
//! WordprocessingML content types; `opc.rs` says so explicitly and names the fix.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod color;
mod error;
mod ids;
mod import;
mod limits;
mod loss;
mod media;
mod opc;
mod parts;
mod shapes;
mod text;
mod xml;

pub use error::ImportError;
pub use import::{ImportedPresentation, import_pptx};
pub use limits::ImportLimits;

#[cfg(test)]
mod tests;
