// SPDX-License-Identifier: Apache-2.0

//! A **participant's** access level, enforced at the operation — `107` 6.7, `152` §10 Q4/Q5.
//!
//! # The two authorities, which are not the same question
//!
//! [`protection`](crate::protection) answers *what the document asks of everyone*:
//! `w:documentProtection` is a property of the file, it travels with the file, and ADR-052
//! records that it is **policy and not security** — a local reader holds the bytes and may
//! lift it, which is exactly what Word does with an unpassworded restriction.
//!
//! This module answers *what this participant may do*, which is a different authority with a
//! different source. A document that says "do not edit me" and a participant who is not
//! allowed to edit it are two separate facts, and `152` §10 Q5 is the record that collapsing
//! them is wrong. The distinction has a concrete consequence: a room's read-only guest must
//! **not** be able to lift a restriction, while a standalone reader must still be able to,
//! and `protection::exempt_from_protection` is the one place that tells them apart (ADR-059).
//!
//! **The grant is the outer gate and the document's policy is the inner one.** A participant
//! with no write capability is refused on an unprotected document too, so the access check
//! runs first and the protection check runs second. Reversing them would let an unprotected
//! document admit a read-only guest's edit.
//!
//! # The established pattern, named before any code
//!
//! A **least-privilege capability grant**: the host signs a short-lived token binding a
//! subject to a document and a capability set, the boundary verifies it, and the engine
//! enforces the capabilities it got back. That is OAuth2/JWT's audience-subject-scope-expiry,
//! WOPI's access token, ONLYOFFICE's JWT and Fluid's permission-bound container — four
//! independent precedents for the same shape, which `143` §10 had already written down as
//! five enforcement layers. Nothing here is new; what is new is that the layer this repository
//! owns now exists.
//!
//! # What travels, and what does not
//!
//! **[`Capabilities`] never travels from a client.** `152`'s presence rule applies with equal
//! force here: the relay hands a participant its capabilities in
//! `ServerMessage::Welcome`, and a client has nowhere to state its own. A capability a client
//! could assert is a capability a client can forge, and `143` §10 says plainly that the
//! provider does not trust a client-supplied role label. The *token* travels (opaque bytes);
//! the *claims* only ever come out of verification.
//!
//! What a client may assume from its copy is therefore: **this is what the relay will let me
//! do**, useful for disabling a control with a reason, and not an authority. The authority is
//! the relay's own copy, and it refuses regardless of what the chrome offered.
//!
//! # What this enforces exactly, and what it cannot
//!
//! [`refuse_if_not_permitted`] takes the document as an `Option`, because the two boundaries
//! that call it hold different information and must not have two rules:
//!
//! - **With a document** — every honest replica, at the facade's choke point. The `comment`
//!   and `suggest` classes are decided by ADR-052's projection equality, exactly and with no
//!   heuristic.
//! - **Without one** — the relay. ADR-047 makes it hold no document, so it can only judge what
//!   an operation's *variant* admits. That answer is **strictly weaker** and never refuses
//!   something a replica would allow, which
//!   `the_relay_s_document_free_answer_never_refuses_what_a_replica_allows` pins.
//!
//! So the line the relay can hold against a **modified client** is write versus no-write, plus
//! "that was definitely not a comment". The finer classes are enforced by every replica and
//! reflected by the chrome; against an adversary who rewrote their own client they are policy,
//! in exactly the sense ADR-052 uses the word. Saying so is the point: a boundary claimed and
//! not held is worse than one that was never claimed.
//!
//! # Complexity
//!
//! O(the inlines the operations name) with a document — it reuses
//! [`protection`](crate::protection)'s projections and adds no walk of its own. O(operations)
//! without one. This runs on the keystroke path, so it may not walk the document (`107` §4 B1).

use casual_doc_model::v1::Document;
use serde::{Deserialize, Serialize};

use crate::Operation;
use crate::protection::{is_comment_only, is_review_only, is_tracked_only};

