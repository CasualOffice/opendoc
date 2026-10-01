// SPDX-License-Identifier: Apache-2.0

//! The two session state machines, and the rollback/replay rebase driver.
//!
//! # What is here
//!
//! - [`ClientSession`] — a participant's protocol state: its chunk counter, what is in
//!   flight, where the order has reached, and the one-way desync latch. Pure, except for
//!   [`ClientSession::receive`], which is the driver.
//! - [`ServerSession`] — the order, the dedupe table, and the retained history. It holds
//!   **no document**, runs **no transform**, and interprets no operation. Doc 152 / ADR-047
//!   is the decision that made that a property rather than an accident.
//! - [`ClientSession::receive`] — the rebase driver, and the only thing in this module that
//!   touches a document. It lives here because this is the one place that holds the log, the
//!   document and [`transform`](crate::transform) together, which is what doc 147's choke
//!   point requires: every mutation still goes through
//!   [`RevisionLog`], including a rollback.
//!
//! No clock, no socket, no task, no channel. Time and identity arrive as arguments, which is
//! the only way a save cadence or a reconnect race is testable at all.
//!
//! # Why a client with nothing pending does not roll back
//!
//! The common case is not the contended one. A participant who is reading, or who has had
//! every edit acknowledged, has no unordered commit — so an arriving chunk is applied by
//! [`RevisionLog::apply`], **the same call a local keystroke
//! makes**, with no rollback, no transform, no placement and no working copy. That is doc
//! 107 B6 ("a remote keystroke must cost what a local one costs") for free, and it is
//! asserted rather than hoped for.
//!
//! # Why rebasing a sequence needs a probe, and what the probe rests on
//!
//! `transform(subject, against, side)` reads `against`'s **inverse** to know what the
//! concurrent change destroyed (doc 150 §2.3). A peer receives operations, never inverses —
//! a malicious inverse would be a way to diverge the replicas quietly — so the inverse has
//! to be produced locally, and only `apply` produces one.
//!
//! For the first unordered commit, rollback gives it: at the base state, applying the
//! arriving operations yields their inverses. For the *second*, the arrival has to be seen
//! as it would be after the first, and its inverse there is the composition
//! `p₁'⁻¹ ∘ R⁻¹ ∘ p₁` — three operations, where [`Change`] carries
//! one. So the driver does not compose; it **probes**: at each state the rollback walks back
//! through, it applies the arrival's image, keeps the inverses, and applies them straight
//! back. Two O(edit) applications and no document copy.
//!
//! The probe rests on exactly one invariant, and it is one undo already rests on: applying
//! an operation's inverse restores the state it was applied to (ADR-030 I2). If that is ever
//! false, undo is broken too, and `a_probe_leaves_the_document_exactly_as_it_found_it` is
//! the guard that says so.
//!
//! # Where the base-state placement comes from — doc 150 §10 Q1, closed
//!
//! Doc 150 §4 requires a [`BlockPlacement`] resolved
//! against the state **both** operations were written against, and recorded as its sharpest
//! edge that "neither replica holds that state at the moment it transforms". A rollback
//! driver does: rolling back to the horizon *is* arriving at that state. The driver builds
//! [`BlockIndex`] there and nowhere else, so the precondition
//! is satisfied by construction rather than by a caller's promise. The cost is one
//! O(document) walk per **contended** arrival — never per keystroke, and never on the
//! uncontended path. Doc 152 §10 Q1 records maintaining it incrementally as the way to pay
//! less.

use casual_doc_model::IdGenerator;
use casual_doc_model::v1::Document;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::error::Error;
use std::fmt;

use casual_doc_edit::access::Capabilities;
use casual_doc_edit::{EditError, Mint};

use crate::protocol::{
    Arrival, Base, CHUNK_BUDGET_BYTES, ClientId, ClientMessage, Identity, Join, MAX_OUTSTANDING,
    Outcome, PROTOCOL_VERSION, Refusal, Resume, ResumeKey, Revision, Seq, ServerMessage,
    Submission,
};
use crate::transform::{
    ANCHOR_UNRESOLVED, BlockIndex, BlockPlacement, Change, Rebase, Side, Tombstone, TransformError,
    transform_declared,
};
use crate::wire::{self, Collision, IdSpace, WireOperation};
use crate::{AnchorError, Intent};
use crate::{Commit, Operation, RevisionId, RevisionLog, TransactionError};

/// Why a session step could not be taken.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SessionError {
    /// The two ends do not speak the same protocol. **Terminal, and never retried.**
    ProtocolVersion {
        /// What the far end speaks.
        server: u32,
        /// What this end speaks.
        client: u32,
    },
    /// A message arrived that this state does not accept — a `Resumed` on a fresh session, a
    /// `Welcome` on an established one, a submission before a join.
    OutOfOrder,
    /// The session has been stopped and accepts nothing further.
    Stopped(Refusal),
    /// An arrival could not be merged, and the latch is now set: this replica's view and the
    /// order have diverged and nothing here knows how to reconcile them. Local editing still
    /// works — the line is drawn exactly at the network — and recovery is an ordinary
    /// rejoin with a fresh resume key, so the server cannot resume it.
    Desynced,
    /// An unordered commit records no inverse, so it cannot be rolled back and the arrival
    /// cannot be merged.
    ///
    /// This is **not** an open question — doc 150 §10 Q2 recorded it as one. A
    /// [`Coalesce::ContinueKeepingFirstInverse`](crate::Coalesce::ContinueKeepingFirstInverse) commit deliberately keeps
    /// no inverse, to stop one word of suggested typing retaining one whole-paragraph
    /// snapshot per character. Such a commit can neither serve as a concurrent change nor be
    /// rolled back, so **suggesting mode cannot take part in a session** until doc 147's
    /// envelope retains inverses and drops them at undo-read time instead. Refused with a
    /// stable code rather than allowed to diverge.
    NotRollbackable,
    /// No transform exists for a pair that met.
    CannotMerge(TransformError),
    /// The model refused a correctly rebased operation — its own invariants still apply, and
    /// doc 150 §6 says a caller must treat this exactly as it treats a refusal to transform.
    Refused(TransactionError),
    /// An arriving operation introduces an identity this replica already holds.
    IdCollision(Collision),
    /// A participant number for which no id space exists (`u64::MAX`, the one value
    /// [`wire::space_of`] refuses).
    NoIdSpace,
    // A `Chained` submission from a client with nothing accepted, and a revision outside the
    // retained history, are deliberately NOT variants here. They are things a relay decides,
    // and it answers them with `Outcome::Refused` / `ServerMessage::Refused` directly — so a
    // variant for either would be a state this type claims and nothing can reach. An
    // unreachable variant of a public enum is a claim about the API that the code does not
    // make.
}

