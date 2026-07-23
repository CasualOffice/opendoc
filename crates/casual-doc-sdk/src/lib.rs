//! Host-facing OpenDoc engine and document-session facade.
//!
//! The Phase 0 API creates a blank document and applies grapheme-aware text
//! insertions through atomic transactions:
//!
//! ```
//! use std::collections::BTreeSet;
//!
//! use casual_doc_sdk::{
//!     Affinity, BlockSnapshot, Engine, EngineConfig, InsertTextRequest, Position,
//! };
//!
//! let engine = Engine::new(EngineConfig::default())?;
//! let session = engine.create_blank()?;
//! let snapshot = session.snapshot()?;
//! let paragraph = match &snapshot.body[0] {
//!     BlockSnapshot::Paragraph(paragraph) => paragraph.id.clone(),
//! };
//!
//! session.insert_text(InsertTextRequest {
//!     base_revision: snapshot.revision,
//!     at: Position {
//!         node: paragraph,
//!         grapheme_offset: 0,
//!         affinity: Affinity::After,
//!     },
//!     text: "Hello".to_owned(),
//!     marks: BTreeSet::new(),
//! })?;
//!
//! # Ok::<(), casual_doc_sdk::SdkError>(())
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use casual_doc_model as model;
use casual_doc_transaction as transaction;
use serde::{Deserialize, Serialize};

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
        let document_sequence = next_counter(&self.next_document, "document")?;
        let session_id = SessionId(next_counter(&self.next_session, "session")?);
        let namespace = self
            .config
            .id_namespace
            .checked_add(document_sequence - 1)
            .ok_or_else(|| SdkError::internal("document namespace is exhausted"))?;

        let mut ids = model::IdGenerator::new(namespace);
        let document_id = ids
            .next_id()
            .map_err(|_| SdkError::internal("document ID allocation failed"))?;
        let paragraph_id = ids
            .next_id()
            .map_err(|_| SdkError::internal("paragraph ID allocation failed"))?;
        let document = model::Document::blank(document_id, paragraph_id)
            .map_err(|_| SdkError::internal("blank document construction failed"))?;

        Ok(DocumentSession {
            id: session_id,
            state: Arc::new(RwLock::new(SessionState {
                document,
                revision: transaction::RevisionId::new(0),
                undo_stack: Vec::new(),
                redo_stack: Vec::new(),
            })),
            next_transaction: Arc::clone(&self.next_transaction),
            node_namespace: namespace,
            next_node: Arc::new(AtomicU64::new(3)),
        })
    }
}

fn next_counter(counter: &AtomicU64, kind: &'static str) -> Result<u64, SdkError> {
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

/// Stable host-visible node identity.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct NodeId(String);

impl NodeId {
    fn from_internal(id: model::NodeId) -> Self {
        Self(id.to_string())
    }

    fn to_internal(&self) -> Result<model::NodeId, SdkError> {
        self.0.parse().map_err(|_| {
            SdkError::new(
                ErrorCode::InvalidArgument,
                ErrorSeverity::Error,
                "node ID is invalid",
            )
        })
    }

    /// Returns the fixed-width lowercase hexadecimal representation.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Session-local document revision.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Revision(u64);

impl Revision {
    /// Creates a revision value.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the numeric revision.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Boundary behavior when content is inserted at a position.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Affinity {
    /// Keep the position before inserted content.
    Before,
    /// Move the position after inserted content.
    After,
}

impl Affinity {
    fn to_internal(self) -> transaction::Affinity {
        match self {
            Self::Before => transaction::Affinity::Before,
            Self::After => transaction::Affinity::After,
        }
    }
}

/// Public text position at an extended-grapheme boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    /// Paragraph node ID.
    pub node: NodeId,
    /// Zero-based extended grapheme boundary.
    pub grapheme_offset: u32,
    /// Mapping behavior at an equal insertion boundary.
    pub affinity: Affinity,
}

impl Position {
    fn to_internal(&self) -> Result<transaction::Position, SdkError> {
        Ok(transaction::Position {
            node: self.node.to_internal()?,
            grapheme_offset: self.grapheme_offset,
            affinity: self.affinity.to_internal(),
        })
    }
}

/// A public ordered text range.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Range {
    /// Inclusive start boundary.
    pub start: Position,
    /// Exclusive end boundary.
    pub end: Position,
}

