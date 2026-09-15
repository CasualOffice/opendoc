//! The bounded, deterministic compatibility report, and the preservation ledger
//! that licenses its `preserved` claims.
//!
//! This module is the single home of the `35-DISPOSITION-TAXONOMY.md`
//! vocabulary. `casual-doc-export` re-exports [`ModelOutcome`],
//! [`RetentionOutcome`], [`Disposition`] and [`FeatureLocation`] from here
//! rather than redeclaring them, so the import and export halves of the same
//! format cannot drift into two spellings of one fact (FID-R-02).
//!
//! Three properties are deliberate.
//!
//! - **Only unrecovered meaning is enumerated.** A construct the model captures
//!   in full raises nothing, so an ordinary document imports with an *empty*
//!   report. A report that fires on healthy documents is one every caller learns
//!   to ignore, and an ignored report is worse than none. The corollary is that
//!   [`ModelOutcome::Mapped`] is unreachable in an import report by
//!   construction, not by omission: a mapped construct has no unrecovered
//!   meaning, so there is nothing to enumerate. `35` states this rule.
//! - **The disposition pair is chosen per construct, never per import mode.**
//!   Each call site states what happened to *that* construct as a `Finding`,
//!   and the report resolves it against what retains the source. Within a single
//!   `Semantic` import that yields `not-retained`, `rejected`, `blocked` and
//!   `preserved` on different constructs — the distinction `35` exists to draw,
//!   and which a per-mode constant could not express.
//! - **Only the nine legal pairs are representable.** `35` says any other
//!   pairing "is an internal error and must fail import, not be reported";
//!   naming the legal pairs ([`Disposition`]) makes the illegal ones impossible
//!   to construct, which is stronger than checking for them afterwards. What is
//!   left to validate is the preservation rule, which a type cannot enforce:
//!   `preserved` is legal *only* when the entry references a validated
//!   [`PreservationLedger`] record ([`CompatibilityReport::validate`]).

use std::collections::BTreeMap;

/// Distinct-feature ceiling; excess folds into an `(overflow)` bucket.
const MAX_REPORT_FEATURES: usize = 4_096;

/// Stable class identifier for revision-save IDs (`w:rsid*` attributes and the
/// `w:rsids`/`w:rsid` table in `settings.xml`). The reasoning for reporting the
/// class once per document rather than per construct is on `Reporter::report_rsid`
/// and in `35-DISPOSITION-TAXONOMY.md`.
pub const RSID_CLASS_FEATURE: &str = "docx.rsid";

/// How a construct was represented in the model.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ModelOutcome {
    /// Fully represented.
    Mapped,
    /// Partially represented.
    Degraded,
    /// Not represented.
    Omitted,
}

/// What happened to source detail the model did not consume.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RetentionOutcome {
    /// Retained in a validated preservation record.
    Preserved,
    /// Intentionally and reportably dropped (no record).
    NotRetained,
    /// Retention refused by policy.
    Blocked,
    /// Structurally invalid or over-limit.
    Rejected,
    /// No unconsumed remainder.
    NotApplicable,
}

/// One of the nine per-construct dispositions `35-DISPOSITION-TAXONOMY.md`
/// admits, as a single value.
///
/// The taxonomy is two orthogonal axes, but only nine of their fifteen pairs are
/// meaningful. Naming the legal pairs makes the illegal ones impossible to
/// construct, which is stronger than checking for them: there is no code path on
/// which an import or an export can emit `mapped` + `rejected`.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Disposition {
    /// Fully understood; nothing left to retain.
    MappedComplete,
    /// Fully understood; incidental source detail also kept for exact save.
    MappedPreserved,
    /// Partially understood; the remainder is kept verbatim.
    DegradedPreserved,
    /// Partially understood; the remainder is reportably dropped.
    DegradedNotRetained,
    /// Partially understood; the remainder was refused by policy.
    DegradedBlocked,
    /// Not modeled, but retained verbatim for save or inspection.
    OmittedPreserved,
    /// Not modeled and reportably dropped.
    OmittedNotRetained,
    /// Not modeled; retention refused by policy.
    OmittedBlocked,
    /// Structurally invalid or over-limit; reported, not modeled, not retained.
    OmittedRejected,
}

impl Disposition {
    /// This disposition's axis-A (model) outcome.
    #[must_use]
    pub const fn model_outcome(self) -> ModelOutcome {
        match self {
            Self::MappedComplete | Self::MappedPreserved => ModelOutcome::Mapped,
            Self::DegradedPreserved | Self::DegradedNotRetained | Self::DegradedBlocked => {
                ModelOutcome::Degraded
            }
            Self::OmittedPreserved
            | Self::OmittedNotRetained
            | Self::OmittedBlocked
            | Self::OmittedRejected => ModelOutcome::Omitted,
        }
    }

    /// This disposition's axis-B (retention) outcome.
    #[must_use]
    pub const fn retention_outcome(self) -> RetentionOutcome {
        match self {
            Self::MappedComplete => RetentionOutcome::NotApplicable,
            Self::MappedPreserved | Self::DegradedPreserved | Self::OmittedPreserved => {
                RetentionOutcome::Preserved
            }
            Self::DegradedNotRetained | Self::OmittedNotRetained => RetentionOutcome::NotRetained,
            Self::DegradedBlocked | Self::OmittedBlocked => RetentionOutcome::Blocked,
            Self::OmittedRejected => RetentionOutcome::Rejected,
        }
    }

