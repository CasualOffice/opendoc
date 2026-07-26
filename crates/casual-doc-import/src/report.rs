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

/// A whole-part disposition: an admitted package part the semantic model does
/// not consume. Such a part is `omitted` (never in the model). Its retention
/// outcome depends on preservation (P1F-2): a part carried verbatim through the
/// semantic writer via the opaque side-table is `preserved`; a digital signature
/// (not preserved, because editing invalidates it) is `not-retained` on the
/// semantic path. In Retention mode the source byte floor keeps every part, so
/// all are `preserved`. Carries the part name and its declared content type so
/// the disposition is auditable per `35-DISPOSITION-TAXONOMY`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PartDisposition {
    /// Normalized package part name (e.g. `customXml/item1.xml`).
    pub part_name: String,
    /// Declared content type, if the package declared one.
    pub content_type: Option<String>,
}

/// One compatibility-report entry, aggregated by feature.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompatibilityEntry {
    /// Feature: a WordprocessingML local element name, an admitted part name
    /// (for a whole-part disposition), or `(overflow)`.
    pub feature: String,
    /// Bounded occurrence count.
    pub occurrences: u32,
    /// Model outcome.
    pub model_outcome: ModelOutcome,
    /// Retention outcome.
    pub retention_outcome: RetentionOutcome,
    /// Set when this entry dispositions a whole admitted part the semantic model
    /// does not consume; `None` for element/attribute feature entries.
    pub part: Option<PartDisposition>,
}

/// A deterministic compatibility report ordered by feature name.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompatibilityReport {
    /// Entries ordered by feature name.
    pub entries: Vec<CompatibilityEntry>,
}

impl CompatibilityReport {
    /// Folds a second report's entries (e.g. unmapped `docProps` fields
    /// discovered after the main body pass, which is parsed separately because
    /// the property parts hang off the package root, not the main document)
    /// into this one, aggregating by feature and preserving the deterministic
    /// feature ordering. An entry seen in both reports sums its occurrences.
    pub(crate) fn merge(&mut self, other: Self) {
        for entry in other.entries {
            match self
                .entries
                .iter_mut()
                .find(|existing| existing.feature == entry.feature)
            {
                Some(existing) => {
                    existing.occurrences = existing.occurrences.saturating_add(entry.occurrences);
                }
                None => self.entries.push(entry),
            }
        }
        self.entries
            .sort_by(|left, right| left.feature.cmp(&right.feature));
    }
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

    /// Builds the report, dispositioning every reported (unmapped) construct
    /// with `retention` — `NotRetained` in Semantic mode, `Preserved` in
    /// Retention mode (covered by the retained source byte floor).
    pub(crate) fn into_report(self, retention: RetentionOutcome) -> CompatibilityReport {
        let mut entries: Vec<CompatibilityEntry> = self
            .counts
            .into_iter()
            .map(|(feature, occurrences)| CompatibilityEntry {
                feature,
                occurrences,
                model_outcome: ModelOutcome::Omitted,
                retention_outcome: retention,
                part: None,
            })
            .collect();
        if self.overflow > 0 {
            entries.push(CompatibilityEntry {
                feature: "(overflow)".to_owned(),
                occurrences: self.overflow,
                model_outcome: ModelOutcome::Omitted,
                retention_outcome: retention,
                part: None,
            });
        }
        entries.sort_by(|left, right| left.feature.cmp(&right.feature));
        CompatibilityReport { entries }
    }
}

impl CompatibilityReport {
    /// Appends a whole-part disposition for every admitted part the semantic
    /// model does not consume, closing the silent-whole-part-loss class. Each
    /// part carries its own retention outcome (P1F-2): a part preserved through
    /// the semantic writer via the opaque side-table is `omitted` + `preserved`;
    /// a digital signature — deliberately not preserved on the semantic path —
    /// is `omitted` + `not-retained` in Semantic mode. Re-sorts deterministically
    /// by feature so the report order is stable regardless of insertion order.
    pub(crate) fn add_part_dispositions(
        &mut self,
        parts: impl IntoIterator<Item = (PartDisposition, RetentionOutcome)>,
    ) {
        for (part, retention) in parts {
            self.entries.push(CompatibilityEntry {
                feature: part.part_name.clone(),
                occurrences: 1,
                model_outcome: ModelOutcome::Omitted,
                retention_outcome: retention,
                part: Some(part),
            });
        }
        self.entries
            .sort_by(|left, right| left.feature.cmp(&right.feature));
    }
}
