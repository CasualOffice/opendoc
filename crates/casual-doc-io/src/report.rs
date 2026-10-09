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

/// How much document meaning a best-effort open's repair put at risk.
///
/// A host ranks its notice off this rather than off an adapter's own repair
/// vocabulary, so a format adapter can name a new kind of damage without every
/// host learning the name.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RepairSeverity {
    /// Packaging structure was rebuilt or inferred. The same text, formatting
    /// and layout are shown.
    Structural,
    /// Some of the document was dropped — one part, one damaged construct, or
    /// the tail of the content after the damage.
    ContentDropped,
    /// None of the document body could be read.
    BodyLost,
}

/// One repair a best-effort open applied to damaged source, format-neutrally.
///
/// Two fields carry the same fact for two different audiences, and both are
/// required: `token` is what a host keys a translation or a metric off, and
/// `summary` is the sentence a reader actually gets. Shipping only the token is
/// how an internal name ends up in front of a reader; shipping only the sentence
/// is how a host loses any ability to localise or group.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceRepair {
    /// Stable adapter-defined token for this kind of repair.
    pub token: String,
    /// One sentence a reader can act on, with no internal vocabulary in it.
    pub summary: String,
    /// How much document meaning the repair put at risk.
    pub severity: RepairSeverity,
    /// The source part the repair is charged to, when the adapter knows one.
    pub part_name: Option<String>,
    /// How many times this repair was applied. Saturating.
    pub occurrences: u32,
}

/// What a best-effort open had to repair before it could produce a document.
///
/// Empty for every well-formed source, which is what makes it usable: a host can
/// show a notice exactly when this is non-empty, without having to decide which
/// of a healthy document's compatibility findings are worth interrupting for.
///
/// This is deliberately **not** folded into [`CompatibilityReport`]. That report
/// answers "what did this document mean that the model could not carry?" — a
/// fidelity question about a well-formed file. This one answers "what was wrong
/// with the bytes?" A reader must see the second before saving over the original,
/// and a list that mixes the two cannot be used that way.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RecoveryReport {
    /// Repairs in the adapter's deterministic order.
    pub repairs: Vec<SourceRepair>,
}

impl RecoveryReport {
    /// Whether the open had to repair anything.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.repairs.is_empty()
    }

    /// The worst severity any repair reached, or `None` when nothing was
    /// repaired.
    #[must_use]
    pub fn severity(&self) -> Option<RepairSeverity> {
        self.repairs.iter().map(|repair| repair.severity).max()
    }
}