    /// Whether this disposition claims the unconsumed remainder is retained, and
    /// therefore requires a validated preservation-ledger reference.
    #[must_use]
    pub const fn claims_preservation(self) -> bool {
        matches!(self.retention_outcome(), RetentionOutcome::Preserved)
    }
}

/// Bounded location of a compatibility finding.
///
/// Element and attribute names are XML **local** names. The namespace *prefix* a
/// producer chose is deliberately not recorded: it is arbitrary (`w14:paraId`
/// and `x:paraId` are the same attribute if both prefixes bind the same URI), so
/// a prefix in a stable feature identifier would make the identifier a property
/// of the writer rather than of the construct.
///
/// `part_name` is populated where the importer genuinely knows it — whole-part
/// dispositions, which are enumerated from the package manifest. Element and
/// attribute findings leave it `None`, because the parsers are handed part
/// *bytes* rather than part names, and a conventional name (`word/styles.xml`)
/// would be an invented location that a package need not use. An invented
/// location is worse than none.
#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct FeatureLocation {
    /// Normalized package part the finding is charged to, when known.
    pub part_name: Option<String>,
    /// XML local name of the element the finding is about.
    pub element: Option<String>,
    /// XML local name of the attribute, when the finding is about an attribute
    /// of `element` rather than the element itself.
    pub attribute: Option<String>,
}

impl FeatureLocation {
    /// A location naming only an element.
    fn element(local: &[u8]) -> Self {
        Self {
            part_name: None,
            element: Some(String::from_utf8_lossy(local).into_owned()),
            attribute: None,
        }
    }

    /// A location naming an attribute of an element.
    fn attribute(element: &[u8], attribute: &[u8]) -> Self {
        Self {
            part_name: None,
            element: Some(String::from_utf8_lossy(element).into_owned()),
            attribute: Some(String::from_utf8_lossy(attribute).into_owned()),
        }
    }
}

/// Identifier of a [`PreservationLedger`] record, referenced by every report
/// entry whose retention outcome is `preserved`.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct LedgerId(u32);

impl LedgerId {
    /// The record's stable, deterministic index within its ledger.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Where a ledger record's retained bytes live.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreservationKind {
    /// The verbatim source snapshot — `Retention` mode's tier-1 byte floor,
    /// which reproduces the import input exactly.
    SourceSnapshot,
    /// An admitted package part carried verbatim through the semantic writer via
    /// the opaque side-table.
    OpaquePart,
    /// A source subtree retained inside the model (not as a package part) and
    /// re-emitted verbatim on save — for example an OMML equation with no typed
    /// projection.
    ModelSubtree,
}

/// One validated record that source detail the model did not consume is retained
/// and recoverable.
///
/// `35` permits a `preserved` retention outcome **only** when the report
/// references such a record: "emitting a warning without retaining the declared
/// content is `not-retained`, never `preserved`". A record retaining zero bytes
/// retains nothing and is refused by [`CompatibilityReport::validate`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LedgerRecord {
    /// Stable, deterministic identifier.
    pub id: LedgerId,
    /// Where the retained bytes live.
    pub kind: PreservationKind,
    /// The package part the record covers, or the construct name for a
    /// [`PreservationKind::ModelSubtree`]. `None` for the whole-source snapshot.
    pub covers: Option<String>,
    /// Retained byte count.
    pub retained_bytes: usize,
}

impl LedgerRecord {
    /// Whether this record retains anything at all.
    ///
    /// For a snapshot or an in-model subtree the retained bytes *are* the
    /// artifact, so zero bytes means nothing was retained and the record cannot
    /// license a `preserved` claim — `35` calls that `not-retained`. An
    /// [`PreservationKind::OpaquePart`] record is different: the artifact is the
    /// package part, which the semantic writer re-emits with its name and
    /// declared content type, and a genuinely zero-length part is still
    /// reproduced exactly.
    #[must_use]
    pub const fn retains_something(&self) -> bool {
        self.retained_bytes > 0 || matches!(self.kind, PreservationKind::OpaquePart)
    }
}

/// The preservation ledger: every record that licenses a `preserved` retention
/// outcome in the companion [`CompatibilityReport`].
///
/// Built during import and returned alongside the report, so a caller can audit
/// a `preserved` claim instead of trusting it. The ledger is *not* the retained
/// data — it is the index of it (doc-45 invariant I4 keeps opaque bytes out of
/// the semantic model); the bytes live in `RetainedSource` and `RetainedParts`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PreservationLedger {
    records: Vec<LedgerRecord>,
    snapshot: Option<LedgerId>,
}

impl PreservationLedger {
    /// A ledger whose source snapshot retains `retained_bytes` of import input
    /// verbatim (`Retention` mode).
    pub(crate) fn with_source_snapshot(retained_bytes: usize) -> Self {
        let mut ledger = Self::default();
        let id = ledger.push(PreservationKind::SourceSnapshot, None, retained_bytes);
        ledger.snapshot = Some(id);
        ledger
    }

