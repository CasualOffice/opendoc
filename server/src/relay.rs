// SPDX-License-Identifier: Apache-2.0

//! One room's message handling: decide, journal, answer, fan out.
//!
//! # Why this is in the library and not in the binary
//!
//! It was in the binary first, and that was wrong. The decisions here are the most intricate in
//! this member — which messages are answered, which are fanned out, who is excluded, what a stale
//! presence update does — and in a `main.rs` behind a `TcpStream` none of them can be tested at
//! all. Generic over the writer, every one of them is exercised against a `Vec<u8>`.
//!
//! The binary is now sockets and threads and nothing else, which is the right division: the part
//! that is hard to get right is the part a test can reach.
//!
//! # The order, which is the contract
//!
//! **decide → journal → answer → fan out**, all under one lock held by the caller. `152` and
//! ADR-058 give the reasons separately; together they mean a chunk cannot be acknowledged before
//! it is durable, and two participants cannot be told about the order in two different orders.

use std::io::Write;

use casual_doc_edit::access::refuse_if_not_permitted;

use casual_doc_transaction::codec::encode_frame;
use casual_doc_transaction::presence::{Accepted, MAX_PARTICIPANTS, Roster};
use casual_doc_transaction::protocol::{
    Arrival, ClientId, ClientMessage, Outcome, Refusal, ServerMessage,
};

use crate::access::Access;
use crate::fanout::Participants;
use crate::room::{Room, RoomError};

/// What handling one message produced.
#[derive(Debug, Default)]
pub struct Handled {
    /// The message to send back to the sender, if any.
    ///
    /// `None` for a presence update, deliberately: it is not ordered, never acknowledged and
    /// never retried, so there is nothing truthful to say about one. A lost update is corrected
    /// by the next.
    pub answer: Option<ServerMessage>,
    /// Participants this message's fan-out could not reach, and which the relay has therefore
    /// **removed from the room** — `152` §2c.
    ///
    /// This used to be "participants whose write failed", reported and left connected, which is
    /// not a back-pressure policy: a dead socket stayed in the set and every later chunk was
    /// written to it and reported as failed again, which is the exact harm
    /// [`Relay::disconnected`]'s own doc comment names. They are now evicted, which is a
    /// decision and is written down where decisions go.
    ///
    /// A caller still wants the list — the eviction is the *policy*, the log line is the
    /// *evidence* — but it no longer has to do anything for the room to stay correct.
    pub evicted: Vec<ClientId>,
    /// Set when this message was the `Join` that assigned an identity.
    pub joined_as: Option<ClientId>,
    /// Whether the sender asked to leave.
    pub leaving: bool,
}

/// One room, its connected participants, and who is looking where.
#[derive(Debug)]
pub struct Relay<W> {
    room: Room,
    /// How this room answers "what may this participant do" — ADR-060. **Not durable**: a grant
    /// is re-verified on every join, and a verifier holds a key, which is not a thing to write
    /// into a journal.
    access: Access,
    participants: Participants<W>,
    /// **Never journalled** (`152` §2b): presence is overwritten wholesale, never merged, never
    /// replayed. It shares the room's lock so one message is handled at a time, not its
    /// durability.
    roster: Roster,
}

impl<W: Write> Relay<W> {
    /// A relay over `room` with nobody connected, admitting participants under `access`.
    ///
    /// **`access` has no default, deliberately** (see [`crate::access`]): a permission policy
    /// nobody configured must fail at the call site as a missing argument, not at runtime as a
    /// room where everybody is an owner.
    #[must_use]
    pub fn new(room: Room, access: Access) -> Self {
        Self {
            room,
            access,
            participants: Participants::new(),
            roster: Roster::new(),
        }
    }

    /// This room's access policy.
    pub const fn access(&self) -> &Access {
        &self.access
    }

    /// The room, for a caller that needs to checkpoint it or report on it.
    pub const fn room(&self) -> &Room {
        &self.room
    }

    /// The room, mutably — what a clean shutdown's checkpoint needs.
    pub const fn room_mut(&mut self) -> &mut Room {
        &mut self.room
    }

    /// Who is looking where.
    pub const fn roster(&self) -> &Roster {
        &self.roster
    }

    /// The connected participants.
    pub const fn participants_mut(&mut self) -> &mut Participants<W> {
        &mut self.participants
    }

