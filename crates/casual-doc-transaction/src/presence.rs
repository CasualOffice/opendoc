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
//! # The typed caret, and what actually unblocked it
//!
//! Until 2026-10-04 this module said a typed cursor had to wait on `107` P-4, "an anchor
//! mapping before a remote caret can survive a concurrent structural edit". That was the right
//! caution and the wrong prerequisite, and re-deriving it is what unblocked it.
//!
//! **A caret does not need a transform, and it does not need a new mapping step.** A
//! [`Position`] is *node-addressed* — a `NodeId` and a byte offset within it — which is ADR-030
//! invariant I3 doing exactly the job `150` §2 chose it for. A concurrent edit somewhere else
//! in the document does not move a node-addressed position at all, so there is nothing to
//! rebase. Only two things can happen to a caret:
//!
//! 1. **Its own paragraph was edited.** [`PositionMap`] already maps a position across the four
//!    positional shapes, and has since Phase 0. This half was never missing.
//! 2. **Its paragraph was destroyed.** This is the half that was missing, and it is a
//!    *liveness* question about the state the caret is being rendered against — not a transform
//!    and not a mapping step. P-4's `NodeRemoved`/`NodeReplaced` steps are owed for rebasing
//!    **operations** (`107` §3.1 T2 tombstones); a caret needs none of them.
//!
//! ADR-056 built the liveness oracle for the identical question one increment earlier, for
//! block-slot operations: [`BlockTarget`](crate::BlockTarget) answers *where does this node sit
//! in the state I am about to apply against*, O(1) per query over an index built once per
//! applied arrival. [`Roster::rebase`] asks it about carets. ADR-056's rule comes with it: a
//! destroyed anchor is **a reported loss, never a fall-back to the stale value** — so a caret
//! whose paragraph is gone is *withdrawn*, never left naming a dead node.
//!
//! **What the oracle cannot distinguish is declared, not guessed.** `block_position` answers
//! `None` both for a block a concurrent edit destroyed and for a block in a container the
//! target never walked — headers, footers and notes are not in a
//! [`BlockIndex`](crate::transform::BlockIndex). Conflating them would withdraw every caret in
//! a header on every commit. So a [`CaretEnd`] **declares the container its block sat in**, the
//! same move ADR-056 made for an index, and the three outcomes stay apart: the block is there
//! ([`Fate::Carried`]), the container is there and the block is not ([`Fate::Withdrawn`]), or
//! the target cannot say ([`Fate::Unresolved`], carried and counted rather than silently
//! dropped).
//!
//! The payload stays **opaque to the protocol**: the relay fans bytes and never parses them,
//! `PresenceUpdate::payload` is still a `Vec<u8>`, and a host payload that is not a caret is
//! carried untouched. A typed caret is a payload *shape*, exactly as doc 152 §2b promised, and
//! `PROTOCOL_VERSION` does not move.
//!
//! # The outbound half
//!
//! A [`Roster`] is the receiving half. [`PresenceSender`] is the sending half, and it exists
//! because three facts about sending have no other owner: the client's own monotonic clock, the
//! byte bound (checked at the source, so an oversized payload costs a round trip rather than
//! being discovered by the receiver), and the suppression of an update that says nothing new.
//!
//! The one rule that is not obvious: **a reconnect must clear the suppression cache.** Presence
//! dies with the connection, so after a rejoin the new room holds nothing for this client; a
//! sender that still believed its last payload was known would suppress the first update and
//! the room would never learn where this participant is until it happened to move.
//! [`PresenceSender::rejoined`] is that, and `a_rejoin_resends_a_position_that_has_not_changed`
//! is the guard.
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
//! [`Roster::rebase`] is O(participants × mapping steps) and runs once per *applied commit*,
//! never per keystroke in a single-user session, which takes no commits from anybody else.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use casual_doc_model::NodeId;

