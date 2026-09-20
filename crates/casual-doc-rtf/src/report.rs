//! RTF-layer compatibility findings.
//!
//! The vocabulary mirrors the format-neutral one in `casual-doc-io` (doc 94)
//! so the adapter's translation is mechanical, and so this crate does not
//! depend on the dispatch layer that depends on it.

use std::collections::BTreeMap;

use crate::limits::enforce;
use crate::{RtfError, RtfLimits};

/// How a source construct was represented in the normalized model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RtfModelOutcome {
    /// Fully represented.
    Mapped,
    /// Partially represented.
    Degraded,
    /// Not represented.
    Omitted,
}

/// What happened to source detail the normalized model did not consume.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RtfRetentionOutcome {
    /// Retained in validated sidecar state.
    Preserved,
    /// Intentionally and reportably not retained.
    NotRetained,
    /// The construct was fully mapped with no remainder.
    NotApplicable,
}

/// One aggregated RTF compatibility finding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RtfCompatibilityEntry {
    /// Stable feature identifier, e.g. `rtf.note`.
    pub feature: String,
    /// The control word the finding is about, when one names it. Never source
    /// text: only a control word, which is part of the format's grammar.
    pub control_word: Option<String>,
    /// Bounded occurrence count.
    pub occurrences: u32,
    /// Semantic mapping result.
    pub model_outcome: RtfModelOutcome,
    /// Preservation result.
    pub retention_outcome: RtfRetentionOutcome,
}

/// Deterministically ordered RTF import findings.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RtfCompatibilityReport {
    /// Findings ordered by feature id, then by control word.
    pub entries: Vec<RtfCompatibilityEntry>,
}

impl RtfCompatibilityReport {
    /// Returns whether a feature was reported.
    #[must_use]
    pub fn has(&self, feature: &str) -> bool {
        self.entries.iter().any(|entry| entry.feature == feature)
    }
}

/// Bounded accumulator that aggregates repeats instead of growing per site.
#[derive(Debug, Default)]
pub(crate) struct Losses {
    entries: BTreeMap<(&'static str, Option<String>), (u32, RtfModelOutcome, RtfRetentionOutcome)>,
}

impl Losses {
    pub(crate) fn record(
        &mut self,
        feature: &'static str,
        model: RtfModelOutcome,
        limits: RtfLimits,
    ) -> Result<(), RtfError> {
        self.record_named(feature, None, model, limits)
    }

    /// Records a finding that names the control word responsible.
    ///
    /// The name is bounded by the lexer's own control-word ceiling before it
    /// reaches here, so an aggregating key cannot be grown by a hostile file.
    pub(crate) fn record_named(
        &mut self,
        feature: &'static str,
        control_word: Option<&str>,
        model: RtfModelOutcome,
        limits: RtfLimits,
    ) -> Result<(), RtfError> {
        let key = (feature, control_word.map(str::to_owned));
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.0 = entry.0.saturating_add(1);
            return Ok(());
        }
        enforce("rtf_findings", self.entries.len() + 1, limits.max_findings)?;
        let retention = match model {
            RtfModelOutcome::Mapped => RtfRetentionOutcome::NotApplicable,
            RtfModelOutcome::Degraded | RtfModelOutcome::Omitted => {
                RtfRetentionOutcome::NotRetained
            }
        };
        self.entries.insert(key, (1, model, retention));
        Ok(())
    }

    pub(crate) fn finish(self) -> RtfCompatibilityReport {
        RtfCompatibilityReport {
            entries: self
                .entries
                .into_iter()
                .map(
                    |((feature, control_word), (occurrences, model, retention))| {
                        RtfCompatibilityEntry {
                            feature: feature.to_owned(),
                            control_word,
                            occurrences,
                            model_outcome: model,
                            retention_outcome: retention,
                        }
                    },
                )
                .collect(),
        }
    }
}