    /// The source-snapshot record, when the import retains one.
    #[must_use]
    pub fn source_snapshot(&self) -> Option<LedgerId> {
        self.snapshot
    }

    /// Every record, in creation order (which is also `LedgerId` order).
    #[must_use]
    pub fn records(&self) -> &[LedgerRecord] {
        &self.records
    }

    /// The record `id` names, if it is in this ledger.
    #[must_use]
    pub fn get(&self, id: LedgerId) -> Option<&LedgerRecord> {
        self.records
            .get(usize::try_from(id.0).unwrap_or(usize::MAX))
    }

    /// Restates the source-snapshot record's extent once the byte floor's true
    /// size is known.
    ///
    /// The record is created during the main-document pass, which can only account
    /// for the main document. The package path then retains **every** admitted
    /// part — the main document among them — so the figure is replaced rather than
    /// added to; adding would count `word/document.xml` twice and make the one
    /// number a caller uses to audit the claim wrong.
    pub(crate) fn restate_source_snapshot(&mut self, retained_bytes: usize) {
        if let Some(id) = self.snapshot
            && let Ok(index) = usize::try_from(id.0)
            && let Some(record) = self.records.get_mut(index)
        {
            record.retained_bytes = retained_bytes;
        }
    }

    /// Records an admitted part carried verbatim through the semantic writer.
    pub(crate) fn record_opaque_part(
        &mut self,
        part_name: &str,
        retained_bytes: usize,
    ) -> LedgerId {
        self.push(
            PreservationKind::OpaquePart,
            Some(part_name.to_owned()),
            retained_bytes,
        )
    }

    /// Records a source subtree retained inside the model and re-emitted on save.
    pub(crate) fn record_model_subtree(&mut self, covers: &str, retained_bytes: usize) -> LedgerId {
        self.push(
            PreservationKind::ModelSubtree,
            Some(covers.to_owned()),
            retained_bytes,
        )
    }

    fn push(
        &mut self,
        kind: PreservationKind,
        covers: Option<String>,
        retained_bytes: usize,
    ) -> LedgerId {
        let id = LedgerId(u32::try_from(self.records.len()).unwrap_or(u32::MAX));
        self.records.push(LedgerRecord {
            id,
            kind,
            covers,
            retained_bytes,
        });
        id
    }
}

/// A violation of the `35-DISPOSITION-TAXONOMY.md` preservation rule.
///
/// `35` is explicit that a pairing it does not admit "is an internal error and
/// must fail import, not be reported", so this surfaces as
/// `ImportError::Disposition` rather than as a report entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DispositionViolation {
    /// An entry claims `preserved` with no preservation-ledger reference.
    PreservedWithoutRecord {
        /// The offending entry's feature identifier.
        feature: String,
    },
    /// An entry references a ledger record that does not exist.
    RecordMissing {
        /// The offending entry's feature identifier.
        feature: String,
        /// The dangling reference.
        ledger_id: LedgerId,
    },
    /// An entry references a record that retains nothing, which is not
    /// preservation.
    RecordRetainsNothing {
        /// The offending entry's feature identifier.
        feature: String,
        /// The empty record.
        ledger_id: LedgerId,
    },
    /// An entry references a ledger record without claiming `preserved`, so the
    /// reference asserts a preservation the disposition denies.
    RecordWithoutPreservation {
        /// The offending entry's feature identifier.
        feature: String,
    },
}

impl std::fmt::Display for DispositionViolation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PreservedWithoutRecord { feature } => write!(
                formatter,
                "{feature} is reported preserved with no preservation-ledger record"
            ),
            Self::RecordMissing { feature, ledger_id } => write!(
                formatter,
                "{feature} references preservation-ledger record {} which does not exist",
                ledger_id.get()
            ),
            Self::RecordRetainsNothing { feature, ledger_id } => write!(
                formatter,
                "{feature} references preservation-ledger record {} which retains no bytes",
                ledger_id.get()
            ),
            Self::RecordWithoutPreservation { feature } => write!(
                formatter,
                "{feature} references a preservation-ledger record without claiming preserved"
            ),
        }
    }
}

/// A whole-part disposition: an admitted package part the semantic model does
/// not consume. Such a part is `omitted` (never in the model). Its retention
/// outcome depends on preservation (P1F-2): a part carried verbatim through the
/// semantic writer via the opaque side-table is `preserved`; a digital signature
/// is `blocked` on the semantic path, because retention is refused for a security
/// reason rather than merely declined. In Retention mode the source byte floor
/// keeps every part, so all are `preserved`. Carries the part name and its
/// declared content type so the disposition is auditable per `35`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PartDisposition {
    /// Normalized package part name (e.g. `customXml/item1.xml`).
    pub part_name: String,
    /// Declared content type, if the package declared one.
    pub content_type: Option<String>,
}

/// One whole-part disposition ready for the report: the part itself, its
/// per-construct disposition, and the preservation-ledger record licensing a
/// `preserved` claim (`None` when it does not make one).
pub(crate) type WholePartDisposition = (PartDisposition, Disposition, Option<LedgerId>);

