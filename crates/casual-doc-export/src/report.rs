//! The bounded, deterministic DOCX **export** compatibility report (FID-R-01).
//!
//! The writer used to hand back bytes and nothing else, so the format adapter
//! above it had nothing to report and filled in
//! `CompatibilityReport::default()` unconditionally. Export-side loss on the
//! primary format was therefore not merely unreported but *structurally
//! unreportable*: there was no channel for a finding to travel on. Verbatim
//! retention is only an advantage over a converter pipeline if the loss that
//! retention does not cover is detected and named, so this channel is part of
//! the fidelity story rather than hygiene.
//!
//! The vocabulary is the one `35-DISPOSITION-TAXONOMY.md` mandates and the DOCX
//! importer already speaks: two orthogonal axes on every entry. Both enums are
//! re-exported from `casual-doc-import` rather than redeclared, so the import
//! and export halves of the same format cannot drift into two spellings of one
//! fact.
//!
//! Two properties are deliberate:
//!
//! - **Only loss is recorded.** A construct the writer emits in full raises
//!   nothing, so an ordinary document exports with an *empty* report. A report
//!   that fires on healthy documents is one every caller learns to ignore, and
//!   an ignored report is worse than none.
//! - **The disposition pair is chosen per finding, never per mode.** Each call
//!   site states what happened to that construct, and only the nine pairs doc 35
//!   admits can be expressed at all ([`Disposition`]) — an illegal pair is
//!   unrepresentable rather than validated after the fact.

use std::collections::BTreeMap;

pub use casual_doc_import::{ModelOutcome, RetentionOutcome};

/// Distinct-finding ceiling; excess folds into an `(overflow)` bucket so a
/// pathological document cannot make the report grow without bound.
const MAX_REPORT_FINDINGS: usize = 4_096;

/// One of the nine per-construct dispositions `35-DISPOSITION-TAXONOMY.md`
/// admits, as a single value.
///
/// The taxonomy is two orthogonal axes, but only nine of their fifteen pairs are
/// meaningful; doc 35 says any other pairing "is an internal error and must fail
/// import, not be reported". Naming the legal pairs makes the illegal ones
/// impossible to construct, which is stronger than checking for them: there is
/// no code path on which an export can emit `mapped` + `rejected`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
}

/// One aggregated export finding: a construct the written package does not
/// carry in full.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompatibilityEntry {
    /// Stable `docx.export.*` finding identifier.
    pub feature: String,
    /// Bounded occurrence count (references to the same lost part aggregate).
    pub occurrences: u32,
    /// The package part the finding is charged to, when it is about one.
    pub part_name: Option<String>,
    /// The per-construct disposition; both taxonomy axes derive from it.
    pub disposition: Disposition,
}

impl CompatibilityEntry {
    /// Axis A — what the written package captured of the construct.
    #[must_use]
    pub const fn model_outcome(&self) -> ModelOutcome {
        self.disposition.model_outcome()
    }

    /// Axis B — what happened to the detail the package did not carry.
    #[must_use]
    pub const fn retention_outcome(&self) -> RetentionOutcome {
        self.disposition.retention_outcome()
    }
}

/// A deterministically ordered set of export findings. Empty for a document the
/// writer emits in full.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompatibilityReport {
    /// Findings ordered by feature, then by part name.
    pub entries: Vec<CompatibilityEntry>,
}

impl CompatibilityReport {
    /// Whether the export lost nothing the writer knows how to notice.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// A written DOCX package together with what writing it cost.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocxExport {
    /// Complete DOCX package bytes.
    pub bytes: Vec<u8>,
    /// Findings for model content the package does not carry in full.
    pub report: CompatibilityReport,
}

/// Aggregating sink used by the semantic writer. Findings are keyed by
/// `(feature, part name)` so repeated references to one lost part become a
/// single entry with a count, and the disposition travels with each record
/// rather than being a property of the run.
#[derive(Debug, Default)]
pub(crate) struct Reporter {
    counts: BTreeMap<(String, Option<String>), (u32, Disposition)>,
    overflow: u32,
}

impl Reporter {
    /// Records a finding that is not about a specific package part.
    pub(crate) fn record(&mut self, feature: &'static str, disposition: Disposition) {
        self.insert(feature, None, disposition);
    }

    /// Records a finding charged to a package part, naming it so the loss is
    /// auditable against the written package.
    pub(crate) fn record_part(
        &mut self,
        feature: &'static str,
        part_name: &str,
        disposition: Disposition,
    ) {
        self.insert(feature, Some(part_name.to_owned()), disposition);
    }

    fn insert(
        &mut self,
        feature: &'static str,
        part_name: Option<String>,
        disposition: Disposition,
    ) {
        let key = (feature.to_owned(), part_name);
        if let Some((count, _)) = self.counts.get_mut(&key) {
            *count = count.saturating_add(1);
        } else if self.counts.len() < MAX_REPORT_FINDINGS {
            self.counts.insert(key, (1, disposition));
        } else {
            self.overflow = self.overflow.saturating_add(1);
        }
    }

    /// Builds the report. `BTreeMap` iteration already orders entries by feature
    /// and then by part name, so the output is deterministic without a sort.
    pub(crate) fn finish(self) -> CompatibilityReport {
        let mut entries: Vec<CompatibilityEntry> = self
            .counts
            .into_iter()
            .map(
                |((feature, part_name), (occurrences, disposition))| CompatibilityEntry {
                    feature,
                    occurrences,
                    part_name,
                    disposition,
                },
            )
            .collect();
        if self.overflow != 0 {
            entries.push(CompatibilityEntry {
                feature: "docx.export.report.overflow".to_owned(),
                occurrences: self.overflow,
                part_name: None,
                disposition: Disposition::OmittedNotRetained,
            });
        }
        CompatibilityReport { entries }
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

    /// Repeated references to one lost part are one finding with a count, and
    /// two different parts stay two findings.
    #[test]
    fn findings_aggregate_by_feature_and_part() {
        let mut reporter = Reporter::default();
        reporter.record_part(
            "docx.export.media.missing_bytes",
            "word/media/image1.png",
            Disposition::OmittedNotRetained,
        );
        reporter.record_part(
            "docx.export.media.missing_bytes",
            "word/media/image1.png",
            Disposition::OmittedNotRetained,
        );
        reporter.record_part(
            "docx.export.media.missing_bytes",
            "word/media/image2.png",
            Disposition::OmittedNotRetained,
        );
        let report = reporter.finish();
        assert_eq!(report.entries.len(), 2);
        assert_eq!(report.entries[0].occurrences, 2);
        assert_eq!(
            report.entries[0].part_name.as_deref(),
            Some("word/media/image1.png")
        );
        assert_eq!(report.entries[1].occurrences, 1);
    }

    #[test]
    fn a_reporter_that_recorded_nothing_produces_an_empty_report() {
        assert!(Reporter::default().finish().is_empty());
    }
}
