//! Bounded runtime event journal and its public event payloads.

use std::collections::VecDeque;

use crate::command::TransactionResult;
use crate::error::SdkError;
use crate::selection::SelectionSnapshot;
use crate::value::Revision;

/// Number of recent runtime events retained by each Phase 0 session.
pub const EVENT_JOURNAL_CAPACITY: usize = 256;

/// Session-local monotonic runtime event identity.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct EventSequence(u64);

impl EventSequence {
    /// Returns the numeric event sequence.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Source of a committed transaction event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransactionOrigin {
    /// A normal forward editing request.
    Forward,
    /// An undo history request.
    Undo,
    /// A redo history request.
    Redo,
}

/// Reason that canonical session selection changed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionChangeReason {
    /// A host explicitly replaced selection.
    Explicit,
    /// A forward transaction mapped selection.
    Transaction,
    /// An undo transaction mapped selection.
    Undo,
    /// A redo transaction mapped selection.
    Redo,
}

/// Payload emitted after one transaction commits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransactionCommittedEvent {
    /// Committed transaction result.
    pub result: TransactionResult,
    /// Source of the transaction.
    pub origin: TransactionOrigin,
}

/// Payload emitted after canonical selection changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectionChangedEvent {
    /// Document revision against which the selection resolves.
    pub revision: Revision,
    /// Complete directed selection after the change.
    pub selection: SelectionSnapshot,
    /// Cause of the selection transition.
    pub reason: SelectionChangeReason,
}

/// Runtime notification emitted by a document session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeEvent {
    /// A document transaction committed.
    TransactionCommitted(TransactionCommittedEvent),
    /// Canonical session selection changed.
    SelectionChanged(SelectionChangedEvent),
}

/// Runtime event paired with its session-local sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SequencedEvent {
    /// Strictly increasing event sequence.
    pub sequence: EventSequence,
    /// Typed event payload.
    pub event: RuntimeEvent,
}

/// One non-blocking subscription read.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EventBatch {
    /// Number of events no longer retained before this batch.
    pub dropped_events: u64,
    /// Ordered retained events, limited by the drain request.
    pub events: Vec<SequencedEvent>,
}

#[derive(Debug)]
pub(crate) struct EventJournal {
    pub(crate) next_sequence: u64,
    pub(crate) retained: VecDeque<SequencedEvent>,
}

impl Default for EventJournal {
    fn default() -> Self {
        Self {
            next_sequence: 1,
            retained: VecDeque::with_capacity(EVENT_JOURNAL_CAPACITY),
        }
    }
}

impl EventJournal {
    pub(crate) fn append(&mut self, events: Vec<RuntimeEvent>) -> Result<(), SdkError> {
        let count = u64::try_from(events.len())
            .map_err(|_| SdkError::internal("runtime event count exceeds sequence capacity"))?;
        let next_sequence = self
            .next_sequence
            .checked_add(count)
            .ok_or_else(|| SdkError::internal("runtime event sequence is exhausted"))?;

        for (sequence, event) in (self.next_sequence..next_sequence).zip(events) {
            if self.retained.len() == EVENT_JOURNAL_CAPACITY {
                self.retained.pop_front();
            }
            self.retained.push_back(SequencedEvent {
                sequence: EventSequence(sequence),
                event,
            });
        }
        self.next_sequence = next_sequence;
        Ok(())
    }

    pub(crate) fn read_from(&self, cursor: u64, max_events: usize) -> (EventBatch, u64) {
        let earliest = self
            .retained
            .front()
            .map_or(self.next_sequence, |event| event.sequence.get());
        let dropped_events = earliest.saturating_sub(cursor);
        let effective_cursor = cursor.max(earliest);
        let events: Vec<_> = self
            .retained
            .iter()
            .filter(|event| event.sequence.get() >= effective_cursor)
            .take(max_events)
            .cloned()
            .collect();
        let next_cursor = events.last().map_or(effective_cursor, |event| {
            event.sequence.get().saturating_add(1)
        });
        (
            EventBatch {
                dropped_events,
                events,
            },
            next_cursor,
        )
    }
}
