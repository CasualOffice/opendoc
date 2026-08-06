//! Typed, redacted ODF admission and import failures.

use std::error::Error;
use std::fmt;

/// ODF package admission or semantic-import failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OdfError {
    /// Generic bounded ZIP admission failed.
    Package(casual_doc_package::PackageError),
    /// A configured ODF limit exceeds its compiled hard ceiling.
    InvalidLimitConfiguration {
        /// Stable limit name.
        limit: &'static str,
        /// Requested value.
        value: usize,
        /// Compiled maximum.
        hard_ceiling: usize,
    },
    /// ODF metadata exceeds an active limit.
    LimitExceeded {
        /// Stable limit name.
        limit: &'static str,
        /// Observed value.
        observed: usize,
        /// Active maximum.
        allowed: usize,
    },
    /// Cooperative cancellation was requested.
    Cancelled,
    /// A required ODF package part is absent.
    MissingRequiredPart {
        /// Required static part name.
        part: &'static str,
    },
    /// The `mimetype` bytes do not identify an ODT.
    InvalidMimetype,
    /// The ODF `mimetype` part is not ZIP entry zero.
    MimetypeNotFirst,
    /// The ODF `mimetype` part is compressed.
    MimetypeCompressed,
    /// The ODF `mimetype` local header has a forbidden extra field.
    MimetypeExtraField,
    /// `META-INF/manifest.xml` is malformed or has the wrong root.
    MalformedManifest,
    /// The declared ODF version is absent or unsupported.
    UnsupportedVersion,
    /// Manifest entries and admitted ZIP parts disagree.
    ManifestMismatch,
    /// The ODF package declares application-level encryption.
    EncryptedDocument,
    /// The first ODT profile refuses scripts, macros, or executable content.
    ActiveContent,
    /// `content.xml` is malformed or violates the admitted text-document profile.
    MalformedContent,
    /// The ODF body is not an OpenDocument Text body.
    UnsupportedDocumentKind,
    /// Parsed content could not satisfy normalized-model invariants.
    InvalidModel,
    /// Normalized text contains a character XML 1.0 cannot represent.
    InvalidXmlCharacter,
    /// Deterministic ODT XML or ZIP serialization failed.
    SerializationFailed,
    /// A requested admitted ODF part does not exist.
    PartNotFound,
    /// A requested admitted ODF part could not be verified.
    PartReadFailed,
}

impl fmt::Display for OdfError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Package(error) => write!(formatter, "ODF package admission failed: {error}"),
            Self::InvalidLimitConfiguration {
                limit,
                value,
                hard_ceiling,
            } => write!(
                formatter,
                "ODF limit {limit} value {value} exceeds hard ceiling {hard_ceiling}"
            ),
            Self::LimitExceeded {
                limit,
                observed,
                allowed,
            } => write!(
                formatter,
                "ODF limit {limit} exceeded: observed {observed}, allowed {allowed}"
            ),
            Self::Cancelled => formatter.write_str("ODF operation was cancelled"),
            Self::MissingRequiredPart { part } => {
                write!(formatter, "ODF package is missing required part {part}")
            }
            Self::InvalidMimetype => formatter.write_str("ODF mimetype is not OpenDocument Text"),
            Self::MimetypeNotFirst => {
                formatter.write_str("ODF mimetype is not the first ZIP entry")
            }
            Self::MimetypeCompressed => formatter.write_str("ODF mimetype is compressed"),
            Self::MimetypeExtraField => {
                formatter.write_str("ODF mimetype local header contains an extra field")
            }
            Self::MalformedManifest => formatter.write_str("ODF package manifest is malformed"),
            Self::UnsupportedVersion => formatter.write_str("ODF version is unsupported"),
            Self::ManifestMismatch => {
                formatter.write_str("ODF package manifest does not match admitted parts")
            }
            Self::EncryptedDocument => {
                formatter.write_str("encrypted ODF documents are unsupported")
            }
            Self::ActiveContent => formatter.write_str("ODF active content is blocked by policy"),
            Self::MalformedContent => formatter.write_str("ODF document content is malformed"),
            Self::UnsupportedDocumentKind => {
                formatter.write_str("ODF document kind is not supported")
            }
            Self::InvalidModel => {
                formatter.write_str("ODF content does not form a valid normalized document")
            }
            Self::InvalidXmlCharacter => {
                formatter.write_str("normalized text contains an invalid XML character")
            }
            Self::SerializationFailed => formatter.write_str("ODT serialization failed"),
            Self::PartNotFound => formatter.write_str("ODF package part was not found"),
            Self::PartReadFailed => formatter.write_str("ODF package part could not be verified"),
        }
    }
}

impl Error for OdfError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Package(error) => Some(error),
            _ => None,
        }
    }
}

impl From<casual_doc_package::PackageError> for OdfError {
    fn from(error: casual_doc_package::PackageError) -> Self {
        match error {
            casual_doc_package::PackageError::Cancelled => Self::Cancelled,
            casual_doc_package::PackageError::PartNotFound => Self::PartNotFound,
            casual_doc_package::PackageError::PartReadFailed => Self::PartReadFailed,
            error => Self::Package(error),
        }
    }
}