/// What one participant may do in one document session.
///
/// Built by the **host**, carried inside a signed token, and handed to the engine only after
/// that token has been verified. There is deliberately no `from_wire`-shaped constructor: the
/// only ways to make one are the named presets below and
/// [`narrowed_to`](Capabilities::narrowed_to), so a value of this type is always something
/// somebody decided rather than something that arrived.
///
/// # Reading the room is not a capability
///
/// Everyone admitted to a room may read it — admission *is* the view right, and a separate
/// `view` flag would be a flag nothing could ever be false for. `143` §10's `view` class is
/// therefore the floor rather than a bit.
///
/// # Why named `bool` fields and not a bitmask
///
/// The wire is JSON (ADR-057), so named fields are self-describing in a frame dump and each
/// one carries `#[serde(default)]`: a peer reading a field it does not know about **ignores**
/// it, and a peer missing a field a newer sender omitted reads `false`. Both directions
/// therefore fail towards *less* access, which is the only safe direction for a permission to
/// drift in. A bitmask would make an unknown bit invisible and an endianness or width change
/// silent.
///
/// # What is deliberately not here
///
/// `143` §10 also lists `history.read`, `history.restore` and `share.admin`. None of them is
/// here, and — unlike `review`, which is now a field — that is a **decision** rather than a
/// deferral, because none of the three has anything in the closed operation set to check:
///
/// - **`share.admin`** changes a room's grants, and `143` §10 says in the same line that it is
///   *host-side only*. A room's policy is an [`Access`](../../casual_doc_relay/access/enum.Access.html)
///   value handed to the relay at construction and no message changes it, so there is no
///   operation and no wire frame for a capability bit to gate. A bit here would gate nothing.
/// - **`history.read`** is a read. Admission to a room *is* the view right — see above — and
///   version listing and diffing (`140`) are reads of storage rather than operations, so the
///   check belongs where the storage call is, not in a vocabulary about mutations.
/// - **`history.restore`** arrives as **ordinary operations**: a restore is a transaction of
///   edits, and `Origin` has `Edit`, `Undo` and `Redo` and no `Restore`. Separating it would
///   mean gating on a label the *client* writes, which is forgeable and therefore not a
///   boundary at all — the same reason `Capabilities` never travels from a client. So a restore
///   is exactly `edit`, deliberately, and a host that wants it narrower enforces that at the
///   surface that reads the version.
///
/// **`review` was here until 2026-10-04 and is now a field.** The note said no exact rule could
/// separate a reviewer from an editor, because `protection::is_tracked_only` is a *negative*
/// test that accept, reject and untracked typing all fail alike. That was a true statement about
/// `is_tracked_only` and a false conclusion about the question: the positive rule is *a revision
/// disappeared, none appeared, and one of the two projections is unchanged*, which needs no
/// heuristic and no third projection. `protection::is_review_only` has it, with the one case it
/// narrows named there.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct Capabilities {
    #[serde(default)]
    comment: bool,
    #[serde(default)]
    suggest: bool,
    #[serde(default)]
    review: bool,
    #[serde(default)]
    edit: bool,
    #[serde(default)]
    manage_protection: bool,
}

impl Capabilities {
    /// A participant who may read and nothing else — `143` §10's `view` role.
    #[must_use]
    pub const fn viewer() -> Self {
        Self {
            comment: false,
            suggest: false,
            review: false,
            edit: false,
            manage_protection: false,
        }
    }

    /// Read, and add or remove comments.
    #[must_use]
    pub const fn commenter() -> Self {
        Self::viewer().with_comment()
    }

    /// Read, comment, and propose tracked changes.
    ///
    /// A superset of [`Capabilities::commenter`], matching Word's own restriction levels,
    /// where the tracked-changes level permits comments.
    #[must_use]
    pub const fn suggester() -> Self {
        Self::commenter().with_suggest()
    }

