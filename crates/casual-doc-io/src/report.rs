//! Format-neutral compatibility reporting.

/// How a source construct was represented in the normalized model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelOutcome {
    /// Fully represented.
    Mapped,
    /// Partially represented.
    Degraded,
    /// Not represented.
    Omitted,
}

/// What happened to source detail the normalized model did not consume.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetentionOutcome {
    /// Retained in validated sidecar state.
    Preserved,
    /// Intentionally and reportably not retained.
    NotRetained,
    /// Refused by security or host policy.
    Blocked,
    /// Invalid or over-limit source data was rejected.
    Rejected,
    /// The construct was fully mapped with no remainder.
    NotApplicable,
}

/// Bounded source location for a compatibility finding.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FeatureLocation {
    /// Package part containing the feature, when applicable.
    pub part_name: Option<String>,
    /// XML namespace identifier, when the adapter supplies one.
    pub namespace: Option<String>,
    /// XML local name or another adapter-defined logical feature name.
    pub local_name: Option<String>,
    /// XML local name of the attribute, when the finding is about an attribute of
    /// `local_name` rather than the element itself (FID-R-03).
    ///
    /// Without this axis an adapter could describe a lost element but not a lost
    /// attribute, and attribute-level facts had to be smuggled through as
    /// feature-level pseudo-names (`theme:nameAttribute`). Attributes are where
    /// most WordprocessingML meaning actually lives, so a report vocabulary
    /// without them cannot be complete.
    pub attribute_name: Option<String>,
}

/// One aggregated compatibility finding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompatibilityEntry {
    /// Stable adapter-defined feature identifier.
    pub feature: String,
    /// Bounded occurrence count.
    pub occurrences: u32,
    /// Source location, if one is available.
    pub location: FeatureLocation,
    /// Semantic mapping result.
    pub model_outcome: ModelOutcome,
    /// Preservation result for unconsumed source detail.
    pub retention_outcome: RetentionOutcome,
}

/// Deterministically ordered import or export compatibility findings.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompatibilityReport {
    /// Findings ordered by adapter-defined feature identifier.
    pub entries: Vec<CompatibilityEntry>,
}

impl CompatibilityReport {
    /// Sorts entries into the required deterministic order.
    pub(crate) fn sort(&mut self) {
        self.entries.sort_by(|left, right| {
            left.feature
                .cmp(&right.feature)
                .then_with(|| left.location.part_name.cmp(&right.location.part_name))
        });
    }
}

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