impl Range {
    fn to_internal(&self) -> Result<transaction::Range, SdkError> {
        Ok(transaction::Range {
            start: self.start.to_internal()?,
            end: self.end.to_internal()?,
        })
    }
}

/// Inline marks accepted by the initial insertion command.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mark {
    /// Bold text.
    Bold,
    /// Italic text.
    Italic,
    /// Underlined text.
    Underline,
    /// Struck-through text.
    Strike,
}

impl Mark {
    fn to_internal(self) -> model::Mark {
        match self {
            Self::Bold => model::Mark::Bold,
            Self::Italic => model::Mark::Italic,
            Self::Underline => model::Mark::Underline,
            Self::Strike => model::Mark::Strike,
        }
    }

    fn from_internal(mark: model::Mark) -> Self {
        match mark {
            model::Mark::Bold => Self::Bold,
            model::Mark::Italic => Self::Italic,
            model::Mark::Underline => Self::Underline,
            model::Mark::Strike => Self::Strike,
        }
    }
}

/// Request for the first transaction-backed SDK command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsertTextRequest {
    /// Revision against which the host constructed the request.
    pub base_revision: Revision,
    /// Insertion boundary.
    pub at: Position,
    /// Inserted text.
    pub text: String,
    /// Marks for the inserted run.
    pub marks: BTreeSet<Mark>,
}

/// Request to delete a non-empty range inside one paragraph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteRangeRequest {
    /// Revision against which the host constructed the request.
    pub base_revision: Revision,
    /// Range to delete.
    pub range: Range,
}

/// Request to split one paragraph at a grapheme boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SplitParagraphRequest {
    /// Revision against which the host constructed the request.
    pub base_revision: Revision,
    /// Split boundary in the original paragraph.
    pub at: Position,
}

/// Request to join two adjacent paragraphs in document order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JoinParagraphRequest {
    /// Revision against which the host constructed the request.
    pub base_revision: Revision,
    /// Paragraph retaining its identity.
    pub first: NodeId,
    /// Adjacent paragraph removed by the join.
    pub second: NodeId,
}

/// Immutable document snapshot returned to hosts.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSnapshot {
    /// Normalized schema version.
    pub schema_version: u32,
    /// Logical document ID.
    pub document_id: NodeId,
    /// Session revision represented by the snapshot.
    pub revision: Revision,
    /// Ordered body blocks.
    pub body: Vec<BlockSnapshot>,
}

/// Body block in a public snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlockSnapshot {
    /// Paragraph block.
    Paragraph(ParagraphSnapshot),
}

/// Paragraph value in a public snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParagraphSnapshot {
    /// Stable paragraph ID.
    pub id: NodeId,
    /// Ordered inline values.
    pub inlines: Vec<InlineSnapshot>,
}

/// Inline value in a public snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InlineSnapshot {
    /// Text with one mark set.
    Text {
        /// Text content.
        text: String,
        /// Deterministically ordered marks.
        marks: BTreeSet<Mark>,
    },
}

/// One public deterministic position-mapping step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MappingStep {
    /// Text insertion.
    Insert {
        /// Paragraph containing the insertion.
        node: NodeId,
        /// Boundary before insertion.
        at: u32,
        /// Number of inserted graphemes.
        graphemes: u32,
    },
    /// Text deletion.
    Delete {
        /// Paragraph containing the deletion.
        node: NodeId,
        /// Inclusive deletion start.
        start: u32,
        /// Exclusive deletion end.
        end: u32,
    },
    /// Paragraph split.
    Split {
        /// Original paragraph.
        original: NodeId,
        /// New trailing paragraph.
        new_node: NodeId,
        /// Split boundary in the original.
        at: u32,
    },
    /// Adjacent paragraph join.
    Join {
        /// Paragraph retaining identity.
        first: NodeId,
        /// Removed paragraph.
        second: NodeId,
        /// Former end of the first paragraph.
        at: u32,
    },
}

