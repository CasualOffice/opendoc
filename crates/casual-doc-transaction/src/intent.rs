// SPDX-License-Identifier: Apache-2.0

//! What the author knew that the operation cannot say — doc 150 §9.1 and §9.2, answered on
//! the **envelope** rather than in the closed operation set.
//!
//! # The two findings, and the one thing they have in common
//!
//! `150` §9.1 and §9.2 were the last two blockers `152` §9 named for the byte codec, and
//! each was recorded as needing an operation-set change (ADR-030 I2):
//!
//! 1. **Block operations are index-addressed, not node-addressed.** `InsertBlocks`,
//!    `InsertTable` and `InsertFieldRange` name a sibling *index*. That index is a cached
//!    resolution of an intention — *put it between the same two neighbours* — and the cache
//!    goes stale the moment a concurrent operation adds or removes a block before it.
//!    Neither operation says where the other sits, which is the whole reason the
//!    [`BlockPlacement`](crate::transform::BlockPlacement) seam exists and the reason
//!    refusals U2 and U6 exist.
//! 2. **`Pos` carries no affinity.** An insertion at a run boundary cannot say which side of
//!    it the text belongs to, so an insertion at offset 0 of a paragraph a concurrent join
//!    absorbed is refused (U5) even though one of the two possible intentions is perfectly
//!    expressible.
//!
//! What they have in common is the reason they do **not** belong in the operation set:
//! neither fact changes what `apply` does. An index is what `apply` consumes and it is
//! already exact on the author's own replica; an affinity is not consumed by `apply` at all
//! — `casual_doc_edit::insert_text`'s attachment rule is fixed, and changing it would break
//! convergence (`150` §5.4). Both are **authoring** facts: things true of the moment the
//! operation was written, needed only by a *transform*, and needed only on a replica that
//! received the operation rather than wrote it.
//!
//! # The precedent, named
//!
//! ADR-051 reached the same conclusion for identity one increment earlier. `150` §9.3
//! proposed that an operation enumerate the ids it mints; that could not work, because the
//! number of identities an operation mints is discovered at *application* time. The answer
//! was to carry the identity **space** on the envelope, and the consequence worth repeating
//! is that **no `Operation` variant changed**.
//!
//! This module is that answer applied twice more. The established pattern is the one every
//! OT engine uses for facts a transform needs and an operation cannot hold: the sibling
//! engine hands its transform a sheet-name table rather than widening its operations, and
//! `150` §4 already adopted that shape for placement. An [`Intent`] is the same move, with
//! the fact travelling *with* the operation instead of being reconstructed beside it — which
//! matters because an authoring fact cannot be reconstructed by a receiver at all.
//!
//! # Why this is reversible, and the op-set change would not have been
//!
//! An envelope field is additive: a transaction that declares nothing behaves exactly as
//! today, which is how every existing caller and every existing test keeps working, and a
//! decoder that meets an intent it does not understand skips it. Changing `Pos` or an
//! operation's fields is irreversible in the one way that matters — it is a **breaking
//! change to every literal in every crate**, measured here as two `E0063` in
//! `casual-doc-wasm`, a file another lane holds, which is precisely the failure the working
//! contract records as "two green PRs can make `main` red".
//!
//! # What an intent is not
//!
//! It is not a second address. [`Operation`](casual_doc_edit::Operation) remains the only
//! thing `apply` reads, and an operation whose envelope declares nothing is exactly as
//! applicable as it was before this module existed. An intent is read by
//! [`transform`](crate::transform::transform) and by [`resolve_anchor`], and by nothing
//! else.

use casual_doc_edit::Operation;
use casual_doc_model::NodeId;

use crate::Affinity;