impl SessionError {
    /// The refusal a relay would put on the wire for this, with its `ODC-7xxx` code.
    #[must_use]
    pub const fn refusal(&self) -> Refusal {
        match self {
            Self::ProtocolVersion { server, client } => Refusal::ProtocolVersion {
                server: *server,
                client: *client,
            },
            Self::CannotMerge(_) | Self::Desynced | Self::NotRollbackable | Self::Refused(_) => {
                Refusal::CannotMerge
            }
            Self::IdCollision(_) => Refusal::IdCollision,
            Self::OutOfOrder | Self::NoIdSpace => Refusal::Malformed,
            Self::Stopped(reason) => *reason,
        }
    }
}

impl fmt::Display for SessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProtocolVersion { server, client } => write!(
                formatter,
                "protocol version {client} cannot talk to protocol version {server}"
            ),
            Self::OutOfOrder => formatter.write_str("message is not accepted in this state"),
            Self::Stopped(reason) => write!(formatter, "session stopped: {}", reason.code()),
            Self::Desynced => {
                formatter.write_str("this replica has diverged from the order and must rejoin")
            }
            Self::NotRollbackable => formatter
                .write_str("an unordered commit records no inverse, so it cannot be rolled back"),
            Self::CannotMerge(error) => write!(formatter, "{error}"),
            Self::Refused(error) => write!(formatter, "{error}"),
            Self::IdCollision(collision) => write!(
                formatter,
                "arriving operation introduces identity {} which this replica already holds",
                collision.id
            ),
            Self::NoIdSpace => formatter.write_str("no identity space exists for that client"),
        }
    }
}

impl Error for SessionError {}

/// What merging one arrival did.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Reception {
    /// Where the arrival landed in the order.
    pub revision: Revision,
    /// How many of this replica's own unordered commits were rolled back and replayed.
    /// **Zero on the uncontended path**, which is the path that must cost nothing.
    pub replayed: usize,
    /// Operations dropped because the thing they addressed no longer exists.
    ///
    /// **Must be reported.** A tombstone is a loss, and no-silent-loss means it reaches the
    /// disposition taxonomy (`35`). Carried out of the driver rather than logged inside it,
    /// because the driver has no idea who to tell.
    pub tombstones: Vec<Tombstone>,
}

/// One chunk this client has sent and not yet had acknowledged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Outstanding {
    seq: Seq,
    /// The local revision the log had reached when this chunk was taken, so an
    /// acknowledgement can settle exactly the commits it covers.
    upto: RevisionId,
}

/// A participant's protocol state.
#[derive(Clone, Debug)]
pub struct ClientSession {
    client: ClientId,
    /// What the relay said this participant may do. **Received, never asserted** — see
    /// [`ServerMessage::Welcome`]'s `capabilities` field.
    capabilities: Capabilities,
    space: IdSpace,
    document_space: IdSpace,
    revision: Revision,
    chunks: u64,
    sent: Vec<Outstanding>,
    flushed: RevisionId,
    awaiting: Option<Revision>,
    desynced: bool,
    stopped: Option<Refusal>,
}

impl ClientSession {
    /// Establishes a session from a [`ServerMessage::Welcome`].
    ///
    /// The protocol version is compared for **equality before anything else**, and a
    /// mismatch is [`SessionError::ProtocolVersion`] — which callers must treat as terminal,
    /// because a client that retries a version mismatch loops for ever.
    ///
    /// Joining **settles the log at its current head**: whatever the host edited before
    /// joining belongs to the document the room was created from, not to this session, and a
    /// rebase must never roll it back.
    ///
    /// # Errors
    ///
    /// [`SessionError::ProtocolVersion`] on a mismatch, [`SessionError::OutOfOrder`] for any
    /// other message, [`SessionError::NoIdSpace`] for the one participant number that has no
    /// space of its own.
    pub fn joined(
        document: &Document,
        message: &ServerMessage,
        log: &mut RevisionLog,
    ) -> Result<Self, SessionError> {
        let ServerMessage::Welcome {
            protocol,
            client,
            revision,
            capabilities,
        } = *message
        else {
            return Err(SessionError::OutOfOrder);
        };
        if protocol != PROTOCOL_VERSION {
            return Err(SessionError::ProtocolVersion {
                server: protocol,
                client: PROTOCOL_VERSION,
            });
        }
        let document_space = wire::document_space(document);
        let space = wire::space_of(document_space, client).ok_or(SessionError::NoIdSpace)?;
        log.settle(log.head());
        Ok(Self {
            client,
            capabilities,
            space,
            document_space,
            revision,
            chunks: 0,
            sent: Vec::new(),
            flushed: log.head(),
            awaiting: None,
            desynced: false,
            stopped: None,
        })
    }

