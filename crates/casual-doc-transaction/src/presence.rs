// SPDX-License-Identifier: Apache-2.0

//! Presence — who else is here, and where they are looking. `107` 6.6, doc 152 §2b.
//!
//! # The established pattern, named before the code
//!
//! **Yjs's awareness protocol**, adopted and not invented: one entry per client, **overwritten
//! wholesale**, never merged, never persisted, never replayed. Because nothing merges, nothing
//! transforms — presence needs no `transform`, no inverse and no position in the total order,
//! which is exactly why it is separable from everything the codec blocks.
//!
//! **ONLYOFFICE agrees, and their source was read rather than their documentation.** In
//! `sdkjs/common/docscoapi.js`:
//!
//! - `_onParticipantsChanged` ends with `this._participants = participantsNew;` — the roster is
//!   a **complete list from the server, replaced wholesale**, not a stream of per-client
//!   deltas;
//! - it is guarded by `participantsTimestamp`, and a list whose timestamp is not newer is
//!   **ignored** — staleness is a real failure they hit, not a hypothetical;
//! - `sendCursor` sends `{"type": "cursor", "cursor": <string>}` and **nothing else**, while
//!   the message the receiver gets carries `'user'` and `'useridoriginal'`
//!   (`word/api.js`'s `Update_ForeignCursor`). **The client never states who it is; the server
//!   attaches that.**
//! - `Update_ForeignCursor` reads `e[e.length - 1]` — the **last** message wins, with no
//!   merge;
//! - `Remove_ForeignCursor(e['id'])` fires on a connection-state change, so presence dies with
//!   the connection rather than being retained.
//!
//! Three independent sources — Yjs, ONLYOFFICE, and doc 152's own design — give the same shape,
//! which is the strongest reason to take it.
//!
//! # The identity rule, held by the type system
//!
//! **A presence update has no field a client could put an identity in.** [`PresenceUpdate`] is
//! a payload and a clock, and that is all; the participant number is supplied by whoever
//! received the message, from the session it arrived on. A client that wants to claim to be
//! somebody else has nowhere to write the claim, which is a stronger guarantee than validating
//! a field would be — and `a_presence_update_has_no_field_a_client_could_claim_an_identity_in`
//! fails the build if a field is ever added.
//!
//! # What this is NOT, and why
//!
//! It is not a *typed* cursor. `107` P-4 owes an anchor mapping before a remote caret can
//! survive a concurrent structural edit, so a typed position would promise something the
//! engine cannot yet keep. The payload is therefore **opaque and bounded**: the host fills it,
//! the engine carries it, and when P-4 lands a typed position becomes a payload *shape* rather
//! than a protocol change. ONLYOFFICE's cursor is an opaque string for what is probably the
//! same reason.
//!
//! # Bounds, explicit like `21`'s `HARD_MAX_*`
//!
//! A roster is unbounded input from the network, so both axes are capped and both refusals are
//! typed rather than silent: [`MAX_PRESENCE_BYTES`] per entry and [`MAX_PARTICIPANTS`] per
//! room.
//!
//! # Complexity
//!
//! O(1) per update — one map write. O(participants) to read the roster. **Nothing here touches
//! the document**, so presence cannot cost a keystroke anything (`107` §4 B1).

use std::collections::BTreeMap;

use crate::protocol::ClientId;

/// The most bytes one participant's presence payload may carry.
///
/// A caret, a selection range and a colour fit in a fraction of this. The cap exists because
/// the payload is opaque — the engine cannot judge the content, so it judges the size — and
/// because a roster is fanned out to every participant, so one client's oversized payload is
/// everybody's bandwidth. Rejected with [`PresenceError::TooLarge`], never truncated: a
/// truncated opaque payload is a payload whose meaning changed silently.
pub const MAX_PRESENCE_BYTES: usize = 4 * 1024;

/// The most participants one room's roster will hold.
///
/// Past this a join is refused rather than the roster growing without limit, because the
/// roster is fanned out to everyone: the cost of the *n*-th participant is paid *n* times.
/// Word's own co-authoring tops out well below this, and a document with more readers than
/// this wants a broadcast mode rather than a roster.
pub const MAX_PARTICIPANTS: usize = 128;

/// A participant's own count of the presence updates it has sent.
///
/// Monotonic per client, and the only thing that orders presence. It is **not** a document
/// revision and has nothing to do with the total order: presence is not ordered against
/// edits, because nothing about it merges.
///
/// This exists because of a defect ONLYOFFICE hit and guarded with `participantsTimestamp`: a
/// roster that arrives out of order would otherwise move a caret backwards and leave it there.
/// A clock is used rather than a wall-clock timestamp because the engine does not read a clock
/// (`protocol`'s module docs), and because a counter cannot be skewed between two machines.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PresenceClock(u64);

