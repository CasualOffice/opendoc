//! Typed DOCX package admission and part-read failures.

use std::error::Error;
use std::fmt;

/// DOCX package admission or part-read failure.
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
    /// A package path is unsafe or outside the accepted profile.
    UnsafePartName,
    /// Two records resolve to the same normalized package part.
    DuplicatePart,
    /// An encrypted ZIP entry is unsupported.
    EncryptedEntry,
    /// A ZIP entry uses a compression method outside the DOCX profile.
    UnsupportedCompression,
    /// Compressed data ranges overlap.
    OverlappingEntries,
    /// A symbolic link or other special entry is unsupported.
    SpecialEntry,
    /// A macro project part is unsupported.
    MacroPart,
    /// A minimal DOCX package part is missing.
    MissingRequiredPart {
        /// Required static part name.
        part: &'static str,
    },
    /// Package-metadata XML (relationships or content types) is malformed.
    MalformedPackageXml {
        /// Static part name of the offending metadata part.
        part: &'static str,
    },
    /// No `officeDocument` relationship resolves to an admitted main document.
    MissingMainDocument,
    /// More than one `officeDocument` relationship is present.
    AmbiguousMainDocument,
    /// The discovered main document does not carry a WordprocessingML type.
    UnsupportedMainDocumentType,
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
            Self::MalformedArchive => formatter.write_str("DOCX ZIP structure is malformed"),
            Self::Cancelled => formatter.write_str("DOCX package operation was cancelled"),
            Self::UnsafePartName => formatter.write_str("DOCX package part name is unsafe"),
            Self::DuplicatePart => formatter.write_str("DOCX package contains a duplicate part"),
            Self::EncryptedEntry => formatter.write_str("encrypted DOCX entries are unsupported"),
            Self::UnsupportedCompression => {
                formatter.write_str("DOCX entry compression method is unsupported")
            }
            Self::OverlappingEntries => formatter.write_str("DOCX ZIP entry data ranges overlap"),
            Self::SpecialEntry => {
                formatter.write_str("DOCX package contains a special filesystem entry")
            }
            Self::MacroPart => formatter.write_str("DOCX macro project parts are unsupported"),
            Self::MissingRequiredPart { part } => {
                write!(formatter, "DOCX package is missing required part {part}")
            }
            Self::MalformedPackageXml { part } => {
                write!(formatter, "DOCX package metadata part {part} is malformed")
            }
            Self::MissingMainDocument => {
                formatter.write_str("DOCX package has no resolvable main document relationship")
            }
            Self::AmbiguousMainDocument => {
                formatter.write_str("DOCX package declares more than one main document")
            }
            Self::UnsupportedMainDocumentType => {
                formatter.write_str("DOCX main document content type is unsupported")
            }
            Self::PartNotFound => formatter.write_str("DOCX package part was not found"),
            Self::PartReadFailed => {
                formatter.write_str("DOCX package part could not be fully verified")
            }
        }
    }
}

impl Error for PackageError {}
