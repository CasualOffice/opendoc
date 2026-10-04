// SPDX-License-Identifier: Apache-2.0

//! PresentationML semantic import: a `.pptx` package into
//! [`casual_pres_model::Presentation`].
//!
//! # What this is
//!
//! A vertical slice, deliberately deep rather than broad. It opens a real deck —
//! container, OPC graph, slide order, the three inheritance tiers, the shape tree
//! with geometry and fills, the placeholder slots, DrawingML text in full, and the
//! theme its colours and fonts resolve against — and it says precisely what it
//! does not cover. [`import_pptx`]'s module documentation enumerates both halves
//! of that, families rather than successes (`SKILL` §9.3).
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
//! # Three things it is not
//!
//! **It is not a round trip.** There is no PresentationML writer in this
//! repository, so nothing retains the source bytes and no finding in the report
//! can claim `preserved`. Every entry reads `not-retained`, `rejected` or
//! `degraded`, which is the honest reading of today's pipeline rather than a
//! pessimistic one — see `loss.rs`.
//!
//! **Text arrives whole.** Every TIER of the placeholder cascade is read — the
//! shape's `a:lstStyle`, the placeholder's on the layout and the master, the
//! master's `p:txStyles` and the presentation's `p:defaultTextStyle` —
//! `TextCascade::resolve` folds them into one effective property set per outline
//! level, and `casual-pres-layout` shapes that into glyph runs. What this reader
//! still does not carry is enumerated in the loss report rather than here, because
//! a module header that lists gaps goes stale and the report cannot.
//!
//! (That paragraph said "no text is drawn" while this reader was being written, and
//! it was true then. The shaping lane landed in parallel, which is the ordinary
//! hazard of a prose claim about a NEIGHBOURING crate — so this one now states what
//! THIS crate does and points at the report for the rest.)
//!
//! **It does not paint every themed appearance.** The theme part IS read — the
//! twelve `a:clrScheme` slots, the `p:clrMap` that binds them, the
//! `a:fontScheme` and the modelled subset of `a:fmtScheme` — so a shape stating
//! `a:solidFill/a:schemeClr` arrives with a concrete colour. A `p:style`
//! reference is kept AS a reference in `Definitions::shape_styles` with only its
//! `a:phClr` argument resolved, because folding the theme entry into the shape's
//! own fill would turn inheritance into authorship.
//!
//! Resolving the entry is `casual-doc-layout`'s job, and it already does it for
//! BOTH document classes: `place_group_child_tree` — the one recursion
//! `casual-pres-layout` drives a slide through — resolves each shape's
//! `Definitions::shape_styles` entry against `Definitions::format_scheme`, which
//! this importer now populates. So a themed fill and a themed outline reach a
//! slide's display list through the same code that paints them in a document.
//! That is read from the shared call path rather than measured here: this crate
//! does not depend on either layout crate, so nothing in these guards can assert
//! a paint item, and the assertion belongs beside the existing
//! `imported_deck.rs` seam.
//!
//! What still does not paint, and is reported per reference with the reason:
//! an `a:effectRef` (there is no shadow, glow or soft-edge primitive anywhere in
//! this build), a pattern fill entry, a non-solid outline entry, `a:fontRef`
//! (neither the collection nor its colour has a field), and `p:bgRef`, whose
//! `a:bgFillStyleLst` is deliberately not modelled.
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
mod theme;
mod xml;

pub use error::ImportError;
pub use import::{ImportedPresentation, import_pptx};
pub use limits::ImportLimits;

#[cfg(test)]
mod tests;
