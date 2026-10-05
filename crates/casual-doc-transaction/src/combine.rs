// SPDX-License-Identifier: Apache-2.0

//! Combining two branches of one document — `107` 6.5, `106` Phase 6.5, `109` OO-007.
//!
//! # What 6.5 actually asks for, split in two
//!
//! `107` §5.3 lists two capabilities under one roadmap step and they are not the same
//! mechanism:
//!
//! | | How `107` defines it | Where it lives |
//! | --- | --- | --- |
//! | **Compare** | "Replay both branches; diff the models; emit the result as tracked changes into the existing revision model" | `casual-doc-diff` plus `applyDiffAsRevisions` — a *diff*, because two arbitrary files share no history to transform |
//! | **Combine** | "Replay both; transform one branch's operations onto the other — **this is the OT transform already built**, applied offline" | here |
//!
//! So compare answers "what is different", and combine answers "give me both". This module is
//! only the second, and it is deliberately not a diff: a diff is an *inference* about two
//! documents, where a branch's commits are a *record* of what somebody actually did. Where the
//! record exists, using it is strictly better — it preserves intent, it keeps node identity, and
//! it cannot invent an edit nobody made. `applyDiffAsRevisions`'s own warning is the reason the
//! distinction matters: a diff change carries a 160-byte *excerpt*, so applying one blindly can
//! write text into a document that was never in either side of it.
//!
//! # The established pattern, named before any code
//!
//! **Rebase a branch onto another**, which is `git rebase` and is Jupiter's client algorithm
//! with the network removed. `150` built the pair transform, `152` §5.3 built the
//! rollback/replay driver around it, and this module supplies two branches and no socket.
//!
//! # One mechanism, not a second one
//!
//! This does **not** re-implement the sequence rebase. [`ClientSession::receive`] already does
//! exactly the right thing — roll this replica's unordered work back to its base, apply the
//! arrival there, replay the work rebased — and it does the part that is easy to get wrong:
//! both *sides* have to be transformed as the sequence advances, because `theirs[1]` was written
//! against `theirs[0]`'s result while `ours` has moved under it. A hand-rolled
//! "transform each of theirs over each of ours" loop is correct for one commit and silently
//! wrong for two, which is `SKILL` §8's "prefer one mechanism over two" with a concrete price.
//!
//! So combine is set up as the live case with nobody on the other end: a session joined at the
//! common ancestor, **our** branch applied as unacknowledged local work, and **their** branch
//! fed in one commit at a time as ordered arrivals. The merged document is what the driver
//! leaves behind.
//!
//! # Why the two branches must have minted in different spaces, and what happens if they did not
//!
//! `150` §9.3 and `152` §4.2: a replica mints node identities in a partitioned space so two
//! people editing one document never produce the same id for different nodes. An **offline fork**
//! — open one file twice and edit both copies — mints both branches in `IdSpace::local`, so the
//! two branches really can name different nodes identically, and combining them would
//! overwrite one side's node with the other's.
//!
//! That is not papered over here. Their operations travel through [`WireOperation::localise`],
//! the same check an arriving operation faces, so a branch that minted in a space it does not
//! own is refused as a [`CombineError::TheirBranch`] carrying
//! [`crate::session::SessionError::IdCollision`], which names the id —
//! the `ODC-7008` family — rather
//! than merged into a document with two meanings for one name. **The practical consequence,
//! stated rather than left to be discovered: combine works between participants of a room, and
//! between replicas that called `adoptParticipantIdentity`, and not between two offline forks of
//! one file.** For those, compare is the answer, and that is `applyDiffAsRevisions`.
//!
//! # What this does not do
//!
//! - **It does not produce tracked changes.** The merged document carries both branches' edits
//!   as edits. `105` OO-007 additionally asks that a combine *merge* pre-existing tracked
//!   changes rather than refusing them, and that is a property of the revision model rather than
//!   of the transform: a tracked change is already content here, so it combines like any other.
//!   What is *not* built is marking one branch's contribution as a suggestion after the fact —
//!   see the module's open question below.
//! - **It does not read or write a file.** A `Document` and a `&[Commit]` in, a `Document` out.
//!   "Combine from file, URL or storage" (`105` OO-007) is a surface, and the surface for it
//!   needs the branches' logs, which no file format carries yet — `147` §7 Q2 is the open
//!   question there (the log has no persisted form, because `Label` is a `&'static str`).
//! - **It does not choose a winner.** Every refusal is reported and nothing is resolved by
//!   guessing. `TransformError::Unsupported` means "these two cannot be merged"; it never means
//!   "carry on" (`transform`'s own words).
//!
//! # Complexity
//!
//! O(their commits × our commits) pair transforms, plus the cost of applying each operation
//! once — which is `107` §4 B2's bound (concurrent operations, not log length, not document
//! size) summed over the arrivals. One document clone per contended arrival, as on the live
//! path, and **not** one per operation. Nothing here runs on a keystroke.