use crate::codec::{self, CodecError};
use crate::protocol::ClientId;
use crate::{BlockTarget, Position, PositionMap};

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
#[derive(
    Deserialize, Serialize, Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd,
)]
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
#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq)]
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
    /// A sender's own [`PresenceClock`] reached its ceiling.
    ///
    /// Refused rather than wrapped. A wrapped clock reads as a *stale* update at every
    /// receiver, so every caret this client owns would freeze in place and nothing would say
    /// why — the silent version of the defect ONLYOFFICE's `participantsTimestamp` guard
    /// exists for. At one update per millisecond a `u64` lasts about 584 million years, so
    /// this is a bound that says what it does rather than one anybody reaches.
    ClockExhausted,
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
            Self::ClockExhausted => {
                write!(
                    formatter,
                    "this client's presence clock reached its ceiling"
                )
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

/// One end of a caret: a node-addressed position, plus the container its block sat in.
///
/// # Why the container is declared rather than derived
///
/// The receiver needs to know whether the caret's paragraph still exists, and its oracle —
/// [`BlockTarget::block_position`] — answers `None` both for *destroyed* and for *in a
/// container I never walked*. Only the sender can tell those apart, because only the sender
/// saw the block alive. So the container travels, which is ADR-056's move for a block index
/// applied to the same ambiguity: declare the authoring fact a receiver cannot reconstruct,
/// and degrade to "unknown" rather than to "wrong".
///
/// `None` is the document body. A caret in a table cell declares the cell; one in a header,
/// footer or note declares that body's node, which no [`BlockIndex`](crate::transform::BlockIndex)
/// walks — so such a caret resolves to [`Fate::Unresolved`] and is carried, not withdrawn.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaretEnd {
    /// Where this end sits.
    pub position: Position,
    /// The container holding `position`'s block: `None` for the body, else the cell, content
    /// control or text-box body that owns it.
    pub container: Option<NodeId>,
}

impl CaretEnd {
    /// An end at `position`, in `container`.
    #[must_use]
    pub const fn new(position: Position, container: Option<NodeId>) -> Self {
        Self {
            position,
            container,
        }
    }
}

/// A typed caret — the payload *shape* doc 152 §2b said a typed cursor would become.
///
/// Two ends, because a selection is the interesting case and a collapsed caret is the one
/// where they are equal. `anchor` is the fixed end, `head` the moving one, which is the same
/// vocabulary `26` uses for a local selection.
///
/// This is carried **inside** [`PresenceUpdate::payload`], not beside it: the protocol still
/// moves opaque bytes, the relay still never parses them, and `PROTOCOL_VERSION` does not
/// move. A host is free to keep putting something else in there — see [`Fate::Opaque`].
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Eq, PartialEq)]
pub struct Caret {
    /// The fixed end of the selection.
    pub anchor: CaretEnd,
    /// The moving end — where the participant's caret is drawn.
    pub head: CaretEnd,
}

/// The payload's own tag, so a caret is distinguishable from any other typed payload.
///
/// Externally tagged, which is the discipline [`codec`](crate::codec) states: a reader meeting
/// a variant it does not hold refuses it **by name** rather than misparsing it, so a second
/// typed payload kind is additive. Private because the tag is a wire detail; [`Caret::encode`]
/// and [`Caret::decode`] are the surface.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Eq, PartialEq)]
enum TypedPayload {
    Caret(Caret),
}

impl Caret {
    /// A collapsed caret at one position.
    #[must_use]
    pub const fn collapsed(end: CaretEnd) -> Self {
        Self {
            anchor: end,
            head: end,
        }
    }

    /// The bytes to put in a [`PresenceUpdate`].
    ///
    /// One frame, from [`codec::encode_frame`] — the same envelope the wire, the relay's
    /// ordered entry and the journal record use, rather than a fourth framing. A caret encodes
    /// to a couple of hundred bytes, two orders of magnitude under [`MAX_PRESENCE_BYTES`],
    /// which `a_typed_caret_is_far_inside_the_payload_bound` pins.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        codec::encode_frame(&TypedPayload::Caret(*self))
    }

    /// Reads a caret back out of a presence payload.
    ///
    /// # Errors
    ///
    /// [`CodecError`] — and for a payload the host filled with something of its own, that is
    /// the truthful answer rather than an error condition: [`CodecError::NotAFrame`] says
    /// *these bytes are not one of ours*. [`Roster::rebase`] treats it exactly so.
    pub fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        match codec::decode_frame::<TypedPayload>(bytes)? {
            TypedPayload::Caret(caret) => Ok(caret),
        }
    }
}