/// Ordered position map returned by a committed transaction.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PositionMap {
    steps: Vec<MappingStep>,
}

impl PositionMap {
    /// Returns mapping steps in transaction order.
    #[must_use]
    pub fn steps(&self) -> &[MappingStep] {
        &self.steps
    }

    /// Maps a host position through every transaction step.
    #[must_use]
    pub fn map(&self, mut position: Position) -> Position {
        for step in &self.steps {
            match step {
                MappingStep::Insert {
                    node,
                    at,
                    graphemes,
                } if position.node == *node => {
                    if position.grapheme_offset > *at
                        || (position.grapheme_offset == *at && position.affinity == Affinity::After)
                    {
                        position.grapheme_offset =
                            position.grapheme_offset.saturating_add(*graphemes);
                    }
                }
                MappingStep::Delete { node, start, end } if position.node == *node => {
                    if position.grapheme_offset > *start {
                        position.grapheme_offset = if position.grapheme_offset < *end {
                            *start
                        } else {
                            position.grapheme_offset - (*end - *start)
                        };
                    }
                }
                MappingStep::Split {
                    original,
                    new_node,
                    at,
                } if position.node == *original => {
                    if position.grapheme_offset > *at
                        || (position.grapheme_offset == *at && position.affinity == Affinity::After)
                    {
                        position.node = new_node.clone();
                        position.grapheme_offset -= *at;
                    }
                }
                MappingStep::Join { first, second, at } if position.node == *second => {
                    position.node = first.clone();
                    position.grapheme_offset = position.grapheme_offset.saturating_add(*at);
                }
                _ => {}
            }
        }
        position
    }
}

/// Result of a successful editing command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransactionResult {
    /// Committed revision.
    pub revision: Revision,
    /// Position mapping from the prior revision.
    pub position_map: PositionMap,
    /// Number of operations committed.
    pub operations_applied: usize,
}

#[derive(Debug)]
struct SessionState {
    document: model::Document,
    revision: transaction::RevisionId,
    undo_stack: Vec<Vec<transaction::Operation>>,
    redo_stack: Vec<Vec<transaction::Operation>>,
}

/// Thread-safe live document session.
#[derive(Clone, Debug)]
pub struct DocumentSession {
    id: SessionId,
    state: Arc<RwLock<SessionState>>,
    next_transaction: Arc<AtomicU64>,
    node_namespace: u64,
    next_node: Arc<AtomicU64>,
}

impl DocumentSession {
    /// Returns this session's identity.
    #[must_use]
    pub const fn id(&self) -> SessionId {
        self.id
    }

    /// Returns the current immutable document snapshot.
    pub fn snapshot(&self) -> Result<DocumentSnapshot, SdkError> {
        let state = self
            .state
            .read()
            .map_err(|_| SdkError::internal("session state lock is poisoned"))?;
        Ok(snapshot_from_internal(&state.document, state.revision))
    }

    /// Inserts text through one atomic transaction.
    pub fn insert_text(&self, request: InsertTextRequest) -> Result<TransactionResult, SdkError> {
        let position = request.at.to_internal()?;
        let marks = request.marks.into_iter().map(Mark::to_internal).collect();
        self.apply_forward(
            request.base_revision,
            vec![transaction::Operation::InsertText {
                at: position,
                text: request.text,
                marks,
            }],
        )
    }

    /// Deletes a non-empty range inside one paragraph.
    pub fn delete_range(&self, request: DeleteRangeRequest) -> Result<TransactionResult, SdkError> {
        self.apply_forward(
            request.base_revision,
            vec![transaction::Operation::DeleteRange {
                range: request.range.to_internal()?,
            }],
        )
    }

