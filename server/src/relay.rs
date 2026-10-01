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

use casual_doc_transaction::codec::encode_frame;
use casual_doc_transaction::presence::{Accepted, Roster};
use casual_doc_transaction::protocol::{Arrival, ClientId, ClientMessage, Outcome, ServerMessage};

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
    /// Participants whose fan-out write failed while handling an **ordered** chunk.
    ///
    /// They are behind the order and `152` §5.5's resume is how they catch up — which only
    /// happens if the caller does something with this. Presence failures are deliberately *not*
    /// reported: presence is not ordered, so a participant that missed one is not behind
    /// anything.
    pub behind: Vec<ClientId>,
    /// Set when this message was the `Join` that assigned an identity.
    pub joined_as: Option<ClientId>,
    /// Whether the sender asked to leave.
    pub leaving: bool,
}

/// One room, its connected participants, and who is looking where.
#[derive(Debug)]
pub struct Relay<W> {
    room: Room,
    participants: Participants<W>,
    /// **Never journalled** (`152` §2b): presence is overwritten wholesale, never merged, never
    /// replayed. It shares the room's lock so one message is handled at a time, not its
    /// durability.
    roster: Roster,
}

impl<W: Write> Relay<W> {
    /// A relay over `room` with nobody connected.
    #[must_use]
    pub fn new(room: Room) -> Self {
        Self {
            room,
            participants: Participants::new(),
            roster: Roster::new(),
        }
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
            ClientMessage::Join(_) => {
                let answer = self.room.join(message);
                let mut handled = Handled {
                    answer: Some(answer.clone()),
                    ..Handled::default()
                };
                // The participant set is keyed by the id the relay just assigned — the same id a
                // resume hands back, which is why `joined` replaces rather than refuses.
                if let ServerMessage::Welcome { client, .. } | ServerMessage::Resumed { client, .. } =
                    answer
                    && let Some(writer) = writer.take()
                {
                    handled.joined_as = Some(client);
                    self.participants.joined(client, writer);
                }
                Ok(handled)
            }
            ClientMessage::Submit(submission) => {
                // `Room::commit` journals before it answers; this cannot be reordered from here.
                match self.room.commit(submission)? {
                    Outcome::Ordered { revision } => {
                        let bytes = encode_frame(&ServerMessage::Apply(Arrival {
                            revision,
                            client: submission.client,
                            operations: submission.operations.clone(),
                        }));
                        let behind = self.participants.fan_out(submission.client, &bytes);
                        Ok(Handled {
                            answer: Some(ServerMessage::Ack {
                                through: submission.seq,
                                revision,
                            }),
                            behind,
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
                    let _ = self.participants.fan_out(client, &bytes);
                }
                Ok(Handled::default())
            }
            ClientMessage::Leave => Ok(Handled {
                leaving: true,
                ..Handled::default()
            }),
        }
    }

    /// Forgets a connection, however it ended, and tells the others its presence is gone.
    ///
    /// Called on **every** exit path — a clean leave, a broken pipe, a refused frame. A writer
    /// left behind is a socket every future chunk is written to and reported as failed forever;
    /// a caret left behind is worse than no caret, because the reader believes somebody is there
    /// (ONLYOFFICE's `Remove_ForeignCursor` covers the same transition).
    pub fn disconnected(&mut self, client: ClientId) -> Vec<ClientId> {
        self.participants.left(client);
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