/// One compatibility-report entry, aggregated by feature and disposition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompatibilityEntry {
    /// Feature: a WordprocessingML local element name, `element/@attribute` for
    /// an attribute finding, an admitted part name (for a whole-part
    /// disposition), a `docx.`-prefixed class identifier, or `(overflow)`.
    pub feature: String,
    /// Bounded occurrence count.
    pub occurrences: u32,
    /// Where the finding is, to the granularity the importer actually knows.
    pub location: FeatureLocation,
    /// The per-construct disposition; both taxonomy axes derive from it.
    pub disposition: Disposition,
    /// The preservation-ledger record licensing a `preserved` retention outcome.
    /// `Some` exactly when [`Disposition::claims_preservation`] holds, which
    /// [`CompatibilityReport::validate`] enforces.
    pub ledger_id: Option<LedgerId>,
    /// Set when this entry dispositions a whole admitted part the semantic model
    /// does not consume; `None` for element/attribute feature entries.
    pub part: Option<PartDisposition>,
}

impl CompatibilityEntry {
    /// Axis A — what the normalized model captured of the construct.
    #[must_use]
    pub const fn model_outcome(&self) -> ModelOutcome {
        self.disposition.model_outcome()
    }

    /// Axis B — what happened to the source detail the model did not consume.
    #[must_use]
    pub const fn retention_outcome(&self) -> RetentionOutcome {
        self.disposition.retention_outcome()
    }
}

/// A deterministic compatibility report ordered by feature name.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompatibilityReport {
    /// Entries ordered by feature name, then by disposition.
    pub entries: Vec<CompatibilityEntry>,
}

impl CompatibilityReport {
    /// Whether the import found nothing it could not fully represent.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Checks `35-DISPOSITION-TAXONOMY.md`'s prohibited-silent-loss rule against
    /// `ledger`: a `preserved` retention outcome is legal only when the entry
    /// references a ledger record that really retains something, and an entry
    /// that does not claim `preserved` must not reference one.
    ///
    /// The nine legal axis pairs need no check here — [`Disposition`] makes the
    /// other six unrepresentable — so this is the part of the contract a type
    /// cannot carry. `35` requires a violation to fail the import.
    pub fn validate(&self, ledger: &PreservationLedger) -> Result<(), DispositionViolation> {
        for entry in &self.entries {
            match (entry.disposition.claims_preservation(), entry.ledger_id) {
                (true, None) => {
                    return Err(DispositionViolation::PreservedWithoutRecord {
                        feature: entry.feature.clone(),
                    });
                }
                (false, Some(_)) => {
                    return Err(DispositionViolation::RecordWithoutPreservation {
                        feature: entry.feature.clone(),
                    });
                }
                (true, Some(ledger_id)) => match ledger.get(ledger_id) {
                    None => {
                        return Err(DispositionViolation::RecordMissing {
                            feature: entry.feature.clone(),
                            ledger_id,
                        });
                    }
                    Some(record) if !record.retains_something() => {
                        return Err(DispositionViolation::RecordRetainsNothing {
                            feature: entry.feature.clone(),
                            ledger_id,
                        });
                    }
                    Some(_) => {}
                },
                (false, None) => {}
            }
        }
        Ok(())
    }

    /// Folds a second report's entries (e.g. unmapped `docProps` fields
    /// discovered after the main body pass, which is parsed separately because
    /// the property parts hang off the package root, not the main document)
    /// into this one, aggregating by feature *and disposition* and preserving the
    /// deterministic ordering. Two findings that share a feature name but differ
    /// in disposition are different fidelity facts and stay separate entries.
    pub(crate) fn merge(&mut self, other: Self) {
        for entry in other.entries {
            match self.entries.iter_mut().find(|existing| {
                existing.feature == entry.feature && existing.disposition == entry.disposition
            }) {
                Some(existing) => {
                    existing.occurrences = existing.occurrences.saturating_add(entry.occurrences);
                }
                None => self.entries.push(entry),
            }
        }
        self.sort();
    }

    fn sort(&mut self) {
        self.entries.sort_by(|left, right| {
            left.feature
                .cmp(&right.feature)
                .then_with(|| left.disposition.cmp(&right.disposition))
        });
    }
}

/// What the importer did with one construct, before retention is resolved.
///
/// This is the per-call-site half of a disposition: the parser knows what *it*
/// did, and only the driver knows what retains the source it was reading. Keeping
/// them apart is what stopped retention being a per-mode constant (FID-R-02):
/// a `Semantic` import now yields four different retention outcomes from these
/// five findings.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum Finding {
    /// Not represented in the model at all.
    Omitted,
    /// Partially represented: the construct is modeled, but some source meaning
    /// it carries — typically an attribute — was not carried across.
    Degraded,
    /// Structurally invalid or unusable: there is nothing to map and nothing to
    /// join it to, so it is refused rather than merely dropped.
    Invalid,
    /// Not represented in the model, but retained verbatim *inside* the model and
    /// re-emitted on save, so the source detail is recoverable on the semantic
    /// path. Carries the retained byte count for the ledger record.
    RetainedInModel(usize),
}

impl Finding {
    /// The taxonomy key this finding aggregates under. `RetainedInModel`'s byte
    /// count must not split one construct into one entry per occurrence.
    fn key(self) -> FindingKey {
        match self {
            Self::Omitted => FindingKey::Omitted,
            Self::Degraded => FindingKey::Degraded,
            Self::Invalid => FindingKey::Invalid,
            Self::RetainedInModel(_) => FindingKey::RetainedInModel,
        }
    }
}

