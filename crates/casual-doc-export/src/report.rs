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
//! importer already speaks. Every one of its types — the two axes, the nine-pair
//! [`Disposition`], and [`FeatureLocation`] — is re-exported from
//! `casual-doc-import` rather than redeclared, so the import and export halves of
//! the same format cannot drift into two spellings of one fact. `Disposition`
//! was first written here and then moved to that shared home when the import side
//! needed it too (FID-R-02): one definition, two users, no conversion.
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

pub use casual_doc_import::{Disposition, FeatureLocation, ModelOutcome, RetentionOutcome};

/// Distinct-finding ceiling; excess folds into an `(overflow)` bucket so a
/// pathological document cannot make the report grow without bound.
const MAX_REPORT_FINDINGS: usize = 4_096;

/// One aggregated export finding: a construct the written package does not
/// carry in full.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompatibilityEntry {
    /// Stable `docx.export.*` finding identifier.
    pub feature: String,
    /// Bounded occurrence count (references to the same lost part aggregate).
    pub occurrences: u32,
    /// Where the finding is: the package part it is charged to, and — when the
    /// loss is of one element or one attribute rather than of a whole part — the
    /// element and attribute local names (FID-R-03). Before the report had an
    /// attribute vocabulary, an attribute-level loss could only be spelled as a
    /// feature-level pseudo-name.
    pub location: FeatureLocation,
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
/// `(feature, location)` so repeated references to one lost part become a
/// single entry with a count, and the disposition travels with each record
/// rather than being a property of the run.
#[derive(Debug, Default)]
pub(crate) struct Reporter {
    counts: BTreeMap<(String, FeatureLocation), (u32, Disposition)>,
    overflow: u32,
}

impl Reporter {
    /// Records a finding charged to a package part, naming it so the loss is
    /// auditable against the written package.
    pub(crate) fn record_part(
        &mut self,
        feature: &'static str,
        part_name: &str,
        disposition: Disposition,
    ) {
        self.insert(
            feature,
            FeatureLocation {
                part_name: Some(part_name.to_owned()),
                element: None,
                attribute: None,
            },
            disposition,
        );
    }

    /// Records a finding about one element, or one attribute of it, that the
    /// written package does not carry. The element and attribute local names make
    /// the loss addressable in the source vocabulary instead of only by a stable
    /// finding id (FID-R-03).
    pub(crate) fn record_construct(
        &mut self,
        feature: &'static str,
        part_name: &str,
        element: &'static str,
        attribute: Option<&'static str>,
        disposition: Disposition,
    ) {
        self.insert(
            feature,
            FeatureLocation {
                part_name: Some(part_name.to_owned()),
                element: Some(element.to_owned()),
                attribute: attribute.map(str::to_owned),
            },
            disposition,
        );
    }

    fn insert(
        &mut self,
        feature: &'static str,
        location: FeatureLocation,
        disposition: Disposition,
    ) {
        let key = (feature.to_owned(), location);
        if let Some((count, _)) = self.counts.get_mut(&key) {
            *count = count.saturating_add(1);
        } else if self.counts.len() < MAX_REPORT_FINDINGS {
            self.counts.insert(key, (1, disposition));
        } else {
            self.overflow = self.overflow.saturating_add(1);
        }
    }

    /// Builds the report. `BTreeMap` iteration already orders entries by feature
    /// and then by location, so the output is deterministic without a sort.
    pub(crate) fn finish(self) -> CompatibilityReport {
        let mut entries: Vec<CompatibilityEntry> = self
            .counts
            .into_iter()
            .map(
                |((feature, location), (occurrences, disposition))| CompatibilityEntry {
                    feature,
                    occurrences,
                    location,
                    disposition,
                },
            )
            .collect();
        if self.overflow != 0 {
            entries.push(CompatibilityEntry {
                feature: "docx.export.report.overflow".to_owned(),
                occurrences: self.overflow,
                location: FeatureLocation::default(),
                disposition: Disposition::OmittedNotRetained,
            });
        }
        CompatibilityReport { entries }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The guard that every `Disposition` projects onto exactly one of doc 35's
    // nine legal pairs lives with the definition, in `casual-doc-import`'s
    // `report` module: the enum has one home, so its contract test has one home.

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
            report.entries[0].location.part_name.as_deref(),
            Some("word/media/image1.png")
        );
        assert_eq!(report.entries[1].occurrences, 1);
    }

    #[test]
    fn a_reporter_that_recorded_nothing_produces_an_empty_report() {
        assert!(Reporter::default().finish().is_empty());
    }
}
