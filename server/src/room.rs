// SPDX-License-Identifier: Apache-2.0

//! A room: one document's order, made durable.
//!
//! The room is the only place where the ordering decision and the durable record of it are
//! taken together, and the order of those two steps is the contract:
//!
//! > **journal first, answer second.**
//!
//! A client that has been told its chunk is ordered must not be able to outlive the record of
//! it. Answering first and journalling after would make every acknowledgement a promise the
//! relay might not keep — and the client has already dropped the chunk from its outstanding set
//! by then, so the work is simply gone. [`Room::commit`] therefore returns the journal's error
//! rather than the relay's answer when the write fails, and the caller must not acknowledge.

use std::path::Path;

use casual_doc_edit::access::AccessChangeRefusal;
use casual_doc_edit::access::Capabilities;
use casual_doc_transaction::protocol::{
    ClientId, ClientMessage, Outcome, Revision, ServerMessage, Submission,
};
use casual_doc_transaction::session::{Ordered, ServerSession};

use crate::journal::{Journal, JournalError, Recovered};

/// Why a room could not be opened or could not take a chunk.
#[derive(Debug)]
#[non_exhaustive]
pub enum RoomError {
    /// The durable log refused.
    Journal(JournalError),
}

impl core::fmt::Display for RoomError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Journal(error) => write!(formatter, "{error}"),
        }
    }
}

impl core::error::Error for RoomError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Journal(error) => Some(error),
        }
    }
}

impl From<JournalError> for RoomError {
    fn from(error: JournalError) -> Self {
        Self::Journal(error)
    }
}

/// One document's room: the ordering state machine plus its durable log.
#[derive(Debug)]
pub struct Room {
    session: ServerSession,
    journal: Journal,
}

impl Room {
    /// Creates a room the **host** has decided to open, at `path`.
    ///
    /// Refuses if a journal is already there, because overwriting one destroys the only copy of
    /// an order clients have been acknowledged against. "The host creates the room, not the
    /// first client" (`152`) is enforced here by there being no other way in.
    ///
    /// # Errors
    ///
    /// [`RoomError::Journal`], including when the room already exists.
    pub fn create(path: impl AsRef<Path>) -> Result<Self, RoomError> {
        let session = ServerSession::default();
        let journal = Journal::create(path, &session)?;
        Ok(Self { session, journal })
    }

    /// Re-opens an existing room, replaying its log.
    ///
    /// # Errors
    ///
    /// [`RoomError::Journal`] — a damaged or disagreeing log is refused rather than started
    /// empty. An empty start would silently drop every chunk already acknowledged.
    pub fn open(path: impl AsRef<Path>) -> Result<(Self, Recovered), RoomError> {
        let (journal, recovered) = Journal::open(path)?;
        Ok((
            Self {
                session: recovered.session.clone(),
                journal,
            },
            recovered,
        ))
    }

    /// Admits a client with `granted`, **journals the admission, and only then answers**.
    ///
    /// The same contract as [`Room::commit`], for the same reason, and the previous version of
    /// this comment was simply wrong: it said "not journalled: a participant number is handed
    /// out again on the next join and nothing anybody was acknowledged for depends on it". The
    /// dedupe table is keyed on the participant number, so *everything* anybody was
    /// acknowledged for depends on it — and `Journal::append_admission`'s doc comment records
    /// what a crash actually did, measured rather than reasoned about.
    ///
    /// A refused join (a version mismatch, a malformed message, a resume that is too far behind)
    /// changed nothing, so there is nothing to journal and nothing to fail on.
    ///
    /// `granted` is what the **boundary** got out of the grant it verified (ADR-060). This type
    /// does not verify anything: it holds a journal and an order, and a key does not belong
    /// beside either. [`Relay::handle`](crate::Relay::handle) is where the
    /// [`Access`](crate::Access) policy is consulted.
    ///
    /// # Errors
    ///
    /// [`RoomError::Journal`] when the admission could not be made durable. The caller must
    /// **not** admit: a participant number handed out and not recorded is one that will be
    /// handed out again.
    pub fn join(
        &mut self,
        message: &ClientMessage,
        granted: Capabilities,
    ) -> Result<ServerMessage, RoomError> {
        let (answer, admitted) = self.session.join(message, granted);
        if let Some(admission) = admitted {
            self.journal.append_admission(&admission)?;
        }
        Ok(answer)
    }

    /// Orders `submission`, **journals the decision, and only then returns it**.
    ///
    /// # Errors
    ///
    /// [`RoomError::Journal`] when the decision could not be made durable. The caller must not
    /// acknowledge: an unacknowledged chunk is retried by its client and lands again, while an
    /// acknowledged one that was never recorded is lost for good. That asymmetry is why this
    /// returns an error instead of the answer.
    pub fn commit(&mut self, submission: &Submission) -> Result<Outcome, RoomError> {
        let outcome = self.session.commit(submission);
        if let Outcome::Ordered { revision } = outcome {
            self.journal.append(submission, revision)?;
            if self.journal.should_compact() {
                self.journal.compact(&self.session)?;
            }
        }
        Ok(outcome)
    }

    /// The ordered entries after `revision`, for catching a reconnecting client up.
    ///
    /// `None` when `revision` is outside the retained window — which is the bounded-offline
    /// refusal, not an error: `152` makes the point that the loss is *announced*.
    pub fn history_since(
        &self,
        revision: Revision,
    ) -> Option<impl ExactSizeIterator<Item = &Ordered>> {
        self.session.history_since(revision)
    }

    /// Changes what `target` may do, at `actor`'s request — `ClientMessage::SetAccess`.
    ///
    /// **Not journalled, and that is the one place this type's own contract does not apply.**
    /// Every other mutation here is journalled before it is answered, because a client that has
    /// been told something took must not be able to outlive the record of it. A rights change is
    /// different in kind: it already ends at the target's next reconnect, where the grant is
    /// re-verified and `ServerSession::join` overwrites the entry — so a journal would make a
    /// *crash* the one reconnect it survived, and one rule with two behaviours is the pair that
    /// diverges. `ServerSession::set_access` carries the full argument.
    ///
    /// # Errors
    ///
    /// The `AccessChangeRefusal` the session decided. No [`RoomError`], because nothing durable
    /// is touched — which is why this signature does not carry one rather than carrying an error
    /// that can never happen.
    pub fn set_access(
        &mut self,
        actor: ClientId,
        target: ClientId,
        requested: Capabilities,
    ) -> Result<Capabilities, AccessChangeRefusal> {
        self.session.set_access(actor, target, requested)
    }

    /// The relay's state, for a caller that needs to checkpoint it or report on it.
    #[must_use]
    pub const fn session(&self) -> &ServerSession {
        &self.session
    }

    /// Writes a checkpoint now, whatever [`Journal::should_compact`] thinks.
    ///
    /// What a clean shutdown calls, so the next start replays nothing.
    ///
    /// # Errors
    ///
    /// [`RoomError::Journal`].
    pub fn checkpoint(&mut self) -> Result<(), RoomError> {
        self.journal.compact(&self.session)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "room_tests.rs"]
mod tests;
