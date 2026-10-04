// SPDX-License-Identifier: Apache-2.0

//! The collaboration wire vocabulary: pure data, no transport, no clock, no I/O.
//!
//! # Why this lives in the engine crate
//!
//! Both ends of a session compile from this module. The browser is this crate through
//! WASM; a relay is a separate workspace member that links it. Nothing here opens a
//! socket, reads a clock or allocates identity — the two facts an engine must not invent
//! are *who you are* and *what time it is*, and both arrive from outside.
//!
//! The sibling engine (`opencalc`, `casual-calc-transaction::protocol`) proves this
//! factoring: no separate client crate, no transport in the core, and a CI rule that
//! nothing under `crates/` may depend on the server. Doc 152 §3 records why we adopt it
//! rather than building a `casual-doc-collab`.
//!
//! # What is deliberately absent
//!
//! Doc 152 §9's list, **kept current rather than left as it was written**. The byte codec
//! (ADR-057), the relay binary, durability (ADR-058) and presence have all landed since, so
//! this paragraph used to say `casual-doc-edit` carries no `serde` dependency — it does, and
//! these types are an encoding now. What is still absent is collaborative undo, and the
//! anchor mapping a typed remote caret waits on (`107` P-4).
//!
//! # The version rule
//!
//! [`PROTOCOL_VERSION`] is checked for **equality** before anything else happens, and a
//! mismatch is [`ServerMessage::Stopped`], never [`ServerMessage::Refused`]: a client that
//! treats a version mismatch as retryable loops for ever.
//!
//! Bump it only when an old and a new peer would read the *same message differently*. An
//! added optional field is not a bump — an old peer skips it and a new peer reading its
//! absence concludes what the sender meant. A new **enum variant** is a hard break, because
//! a tagged enum with an unknown tag does not deserialize at all.

use casual_doc_edit::access::Capabilities;
use serde::{Deserialize, Serialize};

use crate::presence::PresenceUpdate;
use crate::wire::WireOperation;

/// The version both ends must agree on, checked for equality before anything else.
///
/// Bump only when two peers would read one message *differently* (see the module docs).
///
/// - **1** — the first. Participant spaces were `base ^ (K * (client + 1))`.
/// - **2** — participant spaces became `base ^ (K * (client + 2))`, reserving `base ^ K` for
///   a replica minting with **no session** ([`IdSpace::local`](crate::wire::IdSpace::local)).
///   No field changed, which is exactly why this needs a bump: the space is *derived*, so a
///   version-1 peer and a version-2 peer compute different spaces for the same participant
///   number and would refuse each other's every introduction with `ODC-7008` while both
///   believed the message well formed. A silent disagreement about a derived value is the
///   case the equality check exists for.
/// - **3** — presence. [`ClientMessage::Presence`] and [`ServerMessage::Awareness`] /
///   [`ServerMessage::Departed`] are **new enum variants**, which the rule above calls a hard
///   break: a version-2 peer receiving an `Awareness` tag does not deserialize the message at
///   all, so it would drop the frame rather than skip a field. Nothing about edits changed.
/// - **4** — [`Refusal::RoomFull`], the per-room participant ceiling. The **same** rule as
///   version 3 and not a new argument: [`Refusal`] is an enum on the wire, it derives a plain
///   `Deserialize` with no `#[serde(other)]` fallback, and it travels *inside*
///   [`ServerMessage::Stopped`] — so a version-3 peer meeting the `RoomFull` tag fails to
///   decode the whole `Stopped` frame and never learns it was refused at all. It would report
///   `ODC-7007` ("the message could not be read") for a room that is simply full, which is the
///   wrong cause rather than a missing one, and naming the wrong cause is what the
///   `session.*`/`document.*` split in `casual_doc_edit::access` exists to prevent.
///
///   `#[non_exhaustive]` on [`Refusal`] does **not** help here: it constrains Rust callers
///   matching on the enum, not `serde`'s tag resolution, so it buys source compatibility in
///   this workspace and nothing at all on the wire.
///
/// **Not bumped for the host-signed grant** (ADR-060), and the reasoning is the rule above
/// applied rather than waived. [`Join::grant`] and the `capabilities` field on
/// [`ServerMessage::Welcome`] / [`ServerMessage::Resumed`] are *added optional fields*: a
/// version-3 `Join` and a version-4 one with no grant decode to the same value, and a
/// version-3 peer that omits `capabilities` is read as [`Capabilities::viewer`] — the floor,
/// not a widening. No enum variant changed, so no peer meets a tag it cannot decode.
///
/// The case for bumping anyway is that a grantless client is refused from a room that requires
/// one; that refusal is `ODC-7003` with a reason, which is the opposite of the **silent**
/// disagreement version 2 existed for. A loud refusal is not a protocol break.
pub const PROTOCOL_VERSION: u32 = 4;

