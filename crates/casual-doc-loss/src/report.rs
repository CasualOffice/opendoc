// SPDX-License-Identifier: Apache-2.0

//! The bounded, deterministic compatibility report, and the validation that
//! turns a `preserved` claim into an auditable one.

use crate::ledger::{LedgerId, PreservationLedger};
use crate::taxonomy::{Disposition, ModelOutcome, RetentionOutcome};

/// Bounded location of a compatibility finding.
///
/// `element` and `attribute` are **local** names in whatever markup vocabulary
/// the adapter read. The namespace *prefix* a producer chose is deliberately not
/// recorded: it is arbitrary (`w14:paraId` and `x:paraId` are the same attribute
/// if both prefixes bind the same URI), so a prefix in a stable feature
/// identifier would make the identifier a property of the writer rather than of
/// the construct.
///
/// Every field is optional because a location is only populated where it is
/// genuinely known. An adapter handed part *bytes* rather than a part name must
/// leave `part_name` `None` rather than filling in a conventional name the
/// package need not use: `35-DISPOSITION-TAXONOMY.md` is explicit that an
/// invented location is worse than none.
#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct FeatureLocation {
    /// Normalized package part the finding is charged to, when known.
    pub part_name: Option<String>,
    /// Local name of the element the finding is about.
    pub element: Option<String>,
    /// Local name of the attribute, when the finding is about an attribute of
    /// `element` rather than the element itself.
    pub attribute: Option<String>,
}

impl FeatureLocation {
    /// A location naming only an element.
    #[must_use]
    pub fn for_element(element: &str) -> Self {
        Self {
            part_name: None,
            element: Some(element.to_owned()),
            attribute: None,
        }
    }

    /// A location naming an attribute of an element.
    #[must_use]
    pub fn for_attribute(element: &str, attribute: &str) -> Self {
        Self {
            part_name: None,
            element: Some(element.to_owned()),
            attribute: Some(attribute.to_owned()),
        }
    }

    /// A location naming a whole package part.
    #[must_use]
    pub fn for_part(part_name: &str) -> Self {
        Self {
            part_name: Some(part_name.to_owned()),
            element: None,
            attribute: None,
        }
    }
}

/// A violation of the `35-DISPOSITION-TAXONOMY.md` preservation rule.
///
/// `35` is explicit that a pairing it does not admit "is an internal error and
/// must fail import, not be reported", so a caller surfaces this as an import
/// error rather than as a report entry.
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
/// outcome depends on preservation: a part carried verbatim through the semantic
/// writer via an opaque side-table is `preserved`; a digital signature is
/// `blocked` on the semantic path, because retention is refused for a security
/// reason rather than merely declined. Under a whole-source byte floor every
/// part is kept, so all are `preserved`. Carries the part name and its declared
/// content type so the disposition is auditable per `35`.
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
pub type WholePartDisposition = (PartDisposition, Disposition, Option<LedgerId>);

/// One finding about a *construct inside* a package part, charged to that part.
///
/// Distinct from [`WholePartDisposition`], which is about a part an adapter did
/// not consume at all. This describes a part the adapter *did* read, reporting
/// what inside it did not survive the projection — the shape a chart, diagram or
/// embedded-object reader produces.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PartConstructDisposition {
    /// The part the finding is charged to.
    pub part: PartDisposition,
    /// Stable feature identifier, already qualified by the adapter. A construct
    /// name alone is not enough: unrelated vocabularies reuse local names, and a
    /// finding that aggregated two of them would describe neither.
    pub feature: String,
    /// The construct's local name, for the bounded location.
    pub element: Option<String>,
    /// The per-construct disposition.
    pub disposition: Disposition,
    /// The preservation-ledger record licensing a `preserved` claim.
    pub ledger_id: Option<LedgerId>,
}

