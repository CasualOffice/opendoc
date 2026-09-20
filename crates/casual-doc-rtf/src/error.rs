//! Typed, redacted RTF admission and import failures.

use std::error::Error;
use std::fmt;

/// A bounded RTF failure.
///
/// No variant carries document text. An RTF stream is user content, and an
/// error message is a place it can escape into a log, a status bar, or a bug
/// report, so failures name the *construct* and the *bound* and nothing else.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RtfError {
    /// The bytes do not begin with an RTF signature.
    NotRtf,
    /// A configured limit exceeds its compiled hard ceiling.
    LimitAboveCeiling {
        /// Stable limit name.
        limit: &'static str,
        /// Configured value.
        configured: u64,
        /// Compiled hard ceiling.
        ceiling: u64,
    },
    /// An admission bound was reached while parsing.
    LimitExceeded {
        /// Stable limit name.
        limit: &'static str,
        /// Observed value at the moment of refusal.
        observed: u64,
        /// Allowed value.
        allowed: u64,
    },
    /// The control-word stream is not well formed.
    Malformed {
        /// Stable, content-free description of the structural fault.
        reason: &'static str,
    },
    /// The mapped document violated a normalized-model invariant.
    Model {
        /// The model error's own message (model vocabulary, not source text).
        reason: String,
    },
}

impl fmt::Display for RtfError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotRtf => formatter.write_str("input does not carry an RTF signature"),
            Self::LimitAboveCeiling {
                limit,
                configured,
                ceiling,
            } => write!(
                formatter,
                "limit {limit} value {configured} exceeds hard ceiling {ceiling}"
            ),
            Self::LimitExceeded {
                limit,
                observed,
                allowed,
            } => write!(
                formatter,
                "limit {limit} observed {observed} exceeds allowed {allowed}"
            ),
            Self::Malformed { reason } => write!(formatter, "malformed RTF: {reason}"),
            Self::Model { reason } => write!(formatter, "normalized model: {reason}"),
        }
    }
}

impl Error for RtfError {}
