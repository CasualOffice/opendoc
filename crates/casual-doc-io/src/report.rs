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
