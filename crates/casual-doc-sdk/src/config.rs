//! Engine, host configuration, and session-identity allocation.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use casual_doc_model as model;
use casual_doc_selection as selection;
use casual_doc_transaction as transaction;

use crate::error::{
    ErrorCode, ErrorSeverity, SdkError, map_initial_selection_error, map_snapshot_error,
};
use crate::event::EventJournal;
use crate::session::{DocumentSession, SessionState};

/// Configuration shared by sessions created from an engine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EngineConfig {
    /// Non-zero namespace used to construct deterministic model IDs.
    pub id_namespace: u64,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self { id_namespace: 1 }
    }
}

/// Runtime entry point used to create document sessions.
#[derive(Debug)]
pub struct Engine {
    config: EngineConfig,
    next_document: AtomicU64,
    next_session: AtomicU64,
    next_transaction: Arc<AtomicU64>,
}

impl Engine {
    /// Creates an engine after validating its host configuration.
    pub fn new(config: EngineConfig) -> Result<Self, SdkError> {
        if config.id_namespace == 0 {
            return Err(SdkError::new(
                ErrorCode::InvalidConfiguration,
                ErrorSeverity::Error,
                "id_namespace must be non-zero",
            ));
        }
        Ok(Self {
            config,
            next_document: AtomicU64::new(1),
            next_session: AtomicU64::new(1),
            next_transaction: Arc::new(AtomicU64::new(1)),
        })
    }

    /// Creates a blank document session at revision zero.
    pub fn create_blank(&self) -> Result<DocumentSession, SdkError> {
        let (session_id, namespace) = self.allocate_session_identity()?;

        let mut ids = model::IdGenerator::new(namespace);
        let document_id = ids
            .next_id()
            .map_err(|_| SdkError::internal("document ID allocation failed"))?;
        let paragraph_id = ids
            .next_id()
            .map_err(|_| SdkError::internal("paragraph ID allocation failed"))?;
        let document = model::Document::blank(document_id, paragraph_id)
            .map_err(|_| SdkError::internal("blank document construction failed"))?;
        self.session_from_document(session_id, namespace, document)
    }

    /// Opens strict, bounded normalized schema v0 JSON at revision zero.
    pub fn open_normalized_json(
        &self,
        bytes: &[u8],
        options: OpenNormalizedOptions,
    ) -> Result<DocumentSession, SdkError> {
        let document = model::Document::from_json(bytes, options.limits.to_internal())
            .map_err(map_snapshot_error)?;
        let (session_id, namespace) = self.allocate_session_identity()?;
        self.session_from_document(session_id, namespace, document)
    }

    fn session_from_document(
        &self,
        session_id: SessionId,
        namespace: u64,
        document: model::Document,
    ) -> Result<DocumentSession, SdkError> {
        let selection = selection::TextSelection::default_for(&document)
            .map_err(map_initial_selection_error)?;
        Ok(DocumentSession {
            id: session_id,
            state: Arc::new(RwLock::new(SessionState {
                document,
                revision: transaction::RevisionId::new(0),
                selection,
                undo_stack: Vec::new(),
                redo_stack: Vec::new(),
                events: EventJournal::default(),
            })),
            next_transaction: Arc::clone(&self.next_transaction),
            node_namespace: namespace,
            next_node: Arc::new(AtomicU64::new(1)),
        })
    }

    fn allocate_session_identity(&self) -> Result<(SessionId, u64), SdkError> {
        let document_sequence = next_counter(&self.next_document, "document")?;
        let session_id = SessionId(next_counter(&self.next_session, "session")?);
        let namespace = self
            .config
            .id_namespace
            .checked_add(document_sequence - 1)
            .ok_or_else(|| SdkError::internal("document namespace is exhausted"))?;
        Ok((session_id, namespace))
    }
}

/// Options controlling normalized schema v0 loading.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OpenNormalizedOptions {
    /// Resource limits for the input and parsed model.
    pub limits: NormalizedSnapshotLimits,
}

/// Host-configurable normalized snapshot resource limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NormalizedSnapshotLimits {
    /// Maximum input JSON bytes checked before parsing.
    pub max_input_bytes: usize,
    /// Maximum body blocks.
    pub max_blocks: usize,
    /// Maximum Unicode scalar values across text.
    pub max_unicode_scalar_values: usize,
    /// Maximum UTF-8 bytes in one text run.
    pub max_text_run_bytes: usize,
    /// Maximum extension map entries.
    pub max_extension_entries: usize,
    /// Maximum aggregate extension payload bytes.
    pub max_extension_bytes: usize,
}

impl NormalizedSnapshotLimits {
    fn to_internal(self) -> model::SnapshotLimits {
        model::SnapshotLimits {
            max_input_bytes: self.max_input_bytes,
            max_blocks: self.max_blocks,
            max_unicode_scalar_values: self.max_unicode_scalar_values,
            max_text_run_bytes: self.max_text_run_bytes,
            max_extension_entries: self.max_extension_entries,
            max_extension_bytes: self.max_extension_bytes,
        }
    }
}

impl Default for NormalizedSnapshotLimits {
    fn default() -> Self {
        let limits = model::SnapshotLimits::default();
        Self {
            max_input_bytes: limits.max_input_bytes,
            max_blocks: limits.max_blocks,
            max_unicode_scalar_values: limits.max_unicode_scalar_values,
            max_text_run_bytes: limits.max_text_run_bytes,
            max_extension_entries: limits.max_extension_entries,
            max_extension_bytes: limits.max_extension_bytes,
        }
    }
}

pub(crate) fn next_counter(counter: &AtomicU64, kind: &'static str) -> Result<u64, SdkError> {
    counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .map_err(|_| SdkError::internal(format!("{kind} counter is exhausted")))
}

/// Stable host-visible session identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SessionId(u64);

impl SessionId {
    /// Returns the numeric session identity.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}
