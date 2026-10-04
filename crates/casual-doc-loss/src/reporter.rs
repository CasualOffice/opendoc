// SPDX-License-Identifier: Apache-2.0

//! The aggregating sink every format adapter reports findings into, and the
//! resolution of a per-construct finding into a per-construct disposition.

use std::collections::BTreeMap;

use crate::ledger::{LedgerId, PreservationLedger};
use crate::report::{CompatibilityEntry, CompatibilityReport, FeatureLocation};
use crate::taxonomy::Disposition;

/// Distinct-feature ceiling; excess folds into an `(overflow)` bucket so a
/// pathological document cannot make the report grow without bound.
const MAX_REPORT_FEATURES: usize = 4_096;

/// Feature identifier of the bucket that distinct findings beyond
/// `MAX_REPORT_FEATURES` fold into.
const OVERFLOW_FEATURE: &str = "(overflow)";

/// What an adapter did with one construct, before retention is resolved.
///
/// This is the per-call-site half of a disposition: the parser knows what *it*
/// did, and only the driver knows what retains the source it was reading.
/// Keeping them apart is what stopped retention being a per-mode constant
/// (FID-R-02): one semantic import now yields four different retention outcomes
/// from these four findings.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Finding {
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
    const fn key(self) -> FindingKey {
        match self {
            Self::Omitted => FindingKey::Omitted,
            Self::Degraded => FindingKey::Degraded,
            Self::Invalid => FindingKey::Invalid,
            Self::RetainedInModel(_) => FindingKey::RetainedInModel,
        }
    }
}

/// [`Finding`] without its payload, so occurrences of one construct aggregate.
///
/// The declaration order is load-bearing: it is the `BTreeMap` key order, and
/// therefore the order in which two findings about one feature that resolve to
/// the *same* disposition appear in the report (under a whole-source byte floor,
/// `Omitted`, `Invalid` and `RetainedInModel` all resolve to `omitted` +
/// `preserved`). The report's own sort is stable, so it does not reorder them.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum FindingKey {
    Omitted,
    Degraded,
    Invalid,
    RetainedInModel,
}