/// [`Finding`] without its payload, so occurrences of one construct aggregate.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum FindingKey {
    Omitted,
    Degraded,
    Invalid,
    RetainedInModel,
}

/// What retains the unconsumed source detail of everything a [`Reporter`] is
/// reading.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SourceRetention {
    /// A verbatim source snapshot covers it — `Retention` mode's byte floor,
    /// which reproduces the import input exactly, so every finding's remainder is
    /// recoverable from the ledger's snapshot record.
    Snapshot,
    /// Nothing retains it: the part is regenerated from the model on save, so a
    /// finding's remainder is recoverable only if the finding itself says so.
    Regenerated,
}

impl SourceRetention {
    /// Resolves a per-construct [`Finding`] into the per-construct disposition.
    ///
    /// Under a verbatim snapshot every remainder is genuinely recoverable, so a
    /// refusal or an invalidity of the *model* does not make the *bytes*
    /// unretained — which is why the whole column collapses to `preserved`. That
    /// is a fact about the byte floor, not a constant stamped in ignorance: the
    /// same findings produce four different retention outcomes without one.
    fn resolve(self, finding: Finding) -> Disposition {
        match (self, finding) {
            (Self::Snapshot, Finding::Degraded) => Disposition::DegradedPreserved,
            (Self::Snapshot, _) => Disposition::OmittedPreserved,
            (Self::Regenerated, Finding::Omitted) => Disposition::OmittedNotRetained,
            (Self::Regenerated, Finding::Degraded) => Disposition::DegradedNotRetained,
            (Self::Regenerated, Finding::Invalid) => Disposition::OmittedRejected,
            (Self::Regenerated, Finding::RetainedInModel(_)) => Disposition::OmittedPreserved,
        }
    }
}

/// Whether `local` names revision-save-ID markup: the `w:rsids` table in
/// `settings.xml`, its `w:rsid` entries, its `w:rsidRoot`, and the per-style
/// `w:rsid` in `styles.xml`. All are members of the one class
/// [`Reporter::report_rsid`] describes.
fn is_revision_save_id_element(local: &[u8]) -> bool {
    matches!(local, b"rsid" | b"rsids" | b"rsidRoot")
}

/// Whether `local` names a revision-save-ID attribute (`w:rsidR`, `w:rsidRPr`,
/// `w:rsidRDefault`, `w:rsidP`, `w:rsidDel`, `w:rsidTr`, `w:rsidSect`, and any
/// future sibling): the same class, carried on an element rather than as one.
pub(crate) fn is_revision_save_id_attribute(local: &[u8]) -> bool {
    local.starts_with(b"rsid")
}

/// One aggregated finding before the ledger resolves its preservation claim.
#[derive(Debug)]
struct Pending {
    occurrences: u32,
    location: FeatureLocation,
    /// Bytes retained in the model for a [`Finding::RetainedInModel`] finding,
    /// summed across occurrences so one ledger record covers the construct.
    retained_bytes: usize,
}

/// Aggregating report sink shared by every parser.
///
/// Findings aggregate on `(feature, finding kind)`: two findings that share a
/// feature name but differ in what happened to the construct are different
/// fidelity facts. The **first** location for a key is kept, which is what `35`
/// permits ("Repeated equivalent findings may be aggregated only when counts and
/// first bounded locations remain deterministic").
#[derive(Debug)]
pub(crate) struct Reporter {
    findings: BTreeMap<(String, FindingKey), Pending>,
    retention: SourceRetention,
    overflow: u32,
}

impl Reporter {
    /// A reporter for source whose unconsumed detail is retained as `retention`
    /// describes. There is no `Default`: what retains the source is a policy
    /// decision every construction site must state, and defaulting it is how it
    /// became a per-mode constant in the first place.
    pub(crate) fn new(retention: SourceRetention) -> Self {
        Self {
            findings: BTreeMap::new(),
            retention,
            overflow: 0,
        }
    }

    /// Reports an element the model does not represent.
    ///
    /// Revision-save-ID markup is folded into its document-level class here
    /// rather than at each call site: the `w:rsid` element appears in both
    /// `styles.xml` and `settings.xml` and reaches this sink through two
    /// different catch-alls, so a per-site rule is a rule that the next catch-all
    /// forgets. Routing at the single choke point makes the class total.
    pub(crate) fn report(&mut self, local: &[u8]) {
        if is_revision_save_id_element(local) {
            self.report_rsid();
            return;
        }
        let feature = String::from_utf8_lossy(local).into_owned();
        self.insert(feature, FeatureLocation::element(local), Finding::Omitted);
    }

    /// Reports an element that is structurally invalid or unusable — refused,
    /// not merely dropped.
    pub(crate) fn report_invalid(&mut self, local: &[u8]) {
        let feature = String::from_utf8_lossy(local).into_owned();
        self.insert(feature, FeatureLocation::element(local), Finding::Invalid);
    }

