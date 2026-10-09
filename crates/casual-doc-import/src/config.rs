//! Host-configurable import options.

use crate::error::ImportError;

/// How much source detail the import retains for round-trip.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ImportMode {
    /// Model the supported subset; unmapped constructs are reported and dropped
    /// (`not-retained`). No source is retained.
    #[default]
    Semantic,
    /// Additionally retain the original main-document bytes (D5 tier-1 byte
    /// floor), so unmapped constructs are `preserved` and an unedited document
    /// can be reproduced verbatim.
    Retention,
}

/// Host-configurable import options with bounded, non-bypassable ceilings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImportConfig {
    /// Non-zero namespace used to derive deterministic model IDs.
    pub id_namespace: u64,
    /// Whether to retain source for round-trip.
    pub mode: ImportMode,
    /// Maximum XML elements traversed.
    pub max_elements: u64,
    /// Maximum XML nesting depth.
    pub max_depth: u64,
    /// Maximum aggregate text bytes mapped into runs, and the ceiling on
    /// retained source bytes in `Retention` mode.
    pub max_text_bytes: usize,
    /// Whether opening a **damaged** document recovers what the bytes allow and
    /// reports it, instead of refusing.
    ///
    /// Off by default, because every existing caller — the round-trip suites, the
    /// fidelity gates, the writer's fixed-point tests — depends on a refusal
    /// being a refusal. The product path turns it on: a reader who is handed a
    /// damaged file is better served by the document plus a report of what was
    /// repaired than by an error dialog, and a converter pipeline cannot offer
    /// that at all.
    ///
    /// What it does **not** do is widen what counts as damage. A strict open is
    /// still attempted first and still has to fail before any repair is
    /// considered, so a healthy document takes exactly the path it took before
    /// and produces an empty [`crate::RecoveryReport`]. It also does not relax a
    /// resource bound: a limit is a refusal on purpose
    /// ([`crate::ImportError::LimitExceeded`]), and recovering past one would
    /// turn a defence into a suggestion.
    ///
    /// Every repair it performs is reported. A document that opens with half its
    /// tables gone and says nothing is worse than a refusal, because the reader
    /// saves over the original — so the report is the feature, not the document.
    ///
    /// # One consequence for the retention byte floor, stated rather than left to be found
    ///
    /// In [`ImportMode::Retention`] the snapshot retains the **repaired** main
    /// document, not the damaged original, so an "exact if unchanged" export of a
    /// damaged file returns the *fixed* file. That is a deliberate choice between
    /// two imperfect answers, and handing the reader back bytes no consumer can
    /// open is the worse one; the recovery report is non-empty either way, so a
    /// host can say what changed. It does mean the source-snapshot record no
    /// longer reproduces the import input byte for byte when a repair happened.
    /// `35-DISPOSITION-TAXONOMY.md` describes the floor as reproducing the input
    /// exactly, and for a damaged input that is not achievable at the same time
    /// as producing a readable document.
    pub recover: bool,
}

impl ImportConfig {
    const HARD_MAX_ELEMENTS: u64 = 50_000_000;
    const HARD_MAX_DEPTH: u64 = 4_096;
    const HARD_MAX_TEXT_BYTES: usize = 256 * 1024 * 1024;

    pub(crate) fn validate(self) -> Result<(), ImportError> {
        if self.id_namespace == 0
            || self.max_elements > Self::HARD_MAX_ELEMENTS
            || self.max_depth > Self::HARD_MAX_DEPTH
            || self.max_text_bytes > Self::HARD_MAX_TEXT_BYTES
        {
            return Err(ImportError::InvalidConfig);
        }
        Ok(())
    }
}

impl Default for ImportConfig {
    fn default() -> Self {
        Self {
            id_namespace: 1,
            mode: ImportMode::Semantic,
            max_elements: 5_000_000,
            max_depth: 512,
            max_text_bytes: 64 * 1024 * 1024,
            recover: false,
        }
    }
}