/// What retains the unconsumed source detail of everything a [`LossReporter`] is
/// reading.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceRetention {
    /// A verbatim source snapshot covers it — a retention mode's byte floor,
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
    #[must_use]
    pub const fn resolve(self, finding: Finding) -> Disposition {
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

/// One aggregated finding before the ledger resolves its preservation claim.
#[derive(Debug)]
struct Pending {
    occurrences: u32,
    location: FeatureLocation,
    /// Bytes retained in the model for a [`Finding::RetainedInModel`] finding,
    /// summed across occurrences so one ledger record covers the construct.
    retained_bytes: usize,
}

/// Aggregating report sink shared by every format adapter.
///
/// Findings aggregate on `(feature, finding kind)`: two findings that share a
/// feature name but differ in what happened to the construct are different
/// fidelity facts. The **first** location for a key is kept, which is what `35`
/// permits ("Repeated equivalent findings may be aggregated only when counts and
/// first bounded locations remain deterministic").
///
/// Nothing here knows a markup vocabulary. The adapter decides the feature
/// identifier, the bounded location and which [`Finding`] applies — classifying
/// markup as meaningful or as a no-op is a format decision and belongs in the
/// format crate. What is shared is the aggregation, the ceiling, the resolution
/// into a disposition and the ledger bookkeeping, so there is exactly one
/// implementation of each.
///
/// Complexity: O(log distinct-features) per recorded finding, with the ceiling
/// bounding the map; O(1) in document size.
#[derive(Debug)]
pub struct LossReporter {
    findings: BTreeMap<(String, FindingKey), Pending>,
    retention: SourceRetention,
    overflow: u32,
}

impl LossReporter {
    /// A reporter for source whose unconsumed detail is retained as `retention`
    /// describes. There is no `Default`: what retains the source is a policy
    /// decision every construction site must state, and defaulting it is how it
    /// became a per-mode constant in the first place.
    #[must_use]
    pub fn new(retention: SourceRetention) -> Self {
        Self {
            findings: BTreeMap::new(),
            retention,
            overflow: 0,
        }
    }

    /// What retains the source this reporter is reading.
    #[must_use]
    pub const fn retention(&self) -> SourceRetention {
        self.retention
    }

    /// Records one finding about `feature`, at `location`.
    ///
    /// This is the only way in. An adapter that wants a vocabulary of its own —
    /// "report this element", "report this attribute", "report this class once
    /// per document" — builds it as a thin layer over this call, and the
    /// taxonomy stays with one implementation.
    pub fn record(&mut self, feature: String, location: FeatureLocation, finding: Finding) {
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
    #[must_use]
    pub fn into_report(self, ledger: &mut PreservationLedger) -> CompatibilityReport {
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
            let ledger_id: Option<LedgerId> = if disposition.claims_preservation() {
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
                feature: OVERFLOW_FEATURE.to_owned(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taxonomy::{ModelOutcome, RetentionOutcome};

    /// A reporter that recorded nothing produces an empty report: the report
    /// enumerates unrecovered meaning, so silence is the healthy case.
    #[test]
    fn a_reporter_that_recorded_nothing_produces_an_empty_report() {
        let mut ledger = PreservationLedger::default();
        assert!(
            LossReporter::new(SourceRetention::Regenerated)
                .into_report(&mut ledger)
                .is_empty()
        );
    }

    /// One semantic report yields four different retention outcomes from four
    /// different findings. This is the property FID-R-02 is about: before it,
    /// every entry in a semantic report read `not-retained` because retention was
    /// a property of the *mode*.
    #[test]
    fn one_semantic_report_carries_four_different_retention_outcomes() {
        let mut reporter = LossReporter::new(SourceRetention::Regenerated);
        reporter.record(
            "unmapped".to_owned(),
            FeatureLocation::for_element("unmapped"),
            Finding::Omitted,
        );
        reporter.record(
            "commentEx".to_owned(),
            FeatureLocation::for_element("commentEx"),
            Finding::Invalid,
        );
        reporter.record(
            "theme/@name".to_owned(),
            FeatureLocation::for_attribute("theme", "name"),
            Finding::Degraded,
        );
        reporter.record(
            "oMath".to_owned(),
            FeatureLocation::for_element("oMath"),
            Finding::RetainedInModel(64),
        );
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

    /// An in-model retained subtree mints its own ledger record, so its
    /// `preserved` claim resolves on the semantic path where no byte floor exists.
    #[test]
    fn an_in_model_retained_subtree_mints_its_own_ledger_record() {
        let mut reporter = LossReporter::new(SourceRetention::Regenerated);
        reporter.record(
            "oMath".to_owned(),
            FeatureLocation::for_element("oMath"),
            Finding::RetainedInModel(40),
        );
        reporter.record(
            "oMath".to_owned(),
            FeatureLocation::for_element("oMath"),
            Finding::RetainedInModel(24),
        );
        let mut ledger = PreservationLedger::default();
        let report = reporter.into_report(&mut ledger);
        assert_eq!(report.entries.len(), 1, "one construct, one entry");
        assert_eq!(report.entries[0].occurrences, 2);
        let record = ledger
            .get(report.entries[0].ledger_id.expect("a record"))
            .expect("in ledger");
        assert_eq!(record.covers.as_deref(), Some("oMath"));
        assert_eq!(record.retained_bytes, 64, "bytes sum across occurrences");
        assert_eq!(report.validate(&ledger), Ok(()));
    }

    /// Findings that share a feature name but not a disposition are different
    /// facts and must not be merged into one count.
    #[test]
    fn a_feature_with_two_dispositions_stays_two_entries() {
        let mut reporter = LossReporter::new(SourceRetention::Regenerated);
        for finding in [Finding::Omitted, Finding::Invalid, Finding::Invalid] {
            reporter.record(
                "commentEx".to_owned(),
                FeatureLocation::for_element("commentEx"),
                finding,
            );
        }
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

    /// Under a byte floor every remainder is recoverable, so three findings that
    /// differ on the model axis collapse onto one disposition — and still stay
    /// separate entries, in `FindingKey` order, because they are different facts.
    #[test]
    fn a_byte_floor_collapses_the_retention_axis_without_merging_the_findings() {
        let mut reporter = LossReporter::new(SourceRetention::Snapshot);
        for finding in [
            Finding::Omitted,
            Finding::Invalid,
            Finding::RetainedInModel(8),
        ] {
            reporter.record(
                "thing".to_owned(),
                FeatureLocation::for_element("thing"),
                finding,
            );
        }
        let mut ledger = PreservationLedger::with_source_snapshot(1_024);
        let report = reporter.into_report(&mut ledger);
        assert_eq!(report.entries.len(), 3);
        for entry in &report.entries {
            assert_eq!(entry.disposition, Disposition::OmittedPreserved);
            assert_eq!(entry.ledger_id, ledger.source_snapshot());
        }
        assert_eq!(report.validate(&ledger), Ok(()));
    }

    /// The feature ceiling bounds the report: beyond it, distinct findings fold
    /// into one counted overflow bucket rather than growing without bound.
    #[test]
    fn distinct_findings_beyond_the_ceiling_fold_into_a_counted_overflow_bucket() {
        let mut reporter = LossReporter::new(SourceRetention::Regenerated);
        for index in 0..(MAX_REPORT_FEATURES + 10) {
            let feature = format!("f{index:08}");
            let location = FeatureLocation::for_element(&feature);
            reporter.record(feature, location, Finding::Omitted);
        }
        let mut ledger = PreservationLedger::default();
        let report = reporter.into_report(&mut ledger);
        assert_eq!(report.entries.len(), MAX_REPORT_FEATURES + 1);
        let overflow = report
            .entries
            .iter()
            .find(|entry| entry.feature == OVERFLOW_FEATURE)
            .expect("an overflow bucket");
        assert_eq!(overflow.occurrences, 10);
    }
}