    /// Read, comment, and decide other people's tracked changes — `143` §10's `review`.
    ///
    /// **Not** a superset of [`Capabilities::suggester`], and that asymmetry is the role: a
    /// reviewer resolves suggestions and does not author them, which is exactly what
    /// `protection::is_review_only` tests. A host wanting a reviewer who may also suggest
    /// composes it — `Capabilities::suggester().with_review()` — rather than having the wider
    /// right handed over silently, because a preset that grants more than its name says is how
    /// this capability spent its first increment.
    ///
    /// Commenting is included because `143` §10 defines the class as "accept/reject suggestions
    /// **and moderate comments**", and because resolving a suggestion without being able to
    /// answer the comment thread attached to it is not a reviewable document.
    #[must_use]
    pub const fn reviewer() -> Self {
        Self::commenter().with_review()
    }

    /// Read, comment, suggest, review, and change content directly.
    ///
    /// `review` is included because an editor can already produce any state a reviewer can, so
    /// withholding the bit would make the bit a lie rather than a restriction.
    ///
    /// **Not** permission to change the document's protection: that is
    /// [`Capabilities::with_manage_protection`], and the split is the whole point of this
    /// module — see [`crate::protection::exempt_from_protection`].
    #[must_use]
    pub const fn editor() -> Self {
        Self::suggester().with_review().with_edit()
    }

    /// Everything, including imposing and lifting `w:documentProtection`.
    #[must_use]
    pub const fn owner() -> Self {
        Self::editor().with_manage_protection()
    }

    /// The authority a reader has over a document on their own machine: all of it.
    ///
    /// **This is the standalone mode, and it is the same value as [`Capabilities::owner`] on
    /// purpose.** `152` §2a: a standalone document needs no server and never will, so there is
    /// no grant, nobody to issue one, and the local reader is the only authority — which is
    /// what Word does with an unpassworded restriction and what ADR-052 already decided. It is
    /// a *separate named constructor* rather than a use of `owner()` so that every call site
    /// that means "there is no session here" says so, and a search for the standalone
    /// assumption finds them all.
    #[must_use]
    pub const fn local() -> Self {
        Self::owner()
    }

    /// The same capabilities plus commenting.
    #[must_use]
    pub const fn with_comment(mut self) -> Self {
        self.comment = true;
        self
    }

    /// The same capabilities plus proposing tracked changes.
    #[must_use]
    pub const fn with_suggest(mut self) -> Self {
        self.suggest = true;
        self
    }

    /// The same capabilities plus deciding other people's tracked changes.
    #[must_use]
    pub const fn with_review(mut self) -> Self {
        self.review = true;
        self
    }

    /// The same capabilities plus direct content change.
    #[must_use]
    pub const fn with_edit(mut self) -> Self {
        self.edit = true;
        self
    }

    /// The same capabilities plus imposing and lifting the document's protection.
    #[must_use]
    pub const fn with_manage_protection(mut self) -> Self {
        self.manage_protection = true;
        self
    }

    /// The capabilities held by **both**, which is how a host narrows a role.
    ///
    /// `143` §10: "Role presets are convenience. The wire carries explicit capabilities so a
    /// host can narrow a role without inventing a new role vocabulary." Intersection is the
    /// only composition offered, deliberately: a union would let one grant widen another, and
    /// a narrowing operation that can widen is not a narrowing operation.
    #[must_use]
    pub const fn narrowed_to(self, other: Self) -> Self {
        Self {
            comment: self.comment && other.comment,
            suggest: self.suggest && other.suggest,
            review: self.review && other.review,
            edit: self.edit && other.edit,
            manage_protection: self.manage_protection && other.manage_protection,
        }
    }

    /// Whether comments may be added, edited or removed.
    #[must_use]
    pub const fn may_comment(self) -> bool {
        self.comment
    }

    /// Whether tracked changes may be proposed.
    #[must_use]
    pub const fn may_suggest(self) -> bool {
        self.suggest
    }