    /// Splits a paragraph and allocates a stable ID for the trailing paragraph.
    pub fn split_paragraph(
        &self,
        request: SplitParagraphRequest,
    ) -> Result<TransactionResult, SdkError> {
        let node_counter = next_counter(&self.next_node, "node")?;
        let new_id = model::NodeId::from_parts(self.node_namespace, node_counter)
            .map_err(|_| SdkError::internal("node ID allocation failed"))?;
        self.apply_forward(
            request.base_revision,
            vec![transaction::Operation::SplitParagraph {
                at: request.at.to_internal()?,
                new_id,
            }],
        )
    }

    /// Joins two adjacent paragraphs in document order.
    pub fn join_paragraphs(
        &self,
        request: JoinParagraphRequest,
    ) -> Result<TransactionResult, SdkError> {
        self.apply_forward(
            request.base_revision,
            vec![transaction::Operation::JoinParagraph {
                first: request.first.to_internal()?,
                second: request.second.to_internal()?,
            }],
        )
    }

    /// Undoes the latest local history entry as a new transaction.
    pub fn undo(&self, base_revision: Revision) -> Result<TransactionResult, SdkError> {
        self.apply_history(base_revision, HistoryDirection::Undo)
    }

    /// Redoes the latest locally undone history entry as a new transaction.
    pub fn redo(&self, base_revision: Revision) -> Result<TransactionResult, SdkError> {
        self.apply_history(base_revision, HistoryDirection::Redo)
    }

    fn apply_forward(
        &self,
        base_revision: Revision,
        operations: Vec<transaction::Operation>,
    ) -> Result<TransactionResult, SdkError> {
        let mut state = self.lock_state()?;
        let edit = self.transaction(base_revision, operations)?;
        let commit = transaction::apply(&state.document, state.revision, &edit)
            .map_err(map_transaction_error)?;
        let result = transaction_result(&commit);
        state.document = commit.document;
        state.revision = commit.revision;
        state.undo_stack.push(commit.inverse_operations);
        state.redo_stack.clear();
        Ok(result)
    }

    fn apply_history(
        &self,
        base_revision: Revision,
        direction: HistoryDirection,
    ) -> Result<TransactionResult, SdkError> {
        let mut state = self.lock_state()?;
        if base_revision.get() != state.revision.get() {
            return Err(stale_revision_error(base_revision, state.revision));
        }
        let operations = match direction {
            HistoryDirection::Undo => state.undo_stack.last(),
            HistoryDirection::Redo => state.redo_stack.last(),
        }
        .cloned()
        .ok_or_else(|| {
            SdkError::new(
                ErrorCode::HistoryEmpty,
                ErrorSeverity::Error,
                "requested history stack is empty",
            )
        })?;
        let edit = self.transaction(base_revision, operations)?;
        let commit = transaction::apply(&state.document, state.revision, &edit)
            .map_err(map_transaction_error)?;
        let result = transaction_result(&commit);

        state.document = commit.document;
        state.revision = commit.revision;
        match direction {
            HistoryDirection::Undo => {
                state.undo_stack.pop();
                state.redo_stack.push(commit.inverse_operations);
            }
            HistoryDirection::Redo => {
                state.redo_stack.pop();
                state.undo_stack.push(commit.inverse_operations);
            }
        }
        Ok(result)
    }

