// SPDX-License-Identifier: Apache-2.0

//! Selective undo: reverting **this replica's own** step in a document other people are
//! editing, without touching what they wrote.
//!
//! # The established pattern, named first — SKILL §8
//!
//! **Selective (per-user) undo over an operation log by transformation**: the inverse of the
//! step being undone is rebased over everything that happened after it, exactly as a
//! concurrent operation would be, and only then applied. It is the undo model of Google Docs
//! and of ONLYOFFICE's fast co-editing, and in the OT literature it is the "undo as a
//! concurrent inverse" construction (Ressel & Gunzenhäuser; Sun's ANYUNDO), restricted here to
//! the case a relay-ordered log needs — the TP1-only one, because every operation the inverse
//! is rebased over is already in the settled order.
//!
//! # Why the plain inverse is wrong in a shared document
//!
//! An inverse is written against the state *immediately after* its step. With nothing after
//! the step that is the current state, and undo is just "apply the inverse" — the single-user
//! path, which this module leaves exactly as it was. Once somebody else's edit lands after the
//! step, the inverse's coordinates are stale: undoing three typed characters at offset 7 after
//! a collaborator typed in front of them would delete three of *their* characters. So the
//! inverse has to travel forward over the later edits first.
//!
//! # The path an inverse travels, and why each leg is exact
//!
//! A step's commits are undone newest first, and each commit's inverses in the order the
//! commit recorded them. Inverse `x` of commit `c` was written against the state reached by
//! applying `c` and then the inverses of `c` already undone in this call. To reach the
//! current document it travels:
//!
//! 1. **back** over this commit's already-undone inverses — each as the change "apply the
//!    forward operation it undid", whose own inverse is that undone inverse, exactly;
//! 2. over **every commit after `c`** in log order — anybody's — each as the `(operation,
//!    inverse)` pair the log recorded;
//! 3. over **everything this call has already emitted**, each with the inverse that applying
//!    it actually returned, because it is applied to the document as it is produced.
//!
//! Every leg is a recorded or freshly observed `(operation, inverse)` pair, never one
//! reconstructed by hand, which is what makes the whole path a sequence of exact changes
//! [`transform`](crate::transform) can read. Step 3 needs inverses nobody has recorded, so the
//! emitted operations are applied as they are produced and **applied back** before returning:
//! the same probe the session's rebase driver uses, resting on the same invariant — an
//! operation's inverse restores the state it was applied to.
//!
//! # When it refuses, and why that is the safe answer
//!
//! A transform with no answer, an operation it would have to drop as a tombstone, and a
//! commit that kept no inverse all refuse the whole undo. The alternative in each case is to
//! guess, and a guessed undo in a shared document deletes or rewrites somebody else's text —
//! the defect this module exists to remove. A refused undo changes nothing: the document,
//! the log and the step are exactly as they were, and the step stays at the top of Undo so the
//! reader can see what they could not reverse.
//!
//! An inverse whose every piece is **satisfied** — somebody already deleted the text the step
//! typed — produces no operations at all. That is not a refusal; there is nothing left to
//! undo. The caller retires the step and moves on to the next older one, as Google Docs does.
//!
//! # Complexity
//!
//! `O(k · (k + m))` transforms, for `k` operations in the step and `m` operations committed
//! after it; each transform is O(1) in document size (`150` §Cost). Both counts are bounded by
//! the log's own bound (`107` B7), so this is O(1) in document size, and it is paid only when
//! the reader presses Undo in a document someone else edited since — **never on a keystroke**,
//! and never in a single-user session, which takes the plain inverse without looking further
//! than the one scan that proves nobody else wrote after the step.

use casual_doc_edit::{Mint, Operation};
use casual_doc_model::IdGenerator;
use casual_doc_model::v1::Document;

use crate::transform::{Change, NoPlacement, Rebase, Side, transform_declared};
use crate::{GroupId, Intent, Origin, RevisionLog};

/// Why a step could not be undone selectively.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SelectiveUndoError {
    /// No such step is held.
    Unknown,
    /// A commit of the step, or one after it, kept no inverse, so neither its effect nor its
    /// undo can be described to the transform (`Coalesce::ContinueKeepingFirstInverse`).
    NoInverse,
    /// The inverse cannot be carried over somebody else's later edit: the transform has no
    /// answer for the pair, or would have to drop part of the undo.
    Conflict {
        /// The undo operation that could not be carried.
        subject: &'static str,
        /// The later operation it met.
        against: &'static str,
    },
    /// Applying the carried inverse was refused by the document.
    Refused,
}

impl SelectiveUndoError {
    /// The sentence a reader is shown. It names what is protected, because the refusal is
    /// the protection: their collaborator's text is untouched.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Unknown => "There is nothing to undo.",
            Self::NoInverse | Self::Conflict { .. } | Self::Refused => {
                "Undo cannot reverse your last change cleanly, because someone else has since \
                 edited the same place. Their work and yours are both kept; change it by hand."
            }
        }
    }
}

