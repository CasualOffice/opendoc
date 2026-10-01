// SPDX-License-Identifier: Apache-2.0

//! The identity space one operation mints in — doc 150 §9.3.
//!
//! # The defect this closes
//!
//! [`apply`](crate::apply) does not only move bytes around: some operations **create
//! nodes**. A `FormatText` whose range starts in the middle of a run has to split that run,
//! and the tail half is a new [`Run`](casual_doc_model::v1::Run) with an id of its own. So
//! did `DeleteText`, `SplitParagraph`, `CreateBookmark`, `InsertField` and five more.
//!
//! Until this module existed those ids came from the *applier's* generator. Two replicas
//! applying the same operation therefore produced the same document with **different names
//! for the same run**: replica A's tail was `(space-A, 7)` and replica B's was
//! `(space-B, 3)`. Nothing broke at once, because no operation in the set addresses a run —
//! but the deterministic snapshot (doc 25) was then not byte-identical across replicas,
//! which defeats *"a snapshot can be verified rather than trusted"* the moment
//! collaboration is persisted, and it is the reason doc 152 §9 refused to freeze a byte
//! codec while this was open.
//!
//! # The established pattern, named before the code
//!
//! **Deterministic identity derived from the operation, not from the applier** — Yjs's
//! `(client, clock)` and Automerge's `(actor, counter)`, where an element created by an
//! operation takes its identity from the operation that created it rather than from a
//! counter the receiver holds. `casual_doc_model::IdSpace` already partitions the
//! *namespace* half of a [`NodeId`] per participant; this partitions the *counter* half per
//! operation, and the two compose: an id minted here is still in its author's `IdSpace`, so
//! `WireOperation::localise`'s existing space check covers it unchanged.
//!
//! The rejected alternative is the one doc 150 §9.3 first reached for: have the operation
//! **enumerate** the ids it mints, as `SplitParagraph` enumerates `new_id`. It cannot work,
//! and the reason is not effort. The *number* of ids an operation mints depends on the
//! document it lands on — `ensure_run_boundary` mints one id, or none, according to whether
//! a run straddles the offset — and an operation is **transformed** before a remote replica
//! applies it, so the state it lands on there is not the state its author saw. An
//! enumeration is a count fixed at authoring time; the truth is a count discovered at
//! application time. A *space* is the only form of declaration that survives the transform.
//!
//! # The shape
//!
//! A [`NodeId`] is `(namespace: u64, counter: u64)`. A mint is a **lane**: an aligned,
//! [`Mint::LANE`]-wide run of counters inside an aligned [`Mint::BLOCK`]-wide block.
//!
//! ```text
//! block (1024 counters, aligned)
//! ├── lane 0  forward, piece 0   ← the operation as authored
//! ├── lane 1  inverse, piece 0   ← the inverse `apply` returned for it
//! ├── lane 2  forward, piece 1   ┐
//! ├── lane 3  inverse, piece 1   │ the pieces a `Rebase::KeepMany`
//! ├── …                          ┘ splits one operation into
//! └── lane 7  inverse, piece 3
//! ```
//!
//! Every derivation is a **bit field**, not a hash, so distinctness is by construction
//! rather than by a collision argument: two lanes of one block differ in the lane bits, and
//! two blocks differ above them.
//!
//! # What this buys, stated as properties
//!
//! 1. `apply` has **no id generator at all**. It cannot mint an identity the operation did
//!    not declare, because there is nothing for it to mint from. That is a compile-time
//!    choke point rather than a review rule.
//! 2. Applying one operation to one state mints one sequence of ids, on every replica.
//! 3. Undo then redo restores the **same** node ids, because `inverse` is an involution.

use casual_doc_model::{IdGenerator, NodeId};

use crate::RunIds;

/// The identity space one operation mints in.
///
/// Carried by the operation's envelope (`Transaction`, and `WireOperation` on the wire),
/// never allocated by whoever applies it. See the module documentation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Mint(NodeId);

