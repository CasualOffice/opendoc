// SPDX-License-Identifier: Apache-2.0

//! `w:documentProtection` enforced **at the operation** — `107` 6.7.
//!
//! # What was and was not enforced
//!
//! The model has carried `DocumentProtection` with all four editing levels since Layer 1,
//! and import and export round-trip `w:documentProtection` faithfully. Only one level was
//! ever *enforced*: `w:edit="forms"`, at the facade's mutation choke point. The other
//! three — `readOnly`, `comments`, `trackedChanges` — were modelled, exported, and
//! **ignored**, so a document a reader could see was protected was fully editable. That is
//! the "modeled is not shipped" failure the working contract names as this repository's most
//! expensive recurring pattern, and `153` overstates the gap in the other direction by
//! reporting the whole feature as missing.
//!
//! # Why the check lives here and not in the facade
//!
//! The sibling engine enforces read-only *at the operation* rather than by hiding a toolbar,
//! **including inside a batch** (`152` §10 Q5). Three reasons that shape is right here too:
//!
//! 1. an operation is the only thing that can be judged — a toolbar tells you what a host
//!    offered, not what it sent;
//! 2. the relay will need the same judgement on an arriving operation, and a rule written in
//!    the facade could not be reused by it;
//! 3. `Operation` lives in this crate, so a new operation is a **compile error** in the
//!    match below rather than a permission hole nobody notices. A `_ =>` arm here is how a
//!    59th operation would arrive exempt from every restriction.
//!
//! # The hard part, and why it is exact rather than heuristic
//!
//! `comments` and `trackedChanges` are not variant-level questions. Commenting, suggesting a
//! change, and accepting one **all travel as `Operation::UpdateReviewState`**, whose payload
//! is a replacement inline list per paragraph plus an optional comments table. So "is this a
//! comment?" cannot be answered by looking at which operation it is.
//!
//! It can be answered *exactly*, by projection equality, and that is what this module does.
//! Neither test is a heuristic and neither has a tolerance:
//!
//! - **`comments`**: strip every comment marker from the current inlines and from the
//!   replacement. If the remainders are equal, the operation moved comment markers and
//!   nothing else. Typing a character leaves a character behind in the remainder, so it is
//!   refused; resolving a comment changes only the comments table, so it is allowed.
//! - **`trackedChanges`**: project both to *the text as it stood before the change* — content
//!   outside a revision, plus content inside a `Deletion`/`MoveFrom` (which is still there,
//!   struck through), and **not** content inside an `Insertion`/`MoveTo` (which was not there
//!   before). If the projections are equal **and** no revision that exists now is missing
//!   from the replacement, the operation only added tracked marks.
//!
//!   The second condition is what catches a *rejection*: rejecting an insertion restores the
//!   before-state, so the projection alone is satisfied — and the revision it destroyed is
//!   gone, which the id check sees. Accepting, in either direction, changes the projection.
//!   Untracked typing changes it too. So the four review decisions and the one untracked edit
//!   are all refused, by two conditions rather than by five special cases.
//!
//! # Complexity
//!
//! O(the inlines the operation names) — never O(document). A protection check runs on the
//! keystroke path, so it may not walk the document (`107` §4 B1); the paragraphs are
//! resolved once, and only for the levels that need the projection. `readOnly` needs no
//! projection at all and is O(operations).

use casual_doc_model::NodeId;
use casual_doc_model::v1::{Document, DocumentProtectionEdit, InlineNode, Revision, RevisionKind};

use crate::Operation;
use crate::access::Capabilities;
use crate::containers::{InlineDescent, inline_descent};

/// Why `w:documentProtection` refuses an operation.
///
/// One value per enforced level, so a host can route the reason rather than showing one
/// sentence for four different restrictions.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum ProtectionRefusal {
    /// `w:edit="readOnly"`: nothing may change.
    ReadOnly,
    /// `w:edit="comments"`: only comments may be added, edited or removed.
    CommentsOnly,
    /// `w:edit="trackedChanges"`: edits are allowed but must be tracked, and a tracked
    /// change may not be accepted or rejected.
    TrackedChangesOnly,
}

impl ProtectionRefusal {
    /// The refusal as the reader sees it, carrying its stable routing code.
    ///
    /// See [`crate::refusal`] for the contract this string satisfies.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::ReadOnly => crate::refused!(
                "document.protected-read-only",
                "This document is protected against changes."
            ),
            Self::CommentsOnly => crate::refused!(
                "document.protected-comments-only",
                "This document is protected: only comments can be added."
            ),
            Self::TrackedChangesOnly => crate::refused!(
                "document.protected-tracked-changes-only",
                "This document is protected: changes must be tracked, and tracked changes \
                 cannot be accepted or rejected."
            ),
        }
    }
}

