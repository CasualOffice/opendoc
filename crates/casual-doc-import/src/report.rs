//! The bounded, deterministic compatibility report.

use std::collections::BTreeMap;

/// Distinct-feature ceiling; excess folds into an `(overflow)` bucket.
const MAX_REPORT_FEATURES: usize = 4_096;

/// How a construct was represented in the model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelOutcome {
    /// Fully represented.
    Mapped,
    /// Partially represented.
    Degraded,
    /// Not represented.
    Omitted,
}

/// What happened to source detail the model did not consume.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

/// One compatibility-report entry, aggregated by feature.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompatibilityEntry {
    /// Feature (WordprocessingML local element name, or `(overflow)`).
    pub feature: String,
    /// Bounded occurrence count.
    pub occurrences: u32,
    /// Model outcome.
    pub model_outcome: ModelOutcome,
    /// Retention outcome.
    pub retention_outcome: RetentionOutcome,
}

/// A deterministic compatibility report ordered by feature name.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompatibilityReport {
    /// Entries ordered by feature name.
    pub entries: Vec<CompatibilityEntry>,
}

/// Aggregating report sink shared by the body and styles parsers. This slice
/// imports in Semantic mode, so every reported construct is dispositioned
/// `omitted` + `not-retained`; Retention mode (round-trip) will preserve them.
#[derive(Debug, Default)]
pub(crate) struct Reporter {
    counts: BTreeMap<String, u32>,
    overflow: u32,
}

impl Reporter {
    pub(crate) fn report(&mut self, local: &[u8]) {
        let feature = String::from_utf8_lossy(local).into_owned();
        if let Some(count) = self.counts.get_mut(&feature) {
            *count = count.saturating_add(1);
        } else if self.counts.len() < MAX_REPORT_FEATURES {
            self.counts.insert(feature, 1);
        } else {
            self.overflow = self.overflow.saturating_add(1);
        }
    }

    pub(crate) fn into_report(self) -> CompatibilityReport {
        let mut entries: Vec<CompatibilityEntry> = self
            .counts
            .into_iter()
            .map(|(feature, occurrences)| CompatibilityEntry {
                feature,
                occurrences,
                model_outcome: ModelOutcome::Omitted,
                retention_outcome: RetentionOutcome::NotRetained,
            })
            .collect();
        if self.overflow > 0 {
            entries.push(CompatibilityEntry {
                feature: "(overflow)".to_owned(),
                occurrences: self.overflow,
                model_outcome: ModelOutcome::Omitted,
                retention_outcome: RetentionOutcome::NotRetained,
            });
        }
        entries.sort_by(|left, right| left.feature.cmp(&right.feature));
        CompatibilityReport { entries }
    }
}
