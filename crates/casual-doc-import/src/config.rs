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
        }
    }
}