    /// Whether other people's tracked changes may be accepted or rejected.
    #[must_use]
    pub const fn may_review(self) -> bool {
        self.review
    }

    /// Whether content may be changed directly.
    #[must_use]
    pub const fn may_edit(self) -> bool {
        self.edit
    }

    /// Whether `w:documentProtection` may be imposed or lifted.
    #[must_use]
    pub const fn may_manage_protection(self) -> bool {
        self.manage_protection
    }

    /// Whether this participant may change the document **at all**.
    ///
    /// The one line a relay that holds no document can hold against a modified client, and the
    /// reason `Refusal::ReadOnlyAccess` exists as a single code rather than one per class.
    #[must_use]
    pub const fn may_write(self) -> bool {
        self.comment || self.suggest || self.review || self.edit || self.manage_protection
    }

    /// Whether this and `other` share at least one capability.
    #[must_use]
    const fn intersects(self, other: Self) -> bool {
        (self.comment && other.comment)
            || (self.suggest && other.suggest)
            || (self.review && other.review)
            || (self.edit && other.edit)
            || (self.manage_protection && other.manage_protection)
    }
}

/// Why a participant's access level refuses an operation.
///
/// One value per class, so a host can route the reason rather than showing one sentence for
/// four different refusals — the same contract [`crate::protection::ProtectionRefusal`]
/// satisfies, and for the same reason.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum AccessRefusal {
    /// This participant may read and nothing else.
    ReadOnly,
    /// This participant may comment, and the operation was not a comment.
    CommentsOnly,
    /// This participant may comment and suggest, and the operation was neither.
    SuggestionsOnly,
    /// This participant may comment and decide other people's tracked changes, and the
    /// operation was neither.
    ReviewOnly,
    /// This participant may not change the document's protection.
    NoProtectionChange,
}

impl AccessRefusal {
    /// The refusal as the reader sees it, carrying its stable routing code.
    ///
    /// See [`crate::refusal`] for the contract this string satisfies. The codes are a
    /// `session.*` family rather than `document.*`, because the cause is who the reader is and
    /// not what the file asks: a host showing "this document is protected" for a read-only
    /// *guest* sends them to look at the wrong thing.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::ReadOnly => crate::refused!(
                "session.read-only-access",
                "You have read-only access to this document."
            ),
            Self::CommentsOnly => crate::refused!(
                "session.comments-only-access",
                "You can add comments to this document, but not change it."
            ),
            Self::SuggestionsOnly => crate::refused!(
                "session.suggestions-only-access",
                "You can comment and suggest changes to this document, but not change it \
                 directly."
            ),
            Self::ReviewOnly => crate::refused!(
                "session.review-only-access",
                "You can accept or reject other people's changes and add comments, but not \
                 change this document yourself."
            ),
            Self::NoProtectionChange => crate::refused!(
                "session.no-protection-change",
                "You are not allowed to change how this document is protected."
            ),
        }
    }
}

/// Any **one** of these capabilities admits `op`, judged without looking at a document.
///
/// This is the relay's whole vocabulary (ADR-047: it holds no document), and it is a
/// *necessary* condition rather than a sufficient one. [`refuse_if_not_permitted`] sharpens it
/// where a document is available.
///
/// # Why a catch-all arm here, when `protection`'s matches are exhaustive
///
/// Because the default runs the other way. In `protection::is_comment_only` a `_ =>` arm would
/// **exempt** a 59th operation from every restriction, so exhaustiveness is the guard. Here the
/// catch-all answers "this needs `edit`", which is the **strictest** answer available and the one a
/// content-changing operation deserves; a new operation therefore arrives governed, and an
/// exhaustive list would buy fifty-six arms and no safety. The two operations that are not
/// plain content changes are named explicitly, and they are the only two the op set has.
///
/// O(1).
#[must_use]
pub const fn admitted_by(op: &Operation) -> Capabilities {
    match op {
        // The one operation whose authority question is about the policy itself (ADR-059).
        Operation::SetDocumentProtection { .. } => Capabilities::viewer().with_manage_protection(),
        // Commenting, suggesting, accepting and untracked typing all travel as this one
        // operation (ADR-052), and without a document they cannot be told apart. So the
        // weakest capability that could legitimately have sent it admits it here, and the
        // document-aware pass decides which it actually was.
        Operation::UpdateReviewState { .. } => Capabilities::editor(),
        _ => Capabilities::viewer().with_edit(),
    }
}

