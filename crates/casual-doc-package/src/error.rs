//! Typed bounded-ZIP admission and part-read failures.

use std::error::Error;
use std::fmt;

/// Format-neutral ZIP package admission or part-read failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackageError {
    /// A host limit exceeds its non-bypassable hard ceiling.
    InvalidLimitConfiguration {
        /// Stable limit name.
        limit: &'static str,
        /// Requested value.
        value: u64,
        /// Non-bypassable maximum.
        hard_ceiling: u64,
    },
    /// Package metadata exceeds an active resource limit.
    LimitExceeded {
        /// Stable limit name.
        limit: &'static str,
        /// Observed value.
        observed: u64,
        /// Active allowed value.
        allowed: u64,
    },
    /// ZIP records are malformed or inconsistent.
    MalformedArchive,
    /// Package work was cooperatively cancelled.
    Cancelled,
    /// A package path is unsafe or outside the accepted generic profile.
    UnsafePartName,
    /// Two records resolve to the same normalized package part.
    DuplicatePart,
    /// An encrypted ZIP entry is unsupported at this substrate boundary.
    EncryptedEntry,
    /// A ZIP entry uses a compression method outside stored/deflated.
    UnsupportedCompression,
    /// Compressed data ranges overlap.
    OverlappingEntries,
    /// A symbolic link or other special entry is unsupported.
    SpecialEntry,
    /// A requested admitted part does not exist.
    PartNotFound,
    /// A part could not be fully decompressed and verified.
    PartReadFailed,
}

impl fmt::Display for PackageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimitConfiguration {
                limit,
                value,
                hard_ceiling,
            } => write!(
                formatter,
                "package limit {limit} value {value} exceeds hard ceiling {hard_ceiling}"
            ),
            Self::LimitExceeded {
                limit,
                observed,
                allowed,
            } => write!(
                formatter,
                "package limit {limit} exceeded: observed {observed}, allowed {allowed}"
            ),
            Self::MalformedArchive => formatter.write_str("ZIP package structure is malformed"),
            Self::Cancelled => formatter.write_str("ZIP package operation was cancelled"),
            Self::UnsafePartName => formatter.write_str("ZIP package part name is unsafe"),
            Self::DuplicatePart => formatter.write_str("ZIP package contains a duplicate part"),
            Self::EncryptedEntry => formatter.write_str("encrypted ZIP entries are unsupported"),
            Self::UnsupportedCompression => {
                formatter.write_str("ZIP entry compression method is unsupported")
            }
            Self::OverlappingEntries => formatter.write_str("ZIP entry data ranges overlap"),
            Self::SpecialEntry => formatter.write_str("ZIP package contains a special entry"),
            Self::PartNotFound => formatter.write_str("ZIP package part was not found"),
            Self::PartReadFailed => {
                formatter.write_str("ZIP package part could not be fully verified")
            }
        }
    }
}

impl Error for PackageError {}
