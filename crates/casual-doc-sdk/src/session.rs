//! Thread-safe live document session and its runtime subscription.

use std::sync::atomic::AtomicU64;
use std::sync::{Arc, RwLock};

use casual_doc_model as model;
use casual_doc_selection as selection;
use casual_doc_transaction as transaction;

use crate::command::{
    DeleteRangeRequest, InsertTextRequest, JoinParagraphRequest, SetSelectionRequest,
    SplitParagraphRequest, TransactionResult, transaction_result,
};
use crate::config::{SessionId, next_counter};
use crate::error::{
    ErrorCode, ErrorSeverity, SdkError, map_mapped_selection_error, map_requested_selection_error,
    map_snapshot_error, map_transaction_error, stale_revision_error,
};
use crate::event::{
    EventBatch, EventJournal, RuntimeEvent, SelectionChangeReason, SelectionChangedEvent,
    TransactionCommittedEvent, TransactionOrigin,
};
use crate::selection::SelectionSnapshot;
use crate::snapshot::{DocumentSnapshot, snapshot_from_internal};
use crate::value::{Mark, Revision};

#[derive(Debug)]
pub(crate) struct SessionState {
    pub(crate) document: model::Document,
    pub(crate) revision: transaction::RevisionId,
    pub(crate) selection: selection::TextSelection,
    pub(crate) undo_stack: Vec<Vec<transaction::Operation>>,
    pub(crate) redo_stack: Vec<Vec<transaction::Operation>>,
    pub(crate) events: EventJournal,
}

/// Future-only cursor over one session's bounded runtime event journal.
#[derive(Debug)]
pub struct Subscription {
    state: Arc<RwLock<SessionState>>,
    next_sequence: u64,
}

impl Subscription {
    /// Drains at most `max_events` without blocking.
    pub fn drain(&mut self, max_events: usize) -> Result<EventBatch, SdkError> {
        if max_events == 0 {
            return Err(SdkError::new(
                ErrorCode::InvalidArgument,
                ErrorSeverity::Error,
                "event drain size must be greater than zero",
            ));
        }
        let state = self
            .state
            .read()
            .map_err(|_| SdkError::internal("session state lock is poisoned"))?;
        let (batch, next_sequence) = state.events.read_from(self.next_sequence, max_events);
        self.next_sequence = next_sequence;
        Ok(batch)
    }
}

/// Thread-safe live document session.
#[derive(Clone, Debug)]
pub struct DocumentSession {
    pub(crate) id: SessionId,
    pub(crate) state: Arc<RwLock<SessionState>>,
    pub(crate) next_transaction: Arc<AtomicU64>,
    pub(crate) node_namespace: u64,
    pub(crate) next_node: Arc<AtomicU64>,
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

    /// Returns the current directed logical selection.
    pub fn selection(&self) -> Result<SelectionSnapshot, SdkError> {
        let state = self
            .state
            .read()
            .map_err(|_| SdkError::internal("session state lock is poisoned"))?;
        Ok(SelectionSnapshot::from_internal(state.selection))
    }

    /// Subscribes to events emitted after this call.
    pub fn subscribe(&self) -> Result<Subscription, SdkError> {
        let state = self
            .state
            .read()
            .map_err(|_| SdkError::internal("session state lock is poisoned"))?;
        let next_sequence = state.events.next_sequence;
        Ok(Subscription {
            state: Arc::clone(&self.state),
            next_sequence,
        })
    }

    /// Replaces selection after validating its revision and document positions.
    pub fn set_selection(&self, request: SetSelectionRequest) -> Result<(), SdkError> {
        let anchor = request.selection.anchor.to_internal()?;
        let focus = request.selection.focus.to_internal()?;
        let mut state = self.lock_state()?;
        if request.base_revision.get() != state.revision.get() {
            return Err(stale_revision_error(request.base_revision, state.revision));
        }
        let selection = selection::TextSelection::new(&state.document, anchor, focus)
            .map_err(map_requested_selection_error)?;
        if state.selection == selection {
            return Ok(());
        }
        let event = RuntimeEvent::SelectionChanged(SelectionChangedEvent {
            revision: Revision(state.revision.get()),
            selection: SelectionSnapshot::from_internal(selection),
            reason: SelectionChangeReason::Explicit,
        });
        state.events.append(vec![event])?;
        state.selection = selection;
        Ok(())
    }

    /// Exports the committed normalized document as deterministic compact JSON.
    pub fn export_normalized_json(&self) -> Result<Vec<u8>, SdkError> {
        let state = self
            .state
            .read()
            .map_err(|_| SdkError::internal("session state lock is poisoned"))?;
        state.document.to_json().map_err(map_snapshot_error)
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
        let new_id = self.allocate_node_id()?;
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
        let mapped_selection = state
            .selection
            .mapped(&commit.position_map, &commit.document)
            .map_err(map_mapped_selection_error)?;
        let mut events = vec![RuntimeEvent::TransactionCommitted(
            TransactionCommittedEvent {
                result: result.clone(),
                origin: TransactionOrigin::Forward,
            },
        )];
        if state.selection != mapped_selection {
            events.push(RuntimeEvent::SelectionChanged(SelectionChangedEvent {
                revision: result.revision,
                selection: SelectionSnapshot::from_internal(mapped_selection),
                reason: SelectionChangeReason::Transaction,
            }));
        }
        state.events.append(events)?;
        state.document = commit.document;
        state.revision = commit.revision;
        state.selection = mapped_selection;
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
        let mapped_selection = state
            .selection
            .mapped(&commit.position_map, &commit.document)
            .map_err(map_mapped_selection_error)?;
        let (origin, selection_reason) = match direction {
            HistoryDirection::Undo => (TransactionOrigin::Undo, SelectionChangeReason::Undo),
            HistoryDirection::Redo => (TransactionOrigin::Redo, SelectionChangeReason::Redo),
        };
        let mut events = vec![RuntimeEvent::TransactionCommitted(
            TransactionCommittedEvent {
                result: result.clone(),
                origin,
            },
        )];
        if state.selection != mapped_selection {
            events.push(RuntimeEvent::SelectionChanged(SelectionChangedEvent {
                revision: result.revision,
                selection: SelectionSnapshot::from_internal(mapped_selection),
                reason: selection_reason,
            }));
        }
        state.events.append(events)?;

        state.document = commit.document;
        state.revision = commit.revision;
        state.selection = mapped_selection;
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

    pub(crate) fn lock_state(
        &self,
    ) -> Result<std::sync::RwLockWriteGuard<'_, SessionState>, SdkError> {
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

    fn allocate_node_id(&self) -> Result<model::NodeId, SdkError> {
        loop {
            let node_counter = next_counter(&self.next_node, "node")?;
            let candidate = model::NodeId::from_parts(self.node_namespace, node_counter)
                .map_err(|_| SdkError::internal("node ID allocation failed"))?;
            let state = self
                .state
                .read()
                .map_err(|_| SdkError::internal("session state lock is poisoned"))?;
            if !state.document.has_node_id(candidate) {
                return Ok(candidate);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HistoryDirection {
    Undo,
    Redo,
}