/// How many of a client's own chunks may be in flight before it stops sending.
///
/// At the bound [`ClientSession::flush`](crate::session::ClientSession::flush) returns
/// `None` and edits keep accumulating in the log instead. **It degrades to stop-and-wait,
/// which is a good thing to degrade to** — that is what an unpipelined client already was.
///
/// The value is the sibling's, adopted rather than derived; the number that matters is
/// measured commit latency under contention, which doc 152 §10 records as owed.
pub const MAX_OUTSTANDING: usize = 32;

/// The payload budget one submission chunk may spend, in bytes of carried content.
///
/// A large paste is split across several flushes rather than sent as one frame a transport
/// would close the connection over — an over-cap frame does not come back refused, it drops
/// the socket, which is the failure that is hardest to read from either end.
///
/// **At least one commit is always admitted**, even when it alone exceeds the budget, or
/// `flush` would spin on a submission it can never make.
///
/// The measure is [`WireOperation::carried_bytes`], a lower bound on any codec's output.
/// The only property this rule needs is that the measure grows with the payload; the codec
/// lane replaces the measure, not the rule.
pub const CHUNK_BUDGET_BYTES: usize = 3 * 1024 * 1024;

/// A participant's identity within one document session.
///
/// **Assigned by the server, never invented by the engine** — the same reason nothing here
/// reads a clock. It is a per-relay, per-document counter and that is all it promises, so a
/// clustered relay must carry its node alongside it; two nodes serving one document would
/// otherwise both believe an edit was theirs.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ClientId(u64);

impl ClientId {
    /// Wraps a server-assigned participant number.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// The numeric participant number.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// The server's ordered position in one document's history.
///
/// **Deliberately not [`RevisionId`](crate::RevisionId).** That type is the *local* log's
/// revision — "monotonic session-local", as its own doc comment says — and the two count
/// different things: a remote chunk of five operations becomes one local commit and one
/// local revision, while the server has ordered five. Conflating them is how a client
/// concludes it is up to date while five operations are still owed to it.
///
/// A client only ever names an absolute `Revision` in [`Base::Revision`], and only from a
/// value the server just handed it. Doc 152 §5.2.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Revision(u64);

impl Revision {
    /// Wraps an ordered position.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// The numeric position.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// The next position, or `None` when the counter is exhausted.
    #[must_use]
    pub const fn next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

/// One client's chunk counter, counted from one.
///
/// `(client, seq)` is the **idempotency key**: a resumed client resends its outstanding
/// chunks with their original sequence numbers and the server answers
/// [`Outcome::Duplicate`] for anything already ordered. Restarting the counter would let a
/// new chunk collide with an old number and be discarded as a duplicate — silently.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Seq(u64);

impl Seq {
    /// Wraps a chunk number.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// The numeric chunk number.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// An opaque disambiguator a client presents to be recognised after a reconnect.
///
/// Generated by the client, once per session, held for the life of that tab. **It is not a
/// credential**: the server honours it only when the identity it was issued to matches, so
/// it reduces from something that authorises to something that merely disambiguates. This
/// increment carries no token, so "the identity it was issued to" is
/// [`Join::identity`] — doc 152 §9 records the host-signed grant as the next increment's
/// work, and §5.5 records why the key alone must never be enough.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ResumeKey(String);

impl ResumeKey {
    /// The longest key a server will hold. A key is a disambiguator, not a payload.
    pub const MAX_LEN: usize = 128;

    /// Wraps a client-generated key, or `None` when it is empty or over
    /// [`ResumeKey::MAX_LEN`].
    #[must_use]
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        (!value.is_empty() && value.len() <= Self::MAX_LEN).then_some(Self(value))
    }