    /// Adopts a [`ServerMessage::Resumed`].
    ///
    /// **Keeps `sent`, the chunk counter and the flush mark.** Restarting the counter would
    /// let a new chunk collide with an old number and be discarded as a duplicate — silently.
    /// The client id must be the one this session already has: a resume asserts continuity,
    /// and a different id is not continuity.
    ///
    /// The missed operations travel **inside** the message, so a caller cannot flush before
    /// it has fed them to [`ClientSession::receive`]; `awaiting` enforces that as state
    /// rather than as an ordering assumption.
    ///
    /// # Errors
    ///
    /// As [`ClientSession::joined`], plus [`SessionError::OutOfOrder`] when the id changed.
    pub fn resumed(&mut self, message: &ServerMessage) -> Result<(), SessionError> {
        let ServerMessage::Resumed {
            protocol,
            client,
            revision,
            ref missed,
            capabilities,
        } = *message
        else {
            return Err(SessionError::OutOfOrder);
        };
        if protocol != PROTOCOL_VERSION {
            return Err(SessionError::ProtocolVersion {
                server: protocol,
                client: PROTOCOL_VERSION,
            });
        }
        if client != self.client {
            return Err(SessionError::OutOfOrder);
        }
        // **Replaced, not merged.** The grant presented on *this* reconnect is the one that
        // applies, so a revocation cannot be undone by reconnecting (`143` §10). A client whose
        // own copy must only narrow is a different rule in a different place — the facade's
        // `adoptParticipantCapabilities` — because here the relay is the one speaking.
        self.capabilities = capabilities;
        if !missed.is_empty() {
            self.awaiting = Some(revision);
        }
        Ok(())
    }

    /// The participant number the relay assigned.
    #[must_use]
    pub const fn client(&self) -> ClientId {
        self.client
    }

    /// The space this participant mints identities in. Nobody else mints in it.
    #[must_use]
    pub const fn id_space(&self) -> IdSpace {
        self.space
    }

    /// What the relay said this participant may do.
    ///
    /// Useful for disabling a control *with a reason*. **Not an authority**: the relay judges
    /// every submission against its own copy, so a client that ignores this is refused on the
    /// wire rather than obeyed.
    #[must_use]
    pub const fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    /// The ordered position this session has reached.
    #[must_use]
    pub const fn revision(&self) -> Revision {
        self.revision
    }

    /// How many of this client's chunks are in flight.
    #[must_use]
    pub fn outstanding(&self) -> usize {
        self.sent.len()
    }

    /// Whether any of this client's work is still unacknowledged.
    ///
    /// Stays true while [`ClientSession::is_desynced`] is latched, so the loss is nameable
    /// rather than quietly forgotten.
    #[must_use]
    pub fn has_unacknowledged(&self) -> bool {
        !self.sent.is_empty()
    }

    /// Whether this replica has diverged from the order. **One way; never cleared.**
    #[must_use]
    pub const fn is_desynced(&self) -> bool {
        self.desynced
    }

    /// Why this session stopped, if it has.
    #[must_use]
    pub const fn stopped_because(&self) -> Option<Refusal> {
        self.stopped
    }

    /// Takes the next chunk to send, or `None` when there is nothing to send or no room to
    /// send it.
    ///
    /// Returns `None` — rather than an error — in five cases, because none of them is a
    /// fault: the session has stopped; it has desynced; it is waiting for an ordered
    /// position a refusal named; [`MAX_OUTSTANDING`] chunks are already in flight (**this is
    /// the degradation to stop-and-wait**, which is what an unpipelined client already was);
    /// or the log holds nothing above the flush mark.
    ///
    /// The byte budget spends [`CHUNK_BUDGET_BYTES`] in
    /// commit order and **always takes at least one commit**, even one that alone exceeds it,
    /// or this would spin on a submission it can never make.
    ///
    /// The base is absolute only for the first chunk after a join or a resume — the one
    /// moment a client knows an absolute answer — and [`Base::Chained`] for everything
    /// written on top of its own previous chunk.
    #[must_use]
    pub fn flush(&mut self, log: &RevisionLog) -> Option<Submission> {
        if self.stopped.is_some() || self.desynced {
            return None;
        }
        if self.awaiting.is_some_and(|needed| self.revision < needed) {
            return None;
        }
        if self.sent.len() >= MAX_OUTSTANDING {
            return None;
        }
        // The floor is the horizon as well as the flush mark, and both halves are load-bearing.
        // A remote chunk becomes a commit in *this* log — that is the point of it going down
        // `RevisionLog::apply` — so "everything not yet sent" would include somebody else's
        // edit, offered back under this client's own sequence number and applied twice by
        // everyone. Below the horizon is, by definition, already ordered.
        let floor = self.flushed.max(log.horizon());
        let mut operations = Vec::new();
        let mut spent = 0_usize;
        let mut upto = floor;
        for commit in log.commits().filter(|c| c.revision() > floor) {
            // Each operation travels with the space it minted in, so the receiver creates
            // the same nodes under the same names rather than under its own.
            let carried: Vec<WireOperation> = commit
                .operations()
                .iter()
                .cloned()
                .zip(commit.mints().iter().copied())
                .enumerate()
                .map(|(index, (operation, mint))| {
                    // The author's declarations travel too: a receiver cannot reconstruct an
                    // authoring fact, which is the whole reason it is on the envelope
                    // (doc 150 §9.1/§9.2, ADR-056).
                    WireOperation::of(operation, mint).declaring(commit.intent(index))
                })
                .collect();
            let cost: usize = carried.iter().map(WireOperation::carried_bytes).sum();
            if !operations.is_empty() && spent + cost > CHUNK_BUDGET_BYTES {
                break;
            }
            spent += cost;
            operations.extend(carried);
            upto = commit.revision();
        }
        if operations.is_empty() {
            return None;
        }
        self.chunks = self.chunks.saturating_add(1);
        let seq = Seq::new(self.chunks);
        let base = if self.sent.is_empty() {
            Base::Revision(self.revision)
        } else {
            Base::Chained
        };
        self.sent.push(Outstanding { seq, upto });
        self.flushed = upto;
        Some(Submission {
            client: self.client,
            seq,
            base,
            operations,
        })
    }