/// The block a sibling-indexed operation counted its index to — doc 150 §9.1.
///
/// This is the node-addressed form of a block slot, and it is *invariant* under every
/// concurrent change that does not destroy the block it names. That invariance is the whole
/// value: an index has to be rebased over every concurrent structural edit and needs a fact
/// about the base state to do it, while an anchor needs nothing and is rebased by doing
/// nothing at all.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BlockAnchor {
    /// The operation was authored immediately **before** this block.
    Before(NodeId),
    /// The operation was authored at the **end** of its container: no block followed it.
    ///
    /// A separate variant rather than `Before(None)` because "after everything" is a
    /// different fact from "before a specific thing", and it survives differently: appending
    /// stays appending however many blocks a concurrent operation added.
    AtEnd,
}

/// What the author of one operation knew that the operation's own fields cannot say.
///
/// One value per operation, in the transaction's operation order — the same carrying
/// discipline [`Mint`](casual_doc_edit::Mint) uses, and for the same reason: the unit that
/// declares is the unit that is transformed.
///
/// [`Intent::NONE`] declares nothing and is the default, so a caller that does not know
/// these facts is in exactly the position every caller was in before this type existed: its
/// pairs are refused rather than guessed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Intent {
    anchor: Option<BlockAnchor>,
    affinity: Option<Affinity>,
}

impl Intent {
    /// Declares nothing. The default, and what every pair that needs a declaration is
    /// refused against.
    pub const NONE: Self = Self {
        anchor: None,
        affinity: None,
    };

    /// The operation was authored immediately before `block`.
    #[must_use]
    pub const fn before(block: NodeId) -> Self {
        Self {
            anchor: Some(BlockAnchor::Before(block)),
            affinity: None,
        }
    }

    /// The operation was authored at the end of its container.
    #[must_use]
    pub const fn at_end() -> Self {
        Self {
            anchor: Some(BlockAnchor::AtEnd),
            affinity: None,
        }
    }

    /// The same intent, declaring which side of a boundary the author's content belongs to.
    ///
    /// [`Affinity::Before`] means *with the text that precedes the position*;
    /// [`Affinity::After`] means *with the text that follows it*. This is the same
    /// vocabulary [`Position`](crate::Position) already uses for selection mapping, reused
    /// deliberately: one mechanism for "which side of a boundary", not two.
    #[must_use]
    pub const fn with_affinity(mut self, affinity: Affinity) -> Self {
        self.affinity = Some(affinity);
        self
    }

    /// The block this operation's index was counted to, if it declared one.
    #[must_use]
    pub const fn anchor(self) -> Option<BlockAnchor> {
        self.anchor
    }

    /// Which side of its position the author's content belongs to, if it declared.
    #[must_use]
    pub const fn affinity(self) -> Option<Affinity> {
        self.affinity
    }

    /// Whether this intent declares nothing at all.
    #[must_use]
    pub const fn is_none(self) -> bool {
        self.anchor.is_none() && self.affinity.is_none()
    }
}

/// Where a block sits **in the state an operation is about to be applied to**.
///
/// # Why this is a different trait from [`BlockPlacement`](crate::transform::BlockPlacement)
///
/// They have the same shape and opposite preconditions, and conflating them is the hazard
/// `150` §4 spends a paragraph on. `BlockPlacement` must describe the **base** state — the
/// state both concurrent operations were written against — which neither replica holds at
/// the moment it transforms, so a caller that cannot produce one must take a refusal.
/// `BlockTarget` describes the state the caller is **about to mutate**, which every caller
/// holds by definition. One is a promise that is hard to keep; the other is a fact in hand.
///
/// Two traits over one [`BlockIndex`](crate::transform::BlockIndex) is therefore not two
/// mechanisms for one rule — it is one mechanism answering two different questions, and the
/// names are what stop the wrong one being passed.
pub trait BlockTarget {
    /// The container holding `node`'s block and `node`'s 0-based index within it.
    ///
    /// `None` when the block is not there — which, for an anchor, means a concurrent change
    /// destroyed it.
    fn block_position(&self, node: NodeId) -> Option<(Option<NodeId>, u32)>;