impl Mint {
    /// Counters reserved for one operation, across all its lanes.
    pub const BLOCK: u64 = 1024;

    /// Counters one lane can hand out before it refuses.
    ///
    /// An operation in the set mints at most four (a `SplitParagraph` through three nested
    /// inline wrappers), so this is roughly thirty times the worst case that exists. It is
    /// a bound rather than a hope: [`MintedIds`] returns `None` past it and the caller
    /// raises [`EditError::IdExhausted`](crate::EditError::IdExhausted), which is a refusal
    /// a host can show — not a silent wrap into the next lane.
    pub const LANE: u64 = 128;

    /// Lanes in a block: four transform pieces, each with a forward and an inverse lane.
    pub const LANES: u64 = Self::BLOCK / Self::LANE;

    /// Reserves `blocks` consecutive operation blocks from `ids`, returning the first.
    ///
    /// The generator is advanced past every reserved counter, so ids it hands out
    /// afterwards — a payload's fresh paragraph, a clipboard clone — can never land inside
    /// a lane. One allocator, two consumers, provably disjoint.
    ///
    /// `None` when the counter space cannot hold the request.
    #[must_use]
    pub fn reserve(ids: &mut IdGenerator, blocks: u64) -> Option<Self> {
        let span = blocks.checked_mul(Self::BLOCK)?;
        // `max(BLOCK)` rather than `next_multiple_of` alone: counter 0 with namespace 0 is
        // not a representable `NodeId`, so the first block never starts at zero.
        let base = ids
            .next_counter()
            .checked_next_multiple_of(Self::BLOCK)?
            .max(Self::BLOCK);
        let end = base.checked_add(span)?;
        ids.reserve_through(end - 1);
        NodeId::from_parts(ids.namespace(), base).ok().map(Self)
    }

    /// The lane whose first identity is `base`, or `None` if `base` is not lane-aligned.
    ///
    /// The wire's constructor: a receiver builds the sender's mint from the bytes and gets
    /// a refusal rather than a silently misaligned lane.
    #[must_use]
    pub fn at(base: NodeId) -> Option<Self> {
        (base.as_u128() as u64)
            .is_multiple_of(Self::LANE)
            .then_some(Self(base))
    }

    /// The first identity this lane hands out.
    #[must_use]
    pub const fn base(self) -> NodeId {
        self.0
    }

    /// One lane per operation, from a single reservation.
    ///
    /// The shape a transaction wants: `count` blocks reserved in one step, handed back as
    /// `count` spaces. `count` of zero still reserves one block, so a caller never gets an
    /// empty vector it has to special-case.
    #[must_use]
    pub fn reserve_each(ids: &mut IdGenerator, count: usize) -> Option<Vec<Self>> {
        let count = count.max(1) as u64;
        let first = Self::reserve(ids, count)?;
        (0..count).map(|index| first.nth(index)).collect()
    }

    /// The `index`-th operation block after this one's block, on the same lane.
    ///
    /// How a transaction gives each of its operations a space of its own from one
    /// reservation.
    #[must_use]
    pub fn nth(self, index: u64) -> Option<Self> {
        let counter = (self.0.as_u128() as u64).checked_add(index.checked_mul(Self::BLOCK)?)?;
        NodeId::from_parts(self.namespace(), counter).ok().map(Self)
    }

    /// The lane an operation's **inverse** mints in.
    ///
    /// An involution — `mint.inverse().inverse() == mint` — so undo followed by redo
    /// re-mints exactly the identities the original edit minted. A comment anchored to a
    /// run that an undo destroyed therefore finds the same run when the redo brings it
    /// back, instead of a stranger wearing its place.
    #[must_use]
    pub fn inverse(self) -> Self {
        self.lane(self.lane_index() ^ 1)
    }