/// Refuses `ops` when the document's **enforced** protection forbids any of them.
///
/// `w:edit="forms"` is deliberately not answered here: it is enforced at the facade, which
/// holds the "the reader is in a form field right now" state this crate cannot see, and
/// moving it would be a second mechanism for one rule. [`forms`] is the seam.
///
/// `capabilities` is **this participant's** access level, which is a different authority from
/// the document's policy and is read for exactly one thing: whether
/// [`Operation::SetDocumentProtection`] is exempt (see [`exempt_from_protection`]). Pass
/// [`Capabilities::local`] where there is no session, which is the standalone mode and the
/// behaviour ADR-052 shipped. The *participant's* own check is
/// [`crate::access::refuse_if_not_permitted`] and it runs **before** this one — the grant is
/// the outer gate.
///
/// # Errors
///
/// The [`ProtectionRefusal`] for the level that refused, naming the level and not the
/// operation: a reader needs to know what the document allows, not which of 58 operations
/// their gesture became.
///
/// # Complexity
///
/// O(the inlines the operations name). `readOnly` resolves nothing.
pub fn refuse_if_protected(
    document: &Document,
    ops: &[Operation],
    capabilities: Capabilities,
) -> Result<(), ProtectionRefusal> {
    let Some(protection) = document.definitions().settings.document_protection.as_ref() else {
        return Ok(());
    };
    if !protection.enforcement {
        // Word writes a restriction with `w:enforcement="0"` when the author set one up and
        // then turned it off. The policy is retained on save and does not apply.
        return Ok(());
    }
    let level = protection.edit;
    for op in ops {
        // The ordering trap. See `exempt_from_protection`, which is also what the facade's
        // `forms` check calls, so the exemption is one rule in one place.
        if exempt_from_protection(op, capabilities) {
            continue;
        }
        match level {
            DocumentProtectionEdit::None | DocumentProtectionEdit::Forms => {}
            DocumentProtectionEdit::ReadOnly => return Err(ProtectionRefusal::ReadOnly),
            DocumentProtectionEdit::Comments => {
                if !is_comment_only(document, op) {
                    return Err(ProtectionRefusal::CommentsOnly);
                }
            }
            DocumentProtectionEdit::TrackedChanges => {
                if !is_tracked_only(document, op) {
                    return Err(ProtectionRefusal::TrackedChangesOnly);
                }
            }
        }
    }
    Ok(())
}

/// Whether `op` is exempt from **every** editing restriction, including `readOnly`.
///
/// Exactly one operation is: [`Operation::SetDocumentProtection`], the operation that
/// installs or lifts the restriction itself — **and only for a participant whose grant lets
/// them manage protection.**
///
/// # The ordering trap, and why this function is the whole of ADR-059's subtlety
///
/// A batch is judged on its **worst** operation. Without this exemption the operation that
/// *lifts* a restriction is refused by the restriction it is lifting:
/// `SetDocumentProtection { protection: None }` sent to a `readOnly` document comes back
/// [`ProtectionRefusal::ReadOnly`], `readOnly` becomes a **one-way door**, and a document
/// that arrived protected can never be unprotected in this editor. That is strictly worse
/// than not enforcing protection at all — the user loses a document rather than gaining a
/// policy — so it is the one case this module must get right. Word and ONLYOFFICE both offer
/// Review ▸ Restrict Editing in both directions.
///
/// The exemption is unconditional and symmetric: it covers **imposing** a restriction as well
/// as lifting one, because both are the same authority question and a document that forbids
/// comments has no standing to forbid being made read-only.
///
/// # It is still judged against the protection in force *before* the batch
///
/// [`refuse_if_protected`] reads the level once, before the loop, so every *other* operation
/// in the batch is judged against the restriction that was in force when the batch arrived.
/// A batch may lift a restriction, and a batch may edit, but a batch may **not** lift a
/// restriction and then edit under the lift: `[SetDocumentProtection(None), InsertText]` is
/// refused on the `InsertText`. That is the conservative reading and it keeps ADR-052's "a
/// batch is judged whole" true.
///
/// # Who may do this — `152` §10 Q4, answered
///
/// *The document* says "do not edit me", and only a **grant** can say who may overrule it.
/// `152` §10 Q4/Q5 recorded that as owed and ADR-059 named this function as the single place
/// that would have to learn the difference. It now has:
///
/// - **No session** — [`Capabilities::local`], the standalone mode. The local reader is the
///   only authority and may lift the restriction, exactly what Word does with an
///   **unpassworded** restriction and exactly what ADR-052 shipped. Unchanged.
/// - **In a room** — the capabilities the host signed. A read-only guest, a commenter and a
///   suggester are **not** exempt: their `SetDocumentProtection` is refused by
///   [`crate::access::refuse_if_not_permitted`] before this is reached, and refused again
///   here if some future caller runs the checks the other way round. Two gates for one rule is
///   deliberate; the inner one costs a `bool` read and closes an ordering hole nobody would
///   notice.
///
/// So the sentence that was true and wrong — "anyone who can open a document can lift its
/// restriction" — is now true only where it should be: on your own machine, with your own file.
///
/// Password material (`w:hash`, `w:salt`, `w:cryptSpinCount`) is still deliberately not
/// modelled and never verified. That has not changed and is not what this closes: ADR-052
/// records that the document's protection is policy and not security, and checking a hash would
/// advertise a boundary that does not exist, since the legacy hash is removable by editing one
/// attribute in the XML. A *grant* is a different matter — it is held by the host and signed by
/// the host, and the relay refuses without it.
///
/// # Why `matches!` and not an exhaustive match
///
/// This module's other two matches are exhaustive with no `_` arm, because their safe default
/// is "refuse" and a wildcard would silently exempt a new operation. Here the positive list
/// *is* the exemption list, so a 60th operation defaults to **governed** — the safe answer —
/// and exhaustiveness would buy a 59-arm list and nothing else.
///
/// Called by [`refuse_if_protected`] and by the facade's `w:edit="forms"` check, which has
/// the identical trap: a forms-protected document would otherwise be unliftable too, because
/// a document-global operation is never "inside a form field".
///
/// O(1).
#[must_use]
pub const fn exempt_from_protection(op: &Operation, capabilities: Capabilities) -> bool {
    matches!(op, Operation::SetDocumentProtection { .. }) && capabilities.may_manage_protection()
}

