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
fn is_comment_only(document: &Document, op: &Operation) -> bool {
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
fn is_tracked_only(document: &Document, op: &Operation) -> bool {
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
                refuse_if_protected(&document, std::slice::from_ref(&op)),
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
        assert_eq!(refuse_if_protected(&document, &[typing()]), Ok(()));
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
            refuse_if_protected(&document, &[anchored]),
            Ok(()),
            "a comment anchor must be accepted, or the level allows nothing it exists for"
        );

        // A suggested keystroke: the same operation, one character more.
        let suggested = review(vec![
            revision(30, RevisionKind::Insertion, vec![run(31, "x")]),
            run(11, "abcdefgh"),
        ]);
        assert_eq!(
            refuse_if_protected(&document, &[suggested]),
            Err(ProtectionRefusal::CommentsOnly),
            "a tracked insertion is not a comment"
        );
        assert_eq!(
            refuse_if_protected(&document, &[typing()]),
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
            refuse_if_protected(&document, &[elsewhere]),
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
            refuse_if_protected(&plain, &[insert]),
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
        assert_eq!(refuse_if_protected(&plain, &[delete]), Ok(()));

        // Untracked typing through the same operation: a character appears outside any
        // revision, so the projection gains it.
        let untracked = review(vec![run(50, "x"), run(11, "abcdefgh")]);
        assert_eq!(
            refuse_if_protected(&plain, &[untracked]),
            Err(ProtectionRefusal::TrackedChangesOnly),
            "an untracked edit is what this level exists to prevent"
        );
        assert_eq!(
            refuse_if_protected(&plain, &[typing()]),
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
            refuse_if_protected(&suggested, &[accepted]),
            Err(ProtectionRefusal::TrackedChangesOnly),
            "accepting a tracked change is a review decision, not a tracked edit"
        );

        // Rejecting it removes wrapper and text together, so the projection is unchanged and
        // only the vanished revision id distinguishes it from nothing having happened.
        let rejected = review(vec![run(11, "abcdefgh")]);
        assert_eq!(
            refuse_if_protected(&suggested, &[rejected]),
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
        assert_eq!(refuse_if_protected(&document, &[anchored]), Ok(()));
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
            refuse_if_protected(&document, &ops),
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
        assert_eq!(refuse_if_protected(&document, &[typing()]), Ok(()));
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
}
