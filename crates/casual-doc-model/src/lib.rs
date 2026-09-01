//! Normalized document values and invariants used inside the OpenDoc runtime.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod body;
mod document;
mod error;
mod extension;
mod ids;
mod snapshot;

pub mod v1;

pub use body::{BlockNode, InlineNode, Mark, Paragraph, TextRun};
pub use document::Document;
pub use error::ModelError;
pub use extension::ExtensionValue;
pub use ids::{IdGenerator, NodeId};
pub use snapshot::{SnapshotError, SnapshotLimits};

pub(crate) use snapshot::enforce_limit;

/// Removes the characters XML 1.0 cannot represent, returning the input
/// untouched when there are none.
///
/// Both output formats are XML, and XML 1.0 has no representation at all for
/// most C0 control characters — not an escape, not a numeric reference, nothing.
/// Nor does an XML writer defend against them: `quick-xml` escapes `< > & ' "`
/// and passes everything else through byte for byte. So a NUL, vertical tab or
/// form feed — routine in text pasted from a PDF, a terminal, or an odd
/// clipboard source — was written raw into `document.xml`, and the resulting
/// package was not well-formed. Word reported the contents as unreadable and
/// this runtime's own importer failed with `MalformedXml`: one paste, and the
/// document could no longer be opened by anything.
///
/// Dropping them is the faithful choice rather than a lossy one. The formats
/// offer nowhere to preserve them into, so the alternative to dropping a
/// character is losing the file. Tab, line feed and carriage return are legal
/// XML and are kept; line breaks are handled structurally elsewhere. Surrogates
/// cannot occur in a Rust `str`.
#[must_use]
pub fn strip_xml_forbidden(text: &str) -> std::borrow::Cow<'_, str> {
    #[inline]
    const fn permitted(c: char) -> bool {
        matches!(c, '\t' | '\n' | '\r') || (c >= ' ' && c != '\u{fffe}' && c != '\u{ffff}')
    }
    if text.chars().all(permitted) {
        return std::borrow::Cow::Borrowed(text);
    }
    std::borrow::Cow::Owned(text.chars().filter(|c| permitted(*c)).collect())
}

/// The normalized document schema implemented at the crate root (v0).
pub const SCHEMA_VERSION: u32 = 0;

#[cfg(test)]
mod tests;