    /// The lane the `piece`-th product of a `Rebase::KeepMany` mints in.
    ///
    /// One intention does not always survive a transform as one operation: doc 150 §5.8
    /// leaves a formatting write holding up to three pieces. Each needs a space of its own
    /// or two of them would mint the same id. `None` past [`Mint::LANES`] / 2, which is a
    /// refusal rather than a wrap.
    #[must_use]
    pub fn piece(self, piece: u8) -> Option<Self> {
        let piece = u64::from(piece);
        (piece < Self::LANES / 2).then(|| self.lane((piece << 1) | (self.lane_index() & 1)))
    }

    /// The identities this lane hands out, in order.
    #[must_use]
    pub const fn ids(self) -> MintedIds {
        MintedIds {
            namespace: (self.0.as_u128() >> 64) as u64,
            next: self.0.as_u128() as u64,
            remaining: Self::LANE,
        }
    }

    /// The namespace half — the author's `IdSpace`.
    #[must_use]
    const fn namespace(self) -> u64 {
        (self.0.as_u128() >> 64) as u64
    }

    /// Which of the block's lanes this is.
    #[must_use]
    const fn lane_index(self) -> u64 {
        ((self.0.as_u128() as u64) % Self::BLOCK) / Self::LANE
    }

    /// The same block's `lane`-th lane. `lane` is always below [`Mint::LANES`] at every
    /// call, so the arithmetic cannot leave the block.
    #[must_use]
    fn lane(self, lane: u64) -> Self {
        let counter = self.0.as_u128() as u64;
        let block = counter - (counter % Self::BLOCK);
        Self(
            NodeId::from_parts(self.namespace(), block + lane * Self::LANE)
                .expect("a lane of a representable block is representable"),
        )
    }
}

/// The identities one [`Mint`] lane hands out, in order.
///
/// Deterministic: the same lane yields the same sequence on every replica, which is the
/// whole point of the type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MintedIds {
    namespace: u64,
    next: u64,
    remaining: u64,
}