impl PresenceClock {
    /// Wraps a raw counter.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// The raw counter.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// The next value, or `None` at the ceiling.
    #[must_use]
    pub const fn next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

/// What a client sends to say where it is.
///
/// **There is no identity field, and that is the point** — see the module docs. Adding one
/// would let a client claim to be somebody else, and the guard in this module's tests fails
/// the build if the shape grows one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PresenceUpdate {
    /// The sender's own monotonic count of its presence updates.
    pub clock: PresenceClock,
    /// Opaque, host-defined, bounded by [`MAX_PRESENCE_BYTES`]. The engine carries it and
    /// never interprets it; `107` P-4 is what a typed caret waits for.
    pub payload: Vec<u8>,
}

/// One participant's presence as a receiver holds it.
///
/// The `client` is attached by the receiver from the session the update arrived on, never read
/// from the message.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Presence {
    /// Who this is. **Supplied by the relay, not by the sender.**
    pub client: ClientId,
    /// The clock of the newest update accepted for this participant.
    pub clock: PresenceClock,
    /// The newest payload accepted for this participant.
    pub payload: Vec<u8>,
}

/// Why a presence update was not accepted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PresenceError {
    /// The payload is above [`MAX_PRESENCE_BYTES`].
    TooLarge {
        /// What arrived.
        bytes: usize,
        /// The cap.
        limit: usize,
    },
    /// The roster already holds [`MAX_PARTICIPANTS`] other participants.
    RoomFull {
        /// The cap.
        limit: usize,
    },
}

impl core::fmt::Display for PresenceError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::TooLarge { bytes, limit } => write!(
                formatter,
                "presence payload is {bytes} bytes, above the {limit}-byte limit"
            ),
            Self::RoomFull { limit } => {
                write!(formatter, "the room already holds {limit} participants")
            }
        }
    }
}

impl std::error::Error for PresenceError {}

/// What accepting an update did, so a caller knows whether to repaint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Accepted {
    /// A participant appeared.
    Joined,
    /// A participant's payload changed.
    Moved,
    /// The update was older than, or equal to, what is already held — **ignored**. This is the
    /// case ONLYOFFICE's `participantsTimestamp` guard exists for.
    Stale,
}

/// Everyone in the room, and where they are.
///
/// Wholesale replacement per client, no merge, no transform, no persistence. A [`Roster`] is
/// built at join and dropped when the session ends; **nothing writes it to a log**, which
/// `presence_is_never_written_to_the_revision_log` holds structurally.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Roster {
    /// Ordered by participant number so two replicas list the room identically — a roster
    /// rendered in map-iteration order would reshuffle the reader's sidebar on every update.
    by_client: BTreeMap<u64, Presence>,
}

impl Roster {
    /// An empty roster.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Accepts `update` as `client`'s presence, replacing whatever that client had.
    ///
    /// `client` comes from the session the message arrived on and **not** from the message, so
    /// a sender cannot write somebody else's presence.
    ///
    /// # Errors
    ///
    /// [`PresenceError::TooLarge`] or [`PresenceError::RoomFull`]. Neither changes the roster.
    ///
    /// # Complexity
    ///
    /// O(log participants) — one map write, no document access.
    pub fn accept(
        &mut self,
        client: ClientId,
        update: PresenceUpdate,
    ) -> Result<Accepted, PresenceError> {
        if update.payload.len() > MAX_PRESENCE_BYTES {
            return Err(PresenceError::TooLarge {
                bytes: update.payload.len(),
                limit: MAX_PRESENCE_BYTES,
            });
        }
        let key = client.get();
        match self.by_client.get_mut(&key) {
            Some(held) => {
                // `<=`, not `<`: an update at the clock already held says nothing new, and
                // accepting it would let a replayed message overwrite a newer payload that
                // happened to share a clock with an older one.
                if update.clock <= held.clock {
                    return Ok(Accepted::Stale);
                }
                held.clock = update.clock;
                held.payload = update.payload;
                Ok(Accepted::Moved)
            }
            None => {
                if self.by_client.len() >= MAX_PARTICIPANTS {
                    return Err(PresenceError::RoomFull {
                        limit: MAX_PARTICIPANTS,
                    });
                }
                self.by_client.insert(
                    key,
                    Presence {
                        client,
                        clock: update.clock,
                        payload: update.payload,
                    },
                );
                Ok(Accepted::Joined)
            }
        }
    }

