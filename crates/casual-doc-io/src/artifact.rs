//! Format-neutral import/export values and preservation envelope.

use std::any::Any;
use std::collections::BTreeMap;
use std::fmt;

use casual_doc_model::v1::Document;

use crate::{CompatibilityReport, FormatId};

/// A concrete format plus its source or emitted profile version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormatProfile {
    /// Stable format identity.
    pub format: FormatId,
    /// Adapter-defined profile/version, when known.
    pub version: Option<String>,
}

/// Binary document resources indexed by their normalized adapter identity.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DocumentResources {
    bytes: BTreeMap<String, Vec<u8>>,
}

impl DocumentResources {
    /// Inserts or replaces one resource.
    pub fn insert(&mut self, id: String, bytes: Vec<u8>) -> Option<Vec<u8>> {
        self.bytes.insert(id, bytes)
    }

    /// Returns one resource's bytes.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&[u8]> {
        self.bytes.get(id).map(Vec::as_slice)
    }

    /// Returns all resources in deterministic identity order.
    #[must_use]
    pub fn as_map(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.bytes
    }

    /// Returns whether the resource collection is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

trait EnvelopeState: Any + Send + Sync + fmt::Debug {
    fn as_any(&self) -> &dyn Any;
}

impl<T> EnvelopeState for T
where
    T: Any + Send + Sync + fmt::Debug,
{
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Format-tagged, bounded source-preservation sidecar owned by a session.
///
/// Its adapter-private payload never enters the normalized document model and
/// can only be interpreted by code that also checks the format identifier.
pub struct SourceEnvelope {
    format: FormatId,
    adapter_version: String,
    state: Box<dyn EnvelopeState>,
}

impl SourceEnvelope {
    pub(crate) fn new<T>(format: FormatId, adapter_version: String, state: T) -> Self
    where
        T: Any + Send + Sync + fmt::Debug,
    {
        Self {
            format,
            adapter_version,
            state: Box::new(state),
        }
    }

    /// Returns the source format.
    #[must_use]
    pub fn format(&self) -> &FormatId {
        &self.format
    }

    /// Returns the adapter version that created this envelope.
    #[must_use]
    pub fn adapter_version(&self) -> &str {
        &self.adapter_version
    }

    pub(crate) fn state<T: Any>(&self) -> Option<&T> {
        self.state.as_ref().as_any().downcast_ref()
    }
}

impl fmt::Debug for SourceEnvelope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceEnvelope")
            .field("format", &self.format)
            .field("adapter_version", &self.adapter_version)
            .finish_non_exhaustive()
    }
}

/// Complete atomic result of a successful format import.
#[derive(Debug)]
pub struct ImportArtifact {
    /// Normalized editable document.
    pub document: Document,
    /// Binary resources needed by layout, rendering, and export.
    pub resources: DocumentResources,
    /// Format-specific validated preservation state.
    pub source: SourceEnvelope,
    /// Import compatibility findings.
    pub report: CompatibilityReport,
    /// Detected source format/profile.
    pub format: FormatProfile,
}

/// Import request passed to a selected adapter after detection.
///
/// `#[non_exhaustive]` plus a builder, deliberately, and *before* the field this
/// shape exists to carry. A widely-constructed struct that can be written as a
/// literal breaks every literal on every other open branch the moment a field is
/// added, and the merge then compiles on neither branch alone — `SKILL` §5a-5,
/// which this repository has already paid for twice (#645/#646, #738/#739).
/// Construction goes through [`ImportRequest::new`], so adding a field later is
/// one line here and nothing anywhere else.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct ImportRequest<'a> {
    /// Untrusted source bytes.
    pub bytes: &'a [u8],
    /// Whether to retain the original bytes for exact unchanged export.
    pub retain_source: bool,
}

impl<'a> ImportRequest<'a> {
    /// Creates a request over `bytes` that does **not** retain the source.
    ///
    /// Not retaining is the default because retention costs a second copy of the
    /// whole input; a caller that wants exact-unchanged export asks for it with
    /// [`ImportRequest::retain_source`].
    ///
    /// Complexity: O(1). Borrows the bytes, never copies them.
    #[must_use]
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            retain_source: false,
        }
    }

    /// Sets whether the original bytes are retained for exact unchanged export.
    ///
    /// Complexity: O(1).
    #[must_use]
    pub const fn retain_source(mut self, retain: bool) -> Self {
        self.retain_source = retain;
        self
    }
}

/// Requested export behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExportMode {
    /// Write normalized semantics without source-native opaque data.
    Semantic,
    /// Preserve safe opaque data when the source and target adapters match.
    PreserveWhenSafe,
    /// Return the original bytes only if the source is retained and unchanged.
    ExactIfUnchanged,
}

/// Export request passed to the explicitly selected target adapter.
///
/// `#[non_exhaustive]` plus a builder for the same reason as [`ImportRequest`],
/// and here the risk was never hypothetical: `casual-doc-wasm` builds this type
/// from *outside* this crate, so a field added here was already a compile error
/// in another crate — exactly the cross-branch shape `SKILL` §5a-5 describes.
#[derive(Debug)]
#[non_exhaustive]
pub struct ExportRequest<'a> {
    /// Immutable normalized document snapshot.
    pub document: &'a Document,
    /// Binary resources referenced by the document.
    pub resources: &'a DocumentResources,
    /// Optional source-format preservation state.
    pub source: Option<&'a SourceEnvelope>,
    /// Whether the document is unchanged since import.
    pub source_unchanged: bool,
    /// Requested export behavior.
    pub mode: ExportMode,
}

impl<'a> ExportRequest<'a> {
    /// Creates a semantic export request carrying no preservation state.
    ///
    /// The defaults are the conservative ones: no source envelope, the document
    /// treated as **changed**, and [`ExportMode::Semantic`]. Preservation and
    /// exact-if-unchanged both rest on facts only the caller holds, and a default
    /// that claimed either would emit source-native opaque data, or the original
    /// bytes, that nobody vouched for.
    ///
    /// Complexity: O(1). Borrows the document and resources, never clones them.
    #[must_use]
    pub const fn new(document: &'a Document, resources: &'a DocumentResources) -> Self {
        Self {
            document,
            resources,
            source: None,
            source_unchanged: false,
            mode: ExportMode::Semantic,
        }
    }

    /// Attaches the source-format preservation state, when there is one.
    ///
    /// Complexity: O(1).
    #[must_use]
    pub const fn source(mut self, source: Option<&'a SourceEnvelope>) -> Self {
        self.source = source;
        self
    }

    /// Declares whether the document is unchanged since import.
    ///
    /// Complexity: O(1).
    #[must_use]
    pub const fn source_unchanged(mut self, unchanged: bool) -> Self {
        self.source_unchanged = unchanged;
        self
    }

    /// Sets the requested export behavior.
    ///
    /// Complexity: O(1).
    #[must_use]
    pub const fn mode(mut self, mode: ExportMode) -> Self {
        self.mode = mode;
        self
    }
}

/// Complete result of a successful export.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportArtifact {
    /// Encoded target-format bytes.
    pub bytes: Vec<u8>,
    /// Export compatibility findings.
    pub report: CompatibilityReport,
    /// Emitted format/profile.
    pub format: FormatProfile,
    /// Emitted MIME type.
    pub mime_type: String,
    /// Suggested filename extension without a leading dot.
    pub suggested_extension: String,
}
