//! Format-dispatch and adapter errors.

use std::error::Error;
use std::fmt;

use crate::FormatId;

/// A safe adapter failure that does not expose document contents.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterError {
    message: String,
}

impl AdapterError {
    /// Creates an adapter failure from a safe, bounded message.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        let mut message = message.into();
        message.truncate(512);
        Self { message }
    }

    /// Returns the safe public message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for AdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for AdapterError {}

/// Format registry, detection, import, or export failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IoError {
    /// A format already has a registered importer or exporter.
    DuplicateAdapter {
        /// Duplicated format.
        format: FormatId,
        /// Duplicated capability (`import` or `export`).
        capability: &'static str,
    },
    /// Two registrations disagree about one format's descriptor.
    DescriptorConflict {
        /// Conflicting format.
        format: FormatId,
    },
    /// No registered importer matched the bytes or requested format.
    UnsupportedFormat {
        /// Explicitly requested format, if any.
        requested: Option<FormatId>,
    },
    /// Multiple importers produced the same authoritative probe result.
    AmbiguousFormat {
        /// Candidate formats in stable sorted order.
        candidates: Vec<FormatId>,
    },
    /// A selected adapter failed to import the source.
    ImportFailed {
        /// Selected format.
        format: FormatId,
        /// Safe adapter error.
        source: AdapterError,
    },
    /// A selected adapter failed to export the document.
    ExportFailed {
        /// Selected format.
        format: FormatId,
        /// Safe adapter error.
        source: AdapterError,
    },
}

impl fmt::Display for IoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateAdapter { format, capability } => {
                write!(formatter, "duplicate {capability} adapter for {format}")
            }
            Self::DescriptorConflict { format } => {
                write!(formatter, "conflicting descriptors for {format}")
            }
            Self::UnsupportedFormat {
                requested: Some(format),
            } => {
                write!(formatter, "unsupported document format {format}")
            }
            Self::UnsupportedFormat { requested: None } => {
                formatter.write_str("document format could not be detected")
            }
            Self::AmbiguousFormat { candidates } => write!(
                formatter,
                "document format is ambiguous between {} adapters",
                candidates.len()
            ),
            Self::ImportFailed { format, source } => {
                write!(formatter, "{format} import failed: {source}")
            }
            Self::ExportFailed { format, source } => {
                write!(formatter, "{format} export failed: {source}")
            }
        }
    }
}

impl Error for IoError {}