    /// Records a cumulative acknowledgement.
    ///
    /// Every chunk up to and including `through` is ordered, so that prefix is dropped and
    /// the log's horizon advances to cover exactly those commits. A lost or skipped
    /// acknowledgement is therefore self-healing: the next one covers it.
    ///
    /// # Errors
    ///
    /// [`SessionError::Stopped`] when the session has stopped.
    pub fn acknowledge(
        &mut self,
        through: Seq,
        revision: Revision,
        log: &mut RevisionLog,
    ) -> Result<(), SessionError> {
        if let Some(reason) = self.stopped {
            return Err(SessionError::Stopped(reason));
        }
        let settled = self
            .sent
            .iter()
            .filter(|chunk| chunk.seq <= through)
            .map(|chunk| chunk.upto)
            .max();
        self.sent.retain(|chunk| chunk.seq > through);
        if let Some(settled) = settled {
            log.settle(settled);
        }
        self.revision = self.revision.max(revision);
        Ok(())
    }

    /// Records a refusal of one chunk, or of the session.
    ///
    /// A [`Refusal::StaleBase`] puts the chunk back: nothing was lost, the operations are
    /// still in the log, and the flush mark rewinds so they are sent again with a base the
    /// document has actually reached. `awaiting` then holds `flush` closed until this
    /// replica has received everything that caused the refusal — a **state** guarantee
    /// rather than a promise about message ordering.
    ///
    /// # Errors
    ///
    /// [`SessionError::Stopped`] once a terminal refusal has been recorded.
    pub fn refused(
        &mut self,
        seq: Option<Seq>,
        reason: Refusal,
        log: &RevisionLog,
    ) -> Result<(), SessionError> {
        if reason.is_terminal() {
            self.stopped = Some(reason);
            return Err(SessionError::Stopped(reason));
        }
        match (seq, reason) {
            (Some(seq), Refusal::StaleBase { current }) => {
                if let Some(position) = self.sent.iter().position(|chunk| chunk.seq == seq) {
                    // Everything from this chunk onward is unsent again: the chunks after it
                    // said `Chained`, and a chain whose first link was refused has no base.
                    self.sent.truncate(position);
                    self.flushed = self
                        .sent
                        .last()
                        .map_or_else(|| log.horizon(), |chunk| chunk.upto);
                }
                self.awaiting = Some(current);
            }
            (_, Refusal::TooFarBehind { .. }) => {
                // The work this client had not had acknowledged is gone, and saying so is
                // the whole point of the refusal. The caller takes a fresh snapshot.
                self.desynced = true;
            }
            _ => {}
        }
        Ok(())
    }

    /// Records a terminal stop.
    pub fn stop(&mut self, reason: Refusal) {
        self.stopped = Some(reason);
    }

    /// Merges an arrival into this replica: **the rollback/replay rebase driver**.
    ///
    /// # What it does, in order
    ///
    /// 1. Localises every arriving operation through
    ///    [`WireOperation::localise`], so an identity
    ///    collision is refused before anything is applied rather than silently overwriting a
    ///    definition.
    /// 2. **If this replica has no unordered commit**, applies the arrival through
    ///    [`RevisionLog::apply`] — the same call a keystroke makes
    ///    — and returns. No rollback, no transform, no placement, no working copy.
    /// 3. Otherwise: takes one working copy; computes the arrival's image at each state the
    ///    rollback will pass through; rolls the unordered commits back, probing each image's
    ///    inverse as it goes; applies the arrival at the horizon; and replays each unordered
    ///    commit rebased over the arrival's image at that commit's own base, keeping the
    ///    commit's identity so undo is unaffected.
    ///
    /// On any failure the document and the log are exactly what they were on entry, and the
    /// desync latch is set only when the failure is one this replica cannot recover from by
    /// resubmitting.
    ///
    /// # Errors
    ///
    /// [`SessionError::NotRollbackable`], [`SessionError::CannotMerge`],
    /// [`SessionError::Refused`], [`SessionError::IdCollision`], [`SessionError::Desynced`]
    /// or [`SessionError::Stopped`]. Every one of them leaves the document untouched.
    pub fn receive(
        &mut self,
        arrival: &Arrival,
        document: &mut Document,
        ids: &mut IdGenerator,
        log: &mut RevisionLog,
    ) -> Result<Reception, SessionError> {
        if let Some(reason) = self.stopped {
            return Err(SessionError::Stopped(reason));
        }
        if self.desynced {
            return Err(SessionError::Desynced);
        }
        if arrival.operations.is_empty() {
            // A relay must not fan out an empty chunk, and a receiver must not quietly accept
            // one: `apply_remote` would answer "nothing to do" and the arrival would vanish
            // with a revision recorded for it. Mutating the uncontended fast path away is what
            // exposed this, which is the value of driving a guard red rather than trusting it.
            return Err(SessionError::OutOfOrder);
        }
        let sender =
            wire::space_of(self.document_space, arrival.client).ok_or(SessionError::NoIdSpace)?;
        let mut carried = Carried {
            operations: Vec::with_capacity(arrival.operations.len()),
            mints: Vec::with_capacity(arrival.operations.len()),
            intents: Vec::with_capacity(arrival.operations.len()),
        };
        for wire_operation in &arrival.operations {
            carried.operations.push(
                wire_operation
                    .localise(document, sender)
                    .map_err(SessionError::IdCollision)?
                    .clone(),
            );
            carried.mints.push(wire_operation.mint());
            carried.intents.push(wire_operation.intent());
        }

        if log.unordered_commits() == 0 {
            let revision = apply_remote(
                document,
                carried.mints,
                carried.intents,
                log,
                carried.operations,
            )?;
            log.settle(revision);
            self.revision = self.revision.max(arrival.revision);
            return Ok(Reception {
                revision: arrival.revision,
                replayed: 0,
                tombstones: Vec::new(),
            });
        }

        // The one working copy this design needs, and only on the contended path: a remote
        // edit arriving while this replica has unacknowledged work of its own. A rollback
        // that fails half way through has damaged the document and no operation repairs it,
        // so the copy is what makes "on Err the document is exactly what it was" as true here
        // as it is of `RevisionLog::apply`. The log is copied too, and that copy is O(undo
        // bound) rather than O(document). Cost: one document clone per **contended** arrival,
        // never per keystroke and never when nothing is in flight. Doc 152 §10 Q1.
        let restore_document = document.clone();
        let restore_log = log.clone();
        match self.rebase(arrival, document, ids, log, carried) {
            Ok(reception) => Ok(reception),
            Err(error) => {
                *document = restore_document;
                *log = restore_log;
                Err(error)
            }
        }
    }