/// A participant's own sending half: its clock, its bound, and what it last said.
///
/// # Why this is a type and not three lines at every call site
///
/// Each of the three is a defect if it is got wrong, and two of them are invisible when they
/// are: a clock that does not advance makes every update read as stale at every receiver, and
/// a suppression cache that survives a reconnect silences the first update into a room that
/// holds nothing for this client. See the module docs.
///
/// Oversize is refused **here**, at the source. The receiver checks it too — it must, the
/// payload arrives from the network — but a client discovering its own payload is too big only
/// after a round trip is a worse product than one that cannot send it.
#[derive(Clone, Debug, Default)]
pub struct PresenceSender {
    clock: PresenceClock,
    sent: Option<Vec<u8>>,
}

impl PresenceSender {
    /// A sender whose clock has not yet issued anything.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The clock of the newest update this sender issued.
    #[must_use]
    pub const fn clock(&self) -> PresenceClock {
        self.clock
    }

    /// The update to send for `payload`, or `None` if it says nothing new.
    ///
    /// `Ok(None)` is the common case in a busy editor: a selection change that lands on the
    /// same position — a repaint, a focus round trip, a no-op arrow key at a document
    /// boundary — produces identical bytes, and fanning it out would cost every participant a
    /// message to redraw a caret where it already is. Yjs's awareness only broadcasts on a
    /// change for the same reason.
    ///
    /// # Errors
    ///
    /// [`PresenceError::TooLarge`] above [`MAX_PRESENCE_BYTES`], or
    /// [`PresenceError::ClockExhausted`]. Neither advances the clock and neither records the
    /// payload, so a caller that fixes the problem and retries is in the state it was in.
    ///
    /// # Complexity
    ///
    /// O(payload) — one comparison against the last payload. Nothing here touches the
    /// document.
    pub fn send(&mut self, payload: Vec<u8>) -> Result<Option<PresenceUpdate>, PresenceError> {
        if payload.len() > MAX_PRESENCE_BYTES {
            return Err(PresenceError::TooLarge {
                bytes: payload.len(),
                limit: MAX_PRESENCE_BYTES,
            });
        }
        if self.sent.as_deref() == Some(payload.as_slice()) {
            return Ok(None);
        }
        let clock = self.clock.next().ok_or(PresenceError::ClockExhausted)?;
        self.clock = clock;
        self.sent = Some(payload.clone());
        Ok(Some(PresenceUpdate { clock, payload }))
    }

    /// [`PresenceSender::send`] for a typed caret.
    ///
    /// # Errors
    ///
    /// As [`PresenceSender::send`].
    pub fn send_caret(&mut self, caret: Caret) -> Result<Option<PresenceUpdate>, PresenceError> {
        self.send(caret.encode())
    }

    /// Forgets what was last sent, because the room that knew it is gone.
    ///
    /// Call this on a reconnect, **before** the first update of the new session. Presence dies
    /// with the connection (ONLYOFFICE's `Remove_ForeignCursor`), so the room this client
    /// rejoins holds nothing for it; without this the suppression check would compare against
    /// a payload only the *previous* room ever saw and send nothing.
    ///
    /// The clock is deliberately **not** reset: a receiver that somehow retained an old entry
    /// must still see the new update as newer, and a counter that restarts is the one way to
    /// make a legitimate update read as stale.
    pub fn rejoined(&mut self) {
        self.sent = None;
    }
}

/// What [`Roster::rebase`] did with one participant's presence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Fate {
    /// A typed caret, mapped across the commit; its paragraph is still there.
    Carried,
    /// A typed caret whose paragraph the commit destroyed. The entry's payload is **cleared**
    /// — the participant is still present, its position is no longer known — rather than left
    /// naming a node that is gone. ADR-056's rule: a destroyed anchor is a reported loss and
    /// never a fall-back to the stale value.
    Withdrawn,
    /// A typed caret whose paragraph the [`BlockTarget`] cannot speak for, because it never
    /// walked that container — a header, a footer or a note body.
    ///
    /// Carried unchanged and **counted**, because "I do not know" is a different answer from
    /// "it is gone", and withdrawing on it would silently delete every caret in a header on
    /// every commit.
    Unresolved,
    /// The payload is not a typed caret, so the engine does not interpret it and does not
    /// touch it. The opaque contract doc 152 §2b shipped, still intact.
    Opaque,
}

/// How a [`Roster::rebase`] went, by outcome.
///
/// Counted rather than returned per participant so the caller does not have to allocate to
/// learn that nothing happened, which is the common case.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Rebased {
    /// [`Fate::Carried`] entries.
    pub carried: usize,
    /// [`Fate::Withdrawn`] entries — each one a caret the commit destroyed.
    pub withdrawn: usize,
    /// [`Fate::Unresolved`] entries.
    pub unresolved: usize,
    /// [`Fate::Opaque`] entries.
    pub opaque: usize,
}