    /// The key's characters.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A host-signed grant, **opaque to every line of this engine** — `152` §10 Q4.
///
/// # Why bytes and not claims
///
/// Because a capability a client can state is a capability a client can forge. `143` §10 says
/// the provider does not trust a client-supplied role label, and `152` §2b made the same rule
/// structural for presence by giving the message no field to put an identity in. This is that
/// rule again: the claims travel *inside* the signed blob, and
/// [`Capabilities`] appears on the wire only in the
/// **server-to-client** direction, where the relay is the one saying it.
///
/// # What the engine does with it: nothing
///
/// It carries it. Verification needs a key and a clock, and this crate holds neither — the same
/// reason [`ClientId`] is assigned rather than minted and nothing here reads a time. ADR-060
/// records the split: the **host** signs, the **boundary** verifies, the **engine** enforces
/// the capabilities that came back. The signature profile (JWT, PASETO, an opaque provider
/// token) stays `143` §16 Q5's open question, and deliberately so: freezing it here would pick
/// a cryptographic dependency for every embedder, including the ones that already have an
/// identity system and a key.
///
/// Bounded like every other network-supplied value here ([`MAX_GRANT_BYTES`]): a token is a
/// credential, not a payload.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct GrantToken(Vec<u8>);

/// The longest grant a relay will read.
///
/// Generous enough for a JWT with a certificate chain and far below a frame bound, because the
/// failure it prevents is not a big token — it is an unbounded one, arriving before anything has
/// been authorised and therefore from nobody in particular.
pub const MAX_GRANT_BYTES: usize = 8 * 1024;

impl GrantToken {
    /// Wraps host-supplied bytes, or `None` when they are empty or over [`MAX_GRANT_BYTES`].
    #[must_use]
    pub fn new(value: impl Into<Vec<u8>>) -> Option<Self> {
        let value = value.into();
        (!value.is_empty() && value.len() <= MAX_GRANT_BYTES).then_some(Self(value))
    }

    /// The bytes, for the verifier and for nobody else.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// An opaque durable identity for the person behind a session.
///
/// Supplied by the host, never minted here. Two tabs belonging to one person are two
/// [`ClientId`]s and one `Identity`, which is why a resume key is scoped to an identity
/// rather than used alone: otherwise anyone holding a valid session could adopt another
/// participant's [`ClientId`] and have that participant's submissions suppressed as
/// duplicates.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Identity(String);

impl Identity {
    /// Wraps a host-supplied opaque identity, or `None` when it is empty.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        (!value.is_empty()).then_some(Self(value))
    }