    /// How many blocks `container` holds (`None` = the document body).
    ///
    /// `None` when the container is not one this target walked, which becomes a refusal.
    fn block_count(&self, container: Option<NodeId>) -> Option<u32>;
}

/// Why an anchor could not be resolved against the state it was to be applied to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnchorError {
    /// The anchored block is gone: a concurrent change destroyed it. The operation's
    /// intention named a neighbour that no longer exists, so there is no position to put it
    /// in — the caller must report this as a loss, exactly as a
    /// [`Tombstone`](crate::transform::Tombstone) is reported.
    AnchorDestroyed {
        /// The block the operation was authored before.
        anchor: NodeId,
    },
    /// The container is not one the target indexed, so the answer is unknown rather than
    /// wrong. The degradation rule `150` §4 states for `BlockPlacement` holds here too.
    ContainerUnknown,
    /// The operation does not address a block slot, so an anchor means nothing for it. A
    /// caller declaring one is a programming error, reported rather than ignored.
    NotASlotOperation {
        /// The operation that was handed an anchor.
        operation: &'static str,
    },
}

/// Rewrites a block-slot operation's cached index from the anchor its author declared.
///
/// # Why the caller does this and `transform` does not
///
/// An arriving operation is rebased over *every* commit since its base revision, so a
/// resolution performed inside one transform step would be against an intermediate state
/// and would then have to be rebased again by the next step — reintroducing exactly the
/// arithmetic the anchor removes. The anchor is invariant through the whole sequence, so
/// there is one correct moment to resolve it: immediately before the operation is applied.
/// `transform` therefore leaves an anchored slot *alone* and this function is the single
/// place the index is re-derived.
///
/// That split is also what keeps `transform` pure: it reads two operations and a settled
/// order, never a document.
///
/// # Errors
///
/// [`AnchorError`] — and a caller must treat `AnchorDestroyed` as a loss rather than as a
/// reason to fall back on the stale index, because falling back is silent divergence.
///
/// # Complexity
///
/// O(1) given a built [`BlockTarget`]. Building one is O(document blocks) and happens once
/// per *applied* arrival, not once per keystroke: a single-user edit never resolves an
/// anchor at all, because it never transforms.
pub fn resolve_anchor(
    operation: &mut Operation,
    anchor: BlockAnchor,
    target: &dyn BlockTarget,
) -> Result<(), AnchorError> {
    let (container, index) = match operation {
        Operation::InsertBlocks {
            container, index, ..
        }
        | Operation::InsertTable {
            container, index, ..
        } => (container, index),
        // A body-level-only operation: its container is the body by construction, so the
        // anchor only re-derives the index.
        Operation::InsertFieldRange { index, .. } => {
            *index = resolve_index(anchor, None, target)?;
            return Ok(());
        }
        other => {
            return Err(AnchorError::NotASlotOperation {
                operation: crate::transform::variant_name(other),
            });
        }
    };
    let resolved_container = match anchor {
        BlockAnchor::Before(block) => {
            target
                .block_position(block)
                .ok_or(AnchorError::AnchorDestroyed { anchor: block })?
                .0
        }
        // Appending stays in the container the operation already named: "the end" is a
        // position in a container, and nothing about a concurrent edit moves it elsewhere.
        BlockAnchor::AtEnd => *container,
    };
    *index = resolve_index(anchor, resolved_container, target)?;
    *container = resolved_container;
    Ok(())
}

/// The 0-based slot `anchor` names in `container`, in the target state.
fn resolve_index(
    anchor: BlockAnchor,
    container: Option<NodeId>,
    target: &dyn BlockTarget,
) -> Result<u32, AnchorError> {
    match anchor {
        BlockAnchor::Before(block) => target
            .block_position(block)
            .map(|(_, index)| index)
            .ok_or(AnchorError::AnchorDestroyed { anchor: block }),
        BlockAnchor::AtEnd => target
            .block_count(container)
            .ok_or(AnchorError::ContainerUnknown),
    }
}