    /// Reports an attribute of an otherwise-modeled element whose meaning the
    /// model does not carry: the element is `degraded`, and the location names
    /// which part of its meaning was lost (FID-R-03).
    pub(crate) fn report_attribute(&mut self, element: &[u8], attribute: &[u8]) {
        let feature = format!(
            "{}/@{}",
            String::from_utf8_lossy(element),
            String::from_utf8_lossy(attribute)
        );
        self.insert(
            feature,
            FeatureLocation::attribute(element, attribute),
            Finding::Degraded,
        );
    }

    /// Reports a construct that is not in the model but whose source is retained
    /// verbatim inside it and re-emitted on save, so the detail is recoverable
    /// even on the semantic path.
    pub(crate) fn report_retained_in_model(&mut self, local: &[u8], retained_bytes: usize) {
        let feature = String::from_utf8_lossy(local).into_owned();
        self.insert(
            feature,
            FeatureLocation::element(local),
            Finding::RetainedInModel(retained_bytes),
        );
    }

    /// Reports one revision-save ID (`w:rsid*` attribute, or a `w:rsid`/`w:rsids`
    /// entry in `settings.xml`) as a member of **one document-level class**
    /// rather than as a finding of its own.
    ///
    /// Revision-save IDs are per-editing-session bookkeeping tokens: Word's
    /// compare/merge heuristics are their only consumer, and they have no effect
    /// on layout, rendering, text, or reopen fidelity. They are also the
    /// highest-volume attribute in WordprocessingML — a 14-document sample of
    /// real Word output carries up to 1,055 of them in one file, across seven
    /// names on `w:p`, `w:r`, `w:tr` and `w:sectPr`. Reporting each
    /// element-and-attribute pair would add roughly fourteen entries to every
    /// Word document's report, against the 28-30 entries such a document
    /// currently raises in total: a ~50% inflation carrying no actionable
    /// content, which is how a report becomes something callers filter out.
    ///
    /// So the class is one entry with a count. That is strictly *less* noisy than
    /// what preceded it (the `w:rsids` table in `settings.xml` used to raise two
    /// separate element findings) while covering the attributes, which could not
    /// be described at all. The loss is still named, and its disposition is still
    /// honest: `preserved` under the retention byte floor, `not-retained` on a
    /// semantic save. `35-DISPOSITION-TAXONOMY.md` records the decision.
    pub(crate) fn report_rsid(&mut self) {
        self.insert(
            RSID_CLASS_FEATURE.to_owned(),
            FeatureLocation::default(),
            Finding::Omitted,
        );
    }

    fn insert(&mut self, feature: String, location: FeatureLocation, finding: Finding) {
        let retained_bytes = match finding {
            Finding::RetainedInModel(bytes) => bytes,
            _ => 0,
        };
        let key = (feature, finding.key());
        if let Some(pending) = self.findings.get_mut(&key) {
            pending.occurrences = pending.occurrences.saturating_add(1);
            pending.retained_bytes = pending.retained_bytes.saturating_add(retained_bytes);
        } else if self.findings.len() < MAX_REPORT_FEATURES {
            self.findings.insert(
                key,
                Pending {
                    occurrences: 1,
                    location,
                    retained_bytes,
                },
            );
        } else {
            self.overflow = self.overflow.saturating_add(1);
        }
    }

    /// Builds the report, resolving each finding's preservation claim against
    /// `ledger` — creating the ledger record for an in-model retained subtree,
    /// and citing the source-snapshot record when the byte floor covers the
    /// source. `BTreeMap` iteration already orders entries by feature and then by
    /// finding kind, so the output is deterministic without a sort.
    pub(crate) fn into_report(self, ledger: &mut PreservationLedger) -> CompatibilityReport {
        let retention = self.retention;
        let snapshot = ledger.source_snapshot();
        let mut entries: Vec<CompatibilityEntry> = Vec::with_capacity(self.findings.len());
        for ((feature, key), pending) in self.findings {
            let finding = match key {
                FindingKey::Omitted => Finding::Omitted,
                FindingKey::Degraded => Finding::Degraded,
                FindingKey::Invalid => Finding::Invalid,
                FindingKey::RetainedInModel => Finding::RetainedInModel(pending.retained_bytes),
            };
            let disposition = retention.resolve(finding);
            // A preserved finding cites the record that licenses the claim: the
            // snapshot when the byte floor covers the source, otherwise the
            // in-model subtree record this finding is the reason for.
            let ledger_id = if disposition.claims_preservation() {
                match (retention, key) {
                    (SourceRetention::Snapshot, _) => snapshot,
                    (SourceRetention::Regenerated, FindingKey::RetainedInModel) => {
                        Some(ledger.record_model_subtree(&feature, pending.retained_bytes))
                    }
                    (SourceRetention::Regenerated, _) => None,
                }
            } else {
                None
            };
            entries.push(CompatibilityEntry {
                feature,
                occurrences: pending.occurrences,
                location: pending.location,
                disposition,
                ledger_id,
                part: None,
            });
        }
        if self.overflow > 0 {
            let disposition = retention.resolve(Finding::Omitted);
            entries.push(CompatibilityEntry {
                feature: "(overflow)".to_owned(),
                occurrences: self.overflow,
                location: FeatureLocation::default(),
                disposition,
                ledger_id: disposition
                    .claims_preservation()
                    .then_some(snapshot)
                    .flatten(),
                part: None,
            });
        }
        let mut report = CompatibilityReport { entries };
        report.sort();
        report
    }
}