    /// The contended half of [`ClientSession::receive`]. Assumes the caller holds a working
    /// copy: this leaves the document part-rebased on failure and the caller restores it.
    fn rebase(
        &mut self,
        arrival: &Arrival,
        document: &mut Document,
        ids: &mut IdGenerator,
        log: &mut RevisionLog,
        carried: Carried,
    ) -> Result<Reception, SessionError> {
        let Carried {
            operations: remote,
            mints: remote_mints,
            intents: remote_intents,
        } = carried;
        let mut tombstones = Vec::new();
        let unordered = log.detach_unordered();

        // Phase 0, pure: the arrival's image at the base of each unordered commit.
        // `images[i]` is the arrival as commit `i` would have to read it, and it is computed
        // **forwards** from the inverses the commits already recorded, touching the document
        // not at all. That is why only the inverses need a probe: the operations do not.
        let mut images: Vec<Vec<Operation>> = Vec::with_capacity(unordered.len());
        // The arrival's own declarations travel with its operations through the images, so a
        // sender that said what its index was counted to is still saying it after the image
        // has been rebased three times. An image piece inherits its parent's declaration.
        let mut image: Vec<(Operation, Intent)> = remote
            .iter()
            .enumerate()
            .map(|(index, operation)| {
                (
                    operation.clone(),
                    remote_intents.get(index).copied().unwrap_or(Intent::NONE),
                )
            })
            .collect();
        for commit in &unordered {
            images.push(image.iter().map(|(op, _)| op.clone()).collect());
            let changes = changes_of(commit)?;
            image = rebase_over_declared(
                image,
                &changes,
                Side::Earlier,
                &crate::transform::NoPlacement,
                &mut tombstones,
            )
            .map_err(SessionError::CannotMerge)?;
        }

        // Phase 1: roll back newest-first. Rolling commit `i` back arrives at commit `i`'s
        // own base, which is exactly where `images[i]`'s inverse has to be taken, so the
        // probe happens on the way past.
        let mut probed: Vec<Vec<(Operation, Operation)>> = Vec::with_capacity(unordered.len());
        for (commit, image) in unordered.iter().zip(images).rev() {
            // `inverse_operations` is stored newest-first, so position `p` undoes operation
            // `len - 1 - p` and mints in *that* operation's inverse lane. Anything else would
            // give the rollback identities the replay could not agree with.
            let count = commit.inverse_operations().len();
            for (position, operation) in commit.inverse_operations().iter().enumerate() {
                let lane = commit
                    .mints()
                    .get(count - 1 - position)
                    .ok_or(SessionError::Refused(TransactionError::Edit(
                        EditError::IdExhausted,
                    )))?
                    .inverse();
                casual_doc_edit::apply(document, lane, operation)
                    .map_err(|error| SessionError::Refused(TransactionError::Edit(error)))?;
            }
            probed.push(probe(document, ids, image)?);
        }
        probed.reverse();

        // Phase 2: the arrival lands at the horizon, through `RevisionLog::apply` — the same
        // call a keystroke makes. Its inverses come from `apply`, which is what makes the
        // arrival usable as `against` at all.
        let at_horizon: Vec<Operation> = probed.first().map_or_else(Vec::new, |changes| {
            changes
                .iter()
                .map(|(operation, _)| operation.clone())
                .collect()
        });
        // The arrival's image at the horizon *is* the arrival — `images[0]` is `remote`
        // before any transform — so it lands under the spaces its sender declared, and not
        // under spaces of this replica's own. That is the whole point of carrying them.
        let ordered_at = apply_remote(document, remote_mints, remote_intents, log, at_horizon)?;
        log.settle(ordered_at);

        // Phase 3: replay each step rebased, keeping its identity so undo is untouched. The
        // placement is resolved here and nowhere else, because *here* the document is at the
        // state both operations were written against — doc 150 §10 Q1's precondition, met by
        // construction rather than by a caller's promise.
        let placement = BlockIndex::of(document);
        let mut moved: Vec<(RevisionId, Option<RevisionId>)> = Vec::with_capacity(unordered.len());
        let mut replayed = 0_usize;
        for (commit, changes) in unordered.iter().zip(&probed) {
            let subjects: Vec<(Operation, Intent)> = commit
                .operations()
                .iter()
                .enumerate()
                .map(|(index, operation)| (operation.clone(), commit.intent(index)))
                .collect();
            let rebased =
                rebase_over_declared(subjects, changes, Side::Later, &placement, &mut tombstones)
                    .map_err(SessionError::CannotMerge)?;
            // **The anchor's one resolution point** (doc 150 §9.1, ADR-056). An anchored slot
            // came through `transform` untouched, because an identity does not move; its
            // cached index is re-derived *here*, against the document this replay is about to
            // mutate. That state is the only one the index can be right about, and unlike the
            // base state `BlockPlacement` needs, this replica holds it.
            //
            // A destroyed anchor is a loss, not a licence to fall back on the stale index:
            // falling back is the silent divergence this whole layer exists to refuse.
            let target = BlockIndex::of(document);
            let mut rebased = rebased;
            let mut lost = Vec::new();
            for (position, (operation, intent)) in rebased.iter_mut().enumerate() {
                let Some(anchor) = intent.anchor() else {
                    continue;
                };
                match crate::resolve_anchor(operation, anchor, &target) {
                    Ok(()) => {}
                    Err(AnchorError::AnchorDestroyed { .. }) => {
                        tombstones.push(Tombstone {
                            operation: crate::transform::variant_name(operation),
                            against: "Remote change",
                        });
                        lost.push(position);
                    }
                    Err(AnchorError::ContainerUnknown | AnchorError::NotASlotOperation { .. }) => {
                        return Err(SessionError::CannotMerge(TransformError::Unsupported {
                            subject: crate::transform::variant_name(operation),
                            against: "Remote change",
                            reason: ANCHOR_UNRESOLVED,
                        }));
                    }
                }
            }
            for position in lost.into_iter().rev() {
                rebased.remove(position);
            }
            let rebased: Vec<Operation> = rebased
                .into_iter()
                .map(|(operation, _)| operation)
                .collect();
            if rebased.is_empty() {
                // Every operation of this step was satisfied or tombstoned by the arrival, so
                // the step itself is gone. The tombstones already say what was lost.
                moved.push((commit.revision(), None));
                continue;
            }
            let mints = Mint::reserve_each(ids, rebased.len()).ok_or(SessionError::Refused(
                TransactionError::Edit(EditError::IdExhausted),
            ))?;
            let landed = log
                .append_rebased(document, mints, commit, rebased)
                .map_err(SessionError::Refused)?;
            moved.push((commit.revision(), Some(landed)));
            replayed += 1;
        }

        // The replay renumbered this replica's own revisions, so every mark that names one
        // is re-anchored. Leaving them would let an acknowledgement settle the horizon past a
        // commit nobody has ordered — which is divergence, arrived at by arithmetic.
        let reanchor = |old: RevisionId| -> RevisionId {
            moved
                .iter()
                .filter(|(was, _)| *was <= old)
                .filter_map(|(_, now)| *now)
                .next_back()
                .unwrap_or(ordered_at)
        };
        for chunk in &mut self.sent {
            chunk.upto = reanchor(chunk.upto);
        }
        self.flushed = reanchor(self.flushed);
        self.revision = self.revision.max(arrival.revision);
        Ok(Reception {
            revision: arrival.revision,
            replayed,
            tombstones,
        })
    }
}