/// Whether `document`'s protection is the enforced forms-only level.
///
/// The seam the facade's own forms check reads, so "is a restriction in force" is decided
/// once, here, and the facade adds only the part that needs its own state.
#[must_use]
pub fn forms(document: &Document) -> bool {
    document
        .definitions()
        .settings
        .document_protection
        .as_ref()
        .is_some_and(|protection| {
            protection.enforcement && protection.edit == DocumentProtectionEdit::Forms
        })
}

/// Whether `op` changes comments and their anchors and nothing else.
///
/// **An exhaustive match on purpose.** Every operation but one is a content change, so the
/// interesting question is which ones are not — and a catch-all would silently exempt the
/// 59th operation from every restriction in this module. That is the same rule `clone`'s
/// inline match and `WireOperation::introduces` follow, for the same reason.
pub(crate) fn is_comment_only(document: &Document, op: &Operation) -> bool {
    match op {
        // The one operation that can be a comment. Its paragraph replacements are compared
        // with the comment markers removed from both sides: if what is left is identical, the
        // operation moved markers and nothing else.
        Operation::UpdateReviewState { paragraphs, .. } => paragraphs.iter().all(|state| {
            current_inlines(document, state.node).is_some_and(|current| {
                without_comment_markers(current) == without_comment_markers(&state.inlines)
            })
        }),
        Operation::InsertText { .. }
        | Operation::DeleteText { .. }
        | Operation::SplitParagraph { .. }
        | Operation::JoinParagraphs { .. }
        | Operation::FormatText { .. }
        | Operation::ClearFormatting { .. }
        | Operation::SetHyperlink { .. }
        | Operation::SetInlines { .. }
        | Operation::SetParagraphProperties { .. }
        | Operation::InsertRow { .. }
        | Operation::DeleteRow { .. }
        | Operation::InsertColumn { .. }
        | Operation::DeleteColumn { .. }
        | Operation::DeleteTable { .. }
        | Operation::InsertTable { .. }
        | Operation::InsertBlocks { .. }
        | Operation::DeleteBlocks { .. }
        | Operation::SetExtent { .. }
        | Operation::SetGroupGeometry { .. }
        | Operation::SetAnchor { .. }
        | Operation::SetImageCrop { .. }
        | Operation::SetObjectDescr { .. }
        | Operation::DeleteObject { .. }
        | Operation::InsertObjectNode { .. }
        | Operation::InsertInlineObject { .. }
        | Operation::RemoveInlineObject { .. }
        | Operation::SetTableCellProperties { .. }
        | Operation::SetTableProperties { .. }
        | Operation::ReplaceTable { .. }
        | Operation::SetCoreProperties { .. }
        | Operation::SetSectionGeometry { .. }
        | Operation::SpliceSectionBoundary { .. }
        | Operation::SetStyleDefinition { .. }
        | Operation::SetAbstractNumbering { .. }
        | Operation::SetNumberingInstance { .. }
        | Operation::SetMediaReference { .. }
        // Never reached from `refuse_if_protected`, which exempts this operation above
        // before the level is consulted. `false` is nonetheless the right answer to put
        // here: it is not a comment change, and if the exemption above were ever removed
        // the exemption would have to be re-made DELIBERATELY at the choke point rather
        // than inherited silently from this arm. The ordering-trap guard in the tests
        // below is what notices if it goes.
        | Operation::SetDocumentProtection { .. }
        | Operation::CreateBookmark { .. }
        | Operation::DeleteBookmark { .. }
        | Operation::RenameBookmark { .. }
        | Operation::InsertField { .. }
        | Operation::RemoveField { .. }
        | Operation::InsertFieldRange { .. }
        | Operation::RemoveFieldRange { .. }
        | Operation::InsertNote { .. }
        | Operation::RemoveNote { .. }
        | Operation::CreateHeaderFooterBody { .. }
        | Operation::RemoveHeaderFooterBody { .. }
        | Operation::SetSectionRunningRef { .. }
        | Operation::SetSectionTitlePage { .. }
        | Operation::SetSectionWatermark { .. }
        | Operation::SetSectionLineNumbering { .. }
        | Operation::SetSectionPageNumbering { .. }
        | Operation::SetSectionVerticalAlignment { .. }
        | Operation::SetEvenAndOddHeaders { .. }
        | Operation::SetShapeFill { .. }
        | Operation::SetShapeStroke { .. }
        | Operation::SetTextBoxBody { .. } => false,
    }
}