/// One compatibility-report entry, aggregated by feature and disposition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompatibilityEntry {
    /// Stable adapter-defined feature identifier: a markup local name,
    /// `element/@attribute` for an attribute finding, an admitted part name (for
    /// a whole-part disposition), a dotted class identifier, or `(overflow)`.
    pub feature: String,
    /// Bounded occurrence count.
    pub occurrences: u32,
    /// Where the finding is, to the granularity the adapter actually knows.
    pub location: FeatureLocation,
    /// The per-construct disposition; both taxonomy axes derive from it.
    pub disposition: Disposition,
    /// The preservation-ledger record licensing a `preserved` retention outcome.
    /// `Some` exactly when [`Disposition::claims_preservation`] holds, which
    /// [`CompatibilityReport::validate`] enforces.
    pub ledger_id: Option<LedgerId>,
    /// Set when this entry dispositions a whole admitted part the semantic model
    /// does not consume, or a construct charged to a part; `None` for
    /// element/attribute feature entries.
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
    /// Whether the adapter found nothing it could not fully represent.
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
    ///
    /// Complexity: O(entries), with an O(1) ledger lookup per entry.
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

    /// Folds a second report's entries into this one, aggregating by feature
    /// *and disposition* and preserving the deterministic ordering.
    ///
    /// Two findings that share a feature name but differ in disposition are
    /// different fidelity facts and stay separate entries. Used where one source
    /// is read in more than one pass — package property parts hang off the
    /// package root rather than off the main document, so they are parsed
    /// separately and their findings merged.
    ///
    /// Complexity: O(self.entries x other.entries) plus the sort. Intended for a
    /// handful of passes per document, not per construct.
    pub fn merge(&mut self, other: Self) {
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

    /// Restores the deterministic order: by feature name, then by disposition.
    ///
    /// Public because an adapter may append its own findings to a report built
    /// here, and a report whose order depends on insertion order is not
    /// reproducible.
    pub fn sort(&mut self) {
        self.entries.sort_by(|left, right| {
            left.feature
                .cmp(&right.feature)
                .then_with(|| left.disposition.cmp(&right.disposition))
        });
    }

    /// Appends a whole-part disposition for every admitted part the semantic
    /// model does not consume, closing the silent-whole-part-loss class. Each
    /// part carries its own disposition and, when it claims preservation, the
    /// ledger record that licenses the claim. Re-sorts deterministically so the
    /// report order is stable regardless of insertion order.
    pub fn add_part_dispositions(&mut self, parts: impl IntoIterator<Item = WholePartDisposition>) {
        for (part, disposition, ledger_id) in parts {
            self.entries.push(CompatibilityEntry {
                feature: part.part_name.clone(),
                occurrences: 1,
                location: FeatureLocation::for_part(&part.part_name),
                disposition,
                ledger_id,
                part: Some(part),
            });
        }
        self.sort();
    }

    /// Appends one finding per construct inside a part that the adapter's
    /// projection of that part did not represent, charged to the part it was
    /// found in.
    ///
    /// The feature identifier arrives already qualified by the adapter, because
    /// qualifying it is a vocabulary decision: a chart's unmodeled series fill
    /// and a shape's unmodeled fill are both `spPr`, and one entry covering both
    /// would describe neither.
    pub fn add_part_constructs(
        &mut self,
        constructs: impl IntoIterator<Item = PartConstructDisposition>,
    ) {
        for construct in constructs {
            self.entries.push(CompatibilityEntry {
                feature: construct.feature,
                occurrences: 1,
                location: FeatureLocation {
                    part_name: Some(construct.part.part_name.clone()),
                    element: construct.element,
                    attribute: None,
                },
                disposition: construct.disposition,
                ledger_id: construct.ledger_id,
                part: Some(construct.part),
            });
        }
        self.sort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::PreservationLedger;

    fn entry(disposition: Disposition, ledger_id: Option<LedgerId>) -> CompatibilityEntry {
        CompatibilityEntry {
            feature: "tblStyle".to_owned(),
            occurrences: 1,
            location: FeatureLocation::for_element("tblStyle"),
            disposition,
            ledger_id,
            part: None,
        }
    }

    /// A `preserved` entry with no ledger record must fail the import, not be
    /// reported. This is the rule a type cannot carry, so it is validated.
    #[test]
    fn validation_refuses_preserved_without_a_ledger_record() {
        let report = CompatibilityReport {
            entries: vec![entry(Disposition::OmittedPreserved, None)],
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
            entries: vec![entry(Disposition::OmittedPreserved, Some(ledger_id))],
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
        let ledger_id = LedgerId::from_index(7);
        let report = CompatibilityReport {
            entries: vec![entry(Disposition::OmittedPreserved, Some(ledger_id))],
        };
        assert_eq!(
            report.validate(&PreservationLedger::default()),
            Err(DispositionViolation::RecordMissing {
                feature: "tblStyle".to_owned(),
                ledger_id
            })
        );
    }

    /// The converse: citing a record while reporting the remainder dropped
    /// asserts a preservation the disposition denies.
    #[test]
    fn validation_refuses_a_record_reference_without_a_preservation_claim() {
        let ledger = PreservationLedger::with_source_snapshot(16);
        let report = CompatibilityReport {
            entries: vec![entry(
                Disposition::OmittedNotRetained,
                ledger.source_snapshot(),
            )],
        };
        assert_eq!(
            report.validate(&ledger),
            Err(DispositionViolation::RecordWithoutPreservation {
                feature: "tblStyle".to_owned()
            })
        );
    }

    /// A legal report passes, and every disposition that claims preservation is
    /// exercised against a real record rather than only the failure paths.
    #[test]
    fn validation_accepts_every_preserving_disposition_with_a_real_record() {
        let ledger = PreservationLedger::with_source_snapshot(16);
        for disposition in [
            Disposition::MappedPreserved,
            Disposition::DegradedPreserved,
            Disposition::OmittedPreserved,
        ] {
            let report = CompatibilityReport {
                entries: vec![entry(disposition, ledger.source_snapshot())],
            };
            assert_eq!(report.validate(&ledger), Ok(()), "{disposition:?}");
        }
        for disposition in [
            Disposition::MappedComplete,
            Disposition::DegradedNotRetained,
            Disposition::DegradedBlocked,
            Disposition::OmittedNotRetained,
            Disposition::OmittedBlocked,
            Disposition::OmittedRejected,
        ] {
            let report = CompatibilityReport {
                entries: vec![entry(disposition, None)],
            };
            assert_eq!(report.validate(&ledger), Ok(()), "{disposition:?}");
        }
    }

    /// Merging aggregates on feature *and* disposition, and leaves the result
    /// deterministically ordered.
    #[test]
    fn merging_keeps_two_dispositions_of_one_feature_apart() {
        let mut report = CompatibilityReport {
            entries: vec![entry(Disposition::OmittedNotRetained, None)],
        };
        report.merge(CompatibilityReport {
            entries: vec![
                entry(Disposition::OmittedNotRetained, None),
                entry(Disposition::OmittedRejected, None),
            ],
        });
        assert_eq!(report.entries.len(), 2);
        assert_eq!(
            report.entries[0].disposition,
            Disposition::OmittedNotRetained
        );
        assert_eq!(report.entries[0].occurrences, 2);
        assert_eq!(report.entries[1].disposition, Disposition::OmittedRejected);
        assert_eq!(report.entries[1].occurrences, 1);
    }

    /// A whole-part disposition names the part in both the feature and the
    /// location, and carries the declared content type for the audit.
    #[test]
    fn a_whole_part_disposition_names_its_part_and_content_type() {
        let mut report = CompatibilityReport::default();
        let mut ledger = PreservationLedger::default();
        let ledger_id = ledger.record_opaque_part("customXml/item1.xml", 12);
        report.add_part_dispositions([(
            PartDisposition {
                part_name: "customXml/item1.xml".to_owned(),
                content_type: Some("application/xml".to_owned()),
            },
            Disposition::OmittedPreserved,
            Some(ledger_id),
        )]);
        let entry = &report.entries[0];
        assert_eq!(entry.feature, "customXml/item1.xml");
        assert_eq!(
            entry.location.part_name.as_deref(),
            Some("customXml/item1.xml")
        );
        assert_eq!(
            entry
                .part
                .as_ref()
                .and_then(|part| part.content_type.as_deref()),
            Some("application/xml")
        );
        assert_eq!(report.validate(&ledger), Ok(()));
    }

    /// A construct charged to a part keeps the part in the location and the
    /// adapter's qualified feature name in the feature.
    #[test]
    fn a_part_construct_keeps_the_part_and_the_qualified_feature() {
        let mut report = CompatibilityReport::default();
        report.add_part_constructs([PartConstructDisposition {
            part: PartDisposition {
                part_name: "word/charts/chart1.xml".to_owned(),
                content_type: None,
            },
            feature: "chart.spPr".to_owned(),
            element: Some("spPr".to_owned()),
            disposition: Disposition::OmittedNotRetained,
            ledger_id: None,
        }]);
        let entry = &report.entries[0];
        assert_eq!(entry.feature, "chart.spPr");
        assert_eq!(entry.location.element.as_deref(), Some("spPr"));
        assert_eq!(
            entry.location.part_name.as_deref(),
            Some("word/charts/chart1.xml")
        );
    }
}