    /// Handles one message from a connection whose identity is `me` (`None` before its `Join`).
    ///
    /// `writer` is taken on the `Join` and nowhere else: a connection joins the participant set
    /// exactly once, and until its identity is known there is nobody to attribute anything to.
    ///
    /// # Errors
    ///
    /// [`RoomError`] when an ordered chunk could not be made durable. The caller must **not**
    /// acknowledge: see [`Room::commit`].
    pub fn handle(
        &mut self,
        me: Option<ClientId>,
        writer: &mut Option<W>,
        message: &ClientMessage,
    ) -> Result<Handled, RoomError> {
        match message {
            ClientMessage::Join(join) => {
                // The policy, consulted before the order hears about this connection at all. A
                // failure is `NotAuthorised` and **terminal**: a client that retries an
                // unauthorised join loops for ever, which is exactly what
                // `Refusal::is_terminal` already says about this code.
                let granted = match self.access.capabilities(join.grant.as_ref()) {
                    Ok(granted) => granted,
                    Err(_) => {
                        return Ok(Handled {
                            answer: Some(ServerMessage::Stopped {
                                reason: Refusal::NotAuthorised,
                            }),
                            ..Handled::default()
                        });
                    }
                };
                // The room's occupancy ceiling, consulted **after** the grant and **before**
                // the order hears about this connection — `152` §2c.
                //
                // After the grant, because a caller with no grant must not be able to use the
                // refusal to measure how full a room it was never admitted to is; that is the
                // same rule `GrantRefusal` already follows when it reports every failure as one
                // undetailed `NotAuthorised` on the wire.
                //
                // Before `Room::join`, because that call **journals** the admission (ADR-058),
                // and a durable `Record::Admitted` for a participant who was then refused is a
                // lie in the log. The cost of checking first is that the relay cannot yet know
                // whether this join would GROW the set — a resume carries a `ResumeKey` and the
                // session resolves it to a participant number afterwards — so the test is
                // pessimistic: at the ceiling, a reconnect whose predecessor has not yet been
                // reaped is refused too. That is narrow, loud and self-healing (every exit path
                // runs `disconnected`, and the client's next connection is admitted), and the
                // alternative is worse in both available directions: journalling an admission
                // and then refusing it, or exempting a join that merely *claims* `resume`,
                // which is a ceiling any client bypasses by setting one field.
                if self.participants.len() >= MAX_PARTICIPANTS {
                    return Ok(Handled {
                        answer: Some(ServerMessage::Stopped {
                            reason: Refusal::RoomFull {
                                limit: MAX_PARTICIPANTS,
                            },
                        }),
                        ..Handled::default()
                    });
                }
                // `Room::join` journals the admission before it answers, exactly as
                // `Room::commit` does for a chunk, so a participant number cannot outlive the
                // record of it (ADR-058). The `?` is the whole of "a caller that cannot journal
                // must not admit".
                let answer = self.room.join(message, granted)?;
                let mut handled = Handled {
                    answer: Some(answer.clone()),
                    ..Handled::default()
                };
                // The participant set is keyed by the id the relay just assigned — the same id a
                // resume hands back, which is why `joined` replaces rather than refuses.
                if let ServerMessage::Welcome { client, .. }
                | ServerMessage::Resumed { client, .. } = answer
                {
                    // The grant itself is recorded by the session, on every accepted join,
                    // resumed or not — so a resume re-verifies it and reconnecting cannot
                    // restore a right the room revoked (`143` §10). This relay keeps **no second
                    // copy**: one table, durable, and `commit` reads the same one.
                    if let Some(writer) = writer.take() {
                        handled.joined_as = Some(client);
                        self.participants.joined(client, writer);
                    }
                }
                Ok(handled)
            }
            ClientMessage::Submit(submission) => {
                // **A submission may only claim the connection it arrived on.** `152` §2b made a
                // forged identity unexpressible for presence by giving the message no field to
                // put one in; `Submission` has one, because the relay's dedupe table is keyed on
                // it, so here the claim is *checked* instead.
                //
                // Unchecked, this was a real hole and not a theoretical one. Writing as somebody
                // else attributes the work to them, and — worse — it writes their `(client, seq)`
                // entry, so their own next chunk at that seq comes back `Duplicate` and is
                // silently dropped. That is precisely the harm `ResumeKey`'s doc comment names
                // for a stolen resume key, reachable without one.
                //
                // `None` means nothing has joined on this connection yet, which is the same
                // answer for the same reason.
                //
                // **One check here now, not two.** This used to be paired with a
                // `has_assigned` range check, because `ServerSession::commit` could not look at
                // membership — a join was not journalled, so a membership check there refused
                // the relay's own file on recovery. ADR-058's `Record::Admitted` made admission
                // durable, so `commit` holds that line itself, *exactly* rather than as a range,
                // and a third-party relay driving `ServerSession` directly gets it without
                // having to know to ask. What is left here is the one thing no pure state
                // machine can check: which connection the message arrived on.
                if me != Some(submission.client) {
                    return Ok(Handled {
                        answer: Some(ServerMessage::Refused {
                            seq: Some(submission.seq),
                            reason: Refusal::NotAuthorised,
                        }),
                        ..Handled::default()
                    });
                }
                // Then the write line, which is the whole of what a relay holding no document
                // can judge (ADR-047, ADR-060). `None` for the document is not a shortcut: it is
                // the honest argument, and `casual_doc_edit::access` answers it with a provably
                // weaker rule that never refuses what a replica would allow. The finer classes —
                // was that really a comment? — need the document and are enforced by every
                // replica, which the module docs say out loud rather than implying.
                let granted = self.room.session().granted_for(submission.client);
                let offered: Vec<_> = submission
                    .operations
                    .iter()
                    .map(|operation| operation.as_offered().clone())
                    .collect();
                let permitted = granted.is_some_and(|granted| {
                    refuse_if_not_permitted(None, &offered, granted).is_ok()
                });
                if !permitted {
                    return Ok(Handled {
                        answer: Some(ServerMessage::Refused {
                            seq: Some(submission.seq),
                            reason: Refusal::ReadOnlyAccess,
                        }),
                        ..Handled::default()
                    });
                }
                // `Room::commit` journals before it answers; this cannot be reordered from here.
                match self.room.commit(submission)? {
                    Outcome::Ordered { revision } => {
                        let bytes = encode_frame(&ServerMessage::Apply(Arrival {
                            revision,
                            client: submission.client,
                            operations: submission.operations.clone(),
                        }));
                        let unreachable = self.participants.fan_out(submission.client, &bytes);
                        let evicted = self.evict_unreachable(unreachable);
                        Ok(Handled {
                            answer: Some(ServerMessage::Ack {
                                through: submission.seq,
                                revision,
                            }),
                            evicted,
                            ..Handled::default()
                        })
                    }
                    // Acknowledged and deliberately **not** fanned out again: everybody already
                    // has it, and a second copy would be applied twice.
                    Outcome::Duplicate { revision } => Ok(Handled {
                        answer: Some(ServerMessage::Ack {
                            through: submission.seq,
                            revision,
                        }),
                        ..Handled::default()
                    }),
                    Outcome::Refused { reason } => Ok(Handled {
                        answer: Some(ServerMessage::Refused {
                            seq: Some(submission.seq),
                            reason,
                        }),
                        ..Handled::default()
                    }),
                }
            }
            ClientMessage::Presence(update) => {
                // Fanned out with the identity **the relay attaches**: the client's own message
                // carries no identity field and `ServerMessage::Awareness` does, which is what
                // makes a forged identity unexpressible rather than merely rejected (`152` §2b).
                if let Some(client) = me
                    && matches!(
                        self.roster.accept(client, update.clone()),
                        Ok(Accepted::Joined | Accepted::Moved)
                    )
                {
                    // A `Stale` update is dropped rather than fanned: it says nothing new, and
                    // sending it would move a caret backwards on every other screen.
                    let bytes = encode_frame(&ServerMessage::Awareness {
                        client,
                        update: update.clone(),
                    });
                    // Evicted here too, and that is one mechanism rather than two. The policy is
                    // about **reachability**, not about ordering: a participant whose socket
                    // refused a presence frame is not merely behind an order it was never part
                    // of, it is gone, and leaving it in the set is what made a dead writer
                    // outlive its connection. What is still true is the old comment's point —
                    // it is not *behind* anything, so nothing needs catching up; it simply is
                    // not here.
                    let unreachable = self.participants.fan_out(client, &bytes);
                    let evicted = self.evict_unreachable(unreachable);
                    return Ok(Handled {
                        evicted,
                        ..Handled::default()
                    });
                }
                Ok(Handled::default())
            }
            ClientMessage::Leave => Ok(Handled {
                leaving: true,
                ..Handled::default()
            }),
        }
    }