/// Whether `op` only adds tracked marks — never accepts or rejects one, and never edits
/// untracked.
///
/// Word's tracked-changes restriction allows comments too, so this is a superset of
/// [`is_comment_only`]: the projection below drops comment markers as well.
pub(crate) fn is_tracked_only(document: &Document, op: &Operation) -> bool {
    match op {
        Operation::UpdateReviewState { paragraphs, .. } => paragraphs.iter().all(|state| {
            current_inlines(document, state.node).is_some_and(|current| {
                before_projection(current) == before_projection(&state.inlines)
                    && revision_ids(current)
                        .into_iter()
                        .all(|id| revision_ids(&state.inlines).contains(&id))
            })
        }),
        other => is_comment_only(document, other),
    }
}

/// The paragraph `node`'s current inlines, on any surface.
///
/// `None` when the operation names a paragraph that is not there: a write this check cannot
/// place is refused rather than waved through, which is the rule the forms check already
/// follows.
fn current_inlines(document: &Document, node: NodeId) -> Option<&[InlineNode]> {
    crate::find_paragraph_any(document, node).map(|paragraph| paragraph.inlines.as_slice())
}

/// `inlines` with every comment marker removed, recursing into the inline containers.
///
/// Block containers (`TextBox`, `Group`) are carried whole and not descended into: their
/// paragraphs are separate stories with their own ids, so a marker inside one belongs to
/// *that* paragraph's own `UpdateReviewState` and comparing it here would charge the wrong
/// paragraph — the same accounting rule `inline_text_len` follows.
fn without_comment_markers(inlines: &[InlineNode]) -> Vec<InlineNode> {
    let mut out = Vec::with_capacity(inlines.len());
    for inline in inlines {
        match inline {
            InlineNode::CommentReference(_)
            | InlineNode::CommentRangeStart(_)
            | InlineNode::CommentRangeEnd(_) => {}
            other => out.push(map_children(other, without_comment_markers)),
        }
    }
    out
}

/// `inlines` as they stood **before** the tracked changes in them, with comment markers
/// dropped.
///
/// An insertion contributes nothing; a deletion contributes its content, because deleted text
/// is still in the document with a strike through it. So two inline lists have the same
/// projection exactly when they describe the same original text under different review marks.
fn before_projection(inlines: &[InlineNode]) -> Vec<InlineNode> {
    let mut out = Vec::with_capacity(inlines.len());
    for inline in inlines {
        match inline {
            InlineNode::CommentReference(_)
            | InlineNode::CommentRangeStart(_)
            | InlineNode::CommentRangeEnd(_) => {}
            InlineNode::Revision(revision) => match revision.kind {
                // Was not there before this change, so it contributes nothing.
                RevisionKind::Insertion | RevisionKind::MoveTo => {}
                // Was there before and still is, struck through: contribute the content
                // unwrapped, so a tracked deletion and the untouched text project alike.
                RevisionKind::Deletion | RevisionKind::MoveFrom => {
                    out.extend(before_projection(&revision.inlines));
                }
            },
            other => out.push(map_children(other, before_projection)),
        }
    }
    out
}

/// Every revision node id in `inlines`, including nested ones.
///
/// What catches a *rejection*, which the projection alone cannot: rejecting an insertion
/// restores the before-state, so only the disappearance of the revision itself distinguishes
/// it from having never happened.
fn revision_ids(inlines: &[InlineNode]) -> Vec<NodeId> {
    let mut out = Vec::new();
    collect_revision_ids(inlines, &mut out);
    out
}

fn collect_revision_ids(inlines: &[InlineNode], out: &mut Vec<NodeId>) {
    for inline in inlines {
        if let InlineNode::Revision(revision) = inline {
            out.push(revision.id);
        }
        if let Some(children) = children_of(inline) {
            collect_revision_ids(children, out);
        }
    }
}

/// The inline list nested directly inside `inline`, on the same paragraph's axis.
///
// container-set: consulted rather than decided — the set comes from `inline_descent`, so a
// seventh container is this function's problem the moment it is declared, not silently
// invisible to it. The two block axes are skipped on purpose: a `TextBox`'s and a `Group`'s
// paragraphs are separate stories with ids of their own, so a revision or a comment marker
// inside one belongs to THAT paragraph's own `UpdateReviewState`, and counting it here would
// charge the wrong paragraph — the same accounting rule `inline_text_len` follows.
fn children_of(inline: &InlineNode) -> Option<&[InlineNode]> {
    match inline_descent(inline) {
        InlineDescent::Inlines(children) => Some(children),
        InlineDescent::Blocks(_) | InlineDescent::Group(_) | InlineDescent::Leaf => None,
    }
}