    /// The identity's characters.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// What a submission was written against.
///
/// **A sender does not need to know the receiver's position.** The first chunk after a join
/// or a resume is the only moment a client knows an absolute answer, so it names one;
/// everything written on top of its own previous chunk says [`Base::Chained`] and lets the
/// server resolve it from the table it already keeps to suppress duplicates.
///
/// Sound because one client's chunks arrive in order on one connection, so chunk *n − 1* is
/// ordered before chunk *n* is read. A `Chained` chunk from a client with nothing accepted
/// is **refused rather than guessed at**: it cannot happen from a correct client, and
/// inventing a base is how divergence starts.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Base {
    /// An absolute ordered position, named only from a value the server just handed out.
    Revision(Revision),
    /// Wherever this client's previous chunk landed.
    Chained,
}

/// One chunk of a client's own operations, offered for ordering.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Submission {
    /// Who wrote it.
    pub client: ClientId,
    /// This client's chunk number, counted from one.
    pub seq: Seq,
    /// What it was written against.
    pub base: Base,
    /// The operations, in application order.
    pub operations: Vec<WireOperation>,
}

/// A client's opening message. **Nothing else is accepted before it.**
#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq)]
pub struct Join {
    /// The version this client speaks, compared for equality with [`PROTOCOL_VERSION`].
    pub protocol: u32,
    /// Who is joining, as the host names them.
    pub identity: Identity,
    /// The host-signed grant, when the room requires one.
    ///
    /// `None` is **not** "no restrictions": it is "I present nothing", and what that gets is
    /// the room's own policy. A room that requires a grant answers [`Refusal::NotAuthorised`].
    /// Defaulted so a peer that predates the field decodes, and so the absence is expressible
    /// rather than inferred from a missing key.
    #[serde(default)]
    pub grant: Option<GrantToken>,
    /// A key from a previous connection, when this is a reconnect.
    pub resume: Option<Resume>,
}

/// What a reconnecting client offers so the server can recognise it.
#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq)]
pub struct Resume {
    /// The key this client presented on its previous join.
    pub key: ResumeKey,
    /// The ordered position it had reached.
    pub revision: Revision,
}

/// Client to server.
#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq)]
pub enum ClientMessage {
    /// The opening message.
    Join(Join),
    /// A chunk of this client's own edits.
    Submit(Submission),
    /// Where this client is looking now. **Carries no identity** — the receiver attaches the
    /// participant number from the session the message arrived on, so a client has nowhere to
    /// claim to be somebody else. See [`presence`](crate::presence).
    ///
    /// Not ordered against edits, never acknowledged, never retried: a presence update that is
    /// lost is corrected by the next one, and a stale one is ignored by its clock.
    Presence(PresenceUpdate),
    /// Leaving deliberately rather than by disconnecting.
    Leave,
}

/// Somebody's ordered operations, to be applied locally.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Arrival {
    /// Where this chunk landed in the order.
    pub revision: Revision,
    /// Who wrote it. The receiver derives the sender's minting space from this, which is
    /// why no id space appears on the wire — see [`IdSpace`](crate::wire::IdSpace).
    pub client: ClientId,
    /// The operations, in application order.
    pub operations: Vec<WireOperation>,
}

/// Server to client.
#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq)]
pub enum ServerMessage {
    /// A first join: the participant's identity and where the document is.
    Welcome {
        /// The version the server speaks.
        protocol: u32,
        /// The identity assigned to this participant.
        client: ClientId,
        /// The ordered position the accompanying snapshot is at.
        revision: Revision,
        /// What this participant may do, as the relay decided it from the grant it verified.
        ///
        /// **This direction only.** A client receives its capabilities and never states them —
        /// the same asymmetry [`ServerMessage::Awareness`] has, and for the same reason: a
        /// forged claim is unexpressible rather than merely rejected.
        ///
        /// What a client may assume is "this is what the relay will let me do", which is enough
        /// to disable a control *with a reason*. It is not the authority: the relay judges every
        /// submission against its own copy, so a client that ignores this still cannot write
        /// above its level.
        #[serde(default)]
        capabilities: Capabilities,
    },
    /// A recognised reconnect, sent **instead of** [`ServerMessage::Welcome`].
    ///
    /// No snapshot, because replacing the document would discard the unacknowledged work
    /// this message exists to preserve. `missed` is carried **inside** this message rather
    /// than following it, so the client cannot resend before rebasing past them.
    Resumed {
        /// The version the server speaks.
        protocol: u32,
        /// **The same identity as before**, so `(client, seq)` still suppresses duplicates.
        client: ClientId,
        /// The position the client is being caught up to.
        revision: Revision,
        /// Everything ordered since the client's own position, in order.
        missed: Vec<Arrival>,
        /// What this participant may do **now** — re-derived from the grant presented on *this*
        /// reconnect, not restored from the previous one.
        ///
        /// That is the direction `143` §10 requires: "losing edit permission leaves the document
        /// readable and preserves unsent local work". A resume that restored the old level would
        /// make reconnecting a way to undo a revocation.
        #[serde(default)]
        capabilities: Capabilities,
    },
    /// Cumulative acknowledgement: every seq up to and including `through` is ordered.
    ///
    /// `revision` is where **that chunk** landed, never where the document has since
    /// reached. Naming a later revision would have the client conclude it holds everything
    /// in between, which it would then never be sent.
    Ack {
        /// The newest chunk number now ordered. Every earlier one is ordered too.
        through: Seq,
        /// Where the acknowledged chunk landed.
        revision: Revision,
    },
    /// Somebody else's ordered operations.
    Apply(Arrival),
    /// One submission, or the session, was refused. The connection survives.
    Refused {
        /// The chunk this is about, or `None` when it is about the session.
        seq: Option<Seq>,
        /// Why.
        reason: Refusal,
    },
    /// Somebody else is looking somewhere — fanned out, with the identity **the relay
    /// attached**.
    ///
    /// This is the one place the participant number and the payload travel together, and the
    /// direction matters: a client sends [`ClientMessage::Presence`], which has no identity
    /// field, and receives this, which does. It is the shape ONLYOFFICE uses — `sendCursor`
    /// sends `{type, cursor}` and the received message carries `'user'` as well — and it is
    /// what makes a forged identity unexpressible rather than merely rejected.
    Awareness {
        /// Whose presence this is.
        client: ClientId,
        /// Where they are looking.
        update: PresenceUpdate,
    },
    /// A participant is gone: forget its presence.
    ///
    /// Presence dies with the connection. A caret that outlives its owner is worse than no
    /// caret, because the reader believes somebody is there — ONLYOFFICE calls
    /// `Remove_ForeignCursor` on exactly this transition.
    Departed {
        /// Who left.
        client: ClientId,
    },
    /// Terminal. The connection does not survive and a retry will not help.
    Stopped {
        /// Why.
        reason: Refusal,
    },
}

/// Why something was refused.
///
/// **Three of these ask the user for opposite things and must not be collapsed.**
/// [`Refusal::Malformed`] means *do not send that again*; [`Refusal::CannotMerge`] means
/// *that one action did not take*; [`Refusal::NotSaving`] means *the document is not being
/// saved, copy your work out*. The sibling engine answered an unparseable message with
/// `CannotMerge` — naming the transform, the one part that was working — and lost a live
/// debugging session to it.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Refusal {
    /// The two ends do not speak the same protocol. Always terminal.
    ProtocolVersion {
        /// What the server speaks.
        server: u32,
        /// What the client said it speaks.
        client: u32,
    },
    /// The session is not authorised. Deliberately undetailed: detail is useful to an
    /// operator in a log and useful to an attacker in a response.
    NotAuthorised,
    /// This participant may read but not write.
    ReadOnlyAccess,
    /// The ordered session cannot persist work. **Copy it out.**
    NotSaving,
    /// The client is behind the retained history and cannot be caught up by replay. It
    /// needs a fresh snapshot, and the work it had not had acknowledged is lost.
    TooFarBehind {
        /// The oldest position replay can still start from.
        oldest: Revision,
        /// Where the document is now.
        current: Revision,
    },
    /// The submission was written against a position the document has moved past. Rebase
    /// it and resubmit with the same `seq`; nothing is lost.
    StaleBase {
        /// Where the document actually is.
        current: Revision,
    },
    /// No transform exists for a pair that met. **That one action did not take.**
    CannotMerge,
    /// An operation introduces an identity the receiver already holds, so applying it would
    /// overwrite a node somebody else minted. See [`IdSpace`](crate::wire::IdSpace).
    IdCollision,
    /// The message could not be read. **Do not send it again.**
    Malformed,
    /// The room already holds as many participants as it admits —
    /// [`MAX_PARTICIPANTS`](crate::presence::MAX_PARTICIPANTS).
    ///
    /// Terminal for *this* connection and deliberately **not** retryable on it: a client that
    /// resent the same `Join` down the same socket would spin. A room's occupancy is a property
    /// of a moment, though, so a later connection may be admitted — and that is a new session
    /// rather than a retry of this one, which is why the distinction lives in
    /// [`Refusal::is_retryable`] rather than in prose a client has to read.
    ///
    /// Separate from [`Refusal::NotAuthorised`] because the two send a reader to different
    /// places: one is about who they are and the other about how busy the room is, and only one
    /// of them is worth waiting out. Checked **after** the grant, so a caller with no grant
    /// cannot use the refusal to measure how full a room it was never admitted to is.
    RoomFull {
        /// How many participants the room admits. Returned rather than left to the client's
        /// own copy of the constant, because the two ends are separately versioned and a
        /// ceiling the client guessed is a ceiling it can report wrongly.
        limit: usize,
    },
}