/// Refuses `ops` when this participant's capabilities do not admit them.
///
/// Pass `Some(document)` wherever one is available — every replica has one, and the `comment`
/// and `suggest` classes can only be decided exactly with it. Pass `None` from a boundary that
/// holds no document; the answer is then the weaker variant-level one and the module docs say
/// what that does and does not hold.
///
/// Judged **whole, on its worst operation**, like ADR-052's batch rule and for the same
/// reason: otherwise a permitted comment carries an edit in behind it.
///
/// # Errors
///
/// The [`AccessRefusal`] for the class that refused, naming what the participant may do rather
/// than which of fifty-eight operations their gesture became.
///
/// # Complexity
///
/// O(the inlines the operations name) with a document; O(`ops`) without one. No walk of its
/// own — `107` §4 B1.
pub fn refuse_if_not_permitted(
    document: Option<&Document>,
    ops: &[Operation],
    capabilities: Capabilities,
) -> Result<(), AccessRefusal> {
    for op in ops {
        if !capabilities.intersects(admitted_by(op)) {
            return Err(refusal_for(capabilities, op));
        }
        // `edit` admits every content change outright, so the only operation left to sharpen
        // is the review vehicle — and only for a participant who does not hold `edit`.
        if capabilities.may_edit() || !matches!(op, Operation::UpdateReviewState { .. }) {
            continue;
        }
        let Some(document) = document else {
            // The relay's answer: it got this far, so some capability admits the variant, and
            // nothing more can be decided without the document. Deliberately permissive —
            // refusing here would refuse a legitimate commenter's comment.
            continue;
        };
        // ADR-052's exact projections, reused rather than re-derived. A tracked proposal is
        // judged first because Word's tracked-changes level is a superset of its comments
        // level, so a suggester's comment satisfies `is_tracked_only` too. `review` is a
        // *disjoint* class rather than a rung on the same ladder — a reviewer resolves what it
        // did not author — so it is its own clause and not an ordering of the others.
        let permitted = (capabilities.may_suggest() && is_tracked_only(document, op))
            || (capabilities.may_review() && is_review_only(document, op))
            || (capabilities.may_comment() && is_comment_only(document, op));
        if !permitted {
            return Err(refusal_for(capabilities, op));
        }
    }
    Ok(())
}

/// Which refusal to report, from what the participant holds rather than from what they sent.
///
/// The reader needs to know what they are allowed to do; naming the operation tells them what
/// the chrome happened to send.
fn refusal_for(capabilities: Capabilities, op: &Operation) -> AccessRefusal {
    if matches!(op, Operation::SetDocumentProtection { .. })
        && !capabilities.may_manage_protection()
    {
        return AccessRefusal::NoProtectionChange;
    }
    // Widest write class held first, because the reader needs to know what they *may* do. A
    // participant holding both `suggest` and `review` is told about suggesting: it is the
    // class whose gestures are the commoner ones, and the sentence does not claim the other
    // is absent.
    if capabilities.may_suggest() {
        AccessRefusal::SuggestionsOnly
    } else if capabilities.may_review() {
        AccessRefusal::ReviewOnly
    } else if capabilities.may_comment() {
        AccessRefusal::CommentsOnly
    } else {
        AccessRefusal::ReadOnly
    }
}

#[cfg(test)]
#[path = "access_tests.rs"]
mod tests;