    /// Forgets `client` — it left, or its connection dropped.
    ///
    /// Returns whether anything was held. Presence dies with the connection, as ONLYOFFICE's
    /// `Remove_ForeignCursor` does on a connection-state change: a caret that outlives its
    /// owner is worse than no caret, because the reader believes somebody is there.
    pub fn forget(&mut self, client: ClientId) -> bool {
        self.by_client.remove(&client.get()).is_some()
    }

    /// Everyone held, in participant-number order.
    pub fn participants(&self) -> impl ExactSizeIterator<Item = &Presence> {
        self.by_client.values()
    }

    /// One participant's presence, if held.
    #[must_use]
    pub fn get(&self, client: ClientId) -> Option<&Presence> {
        self.by_client.get(&client.get())
    }

    /// How many participants are held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_client.len()
    }

    /// Whether nobody is held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_client.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update(clock: u64, payload: &str) -> PresenceUpdate {
        PresenceUpdate {
            clock: PresenceClock::new(clock),
            payload: payload.as_bytes().to_vec(),
        }
    }

    #[test]
    fn a_presence_update_has_no_field_a_client_could_claim_an_identity_in() {
        // The identity rule, held by the type system rather than by validation. A client that
        // wants to claim to be somebody else must have somewhere to write the claim; this
        // asserts there is nowhere.
        //
        // Written as a source scan because that is the only way to assert the ABSENCE of a
        // field: a constructor listing every field would still compile if a defaulted one were
        // added, and a round-trip test would not notice. `\r\n` is normalised first, so a
        // checkout with `core.autocrlf` cannot make the match miss.
        let source = include_str!("presence.rs").replace("\r\n", "\n");
        let shape = source
            .split_once("pub struct PresenceUpdate {")
            .and_then(|(_, rest)| rest.split_once("\n}\n"))
            .map(|(body, _)| body.to_owned())
            .expect("the struct is declared in this file");
        for forbidden in [
            "client",
            "ClientId",
            "participant",
            "identity",
            "author",
            "user",
        ] {
            assert!(
                !shape.contains(forbidden),
                "`PresenceUpdate` names `{forbidden}`: a presence message must carry no field a \
                 client could put an identity in, because the participant number comes from the \
                 session the message arrived on. Shape:\n{shape}"
            );
        }
        // And the guard has to be able to see the thing it forbids, or it proves nothing.
        let planted = "pub struct PresenceUpdate {\n    pub client: ClientId,\n}\n";
        assert!(
            planted.contains("ClientId"),
            "the scan cannot see an identity field even when one is planted"
        );
    }

    #[test]
    fn a_later_update_overwrites_wholesale_and_an_earlier_one_is_ignored() {
        // Yjs's rule and ONLYOFFICE's `participantsTimestamp` lesson in one test: the newest
        // wins outright — no merge, so no transform — and an out-of-order message does not move
        // a caret backwards.
        let mut roster = Roster::new();
        let ada = ClientId::new(0);
        assert_eq!(
            roster.accept(ada, update(1, "at-the-top")),
            Ok(Accepted::Joined)
        );
        assert_eq!(
            roster.accept(ada, update(2, "at-the-bottom")),
            Ok(Accepted::Moved)
        );
        assert_eq!(
            roster.get(ada).map(|held| held.payload.as_slice()),
            Some(b"at-the-bottom".as_slice()),
            "the newer payload must replace the older one whole, with nothing merged"
        );

        // The stale cases, both of them: strictly older, and equal.
        assert_eq!(
            roster.accept(ada, update(1, "back-at-the-top")),
            Ok(Accepted::Stale)
        );
        assert_eq!(
            roster.accept(ada, update(2, "a-replay")),
            Ok(Accepted::Stale)
        );
        assert_eq!(
            roster.get(ada).map(|held| held.payload.as_slice()),
            Some(b"at-the-bottom".as_slice()),
            "a stale update changed the payload, so an out-of-order message moves a caret \
             backwards — which is the defect ONLYOFFICE guards with `participantsTimestamp`"
        );
        assert_eq!(roster.len(), 1, "a stale update must not add a participant");
    }

    #[test]
    fn one_participant_cannot_write_another_participants_presence() {
        // The identity rule, now as behaviour rather than as a shape: two clients sending the
        // same payload produce two entries, because the key is the session's participant number
        // and not anything in the message.
        let mut roster = Roster::new();
        let (ada, grace) = (ClientId::new(0), ClientId::new(1));
        assert_eq!(roster.accept(ada, update(1, "same")), Ok(Accepted::Joined));
        assert_eq!(
            roster.accept(grace, update(1, "same")),
            Ok(Accepted::Joined)
        );
        assert_eq!(roster.len(), 2);
        assert_eq!(roster.get(ada).map(|held| held.client), Some(ada));
        assert_eq!(roster.get(grace).map(|held| held.client), Some(grace));
    }

    #[test]
    fn presence_dies_with_the_connection() {
        // ONLYOFFICE's `Remove_ForeignCursor` on a connection-state change. A caret that
        // outlives its owner is worse than no caret: the reader believes somebody is there.
        let mut roster = Roster::new();
        let ada = ClientId::new(7);
        assert_eq!(roster.accept(ada, update(1, "here")), Ok(Accepted::Joined));
        assert!(roster.forget(ada));
        assert!(roster.is_empty());
        assert!(
            !roster.forget(ada),
            "forgetting twice must not report a removal"
        );

        // And rejoining starts clean rather than inheriting the old clock, so a participant
        // whose counter restarted is not permanently stale.
        assert_eq!(roster.accept(ada, update(1, "back")), Ok(Accepted::Joined));
        assert_eq!(
            roster.get(ada).map(|held| held.payload.as_slice()),
            Some(b"back".as_slice())
        );
    }

    #[test]
    fn both_bounds_refuse_rather_than_truncate_or_grow() {
        // Unbounded input from the network, so both axes are capped. A truncated opaque payload
        // is a payload whose meaning changed silently, which is worse than a refusal.
        let mut roster = Roster::new();
        let oversized = PresenceUpdate {
            clock: PresenceClock::new(1),
            payload: vec![0; MAX_PRESENCE_BYTES + 1],
        };
        assert_eq!(
            roster.accept(ClientId::new(0), oversized),
            Err(PresenceError::TooLarge {
                bytes: MAX_PRESENCE_BYTES + 1,
                limit: MAX_PRESENCE_BYTES,
            })
        );
        assert!(
            roster.is_empty(),
            "a refused payload must leave the roster alone"
        );

        // Exactly at the cap is accepted: an off-by-one here would refuse a legitimate payload.
        let at_cap = PresenceUpdate {
            clock: PresenceClock::new(1),
            payload: vec![0; MAX_PRESENCE_BYTES],
        };
        assert_eq!(
            roster.accept(ClientId::new(0), at_cap),
            Ok(Accepted::Joined)
        );

        for number in 1..MAX_PARTICIPANTS as u64 {
            assert_eq!(
                roster.accept(ClientId::new(number), update(1, "x")),
                Ok(Accepted::Joined)
            );
        }
        assert_eq!(roster.len(), MAX_PARTICIPANTS);
        assert_eq!(
            roster.accept(ClientId::new(MAX_PARTICIPANTS as u64), update(1, "x")),
            Err(PresenceError::RoomFull {
                limit: MAX_PARTICIPANTS
            })
        );
        // A participant already in a full room may still move — the cap is on membership, not
        // on movement, and refusing movement would freeze every caret in a busy room.
        assert_eq!(
            roster.accept(ClientId::new(0), update(2, "moved")),
            Ok(Accepted::Moved)
        );
    }

    #[test]
    fn the_roster_reads_in_participant_order_on_every_replica() {
        // Two replicas must list the room identically, or the reader's sidebar reshuffles on
        // every update. Insertion order is NOT the order: these arrive backwards.
        let mut roster = Roster::new();
        for number in [5_u64, 1, 9, 3] {
            assert_eq!(
                roster.accept(ClientId::new(number), update(1, "x")),
                Ok(Accepted::Joined)
            );
        }
        let order: Vec<u64> = roster
            .participants()
            .map(|held| held.client.get())
            .collect();
        assert_eq!(order, vec![1, 3, 5, 9]);
    }

    #[test]
    fn presence_is_never_written_to_the_revision_log() {
        // Doc 152: presence is never persisted and never replayed. Held structurally rather
        // than by intent: the log and the envelope must not name this module at all, so a
        // future change that tried to store presence in a commit fails here.
        //
        // The scan is of the production half only — this test has to name the forbidden strings
        // in order to forbid them, which is the self-reference every source guard hits.
        for (name, source) in [
            ("lib.rs", include_str!("lib.rs")),
            ("session.rs", include_str!("session.rs")),
        ] {
            let production = source.replace("\r\n", "\n");
            let production = production
                .split_once("\n#[cfg(test)]\n")
                .map_or(production.as_str(), |(before, _)| before);
            for forbidden in ["Roster", "PresenceUpdate", "presence::"] {
                assert!(
                    !production.contains(forbidden),
                    "{name} names `{forbidden}`: presence must not reach the revision log or \
                     the session envelope, because it is never persisted and never replayed"
                );
            }
        }
    }
}