impl CompatibilityReport {
    /// Appends a whole-part disposition for every admitted part the semantic
    /// model does not consume, closing the silent-whole-part-loss class. Each
    /// part carries its own disposition and, when it claims preservation, the
    /// ledger record that licenses the claim. Re-sorts deterministically so the
    /// report order is stable regardless of insertion order.
    pub(crate) fn add_part_dispositions(
        &mut self,
        parts: impl IntoIterator<Item = WholePartDisposition>,
    ) {
        for (part, disposition, ledger_id) in parts {
            self.entries.push(CompatibilityEntry {
                feature: part.part_name.clone(),
                occurrences: 1,
                location: FeatureLocation {
                    part_name: Some(part.part_name.clone()),
                    element: None,
                    attribute: None,
                },
                disposition,
                ledger_id,
                part: Some(part),
            });
        }
        self.sort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every [`Disposition`] must project onto exactly one of the nine pairs
    /// `35-DISPOSITION-TAXONOMY.md` lists as legal, and the nine must all be
    /// reachable. This is the guard that keeps the enum an encoding of the
    /// contract rather than a set of names that drifted away from it.
    #[test]
    fn every_disposition_is_one_of_the_nine_legal_pairs() {
        // Transcribed from `35-DISPOSITION-TAXONOMY.md` "Legal combinations".
        let legal = [
            (ModelOutcome::Mapped, RetentionOutcome::NotApplicable),
            (ModelOutcome::Mapped, RetentionOutcome::Preserved),
            (ModelOutcome::Degraded, RetentionOutcome::Preserved),
            (ModelOutcome::Degraded, RetentionOutcome::NotRetained),
            (ModelOutcome::Degraded, RetentionOutcome::Blocked),
            (ModelOutcome::Omitted, RetentionOutcome::Preserved),
            (ModelOutcome::Omitted, RetentionOutcome::NotRetained),
            (ModelOutcome::Omitted, RetentionOutcome::Blocked),
            (ModelOutcome::Omitted, RetentionOutcome::Rejected),
        ];
        let all = [
            Disposition::MappedComplete,
            Disposition::MappedPreserved,
            Disposition::DegradedPreserved,
            Disposition::DegradedNotRetained,
            Disposition::DegradedBlocked,
            Disposition::OmittedPreserved,
            Disposition::OmittedNotRetained,
            Disposition::OmittedBlocked,
            Disposition::OmittedRejected,
        ];
        let mut projected: Vec<(ModelOutcome, RetentionOutcome)> = Vec::new();
        for disposition in all {
            let pair = (disposition.model_outcome(), disposition.retention_outcome());
            assert!(
                legal.contains(&pair),
                "{disposition:?} projects onto {pair:?}, which doc 35 does not admit"
            );
            assert!(
                !projected.contains(&pair),
                "{disposition:?} duplicates the pair {pair:?} of an earlier variant"
            );
            projected.push(pair);
        }
        assert_eq!(
            projected.len(),
            legal.len(),
            "every legal pair must be reachable through exactly one Disposition"
        );
    }

    /// Only `preserved` may cite a ledger record, and every `preserved` must.
    #[test]
    fn claims_preservation_matches_the_retention_axis() {
        for disposition in [
            Disposition::MappedComplete,
            Disposition::MappedPreserved,
            Disposition::DegradedPreserved,
            Disposition::DegradedNotRetained,
            Disposition::DegradedBlocked,
            Disposition::OmittedPreserved,
            Disposition::OmittedNotRetained,
            Disposition::OmittedBlocked,
            Disposition::OmittedRejected,
        ] {
            assert_eq!(
                disposition.claims_preservation(),
                disposition.retention_outcome() == RetentionOutcome::Preserved,
                "{disposition:?}"
            );
        }
    }

    /// A reporter that recorded nothing produces an empty report: the report
    /// enumerates unrecovered meaning, so silence is the healthy case.
    #[test]
    fn a_reporter_that_recorded_nothing_produces_an_empty_report() {
        let mut ledger = PreservationLedger::default();
        assert!(
            Reporter::new(SourceRetention::Regenerated)
                .into_report(&mut ledger)
                .is_empty()
        );
    }

    /// One `Semantic` import yields four different retention outcomes from four
    /// different findings. This is the property FID-R-02 is about: before it,
    /// every entry in a Semantic report read `not-retained` because retention was
    /// a property of the *mode*.
    #[test]
    fn one_semantic_report_carries_four_different_retention_outcomes() {
        let mut reporter = Reporter::new(SourceRetention::Regenerated);
        reporter.report(b"unmapped");
        reporter.report_invalid(b"commentEx");
        reporter.report_attribute(b"theme", b"name");
        reporter.report_retained_in_model(b"oMath", 64);
        let mut ledger = PreservationLedger::default();
        let report = reporter.into_report(&mut ledger);
        let outcomes: Vec<(&str, ModelOutcome, RetentionOutcome)> = report
            .entries
            .iter()
            .map(|entry| {
                (
                    entry.feature.as_str(),
                    entry.model_outcome(),
                    entry.retention_outcome(),
                )
            })
            .collect();
        assert_eq!(
            outcomes,
            vec![
                (
                    "commentEx",
                    ModelOutcome::Omitted,
                    RetentionOutcome::Rejected
                ),
                ("oMath", ModelOutcome::Omitted, RetentionOutcome::Preserved),
                (
                    "theme/@name",
                    ModelOutcome::Degraded,
                    RetentionOutcome::NotRetained
                ),
                (
                    "unmapped",
                    ModelOutcome::Omitted,
                    RetentionOutcome::NotRetained
                ),
            ]
        );
        report.validate(&ledger).expect("the report is legal");
    }

    /// A `preserved` entry with no ledger record must fail the import, not be
    /// reported. This is the rule a type cannot carry, so it is validated.
    #[test]
    fn validation_refuses_preserved_without_a_ledger_record() {
        let report = CompatibilityReport {
            entries: vec![CompatibilityEntry {
                feature: "tblStyle".to_owned(),
                occurrences: 1,
                location: FeatureLocation::element(b"tblStyle"),
                disposition: Disposition::OmittedPreserved,
                ledger_id: None,
                part: None,
            }],
        };
        assert_eq!(
            report.validate(&PreservationLedger::default()),
            Err(DispositionViolation::PreservedWithoutRecord {
                feature: "tblStyle".to_owned()
            })
        );
    }

    /// A record that retains no bytes retains nothing, so it cannot license a
    /// `preserved` claim: doc 35 calls that `not-retained`.
    #[test]
    fn validation_refuses_a_ledger_record_that_retains_nothing() {
        let ledger = PreservationLedger::with_source_snapshot(0);
        let ledger_id = ledger.source_snapshot().expect("a snapshot record");
        let report = CompatibilityReport {
            entries: vec![CompatibilityEntry {
                feature: "tblStyle".to_owned(),
                occurrences: 1,
                location: FeatureLocation::element(b"tblStyle"),
                disposition: Disposition::OmittedPreserved,
                ledger_id: Some(ledger_id),
                part: None,
            }],
        };
        assert_eq!(
            report.validate(&ledger),
            Err(DispositionViolation::RecordRetainsNothing {
                feature: "tblStyle".to_owned(),
                ledger_id
            })
        );
    }

    /// A dangling reference is a violation, not a silently ignored field.
    #[test]
    fn validation_refuses_a_dangling_ledger_reference() {
        let report = CompatibilityReport {
            entries: vec![CompatibilityEntry {
                feature: "tblStyle".to_owned(),
                occurrences: 1,
                location: FeatureLocation::element(b"tblStyle"),
                disposition: Disposition::OmittedPreserved,
                ledger_id: Some(LedgerId(7)),
                part: None,
            }],
        };
        assert_eq!(
            report.validate(&PreservationLedger::default()),
            Err(DispositionViolation::RecordMissing {
                feature: "tblStyle".to_owned(),
                ledger_id: LedgerId(7)
            })
        );
    }

    /// The converse: citing a record while reporting the remainder dropped
    /// asserts a preservation the disposition denies.
    #[test]
    fn validation_refuses_a_record_reference_without_a_preservation_claim() {
        let ledger = PreservationLedger::with_source_snapshot(16);
        let report = CompatibilityReport {
            entries: vec![CompatibilityEntry {
                feature: "tblStyle".to_owned(),
                occurrences: 1,
                location: FeatureLocation::element(b"tblStyle"),
                disposition: Disposition::OmittedNotRetained,
                ledger_id: ledger.source_snapshot(),
                part: None,
            }],
        };
        assert_eq!(
            report.validate(&ledger),
            Err(DispositionViolation::RecordWithoutPreservation {
                feature: "tblStyle".to_owned()
            })
        );
    }

    /// Findings that share a feature name but not a disposition are different
    /// facts and must not be merged into one count.
    #[test]
    fn a_feature_with_two_dispositions_stays_two_entries() {
        let mut reporter = Reporter::new(SourceRetention::Regenerated);
        reporter.report(b"commentEx");
        reporter.report_invalid(b"commentEx");
        reporter.report_invalid(b"commentEx");
        let mut ledger = PreservationLedger::default();
        let report = reporter.into_report(&mut ledger);
        assert_eq!(report.entries.len(), 2);
        assert_eq!(report.entries[0].occurrences, 1);
        assert_eq!(
            report.entries[0].retention_outcome(),
            RetentionOutcome::NotRetained
        );
        assert_eq!(report.entries[1].occurrences, 2);
        assert_eq!(
            report.entries[1].retention_outcome(),
            RetentionOutcome::Rejected
        );
    }

    /// The whole rsid class is one entry however many revision-save IDs the
    /// document carries, and its identifier is the documented class id.
    #[test]
    fn the_rsid_class_is_a_single_counted_entry() {
        let mut reporter = Reporter::new(SourceRetention::Regenerated);
        for _ in 0..1_000 {
            reporter.report_rsid();
        }
        let mut ledger = PreservationLedger::default();
        let report = reporter.into_report(&mut ledger);
        assert_eq!(report.entries.len(), 1);
        assert_eq!(report.entries[0].feature, RSID_CLASS_FEATURE);
        assert_eq!(report.entries[0].occurrences, 1_000);
    }
}