impl RevisionLog {
    /// The operations that revert `group` **as of now**, rebased over everything committed
    /// after it — this replica's selective undo (see the module docs).
    ///
    /// When nothing after the step's first commit came from somebody else, this is exactly
    /// [`RevisionLog::inverse_of`] and touches nothing: the single-user path is unchanged
    /// and costs one scan of the step's suffix. Otherwise `document` is used as a scratch
    /// surface — operations are applied and applied back — and is returned exactly as it was
    /// found, on success and on every error. `ids` provides the scratch identity spaces.
    ///
    /// An empty result means the step has nothing left to revert; see the module docs.
    ///
    /// # Errors
    ///
    /// [`SelectiveUndoError`], with the document and the log unchanged.
    ///
    /// # Complexity
    ///
    /// `O(k · (k + m))` transforms for `k` operations in the step and `m` after it — bounded
    /// by the log's bound, so O(1) in document size — plus two applications per emitted
    /// operation.
    pub fn selective_inverse(
        &self,
        group: GroupId,
        document: &mut Document,
        ids: &mut IdGenerator,
    ) -> Result<Vec<Operation>, SelectiveUndoError> {
        let indices: Vec<usize> = self
            .commits
            .iter()
            .enumerate()
            .filter(|(_, commit)| commit.group == group)
            .map(|(index, _)| index)
            .collect();
        let Some(&first) = indices.first() else {
            return Err(SelectiveUndoError::Unknown);
        };
        // Somebody else's edit after the step — or, for an AI turn (`Coalesce::Join`),
        // ANY commit after its first that is not the turn's own: the person typing while the
        // assistant wrote. Its plain inverse would then be written against a state that no
        // longer exists, exactly as with a collaborator's edit.
        let turn = self.is_turn_group(group);
        let foreign_after = self
            .commits
            .iter()
            .skip(first)
            .any(|commit| commit.origin == Origin::Remote || (turn && commit.group != group));
        if !foreign_after {
            return Ok(self.inverse_of(group));
        }

        let mut emitted: Vec<(Operation, Operation, Mint)> = Vec::new();
        let outcome = self.carry(&indices, document, ids, &mut emitted);
        // Applied back whatever happened, newest first, so the document is the one the caller
        // handed over. This is the probe invariant, and a failure here is a broken inverse —
        // the same fault undo itself would hit.
        for (_, inverse, mint) in emitted.iter().rev() {
            if casual_doc_edit::apply(document, mint.inverse(), inverse).is_err() {
                return Err(SelectiveUndoError::Refused);
            }
        }
        outcome?;
        Ok(emitted
            .into_iter()
            .map(|(operation, _, _)| operation)
            .collect())
    }

    /// The forward half of [`RevisionLog::selective_inverse`]: carries every inverse to the
    /// current state and applies it to `document`, recording what it applied in `emitted`.
    fn carry(
        &self,
        indices: &[usize],
        document: &mut Document,
        ids: &mut IdGenerator,
        emitted: &mut Vec<(Operation, Operation, Mint)>,
    ) -> Result<(), SelectiveUndoError> {
        for &index in indices.iter().rev() {
            let commit = &self.commits[index];
            if commit.inverse_operations.len() != commit.operations.len() {
                return Err(SelectiveUndoError::NoInverse);
            }
            let mut later: Vec<(Operation, Operation)> = Vec::new();
            for after in self.commits.iter().skip(index + 1) {
                let changes = after.changes().ok_or(SelectiveUndoError::NoInverse)?;
                later.extend(
                    changes.map(|change| (change.operation.clone(), change.inverse.clone())),
                );
            }
            let count = commit.operations.len();
            // `(forward, inverse)` of this commit's inverses already undone, oldest first.
            let mut undone: Vec<(Operation, Operation)> = Vec::new();
            for (position, inverse) in commit.inverse_operations.iter().enumerate() {
                let forward = commit.operations[count - 1 - position].clone();
                let mut path: Vec<(Operation, Operation)> = undone
                    .iter()
                    .rev()
                    .map(|(forward, inverse)| (forward.clone(), inverse.clone()))
                    .collect();
                path.extend(later.iter().cloned());
                path.extend(
                    emitted
                        .iter()
                        .map(|(operation, inverse, _)| (operation.clone(), inverse.clone())),
                );
                for piece in carry_one(inverse, &path)? {
                    let mint = Mint::reserve(ids, 1).ok_or(SelectiveUndoError::Refused)?;
                    let applied = casual_doc_edit::apply(document, mint, &piece)
                        .map_err(|_| SelectiveUndoError::Refused)?;
                    emitted.push((piece, applied, mint));
                }
                undone.push((forward, inverse.clone()));
            }
        }
        Ok(())
    }
}

/// Rebases one inverse over a sequence of exact changes, refusing rather than guessing.
fn carry_one(
    subject: &Operation,
    path: &[(Operation, Operation)],
) -> Result<Vec<Operation>, SelectiveUndoError> {
    let mut current = vec![subject.clone()];
    for (operation, inverse) in path {
        let change = Change::new(operation, inverse);
        let mut next = Vec::with_capacity(current.len());
        for piece in &current {
            match transform_declared(piece, Intent::NONE, change, Side::Later, &NoPlacement) {
                Ok(Rebase::Keep(operation)) => next.push(operation),
                Ok(Rebase::KeepMany(operations)) => next.extend(operations),
                Ok(Rebase::Satisfied) => {}
                Ok(Rebase::Tombstoned(tombstone)) => {
                    return Err(SelectiveUndoError::Conflict {
                        subject: tombstone.operation,
                        against: tombstone.against,
                    });
                }
                Err(crate::transform::TransformError::Unsupported {
                    subject, against, ..
                }) => return Err(SelectiveUndoError::Conflict { subject, against }),
            }
        }
        current = next;
    }
    Ok(current)
}