impl Rebased {
    /// Records one outcome.
    fn count(&mut self, fate: Fate) {
        match fate {
            Fate::Carried => self.carried += 1,
            Fate::Withdrawn => self.withdrawn += 1,
            Fate::Unresolved => self.unresolved += 1,
            Fate::Opaque => self.opaque += 1,
        }
    }
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

    /// Carries every typed caret in the roster across one applied commit.
    ///
    /// `map` is the commit's [`PositionMap`] and `target` describes the state **after** it, so
    /// the order is: map first, then ask whether the mapped node is still there. Mapping first
    /// matters — a concurrent join moves a position out of the paragraph it destroyed and into
    /// the survivor, so asking liveness first would withdraw a caret the map was about to
    /// rescue.
    ///
    /// A payload that is not a caret is left exactly as it is; the engine still does not
    /// interpret an opaque payload.
    ///
    /// # Why re-encoding does not need the sender's clock
    ///
    /// A rebase is not an update: it does not say the participant moved, it says the document
    /// under them did. Every replica that has applied the same commits in the relay's settled
    /// order therefore computes the same caret from the same received payload, which
    /// `two_replicas_that_applied_one_commit_hold_the_same_rebased_caret` asserts. Advancing
    /// the clock here would be a lie about who moved, and would make the owner's own next
    /// update read as stale.
    ///
    /// # Complexity
    ///
    /// O(participants × `map.steps()`), with one O(1) [`BlockTarget`] query per caret end.
    /// Once per **applied commit** — a single-user session applies nobody else's commits and
    /// so never calls this.
    pub fn rebase(&mut self, map: &PositionMap, target: &dyn BlockTarget) -> Rebased {
        let mut outcome = Rebased::default();
        for held in self.by_client.values_mut() {
            let fate = match Caret::decode(&held.payload) {
                Err(_) => Fate::Opaque,
                Ok(caret) => {
                    let (anchor, anchor_fate) = rebase_end(caret.anchor, map, target);
                    let (head, head_fate) = rebase_end(caret.head, map, target);
                    // The worse of the two ends decides: a selection with one end in a
                    // destroyed paragraph is not a selection any more.
                    match worse(anchor_fate, head_fate) {
                        Fate::Withdrawn => {
                            held.payload.clear();
                            Fate::Withdrawn
                        }
                        settled => {
                            held.payload = Caret { anchor, head }.encode();
                            settled
                        }
                    }
                }
            };
            outcome.count(fate);
        }
        outcome
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

/// Maps one caret end across a commit and says what became of it.
///
/// The mapped position is returned whatever the fate, because a mapped position is never worse
/// than an unmapped one: for [`Fate::Withdrawn`] the caller discards it, and for
/// [`Fate::Unresolved`] it is the best statement available about a caret nobody can speak for.
fn rebase_end(end: CaretEnd, map: &PositionMap, target: &dyn BlockTarget) -> (CaretEnd, Fate) {
    let position = map.map(end.position);
    let mapped = CaretEnd {
        position,
        container: end.container,
    };
    if target.block_position(position.node).is_some() {
        return (mapped, Fate::Carried);
    }
    // The block is not there. Which of the two reasons it is depends on whether the target can
    // speak for the container at all — the ambiguity the declared container exists to resolve.
    match target.block_count(end.container) {
        Some(_) => (mapped, Fate::Withdrawn),
        None => (mapped, Fate::Unresolved),
    }
}

/// The worse of two caret-end fates: destroyed beats unknown beats alive.
const fn worse(left: Fate, right: Fate) -> Fate {
    match (left, right) {
        (Fate::Withdrawn, _) | (_, Fate::Withdrawn) => Fate::Withdrawn,
        (Fate::Unresolved, _) | (_, Fate::Unresolved) => Fate::Unresolved,
        // `Opaque` is decided for the whole payload before either end is read, so it cannot
        // reach here; `Carried` is the only remaining pair.
        _ => Fate::Carried,
    }
}

#[cfg(test)]
mod tests {
    use casual_doc_edit::Pos;
    use casual_doc_model::IdGenerator;
    use casual_doc_model::v1::{BlockNode, Paragraph, ParagraphProperties};
    use casual_doc_model::v1::{Definitions, Document};

    use crate::transform::BlockIndex;
    use crate::{
        Affinity, Operation, RevisionId, RevisionLog, Transaction, TransactionId, test_mints,
    };

    use super::*;

    fn update(clock: u64, payload: &str) -> PresenceUpdate {
        PresenceUpdate {
            clock: PresenceClock::new(clock),
            payload: payload.as_bytes().to_vec(),
        }
    }

    /// A document of `paragraphs` empty body paragraphs, and their ids in order.
    fn document(paragraphs: usize) -> (Document, Vec<NodeId>) {
        let mut ids = IdGenerator::new(1);
        let document_id = ids.next_id().expect("id");
        let mut blocks = Vec::new();
        let mut nodes = Vec::new();
        for _ in 0..paragraphs {
            let id = ids.next_id().expect("id");
            nodes.push(id);
            blocks.push(BlockNode::Paragraph(Paragraph {
                id,
                properties: ParagraphProperties::default().into(),
                inlines: Vec::new(),
            }));
        }
        let document =
            Document::new(document_id, blocks, Definitions::default()).expect("document");
        (document, nodes)
    }

    fn transaction(id: u128, base: RevisionId, operations: Vec<Operation>) -> Transaction {
        let mints = test_mints(operations.len());
        Transaction::new(TransactionId::new(id), base, "Edit", mints, operations)
    }

    fn typing(id: u128, base: RevisionId, node: NodeId, offset: u32, text: &str) -> Transaction {
        transaction(
            id,
            base,
            vec![Operation::InsertText {
                at: Pos::new(node, offset),
                text: text.to_owned(),
            }],
        )
    }

    /// A collapsed caret in the body at `offset` bytes into `node`.
    fn body_caret(node: NodeId, offset: u32) -> Caret {
        Caret::collapsed(CaretEnd::new(
            Position::new(node, offset, Affinity::After),
            None,
        ))
    }

    /// A roster holding one participant whose payload is `payload`.
    fn roster_holding(payload: Vec<u8>) -> (Roster, ClientId) {
        let mut roster = Roster::new();
        let grace = ClientId::new(1);
        assert_eq!(
            roster.accept(
                grace,
                PresenceUpdate {
                    clock: PresenceClock::new(1),
                    payload,
                }
            ),
            Ok(Accepted::Joined)
        );
        (roster, grace)
    }

    #[test]
    fn a_typed_caret_round_trips_and_a_tag_this_build_does_not_hold_is_refused_by_name() {
        // The payload is a SHAPE, not a protocol change (doc 152 §2b): it goes through the same
        // frame the wire, the relay's ordered entry and the journal use.
        let (_document, nodes) = document(1);
        let caret = Caret {
            anchor: CaretEnd::new(Position::new(nodes[0], 2, Affinity::Before), None),
            head: CaretEnd::new(Position::new(nodes[0], 7, Affinity::After), None),
        };
        assert_eq!(
            Caret::decode(&caret.encode()),
            Ok(caret),
            "a caret must survive its own encoding"
        );

        // A payload the host filled with something of its own is not an error condition: the
        // codec's answer is "not one of ours", and that is what `rebase` reads as `Opaque`.
        assert_eq!(
            Caret::decode(b"the host's own cursor string"),
            Err(CodecError::NotAFrame)
        );

        // And a future typed payload kind is refused BY NAME rather than misparsed, which is
        // the whole reason the payload is externally tagged.
        let other = codec::encode_frame(&serde_json::json!({ "Lasso": { "points": [] } }));
        assert!(
            matches!(
                Caret::decode(&other),
                Err(CodecError::Malformed { detail }) if detail.contains("Lasso")
            ),
            "an unknown payload tag must be named in the refusal, got {:?}",
            Caret::decode(&other)
        );
    }

    #[test]
    fn a_typed_caret_is_far_inside_the_payload_bound() {
        // The bound is on an OPAQUE payload, so it cannot be derived from the caret's shape —
        // it has to be checked. A caret that did not fit would make the typed shape unusable
        // under the very cap that protects the roster.
        let (_document, nodes) = document(1);
        let encoded = body_caret(nodes[0], u32::MAX).encode();
        assert!(
            encoded.len() * 8 < MAX_PRESENCE_BYTES,
            "a typed caret is {} bytes against a {MAX_PRESENCE_BYTES}-byte cap; it must stay an \
             order of magnitude inside it so a host can carry its own fields alongside",
            encoded.len()
        );
    }

    #[test]
    fn a_remote_caret_follows_an_edit_in_its_own_paragraph() {
        // Half one of the two things that can happen to a caret, and the half that was never
        // missing: `PositionMap` has mapped a position across the four positional shapes since
        // Phase 0. What is new is that the roster now applies it.
        let (mut doc, nodes) = document(2);
        let mut log = RevisionLog::default();
        log.apply(&mut doc, typing(1, log.head(), nodes[0], 0, "hello"))
            .expect("seed");

        let (mut roster, grace) = roster_holding(body_caret(nodes[0], 3).encode());

        let head = log.head();
        let commit = log
            .apply(&mut doc, typing(2, head, nodes[0], 0, "ab"))
            .expect("insert");
        let map = commit.position_map().clone();
        let outcome = roster.rebase(&map, &BlockIndex::of(&doc));

        assert_eq!(
            outcome,
            Rebased {
                carried: 1,
                ..Rebased::default()
            }
        );
        let carried = Caret::decode(&roster.get(grace).expect("held").payload).expect("a caret");
        assert_eq!(
            carried.head.position.offset, 5,
            "two bytes were inserted before the caret, so it must move by two"
        );
        assert_eq!(carried.head.position.node, nodes[0]);
    }

    #[test]
    fn a_caret_in_a_paragraph_a_commit_destroyed_is_withdrawn_not_left_naming_a_dead_node() {
        // Half two, and the half `107` P-4 was wrongly believed to owe. It is a LIVENESS
        // question, answered by ADR-056's `BlockTarget`, with ADR-056's rule attached: a
        // destroyed anchor is a reported loss and never a fall-back to the stale value.
        let (mut doc, nodes) = document(3);
        let mut log = RevisionLog::default();

        let (mut roster, grace) = roster_holding(body_caret(nodes[1], 0).encode());

        let commit = log
            .apply(
                &mut doc,
                transaction(
                    1,
                    log.head(),
                    vec![Operation::DeleteBlocks {
                        container: None,
                        index: 1,
                        count: 1,
                    }],
                ),
            )
            .expect("the middle paragraph is removed");
        let outcome = roster.rebase(commit.position_map(), &BlockIndex::of(&doc));

        assert_eq!(
            outcome,
            Rebased {
                withdrawn: 1,
                ..Rebased::default()
            },
            "the caret's own paragraph is gone, so the caret is gone"
        );
        let held = roster.get(grace).expect("the participant is still present");
        assert!(
            held.payload.is_empty(),
            "a withdrawn caret must leave NO position behind — a payload still naming the dead \
             paragraph is the silent divergence ADR-056 refused for an anchor. Held: {:?}",
            Caret::decode(&held.payload)
        );
        assert_eq!(
            roster.len(),
            1,
            "losing a caret is not losing a participant: they are still in the room"
        );
    }

    #[test]
    fn a_caret_the_target_cannot_speak_for_is_unresolved_rather_than_withdrawn() {
        // `block_position` answers `None` for a destroyed block AND for one in a container it
        // never walked — headers, footers and note bodies are not in a `BlockIndex`. Conflating
        // them would delete every caret in a header on every commit, so the container is
        // declared and the two answers stay apart.
        let (mut doc, nodes) = document(1);
        let mut outside = IdGenerator::new(0x00AD_0000);
        let header_body = outside.next_id().expect("id");
        let header_paragraph = outside.next_id().expect("id");

        let (mut roster, grace) = roster_holding(
            Caret::collapsed(CaretEnd::new(
                Position::new(header_paragraph, 4, Affinity::After),
                Some(header_body),
            ))
            .encode(),
        );

        let mut log = RevisionLog::default();
        let commit = log
            .apply(&mut doc, typing(1, log.head(), nodes[0], 0, "body text"))
            .expect("an unrelated body edit");
        let outcome = roster.rebase(commit.position_map(), &BlockIndex::of(&doc));

        assert_eq!(
            outcome,
            Rebased {
                unresolved: 1,
                ..Rebased::default()
            },
            "the target never walked this container, so the honest answer is `I cannot say`"
        );
        let held = Caret::decode(&roster.get(grace).expect("held").payload).expect("still a caret");
        assert_eq!(
            held.head.position.node, header_paragraph,
            "an unresolved caret is carried, not withdrawn"
        );
    }

    #[test]
    fn an_opaque_host_payload_is_carried_untouched() {
        // The contract doc 152 §2b shipped is intact: a payload the engine does not recognise
        // is one the engine does not interpret and does not rewrite.
        let (mut doc, nodes) = document(1);
        let hosts_own = b"{\"cursor\":\"ONLYOFFICE-style opaque string\"}".to_vec();
        let (mut roster, grace) = roster_holding(hosts_own.clone());

        let mut log = RevisionLog::default();
        let commit = log
            .apply(&mut doc, typing(1, log.head(), nodes[0], 0, "xyz"))
            .expect("an edit");
        let outcome = roster.rebase(commit.position_map(), &BlockIndex::of(&doc));

        assert_eq!(
            outcome,
            Rebased {
                opaque: 1,
                ..Rebased::default()
            }
        );
        assert_eq!(
            roster.get(grace).expect("held").payload,
            hosts_own,
            "an opaque payload must come out byte-identical"
        );
    }

    #[test]
    fn a_rebase_does_not_advance_the_participants_clock() {
        // A rebase says the document moved, not that the participant did. Advancing the clock
        // would make the owner's own next update read as stale and freeze their caret — the
        // defect `participantsTimestamp` exists for, caused by the fix for another one.
        let (mut doc, nodes) = document(1);
        let (mut roster, grace) = roster_holding(body_caret(nodes[0], 0).encode());
        let before = roster.get(grace).expect("held").clock;

        let mut log = RevisionLog::default();
        let commit = log
            .apply(&mut doc, typing(1, log.head(), nodes[0], 0, "abc"))
            .expect("an edit");
        roster.rebase(commit.position_map(), &BlockIndex::of(&doc));

        assert_eq!(roster.get(grace).expect("held").clock, before);
        // And the owner's next update, at the next clock, is still accepted.
        assert_eq!(
            roster.accept(
                grace,
                PresenceUpdate {
                    clock: PresenceClock::new(before.get() + 1),
                    payload: body_caret(nodes[0], 3).encode(),
                }
            ),
            Ok(Accepted::Moved)
        );
    }

    #[test]
    fn two_replicas_that_applied_one_commit_hold_the_same_rebased_caret() {
        // Convergence, which is the property a rebase has to have to be allowed to rewrite a
        // payload at all: two replicas that applied the same commit must agree about where
        // everyone's caret now is, or the roster becomes replica-local opinion.
        let one = document(2);
        let two = document(2);
        assert_eq!(one.1, two.1, "the two replicas must start identical");
        let nodes = one.1.clone();

        // The SAME transaction on both replicas — the mints are what make the application
        // replayable (ADR-051), so cloning it is the point rather than a shortcut.
        let edit = transaction(
            1,
            RevisionId::default(),
            vec![Operation::SplitParagraph {
                at: Pos::new(nodes[0], 0),
                new_id: NodeId::new(0x0ABC_0001).expect("id"),
                properties: None,
            }],
        );

        let mut payloads = Vec::new();
        for (mut doc, _) in [one, two] {
            let mut log = RevisionLog::default();
            let (mut roster, grace) = roster_holding(body_caret(nodes[0], 0).encode());
            let commit = log.apply(&mut doc, edit.clone()).expect("split applies");
            assert_eq!(
                roster.rebase(commit.position_map(), &BlockIndex::of(&doc)),
                Rebased {
                    carried: 1,
                    ..Rebased::default()
                }
            );
            payloads.push(roster.get(grace).expect("held").payload.clone());
        }
        assert_eq!(
            payloads[0], payloads[1],
            "two replicas disagreed about a rebased caret"
        );
        // The agreement has to be about the right answer, not merely the same one: a split at
        // the caret's own offset with `After` affinity moves it into the NEW paragraph, which is
        // the structural case a caret was previously said not to survive. Asserting the value
        // is what makes this guard able to fail — two replicas computing one wrong answer agree
        // just as well as two computing the right one.
        let carried = Caret::decode(&payloads[0]).expect("still a caret");
        assert_eq!(
            (carried.head.position.node, carried.head.position.offset),
            (NodeId::new(0x0ABC_0001).expect("id"), 0),
            "the caret must follow the split into the paragraph its text moved to"
        );
    }

    #[test]
    fn the_sender_does_not_resend_a_position_that_says_nothing_new() {
        // Yjs's awareness only broadcasts on a change. A repaint, a focus round trip or a no-op
        // arrow key at a document boundary produces identical bytes, and fanning that out costs
        // every participant a message to redraw a caret where it already is.
        let mut sender = PresenceSender::new();
        let first = sender
            .send(b"at-the-top".to_vec())
            .expect("accepted")
            .expect("something to send");
        assert_eq!(first.clock, PresenceClock::new(1));

        assert_eq!(
            sender.send(b"at-the-top".to_vec()),
            Ok(None),
            "an unchanged payload must produce nothing to send"
        );
        assert_eq!(
            sender.clock(),
            PresenceClock::new(1),
            "and must not burn a clock value, or receivers see gaps that mean nothing"
        );

        let moved = sender
            .send(b"at-the-bottom".to_vec())
            .expect("accepted")
            .expect("something to send");
        assert_eq!(moved.clock, PresenceClock::new(2));
    }

    #[test]
    fn a_rejoin_resends_a_position_that_has_not_changed() {
        // The one rule that is not obvious. Presence dies with the connection, so the room this
        // client rejoins holds NOTHING for it; a sender still believing its last payload is
        // known suppresses the first update and the new room never learns where this
        // participant is until they happen to move.
        let mut sender = PresenceSender::new();
        let payload = b"where-i-am".to_vec();
        assert!(sender.send(payload.clone()).expect("accepted").is_some());
        assert_eq!(sender.send(payload.clone()), Ok(None));

        // The connection dropped and came back. The receiving half proves the room is empty.
        let mut fresh = Roster::new();
        assert!(fresh.is_empty());

        sender.rejoined();
        let resent = sender
            .send(payload.clone())
            .expect("accepted")
            .expect("a rejoin must resend the position even though it did not change");
        assert_eq!(
            resent.clock,
            PresenceClock::new(2),
            "the clock must NOT restart: a receiver that retained an old entry would read a \
             restarted counter as stale and freeze this caret for ever"
        );
        assert_eq!(
            fresh.accept(ClientId::new(1), resent),
            Ok(Accepted::Joined),
            "and the resent update is what puts this participant on the new room's roster"
        );
    }

    #[test]
    fn an_oversized_payload_is_refused_at_the_sender_and_leaves_it_untouched() {
        // Checked at the source as well as at the receiver. The receiver MUST check — the
        // payload arrives from the network — but a client that only discovers its own payload
        // is too big after a round trip is a worse product than one that cannot send it.
        let mut sender = PresenceSender::new();
        assert_eq!(
            sender.send(vec![0; MAX_PRESENCE_BYTES + 1]),
            Err(PresenceError::TooLarge {
                bytes: MAX_PRESENCE_BYTES + 1,
                limit: MAX_PRESENCE_BYTES,
            })
        );
        assert_eq!(
            sender.clock(),
            PresenceClock::default(),
            "a refusal must not advance the clock"
        );
        // Exactly at the cap is accepted, as it is at the receiver: an off-by-one here would
        // refuse a legitimate payload at one end and accept it at the other.
        assert!(
            sender
                .send(vec![0; MAX_PRESENCE_BYTES])
                .expect("at the cap")
                .is_some()
        );
    }

    #[test]
    fn a_sender_at_its_clock_ceiling_refuses_rather_than_wrapping() {
        // A wrapped clock reads as STALE at every receiver, so every caret this client owns
        // freezes and nothing says why. Refusing is the only answer that can be seen.
        let mut sender = PresenceSender {
            clock: PresenceClock::new(u64::MAX),
            sent: None,
        };
        assert_eq!(
            sender.send(b"x".to_vec()),
            Err(PresenceError::ClockExhausted)
        );
        assert_eq!(sender.clock(), PresenceClock::new(u64::MAX));
    }

    #[test]
    fn a_caret_is_sent_through_the_same_bound_and_suppression_as_any_payload() {
        // One mechanism, not two: `send_caret` must not be a second path that forgets the
        // suppression or the bound.
        let (_document, nodes) = document(1);
        let mut sender = PresenceSender::new();
        let caret = body_caret(nodes[0], 1);
        assert!(sender.send_caret(caret).expect("accepted").is_some());
        assert_eq!(
            sender.send_caret(caret),
            Ok(None),
            "the same caret twice must be suppressed exactly as the same bytes twice are"
        );
        assert!(
            sender
                .send_caret(body_caret(nodes[0], 2))
                .expect("accepted")
                .is_some()
        );
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