impl Refusal {
    /// The stable `ODC-7xxx` code for this refusal — the register is `docs/20`.
    ///
    /// Codes are never recycled and their meaning never changes, so this mapping is a
    /// contract and not a formatting choice.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::CannotMerge => "ODC-7001",
            Self::ProtocolVersion { .. } => "ODC-7002",
            Self::NotAuthorised => "ODC-7003",
            Self::ReadOnlyAccess => "ODC-7004",
            Self::NotSaving => "ODC-7005",
            Self::TooFarBehind { .. } => "ODC-7006",
            Self::Malformed => "ODC-7007",
            Self::IdCollision => "ODC-7008",
            Self::StaleBase { .. } => "ODC-7009",
            Self::RoomFull { .. } => "ODC-7010",
        }
    }

    /// Whether a client may ever send the same thing again.
    ///
    /// The distinction the codes exist for: a retryable refusal invites the client to fix
    /// its base and resubmit, and a non-retryable one must not be retried at all — a client
    /// that retries a `Malformed` or a version mismatch loops for ever.
    #[must_use]
    pub const fn is_retryable(self) -> bool {
        matches!(self, Self::StaleBase { .. } | Self::CannotMerge)
    }

    /// Whether receiving this ends the session.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::ProtocolVersion { .. }
                | Self::NotAuthorised
                | Self::NotSaving
                | Self::RoomFull { .. }
        )
    }
}

/// What ordering one submission did, from the server's point of view.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Outcome {
    /// Ordered at `revision`, to be fanned out to everyone else.
    Ordered {
        /// Where it landed.
        revision: Revision,
    },
    /// Already ordered. The answer names where it landed **the first time**, so a resumed
    /// client's resend is idempotent rather than a second copy of the work.
    Duplicate {
        /// Where it landed originally.
        revision: Revision,
    },
    /// Not ordered.
    Refused {
        /// Why.
        reason: Refusal,
    },
}
