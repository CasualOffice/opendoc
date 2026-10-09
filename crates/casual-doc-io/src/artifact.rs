//! Format-neutral import/export values and preservation envelope.

use std::any::Any;
use std::collections::BTreeMap;
use std::fmt;

use casual_doc_model::v1::Document;

use crate::{CompatibilityReport, FormatId, PreservationLedger, RecoveryReport};

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
    /// The preservation ledger licensing every `preserved` retention outcome in
    /// [`ImportArtifact::report`] (`35-DISPOSITION-TAXONOMY.md`).
    ///
    /// This is what makes a preservation claim auditable rather than trusted: a
    /// host can resolve an entry's `ledger_id` to the record that says where the
    /// retained bytes live and how many there are. The adapter has already run
    /// `CompatibilityReport::validate` against it, so an import whose claims did
    /// not resolve failed instead of reporting — but the host can re-check, which
    /// it could not when the ledger stopped at the DOCX importer's own boundary.
    ///
    /// Empty for an adapter that retains nothing: an empty ledger beside a report
    /// with no `preserved` entry is the consistent, honest state.
    pub ledger: PreservationLedger,
    /// What a best-effort open had to repair in damaged source.
    ///
    /// Empty for every well-formed source. A host shows its notice exactly when
    /// this is non-empty — see [`crate::RecoveryReport`] for why that is a
    /// separate report from [`ImportArtifact::report`].
    pub recovery: RecoveryReport,
    /// Detected source format/profile.
    pub format: FormatProfile,
}

/// Import request passed to a selected adapter after detection.
#[derive(Clone, Copy, Debug)]
pub struct ImportRequest<'a> {
    /// Untrusted source bytes.
    pub bytes: &'a [u8],
    /// Whether to retain the original bytes for exact unchanged export.
    pub retain_source: bool,
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
#[derive(Debug)]
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

/// Complete result of a successful export.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportArtifact {
    /// Encoded target-format bytes.
    pub bytes: Vec<u8>,
    /// Export compatibility findings.
    pub report: CompatibilityReport,
    /// The preservation ledger licensing every `preserved` retention outcome in
    /// [`ExportArtifact::report`].
    ///
    /// An export can make a preservation claim too — "these source parts were
    /// carried verbatim into the written package" — and `35`'s rule does not
    /// weaken on the way out: a claim with no record behind it is `not-retained`,
    /// not `preserved`. Keeping the ledger on both artifacts means the invariant
    /// has one spelling rather than one per direction.
    pub ledger: PreservationLedger,
    /// Emitted format/profile.
    pub format: FormatProfile,
    /// Emitted MIME type.
    pub mime_type: String,
    /// Suggested filename extension without a leading dot.
    pub suggested_extension: String,
}
