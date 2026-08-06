//! Stable format identity and capability descriptors.

use std::error::Error;
use std::fmt;
use std::str::FromStr;

/// Maximum byte length of a public format identifier.
const MAX_FORMAT_ID_BYTES: usize = 128;

/// Stable, extensible document-format identity.
///
/// IDs use lowercase ASCII letters, digits, dots, hyphens, and underscores.
/// They are open strings rather than a closed enum so trusted adapters can add
/// formats without changing the SDK's serialized vocabulary.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct FormatId(String);

impl FormatId {
    /// Creates and validates a format identifier.
    pub fn new(value: impl Into<String>) -> Result<Self, FormatIdError> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= MAX_FORMAT_ID_BYTES
            && value.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'-' | b'_')
            });
        if !valid {
            return Err(FormatIdError);
        }
        Ok(Self(value))
    }

    /// Returns the stable string representation.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FormatId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for FormatId {
    type Err = FormatIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

/// A malformed or over-limit format identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FormatIdError;

impl fmt::Display for FormatIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("format id is empty, over-limit, or contains unsupported characters")
    }
}

impl Error for FormatIdError {}

/// Built-in stable format identifiers.
pub mod formats {
    /// Office Open XML word-processing document (`.docx`).
    pub const DOCX: &str = "org.openxmlformats.wordprocessingml.document";
    /// OpenDocument Text document (`.odt`).
    pub const ODT: &str = "org.oasis.opendocument.text";
    /// OpenDoc normalized JSON snapshot.
    pub const NORMALIZED_JSON: &str = "org.casualoffice.normalized-json";
    /// Plain UTF-8 text.
    pub const TEXT: &str = "text.plain";
}

/// Public capabilities and aliases for one registered format.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormatDescriptor {
    /// Stable format identity.
    pub id: FormatId,
    /// Human-readable format name.
    pub display_name: String,
    /// Accepted MIME types, normalized to lowercase.
    pub mime_types: Vec<String>,
    /// Accepted filename extensions without a leading dot, in lowercase.
    pub extensions: Vec<String>,
    /// Whether an importer is expected for this descriptor.
    pub can_import: bool,
    /// Whether an exporter is expected for this descriptor.
    pub can_export: bool,
    /// Whether an unchanged source can be returned exactly when retained.
    pub exact_if_unchanged: bool,
    /// Whether same-format semantic export can preserve safe opaque records.
    pub preserve_when_safe: bool,
}

impl FormatDescriptor {
    /// Returns whether the optional filename or MIME hint names this format.
    #[must_use]
    pub(crate) fn matches_hint(&self, file_name: Option<&str>, mime: Option<&str>) -> bool {
        let mime_match = mime.is_some_and(|hint| {
            self.mime_types
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(hint.trim()))
        });
        let extension_match = file_name
            .and_then(|name| name.rsplit_once('.').map(|(_, extension)| extension))
            .is_some_and(|hint| {
                self.extensions
                    .iter()
                    .any(|candidate| candidate.eq_ignore_ascii_case(hint))
            });
        mime_match || extension_match
    }
}

#[cfg(test)]
mod tests {
    use super::{FormatId, FormatIdError};

    #[test]
    fn format_ids_are_strict_and_stable() {
        let id = FormatId::new("vendor.example-format_1").unwrap();
        assert_eq!(id.as_str(), "vendor.example-format_1");
        assert_eq!(FormatId::new("Vendor.Format"), Err(FormatIdError));
        assert_eq!(FormatId::new(""), Err(FormatIdError));
        assert_eq!(FormatId::new("a/b"), Err(FormatIdError));
    }
}