    fn lock_state(&self) -> Result<std::sync::RwLockWriteGuard<'_, SessionState>, SdkError> {
        self.state
            .write()
            .map_err(|_| SdkError::internal("session state lock is poisoned"))
    }

    fn transaction(
        &self,
        base_revision: Revision,
        operations: Vec<transaction::Operation>,
    ) -> Result<transaction::Transaction, SdkError> {
        let counter = next_counter(&self.next_transaction, "transaction")?;
        let id = transaction::TransactionId::new(
            (u128::from(self.id.get()) << 64) | u128::from(counter),
        );
        Ok(transaction::Transaction::new(
            id,
            transaction::RevisionId::new(base_revision.get()),
            operations,
        ))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HistoryDirection {
    Undo,
    Redo,
}

fn transaction_result(commit: &transaction::Commit) -> TransactionResult {
    TransactionResult {
        revision: Revision(commit.revision.get()),
        position_map: PositionMap {
            steps: commit
                .position_map
                .steps()
                .iter()
                .map(|step| match *step {
                    transaction::MappingStep::Insert {
                        node,
                        at,
                        graphemes,
                    } => MappingStep::Insert {
                        node: NodeId::from_internal(node),
                        at,
                        graphemes,
                    },
                    transaction::MappingStep::Delete { node, start, end } => MappingStep::Delete {
                        node: NodeId::from_internal(node),
                        start,
                        end,
                    },
                    transaction::MappingStep::Split {
                        original,
                        new_node,
                        at,
                    } => MappingStep::Split {
                        original: NodeId::from_internal(original),
                        new_node: NodeId::from_internal(new_node),
                        at,
                    },
                    transaction::MappingStep::Join { first, second, at } => MappingStep::Join {
                        first: NodeId::from_internal(first),
                        second: NodeId::from_internal(second),
                        at,
                    },
                })
                .collect(),
        },
        operations_applied: commit.operations_applied,
    }
}

fn snapshot_from_internal(
    document: &model::Document,
    revision: transaction::RevisionId,
) -> DocumentSnapshot {
    DocumentSnapshot {
        schema_version: document.schema_version(),
        document_id: NodeId::from_internal(document.id()),
        revision: Revision(revision.get()),
        body: document
            .body()
            .iter()
            .map(|block| match block {
                model::BlockNode::Paragraph(paragraph) => {
                    BlockSnapshot::Paragraph(ParagraphSnapshot {
                        id: NodeId::from_internal(paragraph.id()),
                        inlines: paragraph
                            .inlines()
                            .iter()
                            .map(|inline| match inline {
                                model::InlineNode::Text(run) => InlineSnapshot::Text {
                                    text: run.text().to_owned(),
                                    marks: run
                                        .marks()
                                        .iter()
                                        .copied()
                                        .map(Mark::from_internal)
                                        .collect(),
                                },
                            })
                            .collect(),
                    })
                }
            })
            .collect(),
    }
}

fn map_transaction_error(error: transaction::TransactionError) -> SdkError {
    match error {
        transaction::TransactionError::StaleRevision { expected, actual } => SdkError::new(
            ErrorCode::StaleRevision,
            ErrorSeverity::Error,
            "transaction base revision does not match the session",
        )
        .with_context("expected_revision", expected.get().to_string())
        .with_context("actual_revision", actual.get().to_string()),
        transaction::TransactionError::EmptyTransaction => SdkError::new(
            ErrorCode::EmptyTransaction,
            ErrorSeverity::Error,
            "transaction has no effective operation",
        ),
        transaction::TransactionError::InvalidPosition { node, offset } => SdkError::new(
            ErrorCode::InvalidPosition,
            ErrorSeverity::Error,
            "position does not resolve to a valid grapheme boundary",
        )
        .with_context("node_id", node.to_string())
        .with_context("grapheme_offset", offset.to_string()),
        transaction::TransactionError::InvalidRange {
            start_node,
            start,
            end_node,
            end,
        } => SdkError::new(
            ErrorCode::InvalidPosition,
            ErrorSeverity::Error,
            "range is not valid for this operation",
        )
        .with_context("start_node_id", start_node.to_string())
        .with_context("start_offset", start.to_string())
        .with_context("end_node_id", end_node.to_string())
        .with_context("end_offset", end.to_string()),
        transaction::TransactionError::InvalidStructure => SdkError::new(
            ErrorCode::InvalidPosition,
            ErrorSeverity::Error,
            "paragraph structure does not satisfy the operation",
        ),
        transaction::TransactionError::InvalidTextInput => SdkError::new(
            ErrorCode::InvalidTextInput,
            ErrorSeverity::Error,
            "text contains a control that requires a structural command",
        ),
        transaction::TransactionError::TextTooLong => SdkError::new(
            ErrorCode::InvalidArgument,
            ErrorSeverity::Error,
            "inserted text exceeds the supported length",
        ),
        transaction::TransactionError::RevisionExhausted => {
            SdkError::internal("session revision is exhausted")
        }
        transaction::TransactionError::Model(_) => SdkError::new(
            ErrorCode::InvariantViolation,
            ErrorSeverity::Fatal,
            "transaction application failed a document invariant",
        ),
    }
}

fn stale_revision_error(expected: Revision, actual: transaction::RevisionId) -> SdkError {
    SdkError::new(
        ErrorCode::StaleRevision,
        ErrorSeverity::Error,
        "transaction base revision does not match the session",
    )
    .with_context("expected_revision", expected.get().to_string())
    .with_context("actual_revision", actual.get().to_string())
}

/// Stable public error code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorCode {
    /// `ODC-0001`.
    InvalidArgument,
    /// `ODC-0002`.
    InvalidConfiguration,
    /// `ODC-2001`.
    StaleRevision,
    /// `ODC-2002`.
    InvalidPosition,
    /// `ODC-2003`.
    EmptyTransaction,
    /// `ODC-2004`.
    InvalidTextInput,
    /// `ODC-2005`.
    InvariantViolation,
    /// `ODC-2006`.
    HistoryEmpty,
    /// `ODC-9001`.
    Internal,
}

