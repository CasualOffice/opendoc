//! Configurable normalized snapshot limits and failures.

use std::error::Error;
use std::fmt;

use crate::ModelError;

/// Configurable normalized snapshot limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapshotLimits {
    /// Maximum input JSON bytes checked before parsing.
    pub max_input_bytes: usize,
    /// Maximum body blocks.
    pub max_blocks: usize,
    /// Maximum Unicode scalar values across text runs.
    pub max_unicode_scalar_values: usize,
    /// Maximum UTF-8 bytes in one text run.
    pub max_text_run_bytes: usize,
    /// Maximum extension map entries.
    pub max_extension_entries: usize,
    /// Maximum aggregate extension media-type and payload bytes.
    pub max_extension_bytes: usize,
}

impl SnapshotLimits {
    pub(crate) const HARD_MAX_INPUT_BYTES: usize = 256 * 1024 * 1024;
    const HARD_MAX_BLOCKS: usize = 8_000_000;
    const HARD_MAX_UNICODE_SCALAR_VALUES: usize = 200_000_000;
    const HARD_MAX_TEXT_RUN_BYTES: usize = 64 * 1024 * 1024;
    const HARD_MAX_EXTENSION_ENTRIES: usize = 500_000;
    const HARD_MAX_EXTENSION_BYTES: usize = 256 * 1024 * 1024;

    pub(crate) fn validate(self) -> Result<(), SnapshotError> {
        for (name, value, hard_ceiling) in [
            (
                "input_json_bytes",
                self.max_input_bytes,
                Self::HARD_MAX_INPUT_BYTES,
            ),
            ("body_blocks", self.max_blocks, Self::HARD_MAX_BLOCKS),
            (
                "unicode_scalar_values",
                self.max_unicode_scalar_values,
                Self::HARD_MAX_UNICODE_SCALAR_VALUES,
            ),
            (
                "text_run_bytes",
                self.max_text_run_bytes,
                Self::HARD_MAX_TEXT_RUN_BYTES,
            ),
            (
                "extension_entries",
                self.max_extension_entries,
                Self::HARD_MAX_EXTENSION_ENTRIES,
            ),
            (
                "extension_payload_bytes",
                self.max_extension_bytes,
                Self::HARD_MAX_EXTENSION_BYTES,
            ),
        ] {
            if value > hard_ceiling {
                return Err(SnapshotError::InvalidLimitConfiguration {
                    limit: name,
                    value,
                    hard_ceiling,
                });
            }
        }
        Ok(())
    }
}

impl Default for SnapshotLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 64 * 1024 * 1024,
            max_blocks: 2_000_000,
            max_unicode_scalar_values: 50_000_000,
            max_text_run_bytes: 16 * 1024 * 1024,
            max_extension_entries: 100_000,
            max_extension_bytes: 64 * 1024 * 1024,
        }
    }
}

/// Normalized snapshot parsing, limit, or serialization failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SnapshotError {
    /// A configured limit exceeded the runtime hard ceiling.
    InvalidLimitConfiguration {
        /// Stable limit name.
        limit: &'static str,
        /// Requested value.
        value: usize,
        /// Maximum permitted value.
        hard_ceiling: usize,
    },
    /// Input exceeded a configured limit.
    LimitExceeded {
        /// Stable limit name.
        limit: &'static str,
        /// Observed value.
        observed: usize,
        /// Configured allowed value.
        allowed: usize,
    },
    /// JSON syntax or strict schema decoding failed.
    MalformedJson,
    /// Parsed content violated normalized model invariants.
    InvalidModel(ModelError),
    /// A valid committed document could not be serialized.
    Serialization,
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimitConfiguration {
                limit,
                value,
                hard_ceiling,
            } => write!(
                formatter,
                "limit {limit} value {value} exceeds hard ceiling {hard_ceiling}"
            ),
            Self::LimitExceeded {
                limit,
                observed,
                allowed,
            } => write!(
                formatter,
                "limit {limit} observed {observed} exceeds allowed {allowed}"
            ),
            Self::MalformedJson => formatter.write_str("normalized JSON is malformed"),
            Self::InvalidModel(error) => write!(formatter, "normalized model is invalid: {error}"),
            Self::Serialization => formatter.write_str("normalized JSON serialization failed"),
        }
    }
}

impl Error for SnapshotError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidModel(error) => Some(error),
            _ => None,
        }
    }
}

pub(crate) fn enforce_limit(
    limit: &'static str,
    observed: usize,
    allowed: usize,
) -> Result<(), SnapshotError> {
    if observed > allowed {
        return Err(SnapshotError::LimitExceeded {
            limit,
            observed,
            allowed,
        });
    }
    Ok(())
}