/// One arrival's operations and everything that travels beside them, one entry each.
///
/// Three parallel vectors that must stay the same length, so they are one value: an arrival
/// whose mints or declarations had slipped out of step with its operations would mis-apply
/// silently, and the type is what stops them being passed separately.
struct Carried {
    operations: Vec<Operation>,
    mints: Vec<Mint>,
    intents: Vec<Intent>,
}

/// Applies an arrival as one ordered commit through the choke point.
///
/// Labelled from the engine's own step vocabulary so a host never has to invent a name for
/// somebody else's edit, and given a group of its own so a user's Undo can never reach it.
fn apply_remote(
    document: &mut Document,
    mints: Vec<Mint>,
    intents: Vec<Intent>,
    log: &mut RevisionLog,
    operations: Vec<Operation>,
) -> Result<RevisionId, SessionError> {
    if operations.is_empty() {
        return Ok(log.head());
    }
    // The sender's declarations are retained on the commit, not dropped at the boundary: this
    // commit becomes `against` for the next arrival, and a rebase of *this replica's* later
    // work reads them.
    let transaction = crate::Transaction::new(
        crate::TransactionId::new(0),
        log.head(),
        "Remote change",
        mints,
        operations,
    )
    .with_intents(intents);
    log.apply(document, transaction)
        .map(Commit::revision)
        .map_err(SessionError::Refused)
}

/// A commit's `(operation, inverse)` pairs, or [`SessionError::NotRollbackable`].
fn changes_of(commit: &Commit) -> Result<Vec<(Operation, Operation)>, SessionError> {
    commit
        .changes()
        .map_or(Err(SessionError::NotRollbackable), |changes| {
            Ok(changes
                .map(|change| (change.operation.clone(), change.inverse.clone()))
                .collect())
        })
}

/// Applies `operations`, keeps the inverses, and puts the document back exactly as it was.
///
/// This is the probe the module docs describe. It rests on one invariant — an operation's
/// inverse restores the state it was applied to — and
/// `a_probe_leaves_the_document_exactly_as_it_found_it` is what proves the invariant holds
/// rather than assuming it.
fn probe(
    document: &mut Document,
    ids: &mut IdGenerator,
    operations: Vec<Operation>,
) -> Result<Vec<(Operation, Operation)>, SessionError> {
    let exhausted = || SessionError::Refused(TransactionError::Edit(EditError::IdExhausted));
    // Spaces of this replica's own, because a probe is this replica's own work: it applies
    // the arrival to read its inverse and puts the document straight back, so the identities
    // it mints exist only for the length of the probe and never reach the wire.
    let region = Mint::reserve_each(ids, operations.len()).ok_or_else(exhausted)?;
    let lane = |index: usize| region.get(index).copied().ok_or_else(exhausted);
    let mut inverses = Vec::with_capacity(operations.len());
    for (index, operation) in operations.iter().enumerate() {
        match casual_doc_edit::apply(document, lane(index)?, operation) {
            Ok(inverse) => inverses.push(inverse),
            Err(error) => {
                // Undo what the probe did so far, so a refusal costs nothing.
                for (undone, inverse) in inverses.iter().enumerate().rev() {
                    let _ = casual_doc_edit::apply(document, lane(undone)?.inverse(), inverse);
                }
                return Err(SessionError::Refused(TransactionError::Edit(error)));
            }
        }
    }
    for (undone, inverse) in inverses.iter().enumerate().rev() {
        casual_doc_edit::apply(document, lane(undone)?.inverse(), inverse)
            .map_err(|error| SessionError::Refused(TransactionError::Edit(error)))?;
    }
    Ok(operations.into_iter().zip(inverses).collect())
}