impl ErrorCode {
    /// Returns the non-recycled registry code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidArgument => "ODC-0001",
            Self::InvalidConfiguration => "ODC-0002",
            Self::StaleRevision => "ODC-2001",
            Self::InvalidPosition => "ODC-2002",
            Self::EmptyTransaction => "ODC-2003",
            Self::InvalidTextInput => "ODC-2004",
            Self::InvariantViolation => "ODC-2005",
            Self::HistoryEmpty => "ODC-2006",
            Self::Internal => "ODC-9001",
        }
    }
}

/// Public failure severity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorSeverity {
    /// Recoverable limitation.
    Warning,
    /// Requested operation failed; session remains valid.
    Error,
    /// Session cannot safely continue.
    Fatal,
}

/// Error crossing the Rust SDK facade.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SdkError {
    code: ErrorCode,
    severity: ErrorSeverity,
    message: String,
    context: BTreeMap<String, String>,
}

impl SdkError {
    fn new(code: ErrorCode, severity: ErrorSeverity, message: impl Into<String>) -> Self {
        Self {
            code,
            severity,
            message: message.into(),
            context: BTreeMap::new(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, ErrorSeverity::Fatal, message)
    }

    fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }

    /// Returns the stable error code.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        self.code
    }

    /// Returns the public severity.
    #[must_use]
    pub const fn severity(&self) -> ErrorSeverity {
        self.severity
    }

    /// Returns a safe host-facing message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns redacted structured context.
    #[must_use]
    pub const fn context(&self) -> &BTreeMap<String, String> {
        &self.context
    }
}

impl fmt::Display for SdkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code.as_str(), self.message)
    }
}