    /// Removes every participant the fan-out could not reach, and tells the rest they are gone.
    ///
    /// # The policy, named before any code — `152` §2c
    ///
    /// This is **back-pressure**, and the textbook has four answers. Three are wrong here:
    ///
    /// 1. **Block** until the slow reader drains. The relay holds one lock across decide,
    ///    journal, answer and fan out, so one unresponsive socket freezes the room for
    ///    everybody — including the author still waiting for its acknowledgement. Head-of-line
    ///    blocking, and [`crate::fanout`] already names it as what a slow participant costs.
    /// 2. **Buffer per participant, unbounded.** Memory exhaustion from the network: a reader
    ///    that never drains is an out-of-memory condition with extra steps.
    /// 3. **Drop the frame and carry on.** Fatal, because the log is *ordered*: a dropped
    ///    `Apply` makes that replica silently divergent, with nothing able to say so. Reported
    ///    to be ONLYOFFICE's answer — attributed rather than source-verified, since
    ///    `reference/` holds only their two client repositories — but the refusal does not rest
    ///    on that: a silently divergent replica is unacceptable whoever else accepts it.
    /// 4. **Evict the participant and let it resume.** `152` §5.5's resume exists precisely to
    ///    catch a participant up from the position it reached; it keeps its [`ClientId`], so
    ///    `(client, seq)` still suppresses duplicates, and if the gap has fallen outside the
    ///    retained window `Room::history_since` already answers with the **announced**
    ///    bounded-offline refusal rather than a silent hole.
    ///
    /// **(4) is the decision.** It is the only one of the four that preserves the ordered log,
    /// and the mechanism it needs is already built.
    ///
    /// # Why the threshold is one failed write and not a tuned count
    ///
    /// Because under this transport a *slow* reader does not fail. The socket buffer absorbs it
    /// and then the blocking `write_all` absorbs it; an `Err` means the connection is broken or
    /// its buffer is gone. So "persistently behind" has no second meaning to count up to here,
    /// and a retry counter would be a knob pretending to be a policy. If the transport ever
    /// becomes non-blocking, *that* is when a watermark becomes a real design — and the
    /// decision to make then is how much to buffer, not whether to evict.
    ///
    /// # Why at the relay rather than in the binary
    ///
    /// The same reason the rest of this module is here: in `main.rs` behind a `TcpStream` the
    /// policy cannot be tested at all, and what the binary did instead was print a line. A log
    /// line is evidence, not a policy.
    ///
    /// Returns who was removed, in no particular order.
    fn evict_unreachable(&mut self, unreachable: Vec<ClientId>) -> Vec<ClientId> {
        let mut evicted = Vec::new();
        let mut pending = unreachable;
        while let Some(client) = pending.pop() {
            // Already gone — nothing to remove and no departure owed. This is also what makes
            // the loop terminate: every pass either removes one member of a finite set or
            // skips, and a client cannot be removed twice.
            if !self.participants.left(client) {
                continue;
            }
            evicted.push(client);
            if self.roster.forget(client) {
                let bytes = encode_frame(&ServerMessage::Departed { client });
                // An announcement that itself fails identifies ANOTHER unreachable participant,
                // which is the same policy's business — so it goes back on the queue rather
                // than being swallowed. Swallowing it is how one dead socket used to hide the
                // next one behind it.
                pending.extend(self.participants.fan_out(client, &bytes));
            }
        }
        evicted
    }

    /// Forgets a connection, however it ended, and tells the others its presence is gone.
    ///
    /// Called on **every** exit path — a clean leave, a broken pipe, a refused frame. A writer
    /// left behind is a socket every future chunk is written to and reported as failed forever;
    /// a caret left behind is worse than no caret, because the reader believes somebody is there
    /// (ONLYOFFICE's `Remove_ForeignCursor` covers the same transition).
    pub fn disconnected(&mut self, client: ClientId) -> Vec<ClientId> {
        self.participants.left(client);
        // **No grant is forgotten here, and that is a consequence of durability rather than an
        // oversight.** It used to be: a grant left behind was a capability nobody held, and the
        // next participant handed this number would inherit it before its own join had been
        // verified. Participant numbers are no longer re-issued — `Record::Admitted` makes
        // `next_client` a durable floor (ADR-058) — so there is no next holder of this number to
        // inherit anything, and the only connection that may submit as it is one whose own join
        // has just re-verified and overwritten the entry.
        if self.roster.forget(client) {
            let bytes = encode_frame(&ServerMessage::Departed { client });
            return self.participants.fan_out(client, &bytes);
        }
        Vec::new()
    }
}

#[cfg(test)]
#[path = "relay_tests.rs"]
mod tests;