/// Rebases every operation in `subjects` over every change in `against`, in order, carrying
/// each subject's declaration with it.
///
/// A `Satisfied` subject disappears and a `Tombstoned` one is recorded, because a tombstone
/// is a loss and the caller has to be able to report it.
///
/// A declaration is a fact about the operation's *author*, so it survives a rebase unchanged
/// and a [`Rebase::KeepMany`] gives every piece its parent's — the pieces are one intention
/// expressed as several operations, which is exactly what `150` §5.3 says they are.
fn rebase_over_declared(
    subjects: Vec<(Operation, Intent)>,
    against: &[(Operation, Operation)],
    side: Side,
    placement: &dyn BlockPlacement,
    tombstones: &mut Vec<Tombstone>,
) -> Result<Vec<(Operation, Intent)>, TransformError> {
    let mut current = subjects;
    for (operation, inverse) in against {
        let change = Change::new(operation, inverse);
        let mut next = Vec::with_capacity(current.len());
        for (subject, intent) in &current {
            match transform_declared(subject, *intent, change, side, placement)? {
                Rebase::Keep(operation) => next.push((operation, *intent)),
                Rebase::KeepMany(operations) => {
                    next.extend(operations.into_iter().map(|operation| (operation, *intent)));
                }
                Rebase::Satisfied => {}
                Rebase::Tombstoned(tombstone) => tombstones.push(tombstone),
            }
        }
        current = next;
    }
    Ok(current)
}

/// One ordered entry of a relay's history.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Ordered {
    /// Where it landed.
    pub revision: Revision,
    /// Who wrote it.
    pub client: ClientId,
    /// What they wrote, untouched.
    pub operations: Vec<WireOperation>,
}

/// The relay's state machine: the order, the dedupe table, and the retained tail.
///
/// **It holds no document and runs no transform.** That is ADR-047, and it is the whole
/// reason a relay can be replaced by somebody else's without our engine being in it. What
/// it costs is written down in doc 152 §6: a submission written against a position the
/// document has moved past is refused with [`Refusal::StaleBase`] and the client rebases it
/// and resubmits, so a contended document pays a round trip per collision.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ServerSession {
    revision: Revision,
    oldest: Revision,
    history: VecDeque<Ordered>,
    accepted: BTreeMap<ClientId, (Seq, Revision)>,
    resumes: BTreeMap<ResumeKey, (ClientId, Identity)>,
    next_client: u64,
    retain: usize,
}

/// How many ordered entries a relay retains before a disconnected client can no longer be
/// caught up by replay.
///
/// **This number is the definition of "bounded offline".** A client away for longer loses
/// the work it had not had acknowledged, and the whole point of
/// [`Refusal::TooFarBehind`] is that the loss is announced rather than silent.
pub const DEFAULT_RETAINED_REVISIONS: usize = 400;

impl Default for ServerSession {
    fn default() -> Self {
        Self::new(DEFAULT_RETAINED_REVISIONS)
    }
}

impl ServerSession {
    /// An empty session at revision zero, retaining at most `retain` ordered entries.
    #[must_use]
    pub fn new(retain: usize) -> Self {
        Self {
            revision: Revision::default(),
            oldest: Revision::default(),
            history: VecDeque::new(),
            accepted: BTreeMap::new(),
            resumes: BTreeMap::new(),
            next_client: 0,
            retain: retain.max(1),
        }
    }

    /// Where the document is.
    #[must_use]
    pub const fn head(&self) -> Revision {
        self.revision
    }

    /// The oldest position a replay can start from.
    #[must_use]
    pub const fn oldest_rebasable(&self) -> Revision {
        self.oldest
    }

    /// Everything ordered since `revision`, or `None` when that is outside the retained
    /// history.
    ///
    /// **One function decides replay versus snapshot.** `Some` means
    /// [`ServerMessage::Resumed`] with the tail inside it; `None` means
    /// [`Refusal::TooFarBehind`] *first*, then a fresh [`ServerMessage::Welcome`] and a
    /// snapshot — the loss announced before the thing that discards it lands.
    #[must_use]
    pub fn history_since(
        &self,
        revision: Revision,
    ) -> Option<impl ExactSizeIterator<Item = &Ordered>> {
        if revision < self.oldest || revision > self.revision {
            return None;
        }
        let skip = usize::try_from(revision.get().saturating_sub(self.oldest.get())).ok()?;
        Some(self.history.iter().skip(skip))
    }

    /// Handles a [`ClientMessage::Join`].
    ///
    /// The protocol version is checked for **equality before anything else**, and a mismatch
    /// answers [`ServerMessage::Stopped`] rather than [`ServerMessage::Refused`]: a client
    /// treating a version mismatch as retryable loops for ever.
    ///
    /// A resume is honoured only when the key is known **and** the identity it was issued to
    /// is the one presenting it. That reduces a key from something that authorises to
    /// something that merely disambiguates: without the scoping, anyone with a valid session
    /// could adopt another participant's [`ClientId`] and have their submissions suppressed
    /// as duplicates.
    ///
    /// The key is remembered on **every** join that offers one, resumed or not, because the
    /// point of a key is the *next* reconnect.
    ///
    /// # `granted` is an argument, for the same reason time and identity are
    ///
    /// This state machine cannot verify a grant: verification needs a key and a clock and this
    /// crate holds neither (`protocol`'s module docs). So the caller — the boundary, which has
    /// both — verifies [`Join::grant`] and passes what came out. Making it a **required
    /// argument** rather than an optional setter is the point: there is no way to admit a
    /// participant without saying what they may do, which is the same compile-error discipline
    /// ADR-052 used for the operation match. ADR-060.
    ///
    /// A resume adopts the capabilities presented **now**, not the ones the previous connection
    /// held, so reconnecting cannot undo a revocation (`143` §10).
    pub fn join(&mut self, message: &ClientMessage, granted: Capabilities) -> ServerMessage {
        let ClientMessage::Join(Join {
            protocol,
            identity,
            // Deliberately not read here. Verifying a grant needs a key and a clock, and this
            // crate holds neither; the boundary verified it and the answer arrived as
            // `granted`. Named rather than globbed so a future field cannot slip past unread.
            grant: _,
            resume,
        }) = message
        else {
            return ServerMessage::Stopped {
                reason: Refusal::Malformed,
            };
        };
        if *protocol != PROTOCOL_VERSION {
            return ServerMessage::Stopped {
                reason: Refusal::ProtocolVersion {
                    server: PROTOCOL_VERSION,
                    client: *protocol,
                },
            };
        }
        if let Some(Resume { key, revision }) = resume {
            let recognised = self
                .resumes
                .get(key)
                .filter(|(_, issued)| issued == identity)
                .map(|(client, _)| *client);
            if let Some(client) = recognised {
                if let Some(entries) = self.history_since(*revision) {
                    let missed = entries
                        .map(|entry| Arrival {
                            revision: entry.revision,
                            client: entry.client,
                            operations: entry.operations.clone(),
                        })
                        .collect();
                    return ServerMessage::Resumed {
                        protocol: PROTOCOL_VERSION,
                        client,
                        revision: self.revision,
                        missed,
                        capabilities: granted,
                    };
                }
                return ServerMessage::Refused {
                    seq: None,
                    reason: Refusal::TooFarBehind {
                        oldest: self.oldest,
                        current: self.revision,
                    },
                };
            }
        }
        let client = ClientId::new(self.next_client);
        self.next_client = self.next_client.saturating_add(1);
        if let Some(Resume { key, .. }) = resume {
            self.resumes.insert(key.clone(), (client, identity.clone()));
        }
        ServerMessage::Welcome {
            protocol: PROTOCOL_VERSION,
            client,
            revision: self.revision,
            capabilities: granted,
        }
    }