impl RunIds for MintedIds {
    fn next(&mut self) -> Option<NodeId> {
        if self.remaining == 0 {
            return None;
        }
        let id = NodeId::from_parts(self.namespace, self.next).ok()?;
        self.next += 1;
        self.remaining -= 1;
        Some(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn mint() -> Mint {
        Mint::reserve(&mut IdGenerator::new(9), 1).expect("a block")
    }

    #[test]
    fn a_lane_hands_out_consecutive_identities_and_then_refuses() {
        let mut ids = mint().ids();
        let first = ids.next().expect("an id");
        let second = ids.next().expect("an id");
        assert_eq!(second.as_u128(), first.as_u128() + 1);
        for _ in 2..Mint::LANE {
            assert!(ids.next().is_some());
        }
        assert!(
            ids.next().is_none(),
            "a lane must refuse past its width rather than wrap into the next lane"
        );
    }

    #[test]
    fn two_replicas_of_one_lane_mint_the_same_identities() {
        let lane = mint();
        let (mut a, mut b) = (lane.ids(), lane.ids());
        let taken: Vec<_> = (0..8).map(|_| a.next()).collect();
        let again: Vec<_> = (0..8).map(|_| b.next()).collect();
        assert_eq!(taken, again);
        assert!(taken.iter().all(Option::is_some));
    }

    #[test]
    fn inverse_is_an_involution_so_redo_restores_the_original_identities() {
        let lane = mint();
        assert_ne!(lane, lane.inverse());
        assert_eq!(lane, lane.inverse().inverse());
        assert_eq!(lane.ids().next(), lane.inverse().inverse().ids().next());
    }

    #[test]
    fn every_derivation_of_one_block_is_disjoint() {
        let lane = mint();
        let mut seen = BTreeSet::new();
        for piece in 0..4u8 {
            for forward in [lane, lane.inverse()] {
                let derived = forward.piece(piece).expect("a piece");
                let mut ids = derived.ids();
                while let Some(id) = ids.next() {
                    assert!(seen.insert(id), "{id} was minted by two lanes of one block");
                }
            }
        }
        assert_eq!(seen.len() as u64, Mint::BLOCK);
    }

    #[test]
    fn a_piece_past_the_block_is_refused_rather_than_wrapped() {
        assert!(mint().piece(3).is_some());
        assert!(mint().piece(4).is_none());
    }

    #[test]
    fn consecutive_reservations_never_overlap_and_never_meet_the_generators_own_ids() {
        let mut generator = IdGenerator::new(9);
        let payload = generator.next_id().expect("an id");
        let first = Mint::reserve(&mut generator, 2).expect("two blocks");
        let after = generator.next_id().expect("an id");
        let second = Mint::reserve(&mut generator, 1).expect("a block");

        let mut seen = BTreeSet::new();
        assert!(seen.insert(payload));
        assert!(seen.insert(after));
        for lane in [first, first.nth(1).expect("the second block"), second] {
            let mut ids = lane.ids();
            while let Some(id) = ids.next() {
                assert!(seen.insert(id), "{id} was handed out twice");
            }
        }
    }

    #[test]
    fn a_misaligned_base_is_not_a_lane() {
        let lane = mint();
        assert!(Mint::at(lane.base()).is_some());
        let misaligned = NodeId::from_parts(9, (lane.base().as_u128() as u64) + 1).expect("an id");
        assert!(Mint::at(misaligned).is_none());
    }
    #[test]
    fn one_operation_applied_to_one_state_twice_mints_the_same_identities() {
        use casual_doc_model::v1::{
            BlockNode, Definitions, Document, InlineNode, Paragraph, ParagraphProperties, Run,
            RunProperties,
        };

        use crate::{FormatDelta, Operation, Pos, Range};

        /// A one-run paragraph, and the ids of the document and the paragraph.
        fn seed() -> (Document, NodeId) {
            let mut ids = IdGenerator::new(7);
            let document_id = ids.next_id().expect("an id");
            let paragraph = ids.next_id().expect("an id");
            let run = ids.next_id().expect("an id");
            let document = Document::new(
                document_id,
                vec![BlockNode::Paragraph(Paragraph {
                    id: paragraph,
                    properties: ParagraphProperties::default().into(),
                    inlines: vec![InlineNode::Run(Run {
                        id: run,
                        properties: RunProperties::default().into(),
                        text: "abcdefgh".to_owned(),
                    })],
                })],
                Definitions::default(),
            )
            .expect("a document");
            (document, paragraph)
        }

        // Bolding the middle of the run splits it in three, so this operation MINTS. An
        // operation that mints nothing would make the assertion below vacuous, which is why
        // the count is asserted rather than assumed.
        let (base, paragraph) = seed();
        let operation = Operation::FormatText {
            range: Range {
                start: Pos::new(paragraph, 2),
                end: Pos::new(paragraph, 6),
            },
            delta: FormatDelta {
                bold: Some(true),
                ..FormatDelta::default()
            },
        };
        let lane = mint();

        let mut first = base.clone();
        crate::apply(&mut first, lane, &operation).expect("the format applies");
        let mut second = base.clone();
        crate::apply(&mut second, lane, &operation).expect("the format applies");

        let runs = |document: &Document| match &document.body()[0] {
            BlockNode::Paragraph(paragraph) => paragraph
                .inlines
                .iter()
                .map(|inline| match inline {
                    InlineNode::Run(run) => run.id,
                    other => panic!("unexpected inline {other:?}"),
                })
                .collect::<Vec<_>>(),
            other => panic!("unexpected block {other:?}"),
        };
        assert_eq!(
            runs(&first).len(),
            3,
            "the precondition: the operation has to create runs for this to be about minting"
        );
        assert_eq!(
            first, second,
            "two applications of one operation to one state must agree on every node's name"
        );
        assert!(
            runs(&first)
                .iter()
                .skip(1)
                .all(|id| lane.ids().namespace == (id.as_u128() >> 64) as u64),
            "every identity the operation created must come out of the space it declared"
        );
    }
}