use casual_doc_edit::access::Capabilities;
use casual_doc_model::IdGenerator;
use casual_doc_model::v1::Document;

use crate::protocol::{Arrival, ClientId, PROTOCOL_VERSION, Revision, ServerMessage};
use crate::session::{ClientSession, SessionError};
use crate::transform::Tombstone;
use crate::wire::WireOperation;
use crate::{Commit, RevisionLog, Transaction, TransactionError};

/// One side of a combine: who wrote it, and what they did to the common ancestor.
///
/// `commits` are in application order and all of them must be *above* the ancestor — this type
/// carries no base revision because the ancestor is the document passed to [`combine`], and a
/// commit that was written against something else cannot be identified as such from here. A
/// caller holding a longer log slices it; `RevisionLog::commits` is in order.
#[derive(Clone, Copy, Debug)]
pub struct Branch<'a> {
    /// The participant number this branch minted under. Two branches must not share one.
    pub client: ClientId,
    /// What this branch did, oldest first.
    pub commits: &'a [Commit],
}

/// A merged document, and everything that was lost or replayed getting there.
#[derive(Debug)]
pub struct Combined {
    /// The third document: the ancestor with both branches' work in it.
    pub document: Document,
    /// Its history — their branch ordered, ours replayed above it.
    pub log: RevisionLog,
    /// Operations dropped because what they addressed no longer existed after the other
    /// branch's work was applied.
    ///
    /// **Must be reported.** A tombstone is a loss, and no-silent-loss means it reaches the
    /// disposition taxonomy (`35`) rather than a log line. A combine that returns an empty
    /// vector lost nothing; one that does not is not a failure, but it is not silent either.
    pub tombstones: Vec<Tombstone>,
    /// How many of our commits were rolled back and replayed in total, summed over their
    /// arrivals. Zero means the two branches never met — not that nothing happened.
    pub replayed: usize,
}

/// Why two branches could not be combined.
#[derive(Debug)]
#[non_exhaustive]
pub enum CombineError {
    /// The two branches name the same participant, so their identity spaces are the same one
    /// and neither branch's node ids can be trusted against the other's.
    ///
    /// Refused **before** anything is applied, because this is a question about the inputs and
    /// not about any particular operation: letting it through would make the collision check a
    /// matter of luck about which ids each side happened to mint.
    SameParticipant(ClientId),
    /// A participant number with no identity space of its own — the two highest, which alias
    /// the document's own space and the offline space (`152` §4.2).
    NoIdSpace(ClientId),
    /// Replaying one of **our** commits onto the ancestor failed, so the branch does not apply
    /// to the document it was given.
    ///
    /// The usual cause is an ancestor that is not the one the branch was written against.
    OurBranch(TransactionError),
    /// Their branch could not be merged. Carries the driver's own reason, which distinguishes
    /// "these two operations have no transform" from "an id collided" from "a commit kept no
    /// inverse and so cannot be rolled back".
    TheirBranch(SessionError),
    /// A commit carries fewer mints than operations, so the identities one of its operations
    /// would create have no space to come from.
    ///
    /// Refused rather than defaulted. `Transaction::apply` answers the same shape with
    /// `EditError::IdExhausted` "rather than being quietly applied with borrowed identities",
    /// and a combine that borrowed one would be minting a node in whichever space happened to
    /// be to hand — the single failure the identity partition exists to make impossible.
    MintsMissing {
        /// Which side it was on: `true` for ours, `false` for theirs.
        ours: bool,
    },
    /// The ordered position counter is exhausted, which needs 2^64 arrivals.
    RevisionExhausted,
}

