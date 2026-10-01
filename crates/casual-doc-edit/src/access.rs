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
use crate::protection::{is_comment_only, is_tracked_only};

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
/// `143` §10 lists `review`, `history.read`, `history.restore` and `share.admin` as well.
/// None of them is here, because none has an enforcement point in this crate yet and a
/// capability nothing enforces is the "modeled is not shipped" failure the working contract
/// names:
///
/// - **`review`** (accept/reject a tracked change) would need a *positive* classifier for an
///   accept or a reject. [`is_tracked_only`] is the negative one: it says an operation only
///   *added* tracked marks. Accepting and rejecting both fail it, and so does untracked typing,
///   so there is no exact test that separates a reviewer's gesture from an editor's. ADR-052
///   built the two projections it could make exact and no third; inventing a heuristic here
///   would break that rule for a flag no caller has asked for. A reviewer is therefore granted
///   [`Capabilities::editor`] today, which is **wider** than the role, and that is recorded
///   rather than hidden.
/// - **`history.read` / `history.restore` / `share.admin`** are host-side and version-history
///   surfaces (`140`), not operations in this crate's set, so there is nothing here to check
///   them against.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct Capabilities {
    #[serde(default)]
    comment: bool,
    #[serde(default)]
    suggest: bool,
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

    /// Read, comment, suggest, and change content directly.
    ///
    /// **Not** permission to change the document's protection: that is
    /// [`Capabilities::with_manage_protection`], and the split is the whole point of this
    /// module — see [`crate::protection::exempt_from_protection`].
    #[must_use]
    pub const fn editor() -> Self {
        Self::suggester().with_edit()
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
        self.comment || self.suggest || self.edit || self.manage_protection
    }

    /// Whether this and `other` share at least one capability.
    #[must_use]
    const fn intersects(self, other: Self) -> bool {
        (self.comment && other.comment)
            || (self.suggest && other.suggest)
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
/// Because the default runs the other way. In [`is_comment_only`] a `_ =>` arm would **exempt**
/// a 59th operation from every restriction, so exhaustiveness is the guard. Here the catch-all
/// answers "this needs `edit`", which is the **strictest** answer available and the one a
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
        // ADR-052's two exact projections, reused rather than re-derived. A tracked proposal is
        // judged first because Word's tracked-changes level is a superset of its comments
        // level, so a suggester's comment satisfies `is_tracked_only` too.
        let permitted = (capabilities.may_suggest() && is_tracked_only(document, op))
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
    if capabilities.may_suggest() {
        AccessRefusal::SuggestionsOnly
    } else if capabilities.may_comment() {
        AccessRefusal::CommentsOnly
    } else {
        AccessRefusal::ReadOnly
    }
}

#[cfg(test)]
#[path = "access_tests.rs"]
mod tests;
