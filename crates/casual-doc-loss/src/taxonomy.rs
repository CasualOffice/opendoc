// SPDX-License-Identifier: Apache-2.0

//! The two orthogonal axes of `35-DISPOSITION-TAXONOMY.md`, and the nine legal
//! pairs of them, as types.

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Every [`Disposition`] variant, for the exhaustiveness assertions below.
    /// A new variant added without a home here fails to compile.
    pub(crate) const ALL_DISPOSITIONS: &[Disposition] = &[
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
        let mut projected: Vec<(ModelOutcome, RetentionOutcome)> = Vec::new();
        for &disposition in ALL_DISPOSITIONS {
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

    /// The same nine pairs, **read out of `35-DISPOSITION-TAXONOMY.md` itself**
    /// rather than transcribed into this file.
    ///
    /// The test above is a transcription, and a transcription is a second copy of
    /// the contract: edit the doc's "Legal combinations" table and the enum keeps
    /// agreeing with a table that no longer exists. This repository has already
    /// published false numbers twice by hand-maintaining what should have been
    /// derived, so the doc is parsed and the two are required to agree exactly — in
    /// both directions, because a pair dropped from the doc and a pair dropped from
    /// the enum are equally wrong.
    #[test]
    fn the_legal_pairs_are_exactly_the_ones_doc_35_lists() {
        const TAXONOMY: &str = include_str!("../../../docs/35-DISPOSITION-TAXONOMY.md");
        let table = TAXONOMY
            .split_once("## Legal combinations")
            .expect("doc 35 states the legal combinations")
            .1
            .split_once("\n##")
            .expect("the section ends")
            .0;
        let mut documented: Vec<(String, String)> = table
            .lines()
            .filter_map(|line| {
                let mut cells = line.split('|').map(str::trim);
                cells.next()?; // the empty cell before the leading `|`
                let model = cells.next()?.trim_matches('`');
                let retention = cells.next()?.trim_matches('`');
                // Skip the header row and its `---` rule; a value row's first cell
                // is one of the three model outcomes.
                ["mapped", "degraded", "omitted"]
                    .contains(&model)
                    .then(|| (model.to_owned(), retention.to_owned()))
            })
            .collect();
        documented.sort();
        documented.dedup();
        assert_eq!(
            documented.len(),
            9,
            "doc 35's table parsed as {documented:?}, which is not nine pairs — \
             either the doc changed shape or this parser is reading it wrongly, and \
             both mean the guard is not checking what it claims"
        );

        let spelling = |model: ModelOutcome, retention: RetentionOutcome| {
            let model = match model {
                ModelOutcome::Mapped => "mapped",
                ModelOutcome::Degraded => "degraded",
                ModelOutcome::Omitted => "omitted",
            };
            let retention = match retention {
                RetentionOutcome::Preserved => "preserved",
                RetentionOutcome::NotRetained => "not-retained",
                RetentionOutcome::Blocked => "blocked",
                RetentionOutcome::Rejected => "rejected",
                RetentionOutcome::NotApplicable => "not-applicable",
            };
            (model.to_owned(), retention.to_owned())
        };
        let mut implemented: Vec<(String, String)> = ALL_DISPOSITIONS
            .iter()
            .map(|disposition| {
                spelling(disposition.model_outcome(), disposition.retention_outcome())
            })
            .collect();
        implemented.sort();
        assert_eq!(
            implemented, documented,
            "the Disposition enum and doc 35's legal-combination table disagree; \
             the taxonomy has one source of truth and this is it"
        );
    }

    /// Only `preserved` may cite a ledger record, and every `preserved` must.
    #[test]
    fn claims_preservation_matches_the_retention_axis() {
        for &disposition in ALL_DISPOSITIONS {
            assert_eq!(
                disposition.claims_preservation(),
                disposition.retention_outcome() == RetentionOutcome::Preserved,
                "{disposition:?}"
            );
        }
    }
}