impl core::fmt::Display for CombineError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::SameParticipant(client) => write!(
                formatter,
                "both branches are participant {}, so they mint in one identity space and \
                 neither branch's node ids mean anything against the other's",
                client.get()
            ),
            Self::NoIdSpace(client) => write!(
                formatter,
                "participant {} has no identity space of its own",
                client.get()
            ),
            Self::OurBranch(error) => write!(
                formatter,
                "our branch does not apply to that ancestor: {error}"
            ),
            Self::TheirBranch(error) => write!(formatter, "their branch cannot merge: {error}"),
            Self::MintsMissing { ours } => write!(
                formatter,
                "a commit on {} branch carries fewer identity spaces than operations",
                if *ours { "our" } else { "their" }
            ),
            Self::RevisionExhausted => formatter.write_str("the ordered position is exhausted"),
        }
    }
}

impl std::error::Error for CombineError {}

/// Builds the third document: `ancestor` with both branches' work in it.
///
/// `ours` is replayed onto the ancestor first and then **rebased** as each of `theirs` is
/// ordered in, which is the asymmetry a rebase has: the merged history reads as their branch
/// followed by ours. That is a choice and not an accident — their operations are applied exactly
/// as written and ours are the ones expressed in new coordinates — so a caller that wants the
/// other reading swaps the arguments. Convergence does not depend on which way round it is
/// (`150`'s TP1 property is what makes that true), but *authorship* of the rebased side does,
/// and a surface offering Combine should say whose changes are being moved.
///
/// # Errors
///
/// [`CombineError`], every variant of which leaves the caller's inputs untouched — this function
/// takes the ancestor by reference and builds its own copy.
///
/// # Complexity
///
/// See the module docs: O(their commits × our commits) pair transforms. Not a keystroke path.
pub fn combine(
    ancestor: &Document,
    ours: Branch<'_>,
    theirs: Branch<'_>,
) -> Result<Combined, CombineError> {
    if ours.client == theirs.client {
        return Err(CombineError::SameParticipant(ours.client));
    }
    // Both sides' inverses are checked BEFORE anything is applied. The driver would refuse an
    // inverse-less commit of ours in the middle of the replay, having already merged part of
    // their branch, and a half-merged document is not an answer a caller can do anything with.
    // NEITHER SIDE IS CHECKED FOR INVERSES, AND THAT TOOK PROVING. The first draft refused a
    // `Coalesce::ContinueKeepingFirstInverse` commit on either branch, on `152` §11's reading
    // that such a commit "cannot be rolled back". That reading is true of a LIVE session, where
    // the commit is already in the log with no inverse and the rebase has nothing to undo it
    // with — and it does not hold here, for a reason that only shows up when the check is
    // removed and the merge is inspected: **a combine re-applies both branches from scratch**,
    // and `RevisionLog::apply` computes each inverse as it applies the operation. So every
    // commit in the merged log carries an inverse however its source was recorded. Both halves
    // of the refusal were driven red and both produced correctly merged documents.
    //
    // **Offline combine is therefore strictly more capable than a live session on this axis**,
    // which is worth knowing: suggesting mode cannot take part in a room (`152` §5.3) and the
    // same branch can be combined offline afterwards.
    //
    // What IS lost, stated rather than left to be noticed: the branch's undo GROUPING. Each
    // commit is replayed as `Coalesce::New`, so a typing run that was one undo step on its own
    // branch becomes several in the merged document. `Commit` exposes its `GroupId` and not its
    // `Coalesce`, so preserving the grouping means deriving one from the other; it is not needed
    // for the merge to be right and it is not done here.
    for (side, branch) in [(true, &ours), (false, &theirs)] {
        if branch
            .commits
            .iter()
            .any(|commit| commit.mints().len() < commit.operations().len())
        {
            return Err(CombineError::MintsMissing { ours: side });
        }
    }

    let mut document = ancestor.clone();
    let mut log = RevisionLog::default();
    // Joined at the ancestor, through the one constructor sessions have. The `Welcome` is
    // fabricated because there is no relay here, and that is the honest shape of "the transform
    // applied offline": every field of it is a fact this function knows. `Capabilities::owner`
    // because a combine is not an access-control question — whoever may run it has already been
    // allowed to hold both branches.
    let mut session = ClientSession::joined(
        &document,
        &ServerMessage::Welcome {
            protocol: PROTOCOL_VERSION,
            client: ours.client,
            revision: Revision::new(0),
            capabilities: Capabilities::owner(),
            // No room, so no membership. Empty is the fact rather than a placeholder.
            participants: Vec::new(),
        },
        &mut log,
    )
    .map_err(|error| match error {
        SessionError::NoIdSpace => CombineError::NoIdSpace(ours.client),
        other => CombineError::TheirBranch(other),
    })?;
    let mut ids = IdGenerator::new(session.id_space().get());

    // Our branch becomes this replica's **unordered** work: `joined` settled the log at the
    // ancestor, so everything appended after it is above the horizon and is what a rebase rolls
    // back. Each commit keeps its own mints, which is what makes the replay re-create the very
    // nodes it first created rather than renaming them (`Commit::mints`).
    for commit in ours.commits {
        let transaction = Transaction::new(
            commit.id(),
            log.head(),
            commit.label(),
            commit.mints().to_vec(),
            commit.operations().to_vec(),
        )
        .with_intents(commit.intents().to_vec())
        .with_origin(commit.origin());
        log.apply(&mut document, transaction)
            .map_err(CombineError::OurBranch)?;
    }

    // Their branch arrives in order. Through `Arrival` and `WireOperation` rather than straight
    // into the driver, because that is where the identity rule lives: `localise` recomputes what
    // each operation introduces and refuses an id minted outside the sender's own space, which
    // is the whole reason two offline forks cannot be combined (module docs).
    let mut tombstones = Vec::new();
    let mut replayed = 0;
    let mut revision = Revision::new(0);
    for commit in theirs.commits {
        revision = revision.next().ok_or(CombineError::RevisionExhausted)?;
        // Zipped rather than indexed, so a commit whose `mints` is shorter than its
        // `operations` yields a short arrival that the driver refuses as empty or malformed,
        // instead of this loop either panicking or inventing a mint. `MintsMissing` above has
        // already refused that shape; the zip is what makes the refusal structural.
        let operations = commit
            .operations()
            .iter()
            .zip(commit.mints())
            .enumerate()
            .map(|(index, (operation, mint))| {
                WireOperation::of(operation.clone(), *mint).declaring(commit.intent(index))
            })
            .collect();
        let arrival = Arrival {
            revision,
            client: theirs.client,
            operations,
        };
        let reception = session
            .receive(&arrival, &mut document, &mut ids, &mut log)
            .map_err(CombineError::TheirBranch)?;
        replayed += reception.replayed;
        tombstones.extend(reception.tombstones);
    }

    Ok(Combined {
        document,
        log,
        tombstones,
        replayed,
    })
}

#[cfg(test)]
#[path = "combine_tests.rs"]
mod tests;
