//! Stable node identity and deterministic ID generation.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::ModelError;

/// Stable identity of a logical document node.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NodeId(u128);

impl NodeId {
    /// Creates a non-zero node ID.
    pub fn new(value: u128) -> Result<Self, ModelError> {
        if value == 0 {
            return Err(ModelError::ZeroNodeId);
        }
        Ok(Self(value))
    }

    /// Creates an ID from a namespace and a local counter.
    pub fn from_parts(namespace: u64, counter: u64) -> Result<Self, ModelError> {
        Self::new((u128::from(namespace) << 64) | u128::from(counter))
    }

    /// Returns the raw numeric representation.
    #[must_use]
    pub const fn as_u128(self) -> u128 {
        self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:032x}", self.0)
    }
}

impl FromStr for NodeId {
    type Err = ModelError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() != 32
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(ModelError::InvalidNodeId);
        }

        let parsed = u128::from_str_radix(value, 16).map_err(|_| ModelError::InvalidNodeId)?;
        Self::new(parsed)
    }
}

impl Serialize for NodeId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for NodeId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

/// Deterministic namespace-and-counter ID source.
#[derive(Clone, Debug)]
pub struct IdGenerator {
    namespace: u64,
    next_counter: u64,
}

impl IdGenerator {
    /// Creates an ID generator whose first local counter is one.
    #[must_use]
    pub const fn new(namespace: u64) -> Self {
        Self {
            namespace,
            next_counter: 1,
        }
    }

    /// Returns the next ID or an error if the counter is exhausted.
    pub fn next_id(&mut self) -> Result<NodeId, ModelError> {
        if self.next_counter == u64::MAX {
            return Err(ModelError::IdSpaceExhausted);
        }

        let id = NodeId::from_parts(self.namespace, self.next_counter)?;
        self.next_counter += 1;
        Ok(id)
    }

    /// The namespace this generator currently mints in.
    #[must_use]
    pub const fn namespace(&self) -> u64 {
        self.namespace
    }

    /// The counter the next id will carry.
    #[must_use]
    pub const fn next_counter(&self) -> u64 {
        self.next_counter
    }

    /// Moves future ids into `namespace`, **keeping the counter**.
    ///
    /// Keeping the counter is the whole safety property: a generator that re-entered a
    /// namespace it had already minted in — a participant rejoining a session under the
    /// number it held before — would otherwise reissue ids it had already handed out. The
    /// counter never goes backwards, so re-entry cannot reissue.
    pub const fn rebase(&mut self, namespace: u64) {
        self.namespace = namespace;
    }

    /// Guarantees the next id's counter is above `counter`.
    ///
    /// Callers use this with [`IdSpace`]-scoped knowledge of what a document already holds,
    /// so a generator entering a namespace never lands on an id that is already in the
    /// document — which is what a reopened snapshot, whose node ids are preserved verbatim,
    /// would otherwise cause.
    pub const fn reserve_through(&mut self, counter: u64) {
        let next = counter.saturating_add(1);
        if next > self.next_counter {
            self.next_counter = next;
        }
    }
}

/// The 64-bit namespace half of a [`NodeId`], and the partition that keeps two replicas of
/// one document from minting the same id.
///
/// # The established pattern, named before the code
///
/// This is **participant-prefixed identifiers** — the *site id* of Jupiter/Wave OT and of
/// every CRDT that mints identity (Yjs's `(client, clock)`, Automerge's `(actor, counter)`).
/// A [`NodeId`] is already `(namespace, counter)`, so the partition costs no new field and
/// no new type: give every participant a namespace of its own and a collision is impossible
/// rather than unlikely.
///
/// The alternative prior art is a **minted-range allocator** — a coordinator hands each
/// replica a block of ids to spend. It is rejected here for one reason: a replica with no
/// block cannot mint, so the first keystroke of a local-first document would have to wait
/// for a server. Site-id partitioning needs no round trip, so single-user editing keeps
/// working with no session and no server, and minting stays O(1) per keystroke.
///
/// # The three spaces, and why they are disjoint
///
/// With `base` the document's own space and `K` an odd constant:
///
/// | Space | Value | Who mints in it |
/// | --- | --- | --- |
/// | document | `base` | the importer, once, at open |
/// | local | `base ^ K` | a replica with no session — see [`IdSpace::local`] |
/// | participant `c` | `base ^ (K * (c + 2))` | one session participant — see [`IdSpace::participant`] |
///
/// `K` odd makes `x ↦ K * x` a bijection on `u64`, and xor with a constant is a bijection,
/// so `c ↦ base ^ (K * (c + 2))` is **injective**: two participants cannot share a space.
/// It also cannot equal `base` (that needs `c + 2 == 0`) and cannot equal the local space
/// (that needs `c + 2 == 1`); both of those participant numbers overflow `c + 2` and are
/// refused by [`IdSpace::participant`] rather than left as a remark.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct IdSpace(u64);

/// An odd multiplier, so multiplication by it is a bijection on `u64`.
///
/// The golden-ratio constant, chosen for its oddness and its spread, not for any
/// cryptographic property — this has to be collision-free, not unguessable.
const ODD_MULTIPLIER: u64 = 0x9E37_79B9_7F4A_7C15;

impl IdSpace {
    /// Wraps a raw namespace.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// The raw namespace.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// The space a document's own nodes live in — the high half of its id.
    ///
    /// Every replica of one document computes the same value, which is exactly why it
    /// cannot be a *participant's* space.
    #[must_use]
    pub const fn of_document(document_id: NodeId) -> Self {
        Self((document_id.as_u128() >> 64) as u64)
    }

    /// The space a replica mints in when it is in **no session**.
    ///
    /// Reserved: [`IdSpace::participant`] never returns it, so a document edited offline and
    /// then shared into a session cannot have its ids re-minted by a participant. Every
    /// offline replica of one document uses the same value, which is sound because the
    /// protocol's own precondition is that participants start from one snapshot — an
    /// offline replica's own unshared edits never travel.
    #[must_use]
    pub const fn local(base: Self) -> Self {
        Self(base.0 ^ ODD_MULTIPLIER)
    }

    /// The space participant `number` mints in, given the document's own space.
    ///
    /// Derived, not assigned, so no wire field carries it and a receiver can compute the
    /// sender's space from a message's participant number alone.
    ///
    /// Returns `None` for the two participant numbers that would alias a reserved space:
    /// `u64::MAX` (which would be [`IdSpace::local`]) and `u64::MAX - 1` (which would be
    /// the document's own space).
    #[must_use]
    pub const fn participant(base: Self, number: u64) -> Option<Self> {
        match number.checked_add(2) {
            Some(offset) => Some(Self(base.0 ^ ODD_MULTIPLIER.wrapping_mul(offset))),
            None => None,
        }
    }

    /// Whether `id` was minted in this space.
    #[must_use]
    pub const fn holds(self, id: NodeId) -> bool {
        ((id.as_u128() >> 64) as u64) == self.0
    }
}