    /// Whether this session ever handed out `client` as a participant number.
    ///
    /// **Derived from `next_client`, not from a table.** Numbers are handed out from zero
    /// upwards and never reused, so "assigned" is an inequality — which matters for more than
    /// tidiness: it is a pure function of checkpointed state, so
    /// [`commit`](ServerSession::commit) stays replayable and ADR-058's "recovery verifies rather
    /// than trusts" keeps working. A capability *table* here would have made `commit` depend on
    /// a `join` that is deliberately not journalled, and recovery would have refused the relay's
    /// own file — which it did, once, before this was moved.
    #[must_use]
    pub const fn has_assigned(&self, client: ClientId) -> bool {
        client.get() < self.next_client
    }

    /// Orders one submission, or says why it cannot be.
    ///
    /// `(client, seq)` is the idempotency key: a resend of an already-ordered chunk answers
    /// [`Outcome::Duplicate`] naming where it landed **the first time**, and broadcasts
    /// nothing.
    ///
    /// [`Base::Chained`] is resolved from the same table the dedupe uses, and a `Chained`
    /// chunk from a client with nothing accepted is **refused rather than guessed at**.
    ///
    /// # Why no access check happens here, written down because the obvious place is here
    ///
    /// A chunk naming a participant number this session never handed out *should* be refused,
    /// and before ADR-060 nothing refused it anywhere: `commit` keyed everything on the dedupe
    /// table and the base, so a `Base::Revision(head)` submission could name **any**
    /// [`ClientId`]. That attributed the work to somebody else and, worse, wrote *their*
    /// `(client, seq)` entry — so that participant's own next chunk at that seq came back
    /// [`Outcome::Duplicate`] and was dropped. It is exactly the harm [`ResumeKey`]'s doc comment
    /// describes for a stolen resume key, reachable without one.
    ///
    /// **It cannot be refused here, and the reason is ADR-058's replay.** Recovery hands a logged
    /// submission back to this function and checks the answer, so `commit` may only depend on
    /// state the journal records. [`ServerSession::join`] is deliberately *not* journalled — so
    /// `next_client`, `resumes`, and any grant table are all advanced after the last checkpoint
    /// and gone on restart. A check here against any of them refuses the relay's own file:
    /// measured, not reasoned about — `a_chunk_is_durable_before_the_room_says_it_is_ordered`
    /// failed with `DecisionDiffers { logged: 1, replayed: None }` the moment one was added.
    ///
    /// So the check lives at the boundary, which is also where it can be *exact*:
    /// `opendoc_relay::Relay::handle` refuses a submission whose `client` is not the one its own
    /// socket joined as — a connection, which no pure state machine has — and refuses an
    /// operation outside the participant's capabilities. [`ServerSession::has_assigned`] is the
    /// query it uses for the weaker range check, and it is a query rather than a rule here for
    /// exactly the reason above.
    ///
    /// **The underlying gap is recorded rather than papered over.** That a join is not durable
    /// also means the *resume table* does not survive a crash, so a client whose work was
    /// acknowledged cannot resume after one and is told `TooFarBehind` instead. That is a
    /// pre-existing hole, found by this work and belonging with the journal's record set rather
    /// than with this function.
    pub fn commit(&mut self, submission: &Submission) -> Outcome {
        let previous = self.accepted.get(&submission.client).copied();
        if let Some((seq, revision)) = previous
            && submission.seq <= seq
        {
            return Outcome::Duplicate { revision };
        }
        let base = match submission.base {
            Base::Revision(revision) => revision,
            Base::Chained => match previous {
                Some((_, revision)) => revision,
                None => {
                    return Outcome::Refused {
                        reason: Refusal::Malformed,
                    };
                }
            },
        };
        if base != self.revision {
            return Outcome::Refused {
                reason: Refusal::StaleBase {
                    current: self.revision,
                },
            };
        }
        if submission.operations.is_empty() {
            return Outcome::Refused {
                reason: Refusal::Malformed,
            };
        }
        let Some(revision) = self.revision.next() else {
            return Outcome::Refused {
                reason: Refusal::NotSaving,
            };
        };
        self.revision = revision;
        self.history.push_back(Ordered {
            revision,
            client: submission.client,
            operations: submission.operations.clone(),
        });
        self.accepted
            .insert(submission.client, (submission.seq, revision));
        while self.history.len() > self.retain {
            if let Some(dropped) = self.history.pop_front() {
                self.oldest = dropped.revision;
            }
        }
        Outcome::Ordered { revision }
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
