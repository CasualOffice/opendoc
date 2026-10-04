//! Format-neutral compatibility reporting.
//!
//! This module used to declare a **second** taxonomy: a two-field
//! `(model_outcome, retention_outcome)` pair that every adapter converted its own
//! findings into, and that the DOCX adapter converted the importer's
//! `35-DISPOSITION-TAXONOMY.md` vocabulary *down* into. The conversion was lossy
//! in three ways, and the third is the one that mattered:
//!
//! 1. **The nine-pair invariant was gone.** `35` admits nine of the fifteen axis
//!    pairs and calls the other six "an internal error [that] must fail import,
//!    not be reported". A struct with one field per axis makes all fifteen
//!    representable, and the repository was in fact publishing one of the six:
//!    `casual-doc-odf` reports `odf.draw.image-missing-part` as `degraded` +
//!    `not-applicable` (`casual-doc-odf/src/package.rs`), which this layer
//!    carried straight through to hosts. `Disposition` makes that unrepresentable
//!    rather than merely wrong.
//! 2. **The whole-part axis was gone.** A finding about an admitted package part
//!    the model does not consume carries the part's name *and its declared
//!    content type*, which is what makes the disposition auditable. The old entry
//!    had nowhere to put it.
//! 3. **The preservation ledger never crossed the boundary.** The DOCX importer
//!    builds one, validates every `preserved` claim against it, and hands it back
//!    — and this layer dropped it on the floor, so a host reading the neutral
//!    report could see the word `preserved` and had no way to audit it. `35`
//!    recorded that as a known limitation. Meanwhile the ODT adapter, with no
//!    ledger reachable at all, was upgrading `not-retained` to `preserved`
//!    whenever the source happened to be retained — a preservation claim on the
//!    honour system.
//!
//! So there is now one taxonomy, in `casual-doc-loss`, and this module is the
//! re-export that keeps `casual_doc_io::CompatibilityReport` the name adapters
//! and hosts already use. `ImportArtifact` and `ExportArtifact` carry the
//! [`PreservationLedger`] beside the report, and every adapter validates its own
//! report against its own ledger before returning it, so a `preserved` entry
//! that nothing retains fails the import instead of reaching a host.

pub use casual_doc_loss::{
    CompatibilityEntry, CompatibilityReport, Disposition, DispositionViolation, FeatureLocation,
    Finding, LedgerId, LedgerRecord, LossReporter, ModelOutcome, PartConstructDisposition,
    PartDisposition, PreservationKind, PreservationLedger, RetentionOutcome, SourceRetention,
};