impl Error for SdkError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn initial_paragraph(snapshot: &DocumentSnapshot) -> NodeId {
        match &snapshot.body[0] {
            BlockSnapshot::Paragraph(paragraph) => paragraph.id.clone(),
        }
    }

    fn paragraph(snapshot: &DocumentSnapshot, index: usize) -> &ParagraphSnapshot {
        match &snapshot.body[index] {
            BlockSnapshot::Paragraph(paragraph) => paragraph,
        }
    }

    fn paragraph_text(snapshot: &DocumentSnapshot, index: usize) -> String {
        paragraph(snapshot, index)
            .inlines
            .iter()
            .map(|inline| match inline {
                InlineSnapshot::Text { text, .. } => text.as_str(),
            })
            .collect()
    }

    #[test]
    fn blank_insert_snapshot_is_end_to_end() {
        let engine = Engine::new(EngineConfig { id_namespace: 9 }).unwrap();
        let session = engine.create_blank().unwrap();
        let before = session.snapshot().unwrap();
        let paragraph = initial_paragraph(&before);

        let result = session
            .insert_text(InsertTextRequest {
                base_revision: before.revision,
                at: Position {
                    node: paragraph,
                    grapheme_offset: 0,
                    affinity: Affinity::After,
                },
                text: "OpenDoc 👩🏽‍💻".to_owned(),
                marks: BTreeSet::new(),
            })
            .unwrap();

        assert_eq!(result.revision, Revision::new(1));
        assert!(matches!(
            result.position_map.steps()[0],
            MappingStep::Insert { graphemes: 9, .. }
        ));
        assert_eq!(
            serde_json::to_value(session.snapshot().unwrap()).unwrap(),
            serde_json::json!({
                "schemaVersion": 0,
                "documentId": "00000000000000090000000000000001",
                "revision": 1,
                "body": [{
                    "type": "paragraph",
                    "id": "00000000000000090000000000000002",
                    "inlines": [{
                        "type": "text",
                        "text": "OpenDoc 👩🏽‍💻",
                        "marks": []
                    }]
                }]
            })
        );
    }

    #[test]
    fn stale_revision_has_stable_code_and_preserves_state() {
        let engine = Engine::new(EngineConfig::default()).unwrap();
        let session = engine.create_blank().unwrap();
        let before = session.snapshot().unwrap();
        let paragraph = initial_paragraph(&before);
        let request = || InsertTextRequest {
            base_revision: Revision::new(0),
            at: Position {
                node: paragraph.clone(),
                grapheme_offset: 0,
                affinity: Affinity::After,
            },
            text: "A".to_owned(),
            marks: BTreeSet::new(),
        };

        session.insert_text(request()).unwrap();
        let error = session.insert_text(request()).unwrap_err();

        assert_eq!(error.code().as_str(), "ODC-2001");
        assert_eq!(session.snapshot().unwrap().revision, Revision::new(1));
    }

    #[test]
    fn invalid_position_has_stable_code() {
        let engine = Engine::new(EngineConfig::default()).unwrap();
        let session = engine.create_blank().unwrap();
        let snapshot = session.snapshot().unwrap();
        let error = session
            .insert_text(InsertTextRequest {
                base_revision: snapshot.revision,
                at: Position {
                    node: initial_paragraph(&snapshot),
                    grapheme_offset: 1,
                    affinity: Affinity::After,
                },
                text: "A".to_owned(),
                marks: BTreeSet::new(),
            })
            .unwrap_err();

        assert_eq!(error.code().as_str(), "ODC-2002");
        assert_eq!(session.snapshot().unwrap(), snapshot);
    }

    #[test]
    fn split_delete_undo_and_redo_are_revisioned() {
        let engine = Engine::new(EngineConfig::default()).unwrap();
        let session = engine.create_blank().unwrap();
        let blank = session.snapshot().unwrap();
        let first = initial_paragraph(&blank);
        session
            .insert_text(InsertTextRequest {
                base_revision: blank.revision,
                at: Position {
                    node: first.clone(),
                    grapheme_offset: 0,
                    affinity: Affinity::After,
                },
                text: "abCD".to_owned(),
                marks: BTreeSet::new(),
            })
            .unwrap();

        let split = session
            .split_paragraph(SplitParagraphRequest {
                base_revision: Revision::new(1),
                at: Position {
                    node: first.clone(),
                    grapheme_offset: 2,
                    affinity: Affinity::After,
                },
            })
            .unwrap();
        let second = match &split.position_map.steps()[0] {
            MappingStep::Split { new_node, .. } => new_node.clone(),
            other => panic!("unexpected mapping step: {other:?}"),
        };
        let after_split = session.snapshot().unwrap();
        assert_eq!(paragraph_text(&after_split, 0), "ab");
        assert_eq!(paragraph_text(&after_split, 1), "CD");

        session
            .delete_range(DeleteRangeRequest {
                base_revision: Revision::new(2),
                range: Range {
                    start: Position {
                        node: second.clone(),
                        grapheme_offset: 0,
                        affinity: Affinity::Before,
                    },
                    end: Position {
                        node: second,
                        grapheme_offset: 1,
                        affinity: Affinity::After,
                    },
                },
            })
            .unwrap();
        assert_eq!(paragraph_text(&session.snapshot().unwrap(), 1), "D");

        session.undo(Revision::new(3)).unwrap();
        assert_eq!(paragraph_text(&session.snapshot().unwrap(), 1), "CD");
        session.undo(Revision::new(4)).unwrap();
        let joined = session.snapshot().unwrap();
        assert_eq!(joined.body.len(), 1);
        assert_eq!(paragraph_text(&joined, 0), "abCD");

        session.redo(Revision::new(5)).unwrap();
        assert_eq!(session.snapshot().unwrap().body.len(), 2);
        session.redo(Revision::new(6)).unwrap();
        let redone = session.snapshot().unwrap();
        assert_eq!(paragraph_text(&redone, 0), "ab");
        assert_eq!(paragraph_text(&redone, 1), "D");
        assert_eq!(redone.revision, Revision::new(7));
    }

    #[test]
    fn failed_history_action_preserves_state() {
        let engine = Engine::new(EngineConfig::default()).unwrap();
        let session = engine.create_blank().unwrap();
        let before = session.snapshot().unwrap();

        let error = session.undo(before.revision).unwrap_err();

        assert_eq!(error.code().as_str(), "ODC-2006");
        assert_eq!(session.snapshot().unwrap(), before);
    }

    #[test]
    fn stale_undo_does_not_consume_history() {
        let engine = Engine::new(EngineConfig::default()).unwrap();
        let session = engine.create_blank().unwrap();
        let blank = session.snapshot().unwrap();
        session
            .insert_text(InsertTextRequest {
                base_revision: blank.revision,
                at: Position {
                    node: initial_paragraph(&blank),
                    grapheme_offset: 0,
                    affinity: Affinity::After,
                },
                text: "history".to_owned(),
                marks: BTreeSet::new(),
            })
            .unwrap();

        let error = session.undo(Revision::new(0)).unwrap_err();
        assert_eq!(error.code().as_str(), "ODC-2001");
        session.undo(Revision::new(1)).unwrap();
        assert_eq!(paragraph_text(&session.snapshot().unwrap(), 0), "");
    }

    #[test]
    fn reversed_join_is_rejected_atomically() {
        let engine = Engine::new(EngineConfig::default()).unwrap();
        let session = engine.create_blank().unwrap();
        let blank = session.snapshot().unwrap();
        let first = initial_paragraph(&blank);
        let split = session
            .split_paragraph(SplitParagraphRequest {
                base_revision: blank.revision,
                at: Position {
                    node: first.clone(),
                    grapheme_offset: 0,
                    affinity: Affinity::After,
                },
            })
            .unwrap();
        let second = match &split.position_map.steps()[0] {
            MappingStep::Split { new_node, .. } => new_node.clone(),
            other => panic!("unexpected mapping step: {other:?}"),
        };
        let before = session.snapshot().unwrap();

        let error = session
            .join_paragraphs(JoinParagraphRequest {
                base_revision: before.revision,
                first: second,
                second: first,
            })
            .unwrap_err();

        assert_eq!(error.code().as_str(), "ODC-2002");
        assert_eq!(session.snapshot().unwrap(), before);
    }
}