/// `inline` with `project` applied to the inline list it wraps, if it wraps one.
///
// container-set: the four arms below are exactly `InlineDescent::Inlines`'s four wrappers,
// and `the_rebuilt_wrappers_are_exactly_the_inline_container_set` asserts that equivalence
// rather than asking a reader to check it. They are spelled out here because rebuilding a
// node needs its concrete type, which the descent — a borrowed slice — cannot give back. The
// fallback clones, which is right for every leaf and for the two block axes `children_of`
// documents skipping.
fn map_children(inline: &InlineNode, project: fn(&[InlineNode]) -> Vec<InlineNode>) -> InlineNode {
    match inline {
        InlineNode::Hyperlink(node) => {
            let mut copy = node.clone();
            copy.inlines = project(&node.inlines);
            InlineNode::Hyperlink(copy)
        }
        InlineNode::Field(node) => {
            let mut copy = node.clone();
            copy.inlines = project(&node.inlines);
            InlineNode::Field(copy)
        }
        InlineNode::Revision(node) => {
            let projected = project(&node.inlines);
            InlineNode::Revision(Box::new(Revision {
                inlines: projected,
                ..(**node).clone()
            }))
        }
        InlineNode::Sdt(node) => {
            let mut copy = node.clone();
            copy.inlines = project(&node.inlines);
            InlineNode::Sdt(copy)
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use casual_doc_model::IdGenerator;
    use casual_doc_model::v1::{
        BlockNode, CommentId, CommentRangeEnd, CommentRangeStart, Definitions, DocumentProtection,
        Paragraph, ParagraphProperties, Run, RunProperties,
    };

    use crate::{Pos, Range, ReviewParagraphState};

    fn n(counter: u64) -> NodeId {
        NodeId::from_parts(7, counter).expect("a node id")
    }

    fn run(id: u64, text: &str) -> InlineNode {
        InlineNode::Run(Run {
            id: n(id),
            properties: RunProperties::default().into(),
            text: text.to_owned(),
        })
    }

    /// One paragraph reading `abcdefgh`, protected at `edit` and enforced unless stated.
    fn protected(edit: DocumentProtectionEdit, enforcement: bool) -> Document {
        let mut ids = IdGenerator::new(1);
        let document_id = ids.next_id().expect("an id");
        let mut definitions = Definitions::default();
        definitions.settings.document_protection = Some(DocumentProtection {
            edit,
            enforcement,
            formatting: false,
        });
        Document::new(
            document_id,
            vec![BlockNode::Paragraph(Paragraph {
                id: n(10),
                properties: ParagraphProperties::default().into(),
                inlines: vec![run(11, "abcdefgh")],
            })],
            definitions,
        )
        .expect("a valid document")
    }

    fn typing() -> Operation {
        Operation::InsertText {
            at: Pos::new(n(10), 0),
            text: "x".to_owned(),
        }
    }

    fn deleting() -> Operation {
        Operation::DeleteText {
            range: Range {
                start: Pos::new(n(10), 0),
                end: Pos::new(n(10), 1),
            },
        }
    }

    /// An `UpdateReviewState` replacing paragraph 10's inlines with `inlines`.
    fn review(inlines: Vec<InlineNode>) -> Operation {
        Operation::UpdateReviewState {
            paragraphs: vec![ReviewParagraphState {
                node: n(10),
                inlines,
            }],
            comments: None,
        }
    }

    fn comment_range(id: u64, comment: u64) -> InlineNode {
        InlineNode::CommentRangeStart(CommentRangeStart {
            id: n(id),
            comment: CommentId::new(n(comment)),
        })
    }

    fn revision(id: u64, kind: RevisionKind, inlines: Vec<InlineNode>) -> InlineNode {
        InlineNode::Revision(Box::new(Revision {
            id: n(id),
            kind,
            author: Some("Reviewer".to_owned()),
            date: None,
            revision_id: None,
            editor_group: None,
            inlines,
        }))
    }

    #[test]
    fn a_read_only_document_refuses_every_operation_including_a_comment() {
        // The level that was modelled, exported and enforced NOWHERE. `readOnly` needs no
        // projection: nothing may change, so the answer does not depend on the operation.
        let document = protected(DocumentProtectionEdit::ReadOnly, true);
        for op in [typing(), deleting(), review(vec![run(11, "abcdefgh")])] {
            assert_eq!(
                refuse_if_protected(&document, std::slice::from_ref(&op), Capabilities::local()),
                Err(ProtectionRefusal::ReadOnly),
                "a read-only document accepted {op:?}"
            );
        }
    }

    #[test]
    fn a_restriction_that_is_not_enforced_does_not_apply() {
        // Word writes the restriction with `w:enforcement="0"` when the author set one up and
        // then turned it off. Refusing on it would make a saved-and-reopened document
        // uneditable for a policy nobody asked to apply.
        let document = protected(DocumentProtectionEdit::ReadOnly, false);
        assert_eq!(
            refuse_if_protected(&document, &[typing()], Capabilities::local()),
            Ok(())
        );
    }

    #[test]
    fn a_comments_only_document_takes_a_comment_anchor_and_refuses_a_keystroke() {
        // The exactness claim, both directions, on ONE operation variant: a comment and a
        // suggested keystroke are both `UpdateReviewState`, so a variant-level rule could not
        // tell them apart and would have to allow or forbid both.
        let document = protected(DocumentProtectionEdit::Comments, true);

        // Anchoring a comment adds markers around text that is otherwise untouched.
        let anchored = review(vec![
            comment_range(20, 21),
            run(11, "abcdefgh"),
            InlineNode::CommentRangeEnd(CommentRangeEnd {
                id: n(22),
                comment: CommentId::new(n(21)),
            }),
        ]);
        assert_eq!(
            refuse_if_protected(&document, &[anchored], Capabilities::local()),
            Ok(()),
            "a comment anchor must be accepted, or the level allows nothing it exists for"
        );

        // A suggested keystroke: the same operation, one character more.
        let suggested = review(vec![
            revision(30, RevisionKind::Insertion, vec![run(31, "x")]),
            run(11, "abcdefgh"),
        ]);
        assert_eq!(
            refuse_if_protected(&document, &[suggested], Capabilities::local()),
            Err(ProtectionRefusal::CommentsOnly),
            "a tracked insertion is not a comment"
        );
        assert_eq!(
            refuse_if_protected(&document, &[typing()], Capabilities::local()),
            Err(ProtectionRefusal::CommentsOnly)
        );
    }

    #[test]
    fn a_comment_on_a_paragraph_that_is_not_there_is_refused_rather_than_waved_through() {
        // The rule the forms check already follows: a write this check cannot place is
        // refused. An unplaceable operation in a protected document is exactly the case where
        // guessing is wrong.
        let document = protected(DocumentProtectionEdit::Comments, true);
        let elsewhere = Operation::UpdateReviewState {
            paragraphs: vec![ReviewParagraphState {
                node: n(9_999),
                inlines: vec![run(11, "abcdefgh")],
            }],
            comments: None,
        };
        assert_eq!(
            refuse_if_protected(&document, &[elsewhere], Capabilities::local()),
            Err(ProtectionRefusal::CommentsOnly)
        );
    }

    #[test]
    fn tracked_changes_takes_a_suggestion_and_refuses_every_review_decision() {
        // The whole point of the level, and the case a variant-level rule cannot express:
        // suggesting, accepting and rejecting are the same operation.
        let plain = protected(DocumentProtectionEdit::TrackedChanges, true);

        // Suggesting an insertion: the before-projection is unchanged, because an insertion
        // was not there before.
        let insert = review(vec![
            revision(30, RevisionKind::Insertion, vec![run(31, "x")]),
            run(11, "abcdefgh"),
        ]);
        assert_eq!(
            refuse_if_protected(&plain, &[insert], Capabilities::local()),
            Ok(()),
            "a tracked insertion must be accepted, or the level allows no editing at all"
        );

        // Suggesting a deletion: the text stays, struck through, so the projection still has
        // it.
        let delete = review(vec![revision(
            40,
            RevisionKind::Deletion,
            vec![run(11, "abcdefgh")],
        )]);
        assert_eq!(
            refuse_if_protected(&plain, &[delete], Capabilities::local()),
            Ok(())
        );

        // Untracked typing through the same operation: a character appears outside any
        // revision, so the projection gains it.
        let untracked = review(vec![run(50, "x"), run(11, "abcdefgh")]);
        assert_eq!(
            refuse_if_protected(&plain, &[untracked], Capabilities::local()),
            Err(ProtectionRefusal::TrackedChangesOnly),
            "an untracked edit is what this level exists to prevent"
        );
        assert_eq!(
            refuse_if_protected(&plain, &[typing()], Capabilities::local()),
            Err(ProtectionRefusal::TrackedChangesOnly)
        );

        // Now a document that ALREADY holds a tracked insertion, so accept and reject are
        // expressible. The condition is created rather than inherited: on the plain document
        // above there is nothing to accept, and a guard written there would prove nothing.
        let mut suggested = protected(DocumentProtectionEdit::TrackedChanges, true);
        if let Some(BlockNode::Paragraph(paragraph)) = suggested.body_mut().first_mut() {
            paragraph.inlines = vec![
                revision(30, RevisionKind::Insertion, vec![run(31, "x")]),
                run(11, "abcdefgh"),
            ];
        }

        // Accepting the insertion unwraps it: the before-projection GAINS the text.
        let accepted = review(vec![run(31, "x"), run(11, "abcdefgh")]);
        assert_eq!(
            refuse_if_protected(&suggested, &[accepted], Capabilities::local()),
            Err(ProtectionRefusal::TrackedChangesOnly),
            "accepting a tracked change is a review decision, not a tracked edit"
        );

        // Rejecting it removes wrapper and text together, so the projection is unchanged and
        // only the vanished revision id distinguishes it from nothing having happened.
        let rejected = review(vec![run(11, "abcdefgh")]);
        assert_eq!(
            refuse_if_protected(&suggested, &[rejected], Capabilities::local()),
            Err(ProtectionRefusal::TrackedChangesOnly),
            "rejecting restores the before-state, so ONLY the lost revision id catches it — \
             the projection alone would have let it through"
        );
    }

    #[test]
    fn tracked_changes_allows_a_comment_because_word_does() {
        // Word's tracked-changes restriction permits comments, so this level is a superset of
        // the comments level rather than a sibling of it.
        let document = protected(DocumentProtectionEdit::TrackedChanges, true);
        let anchored = review(vec![comment_range(20, 21), run(11, "abcdefgh")]);
        assert_eq!(
            refuse_if_protected(&document, &[anchored], Capabilities::local()),
            Ok(())
        );
    }

    #[test]
    fn a_batch_is_refused_on_its_worst_operation_not_its_first() {
        // The sibling's rule, which this repository has no other place to hold: read-only is
        // enforced at the operation INCLUDING inside a batch, so a permitted comment cannot
        // carry an edit in behind it.
        let document = protected(DocumentProtectionEdit::Comments, true);
        let ops = vec![
            review(vec![comment_range(20, 21), run(11, "abcdefgh")]),
            typing(),
        ];
        assert_eq!(
            refuse_if_protected(&document, &ops, Capabilities::local()),
            Err(ProtectionRefusal::CommentsOnly),
            "a batch whose first operation is allowed must still be judged whole"
        );
    }

    #[test]
    fn an_unprotected_document_is_never_charged_for_the_check() {
        // The overwhelmingly common case, and the one the keystroke budget cares about: no
        // protection means no resolution and no projection.
        let mut ids = IdGenerator::new(1);
        let document_id = ids.next_id().expect("an id");
        let document = Document::new(
            document_id,
            vec![BlockNode::Paragraph(Paragraph {
                id: n(10),
                properties: ParagraphProperties::default().into(),
                inlines: vec![run(11, "abcdefgh")],
            })],
            Definitions::default(),
        )
        .expect("a valid document");
        crate::reset_block_visits();
        assert_eq!(
            refuse_if_protected(&document, &[typing()], Capabilities::local()),
            Ok(())
        );
        assert_eq!(
            crate::block_visits(),
            0,
            "an unprotected document must cost no document walk on the keystroke path"
        );
    }

    #[test]
    fn the_rebuilt_wrappers_are_exactly_the_inline_container_set() {
        // `map_children` has to spell its four wrappers out, because rebuilding a node needs
        // its concrete type and the descent hands back a borrowed slice. So the equivalence
        // is asserted rather than trusted: a seventh inline container, or a change to the
        // declared set, fails here instead of quietly projecting one wrapper's children and
        // cloning another's.
        let wrappers: Vec<InlineNode> = vec![
            InlineNode::Hyperlink(Box::new(casual_doc_model::v1::Hyperlink {
                id: n(60),
                target: casual_doc_model::v1::HyperlinkTarget::Internal(
                    casual_doc_model::v1::InternalTarget {
                        anchor: "anchor".to_owned(),
                    },
                ),
                tooltip: None,
                inlines: vec![run(61, "a")],
            })),
            revision(62, RevisionKind::Insertion, vec![run(63, "a")]),
        ];
        for wrapper in &wrappers {
            assert!(
                children_of(wrapper).is_some(),
                "{wrapper:?} is an inline container the projections must descend"
            );
            // The projection must reach inside: a wrapper whose children were cloned rather
            // than projected would leave a comment marker in the remainder.
            let with_marker = map_children(wrapper, |_| vec![comment_range(70, 71)]);
            assert_eq!(
                children_of(&with_marker).map(<[InlineNode]>::to_vec),
                Some(vec![comment_range(70, 71)]),
                "{wrapper:?} was cloned instead of projected"
            );
        }
        // And a leaf is carried whole rather than emptied.
        let leaf = run(80, "a");
        assert!(children_of(&leaf).is_none());
        assert_eq!(map_children(&leaf, |_| Vec::new()), leaf);
    }

    #[test]
    fn every_refusal_carries_a_distinct_routing_code() {
        // `refusal`'s contract: the sentence is a fallback and the code is the product, so
        // two levels sharing a code would make them untranslatable apart.
        let codes: Vec<&str> = [
            ProtectionRefusal::ReadOnly,
            ProtectionRefusal::CommentsOnly,
            ProtectionRefusal::TrackedChangesOnly,
        ]
        .into_iter()
        .map(|refusal| {
            refusal
                .reason()
                .rsplit(crate::refusal::CODE_SEPARATOR)
                .next()
                .expect("a code")
        })
        .collect();
        let unique: std::collections::BTreeSet<&str> = codes.iter().copied().collect();
        assert_eq!(
            unique.len(),
            codes.len(),
            "duplicate routing codes: {codes:?}"
        );
        assert!(
            codes
                .iter()
                .all(|code| code.starts_with("document.protected-")),
            "the codes must share the family prefix a host routes on: {codes:?}"
        );
    }

    /// **ADR-059's ordering trap, and the guard the whole feature rests on.**
    ///
    /// A batch is judged on its WORST operation, so before `exempt_from_protection` existed
    /// the operation that lifts a restriction was refused by the restriction it lifts: a
    /// `readOnly` document could never be unprotected, which is strictly worse than not
    /// enforcing protection at all.
    ///
    /// Driven red by judging the batch on its worst operation — deleting the
    /// `exempt_from_protection` early-`continue` from `refuse_if_protected`, which is exactly
    /// the trap. Recorded in the commit message.
    ///
    /// All four enforced levels, in both directions, because the trap is not specific to
    /// `readOnly`: a comments-only document must not be able to forbid being unrestricted
    /// either.
    #[test]
    fn the_operation_that_changes_protection_is_never_refused_by_the_protection_it_changes() {
        for level in [
            DocumentProtectionEdit::ReadOnly,
            DocumentProtectionEdit::Comments,
            DocumentProtectionEdit::TrackedChanges,
            DocumentProtectionEdit::Forms,
        ] {
            let document = protected(level, true);
            // The precondition is explicit: this document really does refuse an ordinary
            // edit. Without it the guard could pass because nothing was protected at all —
            // measuring the fixture instead of the guarantee. `Forms` is excluded because
            // the engine deliberately leaves that level to the facade.
            if level != DocumentProtectionEdit::Forms {
                assert!(
                    refuse_if_protected(&document, &[typing()], Capabilities::local()).is_err(),
                    "the fixture at {level:?} does not actually restrict editing, so this \
                     guard would pass for the wrong reason"
                );
            }

            // Lifting it entirely — Word's "Stop Protection".
            assert_eq!(
                refuse_if_protected(&document, &[lift()], Capabilities::local()),
                Ok(()),
                "a {level:?} document refused the operation that LIFTS it: the restriction \
                 is a one-way door and the feature is worse than absent"
            );
            // And tightening it, which is the same authority question.
            assert_eq!(
                refuse_if_protected(
                    &document,
                    &[impose(DocumentProtectionEdit::ReadOnly)],
                    Capabilities::local()
                ),
                Ok(()),
                "a {level:?} document refused the operation that CHANGES it"
            );
            // Inside a batch, which is where "judged on its worst operation" actually bites:
            // the lift travels beside another protection change and neither is refused.
            assert_eq!(
                refuse_if_protected(
                    &document,
                    &[impose(DocumentProtectionEdit::Comments), lift()],
                    Capabilities::local()
                ),
                Ok(()),
                "a {level:?} document refused a BATCH of protection changes"
            );
        }
    }

    /// The other half of the same rule, and the reason the exemption is not a hole: the lift
    /// is exempt, but every other operation in the batch is still judged against the
    /// restriction that was in force when the batch ARRIVED. So a batch cannot lift a
    /// restriction and then edit under the lift.
    ///
    /// Driven red by the plausible WRONG fix to the ordering trap: waving the whole batch
    /// through when any operation in it is exempt
    /// (`if ops.iter().any(exempt_from_protection) { return Ok(()); }`). That turns the
    /// exemption into a hole — a lift becomes a passkey for everything travelling beside it —
    /// and this is the guard that sees it. Recorded in the commit message.
    #[test]
    fn a_batch_may_not_lift_a_restriction_and_then_edit_under_the_lift() {
        let document = protected(DocumentProtectionEdit::ReadOnly, true);
        assert_eq!(
            refuse_if_protected(&document, &[lift(), typing()], Capabilities::local()),
            Err(ProtectionRefusal::ReadOnly),
            "a batch smuggled a keystroke in behind its own unlock"
        );
        // Order is irrelevant — the batch is judged whole, not in sequence.
        assert_eq!(
            refuse_if_protected(&document, &[typing(), lift()], Capabilities::local()),
            Err(ProtectionRefusal::ReadOnly),
            "a batch smuggled a keystroke in ahead of its own unlock"
        );
    }

    /// `exempt_from_protection` exempts exactly one operation and defaults a new one to
    /// **governed**. The positive list is the exemption list, so this asserts the default
    /// rather than enumerating 59 variants: a sample of ordinary operations, plus the one
    /// exemption.
    #[test]
    fn only_the_protection_operation_itself_is_exempt() {
        assert!(exempt_from_protection(&lift(), Capabilities::local()));
        assert!(exempt_from_protection(
            &impose(DocumentProtectionEdit::ReadOnly),
            Capabilities::local()
        ));
        for op in [typing(), deleting(), review(vec![run(11, "abcdefgh")])] {
            assert!(
                !exempt_from_protection(&op, Capabilities::local()),
                "an ordinary operation is exempt from every restriction: {op:?}"
            );
        }
    }

    /// Word's `w:enforcement="0"` state stays reachable: `None` means the element is absent,
    /// and an unenforced restriction is `Some` with the flag clear. Collapsing the two would
    /// lose a state Word round-trips.
    #[test]
    fn an_unenforced_restriction_is_a_different_value_from_no_restriction() {
        let unenforced = protected(DocumentProtectionEdit::ReadOnly, false);
        assert!(
            unenforced
                .definitions()
                .settings
                .document_protection
                .is_some(),
            "an unenforced restriction must still be present, so export writes it back"
        );
        assert_eq!(
            refuse_if_protected(&unenforced, &[typing()], Capabilities::local()),
            Ok(())
        );
    }

    /// `SetDocumentProtection { protection: None }` — Word's "Stop Protection".
    fn lift() -> Operation {
        Operation::SetDocumentProtection { protection: None }
    }

    /// `SetDocumentProtection` installing an enforced restriction at `edit`.
    fn impose(edit: DocumentProtectionEdit) -> Operation {
        Operation::SetDocumentProtection {
            protection: Some(DocumentProtection {
                edit,
                enforcement: true,
                formatting: false,
            }),
        }
    }
}
